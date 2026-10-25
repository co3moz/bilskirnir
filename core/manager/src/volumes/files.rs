//! A volume's files: listed, read, written, moved, deleted, uploaded and downloaded through a
//! helper container, with the same helpers and rules as a running desktop's files. Paths are
//! absolute within the volume, `/` being its root.
use super::{Access, MOUNT, Volume};
use crate::{
    App, auth,
    error::{Error, Result},
    files::{
        DeleteFile, Encoding, FILE_LIMIT, ListFiles, MoveFile, ReadFile, WriteFile, mime_type,
        percent_encode, split_path,
    },
    folder_archive::{self, Target},
    uploads::Upload,
};
use anyhow::{Context, anyhow, bail};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const FILE_OPS: &str = include_str!("../../helpers/file_ops.py");
const READ_FILE: &str = include_str!("../../helpers/read_file.py");
const UPLOAD_FILE: &str = include_str!("../../helpers/upload_file.py");
const DOWNLOAD_FILE: &str = include_str!("../../helpers/download_file.py");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MakeDirectory {
    pub path: String,
}

/// The helper's path for a path within the volume.
pub(crate) fn inside(path: &str) -> String {
    if path == "/" {
        MOUNT.into()
    } else {
        format!("{MOUNT}{path}")
    }
}

/// The path within the volume for one a helper reported.
pub(crate) fn outside(path: &str) -> String {
    match path.strip_prefix(MOUNT) {
        Some("") => "/".into(),
        Some(rest) if rest.starts_with('/') => rest.into(),
        _ => path.into(),
    }
}

/// A file or directory in the volume that can be changed: anything but its root.
fn entry(path: &str) -> anyhow::Result<()> {
    split_path(path)?;
    Ok(())
}

fn script(code: &str, arguments: &[&str]) -> Vec<String> {
    let mut command = vec!["python3".into(), "-c".into(), code.into()];
    command.extend(arguments.iter().map(|argument| argument.to_string()));
    command
}

impl App {
    async fn volume_ops(
        &self,
        volume: &Volume,
        access: Access,
        arguments: &[&str],
    ) -> Result<Value> {
        let output = self
            .run_helper(
                volume,
                access,
                &script(FILE_OPS, arguments),
                None,
                1_048_576,
            )
            .await?;
        let mut value: Value = serde_json::from_slice(&output).map_err(|e| anyhow!(e))?;
        for key in ["path", "source", "destination"] {
            if let Some(path) = value[key].as_str() {
                value[key] = json!(outside(path));
            }
        }
        Ok(value)
    }

    /// Write operations on one volume run one at a time.
    async fn volume_write<T>(
        &self,
        name: &str,
        work: impl AsyncFnOnce(&Volume) -> Result<T>,
    ) -> Result<T> {
        let _guard = self.hold(&format!("volume:{name}")).await?;
        let volume = self.open_volume(name).await?;
        work(&volume).await
    }
}

pub async fn list(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Json(body): Json<ListFiles>,
) -> Result<Json<Value>> {
    body.validate()?;
    let value = app
        .volume_read(&name, async |volume| {
            app.volume_ops(
                volume,
                Access::Read,
                &["list", &inside(&body.path), &body.limit.to_string()],
            )
            .await
        })
        .await?;
    Ok(Json(value))
}

pub async fn read(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Json(body): Json<ReadFile>,
) -> Result<Json<Value>> {
    entry(&body.path)?;
    if body.limit == 0 || body.limit > FILE_LIMIT || body.offset > i64::MAX as u64 {
        return Err(anyhow!(
            "Read limit must be 1 to 524288 bytes; offset must fit a signed 64-bit integer"
        )
        .into());
    }
    let output = app
        .volume_read(&name, async |volume| {
            app.run_helper(
                volume,
                Access::Read,
                &script(
                    READ_FILE,
                    &[
                        &inside(&body.path),
                        &body.offset.to_string(),
                        &body.limit.to_string(),
                    ],
                ),
                None,
                FILE_LIMIT * 2,
            )
            .await
        })
        .await?;
    let mut result: Value = serde_json::from_slice(&output).map_err(|e| anyhow!(e))?;
    let encoded = result["data_base64"]
        .as_str()
        .context("Missing file data")?;
    let (data, encoding) = match body.encoding {
        Encoding::Base64 => (encoded.to_string(), "base64"),
        Encoding::Utf8 => {
            let bytes = STANDARD.decode(encoded).map_err(|e| anyhow!(e))?;
            let text = String::from_utf8(bytes).map_err(|_| {
                Error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "The file is not UTF-8 text; download it instead".into(),
                )
            })?;
            (text, "utf8")
        }
    };
    let object = result.as_object_mut().context("Invalid helper output")?;
    object.remove("data_base64");
    object.insert("data".into(), json!(data));
    object.insert("encoding".into(), json!(encoding));
    object.insert("path".into(), json!(body.path));
    object.insert("offset".into(), json!(body.offset));
    Ok(Json(result))
}

