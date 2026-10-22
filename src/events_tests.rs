//! The event stream: only a signed-in session opens it and it ends with that session, every job
//! transition reaches it, progress is thinned out, and a page that falls behind is told to read
//! everything again instead of holding up whoever publishes.
use super::*;
use crate::tests::app;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Every `type` the stream sends, which `docs/API.md` must describe.
const TYPES: [&str; 6] = [
    "ready",
    "resync",
    "job",
    "job-progress",
    "human-request",
    "human-request-answered",
];

async fn open(app: &Arc<App>, cookie: Option<&str>) -> Response {
    let mut request = Request::builder()
        .uri("/api/events")
        .header("host", "localhost:8080");
    if let Some(token) = cookie {
        request = request.header("cookie", format!("bilskirnir_session={token}"));
    }
    crate::router(app.clone())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// The data of the stream's next event, or `None` once it ended.
async fn next_data(body: &mut Body) -> Option<Value> {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), body.frame())
            .await
            .expect("the stream answers in time")?
            .unwrap();
        let Ok(bytes) = frame.into_data() else {
            continue;
        };
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        if let Some(data) = text.lines().find_map(|line| line.strip_prefix("data: ")) {
            return Some(serde_json::from_str(data).unwrap());
        }
    }
}

fn reader(events: broadcast::Receiver<Arc<str>>, closing: CancellationToken) -> Reader {
    Reader {
        events,
        ended: Box::pin(std::future::pending()),
        closing,
        greeted: false,
    }
}

