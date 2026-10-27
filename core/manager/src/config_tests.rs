//! What the settings page is shown of the configuration: every setting Compose passes to the
//! manager, as `.env.example` names it, and never a secret's value.
use super::*;
use crate::tests::DESKTOP_CONTEXT;
use axum::{body::Body, http::Request};
use std::{collections::BTreeMap, path::Path, sync::Arc};
use tower::ServiceExt;

/// A file of the repository, two folders above the crate.
fn read(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path),
    )
    .unwrap()
}

/// The lines of the `environment:` list of a manifest's manager service, without its comments.
fn environment(manifest: &str) -> Vec<&str> {
    let service = &manifest[manifest.find("\n  manager:\n").unwrap()..];
    let list = &service[service.find("\n    environment:\n").unwrap() + 18..];
    list.lines()
        .take_while(|line| line.starts_with("      "))
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .collect()
}

/// The names in the `environment:` list of a manifest's manager service.
fn passed(manifest: &str) -> Vec<String> {
    environment(manifest)
        .into_iter()
        .map(|line| line.split(':').next().unwrap().to_owned())
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
        read("core/web/src/lib/settings.ts"),
        read("core/web/src/locales/en.ts"),
    );
    for name in &shown {
        assert!(
            example.contains(&format!("\n{name}=")),
            ".env.example must document {name}"
        );
        // Otherwise the page lists it under Other, without a word on what it does.
        assert!(
            sections.contains(&format!("\n  {name}: \"")),
            "core/web/src/lib/settings.ts must give {name} a section"
        );
        assert!(
            catalog.contains(&format!("\"settings.about.{name}\":")),
            "core/web/src/locales/en.ts must say what {name} does"
        );
    }
    for manifest in ["compose.yml", "compose.ghcr.yml"] {
        let passed = passed(&read(manifest));
        assert!(passed.len() > 10, "{manifest}: {passed:?}");
        // The log filter is read by the logger, not the configuration, and a renamed variable
        // only stands in for its new name.
        let renamed = |name: &String| RENAMED.iter().any(|renamed| renamed.old == name);
        for name in passed
            .iter()
            .filter(|name| *name != "RUST_LOG" && !renamed(name))
        {
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

#[test]
fn a_renamed_variable_keeps_its_meaning_and_startup_says_to_rename_it() {
    let read = |vars: &[(&str, &str)]| {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        let get = move |key: &str| {
            vars.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        let mut notices = Vec::new();
        let value = renamed_value(&get, &RENAMED[0], &mut notices);
        (value, notices)
    };
    // The new name alone, or the old one empty as Compose passes it once .env lost it.
    for vars in [
        &[("ROOT_MCP_ENABLED", "true")][..],
        &[("ROOT_MCP_ENABLED", "true"), ("MCP_ENABLED", "")],
    ] {
        let (value, notices) = read(vars);
        assert_eq!(value.unwrap().as_deref(), Some("true"));
        assert!(notices.is_empty(), "{notices:?}");
    }
    assert_eq!(read(&[]).0.unwrap(), None);
    // An .env from before the rename keeps its value and hears of the new name and the release.
    let (value, notices) = read(&[("MCP_ENABLED", "true")]);
    assert_eq!(value.unwrap().as_deref(), Some("true"));
    assert!(
        notices[0].contains("MCP_ENABLED was renamed ROOT_MCP_ENABLED in Thoric 0.6.3"),
        "{notices:?}"
    );
    // As the manifests pass it: the new name falls back to the old one, which is passed too.
    let (value, notices) = read(&[("ROOT_MCP_ENABLED", "true"), ("MCP_ENABLED", "true")]);
    assert_eq!(value.unwrap().as_deref(), Some("true"));
    assert!(notices[0].contains("Remove MCP_ENABLED"), "{notices:?}");
    // Two names that disagree are not guessed between.
    let error = read(&[("ROOT_MCP_ENABLED", "false"), ("MCP_ENABLED", "true")])
        .0
        .unwrap_err()
        .to_string();
    assert!(error.contains("disagree"), "{error}");
}

#[test]
fn the_manifests_carry_a_renamed_variable_over_to_its_new_name() {
    for manifest in ["compose.yml", "compose.ghcr.yml"] {
        let manifest = read(manifest);
        let lines = environment(&manifest);
        for Renamed { old, new, .. } in RENAMED {
            // The old value applies while .env has no new one, and the old name reaches the
            // manager, empty once it is gone, so startup can tell an .env that still has it.
            assert!(
                lines
                    .iter()
                    .any(|line| line.starts_with(&format!("{new}: ${{{new}:-${{{old}:-"))),
                "{lines:?}"
            );
            assert!(
                lines.contains(&format!("{old}: ${{{old}:-}}").as_str()),
                "{lines:?}"
            );
        }
    }
    let example = read(".env.example");
    for Renamed { old, new, .. } in RENAMED {
        assert!(example.contains(&format!("\n{new}=")));
        assert!(!example.contains(&format!("\n{old}=")));
    }
}

#[test]
fn the_page_edits_only_the_settings_it_may_change() {
    let app = crate::tests::app(true);
    let shown = app.config.shown();
    let names = names(&app.config);
    for name in EDITABLE {
        assert!(
            names.iter().any(|shown| shown == name),
            "{name} is not shown"
        );
    }
    // Sign-in, the root MCP and its token, and what Compose reads stay with thoric settings.
    for name in [
        "AUTH_MODE",
        "ADMIN_USERNAME",
        "ADMIN_PASSWORD",
        "MANAGER_CONTAINER",
        "ROOT_MCP_ENABLED",
        "MCP_AUTH_MODE",
        "MCP_TOKEN",
        "DESKTOP_HOST",
    ] {
        assert!(!EDITABLE.contains(&name), "{name}");
    }
    for entry in &shown {
        let editable = EDITABLE.contains(&entry["name"].as_str().unwrap());
        if entry["secret"] == true {
            assert!(!editable && entry.get("editable").is_none(), "{entry}");
        } else {
            assert_eq!(entry["editable"], editable, "{entry}");
            assert_eq!(entry.get("text").is_some(), editable, "{entry}");
        }
    }
}

#[test]
fn a_value_from_the_dashboard_is_one_line_that_env_keeps_as_it_is() {
    for fine in [
        "",
        "true",
        "1024",
        "https://example.com/templates/index.json",
        "GeForce RTX 4090",
        "team=desktops",
        "desktop.lan,other.lan",
    ] {
        env_value("X", fine).unwrap();
    }
    let long = "x".repeat(2049);
    for refused in [
        "a\nb", "a\rb", "a\tb", " a", "a ", "$HOME", "${X}", "a #b", "'a'", "\"a\"", "a`b`",
        r"a\b", "é", &long,
    ] {
        assert!(env_value("X", refused).is_err(), "{refused:?}");
    }
}

#[test]
fn a_change_is_read_as_the_manager_would_start_with_it() {
    let mut app = crate::tests::app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    // Sign-in stays on, but the password, which only its hash is kept of, is not read again.
    config.environment = BTreeMap::from([
        ("AUTH_MODE".into(), "password".into()),
        ("UPLOAD_MAX_MB".into(), "1024".into()),
        ("ALLOWED_HOSTS".into(), "desktop.lan".into()),
        ("DESKTOP_HOST".into(), "secure.lan".into()),
        ("DESKTOP_CONTEXT".into(), DESKTOP_CONTEXT.into()),
    ]);
    let change = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    };
    let changed = config
        .with_changes(&change(&[
            ("UPLOAD_MAX_MB", "2048"),
            ("ALLOWED_HOSTS", "a.lan"),
        ]))
        .unwrap();
    assert_eq!(changed.upload_limit, 2048 * 1024 * 1024);
    assert!(changed.auth);
    // ALLOWED_HOSTS is what .env says; DESKTOP_HOST comes on its own and is answered as well.
    assert_eq!(changed.allowed_hosts, ["a.lan"]);
    assert!(changed.host_allowed("a.lan") && changed.host_allowed("secure.lan"));
    assert!(!changed.host_allowed("desktop.lan"));
    assert_eq!(changed.environment["UPLOAD_MAX_MB"], "2048");
    // Each setting's problem is its own; a change of several is checked together too.
    let problems = config.check_changes(&change(&[
        ("UPLOAD_MAX_MB", "70000"),
        ("WSL_GPU", "false"),
        ("AUTH_MODE", "none"),
    ]));
    assert_eq!(
        problems.fields.keys().collect::<Vec<_>>(),
        ["AUTH_MODE", "UPLOAD_MAX_MB"]
    );
    assert!(problems.fields["AUTH_MODE"].contains("thoric settings"));
    assert!(problems.together.is_none());
    assert!(problems.to_string().contains("UPLOAD_MAX_MB must be"));
    assert!(
        config
            .check_changes(&change(&[("WSL_GPU", "false")]))
            .is_empty()
    );
}
