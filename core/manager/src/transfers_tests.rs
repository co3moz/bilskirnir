//! Transfer links: a link is random, bound to one operation with its arguments, spent by the
//! first request that presents it, expires, is accepted by the transfer route only, and runs the
//! route's own handler without a session that could end it.
use super::*;
use crate::test_support::{app, mcp_app, mcp_request};
use axum::http::Request;
use tower::ServiceExt;

const NOTES: &str = "/home/desktop/Downloads/notes.txt";

fn download(path: &str) -> Operation {
    Operation::ContainerDownload {
        container_id: "e".into(),
        path: path.into(),
    }
}

/// The container `e` as the manager records it, running in Docker as `thoric-e`.
fn container(app: &App) {
    let env = model::Container {
        id: "e".into(),
        container: "thoric-e".into(),
        private_network: "thoric-e-private".into(),
        egress_network: "thoric-e-egress".into(),
        source_image_id: "sha256:desktop".into(),
        created_at: 1,
        spec: serde_json::from_value(json!({"name": "Work", "image": "example/desktop:1"}))
            .unwrap(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, "e", &env).unwrap();
}

fn request(method: &str, uri: &str) -> axum::http::request::Builder {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "localhost:8080")
}

async fn send(app: &Arc<App>, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = crate::router(app.clone()).oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, headers, body.to_vec())
}

