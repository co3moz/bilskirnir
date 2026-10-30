use crate::{App, docker::logged::Ran, error::Result, jobs::launch_shared};
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
const WRAPPER: &str = "script=$(mktemp /tmp/thoric-exec.XXXXXX) && cat >\"$script\" && exec 3<\"$script\" && rm -f -- \"$script\" && exec bash -e /dev/fd/3 </dev/null";

/// `docker exec` arguments up to the container, for a script that reads its input from the
/// manager and runs in the desktop session's environment.
pub(crate) fn desktop_exec(user: &str, root: bool) -> Vec<String> {
    [
        "exec",
        "--interactive",
        "--user",
        user,
        "--workdir",
        "/home/desktop",
        "--env",
        if root {
            "HOME=/root"
        } else {
            "HOME=/home/desktop"
        },
        "--env",
        "DISPLAY=:0",
        "--env",
        "PULSE_SERVER=unix:/run/desktop/pulse/native",
        "--env",
        "XDG_RUNTIME_DIR=/run/desktop",
    ]
    .map(str::to_owned)
    .to_vec()
}

#[derive(Deserialize, schemars::JsonSchema)]
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
            bail!("Script must contain 1 to 1000000 bytes without NUL characters");
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
    app.container(&id)?;
    launch_shared(
        app,
        "exec",
        format!("container:{id}"),
        move |app, job_id| async move {
            let env = app.container(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            let info = app.owned(&env).await?;
            if info["State"]["Running"] != true {
                bail!("Start the container before executing a script");
            }
            let user = if body.root { "root" } else { "1001:1001" };
            app.append_log(
                &job_id,
                &format!(
                    "Running a script in \"{}\" as {} (timeout: {} seconds).",
                    env.spec.name, user, body.timeout_seconds
                ),
            )?;
            let mut args = desktop_exec(user, body.root);
            args.extend([
                info["Id"].as_str().context("Missing container ID")?.into(),
                "timeout".into(),
                "--signal=TERM".into(),
                "--kill-after=5s".into(),
                format!("{}s", body.timeout_seconds),
                "bash".into(),
                "-c".into(),
                WRAPPER.into(),
            ]);
            let started = std::time::Instant::now();
            let ran = app
                .docker_logged_outcome(&job_id, &args, body.script.into_bytes())
                .await?;
            let limit = std::time::Duration::from_secs(body.timeout_seconds);
            let outcome = Outcome::of(&ran, started.elapsed() >= limit);
            let failure = outcome.failure(body.timeout_seconds);
            let result = json!({
                "container_id": id,
                "user": user,
                "exit_code": outcome.exit_code,
                "timed_out": outcome.timed_out,
                "output_tail": outcome.output_tail,
            });
            match failure {
                None => {
                    app.append_log(&job_id, "Script completed successfully.")?;
                    Ok(result)
                }
                Some(reason) => {
                    app.append_log(&job_id, &format!("{reason}."))?;
                    Err(crate::jobs::Failed::new(reason, result).into())
                }
            }
        },
    )
}

/// How a script ended, which its job's result carries whether it succeeded or failed.
#[derive(Debug, PartialEq)]
pub(crate) struct Outcome {
    /// The script's exit code, `0` when it succeeded; `null` when it ran out of the manager's own
    /// time or a signal ended it.
    pub exit_code: Option<i32>,
    /// Whether it ran out of `timeout_seconds` and was stopped.
    pub timed_out: bool,
    /// The end of what it printed, at most [`crate::docker::logged::TAIL_LIMIT`] bytes, without the manager's lines.
    pub output_tail: String,
}

impl Outcome {
    /// `ran` of a script whose time was up when it ended (`over`). `timeout` reports a script it
    /// stopped as 124, or as 137 when the script outlived the TERM by the five seconds it is
    /// given; a script that exits with either code before its time is not taken for one.
    pub(crate) fn of(ran: &Ran, over: bool) -> Self {
        let exit_code = ran.status.and_then(|status| status.code());
        Self {
            exit_code,
            timed_out: ran.status.is_none() || (over && matches!(exit_code, Some(124 | 137))),
            output_tail: ran.tail.clone(),
        }
    }
    /// Why the script failed, for its job's error and log, or `None` when it succeeded.
    fn failure(&self, timeout_seconds: u64) -> Option<String> {
        if self.timed_out {
            let code = self
                .exit_code
                .map(|code| format!(" (exit code {code})"))
                .unwrap_or_default();
            return Some(format!(
                "Script ran out of its time limit of {timeout_seconds} seconds and was stopped{code}"
            ));
        }
        match self.exit_code {
            Some(0) => None,
            Some(code) => Some(format!("Script exited with code {code}")),
            None => Some("Script was ended by a signal".into()),
        }
    }
}

#[cfg(all(test, unix))]
#[path = "exec_tests.rs"]
mod tests;
