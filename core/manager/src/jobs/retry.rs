//! Starting a job again after it failed or was interrupted, with the request its route was given,
//! so a build that failed on a dropped connection is not configured again from the start.
//!
//! A job that can be retried keeps its request in a record of its own beside the job, since the
//! job's record is read and written again with every line of its log and lists and events carry
//! no request. A completed job drops it, a retry hands it to the job it starts, and it goes with
//! its job when the history is pruned.
//! Only requests that hold all the job needs are kept: a script build, a snapshot and a catalog
//! image's download or source build. A container's creation can carry secret values, which live
//! only in its `environment-setup` record, and an upload's body is gone once it has been read.
use crate::{
    App,
    error::{Error, Result},
    model::Job,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// The database kind of the kept requests, saved under their job's ID.
pub(crate) const REQUESTS: &str = "job-request";

/// What a job's route was asked to do, tagged with the job's kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "kebab-case")]
pub(crate) enum Request {
    Build(crate::api::BuildImage),
    Snapshot {
        container_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        description: String,
    },
    ImagePull {
        image_id: String,
    },
    SourceBuild {
        image_id: String,
    },
}

/// Whether `job` ended without its result: it failed, or a restart interrupted it.
fn unfinished(job: &Job) -> bool {
    matches!(job.status.as_str(), "failed" | "interrupted")
}

/// Whether `job` can be started again with its request.
pub(crate) fn retryable(app: &App, job: &Job) -> bool {
    unfinished(job) && app.db.contains(REQUESTS, &job.id).unwrap_or(false)
}

/// Refuse a job that did not fail and was not interrupted.
fn ended_without_result(job: &Job) -> Result<()> {
    if !unfinished(job) {
        return Err(Error(
            StatusCode::CONFLICT,
            "Only a failed or interrupted job can be retried".into(),
        ));
    }
    Ok(())
}

/// Why `job` has no request to start it again with.
fn none_kept(job: &Job) -> Error {
    Error(
        StatusCode::CONFLICT,
        format!(
            "This {} job keeps no request to retry, or it was retried already; start it again where it was started",
            job.kind
        ),
    )
}

/// The request `job` can be started again with.
fn kept(app: &App, job: &Job) -> Result<Request> {
    ended_without_result(job)?;
    if !app.db.contains(REQUESTS, &job.id)? {
        return Err(none_kept(job));
    }
    Ok(app.db.get(REQUESTS, &job.id)?)
}

/// `POST /api/jobs/{id}/retry`: start a failed or interrupted job again. The request goes through
/// its route as it did the first time, checked again, and makes a new job of the same kind,
/// which keeps it from then on: the job retried is not retried again, so a second click starts
/// no second build. A route that refuses it leaves it with the job.
pub async fn retry(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let job: Job = app.db.get("job", &id)?;
    ended_without_result(&job)?;
    // Taken, not read, so of two retries at once only one finds it.
    let request: Request = app.db.take(REQUESTS, &id)?.ok_or_else(|| none_kept(&job))?;
    let started = start(app.clone(), request.clone()).await;
    if started.is_err()
        && let Err(error) = app.db.put(REQUESTS, &id, &request)
    {
        tracing::warn!("Job {id} lost the request it could be retried with: {error:#}");
    }
    started
}

/// Send `request` through its route as a new job.
async fn start(app: Arc<App>, request: Request) -> Result<Json<Value>> {
    match request {
        Request::Build(body) => crate::api::build(State(app), Json(body)).await,
        Request::Snapshot {
            container_id,
            name,
            description,
        } => {
            let body = crate::api::Snapshot { name, description };
            crate::api::snapshot(State(app), Path(container_id), Json(body)).await
        }
        Request::ImagePull { image_id } => {
            crate::api::pull_catalog(State(app), Path(image_id)).await
        }
        Request::SourceBuild { image_id } => {
            crate::api::build_catalog(State(app), Path(image_id)).await
        }
    }
}

/// `GET /api/jobs/{id}/request`: the request a failed or interrupted job would be retried with, so
/// the dashboard can open its form with it to change something first.
pub async fn request(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let job: Job = app.db.get("job", &id)?;
    Ok(Json(json!(kept(&app, &job)?)))
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