fn error(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body).unwrap()["error"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn links_are_random_and_spent_by_their_first_request() {
    let links = Links::default();
    let now = Instant::now();
    let first = links.issue(download(NOTES), now).unwrap();
    let second = links.issue(download(NOTES), now).unwrap();
    assert_ne!(first, second);
    // Two v4 UUIDs: 244 random bits, well over 128.
    assert_eq!(first.len(), 64);
    assert!(first.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(links.take(&first, now), Some(download(NOTES)));
    assert_eq!(links.take(&first, now), None);
    // A link is spent when its lifetime is over, and stays spent.
    let late = now + LIFETIME;
    assert_eq!(links.take(&second, late), None);
    assert_eq!(links.take(&second, now), None);
    let third = links.issue(download(NOTES), now).unwrap();
    assert_eq!(
        links.take(&third, late - Duration::from_secs(1)),
        Some(download(NOTES))
    );
}

#[test]
fn unused_links_are_bounded_and_expired_ones_make_room() {
    let links = Links::default();
    let now = Instant::now();
    for _ in 0..LIMIT {
        links.issue(download(NOTES), now).unwrap();
    }
    let refused = links.issue(download(NOTES), now).unwrap_err();
    assert_eq!(refused.0, StatusCode::TOO_MANY_REQUESTS);
    links.issue(download(NOTES), now + LIFETIME).unwrap();
}

#[test]
fn an_operation_takes_its_own_arguments_only() {
    let parse = |value: Value| serde_json::from_value::<Operation>(value);
    assert_eq!(
        parse(json!({"operation": "volume_import", "name": "project"})).unwrap(),
        Operation::VolumeImport {
            name: "project".into()
        }
    );
    assert_eq!(
        parse(json!({"operation": "container_upload", "container_id": "e", "destination": "/home/desktop/Downloads", "name": "a.txt"})).unwrap(),
        Operation::ContainerUpload {
            container_id: "e".into(),
            destination: "/home/desktop/Downloads".into(),
            name: "a.txt".into(),
            replace: false
        }
    );
    assert_eq!(
        parse(json!({"operation": "volume_extract", "volume": "project", "path": "/worktree"}))
            .unwrap(),
        Operation::VolumeExtract {
            volume: "project".into(),
            path: "/worktree".into()
        }
    );
    // Another operation's argument is refused, not ignored, as are missing and unknown ones.
    for refused in [
        json!({"operation": "volume_import", "name": "project", "path": "/a"}),
        json!({"operation": "volume_extract", "name": "project", "path": "/a"}),
        json!({"operation": "volume_extract", "volume": "project"}),
        json!({"operation": "volume_export", "volume": "project", "container_id": "e"}),
        json!({"operation": "volume_export"}),
        json!({"operation": "delete_volume", "volume": "project"}),
        json!({"volume": "project"}),
    ] {
        assert!(parse(refused.clone()).is_err(), "{refused}");
    }
}

#[test]
fn a_link_is_issued_only_for_arguments_its_route_could_accept() {
    let app = app(true);
    container(&app);
    let upload = |destination: &str, name: &str| Operation::ContainerUpload {
        container_id: "e".into(),
        destination: destination.into(),
        name: name.into(),
        replace: false,
    };
    let refused = |operation: Operation| create(&app, operation).unwrap_err().0;
    assert_eq!(
        refused(Operation::ContainerDownload {
            container_id: "missing".into(),
            path: NOTES.into()
        }),
        StatusCode::NOT_FOUND
    );
    assert_eq!(refused(upload("/etc", "passwd")), StatusCode::BAD_REQUEST);
    assert_eq!(
        refused(upload("/home/desktop/Downloads", "../x")),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(refused(download("relative")), StatusCode::BAD_REQUEST);
    assert_eq!(
        refused(Operation::VolumeImport { name: "x".into() }),
        StatusCode::BAD_REQUEST
    );
    let extract = |volume: &str, path: &str| Operation::VolumeExtract {
        volume: volume.into(),
        path: path.into(),
    };
    for (volume, path) in [
        ("x", "/worktree"),
        ("project", "worktree"),
        ("project", "/a/../b"),
    ] {
        assert_eq!(
            refused(extract(volume, path)),
            StatusCode::BAD_REQUEST,
            "{volume} {path}"
        );
    }
    assert_eq!(
        create(&app, extract("project", "/worktree")).unwrap()["method"],
        "PUT"
    );
    assert_eq!(
        refused(Operation::ImageExport {
            image_id: "missing".into()
        }),
        StatusCode::NOT_FOUND
    );
    let issued = create(&app, upload("/home/desktop/Downloads", "notes.txt")).unwrap();
    let token = issued["url"].as_str().unwrap().strip_prefix(ROUTE).unwrap();
    assert_eq!(issued["method"], "PUT");
    assert_eq!(issued["operation"], "container_upload");
    assert_eq!(issued["expires_in_seconds"], LIFETIME.as_secs());
    assert!(issued["expires_at"].as_u64().unwrap() >= model::now() + 299);
    assert_eq!(
        app.transfers.take(token, Instant::now()),
        Some(upload("/home/desktop/Downloads", "notes.txt"))
    );
    let issued = create(&app, download(NOTES)).unwrap();
    assert_eq!(issued["method"], "GET");
}

#[tokio::test]
async fn create_transfer_binds_the_container_and_the_operation() {
    let app = mcp_app();
    container(&app);
    let (status, reply) = mcp_request(
        app.clone(),
        "tools/call",
        json!({"name": "create_transfer", "arguments": {"operation": "container_download", "container_id": "e", "path": NOTES}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let result = &reply["result"];
    assert_ne!(result["isError"], true, "{reply}");
    let url = result["structuredContent"]["url"].as_str().unwrap();
    // The dispatcher takes container_id out of every call; the link still names the container.
    assert_eq!(
        app.transfers
            .take(url.strip_prefix(ROUTE).unwrap(), Instant::now()),
        Some(download(NOTES))
    );
    let (_, reply) = mcp_request(
        app.clone(),
        "tools/call",
        json!({"name": "create_transfer", "arguments": {"operation": "volume_export", "volume": "project", "path": NOTES}}),
    )
    .await;
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    assert!(
        reply.to_string().contains("unknown field `path`"),
        "{reply}"
    );
}

#[tokio::test]
async fn without_mcp_the_route_answers_nothing() {
    let app = app(true);
    let token = app
        .transfers
        .issue(download(NOTES), Instant::now())
        .unwrap();
    let (status, _, _) = send(
        &app,
        request("GET", &format!("{ROUTE}{token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(app.transfers.take(&token, Instant::now()).is_some());
}

#[tokio::test]
async fn a_refused_request_spends_its_link_without_naming_it() {
    let app = mcp_app();
    let issue = |operation: Operation| {
        format!(
            "{ROUTE}{}",
            app.transfers.issue(operation, Instant::now()).unwrap()
        )
    };
    let used = |app: Arc<App>, url: String| async move {
        let (status, _, body) = send(&app, request("GET", &url).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let message = error(&body);
        assert!(message.contains("unknown, used or expired"), "{message}");
        assert!(!message.contains(url.strip_prefix(ROUTE).unwrap()));
    };
    // A download link is no upload link.
    let url = issue(download(NOTES));
    let (status, headers, _) = send(
        &app,
        request("POST", &url)
            .header("content-length", "5")
            .body(Body::from("hello"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(headers["allow"], "GET");
    used(app.clone(), url).await;
    // The request adds an upload's size and nothing else: the arguments are the link's.
    let url = issue(Operation::VolumeImport {
        name: "project".into(),
    });
    let (status, _, body) = send(
        &app,
        request("PUT", &format!("{url}?name=other&size=5"))
            .body(Body::from("hello"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{}", error(&body));
    used(app.clone(), url).await;
    let url = issue(download(NOTES));
    let (status, _, _) = send(
        &app,
        request("GET", &format!("{url}?size=5"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    used(app.clone(), url).await;
    // A file upload needs its size; an archive import also streams in without one.
    let url = issue(Operation::ContainerUpload {
        container_id: "e".into(),
        destination: "/home/desktop/Downloads".into(),
        name: "a.txt".into(),
        replace: false,
    });
    let (status, _, _) = send(
        &app,
        request("PUT", &url).body(Body::from("hello")).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::LENGTH_REQUIRED);
    used(app.clone(), url).await;
    let url = issue(Operation::VolumeImport {
        name: "project".into(),
    });
    let (status, _, _) = send(
        &app,
        request("PUT", &url).body(Body::from("hello")).unwrap(),
    )
    .await;
    assert_ne!(status, StatusCode::LENGTH_REQUIRED);
    used(app.clone(), url).await;
    used(app.clone(), format!("{ROUTE}{}", "0".repeat(64))).await;
}

#[tokio::test]
async fn an_extract_link_streams_into_the_extract_route() {
    // An archive of unknown size reaches the route, whose own refusal comes back: without
    // MANAGER_CONTAINER no helper can mount the volume, so nothing is created and no job runs.
    let app = mcp_app();
    let operation = Operation::VolumeExtract {
        volume: "project".into(),
        path: "/worktree".into(),
    };
    let token = app.transfers.issue(operation, Instant::now()).unwrap();
    let (status, _, body) = send(
        &app,
        request("PUT", &format!("{ROUTE}{token}"))
            .body(Body::from("archive"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{}", error(&body));
    assert!(error(&body).contains("MANAGER_CONTAINER"));
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
    assert!(app.transfers.take(&token, Instant::now()).is_none());
}

#[tokio::test]
async fn a_cross_origin_request_is_refused_and_leaves_the_link() {
    let app = mcp_app();
    let token = app
        .transfers
        .issue(download(NOTES), Instant::now())
        .unwrap();
    let (status, _, _) = send(
        &app,
        request("GET", &format!("{ROUTE}{token}"))
            .header("origin", "http://attacker.example")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(app.transfers.take(&token, Instant::now()).is_some());
}

#[tokio::test]
async fn a_link_runs_its_routes_handler_with_the_bound_arguments() {
    let app = mcp_app();
    // The handlers' own checks answer, so the bound path and name reached them, and the body
    // counts as binary whatever its label.
    let token = app
        .transfers
        .issue(download("relative"), Instant::now())
        .unwrap();
    let (status, _, body) = send(
        &app,
        request("GET", &format!("{ROUTE}{token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error(&body).contains("canonical absolute file path"));
    let token = app
        .transfers
        .issue(Operation::VolumeImport { name: "x".into() }, Instant::now())
        .unwrap();
    let (status, _, body) = send(
        &app,
        request("POST", &format!("{ROUTE}{token}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .header("content-length", "5")
            .body(Body::from("hello"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error(&body), "Invalid volume name");
}

#[tokio::test]
async fn a_link_is_neither_a_session_nor_a_bearer_token() {
    let app = mcp_app();
    let token = app
        .transfers
        .issue(download(NOTES), Instant::now())
        .unwrap();
    let (status, _, _) = send(
        &app,
        request("GET", "/api/containers")
            .header("cookie", format!("thoric_session={token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send(
        &app,
        request("POST", "/mcp")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(Body::from(
                json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(app.transfers.take(&token, Instant::now()).is_some());
}

#[tokio::test]
async fn no_session_ends_a_link_transfer() {
    // Without a cookie, a signed-in installation ends a transfer at once: a link must say what
    // it is instead of passing no session.
    let app = app(true);
    tokio::time::timeout(
        Duration::from_millis(100),
        app.session_revoked(Session::Cookie(None)),
    )
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            app.session_revoked(Session::Link)
        )
        .await
        .is_err()
    );
}

#[test]
fn every_operation_is_offered_and_documented() {
    let mcp = include_str!("../../../docs/MCP.md");
    let api = include_str!("../../../docs/API.md");
    for name in OPERATIONS {
        // A name the enum does not know is refused as an unknown variant, not a missing argument.
        let error = serde_json::from_value::<Operation>(json!({"operation": name}))
            .err()
            .map(|error| error.to_string());
        assert!(
            error
                .as_deref()
                .is_none_or(|error| error.contains("missing field")),
            "{name}: {error:?}"
        );
        assert!(
            mcp.contains(&format!("| `{name}` |")),
            "docs/MCP.md must list {name}"
        );
    }
    assert!(api.contains("`/api/transfers/{token}`"));
}

/// A running container `e` behind a stand-in Docker CLI: an upload is written to `uploaded`,
/// a download's `file_ops.py stat` probe finds a five-byte file and its stream reads `hello`.
#[cfg(unix)]
fn docker(app: &mut Arc<App>) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("thoric-transfers-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    crate::test_support::write_script(
        &docker,
        &format!(
            "#!/bin/sh\ncase \"$1 $2\" in\n  'container inspect') echo '[{{\"Id\":\"c0ffee\",\"Config\":{{\"Labels\":{{\"thoric.managed\":\"true\",\"thoric.environment\":\"e\"}}}},\"State\":{{\"Running\":true}},\"Mounts\":[]}}]' ;;\n  'exec --interactive') cat > '{dir}/uploaded' ;;\n  'exec --user') if [ \"$8\" = stat ]; then echo '{{\"type\":\"file\",\"size\":5}}'; else printf hello; fi ;;\n  *) exit 1 ;;\nesac\n",
            dir = dir.display()
        ),
    );
    // An upload is staged under the data folder before it is handed to the desktop.
    std::fs::create_dir_all(dir.join("uploads")).unwrap();
    let config = &mut Arc::get_mut(app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.data = dir.clone();
    container(app);
    dir
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_moves_a_file_in_and_out_without_a_session() {
    let mut app = mcp_app();
    let dir = docker(&mut app);
    let issued = create(
        &app,
        Operation::ContainerUpload {
            container_id: "e".into(),
            destination: "/home/desktop/Downloads".into(),
            name: "notes.txt".into(),
            replace: false,
        },
    )
    .unwrap();
    let url = issued["url"].as_str().unwrap().to_owned();
    let (status, _, body) = send(
        &app,
        request("PUT", &url)
            .header("content-length", "5")
            .body(Body::from("hello"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let saved: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(saved["path"], NOTES);
    assert_eq!(saved["bytes"], 5);
    assert_eq!(std::fs::read(dir.join("uploaded")).unwrap(), b"hello");
    let (status, _, _) = send(
        &app,
        request("PUT", &url)
            .header("content-length", "5")
            .body(Body::from("again"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let issued = create(&app, download(NOTES)).unwrap();
    let url = issued["url"].as_str().unwrap();
    let (status, headers, body) =
        send(&app, request("GET", url).body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-length"], "5");
    assert_eq!(body, b"hello");
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn a_used_link_is_recorded_for_the_caller_that_issued_it() {
    let app = mcp_app();
    container(&app);
    let issuer = activity::Caller {
        endpoint: None,
        credential: activity::Credential::McpToken,
        client: Some("agent 1.0".into()),
    };
    // As create_transfer issues it, inside the tool call of its caller.
    let token = activity::as_caller(issuer, async {
        app.transfers.issue(download("relative"), Instant::now())
    })
    .await
    .unwrap();
    let (status, _, body) = send(
        &app,
        request("GET", &format!("{ROUTE}{token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    // The route's answer passes through the log unchanged.
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let message = error(&body);
    let rows = activity::recorded(&app, 1).await;
    let row = &rows[0];
    assert_eq!(
        (row.source.as_str(), row.tool.as_str()),
        ("transfer", "container_download")
    );
    assert_eq!(
        (row.credential.as_str(), row.client.as_deref()),
        ("link", Some("agent 1.0"))
    );
    assert_eq!(row.endpoint, None);
    assert_eq!(
        (row.container_id.as_deref(), row.container_name.as_deref()),
        (Some("e"), Some("Work"))
    );
    assert_eq!((row.kind.as_str(), row.outcome.as_str()), ("read", "error"));
    assert_eq!(row.error.as_deref(), Some(message.as_str()));
    assert_eq!(
        row.arguments,
        json!({"container_id": "e", "path": "relative"})
    );
    assert!(!serde_json::to_string(row).unwrap().contains(&token));
}
