use crate::{App, error::Result, jobs::launch};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

// The script is copied to an unlinked temporary file first, so commands that read stdin see
// end-of-file instead of consuming the rest of the script, and large scripts avoid argument limits.
const WRAPPER: &str = "script=$(mktemp /tmp/bilskirnir-exec.XXXXXX) && cat >\"$script\" && exec 3<\"$script\" && rm -f -- \"$script\" && exec bash -e /dev/fd/3 </dev/null";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Execute {
    pub script: String,
    #[serde(default)]
    pub root: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}
fn default_timeout() -> u64 {
    300
}
impl Execute {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.script.trim().is_empty()
            || self.script.len() > 1_000_000
            || self.script.contains('\0')
        {
            bail!("Script must contain 1–1000000 bytes without NUL characters");
        }
        if !(1..=3500).contains(&self.timeout_seconds) {
            bail!("timeout_seconds must be between 1 and 3500");
        }
        Ok(())
    }
}
pub async fn execute(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Execute>,
) -> Result<Json<Value>> {
    body.validate()?;
    app.environment(&id)?;
    launch(
        app,
        "exec",
        format!("environment:{id}"),
        move |app, job_id| async move {
            let env = app.environment(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            let info = app.owned(&env).await?;
            if info["State"]["Running"] != true {
                bail!("Start the environment before executing a script");
            }
            let user = if body.root { "root" } else { "1001:1001" };
            app.append_log(
                &job_id,
                &format!(
                    "Running a script in \"{}\" as {} (timeout: {} seconds).",
                    env.spec.name, user, body.timeout_seconds
                ),
            )?;
            let args = vec![
                "exec".into(),
                "--interactive".into(),
                "--user".into(),
                user.into(),
                "--workdir".into(),
                "/home/desktop".into(),
                "--env".into(),
                format!("HOME={}", if body.root { "/root" } else { "/home/desktop" }),
                "--env".into(),
                "DISPLAY=:0".into(),
                "--env".into(),
                "PULSE_SERVER=unix:/run/desktop/pulse/native".into(),
                "--env".into(),
                "XDG_RUNTIME_DIR=/run/desktop".into(),
                info["Id"].as_str().context("Missing container ID")?.into(),
                "timeout".into(),
                "--signal=TERM".into(),
                "--kill-after=5s".into(),
                format!("{}s", body.timeout_seconds),
                "bash".into(),
                "-c".into(),
                WRAPPER.into(),
            ];
            app.docker_logged_input(&job_id, &args, Some(body.script.into_bytes()))
                .await?;
            app.append_log(&job_id, "Script completed successfully.")?;
            Ok(json!({"environment_id": id, "user": user, "exit_code": 0}))
        },
    )
}
