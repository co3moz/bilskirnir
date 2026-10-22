//! Newer releases of Bilskirnir, and the update the dashboard asks the WSL package to run.
//!
//! Unless `UPDATE_CHECK=false`, the manager asks GitHub for the newest release when it starts and
//! every six hours after, with the `curl` its image carries for the health check, and at once when
//! the settings page asks. The dashboard offers a release that is newer than this manager.
//!
//! Only the WSL package updates from the dashboard, with the same `bilskirnir update` a terminal
//! runs: it moves `MANAGER_IMAGE` in `/opt/bilskirnir/.env` to the release, brings that release's
//! installation files and command, and recreates this container. None of that is reachable from
//! inside the container, and the manager never reaches into the distribution itself. Instead the
//! package mounts `/opt/bilskirnir/update` here at [`UPDATE_DIR`], with a [`WATCHED`] file in it.
//! The manager leaves a [`REQUEST`] naming its job there, and `bilskirnir-update.path` in the
//! distribution starts `bilskirnir-update.service`, which renames the request to [`STARTED`], runs
//! the command as root with its output in [`LOG`] and writes how it ended to [`EXIT`]. The job
//! follows that log. The command replaces this container on the way, so the next manager finds
//! the job still running and follows it to its end; see [`resumable`].
use crate::{
    App,
    error::{Error, Result},
    jobs,
    model::{Job, now},
    process::TextDecoder,
};
use anyhow::{Context, bail};
use axum::{Json, extract::State, http::StatusCode};
use serde_json::{Value, json};
use std::{
    io::{ErrorKind, SeekFrom},
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// This manager's release.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The newest published release. Drafts and prereleases are left out, as `bilskirnir update`
/// leaves them out.
const RELEASES: &str = "https://api.github.com/repos/co3moz/bilskirnir/releases/latest";
/// Where a release's notes are, followed by its version.
const RELEASE_PAGE: &str = "https://github.com/co3moz/bilskirnir/releases/tag/v";
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// curl gives up within its own limits below; the process is stopped here in any case.
const CHECK_LIMIT: Duration = Duration::from_secs(30);

/// The package's `/opt/bilskirnir/update` in the manager container.
pub(crate) const UPDATE_DIR: &str = "/update";
/// Put there by the package and by `bilskirnir update` once the path unit watches the folder.
pub(crate) const WATCHED: &str = "watched";
pub(crate) const REQUEST: &str = "request";
pub(crate) const STARTED: &str = "started";
pub(crate) const LOG: &str = "log";
pub(crate) const EXIT: &str = "exit";
/// The kind of the job that follows an update.
pub(crate) const JOB: &str = "update";

/// How long an update job waits for each thing it expects.
#[derive(Clone, Copy)]
pub(crate) struct Timing {
    /// The distribution taking the request, which its path unit does at once.
    pub take: Duration,
    /// Between two reads of the log.
    pub poll: Duration,
    /// The whole update. The service's own limit is two hours; a little more covers reading
    /// what it wrote last.
    pub limit: Duration,
}
pub(crate) const TIMING: Timing = Timing {
    take: Duration::from_secs(30),
    poll: Duration::from_secs(1),
    limit: Duration::from_secs(2 * 3600 + 300),
};
/// How much of the log one read takes; the rest comes with the next.
const READ_LIMIT: u64 = 1 << 20;

/// What the last look at GitHub found.
#[derive(Default)]
pub(crate) struct Releases {
    last: Mutex<Check>,
    /// Held while GitHub is asked, so one look runs at a time.
    asking: tokio::sync::Mutex<()>,
}
#[derive(Clone, Default)]
struct Check {
    /// The newest release seen, kept when a later look fails.
    newest: Option<String>,
    checked_at: Option<u64>,
    error: Option<String>,
    /// How many looks have ended.
    looks: u64,
}

impl Releases {
    /// Ask GitHub with `ask`, unless a look that ended while this one waited for its turn already
    /// did: a second click, or the six-hourly look, answers for this one too.
    pub(crate) async fn check(&self, ask: impl Future<Output = anyhow::Result<String>>) {
        let before = self.last.lock().unwrap().looks;
        let _turn = self.asking.lock().await;
        if self.last.lock().unwrap().looks == before {
            self.record(ask.await);
        }
    }
    fn record(&self, result: anyhow::Result<String>) {
        let mut check = self.last.lock().unwrap();
        check.looks += 1;
        check.checked_at = Some(now());
        match result {
            Ok(version) => {
                if newer(&version, VERSION) && check.newest.as_deref() != Some(version.as_str()) {
                    tracing::info!("Bilskirnir {version} is available; this manager is {VERSION}");
                }
                check.newest = Some(version);
                check.error = None;
            }
            Err(error) => {
                tracing::debug!("Release check: {error:#}");
                check.error = Some(format!("{error:#}"));
            }
        }
    }
    /// The newest release when it is newer than this manager.
    fn available(&self) -> Option<String> {
        let check = self.last.lock().unwrap();
        check.newest.clone().filter(|newest| newer(newest, VERSION))
    }
}

/// Look for new releases for as long as the manager runs, unless the installation turned it off.
pub(crate) fn spawn(app: Arc<App>) {
    if !app.config.update_check {
        return;
    }
    tokio::spawn(async move {
        loop {
            app.releases.check(newest_release()).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

async fn newest_release() -> anyhow::Result<String> {
    let mut command = tokio::process::Command::new("curl");
    command
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--max-time",
            "20",
            "--max-filesize",
            "1048576",
            "--header",
            "Accept: application/vnd.github+json",
            RELEASES,
        ])
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(CHECK_LIMIT, command.output())
        .await
        .context("GitHub did not answer in time")?
        .context("Could not run curl")?;
    if !output.status.success() {
        bail!(
            "Could not read the newest release from GitHub: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    newest(&output.stdout)
}

/// The version of the release GitHub describes, without the v of its tag.
pub(crate) fn newest(body: &[u8]) -> anyhow::Result<String> {
    let release: Value =
        serde_json::from_slice(body).context("GitHub did not answer with a release")?;
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = tag.strip_prefix('v').unwrap_or(tag);
    if parse(version).is_none() {
        bail!("The newest release has a tag that is not a version: {tag:.40}");
    }
    Ok(version.into())
}

fn parse(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.split('.').map(|part| {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse::<u64>().ok()
    });
    let numbers = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(numbers)
}

/// Whether `candidate` is a later release than `current`. Anything other than
/// `major.minor.patch` is never newer, nor older.
pub(crate) fn newer(candidate: &str, current: &str) -> bool {
    matches!((parse(candidate), parse(current)), (Some(candidate), Some(current)) if candidate > current)
}

/// Whether the WSL distribution takes update requests: its package mounts the folder with
/// [`WATCHED`] in it. Elsewhere the folder is missing or empty.
pub(crate) fn installable(dir: &Path) -> bool {
    dir.join(WATCHED).is_file()
}

fn active(job: &Job) -> bool {
    job.status == "queued" || job.status == "running"
}

/// `GET /api/update`: this release, the newest one and whether the dashboard can update to it.
pub async fn status(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let check = app.releases.last.lock().unwrap().clone();
    let running = app
        .db
        .active_jobs()?
        .into_iter()
        .find(|job| job.kind == JOB);
    Ok(Json(json!({
        "enabled": app.config.update_check,
        "current": VERSION,
        "newest": check.newest,
        "available": check.newest.as_deref().is_some_and(|newest| newer(newest, VERSION)),
        "release_url": check.newest.as_ref().map(|version| format!("{RELEASE_PAGE}{version}")),
        "checked_at": check.checked_at,
        "error": check.error,
        "installable": app.config.update_check && installable(&app.config.update_dir),
        "job_id": running.map(|job| job.id),
    })))
}

/// `POST /api/update/check`: ask GitHub now rather than at the next six-hourly look, and answer
/// as `GET /api/update` does once it has answered.
pub async fn check(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    if !app.config.update_check {
        return Err(Error(
            StatusCode::CONFLICT,
            "Release checks are turned off with UPDATE_CHECK=false".into(),
        ));
    }
    app.releases.check(newest_release()).await;
    status(State(app)).await
}

/// `POST /api/update`: ask the WSL distribution to run `bilskirnir update`; returns a job.
pub async fn start(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let conflict = |message: &str| Error(StatusCode::CONFLICT, message.into());
    if !app.config.update_check {
        return Err(conflict(
            "Release checks are turned off with UPDATE_CHECK=false",
        ));
    }
    if !installable(&app.config.update_dir) {
        return Err(conflict(
            "Only the WSL package updates from the dashboard; update this installation the way it was installed",
        ));
    }
    if app.releases.available().is_none() {
        return Err(conflict("This manager is the newest release"));
    }
    // Every job of this manager ends with it, so none may be running.
    if !app.db.active_jobs()?.is_empty() {
        return Err(conflict(
            "The update restarts the manager; start it once the running jobs have finished",
        ));
    }
    jobs::launch(app, JOB, JOB.into(), |app, id| async move {
        run(&app, &id, TIMING).await
    })
}

/// Leave the request, wait for the distribution to take it and follow the update.
pub(crate) async fn run(app: &App, id: &str, timing: Timing) -> anyhow::Result<Value> {
    let dir = &app.config.update_dir;
    // What the last update left; the request alone names this job.
    for name in [REQUEST, STARTED, LOG, EXIT] {
        remove(&dir.join(name)).await?;
    }
    // Renamed into place, so the path unit never finds half a request.
    let partial = dir.join(format!("{REQUEST}.new"));
    tokio::fs::write(&partial, format!("{id}\n"))
        .await
        .with_context(|| format!("Could not write {}", partial.display()))?;
    tokio::fs::rename(&partial, dir.join(REQUEST)).await?;
    let target = app
        .releases
        .available()
        .unwrap_or_else(|| "the newest release".into());
    app.append_log(
        id,
        &format!("Asked the WSL distribution to run bilskirnir update, which moves this installation to {target}."),
    )?;
    let taken = async {
        loop {
            if read_text(&dir.join(STARTED)).await?.as_deref() == Some(id) {
                return anyhow::Ok(());
            }
            tokio::time::sleep(timing.poll / 4).await;
        }
    };
    if let Ok(result) = tokio::time::timeout(timing.take, taken).await {
        result?;
    } else {
        // A request taken later would start an update that nobody follows.
        remove(&dir.join(REQUEST)).await?;
        if read_text(&dir.join(STARTED)).await?.as_deref() != Some(id) {
            bail!(
                "The WSL distribution did not take the request. Check 'systemctl status bilskirnir-update.path' there, or run 'bilskirnir update' in a Bilskirnir terminal"
            );
        }
    }
    follow(app, id, timing).await
}

/// Whether an update job outlived the manager that started it: the distribution took its
/// request, so `bilskirnir update` replaced that manager and goes on, or ended while no manager
/// was there to see it.
pub(crate) fn resumable(dir: &Path, job: &Job) -> bool {
    job.kind == JOB
        && active(job)
        && std::fs::read_to_string(dir.join(STARTED)).is_ok_and(|started| started.trim() == job.id)
}

/// Follow an update the previous manager started, to its end.
pub(crate) fn resume(app: Arc<App>, job: Job) {
    tokio::spawn(async move {
        let id = job.id.clone();
        let result = async {
            // The log is read again from its start.
            let mut record: Job = app.db.get("job", &id)?;
            record.log = "The manager restarted while bilskirnir update ran in the WSL distribution; its output follows.\n".into();
            app.db.put("job", &id, &record)?;
            follow(&app, &id, TIMING).await
        }
        .await;
        let _ = jobs::finish(&app, &id, job, result);
    });
}

async fn follow(app: &App, id: &str, timing: Timing) -> anyhow::Result<Value> {
    let dir = &app.config.update_dir;
    let mut log = LogReader::default();
    let mut waited = Duration::ZERO;
    loop {
        // The service writes how it ended after its last output, so the log is whole by then.
        let ended = read_text(&dir.join(EXIT)).await?;
        let mut read = log.read(app, id, &dir.join(LOG)).await?;
        if let Some(ended) = ended {
            while read > 0 {
                read = log.read(app, id, &dir.join(LOG)).await?;
            }
            log.finish(app, id)?;
            return outcome(&ended);
        }
        if waited >= timing.limit {
            bail!("bilskirnir update did not end within two hours");
        }
        tokio::time::sleep(timing.poll).await;
        waited += timing.poll;
    }
}

/// The job's result from what the service wrote when it ended: systemd's result for the
/// service and the command's exit status.
pub(crate) fn outcome(ended: &str) -> anyhow::Result<Value> {
    let ended = ended.trim();
    if ended.split_whitespace().next() == Some("success") {
        return Ok(json!({"version": VERSION}));
    }
    bail!("bilskirnir update did not succeed in the WSL distribution ({ended:.80})")
}

/// The command's output as it grows, added to the job a whole line at a time.
#[derive(Default)]
struct LogReader {
    offset: u64,
    decoder: TextDecoder,
    partial: String,
}

impl LogReader {
    /// Add what was written since the last read; returns how many bytes that was.
    async fn read(&mut self, app: &App, id: &str, path: &Path) -> anyhow::Result<u64> {
        let mut file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            // The service has not started the command yet.
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error).context("Could not read the update's log"),
        };
        file.seek(SeekFrom::Start(self.offset)).await?;
        let mut bytes = Vec::new();
        file.take(READ_LIMIT).read_to_end(&mut bytes).await?;
        self.offset += bytes.len() as u64;
        self.partial.push_str(&self.decoder.push(&bytes));
        if let Some(end) = self.partial.rfind('\n') {
            let lines: String = self.partial.drain(..=end).collect();
            app.append_log(id, lines.trim_end_matches('\n'))?;
        }
        Ok(bytes.len() as u64)
    }
    fn finish(&mut self, app: &App, id: &str) -> anyhow::Result<()> {
        self.partial.push_str(&self.decoder.finish());
        if !self.partial.is_empty() {
            app.append_log(id, &std::mem::take(&mut self.partial))?;
        }
        Ok(())
    }
}

async fn read_text(path: &Path) -> anyhow::Result<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(text) => Ok(Some(text.trim().to_owned())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Could not read {}", path.display())),
    }
}

async fn remove(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Err(error) if error.kind() != ErrorKind::NotFound => {
            Err(error).with_context(|| format!("Could not remove {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "releases_tests.rs"]
mod tests;
