use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn app(auth: bool) -> Arc<App> {
    Arc::new(App {
        config: config::Config {
            listen: "127.0.0.1:0".into(),
            data: "data".into(),
            web: "web/dist".into(),
            docker_bin: "definitely-not-a-docker-command".into(),
            docker_socket: None,
            manager: None,
            base_image: "wsl-desktop/ubuntu:24.04".into(),
            desktop_context: "desktop".into(),
            auth,
            username: "admin".into(),
            password_hash: None,
            secure_cookie: false,
            audio: true,
            microphone: true,
            wsl_gpu: true,
            cuda: true,
            adapter: String::new(),
            allowed_hosts: vec![],
            upload_limit: 1024 * 1024,
            mcp_enabled: false,
            mcp_token: String::new(),
        },
        db: db::Db::open(std::path::Path::new(":memory:")).unwrap(),
        sessions: Mutex::new(HashMap::new()),
        login_attempts: Mutex::new(Vec::new()),
        operations: Arc::new(tokio::sync::Mutex::new(())),
        resource_locks: Mutex::new(HashMap::new()),
        session_events: tokio::sync::Notify::new(),
        screenshots: tokio::sync::Semaphore::new(2),
        uploads: tokio::sync::Semaphore::new(2),
        desktop_inputs: tokio::sync::Semaphore::new(4),
    })
}
fn spec() -> model::EnvironmentSpec {
    serde_json::from_value(serde_json::json!({"name":"Unity","image":"wsl-desktop/ubuntu:24.04"}))
        .unwrap()
}

