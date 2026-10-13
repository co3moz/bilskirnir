use crate::{
    App,
    error::{Error, Result},
    process,
};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub const FILE_LIMIT: usize = 512 * 1024;
#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    #[default]
    Utf8,
    Base64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadFile {
    pub path: String,
    #[serde(default)]
    pub encoding: Encoding,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    65536
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteFile {
    pub path: String,
    pub data: String,
    #[serde(default)]
    pub encoding: Encoding,
    #[serde(default)]
    pub replace: bool,
}
pub(crate) fn split_path(path: &str) -> anyhow::Result<(&str, &str)> {
    if !path.starts_with('/')
        || path.len() > 4096
        || path.contains(['\0', '\n', '\r', '\\'])
        || path[1..]
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        bail!("Use a canonical absolute file path without dot segments or symlinks");
    }
    let (directory, name) = path.rsplit_once('/').context("Invalid file path")?;
    Ok((if directory.is_empty() { "/" } else { directory }, name))
}
pub async fn read(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<ReadFile>,
) -> Result<Json<Value>> {
    split_path(&body.path)?;
    if body.limit == 0 || body.limit > FILE_LIMIT || body.offset > i64::MAX as u64 {
        return Err(anyhow::anyhow!(
            "Read limit must be 1–524288 bytes; offset must fit a signed 64-bit integer"
        )
        .into());
    }
    let lock = app.resource_lock(&format!("environment:{id}"));
    let _guard = lock.lock().await;
    let env = app.environment(&id)?;
    let container = app.owned(&env).await?;
    if container["State"]["Running"] != true {
        return Err(anyhow::anyhow!("Start the environment before reading files").into());
    }
    let bytes = app
        .docker(&[
            "exec",
            "--user",
            "1001:1001",
            container["Id"].as_str().context("Missing container ID")?,
            "python3",
            "-c",
            include_str!("read_file.py"),
            &body.path,
            &body.offset.to_string(),
            &body.limit.to_string(),
        ])
        .await?;
    let mut result: Value = serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!(e))?;
    let encoded = result["data_base64"]
        .as_str()
        .context("Missing file data")?;
    let (data, encoding) = match body.encoding {
        Encoding::Base64 => (encoded.to_string(), "base64"),
        Encoding::Utf8 => {
            let bytes = STANDARD.decode(encoded).map_err(|e| anyhow::anyhow!(e))?;
            (String::from_utf8(bytes).context("File slice is not valid UTF-8; use encoding=base64 for binary files or split characters")?, "utf8")
        }
    };
    result.as_object_mut().unwrap().remove("data_base64");
    result["data"] = json!(data);
    result["encoding"] = json!(encoding);
    result["path"] = json!(body.path);
    result["offset"] = json!(body.offset);
    Ok(Json(result))
}
pub async fn write(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<WriteFile>,
) -> Result<Json<Value>> {
    let (directory, name) = split_path(&body.path)?;
    let data = match body.encoding {
        Encoding::Utf8 => body.data.as_bytes().to_vec(),
        Encoding::Base64 => {
            if body.data.len() > FILE_LIMIT.div_ceil(3) * 4 {
                return Err(anyhow::anyhow!("File writes are limited to 512 KiB").into());
            }
            STANDARD
                .decode(&body.data)
                .map_err(|_| anyhow::anyhow!("data must be valid standard base64"))?
        }
    };
    if data.len() > FILE_LIMIT {
        return Err(anyhow::anyhow!("File writes are limited to 512 KiB").into());
    }
    let size = data.len();
    let lock = app.resource_lock(&format!("environment:{id}"));
    let _guard = lock.lock().await;
    let env = app.environment(&id)?;
    let container = app.owned(&env).await?;
    if container["State"]["Running"] != true {
        return Err(anyhow::anyhow!("Start the environment before writing files").into());
    }
    let args = [
        "exec",
        "--interactive",
        "--user",
        "1001:1001",
        container["Id"].as_str().context("Missing container ID")?,
        "python3",
        "-c",
        include_str!("upload_file.py"),
        directory,
        name,
        &size.to_string(),
        if body.replace { "true" } else { "false" },
        "600",
    ]
    .map(str::to_owned);
    let output = process::capture_input(
        app.command(&args),
        Duration::from_secs(30),
        8192,
        process::Overflow::Tail,
        Some(data),
    )
    .await?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error(
            if message.starts_with("CONFLICT:") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            },
            message,
        ));
    }
    Ok(Json(json!({"path":body.path,"bytes":size})))
}

