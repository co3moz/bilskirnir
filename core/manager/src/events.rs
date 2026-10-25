//! `GET /api/events`: what the manager tells a signed-in dashboard as it happens, so a page learns
//! that a job was queued, started or ended, or that an agent asks for a person, without polling
//! for it. Every event is one JSON object
//! with a `type`, sent as the data of an unnamed server-sent event. Nothing is replayed: a page
//! reads its lists when it connects, and after any gap in the stream it reads them again.
use crate::{App, auth, model::Job};
use axum::{
    extract::State,
    http::HeaderValue,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio_util::sync::CancellationToken;

/// Events a slow page may fall behind by before it is told to read everything again.
const CAPACITY: usize = 256;
/// The shortest time between two progress events of one job.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);
/// How often an idle stream sends a comment, so proxies and browsers keep it open.
const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// The longest error a job event carries; the job's log has the whole of it.
const ERROR_LIMIT: usize = 300;

/// The first event of every stream: it is open and events follow.
const READY: &str = r#"{"type":"ready"}"#;
/// Sent instead of the events a page fell too far behind to receive.
const RESYNC: &str = r#"{"type":"resync"}"#;

/// The manager's events and the pages that listen. Publishing never waits for a listener: each
/// stream has its own place in a bounded ring, and one that falls behind loses the oldest events
/// and is told to read everything again.
pub(crate) struct Hub {
    sender: broadcast::Sender<Arc<str>>,
    /// Cancelled when the manager shuts down, which ends every stream and every wait on one.
    closing: CancellationToken,
    /// When each running job last sent its progress.
    progress: Mutex<HashMap<String, Instant>>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::with_capacity(CAPACITY)
    }
}

impl Hub {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            closing: CancellationToken::new(),
            progress: Mutex::new(HashMap::new()),
        }
    }
    /// Send `event` to every open stream; without one it goes nowhere.
    pub(crate) fn publish(&self, event: Value) {
        let _ = self.sender.send(Arc::from(event.to_string()));
    }
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.sender.subscribe()
    }
    /// A job was queued, started, began a stage or ended. `error` is why it failed.
    pub(crate) fn job(&self, job: &Job, error: Option<&str>) {
        if job.finished_at.is_some() {
            self.progress.lock().unwrap().remove(&job.id);
        }
        self.publish(json!({
            "type": "job",
            "job": {
                "id": job.id,
                "kind": job.kind,
                "resource": job.resource,
                "name": job.name,
                "status": job.status,
                "created_at": job.created_at,
                "finished_at": job.finished_at,
                "error": error.map(short),
            },
        }));
    }
    /// A running job measured its progress again. A page hears of it at most once a second; the
    /// end of a measured step, `None`, always reaches it.
    pub(crate) fn progress(&self, id: &str, progress: Option<&Value>) {
        if progress.is_some() {
            let mut sent = self.progress.lock().unwrap();
            let now = Instant::now();
            if sent
                .get(id)
                .is_some_and(|at| now.duration_since(*at) < PROGRESS_EVERY)
            {
                return;
            }
            sent.insert(id.to_owned(), now);
        }
        self.publish(json!({"type": "job-progress", "id": id, "progress": progress}));
    }
    /// End every stream, as the manager shuts down.
    pub(crate) fn close(&self) {
        self.closing.cancel();
    }
    /// Resolves once the manager shuts down, which also ends every wait for a person.
    pub(crate) async fn closed(&self) {
        self.closing.cancelled().await;
    }
}

/// The start of an error, on one line.
fn short(error: &str) -> String {
    let line = error
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    match line.char_indices().nth(ERROR_LIMIT) {
        Some((end, _)) => format!("{}...", &line[..end]),
        None => line.to_owned(),
    }
}

/// What one stream reads from: the hub, and what ends it.
struct Reader {
    events: broadcast::Receiver<Arc<str>>,
    /// Resolves when the session that opened the stream signs out or expires.
    ended: Pin<Box<dyn Future<Output = ()> + Send>>,
    closing: CancellationToken,
    greeted: bool,
}

impl Reader {
    /// The next event's data, or `None` once the stream ends.
    async fn next(&mut self) -> Option<Arc<str>> {
        if !self.greeted {
            self.greeted = true;
            return Some(Arc::from(READY));
        }
        tokio::select! {
            biased;
            () = &mut self.ended => None,
            () = self.closing.cancelled() => None,
            received = self.events.recv() => match received {
                Ok(event) => Some(event),
                Err(RecvError::Lagged(_)) => Some(Arc::from(RESYNC)),
                Err(RecvError::Closed) => None,
            },
        }
    }
}

/// `GET /api/events`: the event stream of the signed-in session, which ends with it.
pub async fn stream(State(app): State<Arc<App>>, session: auth::Session) -> Response {
    let reader = Reader {
        events: app.events.subscribe(),
        closing: app.events.closing.clone(),
        ended: Box::pin(async move { app.session_revoked(session).await }),
        greeted: false,
    };
    let events = futures_util::stream::unfold(reader, |mut reader| async move {
        let data = reader.next().await?;
        Some((Ok::<_, Infallible>(Event::default().data(&*data)), reader))
    });
    let mut response = Sse::new(events)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
        .into_response();
    // A proxy that buffers responses, such as nginx, sends each event at once.
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
