//! What the settings page is shown of the configuration: every setting Compose passes to the
//! manager, as `.env.example` names it, and never a secret's value.
use super::*;
use axum::{body::Body, http::Request};
use std::{path::Path, sync::Arc};
use tower::ServiceExt;

fn read(path: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

/// The names in the `environment:` list of a manifest's manager service.
fn passed(manifest: &str) -> Vec<String> {
    let service = &manifest[manifest.find("\n  manager:\n").unwrap()..];
    let list = &service[service.find("\n    environment:\n").unwrap() + 18..];
    list.lines()
        .take_while(|line| line.starts_with("      "))
        .map(|line| line.trim().split(':').next().unwrap().to_owned())
        .collect()
}

fn names(config: &Config) -> Vec<String> {
    config
        .shown()
        .iter()
        .map(|setting| setting["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_page_shows_every_setting_compose_passes_to_the_manager() {
    let app = crate::tests::app(true);
    let shown = names(&app.config);
    let example = read(".env.example");
    let (sections, catalog) = (
        read("web/src/lib/settings.ts"),
        read("web/src/locales/en.ts"),
    );
    for name in &shown {
        assert!(
            example.contains(&format!("\n{name}=")),
            ".env.example must document {name}"
        );
        // Otherwise the page lists it under Other, without a word on what it does.
        assert!(
            sections.contains(&format!("\n  {name}: \"")),
            "web/src/lib/settings.ts must give {name} a section"
        );
        assert!(
            catalog.contains(&format!("\"settings.about.{name}\":")),
            "web/src/locales/en.ts must say what {name} does"
        );
    }
    for manifest in ["compose.yml", "compose.ghcr.yml"] {
        let passed = passed(&read(manifest));
        assert!(passed.len() > 10, "{manifest}: {passed:?}");
        // The log filter is read by the logger, not the configuration.
        for name in passed.iter().filter(|name| *name != "RUST_LOG") {
            assert!(
                shown.contains(name),
                "{manifest} passes {name}, which the settings page does not show"
            );
        }
    }
}

#[test]
fn a_secret_says_only_whether_it_is_set() {
    let mut app = crate::tests::app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.password_hash = Some("$argon2id$v=19$stored-password-hash".into());
    config.mcp_token = Some("token-of-the-agent-endpoint".into());
    let shown = serde_json::to_string(&config.shown()).unwrap();
    assert!(!shown.contains("stored-password-hash"), "{shown}");
    assert!(!shown.contains("token-of-the-agent-endpoint"), "{shown}");
    let setting = |config: &Config, name: &str| {
        config
            .shown()
            .into_iter()
            .find(|setting| setting["name"] == name)
            .unwrap()
    };
    assert_eq!(
        setting(config, "MCP_TOKEN"),
        json!({"name": "MCP_TOKEN", "secret": true, "set": true})
    );
    assert_eq!(setting(config, "ADMIN_PASSWORD")["set"], true);
    config.mcp_token = None;
    config.password_hash = None;
    assert_eq!(setting(config, "MCP_TOKEN")["set"], false);
    assert_eq!(setting(config, "MCP_AUTH_MODE")["value"], "none");
    assert_eq!(setting(config, "ADMIN_PASSWORD")["set"], false);
    // Values are what the manager made of the environment, as .env would spell them.
    config.templates_url = None;
    config.upload_limit = 1024 * 1024 * 1024;
    assert_eq!(setting(config, "TEMPLATES_URL")["value"], "none");
    assert_eq!(setting(config, "UPLOAD_MAX_MB")["value"], 1024);
}

#[tokio::test]
async fn the_configuration_route_needs_a_session_and_is_documented() {
    let response = crate::router(crate::tests::app(true))
        .oneshot(
            Request::builder()
                .uri("/api/configuration")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    let response = crate::router(crate::tests::app(false))
        .oneshot(
            Request::builder()
                .uri("/api/configuration")
                .header("host", "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["variables"][0]["name"], "AUTH_MODE");
    assert!(read("docs/API.md").contains("| GET | `/api/configuration` |"));
}
