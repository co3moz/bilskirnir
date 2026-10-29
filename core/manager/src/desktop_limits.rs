//! What keeps one desktop from exhausting the Docker host it shares with the other desktops and the
//! manager: a bound on its processes and threads, and a log that rotates.
//!
//! Both are `docker create` arguments ([`docker_args`]). Docker fixes a container's log options when
//! it creates the container, so a desktop created before them keeps the log it had until a reset or
//! a settings change recreates it. The pids limit changes on a running container, so the manager
//! gives it to every desktop as it checks them ([`App::keep_pids_limit`]), also to one created
//! before the limit or before `DESKTOP_PIDS_LIMIT` changed.
use crate::{App, config::Config};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::time::Duration;

/// How many processes and threads a desktop may have unless `DESKTOP_PIDS_LIMIT` says otherwise.
/// Threads count against the limit, and a browser with many tabs, an IDE with its language servers
/// or the Unity editor each run hundreds to thousands of them, so it leaves room for several at
/// once; a fork bomb still stops here instead of at the host's own limit, which every other
/// desktop and the manager share.
pub(crate) const DEFAULT_PIDS_LIMIT: u64 = 16384;
/// The smallest limit the setting accepts: a desktop's own session and one browser already take
/// several hundred, and a smaller limit leaves desktops whose programs fail to start.
pub(crate) const MIN_PIDS_LIMIT: u64 = 1024;
/// The kernel's own largest process ID on 64-bit systems.
pub(crate) const MAX_PIDS_LIMIT: u64 = 4_194_304;

/// Docker's default logging driver, which keeps everything a container writes in one file that
/// grows for as long as the container exists unless it is given a size.
const JSON_FILE: &str = "json-file";
/// A desktop's json-file options: at most three files of 10 MB, the values Docker's own
/// documentation gives as its example. **Logs** on the dashboard and `container_logs` read the
/// last 300 lines, a small part of that, and `docker logs` reads the rotated files as well.
pub(crate) const LOG_OPTIONS: [&str; 2] = ["max-size=10m", "max-file=3"];

/// `DESKTOP_PIDS_LIMIT`: how many processes and threads a desktop may have, or `0` for no limit.
pub(crate) fn pids_limit(value: Option<&str>) -> Result<u64> {
    let refused = || {
        format!(
            "DESKTOP_PIDS_LIMIT must be 0 for no limit or a whole number from {MIN_PIDS_LIMIT} to {MAX_PIDS_LIMIT}"
        )
    };
    let limit = match value {
        None => DEFAULT_PIDS_LIMIT,
        Some(text) => text.parse::<u64>().with_context(refused)?,
    };
    if limit != 0 && !(MIN_PIDS_LIMIT..=MAX_PIDS_LIMIT).contains(&limit) {
        bail!(refused());
    }
    Ok(limit)
}

/// The `docker create` arguments that bound a desktop: its pids limit, and its log options while
/// Docker logs through json-file. Another driver is the administrator's choice and stays as it is
/// set up: `local` rotates by itself, and the others, such as journald, refuse these options.
pub(crate) fn docker_args(config: &Config) -> Vec<String> {
    let mut args = Vec::new();
    if config.pids_limit > 0 {
        args.extend(["--pids-limit".into(), config.pids_limit.to_string()]);
    }
    if config.log_driver.as_deref() == Some(JSON_FILE) {
        for option in LOG_OPTIONS {
            args.extend(["--log-opt".into(), option.into()]);
        }
    }
    args
}

/// The `docker update` arguments that give a container, as `info` inspects it, the pids limit
/// `limit`, `0` for none, or `None` when it has that limit already. Docker reports no limit as
/// `null`, or as `0` or `-1` in older releases, and removes one with `-1`.
pub(crate) fn pids_update(info: &Value, limit: u64) -> Option<Vec<String>> {
    let current = info["HostConfig"]["PidsLimit"]
        .as_i64()
        .filter(|limit| *limit > 0);
    let wanted = i64::try_from(limit).ok().filter(|limit| *limit > 0);
    if current == wanted {
        return None;
    }
    Some(vec![
        "update".into(),
        "--pids-limit".into(),
        wanted.unwrap_or(-1).to_string(),
        info["Id"].as_str()?.into(),
    ])
}

/// The logging driver Docker gives a container that names none, from `docker info`.
pub(crate) fn log_driver(info: Option<&Value>) -> Option<String> {
    info?["LoggingDriver"]
        .as_str()
        .filter(|driver| !driver.is_empty())
        .map(str::to_owned)
}

/// Say how desktops' logs are kept, once when the manager starts.
pub(crate) fn log_logging(config: &Config) {
    match config.log_driver.as_deref() {
        Some(JSON_FILE) => tracing::info!(
            "Desktops log through json-file with {}",
            LOG_OPTIONS.join(" and ")
        ),
        Some(driver) => tracing::info!(
            "Desktops log through Docker's {driver} driver, which keeps their logs as it is set up to"
        ),
        None => tracing::warn!(
            "Docker did not name its logging driver, so desktops get its default log settings; json-file without max-size never rotates"
        ),
    }
}

/// `docker info`, which the manager reads once when it starts: the GPU routes look for the NVIDIA
/// Container Toolkit in it ([`crate::gpu::Host::detect`]) and desktops' log options follow its
/// logging driver ([`log_driver`]). `None` when Docker does not answer.
pub(crate) async fn docker_info(docker_bin: &str) -> Option<Value> {
    let mut command = tokio::process::Command::new(docker_bin);
    command.args(["info", "--format", "{{json .}}"]);
    let out = crate::process::capture(
        command,
        Duration::from_secs(20),
        4 * 1024 * 1024,
        crate::process::Overflow::Reject,
    )
    .await
    .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

impl App {
    /// Give a desktop's container, as `info` inspects it, the installation's pids limit in place,
    /// without restarting it.
    pub(crate) async fn keep_pids_limit(&self, info: &Value) -> Result<()> {
        if let Some(args) = pids_update(info, self.config.pids_limit) {
            self.docker_args(&args).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "desktop_limits_tests.rs"]
mod tests;
