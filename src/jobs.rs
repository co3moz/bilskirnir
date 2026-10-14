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
    let (id, _) = start(app, kind, resource, true, work)?;
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
    let id = uuid::Uuid::new_v4().simple().to_string();
    let job = Job {
        id: id.clone(),
        kind: kind.into(),
        resource: Some(resource.clone()),
        status: "queued".into(),
        created_at: now(),
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
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
        if job.kind == "build"
            && let Err(e) = app.cleanup_build(&task_id).await
        {
            let _ = app.append_log(&task_id, &format!("Build cleanup: {e:#}"));
        }
        job = app.db.get("job", &task_id).unwrap_or(job);
        job.finished_at = Some(now());
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
        if let Err(e) = app.db.put("job", &task_id, &job) {
            tracing::error!("Could not save job result: {e}");
        }
        // Nobody may be waiting; most callers return the job ID at once.
        let _ = done.send(reported);
    });
    Ok((id, outcome))
}
