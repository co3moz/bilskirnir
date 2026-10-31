use crate::{
    App, apps,
    capture::{self, Capture, Gray},
    error::{Error, Result},
    ocr, windows,
};
use anyhow::bail;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;

/// How much brighter or darker a pixel must turn to count as changed: more than JPEG's own noise
/// on an edge, less than any change a person would notice.
const PIXEL_CHANGE: u8 = 24;

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
    /// The desktop (or region) must differ from how it looked when the wait began, by at least
    /// screen_change_percent of its pixels.
    #[serde(default)]
    pub screen_changed: bool,
    /// For screen_changed: the share of pixels, in percent, that must differ noticeably from the
    /// start (0.01 to 100, default 0.5). Use a region to notice a small change.
    #[serde(default)]
    pub screen_change_percent: Option<f64>,
    /// Text read on the desktop (or region) with OCR, as find_text reads it: within one line,
    /// ignoring case, at most 256 bytes.
    #[serde(default)]
    pub text: Option<String>,
    /// For text: "visible" (default) waits until it is read, "gone" until it is no longer.
    #[serde(default)]
    pub text_state: Option<String>,
    /// Region x,y,width,height observed by screen_stable_ms, screen_changed and text; defaults to
    /// the whole desktop.
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
            && !self.screen_changed
            && self.text.is_none()
        {
            bail!("Provide window_title, application_id, screen_stable_ms, screen_changed or text");
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
        if let Some(percent) = self.screen_change_percent {
            if !self.screen_changed {
                bail!("screen_change_percent needs screen_changed");
            }
            if !(0.01..=100.0).contains(&percent) {
                bail!("screen_change_percent must be 0.01 to 100");
            }
        }
        if let Some(text) = &self.text {
            ocr::check_text(text, "text")?;
        }
        if let Some(state) = &self.text_state {
            if self.text.is_none() {
                bail!("text_state needs text");
            }
            if !["visible", "gone"].contains(&state.as_str()) {
                bail!("text_state must be visible or gone");
            }
        }
        if self.region.is_some()
            && self.screen_stable_ms.is_none()
            && !self.screen_changed
            && self.text.is_none()
        {
            bail!("region needs screen_stable_ms, screen_changed or text");
        }
        self.screen().validate()?;
        Ok(())
    }
    /// The small capture the screen conditions compare, of the region when there is one.
    fn screen(&self) -> Capture {
        Capture {
            max_width: Some(480),
            region: self.region.clone(),
            window_id: None,
        }
    }
}
/// The first round of a wait may take this long even when timeout_ms is shorter, so a short
/// wait still looks once: a screenshot and its reading take a second or two, longer while the
/// installation's two screenshot slots are taken.
const FIRST_LOOK: Duration = Duration::from_secs(5);

/// What one round saw, and whether every condition held.
struct Observation {
    result: Value,
    satisfied: bool,
}

/// What the screen conditions keep between rounds.
#[derive(Default)]
struct Seen {
    /// How the screen looked when the wait began, which screen_changed compares with: the first
    /// screen a round saw.
    baseline: Option<Gray>,
    /// The last screen and since when it has looked so, which screen_stable_ms measures.
    screen: Option<(Vec<u8>, Instant)>,
}

pub async fn wait_for_condition(app: &Arc<App>, id: &str, body: &WaitFor) -> Result<Value> {
    body.validate()?;
    app.container(id)?;
    let seen = Mutex::new(Seen::default());
    poll(
        Duration::from_millis(body.timeout_ms),
        Duration::from_millis(body.interval_ms),
        |until| observe(app, id, body, &seen, until),
    )
    .await
}

