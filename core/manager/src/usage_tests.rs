//! Pins down what the usage routes and the `get_usage` tool return: only containers this manager
//! has or had a record of, a deleted one until its samples age out, only samples after the
//! cursor, the GPU as the host's, and summaries whose units are in their names. Also that both
//! routes are served and documented.
use super::*;
use crate::model::ContainerSpec;
use axum::{body::Body, http::Request};
use history::GpuState;
use tower::ServiceExt;

fn record(id: &str, cpus: Option<f64>) -> Container {
    Container {
        id: id.into(),
        container: format!("bilskirnir-{id}"),
        private_network: format!("bilskirnir-{id}-private"),
        egress_network: format!("bilskirnir-{id}-egress"),
        source_image_id: "sha256:base".into(),
        created_at: 1,
        spec: serde_json::from_value::<ContainerSpec>(
            json!({"name": format!("Desktop {id}"), "image": "bilskirnir/ubuntu-xfce:24.04", "cpus": cpus}),
        )
        .unwrap(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

fn measure(cpu: f32, net_rx: f32) -> stats::Measure {
    stats::Measure {
        cpu,
        memory: 256 << 20,
        memory_limit: 2 << 30,
        online_cpus: 8,
        disk_read: 0.0,
        disk_write: 0.0,
        net_rx,
        net_tx: 0.0,
    }
}

#[tokio::test]
async fn the_list_holds_known_containers_after_the_cursor() {
    let app = crate::tests::app(false);
    app.db
        .put(CONTAINERS, "a", &record("a", Some(2.0)))
        .unwrap();
    app.usage.record("a", &measure(0.5, 100.0));
    // A container another manager on the same Docker owns is not listed.
    app.usage.record("foreign", &measure(1.0, 0.0));
    let Json(first) = all(State(app.clone()), Query(Since { since: 0 }))
        .await
        .unwrap();
    assert_eq!(first["available"], true);
    assert_eq!(first["window_ms"], 1_800_000);
    let containers = first["containers"].as_array().unwrap();
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0]["id"], "a");
    assert_eq!(containers[0]["name"], "Desktop a");
    assert_eq!(containers[0]["cpu_limit"], 2.0);
    assert_eq!(containers[0]["memory_limit"], 2u64 << 30);
    assert_eq!(containers[0]["samples"][0]["cpu"], 0.5);
    assert_eq!(first["gpu"]["scope"], "host");
    assert_eq!(first["gpu"]["state"], "idle");

    app.usage.record("a", &measure(0.25, 0.0));
    let cursor = first["cursor"].as_u64().unwrap();
    let Json(next) = all(State(app.clone()), Query(Since { since: cursor }))
        .await
        .unwrap();
    let samples = next["containers"][0]["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0]["cpu"], 0.25);
    assert!(next["cursor"].as_u64().unwrap() > cursor);
    // Nothing new: the container is still listed, so a client keeps its history.
    let Json(idle) = all(
        State(app.clone()),
        Query(Since {
            since: next["cursor"].as_u64().unwrap(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(idle["containers"][0]["samples"], json!([]));
}

#[tokio::test]
async fn one_container_is_found_by_its_record() {
    let app = crate::tests::app(false);
    app.db.put(CONTAINERS, "a", &record("a", None)).unwrap();
    let Json(empty) = container(
        State(app.clone()),
        Path("a".into()),
        Query(Since { since: 0 }),
    )
    .await
    .unwrap();
    assert_eq!(empty["samples"], json!([]));
    assert_eq!(empty["memory_limit"], Value::Null);
    assert_eq!(
        empty.get("gpu"),
        None,
        "the GPU is the host's, not a container's"
    );
    app.usage.record("a", &measure(1.0, 0.0));
    let Json(one) = container(
        State(app.clone()),
        Path("a".into()),
        Query(Since { since: 0 }),
    )
    .await
    .unwrap();
    assert_eq!(one["id"], "a");
    assert_eq!(one["online_cpus"], 8);
    assert_eq!(one["samples"].as_array().unwrap().len(), 1);
    let missing = container(State(app), Path("gone".into()), Query(Since { since: 0 }))
        .await
        .unwrap_err();
    assert_eq!(missing.0, axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_deleted_container_stays_listed_until_its_samples_age_out() {
    let app = crate::tests::app(false);
    app.db
        .put(CONTAINERS, "a", &record("a", Some(2.0)))
        .unwrap();
    app.db
        .put(CONTAINERS, "b", &record("b", Some(1.0)))
        .unwrap();
    app.usage.record("a", &measure(0.5, 100.0));
    app.usage.record("b", &measure(1.0, 0.0));
    // The collector labels each series with its record before b is deleted.
    let records: Vec<Container> = app.db.list(CONTAINERS).unwrap();
    app.usage.prune(&records);
    app.db.remove(CONTAINERS, "b").unwrap();
    app.usage
        .prune(&app.db.list::<Container>(CONTAINERS).unwrap());

    let Json(body) = all(State(app.clone()), Query(Since { since: 0 }))
        .await
        .unwrap();
    let containers = body["containers"].as_array().unwrap();
    assert_eq!(containers.len(), 2);
    assert_eq!(containers[0]["id"], "a");
    assert_eq!(containers[0]["removed"], false);
    assert_eq!(containers[1]["id"], "b");
    assert_eq!(containers[1]["removed"], true);
    assert_eq!(containers[1]["name"], "Desktop b");
    assert_eq!(containers[1]["cpu_limit"], 1.0);
    assert_eq!(containers[1]["samples"][0]["cpu"], 1.0);

    let Json(one) = container(
        State(app.clone()),
        Path("b".into()),
        Query(Since { since: 0 }),
    )
    .await
    .unwrap();
    assert_eq!(one["removed"], true);
    assert_eq!(one["samples"].as_array().unwrap().len(), 1);
    let listed = summary(&app, None, &Summary { seconds: None }).unwrap();
    assert_eq!(listed["containers"][1]["removed"], true);
    let alone = summary(&app, Some("b"), &Summary { seconds: None }).unwrap();
    assert_eq!(alone["containers"][0]["name"], "Desktop b");

    // Once its samples age out, it is gone, while a keeps its empty entry as a record.
    app.usage
        .store()
        .prune(now_ms() + WINDOW_MS + 1000, |_| None);
    let Json(body) = all(State(app.clone()), Query(Since { since: 0 }))
        .await
        .unwrap();
    assert_eq!(body["containers"], json!([]));
    let gone = container(
        State(app.clone()),
        Path("b".into()),
        Query(Since { since: 0 }),
    )
    .await
    .unwrap_err();
    assert_eq!(gone.0, axum::http::StatusCode::NOT_FOUND);
    assert!(summary(&app, Some("b"), &Summary { seconds: None }).is_err());
    let Json(kept) = container(State(app), Path("a".into()), Query(Since { since: 0 }))
        .await
        .unwrap();
    assert_eq!(kept["removed"], false);
    assert_eq!(kept["samples"], json!([]));
}

#[tokio::test]
async fn an_unreachable_docker_is_reported_beside_the_history() {
    let app = crate::tests::app(false);
    app.usage.set_error(Some(
        "Docker Engine socket /var/run/docker.sock: refused".into(),
    ));
    app.usage
        .set_gpu(GpuState::Unavailable, Some("No devices were found".into()));
    let Json(body) = all(State(app), Query(Since { since: 0 })).await.unwrap();
    assert_eq!(body["available"], false);
    assert!(body["error"].as_str().unwrap().contains("refused"));
    assert_eq!(body["gpu"]["state"], "unavailable");
    assert_eq!(body["gpu"]["reason"], "No devices were found");
}

#[test]
fn a_summary_gives_latest_average_and_peak() {
    assert_eq!(
        aggregate([1.0, 3.0, 2.0].into_iter(), 3),
        json!({"latest": 2.0, "average": 2.0, "peak": 3.0})
    );
    assert_eq!(
        aggregate([1000.4, 2000.6].into_iter(), 0),
        json!({"latest": 2001, "average": 1501, "peak": 2001})
    );
    assert_eq!(
        aggregate([0.1234, 0.2].into_iter(), 3),
        json!({"latest": 0.2, "average": 0.162, "peak": 0.2})
    );
    assert_eq!(aggregate(std::iter::empty(), 0), Value::Null);
}

#[tokio::test]
async fn the_tool_summarizes_recent_seconds() {
    let app = crate::tests::app(false);
    app.db
        .put(CONTAINERS, "a", &record("a", Some(1.5)))
        .unwrap();
    app.db.put(CONTAINERS, "b", &record("b", None)).unwrap();
    app.usage.record("a", &measure(0.5, 1000.0));
    app.usage.record("a", &measure(1.5, 3000.0));
    let body = summary(&app, None, &Summary { seconds: None }).unwrap();
    assert_eq!(body["seconds"], 60);
    // b has no samples yet, so it is left out.
    let containers = body["containers"].as_array().unwrap();
    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0]["samples"], 2);
    assert_eq!(containers[0]["cpu_limit"], 1.5);
    assert_eq!(
        containers[0]["cpu_cores"],
        json!({"latest": 1.5, "average": 1.0, "peak": 1.5})
    );
    assert_eq!(
        containers[0]["network_rx_bytes_per_second"]["average"],
        2000
    );
    assert_eq!(containers[0]["memory_bytes"]["peak"], 256 << 20);
    assert_eq!(body["gpu"]["devices"], json!([]));

    let one = summary(
        &app,
        Some("a"),
        &Summary {
            seconds: Some(1800),
        },
    )
    .unwrap();
    assert_eq!(one["containers"].as_array().unwrap().len(), 1);
    assert!(summary(&app, Some("gone"), &Summary { seconds: None }).is_err());
    for seconds in [0, 1801] {
        assert!(
            summary(
                &app,
                None,
                &Summary {
                    seconds: Some(seconds)
                }
            )
            .is_err()
        );
    }
}

#[tokio::test]
async fn both_routes_are_served_and_documented() {
    let app = crate::tests::app(false);
    app.db.put(CONTAINERS, "a", &record("a", None)).unwrap();
    for path in [
        "/api/usage",
        "/api/usage?since=5",
        "/api/containers/a/usage",
    ] {
        let response = crate::router(app.clone())
            .oneshot(
                Request::get(path)
                    .header("host", "localhost:8080")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK, "{path}");
    }
    let guide = include_str!("../../../docs/API.md");
    for documented in ["`/api/usage`", "`/api/containers/{id}/usage`"] {
        assert!(
            guide.contains(documented),
            "docs/API.md must document {documented}"
        );
    }
}