pub async fn write(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Json(body): Json<WriteFile>,
) -> Result<Json<Value>> {
    let (directory, file) = split_path(&body.path)?;
    let data = match body.encoding {
        Encoding::Utf8 => body.data.into_bytes(),
        Encoding::Base64 => {
            if body.data.len() > FILE_LIMIT.div_ceil(3) * 4 {
                return Err(anyhow!("File writes are limited to 512 KiB").into());
            }
            STANDARD
                .decode(&body.data)
                .map_err(|_| anyhow!("data must be valid standard base64"))?
        }
    };
    if data.len() > FILE_LIMIT {
        return Err(anyhow!("File writes are limited to 512 KiB").into());
    }
    let size = data.len();
    app.volume_write(&name, async |volume| {
        app.run_helper(
            volume,
            Access::Write,
            &script(
                UPLOAD_FILE,
                &[
                    &inside(directory),
                    file,
                    &size.to_string(),
                    if body.replace { "true" } else { "false" },
                    "644",
                ],
            ),
            Some(data),
            8192,
        )
        .await
    })
    .await?;
    Ok(Json(json!({"path": body.path, "bytes": size})))
}

pub async fn make_directory(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Json(body): Json<MakeDirectory>,
) -> Result<Json<Value>> {
    entry(&body.path)?;
    let value = app
        .volume_write(&name, async |volume| {
            app.volume_ops(volume, Access::Write, &["mkdir", &inside(&body.path)])
                .await
        })
        .await?;
    Ok(Json(value))
}

pub async fn move_(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Json(body): Json<MoveFile>,
) -> Result<Json<Value>> {
    body.validate()?;
    let value = app
        .volume_write(&name, async |volume| {
            app.volume_ops(
                volume,
                Access::Write,
                &[
                    "move",
                    &inside(&body.source),
                    &inside(&body.destination),
                    if body.replace { "true" } else { "false" },
                ],
            )
            .await
        })
        .await?;
    Ok(Json(value))
}

pub async fn delete(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Json(body): Json<DeleteFile>,
) -> Result<Json<Value>> {
    // The volume's root is the volume itself; deleting it is not a file operation.
    entry(&body.path)?;
    let value = app
        .volume_write(&name, async |volume| {
            app.volume_ops(
                volume,
                Access::Write,
                &[
                    "delete",
                    &inside(&body.path),
                    if body.recursive { "true" } else { "false" },
                ],
            )
            .await
        })
        .await?;
    Ok(Json(value))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub path: String,
}

