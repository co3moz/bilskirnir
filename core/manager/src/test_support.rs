//! Fixtures the tests of several modules share: an installation without Docker on an in-memory
//! database, its root MCP channel and a request to it, a container's settings, and a Docker CLI
//! stand-in that answers as a manager with a data volume of its own.
use crate::{
    App, config, db, desktop_limits, desktop_mcp, events, human, layers, model, outbound, releases,
    router, transfers, usage,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

/// The repository's desktop build context, with the seccomp profile a parsed configuration looks
/// for. `cargo test` runs in the crate's folder, two levels below the repository root.
pub(crate) const DESKTOP_CONTEXT: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../images/desktop");

impl config::Config {
    /// An installation as the tests start from it: sign-in off, every feature allowed, the GPU
    /// routes of WSL, no Docker CLI that runs and no manager container.
    pub(crate) fn for_tests() -> Self {
        Self {
            listen: "127.0.0.1:0".into(),
            data: "data".into(),
            web: "core/web/dist".into(),
            docker_bin: "definitely-not-a-docker-command".into(),
            docker_socket: None,
            manager: None,
            base_image: "ghcr.io/co3moz/thoric/ubuntu-xfce:24.04".into(),
            desktop_context: DESKTOP_CONTEXT.into(),
            auth: false,
            username: "admin".into(),
            password_hash: None,
            secure_cookie: false,
            audio: true,
            microphone: true,
            internet: true,
            sudo: true,
            gamepad: true,
            vpn: true,
            defaults: config::Features {
                gpu: true,
                internet: true,
                sudo: true,
                ..Default::default()
            },
            wsl_gpu: true,
            cuda: true,
            dri_gpu: true,
            adapter: String::new(),
            gpu_host: Default::default(),
            // Docker's own default.
            log_driver: Some("json-file".into()),
            user_namespaces: true,
            pids_limit: desktop_limits::DEFAULT_PIDS_LIMIT,
            allowed_hosts: vec![],
            desktop_host: None,
            upload_limit: 1024 * 1024,
            archive_limit: 4 * 1024 * 1024,
            root_mcp_enabled: false,
            mcp_token: Some(String::new()),
            templates_url: None,
            volume_filter: None,
            proxy: None,
            update_check: false,
            update_dir: "update".into(),
            notices: Vec::new(),
            environment: Default::default(),
        }
    }
}

/// An installation on [`config::Config::for_tests`] and an in-memory database, with sign-in on
/// when `auth` is.
pub(crate) fn app(auth: bool) -> Arc<App> {
    Arc::new(App {
        config: config::Config {
            auth,
            ..config::Config::for_tests()
        },
        db: db::Db::open(std::path::Path::new(":memory:")).unwrap(),
        sessions: Mutex::new(HashMap::new()),
        login_attempts: Default::default(),
        https_proxy: Default::default(),
        operations: Arc::new(tokio::sync::Mutex::new(())),
        resource_locks: Mutex::new(HashMap::new()),
        session_events: tokio::sync::Notify::new(),
        screenshots: tokio::sync::Semaphore::new(2),
        uploads: tokio::sync::Semaphore::new(2),
        desktop_inputs: tokio::sync::Semaphore::new(4),
        archives: Arc::new(tokio::sync::Semaphore::new(2)),
        usage: usage::Usage::default(),
        transfers: transfers::Links::default(),
        layers: layers::Cache::default(),
        desktop_mcp: desktop_mcp::DesktopMcp::default(),
        releases: releases::Releases::default(),
        events: events::Hub::default(),
        human: human::Requests::default(),
        outbound: outbound::Outbound::default(),
    })
}
/// A container's settings that name only its name and image.
pub(crate) fn spec() -> model::ContainerSpec {
    serde_json::from_value(
        serde_json::json!({"name":"Unity","image":"ghcr.io/co3moz/thoric/ubuntu-xfce:24.04"}),
    )
    .unwrap()
}

/// [`app`] with sign-in on and the root MCP channel answering a token of its own.
pub(crate) fn mcp_app() -> Arc<App> {
    let mut app = app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.root_mcp_enabled = true;
    config.mcp_token = Some("test-token-with-at-least-32-characters".into());
    app
}
/// A JSON-RPC request to `/mcp` with [`mcp_app`]'s token, answered without a known length.
pub(crate) async fn mcp_request(
    app: Arc<App>,
    method: &str,
    params: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = router(app)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp")
                .header("host", "localhost:8080")
                .header(
                    "authorization",
                    "Bearer test-token-with-at-least-32-characters",
                )
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25")
                .body(Body::from(
                    serde_json::json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    assert!(!response.headers().contains_key("content-length"));
    assert_eq!(
        axum::body::HttpBody::size_hint(response.body()).exact(),
        None,
        "MCP replies must not have a known length; see mcp_replies_are_chunked_on_the_wire"
    );
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

/// A Docker CLI stand-in for a manager named `thoric-manager` whose data volume is
/// `thoric-data`. It answers the manager's inspect, lists no containers, lists that volume
/// and `project` as `GET /api/volumes` asks for them, or only `project` under the default
/// `VOLUME_FILTER`, inspects `project`, which carries the manager's label, and `outside`, which
/// carries none, fails everything else and records its arguments in the returned file.
#[cfg(unix)]
pub(crate) fn manager_docker(app: &mut Arc<App>) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("thoric-docker-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let calls = dir.join("calls");
    let manager = serde_json::json!([{
        "Id": "manager",
        "Image": "sha256:manager",
        "Config": {"Labels": {"thoric.manager": "true"}},
        "Mounts": [{"Type": "volume", "Name": "thoric-data", "Destination": "/data"}],
    }]);
    let docker = dir.join("docker");
    std::fs::write(
        &docker,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{calls}'\ncase \"$*\" in\n  'container inspect thoric-manager') echo '{manager}' ;;\n  'ps -a'*) ;;\n  'volume ls --format {{{{json .}}}}') echo '{data}'; echo '{project}' ;;\n  'volume ls --filter label=thoric.managed=true --format {{{{json .}}}}') echo '{project}' ;;\n  'volume inspect project') echo '{labeled}' ;;\n  'volume inspect outside') echo '{outside}' ;;\n  *) exit 1 ;;\nesac\n",
            calls = calls.display(),
            data = serde_json::json!({"Name": "thoric-data", "Driver": "local"}),
            project = serde_json::json!({"Name": "project", "Driver": "local"}),
            labeled = serde_json::json!([
                {"Name": "project", "Labels": {"thoric.managed": "true"}}
            ]),
            outside = serde_json::json!([{"Name": "outside", "Labels": null}]),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = &mut Arc::get_mut(app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.manager = Some("thoric-manager".into());
    calls
}
