use crate::{
    App, auth,
    compose::Place,
    error::{Error, Result},
    model::{Container, Mount},
    process,
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    path::{Path as FilePath, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncWriteExt;

#[derive(Deserialize)]
pub struct Upload {
    pub destination: String,
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub replace: bool,
}

fn within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

/// A file name an upload may create: one path component without control characters.
pub(crate) fn file_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty()
        || name.len() > 255
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        anyhow::bail!("Invalid file name");
    }
    Ok(())
}

/// The desktop user's home, where every upload may go.
const HOME: &str = "/home/desktop";

/// Whether a file `name` may be written into the folder `destination` of a container.
pub fn validate_target(record: &Container, destination: &str, name: &str) -> anyhow::Result<()> {
    check_target(&record.mounts(), record.places(), destination, name)
}

/// The upload rule: anywhere in the desktop home, in a writable volume or in a folder the
/// compose names, and nowhere under a read-only volume. It keeps uploads out of the system
/// folders; it is not what protects the desktop, which is that files are written as the desktop
/// user and never through a symlink.
pub(crate) fn check_target(
    mounts: &[Mount],
    places: &[Place],
    destination: &str,
    name: &str,
) -> anyhow::Result<()> {
    file_name(name)?;
    if !destination.starts_with('/')
        || destination.len() > 4096
        || destination.contains('\\')
        || destination.chars().any(char::is_control)
        || destination[1..]
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        anyhow::bail!("Destination must be a canonical absolute directory");
    }
    if mounts
        .iter()
        .any(|mount| mount.readonly && within(destination, &mount.target))
    {
        anyhow::bail!("Cannot upload to a read-only volume");
    }
    if !within(destination, HOME)
        && !mounts
            .iter()
            .any(|mount| within(destination, &mount.target))
        && !places.iter().any(|place| within(destination, &place.path))
    {
        anyhow::bail!(
            "Choose a folder in the desktop home, a writable volume or a folder the image's compose names"
        );
    }
    Ok(())
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub async fn cleanup_temporary_files(data: &FilePath) -> anyhow::Result<()> {
    let directory = data.join("uploads");
    tokio::fs::create_dir_all(&directory).await?;
    let mut entries = tokio::fs::read_dir(directory).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name
            .strip_suffix(".part")
            .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            && entry.file_type().await?.is_file()
        {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

pub async fn upload(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(request): Query<Upload>,
    headers: HeaderMap,
    session: auth::Session,
    body: Body,
) -> Result<Json<Value>> {
    if request.size > app.config.upload_limit {
        return Err(Error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "File exceeds the configured upload limit".into(),
        ));
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        != Some("application/octet-stream")
    {
        return Err(Error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Send a binary file body".into(),
        ));
    }
    save_file(&app, id, request, body, app.session_revoked(session)).await
}

pub(crate) async fn save_file(
    app: &Arc<App>,
    id: String,
    request: Upload,
    body: Body,
    revoked: impl std::future::Future<Output = ()>,
) -> Result<Json<Value>> {
    if request.size > app.config.upload_limit {
        return Err(Error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "File exceeds the configured upload limit".into(),
        ));
    }
    let _permit = app.uploads.try_acquire().map_err(|_| {
        Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Two uploads are already in progress. Try again shortly.".into(),
        )
    })?;
    let transfer = async {
        let lock = app.resource_lock(&format!("container:{id}"));
        let _guard = lock.lock().await;
        let record = app.container(&id)?;
        validate_target(&record, &request.destination, &request.name)?;
        let container = app.owned(&record).await?;
        if container["State"]["Running"] != true {
            return Err(Error(
                StatusCode::CONFLICT,
                "Start the container before uploading files".into(),
            ));
        }
        for mount in container["Mounts"].as_array().into_iter().flatten() {
            if mount["Destination"]
                .as_str()
                .is_some_and(|root| within(&request.destination, root))
                && mount["RW"] != true
            {
                return Err(Error(
                    StatusCode::BAD_REQUEST,
                    "Destination is mounted read-only".into(),
                ));
            }
        }
        let temporary = TemporaryFile(
            app.config
                .data
                .join("uploads")
                .join(format!("{}.part", uuid::Uuid::new_v4().simple())),
        );
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(&temporary.0)
            .await
            .map_err(anyhow::Error::from)?;
        let mut stream = body.into_data_stream();
        let mut received = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|error| anyhow::anyhow!("Upload body interrupted: {error}"))?;
            received += chunk.len() as u64;
            if received > request.size {
                return Err(Error(
                    StatusCode::BAD_REQUEST,
                    "File size does not match the upload".into(),
                ));
            }
            file.write_all(&chunk).await.map_err(anyhow::Error::from)?;
        }
        if received != request.size {
            return Err(Error(
                StatusCode::BAD_REQUEST,
                "File transfer was interrupted".into(),
            ));
        }
        file.flush().await.map_err(anyhow::Error::from)?;
        drop(file);
        let mut input = tokio::fs::File::open(&temporary.0)
            .await
            .map_err(anyhow::Error::from)?;
        let args = vec![
            "exec".into(),
            "--interactive".into(),
            "--user".into(),
            "1001:1001".into(),
            container["Id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing container ID"))?
                .into(),
            "python3".into(),
            "-c".into(),
            include_str!("upload_file.py").into(),
            request.destination.clone(),
            request.name.clone(),
            request.size.to_string(),
            request.replace.to_string(),
        ];
        let mut child = app
            .command(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(anyhow::Error::from)?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (written, output, error, status) = tokio::join!(
            async move {
                tokio::io::copy(&mut input, &mut stdin).await?;
                stdin.shutdown().await
            },
            process::read_limited(stdout, 8192, process::Overflow::Tail),
            process::read_limited(stderr, 8192, process::Overflow::Tail),
            child.wait()
        );
        output?;
        let error = String::from_utf8_lossy(&error?).trim().to_owned();
        if !status.map_err(anyhow::Error::from)?.success() {
            return Err(Error(
                if error.starts_with("CONFLICT:") {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::BAD_REQUEST
                },
                if error.is_empty() {
                    "Could not save file in the container".into()
                } else {
                    error.trim_start_matches("CONFLICT: ").into()
                },
            ));
        }
        written.map_err(anyhow::Error::from)?;
        Ok(Json(
            json!({"name":request.name,"path":format!("{}/{}",request.destination,request.name),"bytes":received}),
        ))
    };
    tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(3600), transfer) => result.map_err(|_| Error(StatusCode::REQUEST_TIMEOUT, "Upload timed out".into()))?,
        _ = revoked => Err(Error(StatusCode::UNAUTHORIZED, "Session expired. Sign in again.".into())),
    }
}
