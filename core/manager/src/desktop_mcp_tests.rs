//! Desktop MCP servers and MCP endpoints: which declarations are accepted, what an endpoint offers
//! for its scopes and nothing more, the names its tools get, which single tools a grant leaves
//! out, and that an endpoint's token opens that endpoint and no other route.
use super::{
    client::{Exec, MAX_TOOLS, check_tool_list, exec_args, relayed},
    endpoints::{ENDPOINTS, Endpoint, Grant, GrantRequest, Servers, grants, slug_of, summary},
    registry::{
        Declaration, SERVERS, ServerRecord, ServerSpec, Source, check_declarations, found,
        loopback_url, substitute,
    },
    scopes::{Scope, Target, bound_tool, catalog, exposed_name, exposed_name_hashed},
    *,
};
use crate::{
    model::{CONTAINERS, Container},
    tests::{app, mcp_app},
};
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};
use tower::ServiceExt;

const ADMIN: &str = "test-token-with-at-least-32-characters";

pub(super) fn container(id: &str, name: &str) -> Container {
    Container {
        id: id.into(),
        container: format!("bilskirnir-{id}"),
        private_network: format!("bilskirnir-{id}-private"),
        egress_network: format!("bilskirnir-{id}-egress"),
        source_image_id: "sha256:chrome".into(),
        created_at: 1,
        spec: serde_json::from_value(json!({"name": name, "image": "sha256:chrome"})).unwrap(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}
fn stdio(command: &str) -> ServerSpec {
    ServerSpec {
        command: Some(command.into()),
        ..Default::default()
    }
}
fn tool(name: &str) -> rmcp::model::Tool {
    let mut tool = rmcp::model::Tool::new(
        name.to_owned(),
        format!("The {name} tool"),
        json!({"type":"object","properties":{"url":{"type":"string"}}})
            .as_object()
            .unwrap()
            .clone(),
    );
    tool.annotations = Some(rmcp::model::ToolAnnotations::new().read_only(true));
    tool
}
pub(super) fn server(app: &App, container_id: &str, name: &str, tools: &[&str]) {
    let record = ServerRecord {
        container_id: container_id.into(),
        name: name.into(),
        source: Source::Image,
        spec: Some(stdio("playwright-mcp")),
        tools: tools.iter().map(|t| tool(t)).collect(),
        tools_at: Some(1),
        error: None,
    };
    app.db
        .put(SERVERS, &ServerRecord::key(container_id, name), &record)
        .unwrap();
}
pub(super) fn grant(container_id: &str, alias: &str, scopes: &[Scope]) -> Grant {
    Grant {
        container_id: container_id.into(),
        alias: alias.into(),
        scopes: scopes.to_vec(),
        servers: Servers::All,
        excluded_tools: Vec::new(),
    }
}
pub(super) fn endpoint(name: &str, grants: Vec<Grant>) -> Endpoint {
    Endpoint {
        name: name.into(),
        automatic: false,
        grants,
        token_hash: None,
        token_created_at: None,
        created_at: 1,
    }
}
pub(super) fn names(app: &App, endpoint: &Endpoint) -> Vec<String> {
    catalog(app, endpoint)
        .into_iter()
        .map(|entry| entry.tool.name.to_string())
        .collect()
}

#[test]
fn a_declaration_is_a_command_or_a_loopback_url_and_never_both() {
    assert!(stdio("playwright-mcp").validate().is_ok());
    let http = ServerSpec {
        url: Some("http://127.0.0.1:8931/mcp".into()),
        headers: BTreeMap::from([("Authorization".into(), "Bearer ${API_TOKEN}".into())]),
        ..Default::default()
    };
    assert!(http.validate().is_ok());
    assert_eq!(http.transport(), "http");
    for refused in [
        ServerSpec::default(),
        ServerSpec {
            url: Some("http://127.0.0.1:1/mcp".into()),
            ..stdio("x")
        },
        // A program takes env, an HTTP server headers; mixing them is a mistake worth naming.
        ServerSpec {
            headers: BTreeMap::from([("X-Key".into(), "1".into())]),
            ..stdio("x")
        },
        ServerSpec {
            env: BTreeMap::from([("DISPLAY".into(), ":1".into())]),
            ..stdio("x")
        },
        ServerSpec {
            env: BTreeMap::from([("PATH".into(), "/tmp".into())]),
            ..stdio("x")
        },
        ServerSpec {
            cwd: Some("relative".into()),
            ..stdio("x")
        },
        ServerSpec {
            url: Some("http://127.0.0.1:8931/mcp".into()),
            headers: BTreeMap::from([("Mcp-Session-Id".into(), "x".into())]),
            ..Default::default()
        },
        ServerSpec {
            url: Some("http://127.0.0.1:8931/mcp".into()),
            headers: BTreeMap::from([("X-Key".into(), "a\r\nHost: evil".into())]),
            ..Default::default()
        },
    ] {
        assert!(refused.validate().is_err(), "{refused:?}");
    }
}

#[test]
fn an_http_server_is_on_the_desktops_own_loopback() {
    for url in [
        "http://127.0.0.1:8931/mcp",
        "https://localhost:443/mcp",
        "http://[::1]:9000/",
        "http://127.0.0.1:8931",
    ] {
        assert!(loopback_url(url, false).is_ok(), "{url}");
    }
    for url in [
        "http://example.com:80/mcp",
        "http://127.0.0.1/mcp",
        "http://127.0.0.1:0/mcp",
        "http://127.0.0.1.example.com:80/mcp",
        "http://user@127.0.0.1:80/mcp",
        "ftp://127.0.0.1:21/",
        "http://127.0.0.1:80/a b",
        "http://10.0.0.1:80/mcp",
    ] {
        assert!(loopback_url(url, false).is_err(), "{url}");
    }
    // A compose value may hold the port, and is checked again once it is known.
    assert!(loopback_url("http://127.0.0.1:${PORT}/mcp", true).is_ok());
    assert!(loopback_url("http://127.0.0.1:${PORT}/mcp", false).is_err());
}

#[test]
fn compose_values_fill_placeholders_and_a_missing_one_is_an_error() {
    let values = BTreeMap::from([
        ("PORT".to_owned(), "8931".to_owned()),
        ("chrome.TOKEN".to_owned(), "s3cret".to_owned()),
    ]);
    assert_eq!(
        substitute("http://127.0.0.1:${PORT}/mcp", &values).unwrap(),
        "http://127.0.0.1:8931/mcp"
    );
    assert_eq!(
        substitute("Bearer ${chrome.TOKEN}", &values).unwrap(),
        "Bearer s3cret"
    );
    assert_eq!(substitute("$HOME and $5", &values).unwrap(), "$HOME and $5");
    assert!(substitute("${MISSING}", &values).is_err());
    assert!(substitute("${PORT", &values).is_err());
    assert!(substitute("${a b}", &values).is_err());
    let spec = ServerSpec {
        url: Some("http://127.0.0.1:${PORT}/mcp".into()),
        headers: BTreeMap::from([("Authorization".into(), "Bearer ${chrome.TOKEN}".into())]),
        ..Default::default()
    };
    let resolved = spec.resolve(&values).unwrap();
    assert_eq!(resolved.url.as_deref(), Some("http://127.0.0.1:8931/mcp"));
    assert_eq!(resolved.headers["Authorization"], "Bearer s3cret");
}

#[test]
fn lists_name_variables_and_headers_but_never_show_their_values() {
    let spec = ServerSpec {
        url: Some("http://127.0.0.1:8931/mcp".into()),
        headers: BTreeMap::from([("Authorization".into(), "Bearer hunter2".into())]),
        ..Default::default()
    };
    let summary = spec.summary().to_string();
    assert!(summary.contains("Authorization"));
    assert!(!summary.contains("hunter2"));
}

#[test]
fn a_declaration_has_a_name_and_only_known_fields() {
    let declaration: Declaration = serde_json::from_value(
        json!({"name":"chrome","command":"playwright-mcp","args":["--cdp-endpoint","http://127.0.0.1:9223"]}),
    )
    .unwrap();
    assert_eq!(declaration.name, "chrome");
    assert_eq!(declaration.spec.args.len(), 2);
    assert!(declaration.validate().is_ok());
    assert!(serde_json::from_value::<Declaration>(json!({"command":"x"})).is_err());
    assert!(
        serde_json::from_value::<Declaration>(json!({"name":"x","command":"x","port":1})).is_err()
    );
    for name in ["Chrome", "-x", "a_b", "", &"a".repeat(33)] {
        let declaration = Declaration {
            name: name.into(),
            spec: stdio("x"),
        };
        assert!(declaration.validate().is_err(), "{name}");
    }
    // A build declares each server once.
    let twice = vec![
        Declaration {
            name: "chrome".into(),
            spec: stdio("a"),
        },
        Declaration {
            name: "chrome".into(),
            spec: stdio("b"),
        },
    ];
    assert!(check_declarations(&twice).is_err());
    assert!(check_declarations(&twice[..1]).is_ok());
}

#[test]
fn the_desktop_users_declaration_replaces_the_images_and_bad_files_become_errors() {
    let listing = json!({"servers":[
        {"name":"chrome","source":"image","spec":{"command":"playwright-mcp"}},
        {"name":"x","source":"image","error":"x.json is not valid JSON"},
        {"name":"chrome","source":"container","spec":{"command":"my-chrome-mcp"}},
        {"name":"bad","source":"container","spec":{"command":"a","url":"http://127.0.0.1:1/"}},
        {"name":"Not-A-Name","source":"container","spec":{"command":"a"}}
    ]});
    let servers = found(&listing).unwrap();
    assert_eq!(servers.len(), 3);
    let chrome = servers.iter().find(|s| s.name == "chrome").unwrap();
    assert_eq!(chrome.source, Source::Container);
    assert_eq!(
        chrome.spec.as_ref().unwrap().command.as_deref(),
        Some("my-chrome-mcp")
    );
    assert!(
        servers
            .iter()
            .find(|s| s.name == "x")
            .unwrap()
            .spec
            .is_err()
    );
    assert!(
        servers
            .iter()
            .find(|s| s.name == "bad")
            .unwrap()
            .spec
            .is_err()
    );
}

#[test]
fn a_server_runs_as_the_desktop_user_and_header_values_stay_off_the_command_line() {
    let spec = ServerSpec {
        env: BTreeMap::from([("BROWSER".into(), "chrome".into())]),
        args: vec!["--headless".into()],
        ..stdio("playwright-mcp")
    };
    let Exec { args, env, first } = exec_args("abc", &spec).unwrap();
    assert!(first.is_none());
    assert_eq!(&args[..4], ["exec", "-i", "--user", "1001:1001"]);
    // The variable is named on the command line and its value travels in the CLI's environment,
    // which may hold a compose secret.
    assert!(args.contains(&"BROWSER".to_owned()));
    assert!(!args.iter().any(|a| a.contains("chrome")));
    assert_eq!(env, [("BROWSER".to_owned(), "chrome".to_owned())]);
    assert!(
        args.iter()
            .any(|a| a.starts_with("PATH=/home/desktop/.local/bin:"))
    );
    let at = args.iter().position(|a| a == "abc").unwrap();
    assert_eq!(&args[at + 1..], ["playwright-mcp", "--headless"]);
    let http = ServerSpec {
        url: Some("http://127.0.0.1:8931/mcp".into()),
        headers: BTreeMap::from([("Authorization".into(), "Bearer hunter2".into())]),
        ..Default::default()
    };
    let Exec { args, first, .. } = exec_args("abc", &http).unwrap();
    assert!(!args.iter().any(|a| a.contains("hunter2")));
    let first: Value = serde_json::from_slice(&first.unwrap()).unwrap();
    assert_eq!(first["headers"]["Authorization"], "Bearer hunter2");
    assert_eq!(first["url"], "http://127.0.0.1:8931/mcp");
}

#[test]
fn exposed_names_are_valid_client_tool_names_of_at_most_64() {
    assert_eq!(
        exposed_name(&["chrome", "browser_navigate"]),
        "chrome__browser_navigate"
    );
    assert_eq!(exposed_name(&["x", "a.b/c"]), "x__a_b_c");
    let long = exposed_name(&["ext-dev", "chrome", &"t".repeat(80)]);
    assert_eq!(long.len(), 64);
    assert_ne!(long, exposed_name(&["ext-dev", "chrome", &"t".repeat(81)]));
    assert!(
        long.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    );
}

#[test]
fn a_bound_tool_takes_no_container_id_and_never_root() {
    let exec = crate::mcp::listed_tool("exec_container").unwrap();
    let bound = bound_tool(&exec, None);
    let properties = bound.input_schema["properties"].as_object().unwrap();
    assert!(!properties.contains_key("container_id"));
    assert!(!properties.contains_key("root"));
    assert_eq!(bound.input_schema["required"], json!(["script"]));
    let several = bound_tool(&exec, Some(&["a".into(), "b".into()]));
    assert_eq!(
        several.input_schema["properties"]["container"]["enum"],
        json!(["a", "b"])
    );
    assert!(
        several.input_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("container"))
    );
}

#[test]
fn an_endpoint_offers_its_scopes_tools_and_never_the_administrators() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    let viewer = endpoint("viewer", vec![grant("c1", "chrome-dev", &[Scope::View])]);
    let offered = names(&app, &viewer);
    assert!(offered.contains(&"screenshot".to_owned()));
    for absent in [
        "desktop_input",
        "exec_container",
        "get_job",
        "create_container",
        "delete_container",
        "update_container",
        "reset_container",
        "snapshot",
        "set_internet",
        "list_containers",
        "create_mcp_endpoint",
        "create_transfer",
    ] {
        assert!(!offered.contains(&absent.to_owned()), "{absent}");
    }
    // Every scope together still offers nothing that changes a container's settings or others.
    let everything = endpoint("all", vec![grant("c1", "chrome-dev", &Scope::ALL)]);
    let offered = names(&app, &everything);
    assert!(offered.contains(&"get_job".to_owned()));
    for administrator in [
        "create_container",
        "delete_container",
        "update_container",
        "reset_container",
        "retry_container",
        "snapshot",
        "set_internet",
        "list_containers",
        "build_image",
        "delete_image",
        "delete_volume",
        "list_mcp_endpoints",
    ] {
        assert!(
            !offered.contains(&administrator.to_owned()),
            "{administrator}"
        );
    }
    for name in Scope::ALL.iter().flat_map(|scope| scope.tools()) {
        assert!(
            crate::mcp::listed_tool(name).is_some(),
            "{name} is in a scope but not a Bilskirnir tool"
        );
    }
}

