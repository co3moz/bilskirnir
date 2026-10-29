//! Pins down the readiness check: ready only while the database answers and Docker does, through
//! its Engine socket or its CLI, and an answer within the limit when Docker hangs.
use super::*;
use axum::{body::Body, http::Request};
use serde_json::Value;
use tower::ServiceExt;

async fn readiness(app: Arc<App>) -> (StatusCode, Value) {
    let response = crate::router(app)
        .oneshot(
            Request::get("/readyz")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn without_docker_the_manager_is_not_ready() {
    // The test manager's Docker CLI does not exist.
    let (status, answer) = readiness(crate::tests::app(true)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        answer,
        json!({"ready": false, "database": true, "docker": false})
    );
}

/// A manager whose Docker CLI is `script`, in a new directory of its own.
#[cfg(unix)]
fn with_cli(script: &str) -> (Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("thoric-health-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    std::fs::write(&docker, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(true);
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    (app, dir)
}

#[cfg(unix)]
#[tokio::test]
async fn docker_answers_through_its_cli() {
    let (app, dir) =
        with_cli("[ \"$*\" = 'version --format {{.Server.Version}}' ] && echo 29.0.0 || exit 1");
    let (status, answer) = readiness(app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        answer,
        json!({"ready": true, "database": true, "docker": true})
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_docker_that_hangs_is_not_waited_for() {
    let (app, dir) = with_cli("exec sleep 30");
    let started = std::time::Instant::now();
    let (status, answer) = readiness(app).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(answer["docker"], false);
    assert!(started.elapsed() < LIMIT + Duration::from_secs(2));
    std::fs::remove_dir_all(dir).unwrap();
}

/// An Engine socket in a new directory that answers each connection's first request with
/// `answer`, or never answers when it is `None`.
#[cfg(unix)]
fn engine(answer: Option<&'static str>) -> std::path::PathBuf {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = std::env::temp_dir().join(format!("thoric-health-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let listener = tokio::net::UnixListener::bind(dir.join("docker.sock")).unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut request = [0; 1024];
                let read = stream.read(&mut request).await.unwrap_or_default();
                assert!(request[..read].starts_with(b"GET /_ping HTTP/1.1\r\n"));
                match answer {
                    Some(answer) => {
                        let _ = stream.write_all(answer.as_bytes()).await;
                    }
                    None => std::future::pending::<()>().await,
                }
            });
        }
    });
    dir
}

#[cfg(unix)]
#[tokio::test]
async fn docker_answers_through_its_socket() {
    let dir = engine(Some("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK"));
    let mut app = crate::tests::app(true);
    Arc::get_mut(&mut app).unwrap().config.docker_socket = Some(dir.join("docker.sock"));
    assert!(docker(&app).await);

    let refusing = engine(Some(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n",
    ));
    Arc::get_mut(&mut app).unwrap().config.docker_socket = Some(refusing.join("docker.sock"));
    assert!(!docker(&app).await);

    let silent = engine(None);
    Arc::get_mut(&mut app).unwrap().config.docker_socket = Some(silent.join("docker.sock"));
    let started = std::time::Instant::now();
    assert!(!docker(&app).await);
    assert!(started.elapsed() < LIMIT + Duration::from_secs(2));

    Arc::get_mut(&mut app).unwrap().config.docker_socket = Some(dir.join("missing.sock"));
    assert!(!docker(&app).await);
    for dir in [dir, refusing, silent] {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn the_api_guide_lists_both_checks() {
    let api = include_str!("../../../docs/API.md");
    for route in ["| GET | `/healthz` |", "| GET | `/readyz` |"] {
        assert!(api.contains(route), "docs/API.md must list {route}");
    }
}
