use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

/// The repository's desktop build context, with the seccomp profile a parsed configuration looks
/// for. `cargo test` runs in the crate's folder, two levels below the repository root.
pub(crate) const DESKTOP_CONTEXT: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../images/desktop");

pub(crate) fn app(auth: bool) -> Arc<App> {
    Arc::new(App {
        config: config::Config {
            listen: "127.0.0.1:0".into(),
            data: "data".into(),
            web: "core/web/dist".into(),
            docker_bin: "definitely-not-a-docker-command".into(),
            docker_socket: None,
            manager: None,
            base_image: "ghcr.io/co3moz/thoric/ubuntu-xfce:24.04".into(),
            desktop_context: DESKTOP_CONTEXT.into(),
            auth,
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
fn spec() -> model::ContainerSpec {
    serde_json::from_value(
        serde_json::json!({"name":"Unity","image":"ghcr.io/co3moz/thoric/ubuntu-xfce:24.04"}),
    )
    .unwrap()
}

pub(crate) fn mcp_app() -> Arc<App> {
    let mut app = app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.root_mcp_enabled = true;
    config.mcp_token = Some("test-token-with-at-least-32-characters".into());
    app
}
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
#[test]
fn templates_url_defaults_to_the_wiki_and_none_turns_the_page_off() {
    for unset in [None, Some(""), Some("  ")] {
        assert_eq!(
            config::templates_url(unset).unwrap().as_deref(),
            Some(config::TEMPLATES_URL)
        );
    }
    assert_eq!(config::templates_url(Some("none")).unwrap(), None);
    let mirror = "http://10.0.0.5:8000/thoric/index.json";
    assert_eq!(
        config::templates_url(Some(mirror)).unwrap().as_deref(),
        Some(mirror)
    );
    // The policy header names the host, so nothing may close the directive or start another.
    for refused in [
        "None",
        "ftp://example.com/index.json",
        "https://",
        "https:///index.json",
        "https://example.com/a b.json",
        "https://example.com;script-src *",
        "https://example.com/index.json'",
        "https://user@example.com/index.json",
        "https://example.com?x/index.json",
    ] {
        assert!(config::templates_url(Some(refused)).is_err(), "{refused}");
    }
}
#[test]
fn volume_filter_defaults_to_the_managers_label_and_none_lists_every_volume() {
    // Compose hands an unset variable over as an empty one, so empty cannot switch it off.
    for unset in [None, Some(""), Some("  ")] {
        assert_eq!(
            config::volume_filter(unset).unwrap().as_deref(),
            Some("thoric.managed=true")
        );
    }
    assert_eq!(config::volume_filter(Some("none")).unwrap(), None);
    for label in [
        "team",
        "team=",
        "com.example.team=unity",
        "thoric.managed",
        "thoric.managed=true",
    ] {
        assert_eq!(
            config::volume_filter(Some(label)).unwrap().as_deref(),
            Some(label)
        );
    }
    // A label the manager could not put on what it creates, or one Docker's filter reads apart.
    for refused in [
        "=true",
        "team name=a",
        "team=a b",
        "team=a,b",
        "team\n=a",
        "thoric.managed=false",
        "thoric.managed=",
    ] {
        assert!(config::volume_filter(Some(refused)).is_err(), "{refused}");
    }
}
#[tokio::test]
async fn the_content_security_policy_allows_the_templates_host_only() {
    let mut origins = app(false);
    let config = &mut Arc::get_mut(&mut origins).unwrap().config;
    for (url, origin) in [
        (config::TEMPLATES_URL, "https://raw.githubusercontent.com"),
        ("http://10.0.0.5:8000/x/index.json", "http://10.0.0.5:8000"),
        ("https://example.com", "https://example.com"),
    ] {
        config.templates_url = Some(url.into());
        assert_eq!(config.templates_origin(), Some(origin));
    }
    let policy = |app: Arc<App>| async move {
        let response = router(app)
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .uri("/api/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .to_owned()
    };
    let none = policy(app(false)).await;
    assert!(none.contains("img-src 'self' data:;") && none.contains("connect-src 'self';"));
    let mut github = app(false);
    Arc::get_mut(&mut github).unwrap().config.templates_url = Some(config::TEMPLATES_URL.into());
    let github = policy(github).await;
    let host = "https://raw.githubusercontent.com";
    assert!(
        github.contains(&format!("img-src 'self' data: {host};")),
        "{github}"
    );
    assert!(
        github.contains(&format!("connect-src 'self' {host};")),
        "{github}"
    );
    assert!(github.contains("script-src 'self';"), "{github}");
}
#[test]
fn mcp_token_and_script_validation() {
    // Any length up to 256 is accepted; whitespace and non-ASCII cannot travel in a bearer header.
    for token in [
        "",
        "short token",
        "token\n",
        "şifre",
        "x".repeat(257).as_str(),
    ] {
        assert!(config::validate_mcp_token(true, token).is_err());
    }
    assert!(config::validate_mcp_token(false, "").is_ok());
    for token in ["x", "$hort#'\"`%!", "x".repeat(256).as_str()] {
        assert!(config::validate_mcp_token(true, token).is_ok());
    }
    // Only an explicit MCP_AUTH_MODE=none drops the token; an unset or mistyped mode does not.
    for mode in [None, Some("token")] {
        assert!(config::mcp_token(true, mode, String::new()).is_err());
    }
    for mode in [Some(""), Some("off"), Some("None")] {
        assert!(config::mcp_token(true, mode, "0".repeat(32)).is_err());
    }
    assert_eq!(
        config::mcp_token(true, None, "0".repeat(32)).unwrap(),
        Some("0".repeat(32))
    );
    assert_eq!(
        config::mcp_token(true, Some("none"), String::new()).unwrap(),
        None
    );
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
        Arc::get_mut(&mut app).unwrap().config.root_mcp_enabled = enabled;
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
async fn mcp_without_a_token_still_rejects_other_hosts_and_origins() {
    let initialize = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
    for (token, host, origin, expected) in [
        (None, "localhost:8080", None, StatusCode::OK),
        (Some("Bearer wrong"), "localhost:8080", None, StatusCode::OK),
        (None, "evil.example", None, StatusCode::FORBIDDEN),
        (
            None,
            "localhost:8080",
            Some("https://evil.example"),
            StatusCode::FORBIDDEN,
        ),
    ] {
        let mut app = mcp_app();
        Arc::get_mut(&mut app).unwrap().config.mcp_token = None;
        let mut req = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header("host", host)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25");
        if let Some(token) = token {
            req = req.header("authorization", token);
        }
        if let Some(origin) = origin {
            req = req.header("origin", origin);
        }
        let response = router(app)
            .oneshot(req.body(Body::from(initialize.to_string())).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
#[tokio::test]
async fn mcp_negotiates_and_lists_tools_with_useful_results() {
    let app = mcp_app();
    let (status, body) = mcp_request(app.clone(), "initialize", serde_json::json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["serverInfo"]["name"], "thoric");
    let instructions = body["result"]["instructions"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let (status, body) = mcp_request(app.clone(), "tools/list", serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 76);
    // Usage is read for every container unless one is named.
    let usage = tools.iter().find(|t| t["name"] == "get_usage").unwrap();
    assert_eq!(usage["annotations"]["readOnlyHint"], true);
    assert_eq!(usage["inputSchema"]["required"], serde_json::json!([]));
    assert!(usage["inputSchema"]["properties"]["container_id"].is_object());
    // Compose values reach a new container through create_container; the schema forbids
    // properties it does not list, so a missing one is a value no agent can send.
    let create = tools
        .iter()
        .find(|t| t["name"] == "create_container")
        .unwrap();
    assert!(create["inputSchema"]["properties"]["parameters"].is_object());
    let compose = tools
        .iter()
        .find(|t| t["name"] == "set_image_compose")
        .unwrap();
    assert_eq!(compose["annotations"]["destructiveHint"], true);
    assert_eq!(
        compose["inputSchema"]["properties"]["compose"]["anyOf"][0]["required"],
        serde_json::json!(["script"])
    );
    for name in ["delete_container", "delete_image", "delete_volume"] {
        let delete = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(delete["annotations"]["destructiveHint"], true, "{name}");
    }
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
        serde_json::json!(["path", "container_id"])
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
    // Agents wrote their own install scripts for toolchains a template already builds, since no
    // tool lists templates. The instructions and build_image must point them at the list.
    let build = tools.iter().find(|t| t["name"] == "build_image").unwrap();
    for text in [
        instructions.as_str(),
        build["description"].as_str().unwrap(),
    ] {
        assert!(text.contains("templates_url") && text.contains("template_id"));
    }
    let guide = include_str!("../../../docs/MCP.md");
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            guide.contains(&format!("`{name}`")),
            "docs/MCP.md must mention {name}"
        );
    }
    assert!(guide.contains("`download_url`"));
    assert!(guide.contains("## Build from a template") && guide.contains("`templates_url`"));
    // The count went stale once already; it is stated in the guide, so it is checked from here.
    assert!(
        guide.contains(&format!("exposes {} tools", tools.len())),
        "docs/MCP.md must state the tool count"
    );
    // The README leads with the same count, so it goes stale the same way.
    assert!(
        include_str!("../../../README.md").contains(&format!("with {} tools", tools.len())),
        "README.md must state the tool count"
    );
    assert_eq!(
        screenshot["inputSchema"]["required"],
        serde_json::json!(["container_id"])
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
            .any(|v| v == "container_id")
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
        app.clone(),
        "tools/call",
        serde_json::json!({"name":"get_usage","arguments":{"seconds":30}}),
    )
    .await;
    assert_eq!(body["result"]["isError"], false, "{body}");
    assert_eq!(body["result"]["structuredContent"]["seconds"], 30);
    assert_eq!(body["result"]["structuredContent"]["gpu"]["scope"], "host");
    let (_, body) = mcp_request(
        app,
        "tools/call",
        serde_json::json!({"name":"delete_container","arguments":{"container_id":"missing"}}),
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
        serde_json::json!({"name":"delete_container","arguments":{}}),
        serde_json::json!({"name":"create_container","arguments":{"name":"test","image":"ubuntu","unexpected":true}}),
    ] {
        let (_, body) = mcp_request(app.clone(), "tools/call", args).await;
        assert_eq!(body["error"]["code"], -32602, "{body}");
    }
    let (_,body) = mcp_request(app.clone(),"tools/call",serde_json::json!({"name":"exec_container","arguments":{"container_id":"missing","script":"echo ok","timeout_seconds":0}})).await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
    let (_,body)=mcp_request(app.clone(),"tools/call",serde_json::json!({"name":"create_container","arguments":{"name":"test","image":"ubuntu","mounts":[{"volume":"project","target":"/workspace","read_only":true}]}})).await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
    // copy_volume reaches the copy route, which refuses it before any job without a manager.
    for (arguments, refusal) in [
        (
            serde_json::json!({"volume":"project","name":"project-copy"}),
            "MANAGER_CONTAINER",
        ),
        (
            serde_json::json!({"volume":"project","name":"-bad"}),
            "Invalid volume name",
        ),
    ] {
        let (_, body) = mcp_request(
            app.clone(),
            "tools/call",
            serde_json::json!({"name":"copy_volume","arguments":arguments}),
        )
        .await;
        assert_eq!(body["result"]["isError"], true, "{body}");
        assert!(body.to_string().contains(refusal), "{body}");
    }
    let (_, body) = mcp_request(
        app.clone(),
        "tools/call",
        serde_json::json!({"name":"copy_volume","arguments":{"name":"project-copy"}}),
    )
    .await;
    assert_eq!(body["error"]["code"], -32602, "{body}");
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
}

/// A debug build reserves stack in a function's frame for every future it awaits, and the MCP
/// dispatch awaits dozens in one match. Once that took most of a tokio worker's 2 MiB, so a debug
/// manager aborted on screenshot. The whole /mcp call, down to running Docker, must fit in half.
#[test]
fn mcp_screenshot_fits_in_half_a_worker_stack() {
    let app = mcp_app();
    let env = model::Container {
        id: "test-id".into(),
        container: "test-container".into(),
        private_network: "test-private".into(),
        egress_network: "test-egress".into(),
        source_image_id: "sha256:test".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, &env.id, &env).unwrap();
    // A stack overflow aborts the test binary and names this thread. The call needs under 512 KiB
    // on Linux and 768 KiB on Windows; with Docker's read chunks in arrays it overflowed 2 MiB.
    std::thread::Builder::new()
        .name("mcp-screenshot-in-1-mib".into())
        .stack_size(1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let (_, body) = mcp_request(
                        app,
                        "tools/call",
                        serde_json::json!({"name":"screenshot","arguments":{"container_id":"test-id","max_width":320}}),
                    )
                    .await;
                    // The test has no Docker, so the call ends where it would run the first command.
                    assert_eq!(body["result"]["isError"], true, "{body}");
                    assert!(
                        body.to_string().contains("Could not start Docker CLI"),
                        "{body}"
                    );
                })
        })
        .unwrap()
        .join()
        .unwrap();
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
        "files/mkdir",
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
                .uri(format!("/api/containers/missing/{route}"))
                .header("host", "localhost")
                .header("content-type", "application/json");
            if csrf {
                request = request.header("x-thoric-request", "1");
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
async fn single_container_api_returns_the_same_state_as_the_list() {
    let app = app(false);
    let env = model::Container {
        id: "test-id".into(),
        container: "test-container".into(),
        private_network: "test-private".into(),
        egress_network: "test-egress".into(),
        source_image_id: "sha256:test".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, &env.id, &env).unwrap();
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/containers/test-id")
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
    // The record's own fields and Docker's view of it sit side by side.
    assert_eq!(body["id"], "test-id");
    assert_eq!(body["spec"]["name"], spec().name);
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

/// Norton's web protection rewrites a Content-Length reply on a loopback `/mcp` path into a
/// chunked header over an unchunked body. Chunked replies pass through it intact, so the framing
/// that reaches the socket is what has to stay chunked, rejections included.
#[tokio::test]
async fn mcp_replies_are_chunked_on_the_wire() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router(mcp_app())).await });
    let initialize = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
    .to_string();
    for (authorization, expected) in [
        ("", "HTTP/1.1 401 "),
        (
            "Authorization: Bearer test-token-with-at-least-32-characters\r\n",
            "HTTP/1.1 200 ",
        ),
    ] {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: localhost:8080\r\n{authorization}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nMCP-Protocol-Version: 2025-11-25\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{initialize}",
            initialize.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).await.unwrap();
        let reply = String::from_utf8(reply).unwrap();
        let (head, body) = reply.split_once("\r\n\r\n").unwrap();
        let head = head.to_ascii_lowercase();
        assert!(reply.starts_with(expected), "{head}");
        assert!(head.contains("\r\ntransfer-encoding: chunked"), "{head}");
        assert!(!head.contains("\r\ncontent-length:"), "{head}");
        let (size, rest) = body.split_once("\r\n").unwrap();
        let size = usize::from_str_radix(size, 16).unwrap();
        let json: serde_json::Value = serde_json::from_str(&rest[..size]).unwrap();
        assert!(json.is_object(), "{json}");
        assert!(rest[size..].ends_with("0\r\n\r\n"), "{body}");
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
    let places = [crate::compose::Place {
        label: "Unity project".into(),
        path: "/srv/unity/MyGame".into(),
    }];
    let check = |spec: &model::ContainerSpec, target: &str, name: &str| {
        uploads::check_target(&spec.mounts, &places, target, name)
    };
    // The whole home is the desktop user's, hidden folders too.
    for target in [
        "/home/desktop",
        "/home/desktop/Downloads",
        "/home/desktop/Downloads/project",
        "/home/desktop/Downloads-fake",
        "/home/desktop/Documents/notes",
        "/home/desktop/.config/autostart",
        "/shared",
        "/shared/assets",
        "/srv/unity/MyGame",
        "/srv/unity/MyGame/Assets",
    ] {
        assert!(check(&spec, target, "file & notes.txt").is_ok(), "{target}");
    }
    for target in [
        "/",
        "/etc",
        "/tmp",
        "/home",
        "/home/other",
        "/home/desktopper",
        "/sharedish",
        "/shared/reference",
        "/shared/reference/sub",
        "/srv/unity",
        "/srv/unity/MyGameOld",
        "/home/desktop/Downloads/../.ssh",
        "/home/desktop/",
        "/shared//assets",
        "/shared/.",
        "relative/folder",
    ] {
        assert!(check(&spec, target, "file.txt").is_err(), "{target}");
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
        assert!(check(&spec, "/shared", name).is_err());
    }
    // A read-only volume inside the home closes that folder and nothing else.
    spec.mounts.push(model::Mount {
        volume: "downloads".into(),
        target: "/home/desktop/Downloads".into(),
        readonly: true,
    });
    assert!(check(&spec, "/home/desktop/Downloads", "file.txt").is_err());
    assert!(check(&spec, "/home/desktop/Downloads/sub", "file.txt").is_err());
    assert!(check(&spec, "/home/desktop/Documents", "file.txt").is_ok());
}

#[tokio::test]
async fn uploads_enforce_authentication_origin_and_size_before_docker() {
    let uri =
        "/api/containers/unknown/files?destination=/home/desktop/Downloads&name=test.txt&size=1";
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
            request = request.header("x-thoric-request", "1");
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
                .header("cookie", "thoric_session=media")
                .header("x-thoric-request", "1")
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
async fn unrelated_jobs_progress_while_a_container_queue_is_blocked() {
    let app = app(false);
    let lock = app.resource_lock("container:one");
    let guard = lock.lock().await;
    let queued = jobs::launch(app.clone(), "stop", "container:one".into(), |_, _| async {
        Ok(serde_json::json!({}))
    })
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let free = jobs::launch(app.clone(), "start", "container:two".into(), |_, _| async {
        Ok(serde_json::json!({}))
    })
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
            stages: Vec::new(),
            script: None,
            name: None,
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
fn image_catalog_is_unique_and_metadata_is_explicit() {
    let catalog = catalog::catalog();
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
            .any(|p| p.reference == catalog::DEFAULT_IMAGE && p.session == "xfce")
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.id == "ubuntu-gnome" && p.session == "gnome")
    );
    assert!(
        catalog::from_image(&serde_json::json!({"Config":{"Labels":{"thoric.desktop":"true"}}}))
            .is_none()
    );
    let info = catalog::from_image(&serde_json::json!({"Config":{"Labels":{"thoric.profile":"ubuntu-gnome","thoric.session":"gnome","org.opencontainers.image.title":"Ubuntu 24.04 · GNOME"}}})).unwrap();
    assert_eq!(info.session, "gnome");
    assert_eq!(info.catalog_id, "ubuntu-gnome");
    // Records saved while catalog images were profiles still carry their desktop.
    let old: catalog::DesktopInfo = serde_json::from_value(
        serde_json::json!({"profile":"ubuntu-xfce","title":"Ubuntu 24.04 · Lightweight XFCE","session":"xfce"}),
    )
    .unwrap();
    assert_eq!(old.catalog_id, "ubuntu-xfce");
}
#[test]
fn catalog_images_are_listed_with_the_saved_images() {
    let xfce = catalog::find("ubuntu-xfce").unwrap();
    let labels = |id: &str, session: &str| serde_json::json!({"Id":"sha256:x","Config":{"Labels":{"thoric.profile":id,"thoric.session":session,"org.opencontainers.image.title":"XFCE"}}});
    // Not downloaded yet: still listed, so a container can start from it and download it.
    let missing = catalog::listed(&xfce, None, true);
    assert_eq!(missing["kind"], "catalog");
    assert_eq!(missing["reference"], xfce.reference);
    assert!(missing["available"] == false && missing["downloaded"] == false);
    assert!(missing["image_id"].is_null() && missing["error"].is_null());
    let ready = catalog::listed(&xfce, Some(&labels("ubuntu-xfce", "xfce")), true);
    assert!(ready["available"] == true && ready["image_id"] == "sha256:x");
    assert_eq!(ready["desktop"]["catalog_id"], "ubuntu-xfce");
    // A tag that points at another desktop is reported instead of being used.
    let foreign = catalog::listed(&xfce, Some(&labels("ubuntu-gnome", "gnome")), true);
    assert!(foreign["available"] == false && foreign["downloaded"] == true);
    assert!(foreign["error"].is_string());
    // A catalog reference made for a newer Thoric, such as one pulled by hand, says why it waits.
    let mut newer = labels("ubuntu-xfce", "xfce");
    newer["Config"]["Labels"]["thoric.desktop-level"] =
        serde_json::json!((catalog::DESKTOP_LEVEL + 1).to_string());
    let newer = catalog::listed(&xfce, Some(&newer), true);
    assert!(newer["available"] == false && newer["downloaded"] == true);
    assert!(
        newer["error"].as_str().unwrap().contains("newer Thoric"),
        "{newer}"
    );
    // Without Docker nobody knows.
    assert!(catalog::listed(&xfce, None, false)["available"].is_null());
}
#[test]
fn old_container_records_remain_readable_without_desktop_metadata() {
    let value = serde_json::json!({"id":"old","container":"thoric-old","private_network":"private","egress_network":"egress","source_image_id":"sha256:old","created_at":1,"spec":{"name":"Old XFCE","image":"ghcr.io/co3moz/thoric/ubuntu-xfce:24.04"}});
    let env: model::Container = serde_json::from_value(value).unwrap();
    assert!(env.desktop.is_none());
    assert_eq!(env.source_image_id, "sha256:old");
}
#[test]
fn records_saved_before_containers_were_renamed_keep_their_container() {
    // An installation keeps its database across the upgrade that renamed environments.
    let image: model::ImageRecord = serde_json::from_value(serde_json::json!({"id":"snap","name":"Snapshot","image_id":"sha256:snap","reference":"thoric/snapshot:snap","parent_image_id":"sha256:base","source_environment":"env","kind":"snapshot","created_at":1,"description":""})).unwrap();
    assert_eq!(image.source_container.as_deref(), Some("env"));
    assert!(serde_json::to_value(&image).unwrap()["source_container"] == "env");
    let application: model::Application = serde_json::from_value(serde_json::json!({"id":"app","environment_id":"env","name":"xlogo","command":"xlogo","args":[],"cwd":"/home/desktop","gpu":false,"created_at":1})).unwrap();
    assert_eq!(application.container_id, "env");
    let terminal: model::Terminal = serde_json::from_value(serde_json::json!({"id":"t","environment_id":"env","root":false,"cwd":"/home/desktop","created_at":1})).unwrap();
    assert_eq!(terminal.container_id, "env");
    let recording: model::Recording = serde_json::from_value(
        serde_json::json!({"id":"r","environment_id":"env","name":"run","fps":2,"created_at":1}),
    )
    .unwrap();
    assert_eq!(recording.container_id, "env");
    // Container records stay under the kind they were saved with.
    assert_eq!(model::CONTAINERS, "environment");
}
#[tokio::test]
async fn only_catalog_images_are_built_or_pulled() {
    let app = app(false);
    // A free-form reference or a saved image never reaches a registry or the bundled context.
    for action in ["build", "pull"] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .method("POST")
                    .uri(format!("/api/images/not-in-catalog/{action}"))
                    .header("x-thoric-request", "1")
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
        source_container: Some("env".into()),
        kind: "snapshot".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose: None,
        icon: None,
        template_ids: Vec::new(),
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
        "/api/containers",
        "/api/containers/unknown/screenshot",
        "/api/containers/unknown/vnc",
        "/api/jobs",
        "/api/images",
        "/api/images/unknown/config",
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
                .uri("/api/containers")
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
                .header("cookie", "thoric_session=token")
                .header("x-thoric-request", "1")
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
                .header("cookie", "thoric_session=expired")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn the_interface_page_is_revalidated_while_its_hashed_assets_are_kept() {
    let web = std::env::temp_dir().join(format!(
        "thoric-web-{}-{}",
        std::process::id(),
        model::now()
    ));
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<!doctype html>").unwrap();
    std::fs::write(web.join("assets/index-Ab12Cd34.js"), "export {}").unwrap();
    let mut app = app(false);
    Arc::get_mut(&mut app).unwrap().config.web = web.clone();
    for (path, status, cache) in [
        ("/", StatusCode::OK, "no-cache"),
        ("/index.html", StatusCode::OK, "no-cache"),
        (
            "/assets/index-Ab12Cd34.js",
            StatusCode::OK,
            "public, max-age=31536000, immutable",
        ),
        // A missing asset must not be remembered as missing for a year.
        ("/assets/index-gone.js", StatusCode::NOT_FOUND, "no-cache"),
        ("/api/session", StatusCode::OK, "no-store"),
    ] {
        let response = router(app.clone())
            .oneshot(
                Request::builder()
                    .header("host", "localhost:8080")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{path}");
        assert_eq!(response.headers()["cache-control"], cache, "{path}");
    }
    std::fs::remove_dir_all(web).unwrap();
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
async fn gpu_containers_are_rejected_when_the_installation_has_no_gpu() {
    let mut app = app(false);
    {
        let config = &mut Arc::get_mut(&mut app).unwrap().config;
        config.wsl_gpu = false;
        config.cuda = false;
    }
    let mut spec = spec();
    spec.gpu = true;
    let error = app
        .create_container("gpu-test", spec, Default::default(), "gpu-job")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("GPU"), "{error}");
    assert!(
        app.db
            .list::<model::Container>(model::CONTAINERS)
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn a_host_without_a_wsl_gpu_refuses_a_gpu_container_naming_wsl_gpu() {
    // A Linux host without GPUs, with WSL_GPU left at its default.
    let mut app = app(false);
    Arc::get_mut(&mut app).unwrap().config.gpu_host = gpu::Host {
        seen: Some(Default::default()),
    };
    let mut spec = spec();
    spec.gpu = true;
    let error = app
        .create_container("gpu-test", spec.clone(), Default::default(), "gpu-job")
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("WSL_GPU is true, but this host has no WSL GPU (/dev/dxg)"),
        "{error}"
    );
    assert!(
        app.db
            .list::<model::Container>(model::CONTAINERS)
            .unwrap()
            .is_empty()
    );
    // A retry or reset of a GPU container that exists already stops before Docker too.
    let env = model::Container {
        id: "gpu-test".into(),
        container: "thoric-gpu-test".into(),
        private_network: "thoric-gpu-test-private".into(),
        egress_network: "thoric-gpu-test-egress".into(),
        source_image_id: "sha256:base".into(),
        created_at: 1,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    let error = app.create_stopped(&env, "manager").await.unwrap_err();
    assert!(error.to_string().contains("WSL_GPU"), "{error}");
    // A settings change that creates it again is refused the same way; one applied in place is not.
    let request = |body: serde_json::Value| body.as_object().unwrap().clone();
    let error = app
        .settings_preview(
            &env,
            &request(serde_json::json!({"resolution": "1280x720"})),
        )
        .unwrap_err();
    assert!(error.to_string().contains("WSL_GPU"), "{error}");
    let (_, changes) = app
        .settings_preview(&env, &request(serde_json::json!({"name": "Renamed"})))
        .unwrap();
    assert!(changes.restart.is_empty());
    // And the create form starts without GPU access.
    assert!(!app.config.feature_defaults().gpu);
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

#[cfg(unix)]
#[tokio::test]
async fn no_desktop_is_created_with_the_managers_own_volume() {
    let mut app = app(false);
    let calls = manager_docker(&mut app);
    let mut spec = spec();
    spec.mounts.push(model::Mount {
        volume: "thoric-data".into(),
        target: "/workspace".into(),
        readonly: true,
    });
    let refused = |error: anyhow::Error| {
        let message = error.to_string();
        assert!(
            message.contains("thoric-data is the manager's own volume"),
            "{message}"
        );
    };
    // Creation refuses it before the volume or the image is looked at, and saves nothing.
    refused(
        app.create_container("refused", spec.clone(), Default::default(), "create-job")
            .await
            .unwrap_err(),
    );
    assert!(
        app.db
            .list::<model::Container>(model::CONTAINERS)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect thoric-manager\n"
    );

    // A record saved before the manager refused it: a reset keeps its container, and a retry
    // does not recreate the missing one.
    let env = model::Container {
        id: "legacy".into(),
        container: "thoric-legacy".into(),
        private_network: "thoric-legacy-private".into(),
        egress_network: "thoric-legacy-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, "legacy", &env).unwrap();
    std::fs::remove_file(&calls).unwrap();
    refused(
        app.reset_container("legacy", "reset-job")
            .await
            .unwrap_err(),
    );
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect thoric-manager\n"
    );
    std::fs::remove_file(&calls).unwrap();
    refused(
        app.retry_container("legacy", "retry-job")
            .await
            .unwrap_err(),
    );
    let made = std::fs::read_to_string(&calls).unwrap();
    assert_eq!(
        made.lines().last(),
        Some("container inspect thoric-manager"),
        "{made}"
    );
    assert!(
        !made.contains("network") && !made.contains("create"),
        "{made}"
    );
    let saved: model::Container = app.db.get(model::CONTAINERS, "legacy").unwrap();
    assert!(saved.last_error.unwrap().contains("manager's own volume"));
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_compose_volume_parameter_cannot_bring_back_the_managers_own_volume() {
    let mut app = app(false);
    let calls = manager_docker(&mut app);
    // A container whose compose volume parameter named the data volume before creation refused
    // it: neither a reset nor a retry builds a desktop with it again.
    let setup: crate::compose::Setup = serde_json::from_value(serde_json::json!({
        "status": "completed", "image": "img", "parameters": {"DATA": "/opt/data"},
        "secrets": [],
        "mounts": [{"volume": "thoric-data", "target": "/opt/data", "readonly": false}],
    }))
    .unwrap();
    let env = model::Container {
        id: "composed".into(),
        container: "thoric-composed".into(),
        private_network: "thoric-composed-private".into(),
        egress_network: "thoric-composed-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: Some(setup),
    };
    app.db.put(model::CONTAINERS, "composed", &env).unwrap();
    let refused = |error: anyhow::Error| {
        let message = error.to_string();
        assert!(
            message.contains("thoric-data is the manager's own volume"),
            "{message}"
        );
    };
    refused(
        app.reset_container("composed", "reset-job")
            .await
            .unwrap_err(),
    );
    // Refused before the old container is removed.
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect thoric-manager\n"
    );
    std::fs::remove_file(&calls).unwrap();
    refused(
        app.retry_container("composed", "retry-job")
            .await
            .unwrap_err(),
    );
    let made = std::fs::read_to_string(&calls).unwrap();
    assert_eq!(
        made.lines().last(),
        Some("container inspect thoric-manager"),
        "{made}"
    );
    assert!(
        !made.contains("network") && !made.contains("create"),
        "{made}"
    );
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_desktop_cannot_be_created_with_a_volume_the_filter_leaves_out() {
    let mut app = app(false);
    let calls = manager_docker(&mut app);
    Arc::get_mut(&mut app).unwrap().config.volume_filter = config::volume_filter(None).unwrap();
    let mut spec = spec();
    spec.mounts.push(model::Mount {
        volume: "outside".into(),
        target: "/workspace".into(),
        readonly: false,
    });
    let error = app
        .create_container("outside", spec, Default::default(), "create-job")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Volume not found");
    // Refused before the image is looked at or downloaded, and nothing is saved.
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap(),
        "container inspect thoric-manager\nvolume inspect outside\n"
    );
    assert!(
        app.db
            .list::<model::Container>(model::CONTAINERS)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(calls.parent().unwrap()).unwrap();
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
            .validate(&spec.mounts)
            .is_err(),
            "{path}"
        );
    }
    assert!(
        files::DeleteFile {
            path: "/shared/project".into(),
            recursive: true
        }
        .validate(&spec.mounts)
        .is_ok()
    );
    // A volume a compose parameter mounts is a root too, though the settings never list it.
    let composed = model::Mount {
        volume: "thoric-c-DATA".into(),
        target: "/home/desktop/Projects".into(),
        readonly: false,
    };
    let data = files::DeleteFile {
        path: "/home/desktop/Projects".into(),
        recursive: true,
    };
    assert!(data.validate(&spec.mounts).is_ok());
    let mut all = spec.mounts.clone();
    all.push(composed);
    assert!(data.validate(&all).is_err());
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
    let check = |target: &str, name: &str| uploads::check_target(&[], &[], target, name);
    assert!(check("/home/desktop/Downloads/../.ssh", "id_rsa").is_err());
    assert!(check("/etc", "clock.jpg").is_err());
    assert!(check("/home/desktop/Downloads", "clock.jpg").is_ok());
    assert_eq!(
        files::download_url("abc123", "/home/desktop/Downloads/saat 17:20.jpg"),
        "/api/containers/abc123/files/download?path=/home/desktop/Downloads/saat%2017%3A20.jpg"
    );
    assert_eq!(
        files::download_url("abc123", "/shared/ölçüm.tar.gz"),
        "/api/containers/abc123/files/download?path=/shared/%C3%B6l%C3%A7%C3%BCm.tar.gz"
    );
}
#[tokio::test]
async fn container_settings_are_checked_before_any_job_or_docker_work() {
    let mut app = app(false);
    {
        let config = &mut Arc::get_mut(&mut app).unwrap().config;
        config.audio = false;
        config.microphone = false;
    }
    let mut existing = spec();
    existing.gpu = true;
    let env = model::Container {
        id: "edit-id".into(),
        container: "thoric-edit-id".into(),
        private_network: "thoric-edit-id-private".into(),
        egress_network: "thoric-edit-id-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: existing,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(model::CONTAINERS, &env.id, &env).unwrap();
    let post = |id: &str, body: serde_json::Value| {
        let app = app.clone();
        let request = Request::builder()
            .header("host", "localhost:8080")
            .header("x-thoric-request", "1")
            .header("content-type", "application/json")
            .method("POST")
            .uri(format!("/api/containers/{id}/settings"))
            .body(Body::from(body.to_string()))
            .unwrap();
        async move {
            let response = router(app).oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = axum::body::to_bytes(response.into_body(), 65536)
                .await
                .unwrap();
            (
                status,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            )
        }
    };
    let (status, body) = post(
        "edit-id",
        serde_json::json!({"name": "Renamed", "cpus": 2, "resolution": "1280x720", "dry_run": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["live"], serde_json::json!(["name", "cpus"]));
    assert_eq!(body["restart"], serde_json::json!(["resolution"]));
    // A feature the installation switched off cannot be turned on, but a desktop that has one
    // (GPU here, created before the switch) can still change its other settings.
    for (request, expected) in [
        (
            serde_json::json!({"audio": true, "dry_run": true}),
            "Audio is disabled",
        ),
        (
            serde_json::json!({"image": "thoric/custom:other"}),
            "cannot change",
        ),
        (serde_json::json!({"dry_run": "yes"}), "dry_run"),
        (
            serde_json::json!({"volume": "x"}),
            "Unknown container setting",
        ),
    ] {
        let (status, body) = post("edit-id", request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["error"].as_str().unwrap().contains(expected), "{body}");
    }
    let (status, _) = post("missing-id", serde_json::json!({"dry_run": true})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
    let saved: model::Container = app.db.get(model::CONTAINERS, "edit-id").unwrap();
    assert_eq!(saved.spec.name, "Unity");
}
#[test]
fn a_recreated_container_gets_every_setting_from_the_commit_that_holds_its_files() {
    let mut app = app(false);
    Arc::get_mut(&mut app).unwrap().config.adapter = "NVIDIA".into();
    let mut spec = spec();
    spec.resolution = "1280x720".into();
    spec.audio = true;
    spec.gpu = true;
    spec.gamepad = true;
    spec.vpn = true;
    spec.shm_mb = 1024;
    spec.memory_mb = Some(2048);
    spec.mounts = vec![model::Mount {
        volume: "work".into(),
        target: "/workspace".into(),
        readonly: true,
    }];
    let mut env = model::Container {
        id: "args-id".into(),
        container: "thoric-args-id".into(),
        private_network: "thoric-args-id-private".into(),
        egress_network: "thoric-args-id-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    let joined = |env: &model::Container| app.container_args(env, Some("172.30.0.0/16")).join(" ");
    let args = joined(&env);
    let profile = std::path::Path::new(DESKTOP_CONTEXT).join("seccomp.json");
    let seccomp = format!("--security-opt seccomp={}", profile.display());
    for flag in [
        seccomp.as_str(),
        "--name thoric-args-id",
        "--shm-size 1024m",
        "--env DESKTOP_RESOLUTION=1280x720",
        "--env DESKTOP_AUDIO=true",
        "--env DESKTOP_GPU=true",
        "--env GPU_ADAPTER=NVIDIA",
        "--env DESKTOP_PRIVATE_SUBNET=172.30.0.0/16",
        "--memory 2048m",
        "--mount type=volume,src=work,dst=/workspace,readonly",
        "--gpus all",
        "--device /dev/dxg",
        "--device /dev/uinput",
        "--cap-add NET_ADMIN",
        "--device /dev/net/tun",
        "--sysctl net.ipv4.conf.all.src_valid_mark=1",
        "--label thoric.environment=args-id",
    ] {
        assert!(args.contains(flag), "{flag} missing from {args}");
    }
    assert!(args.ends_with(" sha256:source"), "{args}");
    env.working_image = Some(model::WorkingImage {
        image_id: "sha256:working".into(),
        reference: "thoric/container:job".into(),
    });
    assert!(joined(&env).ends_with(" sha256:working"));
    assert_eq!(env.source_image_id, "sha256:source");
}
#[test]
fn a_host_without_the_tun_device_names_the_vpn_setting() {
    let app = app(false);
    let mut env = model::Container {
        id: "vpn-id".into(),
        container: "thoric-vpn-id".into(),
        private_network: "thoric-vpn-id-private".into(),
        egress_network: "thoric-vpn-id-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    let docker = || {
        anyhow::anyhow!(
            "error gathering device information while adding custom device \"/dev/net/tun\": no such file or directory"
        )
    };
    // Without VPN access the device is not the container's, and Docker's words stay as they are.
    assert!(!format!("{:#}", app.device_error(&env, docker())).contains("vpn"));
    env.spec.vpn = true;
    let error = format!("{:#}", app.device_error(&env, docker()));
    assert!(
        error.starts_with("VPN access needs /dev/net/tun"),
        "{error}"
    );
    assert!(error.contains("without vpn"), "{error}");
}
#[test]
fn vnc_listens_in_the_ipv4_subnet_of_the_private_network() {
    let network = |configs: serde_json::Value| serde_json::json!({"IPAM": {"Config": configs}});
    let dual = network(serde_json::json!([
        {"Subnet": "fd00:7468::/64", "Gateway": "fd00:7468::1"},
        {"Subnet": "172.30.0.0/16"},
    ]));
    assert_eq!(docker::private_subnet(&dual), Some("172.30.0.0/16"));
    // Without an IPv4 subnet the desktop is told nothing and listens on every interface.
    let ipv6 = network(serde_json::json!([{"Subnet": "fd00:7468::/64"}]));
    assert_eq!(docker::private_subnet(&ipv6), None);
    assert_eq!(
        docker::private_subnet(&network(serde_json::Value::Null)),
        None
    );
}
#[test]
fn an_installation_without_user_namespaces_leaves_dockers_seccomp_profile_in_place() {
    let mut app = app(false);
    Arc::get_mut(&mut app).unwrap().config.user_namespaces = false;
    let env = model::Container {
        id: "plain-id".into(),
        container: "thoric-plain-id".into(),
        private_network: "thoric-plain-id-private".into(),
        egress_network: "thoric-plain-id-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    assert_eq!(app.config.seccomp_profile(), None);
    let args = app.container_args(&env, None);
    assert!(!args.iter().any(|arg| arg == "--security-opt"), "{args:?}");
}
#[test]
fn the_desktop_seccomp_profile_adds_only_user_namespaces_to_dockers_default() {
    let profile: serde_json::Value =
        serde_json::from_str(include_str!("../../../images/desktop/seccomp.json")).unwrap();
    assert_eq!(profile["defaultAction"], "SCMP_ACT_ERRNO");
    let rules = profile["syscalls"].as_array().unwrap();
    let unconditional = |name: &str| {
        rules.iter().any(|rule| {
            rule["action"] == "SCMP_ACT_ALLOW"
                && rule["names"].as_array().unwrap().iter().any(|n| n == name)
                && rule.get("args").is_none()
                && rule.get("includes").is_none()
        })
    };
    // A sandbox starts by cloning or unsharing into a new user namespace.
    assert!(unconditional("clone") && unconditional("unshare"));
    // Calls Docker reserves for CAP_SYS_ADMIN stay refused, so a process in its own namespace
    // still cannot mount file systems or join another process's namespaces.
    for name in ["mount", "umount2", "fsopen", "open_tree", "setns", "bpf"] {
        assert!(
            !unconditional(name),
            "{name} is allowed without CAP_SYS_ADMIN"
        );
    }
    // glibc falls back from clone3 to clone only when clone3 reports ENOSYS.
    assert!(
        rules
            .iter()
            .any(|rule| rule["names"] == serde_json::json!(["clone3"])
                && rule["errnoRet"] == 38
                && rule["excludes"]["caps"] == serde_json::json!(["CAP_SYS_ADMIN"]))
    );
}
#[tokio::test]
async fn switched_off_features_are_refused_and_a_container_that_has_them_keeps_them() {
    let mut app = app(false);
    {
        let config = &mut Arc::get_mut(&mut app).unwrap().config;
        config.internet = false;
        config.sudo = false;
        config.gamepad = false;
        config.vpn = false;
    }
    let without = || {
        let mut spec = spec();
        spec.internet = false;
        spec.sudo = false;
        spec.gamepad = false;
        spec.vpn = false;
        spec
    };
    // A new container cannot have them.
    for (feature, says) in [
        ("internet", "INTERNET_ENABLED"),
        ("sudo", "SUDO_ENABLED"),
        ("gamepad", "GAMEPAD_ENABLED"),
        ("vpn", "VPN_ENABLED"),
    ] {
        let mut spec = without();
        match feature {
            "internet" => spec.internet = true,
            "sudo" => spec.sudo = true,
            "gamepad" => spec.gamepad = true,
            _ => spec.vpn = true,
        }
        let error = app
            .create_container("refused", spec.clone(), Default::default(), "create-job")
            .await
            .unwrap_err();
        assert!(error.to_string().contains(says), "{error}");
        // Nor can a settings change turn one on.
        let env = model::Container {
            id: "off".into(),
            container: "thoric-off".into(),
            private_network: "thoric-off-private".into(),
            egress_network: "thoric-off-egress".into(),
            source_image_id: "sha256:source".into(),
            created_at: 0,
            spec: without(),
            last_error: None,
            desktop: None,
            working_image: None,
            setup: None,
        };
        let request = serde_json::json!({ feature: true });
        let error = app
            .settings_preview(&env, request.as_object().unwrap())
            .unwrap_err();
        assert!(error.to_string().contains(says), "{error}");
    }
    // A container created before the switch keeps them all, also when it is created again.
    let mut kept = spec();
    kept.gamepad = true;
    kept.vpn = true;
    let env = model::Container {
        id: "kept".into(),
        container: "thoric-kept".into(),
        private_network: "thoric-kept-private".into(),
        egress_network: "thoric-kept-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec: kept,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    let (_, changes) = app
        .settings_preview(
            &env,
            serde_json::json!({"name": "Renamed"}).as_object().unwrap(),
        )
        .unwrap();
    assert_eq!(changes.live, ["name"]);
    let args = app.container_args(&env, None);
    assert!(args.iter().any(|arg| arg == "/dev/uinput"), "{args:?}");
    assert!(args.iter().any(|arg| arg == "NET_ADMIN"), "{args:?}");
    assert!(
        args.iter().any(|arg| arg == "DESKTOP_SUDO=true"),
        "{args:?}"
    );
    // What the manager carries itself stops while the feature is off.
    app.db.put(model::CONTAINERS, &env.id, &env).unwrap();
    let input: gamepad::GamepadInput =
        serde_json::from_value(serde_json::json!({"actions": [{"type": "reset"}]})).unwrap();
    let error = gamepad::gamepad_input(&app, "kept", &input)
        .await
        .unwrap_err();
    assert!(error.1.contains("GAMEPAD_ENABLED"), "{}", error.1);
    // Internet cannot be given while it is off; the switch refuses before any job, while turning
    // it off goes on as before.
    let internet = |enabled: bool| {
        let app = app.clone();
        async move {
            router(app)
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/containers/kept/internet")
                        .header("host", "localhost:8080")
                        .header("x-thoric-request", "1")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({ "enabled": enabled }).to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }
    };
    assert_eq!(internet(true).await, StatusCode::BAD_REQUEST);
    assert!(app.db.list::<model::Job>("job").unwrap().is_empty());
}
#[tokio::test]
async fn the_settings_route_tells_the_features_and_their_defaults() {
    let mut app = app(false);
    {
        let config = &mut Arc::get_mut(&mut app).unwrap().config;
        config.wsl_gpu = false;
        config.cuda = false;
        config.sudo = false;
    }
    let response = router(app)
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        (
            &body["gpu_enabled"],
            &body["sudo_enabled"],
            &body["internet_enabled"]
        ),
        (
            &serde_json::json!(false),
            &serde_json::json!(false),
            &serde_json::json!(true)
        )
    );
    // The configured defaults turn GPU access and sudo on, which the installation has off.
    assert_eq!(
        body["defaults"],
        serde_json::json!({"gpu": false, "internet": true, "sudo": false, "audio": false, "microphone": false, "gamepad": false, "vpn": false})
    );
    // DRI_GPU is the one GPU setting left on, and the manager cannot see this host.
    assert_eq!(
        body["gpu"],
        serde_json::json!({"routes": [], "unused": [{"route": "dri", "setting": "DRI_GPU", "reason": "unseen"}], "found": null})
    );
}