#[test]
fn desktop_server_tools_are_prefixed_and_their_own_claims_dropped() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    app.db
        .put(CONTAINERS, "c2", &container("c2", "Unity"))
        .unwrap();
    server(&app, "c1", "chrome", &["browser_navigate"]);
    server(&app, "c1", "x", &["reload_extension"]);
    server(&app, "c2", "unity", &["run_tests"]);
    let one = endpoint("one", vec![grant("c1", "chrome-dev", &[])]);
    let entries = catalog(&app, &one);
    let navigate = entries
        .iter()
        .find(|e| e.tool.name == "chrome__browser_navigate")
        .unwrap();
    // A desktop's server cannot make a client run its tool unasked by claiming it is read-only.
    assert!(navigate.tool.annotations.is_none());
    assert!(
        navigate
            .tool
            .description
            .as_deref()
            .unwrap()
            .starts_with("[Chrome dev / chrome]")
    );
    assert_eq!(
        navigate.target,
        Target::Server {
            container_id: "c1".into(),
            server: "chrome".into(),
            tool: "browser_navigate".into()
        }
    );
    assert!(names(&app, &one).contains(&"x__reload_extension".to_owned()));
    assert!(!names(&app, &one).iter().any(|n| n.contains("run_tests")));
    // A chosen list leaves the other servers out, also those added later.
    let mut chosen = one.clone();
    chosen.grants[0].servers = Servers::Named(vec!["x".into()]);
    assert_eq!(names(&app, &chosen), ["x__reload_extension"]);
    // Several containers: tools take the container, server tools carry its alias.
    let two = endpoint(
        "two",
        vec![
            grant("c1", "chrome-dev", &[Scope::View]),
            grant("c2", "unity", &[]),
        ],
    );
    let offered = names(&app, &two);
    assert!(offered.contains(&"chrome-dev__chrome__browser_navigate".to_owned()));
    assert!(offered.contains(&"unity__unity__run_tests".to_owned()));
    let screenshot = catalog(&app, &two)
        .into_iter()
        .find(|e| e.tool.name == "screenshot")
        .unwrap();
    // Only the containers granted the scope are offered for its tools.
    assert_eq!(
        screenshot.tool.input_schema["properties"]["container"]["enum"],
        json!(["chrome-dev"])
    );
    // A server that failed offers nothing.
    let mut failed: ServerRecord = app.db.get(SERVERS, &ServerRecord::key("c1", "x")).unwrap();
    failed.error = Some("crashed".into());
    app.db
        .put(SERVERS, &ServerRecord::key("c1", "x"), &failed)
        .unwrap();
    assert!(!names(&app, &one).contains(&"x__reload_extension".to_owned()));
}

