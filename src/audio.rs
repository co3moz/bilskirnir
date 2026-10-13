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
use serde_json::Value;
use std::{sync::Arc, time::Duration};

const TOOLS: &str = include_str!("audio_tools.py");

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureAudio {
    /// Seconds to record (0.1–10).
    #[serde(default = "seconds")]
    pub seconds: f64,
    /// "desktop" records what the desktop plays; "microphone" records what the browser or
    /// inject_audio sends to the desktop's microphone.
    #[serde(default = "desktop")]
    pub source: String,
}
fn seconds() -> f64 {
    3.0
}
fn desktop() -> String {
    "desktop".into()
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InjectAudio {
    /// Standard base64 of a 16-bit PCM WAV file (at most 1,000,000 characters).
    pub data_base64: String,
    /// "microphone" plays into the desktop's microphone; "speakers" plays into the desktop's
    /// audio output.
    #[serde(default = "microphone")]
    pub target: String,
}
fn microphone() -> String {
    "microphone".into()
}
impl CaptureAudio {
    pub fn validate(&self) -> anyhow::Result<&'static str> {
        if !self.seconds.is_finite() || !(0.1..=10.0).contains(&self.seconds) {
            bail!("seconds must be 0.1–10");
        }
        match self.source.as_str() {
            "desktop" => Ok("desktop-output.monitor"),
            "microphone" => Ok("browser-microphone"),
            _ => bail!("source must be desktop or microphone"),
        }
    }
}
impl InjectAudio {
    pub fn validate(&self) -> anyhow::Result<&'static str> {
        if self.data_base64.is_empty() || self.data_base64.len() > 1_000_000 {
            bail!("data_base64 must be 1–1000000 characters");
        }
        match self.target.as_str() {
            "microphone" => Ok("browser-mic-sink"),
            "speakers" => Ok("desktop-output"),
            _ => bail!("target must be microphone or speakers"),
        }
    }
}
async fn container(app: &App, id: &str, microphone: bool) -> Result<Value> {
    let env = app.environment(id)?;
    let info = app.owned(&env).await?;
    if info["State"]["Running"] != true {
        return Err(Error(
            StatusCode::CONFLICT,
            "Start the environment before using its audio".into(),
        ));
    }
    if !env.spec.audio || !app.config.audio {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Audio is disabled for this environment".into(),
        ));
    }
    if microphone && (!env.spec.microphone || !app.config.microphone) {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Microphone is disabled for this environment".into(),
        ));
    }
    Ok(info)
}
async fn run(
    app: &App,
    info: &Value,
    arguments: &[&str],
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Value> {
    let mut args = vec![
        "exec",
        "--interactive",
        "--user",
        "desktop",
        "--env",
        "PULSE_SERVER=unix:/run/desktop/pulse/native",
        info["Id"].as_str().context("Missing container ID")?,
        "python3",
        "-c",
        TOOLS,
    ];
    args.extend(arguments);
    let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let output = process::capture_input(
        app.command(&args),
        timeout,
        8 * 1024 * 1024,
        process::Overflow::Reject,
        input,
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
pub async fn capture_audio(app: &App, id: &str, body: &CaptureAudio) -> Result<Value> {
    let device = body.validate()?;
    let info = container(app, id, body.source == "microphone").await?;
    run(
        app,
        &info,
        &["capture", &body.seconds.to_string(), device],
        None,
        Duration::from_secs_f64(body.seconds + 30.0),
    )
    .await
}
pub async fn inject_audio(app: &App, id: &str, body: &InjectAudio) -> Result<Value> {
    let sink = body.validate()?;
    let data = STANDARD.decode(&body.data_base64).map_err(|_| {
        Error(
            StatusCode::BAD_REQUEST,
            "data_base64 must be valid standard base64".into(),
        )
    })?;
    let info = container(app, id, body.target == "microphone").await?;
    run(
        app,
        &info,
        &["play", sink],
        Some(data),
        Duration::from_secs(150),
    )
    .await
}
pub async fn capture(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<CaptureAudio>,
) -> Result<Json<Value>> {
    Ok(Json(capture_audio(&app, &id, &body).await?))
}
pub async fn inject(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<InjectAudio>,
) -> Result<Json<Value>> {
    Ok(Json(inject_audio(&app, &id, &body).await?))
}
