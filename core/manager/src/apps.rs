use crate::{
    App,
    error::{Error, Result, missing},
    model::{Application, Container, now},
};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

const RUNNER: &str = include_str!("../helpers/app_runner.py");
const CONTROL: &str = include_str!("../helpers/app_control.py");
/// Supervisor files live in the desktop's tmpfs; they do not survive a restart or enter snapshots.
const ROOT: &str = "/tmp/thoric-apps";
pub const LOG_LIMIT: usize = 1024 * 1024;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    /// Program to start: a command on PATH or an absolute path inside the desktop.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory; defaults to /home/desktop.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Run through gpu-run (VirtualGL) for hardware OpenGL; needs a GPU-enabled container.
    #[serde(default)]
    pub gpu: bool,
    /// Extra environment variables for the application.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Display name for the application record; defaults to the command.
    #[serde(default)]
    pub name: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    /// Seconds between SIGTERM and SIGKILL (0 to 60).
    #[serde(default = "grace")]
    pub grace_seconds: u64,
}
fn grace() -> u64 {
    5
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Logs {
    /// Number of bytes from the end of the log (1 to 1048576).
    #[serde(default = "tail")]
    pub tail_bytes: usize,
}
fn tail() -> usize {
    65536
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    /// Sampling window in milliseconds (100 to 10000).
    #[serde(default = "duration")]
    pub duration_ms: u64,
}
fn duration() -> u64 {
    1000
}
fn text(value: &str, what: &str, limit: usize) -> anyhow::Result<()> {
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        bail!("{what} must be 1 to {limit} bytes without control characters");
    }
    Ok(())
}
pub fn directory(path: &str) -> anyhow::Result<()> {
    if !path.starts_with('/')
        || path.len() > 4096
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || (path != "/"
            && path[1..]
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == ".."))
    {
        bail!("Working directory must be a canonical absolute path");
    }
    Ok(())
}
impl Launch {
    pub fn validate(&self) -> anyhow::Result<()> {
        text(&self.command, "command", 4096)?;
        if self.args.len() > 100 {
            bail!("At most 100 arguments are supported");
        }
        if self
            .args
            .iter()
            .any(|arg| arg.len() > 4096 || arg.contains('\0'))
        {
            bail!("Arguments must be at most 4096 bytes without NUL characters");
        }
        if let Some(cwd) = &self.cwd {
            directory(cwd)?;
        }
        if let Some(name) = &self.name
            && (name.trim().is_empty() || name.chars().count() > 100)
        {
            bail!("Application name must be 1 to 100 characters");
        }
        if self.env.len() > 50 {
            bail!("At most 50 environment variables are supported");
        }
        for (key, value) in &self.env {
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().enumerate().all(|(i, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                })
            {
                bail!("Invalid environment variable name: {key}");
            }
            if ["DISPLAY", "HOME", "USER", "XDG_RUNTIME_DIR", "PULSE_SERVER"]
                .contains(&key.as_str())
            {
                bail!("{key} is set by the desktop and cannot be overridden");
            }
            if value.len() > 4096 || value.chars().any(char::is_control) {
                bail!(
                    "Environment variable {key} must be at most 4096 bytes without control characters"
                );
            }
        }
        Ok(())
    }
}
fn record(app: &App, id: &str, application: &str) -> Result<Application> {
    let record: Application = app.db.get("application", application)?;
    if record.container_id != id {
        return Err(missing());
    }
    Ok(record)
}
async fn running(app: &App, id: &str) -> Result<(Container, Value)> {
    let env = app.container(id)?;
    let info = app.owned(&env).await?;
    if info["State"]["Running"] != true {
        return Err(Error(
            StatusCode::CONFLICT,
            "Start the container before working with its applications".into(),
        ));
    }
    Ok((env, info))
}
async fn control(app: &App, container: &str, arguments: &[&str]) -> anyhow::Result<Value> {
    let mut args = vec![
        "exec",
        "--user",
        "1001:1001",
        container,
        "python3",
        "-c",
        CONTROL,
    ];
    args.extend(arguments);
    let output = app.docker(&args).await?;
    serde_json::from_slice(&output).context("Application control returned invalid data")
}
fn describe(record: &Application, status: Value) -> Value {
    let mut value = json!({
        "application_id": record.id,
        "container_id": record.container_id,
        "name": record.name,
        "command": record.command,
        "args": record.args,
        "cwd": record.cwd,
        "gpu": record.gpu,
        "created_at": record.created_at,
    });
    if let Some(fields) = status.as_object() {
        for (key, field) in fields {
            value[key] = field.clone();
        }
    }
    value
}
fn first(status: Value) -> anyhow::Result<Value> {
    status
        .as_array()
        .and_then(|items| items.first().cloned())
        .context("Missing application status")
}
pub async fn launch_application(app: &Arc<App>, id: &str, body: &Launch) -> Result<Value> {
    body.validate()?;
    let _guard = app.share(&format!("container:{id}")).await?;
    let (env, info) = running(app, id).await?;
    if body.gpu && !env.spec.gpu {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "This container was created without GPU access; gpu=true needs a GPU-enabled container"
                .into(),
        ));
    }
    let record = Application {
        id: uuid::Uuid::new_v4().simple().to_string(),
        container_id: id.into(),
        name: body.name.clone().unwrap_or_else(|| body.command.clone()),
        command: body.command.clone(),
        args: body.args.clone(),
        cwd: body.cwd.clone().unwrap_or_else(|| "/home/desktop".into()),
        gpu: body.gpu,
        created_at: now(),
    };
    let directory = format!("{ROOT}/{}", record.id);
    let container = info["Id"].as_str().context("Missing container ID")?;
    let mut args: Vec<String> = [
        "exec",
        "--detach",
        "--user",
        "1001:1001",
        "--workdir",
        record.cwd.as_str(),
        "--env",
        "HOME=/home/desktop",
        "--env",
        "USER=desktop",
        "--env",
        "DISPLAY=:0",
        "--env",
        "PULSE_SERVER=unix:/run/desktop/pulse/native",
        "--env",
        "XDG_RUNTIME_DIR=/run/desktop",
    ]
    .map(str::to_owned)
    .to_vec();
    for (key, value) in &body.env {
        args.push("--env".into());
        args.push(format!("{key}={value}"));
    }
    args.push(container.into());
    args.extend(["python3", "-c", RUNNER, directory.as_str()].map(str::to_owned));
    if body.gpu {
        args.push("/usr/local/bin/gpu-run".into());
    }
    args.push(record.command.clone());
    args.extend(record.args.iter().cloned());
    app.docker_args(&args).await?;
    app.db.put("application", &record.id, &record)?;
    // Give the supervisor a moment so immediate failures (missing program, bad directory) are reported.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let status = match control(app, container, &["status", &directory]).await {
        Ok(status) => first(status)?,
        Err(_) => json!({"state": "starting"}),
    };
    Ok(describe(&record, status))
}
pub async fn list_applications(app: &App, id: &str) -> Result<Value> {
    let env = app.container(id)?;
    let records = app
        .db
        .list::<Application>("application")?
        .into_iter()
        .filter(|record| record.container_id == id)
        .take(100)
        .collect::<Vec<_>>();
    let info = app.owned(&env).await?;
    let statuses = if info["State"]["Running"] == true && !records.is_empty() {
        let directories = records
            .iter()
            .map(|record| format!("{ROOT}/{}", record.id))
            .collect::<Vec<_>>();
        let mut arguments = vec!["status"];
        arguments.extend(directories.iter().map(String::as_str));
        let container = info["Id"].as_str().context("Missing container ID")?;
        control(app, container, &arguments)
            .await?
            .as_array()
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let items = records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            describe(
                record,
                statuses.get(index).cloned().unwrap_or_else(
                    || json!({"state": "unknown", "error": "Container is not running"}),
                ),
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({"items": items}))
}
pub async fn get_application(app: &App, id: &str, application: &str) -> Result<Value> {
    let record = record(app, id, application)?;
    let env = app.container(id)?;
    let info = app.owned(&env).await?;
    let status = if info["State"]["Running"] == true {
        let container = info["Id"].as_str().context("Missing container ID")?;
        first(
            control(
                app,
                container,
                &["status", &format!("{ROOT}/{}", record.id)],
            )
            .await?,
        )?
    } else {
        json!({"state": "unknown", "error": "Container is not running"})
    };
    Ok(describe(&record, status))
}
pub async fn application_logs(
    app: &App,
    id: &str,
    application: &str,
    tail_bytes: usize,
) -> Result<Value> {
    if tail_bytes == 0 || tail_bytes > LOG_LIMIT {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            format!("tail_bytes must be 1 to {LOG_LIMIT}"),
        ));
    }
    let record = record(app, id, application)?;
    let (_, info) = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let mut value = control(
        app,
        container,
        &[
            "logs",
            &format!("{ROOT}/{}", record.id),
            &tail_bytes.to_string(),
        ],
    )
    .await?;
    value["application_id"] = json!(record.id);
    Ok(value)
}
pub async fn stop_application(
    app: &Arc<App>,
    id: &str,
    application: &str,
    grace_seconds: u64,
) -> Result<Value> {
    if grace_seconds > 60 {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "grace_seconds must be at most 60".into(),
        ));
    }
    let record = record(app, id, application)?;
    let _guard = app.share(&format!("container:{id}")).await?;
    let (_, info) = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let status = first(
        control(
            app,
            container,
            &[
                "stop",
                &format!("{ROOT}/{}", record.id),
                &grace_seconds.to_string(),
            ],
        )
        .await
        .map(|status| json!([status]))?,
    )?;
    Ok(describe(&record, status))
}
pub async fn application_diagnostics(app: &App, id: &str, application: &str) -> Result<Value> {
    let record = record(app, id, application)?;
    let (_, info) = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let status = control(
        app,
        container,
        &["diagnostics", &format!("{ROOT}/{}", record.id)],
    )
    .await?;
    Ok(describe(&record, status))
}
pub async fn sample_metrics(
    app: &App,
    id: &str,
    application: &str,
    duration_ms: u64,
) -> Result<Value> {
    if !(100..=10_000).contains(&duration_ms) {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "duration_ms must be 100 to 10000".into(),
        ));
    }
    let record = record(app, id, application)?;
    let (_, info) = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let mut value = control(
        app,
        container,
        &[
            "metrics",
            &format!("{ROOT}/{}", record.id),
            &duration_ms.to_string(),
        ],
    )
    .await?;
    value["application_id"] = json!(record.id);
    Ok(value)
}
pub async fn diagnostics(
    State(app): State<Arc<App>>,
    Path((id, application)): Path<(String, String)>,
) -> Result<Json<Value>> {
    Ok(Json(
        application_diagnostics(&app, &id, &application).await?,
    ))
}
pub async fn metrics(
    State(app): State<Arc<App>>,
    Path((id, application)): Path<(String, String)>,
    Query(options): Query<Metrics>,
) -> Result<Json<Value>> {
    Ok(Json(
        sample_metrics(&app, &id, &application, options.duration_ms).await?,
    ))
}
pub async fn launch(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Launch>,
) -> Result<Json<Value>> {
    Ok(Json(launch_application(&app, &id, &body).await?))
}
pub async fn list(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    Ok(Json(list_applications(&app, &id).await?))
}
pub async fn get(
    State(app): State<Arc<App>>,
    Path((id, application)): Path<(String, String)>,
) -> Result<Json<Value>> {
    Ok(Json(get_application(&app, &id, &application).await?))
}
pub async fn logs(
    State(app): State<Arc<App>>,
    Path((id, application)): Path<(String, String)>,
    Query(options): Query<Logs>,
) -> Result<Json<Value>> {
    Ok(Json(
        application_logs(&app, &id, &application, options.tail_bytes).await?,
    ))
}
pub async fn stop(
    State(app): State<Arc<App>>,
    Path((id, application)): Path<(String, String)>,
    body: Option<Json<Stop>>,
) -> Result<Json<Value>> {
    let grace_seconds = body
        .map(|Json(body)| body.grace_seconds)
        .unwrap_or_else(grace);
    Ok(Json(
        stop_application(&app, &id, &application, grace_seconds).await?,
    ))
}
#[cfg(test)]
#[path = "apps_tests.rs"]
mod tests;
