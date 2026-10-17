//! Volume archives: a volume's files leave as a gzip-compressed tar and come back into a new
//! volume, streamed through a helper container without passing through memory or the data
//! volume. The archive is an ordinary tar, so other tools read and write it too. A copy pipes
//! the same two helpers into each other.
mod copy;
mod ending;

use super::{Access, HELPER_LIMIT, MOUNT, Volume, helper_args, helper_error, helper_name};
use crate::{
    App,
    archives::{arriving, check_upload, count, import_progress, interrupted, transfer_permit},
    auth,
    error::{Error, Result},
    jobs,
    model::volume_name,
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
pub(crate) use copy::copy;
use ending::Ending;
use flate2::write::MultiGzDecoder;
use futures_util::{Stream, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::{Child, ChildStdin},
    sync::{mpsc, oneshot},
};

/// How often a running transfer's byte count is written to its job.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const READ_SIZE: usize = 256 * 1024;
/// Compressed input unpacked per step. Deflate expands at most about a thousand times, so one
/// step holds at most a few dozen megabytes before it is handed on.
const GZIP_STEP: usize = 16 * 1024;
/// Unpacking the rest after the upload has arrived; a large archive takes minutes.
const EXTRACT_LIMIT: Duration = Duration::from_secs(3600);
/// Creates the folder an archive is unpacked into, or checks that the volume is empty, then
/// runs tar there as the desktop user.
const EXTRACT: &str = include_str!("../volume_extract.py");
/// How tar unpacks an upload: as the desktop user, it cannot change owners, write devices or
/// leave its folder, and the archive's modes lose what the usual umask takes away.
const UPLOADED: &[&str] = &["--no-same-permissions"];

/// Stream a volume's files as a `.tar.gz`. The transfer is a job, so its progress and any error
/// stay visible after the browser's download has begun.
pub async fn export(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Response> {
    let volume = app.open_volume(&name).await?;
    let permit = transfer_permit(&app)?;
    let token = auth::cookie(&headers);
    let (body, chunks) = mpsc::channel::<std::io::Result<Bytes>>(4);
    let (job_id, _) = jobs::start(
        app,
        "volume-export",
        format!("volume:{name}"),
        false,
        move |app, job_id| async move {
            let _permit = permit;
            let result = app.export_volume(&job_id, &volume, &body, token).await;
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
            // Volume names are letters, digits, dots, dashes and underscores.
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}.tar.gz\""),
            ),
            (header::HeaderName::from_static("x-job-id"), job_id),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct Import {
    /// The new volume's name.
    pub name: String,
    /// The archive's exact size, or none for one streamed as it is made.
    #[serde(default)]
    pub size: Option<u64>,
}

/// Receive a `.tar.gz` or `.tar` and unpack it into a new volume. The answer comes once the
/// whole archive has arrived: a job ID while the last files are written, or why it was refused.
pub async fn import(
    State(app): State<Arc<App>>,
    Query(request): Query<Import>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<Value>> {
    check_upload(&app, request.size, &headers)?;
    volume_name(&request.name)?;
    // Refused before anything is uploaded; the job checks again under the volume's lock.
    if app
        .optional_inspect("volume", &request.name)
        .await?
        .is_some()
    {
        return Err(Error(
            StatusCode::CONFLICT,
            "A volume with this name already exists; import into a new name".into(),
        ));
    }
    let permit = transfer_permit(&app)?;
    let session = auth::cookie(&headers);
    let token = session.clone();
    let (accepted, was_accepted) = oneshot::channel();
    let (job_id, outcome) = jobs::start(
        app.clone(),
        "volume-import",
        format!("volume:{}", request.name),
        true,
        move |app, job_id| async move {
            let _permit = permit;
            app.import_volume(&job_id, request, body, token, accepted)
                .await
        },
    )?;
    answer(&app, job_id, was_accepted, outcome, session).await
}

#[derive(Deserialize)]
pub struct Extract {
    /// The new folder, or `/` for a volume that is still empty.
    pub path: String,
    /// The archive's exact size, or none for one streamed as it is made.
    #[serde(default)]
    pub size: Option<u64>,
}

/// Receive a `.tar.gz` or `.tar` and unpack it into a new folder of an existing volume, such as
/// one a running desktop mounts. It answers as an import does.
pub async fn extract(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Query(request): Query<Extract>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<Value>> {
    check_upload(&app, request.size, &headers)?;
    let session = auth::cookie(&headers);
    extract_archive(app, name, request, body, session).await
}

/// An extract once its upload has passed `check_upload`. `session` is the sign-in whose end
/// cancels the transfer, as for every other archive transfer.
pub(crate) async fn extract_archive(
    app: Arc<App>,
    name: String,
    request: Extract,
    body: Body,
    session: Option<String>,
) -> Result<Json<Value>> {
    crate::apps::directory(&request.path)
        .map_err(|_| anyhow!("Use a canonical absolute folder path without dot segments"))?;
    // Refused before anything is uploaded; the job checks again under the volume's lock.
    app.open_volume(&name).await?;
    let permit = transfer_permit(&app)?;
    let token = session.clone();
    let (accepted, was_accepted) = oneshot::channel();
    let (job_id, outcome) = jobs::start(
        app.clone(),
        "volume-extract",
        format!("volume:{name}"),
        true,
        move |app, job_id| async move {
            let _permit = permit;
            app.extract_volume(&job_id, &name, request, body, token, accepted)
                .await
        },
    )?;
    answer(&app, job_id, was_accepted, outcome, session).await
}

/// Tells an upload request that its job took the whole archive, or why it was refused before.
type Accepted = oneshot::Sender<Result<()>>;

/// Answer an upload request once its job has taken the whole archive: a job ID while the last
/// files are written, or why the archive was refused.
async fn answer(
    app: &App,
    job_id: String,
    accepted: oneshot::Receiver<Result<()>>,
    outcome: jobs::Outcome,
    session: Option<String>,
) -> Result<Json<Value>> {
    tokio::select! {
        result = accepted => match result {
            Ok(Ok(())) => Ok(Json(json!({"job_id": job_id}))),
            Ok(Err(refused)) => Err(refused),
            // The job ended before it took the whole archive; its error says why.
            Err(_) => match outcome.await {
                Ok(Err(message)) => Err(Error(StatusCode::BAD_REQUEST, message)),
                _ => Err(Error(StatusCode::INTERNAL_SERVER_ERROR, "The import ended unexpectedly".into())),
            },
        },
        _ = app.session_revoked(session) => {
            Err(Error(StatusCode::UNAUTHORIZED, "Session expired. Sign in again.".into()))
        }
    }
}

/// Pass a refusal on to the waiting request with its status, and keep its message for the job.
fn refuse(accepted: Accepted, refused: Error) -> anyhow::Error {
    let message = anyhow!(refused.1.clone());
    let _ = accepted.send(Err(refused));
    message
}

/// The tar a helper writes or reads, ended when it is dropped before it finished.
struct Helper {
    child: Child,
    name: String,
}

impl Helper {
    fn spawn(app: &App, volume: &Volume, access: Access, command: &[&str]) -> anyhow::Result<Self> {
        let name = helper_name();
        let command = command
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        let input = access == Access::Write;
        let args = helper_args(volume, access, &name, input, &command);
        let child = app
            .command(&args)
            .kill_on_drop(true)
            .stdin(if input { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Docker CLI")?;
        Ok(Self { child, name })
    }

    /// Stop a helper that did not finish: the CLI's container goes on without it.
    async fn abandon(mut self, app: &App) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        let _ = app.docker(&["rm", "--force", &self.name]).await;
    }
}

/// tar unpacking into the folder its helper created, reading what `stdin` is given.
struct Unpacking {
    helper: Helper,
    stdin: ChildStdin,
    errors: tokio::task::JoinHandle<anyhow::Result<Vec<u8>>>,
}

impl App {
    async fn export_volume(
        &self,
        job_id: &str,
        volume: &Volume,
        body: &mpsc::Sender<std::io::Result<Bytes>>,
        token: Option<String>,
    ) -> anyhow::Result<Value> {
        self.append_log(job_id, &format!("Exporting the files of {}.", volume.name))?;
        self.set_progress(job_id, Some(json!({"transfer": "export", "current": 0})))?;
        let mut helper = Helper::spawn(
            self,
            volume,
            Access::Archive,
            &[
                "tar",
                "--create",
                "--gzip",
                "--numeric-owner",
                "--file",
                "-",
                "--directory",
                MOUNT,
                ".",
            ],
        )?;
        let mut stdout = helper.child.stdout.take().context("Missing tar output")?;
        let errors = tokio::spawn(process::read_limited(
            helper.child.stderr.take().context("Missing tar errors")?,
            8192,
            Overflow::Tail,
        ));
        let revoked = self.session_revoked(token);
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
                    self.set_progress(
                        job_id,
                        Some(json!({"transfer": "export", "current": sent})),
                    )?;
                    reported = Instant::now();
                }
            }
        }
        .await;
        if let Err(error) = streamed {
            helper.abandon(self).await;
            return Err(error);
        }
        let status = helper.child.wait().await?;
        let errors = String::from_utf8_lossy(&errors.await??).trim().to_owned();
        match status.code() {
            Some(0) => {}
            // GNU tar ends with 1 when a file changed while it was read, as one in use can; the
            // archive is complete and holds the file as it was read.
            Some(1) => self.append_log(job_id, &format!("tar: {errors}"))?,
            _ => bail!("tar: {errors}"),
        }
        self.append_log(job_id, &format!("Exported {sent} bytes."))?;
        Ok(json!({"volume": volume.name, "bytes": sent}))
    }

    async fn import_volume(
        &self,
        job_id: &str,
        request: Import,
        body: Body,
        token: Option<String>,
        accepted: Accepted,
    ) -> anyhow::Result<Value> {
        let volume = self
            .volume_named(&request.name)
            .await
            .map_err(|error| anyhow!(error.1))?;
        self.new_volume(&volume.name)
            .await
            .map_err(|error| anyhow!(error.1))?;
        self.append_log(
            job_id,
            &format!(
                "Created the volume {}. Receiving an archive {}.",
                volume.name,
                arriving(request.size)
            ),
        )?;
        let received = async {
            let unpacking = self
                .start_unpack(&volume, "/", UPLOADED)
                .await
                .map_err(|error| anyhow!(error.1))?;
            self.unpack(job_id, unpacking, request.size, body, token, accepted)
                .await
        };
        match received.await {
            Ok(received) => {
                self.append_log(
                    job_id,
                    &format!("Unpacked {received} bytes into {}.", volume.name),
                )?;
                Ok(json!({"name": volume.name, "bytes": received}))
            }
            Err(error) => {
                // This import created the volume a moment ago and holds its lock; an incomplete
                // copy is removed rather than left looking like a good one.
                if self.docker(&["volume", "rm", &volume.name]).await.is_ok() {
                    let _ = self.append_log(job_id, "Removed the incomplete volume.");
                }
                Err(error)
            }
        }
    }

    async fn extract_volume(
        &self,
        job_id: &str,
        name: &str,
        request: Extract,
        body: Body,
        token: Option<String>,
        accepted: Accepted,
    ) -> anyhow::Result<Value> {
        let volume = match self.open_volume(name).await {
            Ok(volume) => volume,
            Err(refused) => return Err(refuse(accepted, refused)),
        };
        let unpacking = match self.start_unpack(&volume, &request.path, UPLOADED).await {
            Ok(unpacking) => unpacking,
            Err(refused) => return Err(refuse(accepted, refused)),
        };
        let path = &request.path;
        let arriving = arriving(request.size);
        self.append_log(
            job_id,
            &match path.as_str() {
                "/" => format!("{name} is empty. Receiving an archive {arriving}."),
                _ => format!("Created {path} in {name}. Receiving an archive {arriving}."),
            },
        )?;
        match self
            .unpack(job_id, unpacking, request.size, body, token, accepted)
            .await
        {
            Ok(received) => {
                self.append_log(job_id, &format!("Unpacked {received} bytes into {path}."))?;
                Ok(json!({"volume": name, "path": path, "bytes": received}))
            }
            Err(error) => {
                // The folder is this extract's own, created a moment ago under the volume's
                // lock; an incomplete one is removed rather than left looking like a good one.
                let clear = ["python3", "-c", EXTRACT, "clear", MOUNT, path].map(str::to_owned);
                let cleared = self
                    .run_helper(&volume, Access::Write, &clear, None, 8192)
                    .await;
                let _ = self.append_log(
                    job_id,
                    &match (cleared, path.as_str()) {
                        (Ok(_), "/") => "Emptied the volume again.".to_owned(),
                        (Ok(_), _) => format!("Removed the incomplete {path}."),
                        (Err(cleared), _) => format!("Could not remove {path}: {}", cleared.1),
                    },
                );
                Err(error)
            }
        }
    }

    /// Start tar in a helper that creates `path` in the volume, or takes the volume itself for
    /// `/` while it is empty. A refusal, such as a folder that exists, leaves nothing behind
    /// and keeps the status the request answers with.
    async fn start_unpack(
        &self,
        volume: &Volume,
        path: &str,
        options: &[&str],
    ) -> Result<Unpacking> {
        let mut command = vec!["python3", "-c", EXTRACT, "unpack", MOUNT, path];
        command.extend(options);
        let mut helper = Helper::spawn(self, volume, Access::Write, &command)?;
        let stdin = helper.child.stdin.take().context("Missing tar input")?;
        let mut stdout = helper
            .child
            .stdout
            .take()
            .context("Missing helper output")?;
        let errors = tokio::spawn(process::read_limited(
            helper.child.stderr.take().context("Missing tar errors")?,
            8192,
            Overflow::Tail,
        ));
        let mut ready = [0; 6];
        match tokio::time::timeout(HELPER_LIMIT, stdout.read_exact(&mut ready)).await {
            Ok(Ok(_)) if ready == *b"ready\n" => Ok(Unpacking {
                helper,
                stdin,
                errors,
            }),
            Ok(_) => {
                // The helper ended before tar started; its message says why.
                drop(stdin);
                let _ = tokio::time::timeout(HELPER_LIMIT, helper.child.wait()).await;
                let errors = match errors.await {
                    Ok(Ok(errors)) => errors,
                    _ => Vec::new(),
                };
                helper.abandon(self).await;
                Err(helper_error(&errors))
            }
            Err(_) => {
                helper.abandon(self).await;
                Err(anyhow!("The volume helper did not start in time").into())
            }
        }
    }

    /// Hand an upload to the tar `start_unpack` started and wait for it to finish.
    async fn unpack(
        &self,
        job_id: &str,
        unpacking: Unpacking,
        size: Option<u64>,
        body: Body,
        token: Option<String>,
        accepted: Accepted,
    ) -> anyhow::Result<u64> {
        let Unpacking {
            mut helper,
            mut stdin,
            errors,
        } = unpacking;
        let progress = |current: u64| import_progress(current, size);
        self.set_progress(job_id, Some(progress(0)))?;
        let delivered = deliver(
            body.into_data_stream(),
            &mut stdin,
            size,
            self.config.archive_limit,
            self.session_revoked(token),
            |received| self.set_progress(job_id, Some(progress(received))),
        )
        .await;
        drop(stdin);
        let received = match delivered {
            Ok(received) => received,
            Err(error) if error.is::<Stopped>() => {
                let status = helper.child.wait().await;
                let errors = String::from_utf8_lossy(&errors.await??).trim().to_owned();
                if status.is_err() || errors.is_empty() {
                    helper.abandon(self).await;
                    bail!("tar stopped reading the archive");
                }
                bail!("tar: {errors}");
            }
            Err(error) => {
                helper.abandon(self).await;
                return Err(error);
            }
        };
        // The whole archive is here; the upload request can end.
        let _ = accepted.send(Ok(()));
        self.set_progress(job_id, None)?;
        let status = match tokio::time::timeout(EXTRACT_LIMIT, helper.child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                helper.abandon(self).await;
                bail!("Unpacking the archive exceeded the one-hour limit");
            }
        };
        let errors = String::from_utf8_lossy(&errors.await??).trim().to_owned();
        if !status.success() {
            bail!("tar: {errors}");
        }
        Ok(received)
    }
}

/// Hand an upload to tar as a plain archive, counted against its declared size or, without one,
/// against the import limit, and report the bytes received now and then. Returns how many
/// arrived, or `Stopped` once tar no longer reads.
async fn deliver(
    mut stream: impl Stream<Item = std::result::Result<Bytes, axum::Error>> + Unpin,
    tar: &mut (impl AsyncWrite + Unpin),
    size: Option<u64>,
    limit: u64,
    revoked: impl Future<Output = ()>,
    mut report: impl FnMut(u64) -> anyhow::Result<()>,
) -> anyhow::Result<u64> {
    let mut unpacker = Unpacker::new(limit);
    tokio::pin!(revoked);
    let (mut received, mut reported) = (0u64, Instant::now());
    loop {
        let chunk = tokio::select! {
            chunk = stream.next() => chunk,
            _ = &mut revoked => bail!("Session expired. Sign in again."),
        };
        let Some(chunk) = chunk else { break };
        let chunk = chunk.map_err(interrupted)?;
        received += chunk.len() as u64;
        count(received, size, limit)?;
        for step in chunk.chunks(GZIP_STEP) {
            let output = unpacker.push(step)?;
            if tar.write_all(&output).await.is_err() {
                return Err(anyhow!(Stopped));
            }
        }
        if reported.elapsed() >= PROGRESS_INTERVAL {
            report(received)?;
            reported = Instant::now();
        }
    }
    if let Some(size) = size
        && received != size
    {
        bail!("The upload ended after {received} of {size} bytes");
    }
    let (last, ended) = unpacker.finish()?;
    // GNU tar unpacks an archive that stops at a member boundary as if it were whole. A declared
    // size vouches for the rest; without one, only tar's closing blocks do.
    if size.is_none() && !ended {
        bail!("The archive ended before tar's end-of-archive blocks; the upload is incomplete");
    }
    if tar.write_all(&last).await.is_err() || tar.shutdown().await.is_err() {
        return Err(anyhow!(Stopped));
    }
    Ok(received)
}

/// tar stopped reading its input; its own error output says why.
#[derive(Debug)]
struct Stopped;
impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tar stopped reading the archive")
    }
}
impl std::error::Error for Stopped {}

