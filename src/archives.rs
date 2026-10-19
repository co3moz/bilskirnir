//! Image archives: a saved image leaves as a gzip-compressed `docker save` stream and comes back
//! through `docker load`, without passing through memory or the data volume.
//!
//! `docker load` applies whatever names an archive carries, so an import rewrites the archive on
//! its way in (see `tar`): the image arrives under its ID only and is tagged with a name the
//! manager owns. An export saves by image ID, so its archive names no image either.
mod gzip;
pub(crate) mod tar;

use crate::{
    App, auth,
    catalog::DesktopInfo,
    compose::Compose,
    error::{Error, Result},
    icons::Icon,
    jobs,
    model::{BuildConfig, ImageRecord, build_scripts, image_ref, now, template_id},
    process::{self, Overflow},
};
use anyhow::{Context, anyhow, bail};
use axum::{
    Json,
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin},
    sync::{OwnedSemaphorePermit, mpsc, oneshot},
};

/// How often a running transfer's byte count is written to its job.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// Unpacked archive bytes waiting for Docker at most.
const UNPACK_BUDGET: usize = 4 << 20;
const READ_SIZE: usize = 256 * 1024;
/// `docker load` registers the layers after the archive has arrived; a large image takes minutes.
const LOAD_LIMIT: Duration = Duration::from_secs(3600);
/// The metadata layout this release writes and reads.
const FORMAT: u32 = 1;

/// Bilskirnir's description of the image, the archive's first entry, so an import keeps the
/// saved image's name and description, and a script build its configuration.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ArchiveInfo {
    pub format: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub desktop: Option<DesktopInfo>,
    #[serde(default)]
    pub image_id: String,
    #[serde(default)]
    pub parent_image_id: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,
    /// Archives exported before templates were called composes name it `template`.
    #[serde(default, alias = "template", skip_serializing_if = "Option::is_none")]
    pub compose: Option<Compose>,
    #[serde(
        default,
        deserialize_with = "crate::icons::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub icon: Option<Icon>,
    /// Archives exported before templates could be merged name one as `template_id`.
    #[serde(
        default,
        alias = "template_id",
        deserialize_with = "crate::model::one_or_more",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub template_ids: Vec<String>,
}

impl ArchiveInfo {
    fn of(record: &ImageRecord) -> Self {
        Self {
            format: FORMAT,
            name: record.name.clone(),
            description: record.description.clone(),
            kind: record.kind.clone(),
            desktop: record.desktop.clone(),
            image_id: record.image_id.clone(),
            parent_image_id: record.parent_image_id.clone(),
            created_at: record.created_at,
            build: record.build.clone(),
            compose: record.compose.clone(),
            icon: record.icon.clone(),
            template_ids: record.template_ids.clone(),
        }
    }

    /// Metadata this release understands; anything else is ignored and the image still loads.
    fn read(raw: Option<&[u8]>) -> Option<Self> {
        let mut info = raw
            .and_then(|raw| serde_json::from_slice::<Self>(raw).ok())
            .filter(|info| info.format == FORMAT)?;
        // A configuration the build route would refuse is not one worth keeping.
        info.build = info.build.filter(|build| {
            image_ref(&build.base_image).is_ok()
                && build_scripts(&build.script, &build.parts).is_ok()
                && crate::desktop_mcp::check_declarations(&build.mcp).is_ok()
        });
        info.compose = info.compose.filter(|compose| compose.validate().is_ok());
        info.template_ids.retain(|id| template_id(id).is_ok());
        Some(info)
    }
}

