//! Requests for a person: a waiting agent gets the answer as soon as it is given, a call that runs
//! out of time returns pending with the id that resumes it, asking again joins the open request,
//! a request is read only for its own container, and a manager that shuts down or forgot the
//! request says so instead of leaving the agent waiting.
use super::*;
use crate::{
    model::CONTAINERS,
    tests::{mcp_app, mcp_request},
};
use axum::body::Body;
use tower::ServiceExt;

fn container(id: &str, name: &str) -> Container {
    Container {
        id: id.into(),
        container: format!("thoric-{id}"),
        private_network: format!("thoric-{id}-private"),
        egress_network: format!("thoric-{id}-egress"),
        source_image_id: "sha256:desktop".into(),
        created_at: 1,
        spec: serde_json::from_value(json!({"name": name, "image": "sha256:desktop"})).unwrap(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

fn app_with_containers() -> Arc<App> {
    let app = mcp_app();
    for (id, name) in [("c1", "Chrome dev"), ("c2", "Unity")] {
        app.db.put(CONTAINERS, id, &container(id, name)).unwrap();
    }
    app
}

/// `request_human` on the administrator's `/mcp`: its structured result, or its error text.
async fn call(app: &Arc<App>, arguments: Value) -> std::result::Result<Value, String> {
    let (status, body) = mcp_request(
        app.clone(),
        "tools/call",
        json!({"name": "request_human", "arguments": arguments}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    if result["isError"] == true {
        return Err(result["content"][0]["text"].as_str().unwrap().to_owned());
    }
    Ok(result["structuredContent"].clone())
}

/// A request of the signed-in dashboard.
async fn dashboard(
    app: &Arc<App>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    app.sessions.lock().unwrap().insert(
        "person".into(),
        std::time::Instant::now() + Duration::from_secs(3600),
    );
    let request = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "localhost:8080")
        .header("cookie", "thoric_session=person")
        .header("x-thoric-request", "1")
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap();
    let response = crate::router(app.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn until_pending(app: &App, count: usize) -> Vec<HumanRequest> {
    for _ in 0..500 {
        let pending = app.human.pending();
        if pending.len() == count {
            return pending;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{count} requests never became pending");
}

#[tokio::test]
async fn a_waiting_agent_gets_the_answer_as_soon_as_a_person_gives_it() {
    let app = app_with_containers();
    let started = Instant::now();
    let waiting = tokio::spawn({
        let app = app.clone();
        async move {
            call(
                &app,
                json!({"container_id": "c1", "message": "  Solve the CAPTCHA in Chrome  ", "wait_seconds": 60}),
            )
            .await
        }
    });
    let pending = until_pending(&app, 1).await;
    let (status, listed) = dashboard(&app, "GET", "/api/human-requests", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"][0]["container_name"], "Chrome dev");
    assert_eq!(listed["items"][0]["message"], "Solve the CAPTCHA in Chrome");
    assert_eq!(listed["items"][0]["status"], "pending");
    assert!(listed["items"][0]["endpoint"].is_null());
    let (status, answered) = dashboard(
        &app,
        "POST",
        &format!("/api/human-requests/{}/answer", pending[0].id),
        Some(json!({"answer": "done", "reply": " Solved, go on "})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(answered["status"], "done");
    let result = waiting.await.unwrap().unwrap();
    assert!(started.elapsed() < Duration::from_secs(30));
    assert_eq!(result["status"], "done");
    assert_eq!(result["reply"], "Solved, go on");
    assert_eq!(result["request_id"], json!(pending[0].id));
    assert!(result["answered_at"].is_u64() && result.get("next").is_none());
    // An answered request leaves the dashboard's list, and a second answer is refused.
    assert_eq!(
        dashboard(&app, "GET", "/api/human-requests", None).await.1["items"],
        json!([])
    );
    let (status, _) = dashboard(
        &app,
        "POST",
        &format!("/api/human-requests/{}/answer", pending[0].id),
        Some(json!({"answer": "decline"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_call_without_an_answer_returns_pending_and_its_id_keeps_waiting() {
    let app = app_with_containers();
    let first = call(
        &app,
        json!({"container_id": "c1", "message": "Enter the 2FA code", "wait_seconds": 0}),
    )
    .await
    .unwrap();
    assert_eq!(first["status"], "pending");
    assert!(first["next"].as_str().unwrap().contains("request_id"));
    let id = first["request_id"].as_str().unwrap().to_owned();
    // An agent that lost its call and asks again joins the same request.
    let again = call(
        &app,
        json!({"container_id": "c1", "message": "Enter the 2FA code", "wait_seconds": 0}),
    )
    .await
    .unwrap();
    assert_eq!(again["request_id"], json!(id));
    assert_eq!(app.human.pending().len(), 1);
    let resumed = call(
        &app,
        json!({"container_id": "c1", "request_id": id, "wait_seconds": 0}),
    )
    .await
    .unwrap();
    assert_eq!(resumed["status"], "pending");
    app.human
        .answer(&id, Answer::Decline, Some("No phone at hand"), &app.events)
        .unwrap();
    // The answer is kept for a later call with the id, which returns at once.
    let resumed = call(
        &app,
        json!({"container_id": "c1", "request_id": id, "wait_seconds": 60}),
    )
    .await
    .unwrap();
    assert_eq!(resumed["status"], "declined");
    assert_eq!(resumed["reply"], "No phone at hand");
    // A new ask with the same message is a new request once the first was answered.
    let new = call(
        &app,
        json!({"container_id": "c1", "message": "Enter the 2FA code", "wait_seconds": 0}),
    )
    .await
    .unwrap();
    assert_ne!(new["request_id"], json!(id));
}

#[tokio::test]
async fn a_request_is_read_only_for_its_own_container_and_an_unknown_one_says_why() {
    let app = app_with_containers();
    let mine = call(
        &app,
        json!({"container_id": "c1", "message": "Accept the license", "wait_seconds": 0}),
    )
    .await
    .unwrap();
    let theirs = call(
        &app,
        json!({"container_id": "c2", "request_id": mine["request_id"], "wait_seconds": 0}),
    )
    .await
    .unwrap_err();
    assert!(theirs.contains("No request"), "{theirs}");
    let forgotten = call(
        &app,
        json!({"container_id": "c1", "request_id": "gone", "wait_seconds": 0}),
    )
    .await
    .unwrap_err();
    assert!(forgotten.contains("manager restart"), "{forgotten}");
    for refused in [
        json!({"container_id": "c1", "wait_seconds": 0}),
        json!({"container_id": "c1", "message": "Two at once", "request_id": mine["request_id"]}),
        json!({"container_id": "c1", "message": "   "}),
        json!({"container_id": "c1", "message": "x".repeat(MESSAGE_LIMIT + 1)}),
        json!({"container_id": "missing", "message": "Nobody is here"}),
    ] {
        assert!(call(&app, refused.clone()).await.is_err(), "{refused}");
    }
}

#[tokio::test]
async fn answers_are_checked_and_open_requests_are_bounded() {
    let app = app_with_containers();
    let c1 = app.container("c1").unwrap();
    let request = app.human.ask(&c1, "Sign in", None, &app.events).unwrap();
    let error = app
        .human
        .answer(
            &request.id,
            Answer::Done,
            Some(&"x".repeat(REPLY_LIMIT + 1)),
            &app.events,
        )
        .unwrap_err();
    assert_eq!(error.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        app.human
            .answer("unknown", Answer::Done, None, &app.events)
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = dashboard(
        &app,
        "POST",
        &format!("/api/human-requests/{}/answer", request.id),
        Some(json!({"answer": "maybe"})),
    )
    .await;
    assert!(status.is_client_error(), "{status}");
    // An empty reply is no reply.
    let done = app
        .human
        .answer(&request.id, Answer::Done, Some("  "), &app.events)
        .unwrap();
    assert_eq!(done.reply, None);
    for number in 1..PENDING_LIMIT {
        app.human
            .ask(&c1, &format!("Step {number}"), None, &app.events)
            .unwrap();
    }
    app.human.ask(&c1, "One more", None, &app.events).unwrap();
    let full = app
        .human
        .ask(&c1, "Too many", None, &app.events)
        .unwrap_err();
    assert_eq!(full.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(app.human.pending().len(), PENDING_LIMIT);
}

#[tokio::test]
async fn the_dashboard_hears_of_a_new_request_and_of_its_answer() {
    let app = app_with_containers();
    let mut events = app.events.subscribe();
    let c1 = app.container("c1").unwrap();
    let request = app
        .human
        .ask(&c1, "Solve the CAPTCHA", Some("chrome-dev"), &app.events)
        .unwrap();
    app.human
        .ask(&c1, "Solve the CAPTCHA", Some("chrome-dev"), &app.events)
        .unwrap();
    app.human
        .answer(&request.id, Answer::Done, None, &app.events)
        .unwrap();
    let received: Vec<Value> = std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| serde_json::from_str(&event).unwrap())
        .collect();
    assert_eq!(received.len(), 2, "{received:?}");
    assert_eq!(received[0]["type"], "human-request");
    assert_eq!(received[0]["request"]["endpoint"], "chrome-dev");
    assert_eq!(received[0]["request"]["status"], "pending");
    assert_eq!(received[1]["type"], "human-request-answered");
    assert_eq!(received[1]["request"]["status"], "done");
}

#[tokio::test]
async fn a_wait_ends_with_an_error_when_the_manager_shuts_down_and_quietly_when_its_client_leaves()
{
    let app = app_with_containers();
    let c1 = app.container("c1").unwrap();
    let request = app.human.ask(&c1, "Sign in", None, &app.events).unwrap();
    let left = Caller::default();
    left.cancelled.cancel();
    let returned = app
        .human
        .wait(&request.id, Duration::from_secs(60), &left, &app.events)
        .await
        .unwrap();
    assert_eq!(returned.status, Status::Pending);
    let waiting = tokio::spawn({
        let app = app.clone();
        let id = request.id.clone();
        async move {
            app.human
                .wait(
                    &id,
                    Duration::from_secs(60),
                    &Caller::default(),
                    &app.events,
                )
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    app.events.close();
    let error = waiting.await.unwrap().unwrap_err();
    assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
    assert!(error.1.contains("shutting down"));
    // The request outlives a call its client gave up, for the person to answer.
    assert_eq!(app.human.pending().len(), 1);
}

#[tokio::test]
async fn a_client_that_asked_for_progress_hears_it_while_the_call_waits() {
    let app = app_with_containers();
    let response = crate::router(app.clone())
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("host", "localhost:8080")
                .header("authorization", "Bearer test-token-with-at-least-32-characters")
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25")
                .body(Body::from(
                    json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{
                        "name":"request_human","_meta":{"progressToken":"wait-1"},
                        "arguments":{"container_id":"c1","message":"Approve the sign-in","wait_seconds":1}
                    }})
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let messages: Vec<Value> = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str(data.trim()).ok())
        .collect();
    let progress = messages
        .iter()
        .find(|message| message["method"] == "notifications/progress")
        .expect("a progress notification");
    assert_eq!(progress["params"]["progressToken"], "wait-1");
    assert_eq!(progress["params"]["total"], 1.0);
    let result = messages.iter().find(|message| message["id"] == 7).unwrap();
    assert_eq!(result["result"]["structuredContent"]["status"], "pending");
}

#[test]
fn the_tool_routes_and_events_are_documented() {
    let api = include_str!("../../../docs/API.md");
    for route in [
        "| GET | `/api/human-requests` |",
        "| POST | `/api/human-requests/{id}/answer` |",
    ] {
        assert!(api.contains(route), "docs/API.md must list {route}");
    }
    let guide = include_str!("../../../docs/MCP.md");
    assert!(guide.contains("## Ask a person for help"));
    for field in ["`request_id`", "`wait_seconds`", "`pending`", "`declined`"] {
        assert!(guide.contains(field), "docs/MCP.md must explain {field}");
    }
}
