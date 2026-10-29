//! Whether the manager can do its work, for a probe that does not sign in.
//!
//! `/healthz` only says that the process answers, cheaply enough for the manager image's
//! `HEALTHCHECK` every 20 seconds and the WSL package's wait while the manager starts. `/readyz`
//! also asks the database and Docker, each within [`LIMIT`], so a probe never hangs on either.
use crate::App;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

/// How long each check may take.
const LIMIT: Duration = Duration::from_secs(2);

/// `GET /readyz`: 200 when the database answers and Docker is reachable, otherwise 503. The
/// answer names which check failed and never why, since anyone the host check lets in may ask.
pub async fn ready(State(app): State<Arc<App>>) -> Response {
    let (database, docker) = tokio::join!(database(&app), docker(&app));
    let ready = database && docker;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let answer = json!({"ready": ready, "database": database, "docker": docker});
    (status, Json(answer)).into_response()
}

/// Whether the database answers a query. The query runs on a blocking thread, one at a time, so
/// a database that hangs keeps a single thread waiting while later checks give up on it.
async fn database(app: &Arc<App>) -> bool {
    static RUNNING: Semaphore = Semaphore::const_new(1);
    let app = app.clone();
    let check = async move {
        let Ok(permit) = RUNNING.acquire().await else {
            return false;
        };
        tokio::task::spawn_blocking(move || {
            let _running = permit;
            app.db
                .connection()
                .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .is_ok()
        })
        .await
        .unwrap_or(false)
    };
    tokio::time::timeout(LIMIT, check).await.unwrap_or(false)
}

/// Whether Docker answers: its Engine socket's `/_ping` where the manager reaches the socket,
/// otherwise the CLI's `docker version`, which asks the daemon too.
async fn docker(app: &App) -> bool {
    let check = async {
        match &app.config.docker_socket {
            Some(socket) => ping(socket).await,
            None => app
                .docker(&["version", "--format", "{{.Server.Version}}"])
                .await
                .is_ok(),
        }
    };
    tokio::time::timeout(LIMIT, check).await.unwrap_or(false)
}

async fn ping(socket: &Path) -> bool {
    let Ok(stream) = crate::pulls::connect(socket).await else {
        return false;
    };
    let Ok((mut sender, connection)) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream)).await
    else {
        return false;
    };
    let Ok(request) = hyper::Request::get("/_ping")
        .header(hyper::header::HOST, "docker")
        .body(http_body_util::Empty::<hyper::body::Bytes>::new())
    else {
        return false;
    };
    let _closing = Closing(tokio::spawn(connection));
    sender
        .send_request(request)
        .await
        .is_ok_and(|response| response.status().is_success())
}

/// Ends an Engine connection's task with its check, also when the limit cuts the check short.
struct Closing<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for Closing<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