/// Run `round` every `interval` until it is satisfied or `timeout` has passed, each round given
/// the moment it must end by: the deadline, or for the first one at least [`FIRST_LOOK`] after
/// the start. A round that could not look, because both screenshot slots stayed taken (429) or a
/// step ran out of time (504), is an observation that did not happen, so the wait goes on; once
/// the time is up it answers `satisfied: false` with the last observation and, when the last
/// round could not look, why in `last_error`. Any other error ends the wait.
async fn poll<F, R>(timeout: Duration, interval: Duration, mut round: F) -> Result<Value>
where
    F: FnMut(Instant) -> R,
    R: Future<Output = Result<Observation>>,
{
    let started = Instant::now();
    let deadline = started + timeout;
    let mut until = started + timeout.max(FIRST_LOOK);
    let mut last = json!({});
    loop {
        let missed = match round(until).await {
            Ok(Observation {
                mut result,
                satisfied: true,
            }) => {
                result["satisfied"] = json!(true);
                result["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
                return Ok(result);
            }
            Ok(observation) => {
                last = observation.result;
                None
            }
            Err(Error(status, message))
                if status == StatusCode::TOO_MANY_REQUESTS
                    || status == StatusCode::GATEWAY_TIMEOUT =>
            {
                Some(message)
            }
            Err(error) => return Err(error),
        };
        // Another round starts only with time left before the deadline.
        if Instant::now() + interval >= deadline {
            last["satisfied"] = json!(false);
            last["elapsed_ms"] = json!(started.elapsed().as_millis() as u64);
            if let Some(message) = missed {
                last["last_error"] = json!(message);
            }
            return Ok(last);
        }
        tokio::time::sleep(interval).await;
        until = deadline;
    }
}

/// `step` of a round, ended at `until` as a step that ran out of time.
async fn by<T>(until: Instant, doing: &str, step: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout_at(until, step)
        .await
        .unwrap_or_else(|_| {
            Err(Error(
                StatusCode::GATEWAY_TIMEOUT,
                format!("The wait ran out of time while {doing}"),
            ))
        })
}

/// One look at every condition the wait names, each step ended at `until`.
async fn observe(
    app: &App,
    id: &str,
    body: &WaitFor,
    seen: &Mutex<Seen>,
    until: Instant,
) -> Result<Observation> {
    let mut result = json!({});
    let mut satisfied = true;
    if let Some(title) = &body.window_title {
        let windows = by(until, "listing windows", windows::list_windows(app, id)).await?;
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
        let status = by(
            until,
            "reading the application's state",
            apps::get_application(app, id, application),
        )
        .await?;
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
            let logs = by(
                until,
                "reading the application's log",
                apps::application_logs(app, id, application, apps::LOG_LIMIT),
            )
            .await?;
            let found = logs["log"].as_str().is_some_and(|log| log.contains(text));
            satisfied &= found;
            result["log_found"] = json!(found);
        }
    }
    if body.screen_stable_ms.is_some() || body.screen_changed {
        let shot = by(
            until,
            "taking a screenshot",
            capture::capture(app, id, &body.screen()),
        )
        .await?;
        let gray = if body.screen_changed {
            Some(Gray::from_jpeg(&shot.jpeg)?)
        } else {
            None
        };
        let mut seen = seen.lock().unwrap();
        if let Some(gray) = gray {
            let changed = match &seen.baseline {
                Some(baseline) => gray.changed_percent(baseline, PIXEL_CHANGE),
                None => {
                    seen.baseline = Some(gray);
                    0.0
                }
            };
            satisfied &= changed >= body.screen_change_percent.unwrap_or(0.5);
            result["screen_changed_percent"] = json!((changed * 100.0).round() / 100.0);
        }
        if let Some(stable) = body.screen_stable_ms {
            let stable_for = match &seen.screen {
                Some((bytes, since)) if *bytes == shot.jpeg => since.elapsed(),
                _ => {
                    seen.screen = Some((shot.jpeg, Instant::now()));
                    Duration::ZERO
                }
            };
            satisfied &= stable_for >= Duration::from_millis(stable);
            result["screen_stable_for_ms"] = json!(stable_for.as_millis() as u64);
        }
    }
    if let Some(text) = &body.text {
        let matches = by(
            until,
            "reading text on the screen",
            ocr::locate(app, id, body.region.as_deref(), text),
        )
        .await?;
        let gone = body.text_state.as_deref() == Some("gone");
        satisfied &= matches.is_empty() == gone;
        result["text_found"] = json!(!matches.is_empty());
        result["text_match"] = json!(matches.first().map(ocr::Match::text_match));
    }
    Ok(Observation { result, satisfied })
}
pub async fn wait(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<WaitFor>,
) -> Result<Json<Value>> {
    Ok(Json(wait_for_condition(&app, &id, &body).await?))
}
#[cfg(test)]
#[path = "waits_tests.rs"]
mod tests;
