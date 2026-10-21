use crate::{
    App,
    error::Result,
    model::{Job, now},
};
use axum::Json;
use futures_util::FutureExt;
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::{Arc, Weak},
};

/// A job's result for a caller that waits for it: the value it returned, or its error message.
pub(crate) type Outcome = tokio::sync::oneshot::Receiver<std::result::Result<Value, String>>;

/// How many jobs are kept once finished, newest first; the dashboard lists the latest 100.
pub(crate) const KEPT_JOBS: usize = 500;

impl App {
    pub fn resource_lock(&self, resource: &str) -> Arc<tokio::sync::Mutex<()>> {
        if resource == "global" {
            return self.operations.clone();
        }
        let mut locks = self.resource_locks.lock().unwrap();
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(resource).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(resource.into(), Arc::downgrade(&lock));
        lock
    }
}
pub fn launch<F, Fut>(app: Arc<App>, kind: &str, resource: String, work: F) -> Result<Json<Value>>
where
    F: FnOnce(Arc<App>, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<Value>> + Send + 'static,
{
    launch_named(app, kind, resource, None, work)
}

/// [`launch`] for a job that makes a container or an image called `name`, which the job carries
/// so the lists can show what is on its way before its record exists.
pub fn launch_named<F, Fut>(
    app: Arc<App>,
    kind: &str,
    resource: String,
    name: Option<String>,
    work: F,
) -> Result<Json<Value>>
where
    F: FnOnce(Arc<App>, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<Value>> + Send + 'static,
{
    let (id, _) = start_named(app, kind, resource, name, true, work)?;
    Ok(Json(json!({"job_id":id})))
}

/// Queue a job and return its ID with its eventual outcome. An `exclusive` job first takes its
/// resource's lock; one that only reads the resource, such as an image export, runs alongside.
pub(crate) fn start<F, Fut>(
    app: Arc<App>,
    kind: &str,
    resource: String,
    exclusive: bool,
    work: F,
) -> Result<(String, Outcome)>
where
    F: FnOnce(Arc<App>, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<Value>> + Send + 'static,
{
    start_named(app, kind, resource, None, exclusive, work)
}

/// [`start`] for a job that makes something called `name`, as [`launch_named`] describes.
pub(crate) fn start_named<F, Fut>(
    app: Arc<App>,
    kind: &str,
    resource: String,
    name: Option<String>,
    exclusive: bool,
    work: F,
) -> Result<(String, Outcome)>
where
    F: FnOnce(Arc<App>, String) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<Value>> + Send + 'static,
{
    let id = uuid::Uuid::new_v4().simple().to_string();
    let job = Job {
        id: id.clone(),
        kind: kind.into(),
        resource: Some(resource.clone()),
        name,
        status: "queued".into(),
        created_at: now(),
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
    };
    app.db.put("job", &id, &job)?;
    let lock = exclusive.then(|| app.resource_lock(&resource));
    let (done, outcome) = tokio::sync::oneshot::channel();
    let task_id = id.clone();
    tokio::spawn(async move {
        let _guard = match &lock {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };
        let mut job: Job = match app.db.get("job", &task_id) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("{e}");
                return;
            }
        };
        job.status = "running".into();
        if let Err(e) = app.db.put("job", &task_id, &job) {
            tracing::error!("{e}");
            return;
        }
        let result =
            std::panic::AssertUnwindSafe(async { work(app.clone(), task_id.clone()).await })
                .catch_unwind()
                .await;
        let result = result.unwrap_or_else(|_| Err(anyhow::anyhow!("Job ended unexpectedly")));
        // Includes failures during context preparation and panics; cleanup cannot depend on the script succeeding.
        if job.kind == "build" {
            // A failed build's last stage stays the one it failed in.
            if result.is_ok() {
                let _ = app.begin_stage(&task_id, "cleanup");
            }
            if let Err(e) = app.cleanup_build(&task_id).await {
                let _ = app.append_log(&task_id, &format!("Build cleanup: {e:#}"));
            }
        }
        // Nobody may be waiting; most callers return the job ID at once.
        let _ = done.send(finish(&app, &task_id, job, result));
    });
    Ok((id, outcome))
}

/// Record how a job ended. `job` stands in for the saved record when that cannot be read.
pub(crate) fn finish(
    app: &App,
    id: &str,
    job: Job,
    result: anyhow::Result<Value>,
) -> std::result::Result<Value, String> {
    let mut job = app.db.get("job", id).unwrap_or(job);
    let finished = now();
    job.finished_at = Some(finished);
    job.finish_stages(finished);
    job.progress = None;
    let reported = match result {
        Ok(value) => {
            job.status = "completed".into();
            job.result = Some(value.clone());
            Ok(value)
        }
        Err(e) => {
            job.status = "failed".into();
            job.log.push_str(&format!("\nERROR: {e:#}\n"));
            Err(format!("{e:#}"))
        }
    };
    job.trim_log();
    if let Err(e) = app.db.put("job", id, &job) {
        tracing::error!("Could not save job result: {e}");
    }
    if let Err(e) = app.db.prune_jobs(KEPT_JOBS) {
        tracing::debug!("Job history: {e:#}");
    }
    reported
}

/// How long a request waits for a resource a job holds before answering that it is busy.
pub(crate) const HOLD_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

impl App {
    /// Hold a resource for the length of a request. A job such as a script, an upload or an
    /// export can hold a container or a volume for an hour; a request waits a few seconds for
    /// it and then answers 409, instead of hanging its client for the job's length.
    pub(crate) async fn hold(&self, resource: &str) -> Result<tokio::sync::OwnedMutexGuard<()>> {
        self.hold_for(resource, HOLD_WAIT).await
    }
    async fn hold_for(
        &self,
        resource: &str,
        wait: std::time::Duration,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>> {
        let lock = self.resource_lock(resource);
        tokio::time::timeout(wait, lock.lock_owned())
            .await
            .map_err(|_| {
                let what = match resource.split_once(':') {
                    Some(("container", _)) => "the container",
                    Some(("volume", _)) => "the volume",
                    Some(("image", _)) => "the image",
                    _ => "this resource",
                };
                crate::error::Error(
                    axum::http::StatusCode::CONFLICT,
                    format!(
                        "Another operation on {what} is still running; retry when it has finished"
                    ),
                )
            })
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