#[test]
fn a_grant_leaves_single_tools_out_under_the_names_the_endpoint_gives_them() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    app.db
        .put(CONTAINERS, "c2", &container("c2", "Unity"))
        .unwrap();
    server(
        &app,
        "c1",
        "playwright",
        &["browser_navigate", "browser_run_code_unsafe", "a.b", "a_b"],
    );
    // One container: the deny list holds the exposed names, a hash tail included.
    let mut one = endpoint(
        "one",
        vec![grant(
            "c1",
            "chrome-dev",
            &[Scope::View, Scope::Applications],
        )],
    );
    let before = names(&app, &one);
    assert!(before.contains(&"playwright__a_b".to_owned()));
    let hashed = before
        .iter()
        .find(|name| name.starts_with("playwright__a_b_"))
        .unwrap()
        .clone();
    one.grants[0].excluded_tools = vec![
        "application_diagnostics".into(),
        "playwright__browser_run_code_unsafe".into(),
        "playwright__a_b".into(),
    ];
    let after = names(&app, &one);
    for absent in [
        "application_diagnostics",
        "playwright__browser_run_code_unsafe",
        "playwright__a_b",
    ] {
        assert!(!after.contains(&absent.to_owned()), "{absent}");
    }
    for present in [
        "screenshot",
        "launch_application",
        "playwright__browser_navigate",
        hashed.as_str(),
    ] {
        assert!(after.contains(&present.to_owned()), "{present}");
    }
    // Leaving one of two colliding tools out does not rename the other.
    assert_eq!(before.len(), after.len() + 3);
    // Several containers: a container tool stays for the others and takes only their aliases; a
    // server tool is named without the alias, as the container alone would expose it.
    let mut two = endpoint(
        "two",
        vec![
            grant("c1", "chrome-dev", &[Scope::View]),
            grant("c2", "unity", &[Scope::View]),
        ],
    );
    two.grants[0].excluded_tools = vec![
        "screenshot".into(),
        "playwright__browser_run_code_unsafe".into(),
    ];
    let entries = catalog(&app, &two);
    let screenshot = entries
        .iter()
        .find(|entry| entry.tool.name == "screenshot")
        .unwrap();
    assert_eq!(
        screenshot.tool.input_schema["properties"]["container"]["enum"],
        json!(["unity"])
    );
    assert_eq!(
        screenshot.target,
        Target::Bilskirnir {
            tool: "screenshot",
            containers: vec![("unity".into(), "c2".into())]
        }
    );
    let offered: Vec<String> = entries
        .iter()
        .map(|entry| entry.tool.name.to_string())
        .collect();
    assert!(offered.contains(&"chrome-dev__playwright__browser_navigate".to_owned()));
    assert!(!offered.contains(&"chrome-dev__playwright__browser_run_code_unsafe".to_owned()));
    // Once every grant leaves a tool out, it is gone.
    two.grants[1].excluded_tools = vec!["screenshot".into()];
    assert!(!names(&app, &two).contains(&"screenshot".to_owned()));
    // get_job reads only the jobs of the containers whose grants keep it.
    let mut jobs = endpoint(
        "jobs",
        vec![
            grant("c1", "a", &[Scope::Power]),
            grant("c2", "b", &[Scope::Power]),
        ],
    );
    jobs.grants[0].excluded_tools = vec!["get_job".into()];
    let job = catalog(&app, &jobs)
        .into_iter()
        .find(|entry| entry.tool.name == "get_job")
        .unwrap();
    assert_eq!(
        job.target,
        Target::Job {
            containers: vec!["c2".into()]
        }
    );
    jobs.grants[1].excluded_tools = vec!["get_job".into()];
    assert!(!names(&app, &jobs).contains(&"get_job".to_owned()));
}

