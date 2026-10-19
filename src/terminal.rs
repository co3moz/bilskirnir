use crate::{
    App, apps,
    error::{Error, Result, missing},
    model::{Terminal, now},
    process,
};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

const RUNNER: &str = include_str!("terminal_runner.py");
const CONTROL: &str = include_str!("terminal_control.py");
pub const ROOT: &str = "/tmp/bilskirnir-terminals";

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTerminal {
    /// Working directory; defaults to the user's home.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Run the shell as root instead of the desktop user.
    #[serde(default)]
    pub root: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalInput {
    /// Text to type into the terminal.
    pub text: String,
    /// Append a newline so the shell runs the line.
    #[serde(default = "yes")]
    pub press_enter: bool,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TerminalOutput {
    /// Byte offset to read from; use next_offset from the previous call.
    #[serde(default)]
    pub offset: u64,
    /// Maximum bytes to return (1 to 524288).
    #[serde(default = "limit")]
    pub limit: usize,
    /// Remove terminal escape sequences and carriage returns.
    #[serde(default = "yes")]
    pub strip_ansi: bool,
}
fn yes() -> bool {
    true
}
fn limit() -> usize {
    65536
}
impl OpenTerminal {
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(cwd) = &self.cwd {
            apps::directory(cwd)?;
        }
        Ok(())
    }
}
impl TerminalInput {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.text.is_empty() || self.text.len() > 65536 || self.text.contains('\0') {
            bail!("text must be 1 to 65536 bytes without NUL characters");
        }
        Ok(())
    }
}
impl TerminalOutput {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.limit == 0 || self.limit > 524_288 {
            bail!("limit must be 1 to 524288");
        }
        Ok(())
    }
}
fn record(app: &App, id: &str, terminal: &str) -> Result<Terminal> {
    let record: Terminal = app.db.get("terminal", terminal)?;
    if record.container_id != id {
        return Err(missing());
    }
    Ok(record)
}
async fn running(app: &App, id: &str) -> Result<Value> {
    let env = app.container(id)?;
    let info = app.owned(&env).await?;
    if info["State"]["Running"] != true {
        return Err(Error(
            StatusCode::CONFLICT,
            "Start the container before using terminals".into(),
        ));
    }
    Ok(info)
}
fn user(record: &Terminal) -> &'static str {
    if record.root { "root" } else { "1001:1001" }
}
async fn control(app: &App, container: &str, who: &str, arguments: &[&str]) -> Result<Value> {
    let mut args = vec!["exec", "--user", who, container, "python3", "-c", CONTROL];
    args.extend(arguments);
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let output = process::capture(
        app.command(&args),
        Duration::from_secs(60),
        1_048_576,
        process::Overflow::Reject,
    )
    .await?;
    if !output.status.success() {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| anyhow::anyhow!(e).into())
}
fn describe(record: &Terminal, status: Value) -> Value {
    let mut value = json!({
        "terminal_id": record.id,
        "container_id": record.container_id,
        "root": record.root,
        "cwd": record.cwd,
        "created_at": record.created_at,
    });
    if let Some(fields) = status.as_object() {
        for (key, field) in fields {
            value[key] = field.clone();
        }
    }
    value
}
pub async fn open_terminal(app: &Arc<App>, id: &str, body: &OpenTerminal) -> Result<Value> {
    body.validate()?;
    let _guard = app.hold(&format!("container:{id}")).await?;
    let info = running(app, id).await?;
    let home = if body.root { "/root" } else { "/home/desktop" };
    let record = Terminal {
        id: uuid::Uuid::new_v4().simple().to_string(),
        container_id: id.into(),
        root: body.root,
        cwd: body.cwd.clone().unwrap_or_else(|| home.into()),
        created_at: now(),
    };
    let directory = format!("{ROOT}/{}", record.id);
    let container = info["Id"].as_str().context("Missing container ID")?;
    app.docker(&[
        "exec",
        "--detach",
        "--user",
        user(&record),
        "--env",
        &format!("HOME={home}"),
        "--env",
        if body.root {
            "USER=root"
        } else {
            "USER=desktop"
        },
        container,
        "python3",
        "-c",
        RUNNER,
        &directory,
        &record.cwd,
    ])
    .await?;
    app.db.put("terminal", &record.id, &record)?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let status = control(app, container, user(&record), &["status", &directory])
        .await
        .unwrap_or_else(|_| json!({"state": "starting"}));
    Ok(describe(&record, status))
}
pub async fn terminal_input(
    app: &App,
    id: &str,
    terminal: &str,
    body: &TerminalInput,
) -> Result<Value> {
    body.validate()?;
    let record = record(app, id, terminal)?;
    let info = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let mut text = body.text.clone();
    if body.press_enter {
        text.push('\n');
    }
    let status = control(
        app,
        container,
        user(&record),
        &[
            "write",
            &format!("{ROOT}/{}", record.id),
            &STANDARD.encode(text),
        ],
    )
    .await?;
    Ok(describe(&record, status))
}
pub async fn terminal_output(
    app: &App,
    id: &str,
    terminal: &str,
    body: &TerminalOutput,
) -> Result<Value> {
    body.validate()?;
    let record = record(app, id, terminal)?;
    let info = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let status = control(
        app,
        container,
        user(&record),
        &[
            "read",
            &format!("{ROOT}/{}", record.id),
            &body.offset.to_string(),
            &body.limit.to_string(),
            if body.strip_ansi { "true" } else { "false" },
        ],
    )
    .await?;
    Ok(describe(&record, status))
}
pub async fn close_terminal(app: &App, id: &str, terminal: &str) -> Result<Value> {
    let record = record(app, id, terminal)?;
    let info = running(app, id).await?;
    let container = info["Id"].as_str().context("Missing container ID")?;
    let status = control(
        app,
        container,
        user(&record),
        &["stop", &format!("{ROOT}/{}", record.id), "10"],
    )
    .await?;
    Ok(describe(&record, status))
}
pub async fn list_terminals(app: &App, id: &str) -> Result<Value> {
    let env = app.container(id)?;
    let info = app.owned(&env).await?;
    let records = app
        .db
        .list::<Terminal>("terminal")?
        .into_iter()
        .filter(|record| record.container_id == id)
        .take(50)
        .collect::<Vec<_>>();
    let mut items = Vec::new();
    for record in &records {
        let status = if info["State"]["Running"] == true {
            let container = info["Id"].as_str().context("Missing container ID")?;
            control(
                app,
                container,
                user(record),
                &["status", &format!("{ROOT}/{}", record.id)],
            )
            .await
            .unwrap_or_else(|_| json!({"state": "unknown"}))
        } else {
            json!({"state": "unknown", "error": "Container is not running"})
        };
        items.push(describe(record, status));
    }
    Ok(json!({"items": items}))
}
pub async fn open(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    body: Option<Json<OpenTerminal>>,
) -> Result<Json<Value>> {
    let body = body.map(|Json(body)| body).unwrap_or_default();
    Ok(Json(open_terminal(&app, &id, &body).await?))
}
pub async fn list(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    Ok(Json(list_terminals(&app, &id).await?))
}
pub async fn input(
    State(app): State<Arc<App>>,
    Path((id, terminal)): Path<(String, String)>,
    Json(body): Json<TerminalInput>,
) -> Result<Json<Value>> {
    Ok(Json(terminal_input(&app, &id, &terminal, &body).await?))
}
pub async fn output(
    State(app): State<Arc<App>>,
    Path((id, terminal)): Path<(String, String)>,
    Query(body): Query<TerminalOutput>,
) -> Result<Json<Value>> {
    Ok(Json(terminal_output(&app, &id, &terminal, &body).await?))
}
pub async fn close(
    State(app): State<Arc<App>>,
    Path((id, terminal)): Path<(String, String)>,
) -> Result<Json<Value>> {
    Ok(Json(close_terminal(&app, &id, &terminal).await?))
}