enum Mode {
    /// Waiting for the two bytes that tell gzip from a plain tar.
    Sniffing(Vec<u8>),
    Plain,
    Gzip(Box<MultiGzDecoder<Vec<u8>>>),
}

/// An upload as the plain tar that tar reads: a gzip-compressed one is unpacked here, so its
/// unpacked size can be held to the import limit, which a highly compressed archive would
/// otherwise escape. The plain archive is followed to tell whether it reached its end.
pub(crate) struct Unpacker {
    mode: Mode,
    unpacked: u64,
    limit: u64,
    ending: Ending,
}

impl Unpacker {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            mode: Mode::Sniffing(Vec::new()),
            unpacked: 0,
            limit,
            ending: Ending::new(),
        }
    }

    /// The plain tar bytes for the next part of the upload. Feed it in small steps: a gzip step
    /// expands up to a thousand times.
    pub(crate) fn push(&mut self, mut input: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut output = Vec::new();
        if let Mode::Sniffing(head) = &mut self.mode {
            let take = (2 - head.len()).min(input.len());
            head.extend_from_slice(&input[..take]);
            input = &input[take..];
            if head.len() < 2 {
                return Ok(output);
            }
            let head = std::mem::take(head);
            self.mode = if head == [0x1f, 0x8b] {
                Mode::Gzip(Box::new(MultiGzDecoder::new(Vec::new())))
            } else {
                Mode::Plain
            };
            self.feed(&head, &mut output)?;
        }
        self.feed(input, &mut output)?;
        Ok(output)
    }

    fn feed(&mut self, mut input: &[u8], output: &mut Vec<u8>) -> anyhow::Result<()> {
        let before = output.len();
        match &mut self.mode {
            Mode::Plain => output.extend_from_slice(input),
            Mode::Gzip(decoder) => {
                while !input.is_empty() {
                    let used = decoder
                        .write(input)
                        .map_err(|_| anyhow!("The archive's gzip data could not be read"))?;
                    if used == 0 {
                        bail!("The archive's gzip data could not be read");
                    }
                    input = &input[used..];
                    output.append(decoder.get_mut());
                }
            }
            Mode::Sniffing(_) => unreachable!("the format is known before data is fed"),
        }
        self.ending.push(&output[before..]);
        self.count(output.len() - before)
    }

    fn count(&mut self, bytes: usize) -> anyhow::Result<()> {
        self.unpacked += bytes as u64;
        if self.unpacked > self.limit {
            bail!("The unpacked archive is larger than the import limit (IMAGE_ARCHIVE_MAX_MB)");
        }
        Ok(())
    }

    /// The rest once the upload has ended, and whether the plain archive reached its
    /// end-of-archive block.
    pub(crate) fn finish(mut self) -> anyhow::Result<(Vec<u8>, bool)> {
        let rest = match &mut self.mode {
            // Too short to be an archive; tar says so.
            Mode::Sniffing(head) => std::mem::take(head),
            Mode::Plain => Vec::new(),
            Mode::Gzip(decoder) => {
                decoder
                    .try_finish()
                    .map_err(|_| anyhow!("The archive's gzip data ended early"))?;
                std::mem::take(decoder.get_mut())
            }
        };
        self.ending.push(&rest);
        self.count(rest.len())?;
        Ok((rest, self.ending.reached()))
    }
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