#[test]
fn excluded_tools_are_checked_against_the_scopes_and_kept_for_servers_not_known_yet() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    let request = |scopes: &[Scope], excluded: &[&str]| GrantRequest {
        container_id: "c1".into(),
        alias: None,
        scopes: scopes.to_vec(),
        servers: Servers::All,
        excluded_tools: excluded.iter().map(|name| (*name).to_owned()).collect(),
    };
    let made = grants(
        &app,
        vec![request(
            &[Scope::Applications, Scope::Power],
            &[
                "application_diagnostics",
                "playwright__browser_run_code_unsafe",
                "get_job",
                "application_diagnostics",
            ],
        )],
    )
    .unwrap();
    assert_eq!(
        made[0].excluded_tools,
        [
            "application_diagnostics",
            "playwright__browser_run_code_unsafe",
            "get_job"
        ]
    );
    // A container tool outside the granted scopes is a typo worth naming, as is a name no
    // exposed tool can have.
    for (scopes, excluded) in [
        (&[Scope::View][..], &["application_diagnostics"][..]),
        (&[Scope::View], &["get_job"]),
        (&[Scope::View], &["screenshot "]),
        (&[Scope::View], &[""]),
        (&[Scope::View], &["a.b"]),
        (&[Scope::View], &["Screenshot"]),
        (&[Scope::View], &["Chrome__navigate"]),
        (&[Scope::View], &["__navigate"]),
    ] {
        assert!(
            grants(&app, vec![request(scopes, excluded)]).is_err(),
            "{excluded:?}"
        );
    }
    let longest = format!("s__{}", "a".repeat(61));
    assert!(grants(&app, vec![request(&[], &[&longest])]).is_ok());
    assert!(grants(&app, vec![request(&[], &[&format!("{longest}a")])]).is_err());
    let many: Vec<String> = (0..1025).map(|i| format!("s__t{i}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    assert!(grants(&app, vec![request(&[], &many)]).is_err());
    assert!(grants(&app, vec![request(&[], &many[..1024])]).is_ok());
    // A record from before the list reads as leaving nothing out, and a grant that leaves
    // nothing out is stored as records were, without the field.
    let old: Grant = serde_json::from_value(
        json!({"container_id":"c1","alias":"a","scopes":["view"],"servers":"all"}),
    )
    .unwrap();
    assert!(old.excluded_tools.is_empty());
    assert!(
        !serde_json::to_value(&old)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("excluded_tools")
    );
    // The dashboard and the administrator's tools see the list with each grant.
    let shown = summary(&app, &endpoint("e", made));
    assert_eq!(
        shown["grants"][0]["excluded_tools"],
        json!([
            "application_diagnostics",
            "playwright__browser_run_code_unsafe",
            "get_job"
        ])
    );
    assert_eq!(
        summary(&app, &endpoint("f", vec![old]))["grants"][0]["excluded_tools"],
        json!([])
    );
}

#[test]
fn server_lists_name_each_tool_as_an_endpoint_of_its_container_exposes_it() {
    let app = app(false);
    server(
        &app,
        "c1",
        "playwright",
        &["browser_navigate", "a.b", "a_b", "a/b"],
    );
    let record: ServerRecord = app
        .db
        .get(SERVERS, &ServerRecord::key("c1", "playwright"))
        .unwrap();
    let shown = super::servers::summary(&app, &record, "Chrome dev", true);
    let tools: Vec<(String, String)> = shown["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().unwrap().to_owned(),
                tool["exposed"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        tools[0],
        (
            "browser_navigate".to_owned(),
            "playwright__browser_navigate".to_owned()
        )
    );
    assert_eq!(tools[1], ("a.b".to_owned(), "playwright__a_b".to_owned()));
    assert_eq!(tools[2].0, "a_b");
    assert!(tools[2].1.starts_with("playwright__a_b_"));
    // The tail hashes the tool's own name, so a third tool that cleans the same gets its own.
    assert_eq!(tools[3].0, "a/b");
    assert!(tools[3].1.starts_with("playwright__a_b_") && tools[3].1 != tools[2].1);
    assert_eq!(tools.len(), 4);
    // The list's names are the catalog's on an endpoint of this container alone.
    let offered = names(&app, &endpoint("one", vec![grant("c1", "chrome-dev", &[])]));
    assert_eq!(
        offered,
        tools
            .iter()
            .map(|(_, exposed)| exposed.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn grants_name_existing_containers_once_with_their_own_aliases() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome Dev!"))
        .unwrap();
    app.db
        .put(CONTAINERS, "c2", &container("c2", "Chrome Dev"))
        .unwrap();
    let request = |id: &str, alias: Option<&str>| GrantRequest {
        container_id: id.into(),
        alias: alias.map(str::to_owned),
        scopes: vec![Scope::View, Scope::View],
        servers: Servers::All,
        excluded_tools: Vec::new(),
    };
    let made = grants(&app, vec![request("c1", None), request("c2", None)]).unwrap();
    assert_eq!(made[0].alias, "chrome-dev");
    assert_eq!(made[1].alias, "chrome-dev-2");
    assert_eq!(made[0].scopes, [Scope::View]);
    assert!(grants(&app, vec![]).is_err());
    assert!(grants(&app, vec![request("missing", None)]).is_err());
    assert!(grants(&app, vec![request("c1", None), request("c1", None)]).is_err());
    assert!(grants(&app, vec![request("c1", Some("Bad Alias"))]).is_err());
    assert!(
        grants(
            &app,
            vec![request("c1", Some("same")), request("c2", Some("same"))]
        )
        .is_err()
    );
    assert_eq!(
        slug_of("  Ünity 6 / Editor  ", 24, "desktop"),
        "nity-6-editor"
    );
    assert_eq!(slug_of("***", 24, "desktop"), "desktop");
    assert_eq!(
        serde_json::from_value::<Servers>(json!("all")).unwrap(),
        Servers::All
    );
    assert!(serde_json::from_value::<Servers>(json!("some")).is_err());
    assert_eq!(json!(Servers::Named(vec!["x".into()])), json!(["x"]));
}

#[tokio::test]
async fn a_new_containers_endpoint_is_named_after_it_and_leaves_with_it() {
    let app = app(false);
    let one = container("c1", "Chrome dev");
    let two = container("c2", "Chrome dev");
    app.db.put(CONTAINERS, "c1", &one).unwrap();
    app.db.put(CONTAINERS, "c2", &two).unwrap();
    assert_eq!(automatic_endpoint(&app, &one).await.unwrap(), "chrome-dev");
    assert_eq!(
        automatic_endpoint(&app, &two).await.unwrap(),
        "chrome-dev-2"
    );
    let own: Endpoint = app.db.get(ENDPOINTS, "chrome-dev").unwrap();
    assert!(own.automatic && own.token_hash.is_none());
    assert_eq!(own.grants[0].scopes, Scope::DEFAULT);
    assert!(!own.grants[0].scopes.contains(&Scope::Commands));
    // A custom endpoint covering both keeps the other container when one is deleted.
    app.db
        .put(
            ENDPOINTS,
            "both",
            &endpoint("both", vec![grant("c1", "a", &[]), grant("c2", "b", &[])]),
        )
        .unwrap();
    server(&app, "c1", "chrome", &["browser_navigate"]);
    forget_container(&app, "c1").unwrap();
    assert!(app.db.get::<Endpoint>(ENDPOINTS, "chrome-dev").is_err());
    assert!(app.db.get::<Endpoint>(ENDPOINTS, "chrome-dev-2").is_ok());
    let both: Endpoint = app.db.get(ENDPOINTS, "both").unwrap();
    assert_eq!(both.grants.len(), 1);
    assert_eq!(both.grants[0].container_id, "c2");
    assert!(
        app.db
            .get::<ServerRecord>(SERVERS, &ServerRecord::key("c1", "chrome"))
            .is_err()
    );
}

/// One JSON-RPC message to `/mcp/{name}`, with a bearer token and a session when given.
async fn call(
    app: Arc<App>,
    path: &str,
    token: Option<&str>,
    session: Option<&str>,
    message: Value,
) -> (StatusCode, Option<String>, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("host", "localhost:8080")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let response = crate::router(app)
        .oneshot(request.body(Body::from(message.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let session = response
        .headers()
        .get("mcp-session-id")
        .map(|v| v.to_str().unwrap().to_owned());
    assert!(!response.headers().contains_key("content-length"));
    let bytes = axum::body::to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    // Sessions answer on an event stream; the reply is its message with data.
    let text = String::from_utf8_lossy(&bytes);
    let body = serde_json::from_str(&text).unwrap_or_else(|_| {
        text.lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
            .next_back()
            .unwrap_or(Value::Null)
    });
    (status, session, body)
}
async fn open_session(app: &Arc<App>, path: &str, token: &str) -> String {
    let (status, session, body) = call(
        app.clone(),
        path,
        Some(token),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body["result"]["capabilities"]["tools"]["listChanged"] == true,
        "{body}"
    );
    let session = session.expect("a session for this protocol version");
    let (status, _, _) = call(
        app.clone(),
        path,
        Some(token),
        Some(&session),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    session
}

#[tokio::test]
async fn an_endpoint_token_opens_its_endpoint_and_nothing_else() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    server(&app, "c1", "chrome", &["browser_navigate"]);
    let Json(created) = create_endpoint(
        State(app.clone()),
        Json(
            serde_json::from_value(json!({"name":"chrome-dev","grants":[{"container_id":"c1","scopes":["view"],"servers":"all"}]}))
                .unwrap(),
        ),
    )
    .await
    .unwrap();
    let token = created["token"].as_str().unwrap().to_owned();
    assert_eq!(created["endpoint"]["path"], "/mcp/chrome-dev");
    assert_eq!(created["endpoint"]["has_token"], true);
    // The token is stored as a hash only.
    let stored: Endpoint = app.db.get(ENDPOINTS, "chrome-dev").unwrap();
    assert!(!serde_json::to_string(&stored).unwrap().contains(&token));

    let session = open_session(&app, "/mcp/chrome-dev", &token).await;
    let (status, _, body) = call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(&token),
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tools: Vec<&str> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(tools.contains(&"screenshot") && tools.contains(&"chrome__browser_navigate"));
    assert!(!tools.contains(&"delete_container") && !tools.contains(&"desktop_input"));
    // A tool outside the endpoint's scopes is unknown here, whatever /mcp offers. The refusal
    // sends the agent back to tools/list, since a tool may have gone with a desktop server.
    let (_, _, body) = call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(&token),
        Some(&session),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"delete_container","arguments":{"container_id":"c1"}}}),
    )
    .await;
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("Unknown tool") && message.contains("list the tools again"),
        "{body}"
    );
    // The container is the endpoint's own: an argument naming another is refused.
    let (_, _, body) = call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(&token),
        Some(&session),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_container","arguments":{"container_id":"other"}}}),
    )
    .await;
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Unknown argument"),
        "{body}"
    );

    // The endpoint's token is not the administrator's: not /mcp, not the API, not ports.
    let (status, _, _) = call(
        app.clone(),
        "/mcp",
        Some(&token),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for (method, uri) in [
        ("GET", "/api/containers"),
        ("GET", "/api/mcp/endpoints"),
        ("GET", "/api/containers/c1/ports/8080/"),
    ] {
        let response = crate::router(app.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("host", "localhost:8080")
                    .header("authorization", format!("Bearer {token}"))
                    .header("x-bilskirnir-request", "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    // Another endpoint's token does not open this one; the administrator's does.
    let Json(other) = create_endpoint(
        State(app.clone()),
        Json(
            serde_json::from_value(
                json!({"name":"other","grants":[{"container_id":"c1","scopes":[],"servers":[]}]}),
            )
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    let (status, _, _) = call(
        app.clone(),
        "/mcp/chrome-dev",
        other["token"].as_str(),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    open_session(&app, "/mcp/chrome-dev", ADMIN).await;
    // A replaced token stops working; a revoked one leaves only the administrator's.
    let Json(replaced) = super::token(State(app.clone()), Path("chrome-dev".into()))
        .await
        .unwrap();
    let (status, _, _) = call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(&token),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    open_session(&app, "/mcp/chrome-dev", replaced["token"].as_str().unwrap()).await;
    let Json(revoked) = revoke_token(State(app.clone()), Path("chrome-dev".into()))
        .await
        .unwrap();
    assert_eq!(revoked["has_token"], false);
    let (status, _, _) = call(
        app.clone(),
        "/mcp/chrome-dev",
        replaced["token"].as_str(),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_excluded_tool_is_unknown_on_the_endpoint() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    server(
        &app,
        "c1",
        "chrome",
        &["browser_navigate", "browser_run_code_unsafe"],
    );
    let Json(created) = create_endpoint(
        State(app.clone()),
        Json(
            serde_json::from_value(json!({"name":"chrome-dev","grants":[{"container_id":"c1","scopes":["view"],"servers":"all","excluded_tools":["screenshot","chrome__browser_run_code_unsafe"]}]}))
                .unwrap(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        created["endpoint"]["grants"][0]["excluded_tools"],
        json!(["screenshot", "chrome__browser_run_code_unsafe"])
    );
    let token = created["token"].as_str().unwrap().to_owned();
    let session = open_session(&app, "/mcp/chrome-dev", &token).await;
    let (_, _, body) = call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(&token),
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    )
    .await;
    let tools: Vec<&str> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        tools.contains(&"list_windows") && tools.contains(&"chrome__browser_navigate"),
        "{tools:?}"
    );
    assert!(
        !tools.contains(&"screenshot") && !tools.contains(&"chrome__browser_run_code_unsafe"),
        "{tools:?}"
    );
    // Called anyway, an excluded tool is as unknown as one outside the scopes.
    for name in ["screenshot", "chrome__browser_run_code_unsafe"] {
        let (_, _, body) = call(
            app.clone(),
            "/mcp/chrome-dev",
            Some(&token),
            Some(&session),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":name,"arguments":{}}}),
        )
        .await;
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Unknown tool"),
            "{body}"
        );
    }
    // A container tool of a scope the grant lacks is refused, so a typo cannot pass for one.
    let refused = update_endpoint(
        State(app.clone()),
        Path("chrome-dev".into()),
        Json(
            serde_json::from_value(json!({"grants":[{"container_id":"c1","scopes":["view"],"servers":"all","excluded_tools":["desktop_input"]}]}))
                .unwrap(),
        ),
    )
    .await;
    let Err(error) = refused else {
        panic!("a tool outside the scopes was accepted as excluded");
    };
    assert!(error.1.contains("desktop_input"), "{}", error.1);
    let kept: Endpoint = app.db.get(ENDPOINTS, "chrome-dev").unwrap();
    assert_eq!(kept.grants[0].excluded_tools.len(), 2);
}

#[tokio::test]
async fn endpoints_answer_only_from_the_dashboards_origin() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    app.db
        .put(
            ENDPOINTS,
            "chrome-dev",
            &endpoint(
                "chrome-dev",
                vec![grant("c1", "chrome-dev", &[Scope::View])],
            ),
        )
        .unwrap();
    let (status, _, _) = call(
        app.clone(),
        "/mcp/missing",
        Some(ADMIN),
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let response = crate::router(app.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/mcp/chrome-dev")
                .header("host", "localhost:8080")
                .header("origin", "https://evil.example")
                .header("authorization", format!("Bearer {ADMIN}"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn endpoints_answer_with_their_tokens_while_the_root_mcp_is_off() {
    // ROOT_MCP_ENABLED=false, without an administrator's token.
    let app = app(true);
    assert!(!app.config.root_mcp_enabled);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    let Json(created) = create_endpoint(
        State(app.clone()),
        Json(
            serde_json::from_value(json!({"name":"chrome-dev","grants":[{"container_id":"c1","scopes":["view"],"servers":"all"}]}))
                .unwrap(),
        ),
    )
    .await
    .unwrap();
    let token = created["token"].as_str().unwrap().to_owned();
    open_session(&app, "/mcp/chrome-dev", &token).await;
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    // The endpoint still needs its token, and the empty administrator's token opens nothing.
    for bearer in [None, Some(""), Some(ADMIN)] {
        let (status, _, _) = call(
            app.clone(),
            "/mcp/chrome-dev",
            bearer,
            None,
            initialize.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{bearer:?}");
    }
    // The root /mcp stays off, also for the endpoint's token.
    let (status, _, _) = call(app.clone(), "/mcp", Some(&token), None, initialize.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // MCP_AUTH_MODE=none opens the endpoints without a token only while /mcp is on: with the root
    // channel off, nothing answers an agent without a token.
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
    let mut closed = app_with_endpoint_but_root_mcp_off();
    Arc::get_mut(&mut closed).unwrap().config.mcp_token = None;
    let (status, _, body) = call(closed, "/mcp/chrome-dev", None, None, initialize.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    let mut open = app_with_endpoint_but_root_mcp_off();
    let config = &mut Arc::get_mut(&mut open).unwrap().config;
    config.mcp_token = None;
    config.root_mcp_enabled = true;
    let (status, _, body) = call(open, "/mcp/chrome-dev", None, None, initialize).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["result"]["serverInfo"].is_object(), "{body}");
}
fn app_with_endpoint_but_root_mcp_off() -> Arc<App> {
    let app = app(true);
    app.db
        .put(
            ENDPOINTS,
            "chrome-dev",
            &endpoint("chrome-dev", vec![grant("c1", "chrome-dev", &[])]),
        )
        .unwrap();
    app
}

#[tokio::test]
async fn get_job_reads_only_the_jobs_of_the_endpoints_containers() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    for (id, resource) in [("mine", "container:c1"), ("theirs", "container:c2")] {
        app.db
            .put(
                "job",
                id,
                &crate::model::Job {
                    id: id.into(),
                    kind: "exec".into(),
                    resource: Some(resource.into()),
                    status: "completed".into(),
                    created_at: 1,
                    finished_at: Some(2),
                    log: String::new(),
                    result: None,
                    progress: None,
                    stages: Vec::new(),
                    name: None,
                },
            )
            .unwrap();
    }
    app.db
        .put(
            ENDPOINTS,
            "chrome-dev",
            &endpoint(
                "chrome-dev",
                vec![grant("c1", "chrome-dev", &[Scope::Power])],
            ),
        )
        .unwrap();
    let session = open_session(&app, "/mcp/chrome-dev", ADMIN).await;
    let read = |job: &'static str| {
        call(
            app.clone(),
            "/mcp/chrome-dev",
            Some(ADMIN),
            Some(&session),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_job","arguments":{"job_id":job}}}),
        )
    };
    let (_, _, body) = read("mine").await;
    assert_eq!(body["result"]["structuredContent"]["id"], "mine", "{body}");
    let (_, _, body) = read("theirs").await;
    assert_eq!(body["result"]["isError"], true, "{body}");
}

#[test]
fn every_route_and_tool_is_documented() {
    let api = include_str!("../../../docs/API.md");
    for route in [
        "`/api/mcp/servers`",
        "`/api/mcp/scopes`",
        "`/api/mcp/inspect`",
        "`/api/mcp/endpoints`",
        "`/api/mcp/endpoints/{name}`",
        "`/api/mcp/endpoints/{name}/token`",
        "`/api/mcp/endpoints/{name}/tools`",
        "`/api/containers/{id}/mcp/servers`",
        "`/api/containers/{id}/mcp/servers/{name}`",
        "`/api/containers/{id}/mcp/servers/{name}/refresh`",
        "`/mcp/{name}`",
    ] {
        assert!(api.contains(route), "docs/API.md must list {route}");
    }
    let guide = include_str!("../../../docs/MCP.md");
    for scope in Scope::ALL {
        assert!(
            guide.contains(&format!("`{}`", scope.id())),
            "docs/MCP.md must explain the {} scope",
            scope.id()
        );
    }
}

#[tokio::test]
async fn a_scoped_endpoint_reports_no_host_gpu() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    app.usage.record(
        "c1",
        &crate::usage::stats::Measure {
            cpu: 0.5,
            memory: 1 << 20,
            memory_limit: 0,
            online_cpus: 4,
            disk_read: 0.0,
            disk_write: 0.0,
            net_rx: 0.0,
            net_tx: 0.0,
        },
    );
    let Json(created) = create_endpoint(
        State(app.clone()),
        Json(
            serde_json::from_value(json!({"name":"chrome-dev","grants":[{"container_id":"c1","scopes":["view"],"servers":"all"}]}))
                .unwrap(),
        ),
    )
    .await
    .unwrap();
    let token = created["token"].as_str().unwrap().to_owned();
    let session = open_session(&app, "/mcp/chrome-dev", &token).await;
    let (status, _, body) = call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(&token),
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_usage","arguments":{}}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let result = &body["result"];
    assert_ne!(result["isError"], true, "{body}");
    let usage = &result["structuredContent"];
    assert!(usage["containers"].is_array(), "{body}");
    // The host GPU is outside what the grant of one container covers, in both forms.
    assert!(usage.get("gpu").is_none(), "{body}");
    assert!(
        !serde_json::to_string(&result["content"])
            .unwrap()
            .contains("\"gpu\"")
    );
}

#[test]
fn two_tools_that_clean_to_one_name_are_both_listed() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    server(&app, "c1", "s", &["a.b", "a_b", "plain"]);
    let one = endpoint("one", vec![grant("c1", "chrome-dev", &[])]);
    let offered = names(&app, &one);
    assert!(offered.contains(&"s__a_b".to_owned()), "{offered:?}");
    assert!(offered.contains(&"s__plain".to_owned()), "{offered:?}");
    let hashed = exposed_name_hashed(&["s", "a_b"]);
    assert!(hashed.starts_with("s__a_b_") && hashed.len() == "s__a_b_".len() + 6);
    assert!(offered.contains(&hashed), "{offered:?}");
    assert_eq!(offered.len(), 3);
    // The hash tail comes from the raw parts, so it does not change with the clean name.
    assert_ne!(exposed_name_hashed(&["s", "a.b"]), hashed);
}

#[tokio::test]
async fn an_endpoints_existence_is_told_only_to_a_caller_with_a_token() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    app.db
        .put(
            ENDPOINTS,
            "chrome-dev",
            &endpoint(
                "chrome-dev",
                vec![grant("c1", "chrome-dev", &[Scope::View])],
            ),
        )
        .unwrap();
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
    // Without a token, a name that exists and one that does not answer the same.
    for name in ["/mcp/chrome-dev", "/mcp/missing"] {
        let (status, _, _) = call(app.clone(), name, None, None, initialize.clone()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{name}");
        let (status, _, _) = call(
            app.clone(),
            name,
            Some("not-a-token"),
            None,
            initialize.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{name}");
    }
    let (status, _, _) = call(app.clone(), "/mcp/missing", Some(ADMIN), None, initialize).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn a_tool_list_an_endpoint_cannot_carry_is_refused() {
    let names: Vec<String> = (0..MAX_TOOLS).map(|i| format!("tool_{i}")).collect();
    let tools: Vec<rmcp::model::Tool> = names.iter().map(|name| tool(name)).collect();
    assert!(check_tool_list(&tools).is_ok());
    let mut one_more = tools.clone();
    one_more.push(tool("tool_more"));
    let error = check_tool_list(&one_more).unwrap_err().to_string();
    assert!(error.contains("more than 256 tools"), "{error}");
    // Few tools, but described at a length no endpoint should send with every list.
    let mut wide = tool("wide");
    wide.description = Some("x".repeat(1 << 20).into());
    let error = check_tool_list(&[wide]).unwrap_err().to_string();
    assert!(error.contains("larger than 1 MiB"), "{error}");
}

#[tokio::test]
async fn request_human_comes_with_input_and_asks_only_for_the_endpoints_container() {
    let app = mcp_app();
    for (id, name) in [("c1", "Chrome dev"), ("c2", "Unity")] {
        app.db.put(CONTAINERS, id, &container(id, name)).unwrap();
    }
    let viewer = endpoint("viewer", vec![grant("c1", "chrome-dev", &[Scope::View])]);
    assert!(!names(&app, &viewer).contains(&"request_human".to_owned()));
    let driver = endpoint(
        "chrome-dev",
        vec![grant("c1", "chrome-dev", &[Scope::Input])],
    );
    let entry = catalog(&app, &driver)
        .into_iter()
        .find(|entry| entry.tool.name == "request_human")
        .expect("input offers request_human");
    assert!(
        !entry.tool.input_schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("container_id")
    );
    let annotations = entry.tool.annotations.unwrap();
    assert_eq!(annotations.read_only_hint, Some(false));
    assert_eq!(annotations.destructive_hint, Some(false));
    app.db.put(ENDPOINTS, "chrome-dev", &driver).unwrap();
    let session = open_session(&app, "/mcp/chrome-dev", ADMIN).await;
    let ask = |arguments: Value| {
        call(
            app.clone(),
            "/mcp/chrome-dev",
            Some(ADMIN),
            Some(&session),
            json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"request_human","arguments":arguments}}),
        )
    };
    let (_, _, body) = ask(json!({"message": "Solve the CAPTCHA", "wait_seconds": 0})).await;
    let asked = &body["result"]["structuredContent"];
    assert_eq!(asked["status"], "pending", "{body}");
    let pending = app.human.pending();
    assert_eq!(pending[0].container_id, "c1");
    assert_eq!(pending[0].endpoint.as_deref(), Some("chrome-dev"));
    let (_, _, body) = ask(json!({"request_id": asked["request_id"], "wait_seconds": 0})).await;
    assert_eq!(
        body["result"]["structuredContent"]["status"], "pending",
        "{body}"
    );
    // Another container's request is not this endpoint's to wait on, nor its container_id.
    let other = app
        .human
        .ask(
            &app.container("c2").unwrap(),
            "Accept the license",
            None,
            &app.events,
        )
        .unwrap();
    let (_, _, body) = ask(json!({"request_id": other.id, "wait_seconds": 0})).await;
    assert_eq!(body["result"]["isError"], true, "{body}");
    let (_, _, body) = ask(json!({"container_id": "c2", "message": "Elsewhere"})).await;
    assert!(body["error"].is_object(), "{body}");
}

#[test]
fn a_relayed_result_carries_the_result_type_a_newer_client_requires() {
    // A server of the 2025 revisions answers without resultType; a 2026-07-28 client refuses a
    // result without it, so the endpoint says what the absence means.
    let older: rmcp::model::CallToolResult =
        serde_json::from_value(json!({"content":[{"type":"text","text":"ok"}]})).unwrap();
    assert!(older.result_type.is_none());
    let relayed_result = relayed(older);
    assert_eq!(
        serde_json::to_value(&relayed_result).unwrap()["resultType"],
        "complete"
    );
    // A result that says so already is passed on as it is.
    let newer: rmcp::model::CallToolResult =
        serde_json::from_value(json!({"resultType":"complete","content":[]})).unwrap();
    assert_eq!(relayed(newer.clone()), newer);
}

#[tokio::test]
async fn an_endpoints_calls_are_recorded_with_the_endpoint_and_a_servers_arguments_by_size() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    server(&app, "c1", "chrome", &["browser_navigate"]);
    let Json(created) = create_endpoint(
        State(app.clone()),
        Json(
            serde_json::from_value(json!({"name":"chrome-dev","grants":[{"container_id":"c1","scopes":["view"],"servers":"all"}]}))
                .unwrap(),
        ),
    )
    .await
    .unwrap();
    let token = created["token"].as_str().unwrap().to_owned();
    let session = open_session(&app, "/mcp/chrome-dev", &token).await;
    let tool_call = |id: u64, name: &str, arguments: Value| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    for message in [
        tool_call(2, "get_usage", json!({})),
        tool_call(
            3,
            "chrome__browser_navigate",
            json!({"url": "https://example.com/?key=secret-value-123"}),
        ),
        tool_call(4, "delete_container", json!({})),
    ] {
        call(
            app.clone(),
            "/mcp/chrome-dev",
            Some(&token),
            Some(&session),
            message,
        )
        .await;
    }
    // The administrator's token opens the endpoint too, and says so.
    let admin = open_session(&app, "/mcp/chrome-dev", ADMIN).await;
    call(
        app.clone(),
        "/mcp/chrome-dev",
        Some(ADMIN),
        Some(&admin),
        tool_call(5, "list_windows", json!({})),
    )
    .await;
    let rows = crate::activity::recorded(&app, 4).await;
    let row = |tool: &str| {
        rows.iter()
            .find(|row| row.tool == tool)
            .unwrap_or_else(|| panic!("{tool}: {rows:?}"))
    };

    let usage = row("get_usage");
    assert_eq!(usage.endpoint.as_deref(), Some("chrome-dev"));
    assert_eq!(usage.credential, "endpoint_token");
    // The endpoint binds its one container, which the call does not name.
    assert_eq!(usage.container_id.as_deref(), Some("c1"));
    assert_eq!(usage.container_name.as_deref(), Some("Chrome dev"));
    assert_eq!(
        (usage.source.as_str(), usage.kind.as_str()),
        ("tool", "read")
    );

    // A server's tool by its own name, its arguments as their size whatever it says of itself.
    let navigate = row("browser_navigate");
    assert_eq!(navigate.source, "server");
    assert_eq!(navigate.server.as_deref(), Some("chrome"));
    assert_eq!(navigate.container_id.as_deref(), Some("c1"));
    assert_eq!(
        navigate.kind, "change",
        "a server's read-only claim is not trusted"
    );
    assert_eq!(
        navigate.arguments,
        json!({"url": {"$masked": "string", "length": 41}})
    );
    assert!(
        !serde_json::to_string(navigate)
            .unwrap()
            .contains("secret-value-123")
    );

    // A tool outside the endpoint's scopes is refused, and shown as what it would have been.
    let refused = row("delete_container");
    assert_eq!(
        (refused.outcome.as_str(), refused.kind.as_str()),
        ("refused", "destructive")
    );

    assert_eq!(row("list_windows").credential, "mcp_token");
}