#[tokio::test]
async fn only_a_signed_in_session_opens_the_stream() {
    let app = app(true);
    assert_eq!(open(&app, None).await.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        open(&app, Some("unknown")).await.status(),
        StatusCode::UNAUTHORIZED
    );
    app.sessions
        .lock()
        .unwrap()
        .insert("expired".into(), Instant::now() - Duration::from_secs(1));
    assert_eq!(
        open(&app, Some("expired")).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_stream_carries_what_is_published_until_its_session_signs_out() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "signed-in".into(),
        Instant::now() + Duration::from_secs(3600),
    );
    let response = open(&app, Some("signed-in")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    let mut body = response.into_body();
    assert_eq!(next_data(&mut body).await, Some(json!({"type": "ready"})));
    app.events
        .publish(json!({"type": "job", "job": {"id": "one"}}));
    assert_eq!(
        next_data(&mut body).await.unwrap()["job"]["id"],
        json!("one")
    );
    // Signing out ends the stream, as it ends every other long request of the session.
    app.sessions.lock().unwrap().remove("signed-in");
    app.session_events.notify_waiters();
    assert_eq!(next_data(&mut body).await, None);
}

#[tokio::test]
async fn a_stream_ends_when_the_manager_shuts_down() {
    let app = app(false);
    let mut body = open(&app, None).await.into_body();
    assert_eq!(next_data(&mut body).await, Some(json!({"type": "ready"})));
    app.events.close();
    assert_eq!(next_data(&mut body).await, None);
}

#[tokio::test]
async fn a_page_that_falls_behind_reads_everything_again_and_holds_up_nobody() {
    let hub = Hub::with_capacity(4);
    let mut slow = reader(hub.subscribe(), hub.closing.clone());
    // Nobody reads while these are published, and publishing still returns at once.
    let started = Instant::now();
    for number in 0..10_000 {
        hub.publish(json!({"type": "job-progress", "id": number.to_string()}));
    }
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(slow.next().await.as_deref(), Some(READY));
    assert_eq!(slow.next().await.as_deref(), Some(RESYNC));
    // It goes on with the newest events the ring still holds.
    let next: Value = serde_json::from_str(&slow.next().await.unwrap()).unwrap();
    assert_eq!(next["id"], "9996");
}

#[tokio::test]
async fn every_job_transition_is_published_with_why_it_failed() {
    let app = app(false);
    let mut events = app.events.subscribe();
    let mut statuses = |id: String| {
        let mut seen = Vec::new();
        while let Ok(event) = events.try_recv() {
            let event: Value = serde_json::from_str(&event).unwrap();
            assert_eq!(event["type"], "job");
            if event["job"]["id"] == json!(id) {
                seen.push(event["job"].clone());
            }
        }
        seen
    };
    let (id, outcome) = crate::jobs::start(
        app.clone(),
        "stop",
        "container:one".into(),
        true,
        |_, _| async { Ok(json!({})) },
    )
    .unwrap();
    outcome.await.unwrap().unwrap();
    let seen = statuses(id.clone());
    let names: Vec<&str> = seen
        .iter()
        .map(|job| job["status"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["queued", "running", "completed"]);
    assert_eq!(seen[0]["kind"], "stop");
    assert_eq!(seen[0]["resource"], "container:one");
    assert!(seen[0]["finished_at"].is_null() && seen[2]["finished_at"].is_u64());
    assert!(seen[2]["error"].is_null());

    let (id, outcome) = crate::jobs::start(
        app.clone(),
        "start",
        "container:one".into(),
        true,
        |_, _| async { Err(anyhow::anyhow!("Docker refused\nwith a long story")) },
    )
    .unwrap();
    assert!(outcome.await.unwrap().is_err());
    let seen = statuses(id);
    assert_eq!(seen.last().unwrap()["status"], "failed");
    assert_eq!(seen.last().unwrap()["error"], "Docker refused");
}

#[tokio::test]
async fn a_new_stage_is_published_for_the_lists_that_name_it() {
    let app = app(false);
    let mut events = app.events.subscribe();
    let (id, outcome) = crate::jobs::start(
        app.clone(),
        "build",
        "image:new".into(),
        false,
        |app, id| async move {
            app.plan_stages(&id, &[("base", "Preparing"), ("script", "Running")])?;
            app.begin_stage(&id, "script")?;
            Ok(json!({}))
        },
    )
    .unwrap();
    // A build cleans up after itself, which needs Docker; its outcome does not matter here.
    let _ = outcome.await;
    let mut running = 0;
    while let Ok(event) = events.try_recv() {
        let event: Value = serde_json::from_str(&event).unwrap();
        if event["job"]["id"] == json!(id) && event["job"]["status"] == "running" {
            running += 1;
        }
    }
    // Once when it started, once for the stage it began (and once more for cleanup).
    assert!(running >= 2, "{running}");
}

#[test]
fn progress_reaches_a_page_at_most_once_a_second_but_its_end_always_does() {
    let hub = Hub::default();
    let mut events = hub.subscribe();
    for current in 0..5 {
        hub.progress("job", Some(&json!({"current": current})));
    }
    hub.progress("other", Some(&json!({"current": 1})));
    hub.progress("job", None);
    let received: Vec<Value> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| serde_json::from_str(&event).unwrap())
        .collect();
    assert_eq!(
        received,
        [
            json!({"type": "job-progress", "id": "job", "progress": {"current": 0}}),
            json!({"type": "job-progress", "id": "other", "progress": {"current": 1}}),
            json!({"type": "job-progress", "id": "job", "progress": null}),
        ]
    );
    // A job that ended forgets when it last reported.
    let job: Job = serde_json::from_value(json!({
        "id": "job", "kind": "build", "status": "completed", "created_at": 1,
        "finished_at": 2, "log": "", "result": null
    }))
    .unwrap();
    hub.job(&job, None);
    assert!(!hub.progress.lock().unwrap().contains_key("job"));
}

#[test]
fn an_error_is_cut_to_its_first_line_and_a_bounded_length() {
    assert_eq!(short("\nfirst line\nsecond"), "first line");
    let long = "é".repeat(ERROR_LIMIT + 10);
    let cut = short(&long);
    assert_eq!(cut.chars().count(), ERROR_LIMIT + 3);
    assert!(cut.ends_with("..."));
}

#[test]
fn the_route_and_every_event_type_are_documented() {
    let guide = include_str!("../docs/API.md");
    assert!(guide.contains("| GET | `/api/events` |"));
    for kind in TYPES {
        assert!(
            guide.contains(&format!("`{kind}`")),
            "docs/API.md must describe the {kind} event"
        );
    }
}
