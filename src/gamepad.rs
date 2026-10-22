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
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

const DAEMON: &str = include_str!("gamepad_daemon.py");
const CONTROL: &str = include_str!("gamepad_control.py");
const DIRECTORY: &str = "/tmp/bilskirnir-gamepad";
const BUTTONS: [&str; 11] = [
    "a", "b", "x", "y", "lb", "rb", "back", "start", "guide", "ls", "rs",
];
const STICKS: [&str; 4] = ["left_x", "left_y", "right_x", "right_y"];
const TRIGGERS: [&str; 2] = ["left_trigger", "right_trigger"];
const DPAD: [&str; 2] = ["dpad_x", "dpad_y"];

#[derive(Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GamepadAction {
    /// Press or release a button: a, b, x, y, lb, rb, back, start, guide, ls or rs. Buttons stay
    /// pressed until released or reset.
    Button {
        button: String,
        #[serde(default = "yes")]
        pressed: bool,
    },
    /// Set an axis: left_x, left_y, right_x, right_y from -1 to 1; left_trigger and
    /// right_trigger from 0 to 1; dpad_x and dpad_y as -1, 0 or 1. Axes keep their value.
    Axis { axis: String, value: f64 },
    /// Pause before the next action (at most 10000 ms, 30000 ms per request).
    Wait { milliseconds: u64 },
    /// Release every button and center every axis.
    Reset,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GamepadInput {
    pub actions: Vec<GamepadAction>,
}
impl GamepadInput {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.actions.is_empty() || self.actions.len() > 100 {
            bail!("Send 1 to 100 gamepad actions");
        }
        let mut delay = 0;
        for action in &self.actions {
            match action {
                GamepadAction::Button { button, .. } => {
                    if !BUTTONS.contains(&button.as_str()) {
                        bail!("Unknown gamepad button: {button}");
                    }
                }
                GamepadAction::Axis { axis, value } => {
                    let name = axis.as_str();
                    if !value.is_finite() {
                        bail!("Axis values must be finite");
                    }
                    if STICKS.contains(&name) {
                        if !(-1.0..=1.0).contains(value) {
                            bail!("Stick axes take values from -1 to 1");
                        }
                    } else if TRIGGERS.contains(&name) {
                        if !(0.0..=1.0).contains(value) {
                            bail!("Trigger axes take values from 0 to 1");
                        }
                    } else if DPAD.contains(&name) {
                        if ![-1.0, 0.0, 1.0].contains(value) {
                            bail!("D-pad axes take -1, 0 or 1");
                        }
                    } else {
                        bail!("Unknown gamepad axis: {axis}");
                    }
                }
                GamepadAction::Wait { milliseconds } => {
                    if *milliseconds > 10_000 {
                        bail!("A single wait is limited to 10000 milliseconds");
                    }
                    delay += milliseconds;
                    if delay > 30_000 {
                        bail!("Gamepad actions may wait at most 30 seconds in total");
                    }
                }
                GamepadAction::Reset => {}
            }
        }
        Ok(())
    }
    fn payload(&self) -> Vec<Value> {
        self.actions
            .iter()
            .map(|action| match action {
                GamepadAction::Button { button, pressed } => {
                    json!({"type": "button", "button": button, "pressed": pressed})
                }
                GamepadAction::Axis { axis, value } => {
                    json!({"type": "axis", "axis": axis, "value": value})
                }
                GamepadAction::Wait { milliseconds } => {
                    json!({"type": "wait", "milliseconds": milliseconds})
                }
                GamepadAction::Reset => json!({"type": "reset", "reset": true}),
            })
            .collect()
    }
}
async fn control(
    app: &App,
    container: &str,
    command: &str,
    input: Option<Vec<u8>>,
) -> Result<Value> {
    let args = [
        "exec",
        "--interactive",
        "--user",
        "root",
        container,
        "python3",
        "-c",
        CONTROL,
        DIRECTORY,
        command,
    ]
    .map(str::to_owned);
    let output = process::capture_input(
        app.command(&args),
        Duration::from_secs(45),
        65536,
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
/// Feed a virtual controller; the device is created on first use and persists until the desktop restarts.
pub async fn gamepad_input(app: &Arc<App>, id: &str, body: &GamepadInput) -> Result<Value> {
    body.validate()?;
    let _guard = app.hold(&format!("container:{id}")).await?;
    let env = app.container(id)?;
    // Like the audio channels, the manager carries no input while the installation has it off.
    if !app.config.gamepad {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "The virtual gamepad is disabled for this installation (GAMEPAD_ENABLED=false)".into(),
        ));
    }
    if !env.spec.gamepad {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Create the container with gamepad=true to use a virtual controller".into(),
        ));
    }
    let info = app.owned(&env).await?;
    if info["State"]["Running"] != true {
        return Err(Error(
            StatusCode::CONFLICT,
            "Start the container before using the gamepad".into(),
        ));
    }
    let container = info["Id"].as_str().context("Missing container ID")?;
    let mut status = control(app, container, "status", None).await?;
    if status["state"] != "running" {
        app.docker(&[
            "exec", "--detach", "--user", "root", container, "python3", "-c", DAEMON, DIRECTORY,
        ])
        .await?;
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            status = control(app, container, "status", None).await?;
            if status["state"] == "running" {
                break;
            }
            if status["state"] == "failed" {
                return Err(anyhow::anyhow!(
                    "Gamepad device could not be created: {}",
                    status["error"].as_str().unwrap_or("unknown error")
                )
                .into());
            }
        }
        if status["state"] != "running" {
            return Err(anyhow::anyhow!(
                "Gamepad device did not start; check that /dev/uinput is available"
            )
            .into());
        }
    }
    let payload = serde_json::to_vec(&body.payload()).map_err(|e| anyhow::anyhow!(e))?;
    let mut result = control(app, container, "send", Some(payload)).await?;
    result["container_id"] = json!(id);
    Ok(result)
}
pub async fn input(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<GamepadInput>,
) -> Result<Json<Value>> {
    Ok(Json(gamepad_input(&app, &id, &body).await?))
}