/// Stream a saved image as a gzip-compressed `docker save` archive. The transfer is a job, so
/// its progress and any Docker error stay visible after the browser's download has begun.
pub async fn export(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    token: auth::Session,
) -> Result<Response> {
    let record: ImageRecord = app.db.get("image", &id)?;
    let permit = transfer_permit(&app)?;
    let (ascii, full) = file_names(&record.name);
    let (body, chunks) = mpsc::channel::<std::io::Result<Bytes>>(4);
    let (job_id, _) = jobs::start(
        app,
        "image-export",
        format!("image:{id}"),
        false,
        move |app, job_id| async move {
            let _permit = permit;
            let result = app.export_archive(&job_id, &record, &body, token).await;
            if let Err(error) = &result {
                // The download fails instead of ending in a short file that looks complete.
                let _ = body
                    .send(Err(std::io::Error::other(format!("{error:#}"))))
                    .await;
            }
            result
        },
    )?;
    let stream = futures_util::stream::unfold(chunks, |mut chunks| async move {
        chunks.recv().await.map(|chunk| (chunk, chunks))
    });
    Ok((
        [
            (header::CONTENT_TYPE, "application/gzip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{ascii}\"; filename*=UTF-8''{}",
                    crate::files::percent_encode(&full)
                ),
            ),
            (header::HeaderName::from_static("x-job-id"), job_id),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct Import {
    /// The archive's exact size, or none for one streamed as it is made.
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Receive an image archive and load it into Docker. The answer comes once the whole archive has
/// arrived and passed its checks: a job ID while Docker goes on loading the image, or the reason
/// the archive was refused.
pub async fn import(
    State(app): State<Arc<App>>,
    Query(request): Query<Import>,
    headers: HeaderMap,
    token: auth::Session,
    body: Body,
) -> Result<Json<Value>> {
    check_upload(&app, request.size, &headers)?;
    let name = request.name.as_deref().map(str::trim).unwrap_or_default();
    if name.chars().count() > 100
        || request
            .description
            .as_ref()
            .is_some_and(|description| description.len() > 4096)
    {
        return Err(anyhow!("Invalid image name or description").into());
    }
    let permit = transfer_permit(&app)?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let (accepted, was_accepted) = oneshot::channel();
    let waiting = app.clone();
    let session = token.clone();
    let (job_id, outcome) = jobs::start(
        app,
        "image-import",
        format!("image:{id}"),
        true,
        move |app, job_id| async move {
            let _permit = permit;
            app.import_archive(&job_id, &id, request, body, token, accepted)
                .await
        },
    )?;
    tokio::select! {
        result = was_accepted => match result {
            Ok(()) => Ok(Json(json!({"job_id": job_id}))),
            // The job ended before it accepted the archive; its error says why.
            Err(_) => match outcome.await {
                Ok(Err(message)) => Err(Error(StatusCode::BAD_REQUEST, message)),
                _ => Err(Error(StatusCode::INTERNAL_SERVER_ERROR, "The import ended unexpectedly".into())),
            },
        },
        _ = waiting.session_revoked(session) => {
            Err(Error(StatusCode::UNAUTHORIZED, "Session expired. Sign in again.".into()))
        }
    }
}

/// `docker load` reading the filtered archive. Until the archive has passed its checks the
/// process is killed before its input closes: a closed input would let Docker load what it got.
struct Load {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

/// `docker load` stopped reading; its own error output says why.
#[derive(Debug)]
struct Stopped;
impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("docker load stopped reading the archive")
    }
}
impl std::error::Error for Stopped {}

impl Load {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let stdin = self.stdin.as_mut().context("docker load input is closed")?;
        stdin
            .write_all(data)
            .await
            .map_err(|_| anyhow::Error::new(Stopped))
    }

    /// End the input after the closing blocks, which commits the load.
    async fn close(&mut self) -> anyhow::Result<Child> {
        let mut stdin = self.stdin.take().context("docker load input is closed")?;
        stdin
            .shutdown()
            .await
            .map_err(|_| anyhow::Error::new(Stopped))?;
        drop(stdin);
        self.child.take().context("docker load has ended")
    }
}

impl Drop for Load {
    fn drop(&mut self) {
        if self.stdin.is_some()
            && let Some(child) = &mut self.child
        {
            let _ = child.start_kill();
        }
    }
}

impl App {
    async fn export_archive(
        &self,
        job_id: &str,
        record: &ImageRecord,
        body: &mpsc::Sender<std::io::Result<Bytes>>,
        token: auth::Session,
    ) -> anyhow::Result<Value> {
        let info = self
            .inspect("image", &record.image_id)
            .await
            .context("The image is no longer available in Docker")?;
        if info["Config"]["Labels"]["bilskirnir.desktop"] != "true" {
            bail!("A desktop-compatible image is required");
        }
        self.append_log(
            job_id,
            &format!(
                "Exporting {} with docker save. Docker prepares the image before its layers arrive.",
                record.name
            ),
        )?;
        let metadata = serde_json::to_vec_pretty(&ArchiveInfo::of(record))?;
        let (mut compressor, first) =
            gzip::Compressor::new(&tar::entry(tar::METADATA, &metadata, now()))?;
        let mut sent = 0;
        send(body, first, &mut sent).await?;
        // Nothing counts until Docker has prepared the image and its first layer arrives.
        self.set_progress(job_id, Some(json!({"transfer": "export", "current": 0})))?;
        let mut child = self
            .command(&["save".into(), record.image_id.clone()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Docker CLI")?;
        let mut stdout = child.stdout.take().context("Missing docker save output")?;
        let errors = tokio::spawn(process::read_limited(
            child.stderr.take().context("Missing docker save errors")?,
            8192,
            Overflow::Tail,
        ));
        let revoked = self.session_revoked(token);
        tokio::pin!(revoked);
        let mut buffer = vec![0; READ_SIZE];
        let mut reported = Instant::now();
        loop {
            let read = tokio::select! {
                read = stdout.read(&mut buffer) => read.context("Could not read docker save output")?,
                _ = &mut revoked => bail!("Session expired. Sign in again."),
                _ = body.closed() => bail!("The download was cancelled before it finished"),
            };
            if read == 0 {
                break;
            }
            let input = buffer[..read].to_vec();
            let (back, output) = tokio::task::spawn_blocking(move || {
                let output = compressor.push(&input);
                (compressor, output)
            })
            .await?;
            compressor = back;
            send(body, output?, &mut sent).await?;
            if reported.elapsed() >= PROGRESS_INTERVAL {
                self.set_progress(job_id, Some(json!({"transfer": "export", "current": sent})))?;
                reported = Instant::now();
            }
        }
        let status = child.wait().await?;
        let errors = errors.await??;
        if !status.success() {
            bail!("Docker: {}", String::from_utf8_lossy(&errors).trim());
        }
        let last = tokio::task::spawn_blocking(move || compressor.finish()).await??;
        send(body, last, &mut sent).await?;
        self.append_log(job_id, &format!("Exported {sent} bytes."))?;
        Ok(json!({"image_id": record.image_id, "name": record.name, "bytes": sent}))
    }

    async fn import_archive(
        &self,
        job_id: &str,
        id: &str,
        request: Import,
        body: Body,
        token: auth::Session,
        accepted: oneshot::Sender<()>,
    ) -> anyhow::Result<Value> {
        self.append_log(
            job_id,
            &format!("Receiving an image archive {}.", arriving(request.size)),
        )?;
        let progress = |current: u64| import_progress(current, request.size);
        self.set_progress(job_id, Some(progress(0)))?;
        let mut child = self
            .command(&["load".into(), "--quiet".into()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Docker CLI")?;
        let output = tokio::spawn(process::read_limited(
            child.stdout.take().context("Missing docker load output")?,
            65_536,
            Overflow::Tail,
        ));
        let errors = tokio::spawn(process::read_limited(
            child.stderr.take().context("Missing docker load errors")?,
            8192,
            Overflow::Tail,
        ));
        let mut load = Load {
            stdin: child.stdin.take(),
            child: Some(child),
        };
        let delivered = async {
            let mut unpacker = gzip::Unpacker::new(self.config.archive_limit);
            let revoked = self.session_revoked(token);
            tokio::pin!(revoked);
            let mut stream = body.into_data_stream();
            let (mut received, mut reported) = (0u64, Instant::now());
            loop {
                let chunk = tokio::select! {
                    chunk = stream.next() => chunk,
                    _ = &mut revoked => bail!("Session expired. Sign in again."),
                };
                let Some(chunk) = chunk else { break };
                let mut pending = chunk.map_err(interrupted)?;
                received += pending.len() as u64;
                count(received, request.size, self.config.archive_limit)?;
                while !pending.is_empty() {
                    let (back, step) = tokio::task::spawn_blocking(move || {
                        let step = unpacker
                            .push(&pending, UNPACK_BUDGET)
                            .map(|(used, output)| (pending.slice(used..), output));
                        (unpacker, step)
                    })
                    .await?;
                    unpacker = back;
                    let (rest, output) = step?;
                    pending = rest;
                    load.write(&output).await?;
                }
                if reported.elapsed() >= PROGRESS_INTERVAL {
                    self.set_progress(job_id, Some(progress(received)))?;
                    reported = Instant::now();
                }
            }
            if let Some(size) = request.size
                && received != size
            {
                bail!("The upload ended after {received} of {size} bytes");
            }
            // Without a size, an archive cut at an entry boundary is refused once it lacks what
            // loading needs: the filter wants the manifest and configuration, Docker every layer.
            let (last, contents) = tokio::task::spawn_blocking(move || unpacker.finish()).await??;
            load.write(&last).await?;
            anyhow::Ok(contents)
        }
        .await;
        let contents = match delivered {
            Ok(contents) => contents,
            Err(error) if error.is::<Stopped>() => {
                drop(load);
                return Err(docker_error(errors).await);
            }
            Err(error) => return Err(error),
        };
        let mut child = match load.close().await {
            Ok(child) => child,
            Err(_) => return Err(docker_error(errors).await),
        };
        // Docker has the whole archive; the upload request can end here.
        let _ = accepted.send(());
        self.set_progress(job_id, None)?;
        self.append_log(
            job_id,
            "The archive passed its checks. Docker is loading the image.",
        )?;
        let status = tokio::time::timeout(LOAD_LIMIT, child.wait())
            .await
            .context("docker load exceeded the one-hour limit")??;
        let output = output.await??;
        let errors = errors.await??;
        if !status.success() {
            bail!("Docker: {}", String::from_utf8_lossy(&errors).trim());
        }
        let loaded = loaded_image(&String::from_utf8_lossy(&output))?;
        let info = self.inspect("image", &loaded).await?;
        let image_id = info["Id"].as_str().context("Missing image ID")?.to_owned();
        if info["Config"]["Labels"]["bilskirnir.desktop"] != "true" {
            // Only a configuration written to read differently in Docker gets this far. An
            // untagged image is this import's own; a tagged one was already on the host.
            if info["RepoTags"].as_array().is_none_or(Vec::is_empty) {
                let _ = self.docker(&["image", "rm", &image_id]).await;
            }
            bail!("The archive does not contain a Bilskirnir desktop image");
        }
        let reference = format!("bilskirnir/imported:{id}");
        self.docker(&["image", "tag", &image_id, &reference])
            .await?;
        let metadata = ArchiveInfo::read(contents.metadata.as_deref());
        let record = ImageRecord {
            id: id.into(),
            name: record_name(request.name.as_deref(), metadata.as_ref(), &image_id),
            image_id,
            reference,
            parent_image_id: metadata
                .as_ref()
                .and_then(|info| info.parent_image_id.clone())
                .filter(|parent| image_digest(parent)),
            source_container: None,
            kind: "archive".into(),
            created_at: now(),
            description: request
                .description
                .or_else(|| metadata.as_ref().map(|info| info.description.clone()))
                .map(|description| clip(&description, 4096))
                .unwrap_or_default(),
            desktop: crate::catalog::from_image(&info),
            build: metadata.as_ref().and_then(|info| info.build.clone()),
            compose: metadata.as_ref().and_then(|info| info.compose.clone()),
            icon: metadata.as_ref().and_then(|info| info.icon.clone()),
            template_ids: metadata.map(|info| info.template_ids).unwrap_or_default(),
        };
        self.db.put("image", id, &record)?;
        if record.build.is_some() {
            self.append_log(job_id, "The archive carried its build configuration.")?;
        }
        if record.compose.is_some() {
            self.append_log(job_id, "The archive carried its compose.")?;
        }
        self.append_log(
            job_id,
            &format!("Registered {} as {}.", record.name, record.reference),
        )?;
        // Like a build's result, without the script that only the configuration route returns.
        Ok(crate::api::image_summary(&record))
    }
}

/// What `docker load` printed before it stopped reading.
async fn docker_error(errors: tokio::task::JoinHandle<anyhow::Result<Vec<u8>>>) -> anyhow::Error {
    let errors = match errors.await {
        Ok(Ok(errors)) => errors,
        _ => Vec::new(),
    };
    anyhow!("Docker: {}", String::from_utf8_lossy(&errors).trim())
}

const TOO_LARGE: &str = "The archive is larger than the import limit (IMAGE_ARCHIVE_MAX_MB)";

/// Refuse an archive upload before anything starts: a declared size above the limit, or a body
/// that is not plain bytes. An upload without a size is held to the limit as it arrives.
pub(crate) fn check_upload(app: &App, size: Option<u64>, headers: &HeaderMap) -> Result<()> {
    if size.is_some_and(|size| size > app.config.archive_limit) {
        return Err(Error(StatusCode::PAYLOAD_TOO_LARGE, TOO_LARGE.into()));
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        != Some("application/octet-stream")
    {
        return Err(Error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Send the archive as a binary body".into(),
        ));
    }
    Ok(())
}

/// Count `received` bytes against the declared size, or without one against the import limit.
pub(crate) fn count(received: u64, size: Option<u64>, limit: u64) -> anyhow::Result<()> {
    match size {
        Some(size) if received > size => bail!("The upload is larger than its declared size"),
        None if received > limit => bail!(TOO_LARGE),
        _ => Ok(()),
    }
}

/// An upload body that failed: past the route's body limit, or cut off, as a chunked upload is
/// when its connection closes before the last chunk.
pub(crate) fn interrupted(error: axum::Error) -> anyhow::Error {
    let error = error.into_inner();
    if error.is::<http_body_util::LengthLimitError>() {
        return anyhow!(TOO_LARGE);
    }
    anyhow!("The upload was interrupted: {error}")
}

/// An import's progress: the bytes received, and of how many when the client said.
pub(crate) fn import_progress(current: u64, size: Option<u64>) -> Value {
    match size {
        Some(total) => json!({"transfer": "import", "current": current, "total": total}),
        None => json!({"transfer": "import", "current": current}),
    }
}

/// How an archive arrives, for the job log.
pub(crate) fn arriving(size: Option<u64>) -> String {
    match size {
        Some(size) => format!("of {size} bytes"),
        None => "of unknown size as it streams in".into(),
    }
}

/// Image, volume and folder archives share two transfer slots: each holds a Docker save, load
/// or tar.
pub(crate) fn transfer_permit(app: &App) -> Result<OwnedSemaphorePermit> {
    app.archives.clone().try_acquire_owned().map_err(|_| {
        Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Two archive transfers are already running. Try again when one finishes.".into(),
        )
    })
}

/// Pass on the archive tar writes to `child`'s stdout as a download, with the bytes sent as the
/// job's progress, and return their count once tar has ended well. tar ends with 1 when a file
/// changed while it was read, as one in use can; the archive is complete and holds the file as it
/// was read, so that is only logged. A download that stops early or a session that ends kills
/// `child`; the caller removes any container it started.
pub(crate) async fn send_archive(
    app: &App,
    job_id: &str,
    child: &mut Child,
    body: &mpsc::Sender<std::io::Result<Bytes>>,
    token: auth::Session,
) -> anyhow::Result<u64> {
    app.set_progress(job_id, Some(json!({"transfer": "export", "current": 0})))?;
    let mut stdout = child.stdout.take().context("Missing tar output")?;
    let errors = tokio::spawn(process::read_limited(
        child.stderr.take().context("Missing tar errors")?,
        8192,
        Overflow::Tail,
    ));
    let revoked = app.session_revoked(token);
    tokio::pin!(revoked);
    let mut buffer = vec![0; READ_SIZE];
    let (mut sent, mut reported) = (0u64, Instant::now());
    let streamed = async {
        loop {
            let read = tokio::select! {
                read = stdout.read(&mut buffer) => read.context("Could not read the archive")?,
                _ = &mut revoked => bail!("Session expired. Sign in again."),
                _ = body.closed() => bail!("The download was cancelled before it finished"),
            };
            if read == 0 {
                return anyhow::Ok(());
            }
            sent += read as u64;
            body.send(Ok(Bytes::copy_from_slice(&buffer[..read])))
                .await
                .map_err(|_| anyhow!("The download was cancelled before it finished"))?;
            if reported.elapsed() >= PROGRESS_INTERVAL {
                app.set_progress(job_id, Some(json!({"transfer": "export", "current": sent})))?;
                reported = Instant::now();
            }
        }
    }
    .await;
    if let Err(error) = streamed {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Err(error);
    }
    let status = child.wait().await?;
    let errors = String::from_utf8_lossy(&errors.await??).trim().to_owned();
    match status.code() {
        Some(1) if !errors.is_empty() => app.append_log(job_id, &errors)?,
        Some(0 | 1) => {}
        _ if errors.is_empty() => bail!("tar ended with {status}"),
        _ => bail!("{errors}"),
    }
    Ok(sent)
}

async fn send(
    body: &mpsc::Sender<std::io::Result<Bytes>>,
    chunk: Vec<u8>,
    sent: &mut u64,
) -> anyhow::Result<()> {
    if chunk.is_empty() {
        return Ok(());
    }
    *sent += chunk.len() as u64;
    body.send(Ok(Bytes::from(chunk)))
        .await
        .map_err(|_| anyhow!("The download was cancelled before it finished"))
}

/// The download's ASCII file name, and the image's own name for clients that read `filename*`.
fn file_names(name: &str) -> (String, String) {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.chars().take(80).collect();
    let slug = match slug.trim_end_matches('-') {
        "" => "image",
        slug => slug,
    };
    let full: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "/\\\"*:<>?|".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let full = if full.is_empty() { slug } else { full.as_str() };
    (format!("{slug}.tar.gz"), format!("{full}.tar.gz"))
}

/// The image `docker load --quiet` reports, by ID or, should it apply a name after all, by name.
fn loaded_image(output: &str) -> anyhow::Result<String> {
    let mut images: Vec<&str> = output
        .lines()
        .filter_map(|line| {
            line.strip_prefix("Loaded image ID: ")
                .or_else(|| line.strip_prefix("Loaded image: "))
        })
        .map(str::trim)
        .collect();
    images.sort_unstable();
    images.dedup();
    match images.as_slice() {
        [image] => Ok((*image).to_owned()),
        [] => bail!("Docker did not report the loaded image"),
        _ => bail!("Docker loaded more than one image: {}", images.join(", ")),
    }
}

fn record_name(requested: Option<&str>, metadata: Option<&ArchiveInfo>, image_id: &str) -> String {
    let requested = requested.map(str::trim).filter(|name| !name.is_empty());
    let stored = metadata
        .map(|info| info.name.trim())
        .filter(|name| !name.is_empty());
    match requested.or(stored) {
        Some(name) => name.chars().take(100).collect(),
        None => {
            let short = image_id.trim_start_matches("sha256:");
            format!("Imported image {}", &short[..short.len().min(12)])
        }
    }
}

fn clip(text: &str, bytes: usize) -> String {
    let mut end = text.len().min(bytes);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

fn image_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
#[path = "archives_tests.rs"]
mod tests;
