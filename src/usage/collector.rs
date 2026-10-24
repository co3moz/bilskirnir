//! The loop that follows every running container this manager owns, one stats stream each, and
//! the host's GPU while a GPU desktop runs. It holds no lock an HTTP handler waits for: it reads
//! the records like any handler and takes the usage store only to add a sample.
use super::{
    engine::{self, Abort, Running},
    gpu,
    history::GpuState,
    stats,
};
use crate::{
    App,
    model::{CONTAINERS, Container},
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// How often the running containers are listed; a container that starts shows up within this.
const DISCOVERY: Duration = Duration::from_secs(2);
/// Docker sends stats every second, so a stream this quiet is closed and opened again.
const STATS_IDLE: Duration = Duration::from_secs(30);
const LIST_LIMIT: Duration = Duration::from_secs(10);
/// How long after a failed attempt the GPU is tried again.
const GPU_RETRY: Duration = Duration::from_secs(300);

pub(crate) fn spawn(app: Arc<App>) {
    tokio::spawn(run(app, DISCOVERY));
}

/// A container's stats stream, by the Docker ID it was opened for.
struct Followed {
    docker_id: String,
    task: Abort,
}

async fn run(app: Arc<App>, every: Duration) {
    let Some(socket) = app.config.docker_socket.clone() else {
        app.usage.set_error(Some(
            "Usage needs the Docker Engine socket, and DOCKER_HOST does not name a Unix socket"
                .into(),
        ));
        return;
    };
    let mut followed: HashMap<String, Followed> = HashMap::new();
    let mut gpu = GpuSampler::default();
    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let records: Vec<Container> = match app.db.list(CONTAINERS) {
            Ok(records) => records,
            Err(error) => {
                tracing::debug!("Usage: container records: {error:#}");
                continue;
            }
        };
        match list(&socket).await {
            Ok(listed) => {
                app.usage.set_error(None);
                let running = engine::running(&records, &listed);
                // A stream that ended, such as on a restart, is opened again if still wanted.
                followed.retain(|_, stream| !stream.task.0.is_finished());
                let current = followed
                    .iter()
                    .map(|(id, stream)| (id.clone(), stream.docker_id.clone()))
                    .collect();
                let (stop, start) = plan(&current, &running);
                for id in stop {
                    followed.remove(&id);
                }
                for (id, docker_id) in start {
                    let task = tokio::spawn(follow(
                        app.clone(),
                        socket.clone(),
                        id.clone(),
                        docker_id.clone(),
                    ));
                    let task = Abort(task);
                    followed.insert(id, Followed { docker_id, task });
                }
                gpu.update(&app, running.values().any(|container| container.gpu));
            }
            // Streams on a dropped socket end by themselves and are opened again once it is back.
            Err(error) => app.usage.set_error(Some(format!("{error:#}"))),
        }
        app.usage.prune(&records);
    }
}

/// The streams to close and to open so that exactly the running containers are followed: one
/// whose container was recreated under a new Docker ID is opened again.
pub(crate) fn plan(
    followed: &BTreeMap<String, String>,
    running: &BTreeMap<String, Running>,
) -> (Vec<String>, Vec<(String, String)>) {
    let stop = followed
        .iter()
        .filter(|(id, docker_id)| running.get(*id).is_none_or(|r| &r.docker_id != *docker_id))
        .map(|(id, _)| id.clone())
        .collect();
    let start = running
        .iter()
        .filter(|(id, r)| followed.get(*id) != Some(&r.docker_id))
        .map(|(id, r)| (id.clone(), r.docker_id.clone()))
        .collect();
    (stop, start)
}

async fn list(socket: &Path) -> Result<Value> {
    let io = crate::pulls::connect(socket)
        .await
        .with_context(|| format!("Docker Engine socket {}", socket.display()))?;
    tokio::time::timeout(LIST_LIMIT, engine::get(io, &engine::list_path()))
        .await
        .context("Docker did not list the running containers in time")?
}

/// Store a sample for each second Docker reports until the stream ends or the task is stopped.
async fn follow(app: Arc<App>, socket: PathBuf, id: String, docker_id: String) {
    let result = async {
        let path = engine::stats_path(&docker_id).context("Invalid Docker container ID")?;
        let io = crate::pulls::connect(&socket).await?;
        let mut previous: Option<stats::Reading> = None;
        engine::stream(io, &path, STATS_IDLE, |line| {
            let current = serde_json::from_slice::<Value>(line)
                .ok()
                .as_ref()
                .and_then(stats::reading);
            if let (Some(before), Some(now)) = (&previous, &current)
                && let Some(measure) = stats::measure(before, now)
            {
                app.usage.record(&id, &measure);
            }
            previous = current;
        })
        .await
    }
    .await;
    if let Err(error) = result {
        tracing::debug!("Usage of container {id}: {error:#}");
    }
}

/// Runs the GPU sampler while a GPU desktop runs, and backs off after it fails.
#[derive(Default)]
struct GpuSampler {
    task: Option<Abort>,
    retry_at: Option<Instant>,
}

impl GpuSampler {
    fn update(&mut self, app: &Arc<App>, wanted: bool) {
        if !app.config.gpu_enabled() {
            app.usage.set_gpu(GpuState::Disabled, None);
            return;
        }
        if !wanted {
            self.task = None;
            self.retry_at = None;
            app.usage.set_gpu(GpuState::Idle, None);
            return;
        }
        match &self.task {
            Some(task) if !task.0.is_finished() => return,
            // It gave up and recorded why.
            Some(_) => {
                self.task = None;
                self.retry_at = Some(Instant::now() + GPU_RETRY);
            }
            None => {}
        }
        if self.retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        let app = app.clone();
        self.task = Some(Abort(tokio::spawn(async move {
            let reason = gpu::sample(app.clone()).await;
            app.usage.set_gpu(GpuState::Unavailable, Some(reason));
        })));
    }
}

#[cfg(test)]
#[path = "collector_tests.rs"]
mod tests;
