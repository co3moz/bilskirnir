//! Pins down that a job that can be retried keeps the request its route was given until it
//! completes, that only a failed or interrupted one is started again, and that starting it again
//! goes through its route as a new job of the same kind with the same request.
use super::*;
use crate::{api::BuildImage, model::Container};

/// Wait for a job's end, which a test app without Docker reaches at its first Docker command.
async fn ended(app: &App, id: &str) -> Job {
    for _ in 0..500 {
        let job: Job = app.db.job(id).unwrap();
        if job.finished_at.is_some() {
            return job;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("job {id} did not end");
}

fn started(response: Result<Json<Value>>) -> String {
    response.unwrap().0["job_id"].as_str().unwrap().to_owned()
}

fn build() -> BuildImage {
    serde_json::from_value(json!({
        "name": "Reverse engineering",
        "description": "Ghidra and GDB",
        "base_image": "example/desktop:1",
        "internet": false,
        "parts": [
            {"id": "ghidra", "name": "Ghidra", "script": "apt-get install -y openjdk-21-jdk"},
            {"id": "gdb", "name": "GDB", "script": "apt-get install -y gdb"},
        ],
        "compose": {"parameters": [{"name": "PROJECT", "label": "Project", "type": "text"}], "script": "echo \"$PROJECT\""},
        "mcp": [{"name": "ghidra", "command": "ghidra-mcp", "args": ["--stdio"]}],
        "icon": crate::icons::sample(64, 64),
        "template_ids": ["ghidra", "gdb"],
    }))
    .unwrap()
}

fn job(id: &str, kind: &str, status: &str) -> Job {
    Job {
        id: id.into(),
        kind: kind.into(),
        resource: None,
        name: None,
        status: status.into(),
        created_at: 1,
        finished_at: (status != "running").then_some(2),
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
        script: None,
    }
}

/// The dashboard opens the build form with what `GET /api/jobs/{id}/request` returns, so a kept
/// build is the build route's body as it was sent, with every field it can have. The route accepts
/// this one, as the next test shows.
#[test]
fn a_kept_build_is_the_body_its_route_was_sent() {
    let body = build();
    let kept = serde_json::to_value(Request::Build(body.clone())).unwrap();
    assert_eq!(kept["kind"], "build");
    assert_eq!(kept["request"], serde_json::to_value(&body).unwrap());
    assert_eq!(
        serde_json::from_value::<Request>(kept).unwrap(),
        Request::Build(body)
    );
    let snapshot = Request::Snapshot {
        container_id: "c1".into(),
        name: "Before the upgrade".into(),
        description: String::new(),
    };
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        json!({"kind": "snapshot", "request": {"container_id": "c1", "name": "Before the upgrade"}})
    );
    for (request, kind) in [
        (
            Request::ImagePull {
                image_id: "xfce".into(),
            },
            "image-pull",
        ),
        (
            Request::SourceBuild {
                image_id: "xfce".into(),
            },
            "source-build",
        ),
    ] {
        assert_eq!(serde_json::to_value(request).unwrap()["kind"], kind);
    }
}

#[tokio::test]
async fn a_failed_build_keeps_its_request_and_is_built_again_with_it() {
    let app = crate::tests::app(false);
    let first = started(crate::api::build(State(app.clone()), Json(build())).await);
    assert_eq!(
        app.db.get::<Request>(REQUESTS, &first).unwrap(),
        Request::Build(build())
    );
    assert_eq!(ended(&app, &first).await.status, "failed");
    let Json(shown) = crate::api::job(State(app.clone()), Path(first.clone()))
        .await
        .unwrap();
    assert_eq!(shown["retryable"], true);
    let Json(kept) = request(State(app.clone()), Path(first.clone()))
        .await
        .unwrap();
    assert_eq!(kept, json!(Request::Build(build())));

    let second = started(retry(State(app.clone()), Path(first.clone())).await);
    assert_ne!(second, first);
    let again = ended(&app, &second).await;
    assert_eq!(
        (again.kind.as_str(), again.name.as_deref()),
        ("build", Some("Reverse engineering"))
    );
    assert_eq!(
        app.db.get::<Request>(REQUESTS, &second).unwrap(),
        Request::Build(build())
    );
    // The first can still be retried; each retry is a job of its own.
    assert!(retryable(&app, &app.db.job(&first).unwrap()));
}

#[tokio::test]
async fn a_retried_snapshot_snapshots_the_same_container() {
    let app = crate::tests::app(false);
    let container = Container {
        id: "c1".into(),
        container: "thoric-c1".into(),
        private_network: "thoric-c1-private".into(),
        egress_network: "thoric-c1-egress".into(),
        source_image_id: "sha256:base".into(),
        created_at: 0,
        spec: serde_json::from_value(json!({"name": "Unity", "image": "example/desktop:1"}))
            .unwrap(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db
        .put(crate::model::CONTAINERS, "c1", &container)
        .unwrap();
    let request = Request::Snapshot {
        container_id: "c1".into(),
        name: "Before the upgrade".into(),
        description: "Unity 6".into(),
    };
    app.db
        .put("job", "old", &job("old", "snapshot", "interrupted"))
        .unwrap();
    app.db.put(REQUESTS, "old", &request).unwrap();

    let id = started(retry(State(app.clone()), Path("old".into())).await);
    let snapshot = ended(&app, &id).await;
    assert_eq!(snapshot.kind, "snapshot");
    assert_eq!(snapshot.resource.as_deref(), Some("container:c1"));
    assert_eq!(snapshot.name.as_deref(), Some("Before the upgrade"));
    assert_eq!(app.db.get::<Request>(REQUESTS, &id).unwrap(), request);
}

#[tokio::test]
async fn only_a_failed_or_interrupted_job_with_its_request_is_retried() {
    let app = crate::tests::app(false);
    let pull = Request::ImagePull {
        image_id: "missing".into(),
    };
    for (id, kind, status, kept) in [
        ("running", "image-pull", "running", true),
        ("done", "image-pull", "completed", true),
        ("exec", "exec", "failed", false),
    ] {
        app.db.put("job", id, &job(id, kind, status)).unwrap();
        if kept {
            app.db.put(REQUESTS, id, &pull).unwrap();
        }
        let refused = retry(State(app.clone()), Path(id.into()))
            .await
            .unwrap_err();
        assert_eq!(refused.0, StatusCode::CONFLICT, "{id}");
        assert_eq!(
            request(State(app.clone()), Path(id.into()))
                .await
                .unwrap_err()
                .0,
            StatusCode::CONFLICT,
            "{id}"
        );
        let Json(shown) = crate::api::job(State(app.clone()), Path(id.into()))
            .await
            .unwrap();
        assert!(shown.get("retryable").is_none(), "{id}");
    }
    let refused = retry(State(app.clone()), Path("exec".into()))
        .await
        .unwrap_err();
    assert!(refused.1.contains("exec job"), "{}", refused.1);
    let missing = retry(State(app.clone()), Path("gone".into()))
        .await
        .unwrap_err();
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    // The request goes through its route again: a catalog image that is gone is not pulled.
    app.db
        .put("job", "failed", &job("failed", "image-pull", "failed"))
        .unwrap();
    app.db.put(REQUESTS, "failed", &pull).unwrap();
    let gone = retry(State(app.clone()), Path("failed".into()))
        .await
        .unwrap_err();
    assert_eq!(gone.0, StatusCode::NOT_FOUND);
    assert_eq!(app.db.list::<Job>("job").unwrap().len(), 4);
}

#[tokio::test]
async fn a_completed_job_drops_its_request() {
    let app = crate::tests::app(false);
    let request = Request::SourceBuild {
        image_id: "xfce".into(),
    };
    let ok = started(crate::jobs::launch_retryable(
        app.clone(),
        "source-build",
        "image:ok".into(),
        None,
        request.clone(),
        |_, _| async { Ok(json!({})) },
    ));
    let failed = started(crate::jobs::launch_retryable(
        app.clone(),
        "source-build",
        "image:failed".into(),
        None,
        request,
        |_, _| async { anyhow::bail!("rpc error: code = Unavailable") },
    ));
    assert_eq!(ended(&app, &ok).await.status, "completed");
    assert!(!app.db.contains(REQUESTS, &ok).unwrap());
    assert_eq!(ended(&app, &failed).await.status, "failed");
    assert!(retryable(&app, &app.db.job(&failed).unwrap()));
}

/// The routes are documented where the rest of the job routes are.
#[test]
fn the_api_guide_lists_the_retry_routes() {
    let guide = include_str!("../../../../docs/API.md");
    for route in [
        "| POST | `/api/jobs/{job_id}/retry` |",
        "| GET | `/api/jobs/{job_id}/request` |",
    ] {
        assert!(guide.contains(route), "docs/API.md must list {route}");
    }
    assert!(guide.contains(r#"`"retryable": true`"#));
}