const FILE_OPS: &str = include_str!("file_ops.py");
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListFiles {
    /// Canonical absolute directory path.
    pub path: String,
    /// Maximum number of entries to return (1–2000).
    #[serde(default = "list_limit")]
    pub limit: usize,
}
fn list_limit() -> usize {
    500
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveFile {
    /// Canonical absolute path of the file or directory to move.
    pub source: String,
    /// Canonical absolute destination path on the same volume.
    pub destination: String,
    /// Replace an existing destination.
    #[serde(default)]
    pub replace: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteFile {
    /// Canonical absolute path of the file, symlink or directory to delete.
    pub path: String,
    /// Delete a directory together with its contents.
    #[serde(default)]
    pub recursive: bool,
}
impl ListFiles {
    pub fn validate(&self) -> anyhow::Result<()> {
        crate::apps::directory(&self.path)?;
        if self.limit == 0 || self.limit > 2000 {
            bail!("limit must be 1–2000");
        }
        Ok(())
    }
}
impl MoveFile {
    pub fn validate(&self) -> anyhow::Result<()> {
        split_path(&self.source)?;
        split_path(&self.destination)?;
        Ok(())
    }
}
impl DeleteFile {
    pub fn validate(&self, spec: &crate::model::EnvironmentSpec) -> anyhow::Result<()> {
        split_path(&self.path)?;
        if self.path[1..].split('/').count() < 2
            || self.path == "/home/desktop"
            || spec.mounts.iter().any(|mount| mount.target == self.path)
        {
            bail!(
                "Refusing to delete a top-level directory, the desktop home or a mounted volume's root"
            );
        }
        Ok(())
    }
}
async fn operate(app: &App, id: &str, arguments: &[&str]) -> Result<Value> {
    let lock = app.resource_lock(&format!("environment:{id}"));
    let _guard = lock.lock().await;
    let env = app.environment(id)?;
    let container = app.owned(&env).await?;
    if container["State"]["Running"] != true {
        return Err(anyhow::anyhow!("Start the environment before using its files").into());
    }
    let mut args = vec![
        "exec",
        "--user",
        "1001:1001",
        container["Id"].as_str().context("Missing container ID")?,
        "python3",
        "-c",
        FILE_OPS,
    ];
    args.extend(arguments);
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let output = process::capture(
        app.command(&args),
        Duration::from_secs(120),
        1_048_576,
        process::Overflow::Reject,
    )
    .await?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error(
            if message.starts_with("CONFLICT:") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            },
            message,
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| anyhow::anyhow!(e).into())
}
pub async fn list_files(app: &App, id: &str, body: &ListFiles) -> Result<Value> {
    body.validate()?;
    operate(app, id, &["list", &body.path, &body.limit.to_string()]).await
}
pub async fn move_file(app: &App, id: &str, body: &MoveFile) -> Result<Value> {
    body.validate()?;
    operate(
        app,
        id,
        &[
            "move",
            &body.source,
            &body.destination,
            if body.replace { "true" } else { "false" },
        ],
    )
    .await
}
pub async fn delete_file(app: &App, id: &str, body: &DeleteFile) -> Result<Value> {
    let env = app.environment(id)?;
    body.validate(&env.spec)?;
    operate(
        app,
        id,
        &[
            "delete",
            &body.path,
            if body.recursive { "true" } else { "false" },
        ],
    )
    .await
}
pub async fn list(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<ListFiles>,
) -> Result<Json<Value>> {
    Ok(Json(list_files(&app, &id, &body).await?))
}
pub async fn move_(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<MoveFile>,
) -> Result<Json<Value>> {
    Ok(Json(move_file(&app, &id, &body).await?))
}
pub async fn delete(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<DeleteFile>,
) -> Result<Json<Value>> {
    Ok(Json(delete_file(&app, &id, &body).await?))
}
