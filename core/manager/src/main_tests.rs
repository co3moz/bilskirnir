//! Pins down what the router puts in front of the routes: the content security policy, the
//! caching of the interface's page and its hashed assets, and that the routes which reach Docker,
//! a desktop or a container's files answer only a signed-in dashboard and only its own requests.
use super::*;
use crate::test_support::app;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

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
        "find-text",
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