/// Stream a file or a folder from the volume: a regular file as it is, read as the desktop user
/// with its size known up front and limited by UPLOAD_MAX_MB, a folder as a `.tar.gz` read as a
/// volume export reads, so it is complete.
pub async fn download(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    Query(query): Query<DownloadQuery>,
    token: auth::Session,
) -> Result<Response> {
    let (_, file) = split_path(&query.path)?;
    let volume = app.open_volume(&name).await?;
    let path = inside(&query.path);
    let entry = app
        .volume_ops(&volume, Access::Read, &["stat", &path])
        .await?;
    let size = match folder_archive::target(&entry)? {
        Target::File(size) => size,
        Target::Folder => {
            let helper = super::helper_name();
            let command = folder_archive::command(&path);
            let source = folder_archive::Source {
                args: super::helper_args(&volume, Access::Archive, &helper, true, &command),
                helper: Some(helper),
                lock: Some(format!("volume:{name}")),
                label: format!("{} in the volume {name}", query.path),
                path: query.path.clone(),
            };
            return folder_archive::respond(
                app.clone(),
                format!("volume:{name}"),
                file,
                source,
                token,
            );
        }
    };
    if size > app.config.upload_limit {
        return Err(Error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "File exceeds the configured transfer limit (UPLOAD_MAX_MB)".into(),
        ));
    }
    let helper = super::helper_name();
    let args = super::helper_args(
        &volume,
        Access::Read,
        &helper,
        false,
        &script(
            DOWNLOAD_FILE,
            &[&path, &app.config.upload_limit.to_string()],
        ),
    );
    let mut child = {
        // Under the volume's lock until Docker has the helper: a delete between the check and
        // the run would otherwise have `docker run` create the volume anew.
        let _guard = app.hold(&format!("volume:{name}")).await?;
        app.open_volume(&name).await?;
        let mut child = app
            .command(&args)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(anyhow::Error::from)?;
        app.helper_started(&helper, &mut child).await;
        child
    };
    let stdout = child.stdout.take().context("Missing helper output")?;
    let stream = futures_util::stream::unfold((child, stdout), |(child, mut stdout)| async move {
        let mut buffer = vec![0; 65536];
        match stdout.read(&mut buffer).await {
            Ok(0) | Err(_) => None,
            Ok(n) => {
                buffer.truncate(n);
                Some((
                    Ok::<_, std::io::Error>(axum::body::Bytes::from(buffer)),
                    (child, stdout),
                ))
            }
        }
    });
    Ok((
        [
            (header::CONTENT_TYPE, mime_type(file).to_string()),
            (header::CONTENT_LENGTH, size.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename*=UTF-8''{}", percent_encode(file)),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

/// Stream an upload into the volume as the desktop user. The helper writes a temporary file and
/// moves it into place only once every byte has arrived.
pub async fn upload(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
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
    crate::uploads::file_name(&request.name)?;
    crate::apps::directory(&request.destination)?;
    let _permit = app.uploads.try_acquire().map_err(|_| {
        Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Two uploads are already in progress. Try again shortly.".into(),
        )
    })?;
    let revoked = app.session_revoked(session);
    let transfer = app.volume_write(&name, async |volume| {
        deliver(&app, volume, &request, body).await
    });
    tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(3600), transfer) => result.map_err(|_| Error(StatusCode::REQUEST_TIMEOUT, "Upload timed out".into()))?,
        _ = revoked => Err(Error(StatusCode::UNAUTHORIZED, "Session expired. Sign in again.".into())),
    }
}

async fn deliver(app: &App, volume: &Volume, request: &Upload, body: Body) -> Result<Json<Value>> {
    let helper = super::helper_name();
    let args = super::helper_args(
        volume,
        Access::Write,
        &helper,
        true,
        &script(
            UPLOAD_FILE,
            &[
                &inside(&request.destination),
                &request.name,
                &request.size.to_string(),
                if request.replace { "true" } else { "false" },
            ],
        ),
    );
    let mut child = app
        .command(&args)
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(anyhow::Error::from)?;
    let mut stdin = child.stdin.take().context("Missing helper input")?;
    let stderr = child.stderr.take().context("Missing helper errors")?;
    let errors = tokio::spawn(crate::process::read_limited(
        stderr,
        8192,
        crate::process::Overflow::Tail,
    ));
    let mut refused = false;
    let sent = async {
        let mut stream = body.into_data_stream();
        let mut received = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| anyhow!("Upload body interrupted: {error}"))?;
            received += chunk.len() as u64;
            if received > request.size {
                bail!("File size does not match the upload");
            }
            // The helper stops reading when it fails; its message says why.
            if stdin.write_all(&chunk).await.is_err() {
                refused = true;
                break;
            }
        }
        if received != request.size {
            bail!("File transfer was interrupted");
        }
        let _ = stdin.shutdown().await;
        drop(stdin);
        anyhow::Ok(received)
    }
    .await;
    let received = match sent {
        Ok(received) => received,
        Err(error) => {
            // An incomplete body must never be committed; the helper removes its temporary file.
            let _ = child.start_kill();
            let status = child.wait().await;
            let _ = app.docker(&["rm", "--force", &helper]).await;
            // A helper that stopped reading had refused the file, and its reason, such as a
            // folder the desktop user cannot write, is the answer.
            if refused
                && status.is_ok_and(|status| !status.success())
                && let Ok(Ok(errors)) = errors.await
                && !errors.is_empty()
            {
                return Err(super::helper_error(&errors));
            }
            return Err(Error(StatusCode::BAD_REQUEST, format!("{error:#}")));
        }
    };
    let status = child.wait().await.map_err(anyhow::Error::from)?;
    let errors = errors.await.map_err(anyhow::Error::from)??;
    if !status.success() {
        return Err(super::helper_error(&errors));
    }
    Ok(Json(json!({
        "name": request.name,
        "path": if request.destination == "/" {
            format!("/{}", request.name)
        } else {
            format!("{}/{}", request.destination, request.name)
        },
        "bytes": received,
    })))
}
