//! Keeping each running desktop's proxy in line with its container's settings. Every ten seconds,
//! and at once when a desktop starts or its proxy setting changes, the manager asks the proxy of
//! each running desktop with a proxy which settings it has, starts it as nobody when it is not
//! running, and gives it the settings when their version differs. The settings travel on stdin to
//! a root helper and from there over a socket only root reaches, so the password is never on a
//! command line or in a file. A desktop that started while the manager was away has a proxy that
//! answers 503 until the manager is back.
use super::{PORT, Settings};
use crate::{App, process};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

const DAEMON: &str = include_str!("../../helpers/outbound_proxy.py");
const CONTROL: &str = include_str!("../../helpers/outbound_control.py");
/// The proxy's folder, on the desktop's tmpfs, which no commit keeps.
const DIRECTORY: &str = "/run/thoric-proxy";
/// The user the proxy runs as, who cannot read the desktop user's files nor they its memory.
const NOBODY: &str = "65534:65534";
const EVERY: Duration = Duration::from_secs(10);

pub(crate) fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                () = app.outbound.wake.notified() => {}
            }
            if let Err(error) = supervise(&app).await {
                tracing::debug!("Desktop proxies: {error:#}");
            }
        }
    });
}

/// Bring the proxy of every running desktop with a proxy in line.
async fn supervise(app: &Arc<App>) -> Result<()> {
    let output = app
        .docker(&[
            "ps",
            "--filter",
            &format!("label={}=true", crate::model::LABEL),
            "--format",
            "{{.ID}} {{.Label \"thoric.environment\"}}",
        ])
        .await?;
    let records = app
        .db
        .list::<crate::model::Container>(crate::model::CONTAINERS)?;
    for line in String::from_utf8_lossy(&output).lines() {
        let Some((docker_id, record_id)) = line.split_once(' ') else {
            continue;
        };
        let Ok(container) = app.container(record_id) else {
            continue;
        };
        let password = app.proxy_password(record_id);
        let peers = crate::groups::peers(record_id, &container.spec, &records);
        let Some(settings) = super::settings(&app.config, &container.spec, password, peers) else {
            continue;
        };
        let result = ensure(app, docker_id, &settings).await;
        let mut failures = app.outbound.failures.lock().unwrap();
        match result {
            Ok(()) => {
                failures.remove(record_id);
            }
            Err(error) => {
                let error = format!("{error:#}");
                if failures.get(record_id) != Some(&error) {
                    tracing::warn!(
                        "The proxy of \"{}\" is not ready: {error}",
                        container.spec.name
                    );
                    failures.insert(record_id.to_owned(), error);
                }
            }
        }
    }
    Ok(())
}

/// Start the desktop's proxy when it is not running and give it the settings it lacks.
async fn ensure(app: &App, container: &str, settings: &Settings) -> Result<()> {
    let mut status = control(app, container, "status", None).await?;
    if status["state"] != "running" {
        app.docker(&[
            "exec",
            "--detach",
            "--user",
            NOBODY,
            container,
            "python3",
            "-c",
            DAEMON,
            DIRECTORY,
            &PORT.to_string(),
        ])
        .await?;
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            status = control(app, container, "status", None).await?;
            if status["state"] == "running" {
                break;
            }
            if status["state"] == "failed" {
                bail!(
                    "{}",
                    status["error"]
                        .as_str()
                        .unwrap_or("the proxy did not start")
                );
            }
        }
        if status["state"] != "running" {
            bail!("the proxy did not start");
        }
    }
    if status["version"] != settings.version.as_str() {
        let input = serde_json::to_vec(settings)?;
        let reply = control(app, container, "apply", Some(input)).await?;
        if reply["version"] != settings.version.as_str() {
            bail!("the proxy did not take its settings");
        }
    }
    Ok(())
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
        Duration::from_secs(20),
        65536,
        process::Overflow::Reject,
        input,
    )
    .await?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    serde_json::from_slice(&output.stdout).context("The desktop's proxy gave no answer")
}
