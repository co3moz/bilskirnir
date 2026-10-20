use crate::{
    App, apps,
    capture::{self, Capture},
    error::Result,
    windows,
};
use anyhow::bail;
use axum::{
    Json,
    extract::{Path, State},
};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Conditions that must all hold before the wait returns.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitFor {
    /// A visible window whose title contains this text.
    #[serde(default)]
    pub window_title: Option<String>,
    /// An application from launch_application reaching application_state.
    #[serde(default)]
    pub application_id: Option<String>,
    /// Target state for application_id: "running" or "exited" (exited also matches failed).
    /// Defaults to exited.
    #[serde(default)]
    pub application_state: Option<String>,
    /// Text that must appear in the application's log; needs application_id.
    #[serde(default)]
    pub log_contains: Option<String>,
    /// The desktop (or region) must not change for this many milliseconds.
    #[serde(default)]
    pub screen_stable_ms: Option<u64>,
    /// Region x,y,width,height observed for screen_stable_ms; defaults to the whole desktop.
    #[serde(default)]
    pub region: Option<String>,
    /// Give up after this many milliseconds (100 to 60000).
    #[serde(default = "timeout")]
    pub timeout_ms: u64,
    /// Milliseconds between checks (100 to 5000).
    #[serde(default = "interval")]
    pub interval_ms: u64,
}
fn timeout() -> u64 {
    10_000
}
fn interval() -> u64 {
    500
}
impl WaitFor {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.window_title.is_none()
            && self.application_id.is_none()
            && self.screen_stable_ms.is_none()
        {
            bail!("Provide window_title, application_id or screen_stable_ms");
        }
        if let Some(title) = &self.window_title
            && (title.is_empty() || title.len() > 512)
        {
            bail!("window_title must be 1 to 512 bytes");
        }
        if let Some(state) = &self.application_state {
            if self.application_id.is_none() {
                bail!("application_state needs application_id");
            }
            if !["running", "exited"].contains(&state.as_str()) {
                bail!("application_state must be running or exited");
            }
        }
        if let Some(text) = &self.log_contains {
            if self.application_id.is_none() {
                bail!("log_contains needs application_id");
            }
            if text.is_empty() || text.len() > 4096 {
                bail!("log_contains must be 1 to 4096 bytes");
            }
        }
        if !(100..=60_000).contains(&self.timeout_ms) {
            bail!("timeout_ms must be 100 to 60000");
        }
        if !(100..=5_000).contains(&self.interval_ms) {
            bail!("interval_ms must be 100 to 5000");
        }
        if let Some(stable) = self.screen_stable_ms
            && (!(100..=30_000).contains(&stable) || stable > self.timeout_ms)
        {
            bail!("screen_stable_ms must be 100 to 30000 and at most timeout_ms");
        }
        if self.region.is_some() && self.screen_stable_ms.is_none() {
            bail!("region needs screen_stable_ms");
        }
        Capture {
            max_width: None,
            region: self.region.clone(),
            window_id: None,
        }
        .validate()?;
        Ok(())
    }
}
pub async fn wait_for_condition(app: &Arc<App>, id: &str, body: &WaitFor) -> Result<Value> {
    body.validate()?;
    app.container(id)?;
    let started = Instant::now();
    let deadline = started + Duration::from_millis(body.timeout_ms);
    let interval = Duration::from_millis(body.interval_ms);
    let mut last_screen: Option<(Vec<u8>, Instant)> = None;
    loop {
        let mut result = json!({});
        let mut satisfied = true;
        if let Some(title) = &body.window_title {
            let windows = windows::list_windows(app, id).await?;
            let found = windows["items"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|w| w["title"].as_str().is_some_and(|t| t.contains(title)))
                })
                .cloned();
            satisfied &= found.is_some();
            result["window"] = found.unwrap_or(Value::Null);
        }
        if let Some(application) = &body.application_id {
            let status = apps::get_application(app, id, application).await?;
            let state = status["state"].as_str().unwrap_or("unknown");
            let target = body.application_state.as_deref().unwrap_or("exited");
            let reached = match target {
                "running" => state == "running",
                _ => state == "exited" || state == "failed",
            };
            satisfied &= reached;
            result["application"] = json!({
                "application_id": application,
                "state": state,
                "exit_code": status["exit_code"],
                "signal": status["signal"],
            });
            if let Some(text) = &body.log_contains {
                let logs = apps::application_logs(app, id, application, apps::LOG_LIMIT).await?;
                let found = logs["log"].as_str().is_some_and(|log| log.contains(text));
                satisfied &= found;
                result["log_found"] = json!(found);
            }
        }
        if let Some(stable) = body.screen_stable_ms {
            let shot = capture::capture(
                app,
                id,
                &Capture {
                    max_width: Some(480),
                    region: body.region.clone(),
                    window_id: None,
                },
            )
            .await?;
            let stable_for = match &last_screen {
                Some((bytes, since)) if *bytes == shot.jpeg => since.elapsed(),
                _ => {
                    last_screen = Some((shot.jpeg, Instant::now()));
                    Duration::ZERO
                }
            };
            satisfied &= stable_for >= Duration::from_millis(stable);
            result["screen_stable_for_ms"] = json!(stable_for.as_millis() as u64);
        }
        result["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
        if satisfied {
            result["satisfied"] = json!(true);
            return Ok(result);
        }
        if Instant::now() + interval > deadline {
            result["satisfied"] = json!(false);
            return Ok(result);
        }
        tokio::time::sleep(interval).await;
    }
}
pub async fn wait(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<WaitFor>,
) -> Result<Json<Value>> {
    Ok(Json(wait_for_condition(&app, &id, &body).await?))
}