fn mcp_app() -> Arc<App> {
    let mut app = app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.mcp_enabled = true;
    config.mcp_token = "test-token-with-at-least-32-characters".into();
    app
}
async fn mcp_request(
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
    if status == StatusCode::OK {
        assert!(
            response.headers().contains_key("content-length"),
            "Completed MCP JSON replies should have an explicit length"
        );
        assert!(!response.headers().contains_key("transfer-encoding"));
    }
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
#[test]
fn mcp_token_and_script_validation() {
    for token in [
        "",
        "too-short",
        "01234567890123456789012345678901 ",
        "01234567890123456789012345678901\n",
    ] {
        assert!(config::validate_mcp_token(true, token).is_err());
    }
    assert!(config::validate_mcp_token(false, "").is_ok());
    assert!(config::validate_mcp_token(true, "01234567890123456789012345678901").is_ok());
    for body in [
        serde_json::json!({"script":""}),
        serde_json::json!({"script":"echo ok","timeout_seconds":0}),
        serde_json::json!({"script":"echo ok","timeout_seconds":3501}),
        serde_json::json!({"script":"echo\0ok"}),
    ] {
        assert!(
            serde_json::from_value::<exec::Execute>(body)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let body: exec::Execute = serde_json::from_value(serde_json::json!({"script":"id"})).unwrap();
    assert!(!body.root);
    assert_eq!(body.timeout_seconds, 300);
    assert!(body.validate().is_ok());
}
#[tokio::test]
async fn mcp_requires_its_own_token_and_rejects_cross_origin_access() {
    for (enabled, token, host, origin, expected) in [
        (false, None, "localhost:8080", None, StatusCode::NOT_FOUND),
        (true, None, "localhost:8080", None, StatusCode::UNAUTHORIZED),
        (
            true,
            Some("Bearer wrong"),
            "localhost:8080",
            None,
            StatusCode::UNAUTHORIZED,
        ),
        (
            true,
            Some("Bearer test-token-with-at-least-32-characters"),
            "evil.example",
            None,
            StatusCode::FORBIDDEN,
        ),
        (
            true,
            Some("Bearer test-token-with-at-least-32-characters"),
            "localhost:8080",
            Some("https://evil.example"),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut app = mcp_app();
        Arc::get_mut(&mut app).unwrap().config.mcp_enabled = enabled;
        let mut req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", host);
        if let Some(token) = token {
            req = req.header("authorization", token);
        }
        if let Some(origin) = origin {
            req = req.header("origin", origin);
        }
        let response = router(app)
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
}
#[tokio::test]
async fn mcp_negotiates_and_lists_tools_with_useful_results() {
    let app = mcp_app();
    let (status, body) = mcp_request(app.clone(), "initialize", serde_json::json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["serverInfo"]["name"], "bilskirnir");
    let instructions = body["result"]["instructions"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let (status, body) = mcp_request(app.clone(), "tools/list", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 59);
    let delete = tools
        .iter()
        .find(|t| t["name"] == "delete_environment")
        .unwrap();
    assert_eq!(delete["annotations"]["destructiveHint"], true);
    let screenshot = tools.iter().find(|t| t["name"] == "screenshot").unwrap();
    assert_eq!(screenshot["annotations"]["readOnlyHint"], true);
    assert!(screenshot["inputSchema"]["properties"]["max_width"].is_object());
    assert!(
        screenshot["description"]
            .as_str()
            .unwrap()
            .contains("save_screenshot")
    );
    let save = tools
        .iter()
        .find(|t| t["name"] == "save_screenshot")
        .unwrap();
    assert_eq!(save["annotations"]["readOnlyHint"], false);
    assert!(save["inputSchema"]["properties"]["capture"].is_object());
    assert_eq!(
        save["inputSchema"]["required"],
        serde_json::json!(["path", "environment_id"])
    );
    for name in ["save_screenshot", "download_file", "export_artifacts"] {
        let tool = tools.iter().find(|t| t["name"] == name).unwrap();
        assert!(
            tool["description"]
                .as_str()
                .unwrap()
                .contains("download_url"),
            "{name} must tell agents how to hand files to the user"
        );
    }
    // Agents reached for docker and wsl on the host when a client hid a returned image. The
    // instructions must steer them to the download link instead, and the guide must document it.
    assert!(instructions.contains("download_url") && instructions.contains("wsl"));
    let guide = include_str!("../docs/MCP.md");
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            guide.contains(&format!("`{name}`")),
            "docs/MCP.md must mention {name}"
        );
    }
    assert!(guide.contains("`download_url`"));
    // The count went stale once already; it is stated in the guide, so it is checked from here.
    assert!(
        guide.contains(&format!("exposes {} tools", tools.len())),
        "docs/MCP.md must state the tool count"
    );
    assert_eq!(
        screenshot["inputSchema"]["required"],
        serde_json::json!(["environment_id"])
    );
    let observe = tools
        .iter()
        .find(|t| t["name"] == "act_and_observe")
        .unwrap();
    assert!(observe["inputSchema"]["properties"]["actions"].is_object());
    assert!(
        observe["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "environment_id")
    );
    let stop = tools
        .iter()
        .find(|t| t["name"] == "stop_application")
        .unwrap();
    assert!(
        stop["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "application_id")
    );
    let (_, body) = mcp_request(
        app.clone(),
        "tools/call",
        serde_json::json!({"name":"get_settings","arguments":{}}),
    )
    .await;
    assert_eq!(body["result"]["isError"], false, "{body}");
    assert!(body["result"]["structuredContent"]["base_image"].is_string());
    assert!(!body.to_string().contains("test-token"));
    let (_, body) = mcp_request(
        app,
        "tools/call",
        serde_json::json!({"name":"delete_environment","arguments":{"environment_id":"missing"}}),
    )
    .await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert!(
        body["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not found")
    );
}
#[tokio::test]
async fn mcp_rejects_invalid_tools_and_arguments_without_mutating() {
    let app = mcp_app();
    for args in [
        serde_json::json!({"name":"not_a_tool","arguments":{}}),
        serde_json::json!({"name":"delete_environment","arguments":{}}),
        serde_json::json!({"name":"create_environment","arguments":{"name":"test","image":"ubuntu","unexpected":true}}),
    ] {
        let (_, body) = mcp_request(app.clone(), "tools/call", args).await;
        assert_eq!(body["error"]["code"], -32602, "{body}");
    }
    let (_,body) = mcp_request(app.clone(),"tools/call",serde_json::json!({"name":"exec_environment","arguments":{"environment_id":"missing","script":"echo ok","timeout_seconds":0}})).await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
    let (_,body)=mcp_request(app.clone(),"tools/call",serde_json::json!({"name":"create_environment","arguments":{"name":"test","image":"ubuntu","mounts":[{"volume":"project","target":"/workspace","read_only":true}]}})).await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
}

#[test]
fn file_paths_and_vnc_actions_reject_invalid_requests() {
    for path in [
        "",
        "/",
        "relative.txt",
        "/home/desktop/../root/file",
        "/home//file",
        "/home/file/",
        "/home/./file",
        "/home/file\0",
    ] {
        assert!(files::split_path(path).is_err(), "{path}");
    }
    assert_eq!(
        files::split_path("/etc/os-release").unwrap(),
        ("/etc", "os-release")
    );
    for body in [
        serde_json::json!({"actions":[]}),
        serde_json::json!({"actions":[{"type":"click","x":800,"y":0}]}),
        serde_json::json!({"actions":[{"type":"click","x":0,"y":0,"count":0}]}),
        serde_json::json!({"actions":[{"type":"drag","from_x":1,"from_y":1,"to_x":2,"to_y":2,"duration_ms":99999}]}),
        serde_json::json!({"actions":[{"type":"key_press","keys":["Control","Ctrl"]}]}),
        serde_json::json!({"actions":[{"type":"wait","milliseconds":20000}]}),
        serde_json::json!({"actions":[{"type":"type_text","text":"\0"}]}),
    ] {
        let input: desktop_input::Input = serde_json::from_value(body).unwrap();
        assert!(desktop_input::plan(&input.actions, 800, 600, None).is_err());
    }
    assert_eq!(desktop_input::keysym("Control").unwrap(), 0xffe3);
    assert_eq!(desktop_input::keysym("Ω").unwrap(), 0x010003a9);
    assert_eq!(desktop_input::keysym("\n").unwrap(), 0xff0d);
    assert!(desktop_input::keysym("F13").is_err());
    let input: desktop_input::Input = serde_json::from_value(serde_json::json!({"actions":[{"type":"key_press","keys":["Control","a"]},{"type":"click","x":20,"y":30}]})).unwrap();
    let plan = desktop_input::plan(&input.actions, 800, 600, None)
        .unwrap()
        .steps;
    assert_eq!(
        plan[0].0,
        [
            vec![4, 1, 0, 0, 0, 0, 255, 227],
            vec![4, 1, 0, 0, 0, 0, 0, 97],
            vec![4, 0, 0, 0, 0, 0, 0, 97],
            vec![4, 0, 0, 0, 0, 0, 255, 227]
        ]
        .concat()
    );
    assert_eq!(plan[1].0, vec![5, 1, 0, 20, 0, 30]);
    assert_eq!(plan[2].0, vec![5, 0, 0, 20, 0, 30]);
}

#[tokio::test]
async fn automation_http_routes_keep_browser_auth_and_csrf_checks() {
    for route in [
        "exec",
        "files/read",
        "files/write",
        "files/list",
        "files/move",
        "files/delete",
        "input",
        "act-and-observe",
        "wait",
        "reset",
        "recordings",
        "recordings/x/stop",
        "artifacts",
        "audio-capture",
        "audio-inject",
        "terminals",
        "terminals/x/input",
        "terminals/x/close",
        "gamepad",
        "applications",
        "applications/x/stop",
        "windows/0x1/focus",
        "windows/0x1/geometry",
    ] {
        for (auth, csrf, status) in [
            (true, true, StatusCode::UNAUTHORIZED),
            (false, false, StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::builder()
                .method("POST")
                .uri(format!("/api/environments/missing/{route}"))
                .header("host", "localhost")
                .header("content-type", "application/json");
            if csrf {
                request = request.header("x-wsld-request", "1");
            }
            let response = router(app(auth))
                .oneshot(request.body(Body::from("{}")).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{route}");
        }
    }
}

#[tokio::test]
async fn single_environment_api_returns_the_same_state_as_the_list() {
    let app = app(false);
    let env = model::Environment {
        id: "test-id".into(),
        container: "test-container".into(),
        private_network: "test-private".into(),
        egress_network: "test-egress".into(),
        source_image_id: "sha256:test".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
    };
    app.db.put("environment", &env.id, &env).unwrap();
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/environments/test-id")
                .header("host", "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["environment"]["id"], "test-id");
    assert_eq!(body["state"], "unavailable");
}

#[tokio::test]
async fn mcp_transport_bounds_messages_and_accepts_notifications() {
    let app = mcp_app();
    for (method, version, payload, expected) in [
        (
            "POST",
            "2025-11-25",
            serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
            StatusCode::ACCEPTED,
        ),
        (
            "GET",
            "2025-11-25",
            String::new(),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "POST",
            "1900-01-01",
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string(),
            StatusCode::BAD_REQUEST,
        ),
        (
            "POST",
            "2025-11-25",
            "x".repeat(1_100_001),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/mcp")
                    .header("host", "localhost")
                    .header(
                        "authorization",
                        "Bearer test-token-with-at-least-32-characters",
                    )
                    .header("content-type", "application/json")
                    .header("accept", "application/json, text/event-stream")
                    .header("mcp-protocol-version", version)
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "{method} {version}");
    }
}

#[test]
fn uploads_only_accept_canonical_paths_in_writable_locations() {
    let mut spec = spec();
    spec.mounts = vec![
        model::Mount {
            volume: "projects".into(),
            target: "/shared".into(),
            readonly: false,
        },
        model::Mount {
            volume: "reference".into(),
            target: "/shared/reference".into(),
            readonly: true,
        },
    ];
    for target in [
        "/home/desktop/Downloads",
        "/home/desktop/Downloads/project",
        "/shared/assets",
    ] {
        assert!(
            uploads::validate_target(&spec, target, "file & notes.txt").is_ok(),
            "{target}"
        );
    }
    for target in [
        "/etc",
        "/sharedish",
        "/shared/reference/sub",
        "/home/desktop/Downloads-fake",
        "/home/desktop/Downloads/../.ssh",
        "/shared//assets",
        "/shared/.",
    ] {
        assert!(
            uploads::validate_target(&spec, target, "file.txt").is_err(),
            "{target}"
        );
    }
    for name in [
        "",
        ".",
        "..",
        "../file",
        "file/name",
        "file\\name",
        "file\0name",
    ] {
        assert!(uploads::validate_target(&spec, "/shared", name).is_err());
    }
    spec.mounts.push(model::Mount {
        volume: "downloads".into(),
        target: "/home/desktop/Downloads".into(),
        readonly: true,
    });
    assert!(uploads::validate_target(&spec, "/home/desktop/Downloads", "file.txt").is_err());
}

#[tokio::test]
async fn uploads_enforce_authentication_origin_and_size_before_docker() {
    let uri =
        "/api/environments/unknown/files?destination=/home/desktop/Downloads&name=test.txt&size=1";
    for (auth, csrf, size, expected) in [
        (true, true, 1, StatusCode::UNAUTHORIZED),
        (false, false, 1, StatusCode::FORBIDDEN),
        (false, true, 1048577, StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let uri = uri.replace("size=1", &format!("size={size}"));
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("host", "localhost:8080")
            .header("content-type", "application/octet-stream");
        if csrf {
            request = request.header("x-wsld-request", "1");
        }
        let response = router(app(auth))
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
#[test]
fn resource_and_mount_validation() {
    let mut s = spec();
    assert!(s.validate().is_ok());
    s.cpus = Some(f64::INFINITY);
    assert!(s.validate().is_err());
    s.cpus = None;
    s.resolution = "0x1080".into();
    assert!(s.validate().is_err());
    s.resolution = "1920x1080".into();
    s.mounts.push(model::Mount {
        volume: "/var/run/docker.sock".into(),
        target: "/shared".into(),
        readonly: false,
    });
    assert!(s.validate().is_err());
    s.mounts[0].volume = "project-data".into();
    s.mounts[0].target = "/proc".into();
    assert!(s.validate().is_err());
    s.mounts[0].target = "/home/desktop/../root".into();
    assert!(s.validate().is_err());
    s.mounts[0].target = "/workspace/project".into();
    assert!(s.validate().is_ok());
    s.mounts.push(s.mounts[0].clone());
    assert!(s.validate().is_err());
}
#[test]
fn mount_aliases_and_readonly_home_cannot_break_desktop_infrastructure() {
    let mut s = spec();
    s.mounts.push(model::Mount {
        volume: "project-data".into(),
        target: String::new(),
        readonly: false,
    });
    for target in [
        "//etc",
        "/./etc",
        "/workspace/../etc",
        "/workspace//data",
        "/lib64",
        "/var/run",
        "/var",
        "/home",
        "/opt/VirtualGL",
        "/etc/",
    ] {
        s.mounts[0].target = target.into();
        assert!(s.validate().is_err(), "{target}");
    }
    s.mounts[0].target = "/opt/game".into();
    assert!(s.validate().is_ok());
    s.mounts[0].target = "/home/desktop".into();
    s.mounts[0].readonly = true;
    assert!(s.validate().is_err());
    s.mounts[0].readonly = false;
    assert!(s.validate().is_ok());
    s.name = "\u{00c7}".repeat(100);
    assert!(s.validate().is_ok());
}
#[tokio::test]
async fn untrusted_host_is_rejected_even_with_matching_origin_and_no_login() {
    let app = app(false);
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/session")
                .header("host", "evil.example:8080")
                .header("origin", "http://evil.example:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[test]
fn host_validation_accepts_local_ips_and_explicit_lan_names() {
    let mut app = app(false);
    Arc::get_mut(&mut app)
        .unwrap()
        .config
        .allowed_hosts
        .push("desktop.lan".into());
    for host in [
        "localhost:8080",
        "127.0.0.1:8088",
        "192.168.1.4:8080",
        "[::1]:8080",
        "[fd00::1]:8080",
        "desktop.lan:8443",
    ] {
        let mut h = axum::http::HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert!(app.host_allowed(&h), "{host}");
    }
    for host in [
        "example.org",
        "localhost.evil.com",
        "1.1.1.1",
        "user@localhost",
        "0.0.0.0",
    ] {
        let mut h = axum::http::HeaderMap::new();
        h.insert("host", host.parse().unwrap());
        assert!(!app.host_allowed(&h), "{host}");
    }
}
#[tokio::test]
async fn logout_revokes_waiting_media_lease_and_expiry_ends_it() {
    let app = app(true);
    let expiry = Instant::now() + std::time::Duration::from_secs(60);
    app.sessions.lock().unwrap().insert("media".into(), expiry);
    let lease = app.session_revoked(Some("media".into()));
    tokio::pin!(lease);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(5), &mut lease)
            .await
            .is_err()
    );
    let response = router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/logout")
                .header("host", "localhost:8080")
                .header("cookie", "wsld_session=media")
                .header("x-wsld-request", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    tokio::time::timeout(std::time::Duration::from_millis(100), lease)
        .await
        .unwrap();
    app.sessions.lock().unwrap().insert(
        "short".into(),
        Instant::now() + std::time::Duration::from_millis(15),
    );
    tokio::time::timeout(
        std::time::Duration::from_millis(100),
        app.session_revoked(Some("short".into())),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn unrelated_jobs_progress_while_an_environment_queue_is_blocked() {
    let app = app(false);
    let lock = app.resource_lock("environment:one");
    let guard = lock.lock().await;
    let queued = jobs::launch(
        app.clone(),
        "stop",
        "environment:one".into(),
        |_, _| async { Ok(serde_json::json!({})) },
    )
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let free = jobs::launch(
        app.clone(),
        "start",
        "environment:two".into(),
        |_, _| async { Ok(serde_json::json!({})) },
    )
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.db.get::<model::Job>("job", &free).unwrap().status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.db.get::<model::Job>("job", &queued).unwrap().status,
        "queued"
    );
    drop(guard);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.db.get::<model::Job>("job", &queued).unwrap().status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[test]
fn job_list_is_bounded_and_strips_logs_before_deserialization() {
    let app = app(false);
    for i in 0..120 {
        let id = i.to_string();
        let j = model::Job {
            id: id.clone(),
            kind: "build".into(),
            resource: None,
            status: "completed".into(),
            created_at: 1,
            finished_at: Some(2),
            log: "x".repeat(262144),
            result: None,
            progress: Some(serde_json::json!({"current": 1})),
        };
        app.db.put("job", &id, &j).unwrap();
    }
    let jobs = app.db.jobs_summary().unwrap();
    assert_eq!(jobs.len(), 100);
    assert_eq!(jobs[0].id, "119");
    assert!(jobs.iter().all(|j| j.log.is_empty()));
    // Progress is small and is what the job list shows for a running download.
    assert!(jobs.iter().all(|j| j.progress.is_some()));
}

#[test]
fn profile_catalog_is_unique_and_metadata_is_explicit() {
    let catalog = profiles::catalog();
    assert_eq!(catalog.len(), 2);
    assert_eq!(
        catalog
            .iter()
            .map(|p| &p.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        catalog.len()
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.reference == profiles::DEFAULT_IMAGE && p.session == "xfce")
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.id == "ubuntu-gnome" && p.session == "gnome")
    );
    assert!(
        profiles::from_image(
            &serde_json::json!({"Config":{"Labels":{"io.wsl-desktop.desktop":"true"}}})
        )
        .is_none()
    );
    let info = profiles::from_image(&serde_json::json!({"Config":{"Labels":{"io.wsl-desktop.profile":"ubuntu-gnome","io.wsl-desktop.session":"gnome","org.opencontainers.image.title":"Ubuntu 24.04 · GNOME"}}})).unwrap();
    assert_eq!(info.session, "gnome");
}
#[test]
fn old_environment_records_remain_readable_without_profile_metadata() {
    let value = serde_json::json!({"id":"old","container":"wsld-old","private_network":"private","egress_network":"egress","source_image_id":"sha256:old","created_at":1,"spec":{"name":"Old XFCE","image":"wsl-desktop/ubuntu:24.04"}});
    let env: model::Environment = serde_json::from_value(value).unwrap();
    assert!(env.desktop.is_none());
    assert_eq!(env.source_image_id, "sha256:old");
}
#[tokio::test]
async fn unknown_profile_build_is_rejected_without_queuing_docker_work() {
    let app = app(false);
    // Only catalog profiles are built or pulled; a free-form reference never reaches a registry.
    for action in ["build", "pull"] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .method("POST")
                    .uri(format!("/api/profiles/not-a-profile/{action}"))
                    .header("x-wsld-request", "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{action}");
    }
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
}
#[test]
fn command_option_injection_is_rejected() {
    for bad in [
        "--privileged",
        "ubuntu; echo secret",
        "a\nb",
        "a,b",
        "",
        "$(id)",
    ] {
        assert!(model::image_ref(bad).is_err());
    }
    for valid in [
        "ubuntu:24.04",
        "registry.local:5000/me/desktop:latest",
        "sha256:abcdef",
        "repo@sha256:abcdef",
    ] {
        assert!(model::image_ref(valid).is_ok());
    }
}
#[test]
fn database_round_trip_keeps_snapshot_parent() {
    let db = db::Db::open(std::path::Path::new(":memory:")).unwrap();
    let record = model::ImageRecord {
        id: "a".into(),
        name: "Snapshot".into(),
        image_id: "sha256:new".into(),
        reference: "snapshot:a".into(),
        parent_image_id: Some("sha256:base".into()),
        source_environment: Some("env".into()),
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
    };
    db.put("image", "a", &record).unwrap();
    assert_eq!(
        db.get::<model::ImageRecord>("image", "a")
            .unwrap()
            .parent_image_id,
        record.parent_image_id
    );
    assert_eq!(db.list::<model::ImageRecord>("image").unwrap().len(), 1);
    db.remove("image", "a").unwrap();
    assert!(db.list::<model::ImageRecord>("image").unwrap().is_empty());
}
#[test]
fn websocket_origin_requires_exact_host_and_port() {
    let mut h = axum::http::HeaderMap::new();
    h.insert("host", "localhost:8080".parse().unwrap());
    h.insert("origin", "http://localhost:8080".parse().unwrap());
    assert!(auth::origin_matches(&h));
    h.insert("origin", "https://evil.example".parse().unwrap());
    assert!(!auth::origin_matches(&h));
    h.insert("origin", "http://localhost:8081".parse().unwrap());
    assert!(!auth::origin_matches(&h));
    h.remove("origin");
    assert!(!auth::origin_matches(&h));
}
#[tokio::test]
async fn protected_routes_require_login_before_touching_docker() {
    let router = router(app(true));
    for path in [
        "/api/environments",
        "/api/profiles",
        "/api/environments/unknown/screenshot",
        "/api/environments/unknown/vnc",
        "/api/jobs",
        "/api/images",
        "/api/volumes",
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
}
#[tokio::test]
async fn login_disabled_does_not_disable_csrf_protection() {
    let router = router(app(false));
    let response = router
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/environments")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn valid_session_still_rejects_cross_origin_mutation() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "token".into(),
        Instant::now() + std::time::Duration::from_secs(60),
    );
    let response = router(app)
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .method("POST")
                .uri("/api/logout")
                .header("cookie", "wsld_session=token")
                .header("x-wsld-request", "1")
                .header("host", "localhost:8080")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
#[tokio::test]
async fn expired_session_is_not_accepted() {
    let app = app(true);
    app.sessions.lock().unwrap().insert(
        "expired".into(),
        Instant::now() - std::time::Duration::from_secs(1),
    );
    let response = router(app)
        .oneshot(
            Request::builder()
                .header("host", "localhost:8080")
                .uri("/api/jobs")
                .header("cookie", "wsld_session=expired")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn held_inputs_are_tracked_and_released_at_the_end_of_a_request() {
    let input: desktop_input::Input = serde_json::from_value(serde_json::json!({"actions":[
        {"type":"key_down","keys":["w","Shift"]},
        {"type":"button_down","x":10,"y":20,"button":"right"},
        {"type":"move_relative","dx":15,"dy":-30},
        {"type":"wait","milliseconds":1500},
        {"type":"key_up","keys":["Shift"]}
    ]}))
    .unwrap();
    let planned = desktop_input::plan(&input.actions, 800, 600, None).unwrap();
    assert_eq!(planned.released_keys, vec!["w"]);
    assert_eq!(planned.released_buttons, vec!["right"]);
    assert_eq!(planned.pointer, Some((25, 0)));
    assert_eq!(planned.delay_ms, 1500);
    // The relative move keeps the right button pressed and clamps to the desktop edge.
    assert_eq!(planned.steps[1].0, vec![5, 4, 0, 10, 0, 20]);
    assert_eq!(planned.steps[2].0, vec![5, 4, 0, 25, 0, 0]);
    assert_eq!(planned.steps.last().unwrap().0, vec![5, 0, 0, 25, 0, 0]);
    assert!(!desktop_input::needs_start_position(&input.actions));
    let relative: desktop_input::Input = serde_json::from_value(
        serde_json::json!({"actions":[{"type":"move_relative","dx":1,"dy":1}]}),
    )
    .unwrap();
    assert!(desktop_input::needs_start_position(&relative.actions));
    assert!(desktop_input::plan(&relative.actions, 800, 600, None).is_err());
    assert_eq!(
        desktop_input::plan(&relative.actions, 800, 600, Some((799, 599)))
            .unwrap()
            .pointer,
        Some((799, 599))
    );
    for body in [
        serde_json::json!({"actions":[{"type":"hold_keys","keys":["w"],"milliseconds":10001}]}),
        serde_json::json!({"actions":(0..4).map(|_| serde_json::json!({"type":"wait","milliseconds":10000})).collect::<Vec<_>>()}),
        serde_json::json!({"actions":[{"type":"button_down","x":5}]}),
        serde_json::json!({"actions":[{"type":"button_up"}]}),
    ] {
        let input: desktop_input::Input = serde_json::from_value(body).unwrap();
        assert!(desktop_input::plan(&input.actions, 800, 600, None).is_err());
    }
}
#[test]
fn screenshot_options_and_jpeg_dimensions_are_validated() {
    let ok = capture::Capture {
        max_width: Some(640),
        region: Some("10, 20,300,200".into()),
        window_id: Some("0x3a00007".into()),
    };
    assert_eq!(ok.region().unwrap(), Some([10, 20, 300, 200]));
    assert!(ok.validate().is_ok());
    for bad in [
        capture::Capture {
            max_width: Some(8),
            ..Default::default()
        },
        capture::Capture {
            region: Some("0,0,0,10".into()),
            ..Default::default()
        },
        capture::Capture {
            region: Some("1,2,3".into()),
            ..Default::default()
        },
        capture::Capture {
            window_id: Some("0xzz".into()),
            ..Default::default()
        },
        capture::Capture {
            window_id: Some("0x123456789".into()),
            ..Default::default()
        },
    ] {
        assert!(bad.validate().is_err());
    }
    let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10];
    jpeg.extend([0u8; 14]);
    jpeg.extend([0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0xf0, 0x01, 0x40]);
    assert_eq!(capture::jpeg_dimensions(&jpeg), Some((320, 240)));
    assert_eq!(capture::jpeg_dimensions(&[0xff, 0xd8, 0xff, 0xd9]), None);
    assert!(windows::window_id("42").is_ok());
    assert!(windows::window_id("abc").is_err());
}
#[test]
fn application_launch_requests_are_validated() {
    let launch = |value: serde_json::Value| {
        serde_json::from_value::<apps::Launch>(value)
            .unwrap()
            .validate()
    };
    assert!(
        launch(serde_json::json!({"command":"glxgears","args":["-info"],"cwd":"/home/desktop/game","env":{"MY_VAR":"1"}}))
            .is_ok()
    );
    for bad in [
        serde_json::json!({"command":""}),
        serde_json::json!({"command":"app","cwd":"relative"}),
        serde_json::json!({"command":"app","cwd":"/home/desktop/../root"}),
        serde_json::json!({"command":"app","env":{"1BAD":"x"}}),
        serde_json::json!({"command":"app","env":{"DISPLAY":":1"}}),
        serde_json::json!({"command":"app","args":(0..101).map(|_| "x").collect::<Vec<_>>()}),
    ] {
        assert!(launch(bad).is_err());
    }
    assert!(
        serde_json::from_value::<apps::Launch>(
            serde_json::json!({"command":"app","unexpected":true})
        )
        .is_err()
    );
    assert!(
        windows::Geometry {
            x: Some(1),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        windows::Geometry {
            width: Some(640),
            height: Some(480),
            ..Default::default()
        }
        .validate()
        .is_ok()
    );
}
#[tokio::test]
async fn gpu_environments_are_rejected_when_the_installation_has_no_gpu() {
    let mut app = app(false);
    {
        let config = &mut Arc::get_mut(&mut app).unwrap().config;
        config.wsl_gpu = false;
        config.cuda = false;
    }
    let mut spec = spec();
    spec.gpu = true;
    let error = app
        .create_environment("gpu-test", spec, "gpu-job")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("GPU"), "{error}");
    assert!(
        app.db
            .list::<model::Environment>("environment")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn file_tool_requests_are_validated() {
    let mut spec = spec();
    spec.mounts.push(model::Mount {
        volume: "projects".into(),
        target: "/shared".into(),
        readonly: false,
    });
    assert!(
        files::ListFiles {
            path: "/".into(),
            limit: 10
        }
        .validate()
        .is_ok()
    );
    assert!(
        files::ListFiles {
            path: "relative".into(),
            limit: 10
        }
        .validate()
        .is_err()
    );
    assert!(
        files::ListFiles {
            path: "/home/desktop".into(),
            limit: 0
        }
        .validate()
        .is_err()
    );
    assert!(
        files::MoveFile {
            source: "/home/desktop/a".into(),
            destination: "/home/desktop/b".into(),
            replace: false
        }
        .validate()
        .is_ok()
    );
    assert!(
        files::MoveFile {
            source: "/home/desktop/../a".into(),
            destination: "/home/desktop/b".into(),
            replace: false
        }
        .validate()
        .is_err()
    );
    for path in ["/home/desktop", "/shared", "/etc", "/home/desktop/"] {
        assert!(
            files::DeleteFile {
                path: path.into(),
                recursive: true
            }
            .validate(&spec)
            .is_err(),
            "{path}"
        );
    }
    assert!(
        files::DeleteFile {
            path: "/shared/project".into(),
            recursive: true
        }
        .validate(&spec)
        .is_ok()
    );
}

#[test]
fn wait_conditions_are_validated() {
    let check = |value: serde_json::Value| {
        serde_json::from_value::<waits::WaitFor>(value)
            .unwrap()
            .validate()
    };
    assert!(check(serde_json::json!({"window_title":"Game"})).is_ok());
    assert!(
        check(serde_json::json!({"application_id":"a","log_contains":"ready","timeout_ms":60000}))
            .is_ok()
    );
    assert!(check(serde_json::json!({"screen_stable_ms":1000,"region":"0,0,100,100"})).is_ok());
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"application_state":"running"}),
        serde_json::json!({"application_id":"a","application_state":"paused"}),
        serde_json::json!({"log_contains":"x"}),
        serde_json::json!({"window_title":"x","timeout_ms":60001}),
        serde_json::json!({"screen_stable_ms":20000,"timeout_ms":10000}),
        serde_json::json!({"window_title":"x","region":"0,0,1,1"}),
    ] {
        assert!(check(bad).is_err());
    }
}
#[tokio::test]
async fn metric_sampling_rejects_bad_durations_before_docker() {
    let app = app(false);
    assert!(
        apps::sample_metrics(&app, "missing", "app", 50)
            .await
            .is_err()
    );
    assert!(
        apps::sample_metrics(&app, "missing", "app", 10_001)
            .await
            .is_err()
    );
}

#[test]
fn recording_requests_are_validated() {
    let start = |value: serde_json::Value| {
        serde_json::from_value::<recordings::StartRecording>(value)
            .unwrap()
            .validate()
    };
    assert!(start(serde_json::json!({})).is_ok());
    assert!(
        start(
            serde_json::json!({"fps":5,"max_seconds":600,"max_width":320,"region":"0,0,100,100"})
        )
        .is_ok()
    );
    for bad in [
        serde_json::json!({"fps":0}),
        serde_json::json!({"fps":6}),
        serde_json::json!({"max_seconds":601}),
        serde_json::json!({"max_width":8}),
        serde_json::json!({"region":"1,2"}),
    ] {
        assert!(start(bad).is_err());
    }
    let export = |value: serde_json::Value| {
        serde_json::from_value::<recordings::Export>(value)
            .unwrap()
            .validate()
    };
    assert!(
        export(serde_json::json!({"destination":"/home/desktop/Downloads","name":"run.tar.gz","recording_ids":["a"]}))
            .is_ok()
    );
    assert!(
        export(serde_json::json!({"destination":"/home/desktop/Downloads","name":"run.tar.gz"}))
            .is_err()
    );
}

#[test]
fn audio_terminal_download_and_inspection_requests_are_validated() {
    assert!(
        serde_json::from_value::<audio::CaptureAudio>(serde_json::json!({}))
            .unwrap()
            .validate()
            .is_ok()
    );
    for bad in [
        serde_json::json!({"seconds":0}),
        serde_json::json!({"seconds":11}),
        serde_json::json!({"source":"line-in"}),
    ] {
        assert!(
            serde_json::from_value::<audio::CaptureAudio>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert_eq!(
        serde_json::from_value::<audio::InjectAudio>(serde_json::json!({"data_base64":"AAAA"}))
            .unwrap()
            .validate()
            .unwrap(),
        "browser-mic-sink"
    );
    assert!(
        serde_json::from_value::<audio::InjectAudio>(
            serde_json::json!({"data_base64":"AAAA","target":"headphones"})
        )
        .unwrap()
        .validate()
        .is_err()
    );
    assert!(
        serde_json::from_value::<terminal::TerminalInput>(serde_json::json!({"text":"ls\n"}))
            .unwrap()
            .validate()
            .is_ok()
    );
    assert!(
        serde_json::from_value::<terminal::TerminalInput>(serde_json::json!({"text":""}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<terminal::TerminalOutput>(serde_json::json!({"limit":0}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<terminal::OpenTerminal>(serde_json::json!({"cwd":"relative"}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<windows::Inspect>(serde_json::json!({"max_depth":21}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<windows::Inspect>(serde_json::json!({"pid":12,"name":"Term"}))
            .unwrap()
            .validate()
            .is_ok()
    );
    assert_eq!(files::mime_type("shot.PNG"), "image/png");
    assert_eq!(files::mime_type("archive.tar.gz"), "application/gzip");
    assert_eq!(files::mime_type("binary"), "application/octet-stream");
}
#[test]
fn gamepad_requests_are_validated_and_default_off() {
    assert!(!spec().gamepad);
    let check = |value: serde_json::Value| {
        serde_json::from_value::<gamepad::GamepadInput>(value)
            .unwrap()
            .validate()
    };
    assert!(
        check(serde_json::json!({"actions":[
            {"type":"button","button":"a"},
            {"type":"axis","axis":"left_x","value":-0.5},
            {"type":"axis","axis":"right_trigger","value":1},
            {"type":"axis","axis":"dpad_y","value":-1},
            {"type":"wait","milliseconds":100},
            {"type":"button","button":"a","pressed":false},
            {"type":"reset"}
        ]}))
        .is_ok()
    );
    for bad in [
        serde_json::json!({"actions":[]}),
        serde_json::json!({"actions":[{"type":"button","button":"select"}]}),
        serde_json::json!({"actions":[{"type":"axis","axis":"left_x","value":1.5}]}),
        serde_json::json!({"actions":[{"type":"axis","axis":"left_trigger","value":-0.1}]}),
        serde_json::json!({"actions":[{"type":"axis","axis":"dpad_x","value":0.5}]}),
        serde_json::json!({"actions":[{"type":"wait","milliseconds":10001}]}),
        serde_json::json!({"actions":(0..4).map(|_| serde_json::json!({"type":"wait","milliseconds":10000})).collect::<Vec<_>>()}),
    ] {
        assert!(check(bad).is_err());
    }
}

#[test]
fn screenshot_save_targets_and_download_links_are_shaped_for_users() {
    let save = |path: &str| capture::SaveCapture {
        path: path.into(),
        replace: false,
        capture: capture::Capture::default(),
    };
    assert_eq!(
        save("/home/desktop/Downloads/clock.jpg").target().unwrap(),
        ("/home/desktop/Downloads".into(), "clock.jpg".into())
    );
    for invalid in [
        "Downloads/clock.jpg",
        "/home/desktop/Downloads/",
        "/clock.jpg",
        "",
    ] {
        assert!(save(invalid).target().is_err(), "{invalid}");
    }
    // The directory part still has to pass the upload destination rules.
    assert!(
        uploads::validate_target(&spec(), "/home/desktop/Downloads/../.ssh", "id_rsa").is_err()
    );
    assert!(uploads::validate_target(&spec(), "/home/desktop/Downloads", "clock.jpg").is_ok());
    assert_eq!(
        files::download_url("abc123", "/home/desktop/Downloads/saat 17:20.jpg"),
        "/api/environments/abc123/files/download?path=/home/desktop/Downloads/saat%2017%3A20.jpg"
    );
    assert_eq!(
        files::download_url("abc123", "/shared/ölçüm.tar.gz"),
        "/api/environments/abc123/files/download?path=/shared/%C3%B6l%C3%A7%C3%BCm.tar.gz"
    );
}
