//! Pins down who reaches a terminal: the administrator every one, and an MCP endpoint only the
//! terminals it opened as the desktop user, never a root shell or another caller's.
use super::*;
use crate::test_support::app;

fn terminal(id: &str, root: bool, endpoint: Option<&str>) -> Terminal {
    Terminal {
        id: id.into(),
        container_id: "c1".into(),
        root,
        cwd: "/home/desktop".into(),
        created_at: 1,
        endpoint: endpoint.map(str::to_owned),
    }
}

#[test]
fn an_endpoint_reaches_only_the_terminals_it_opened() {
    let app = app(false);
    for record in [
        terminal("administrators-root", true, None),
        terminal("administrators", false, None),
        terminal("mine", false, Some("dev")),
        terminal("theirs", false, Some("other")),
    ] {
        app.db.put("terminal", &record.id, &record).unwrap();
    }
    for id in ["administrators-root", "administrators", "mine", "theirs"] {
        assert!(record(&app, "c1", id, None).is_ok(), "{id}");
    }
    assert!(record(&app, "c1", "mine", Some("dev")).is_ok());
    for id in ["administrators-root", "administrators", "theirs"] {
        let error = record(&app, "c1", id, Some("dev")).unwrap_err();
        assert_eq!(error.0, StatusCode::NOT_FOUND, "{id}");
    }
    // A terminal is reached only through its own container.
    assert!(record(&app, "c2", "administrators", None).is_err());
    assert!(record(&app, "c2", "mine", Some("dev")).is_err());
}

#[test]
fn a_root_terminal_is_never_an_endpoints() {
    // Not even one a record names an endpoint for, as no release writes.
    assert!(!reaches(&terminal("t", true, Some("dev")), Some("dev")));
    assert!(reaches(&terminal("t", true, Some("dev")), None));
}

#[tokio::test]
async fn an_endpoint_opens_no_root_shell() {
    let app = app(false);
    let body = OpenTerminal {
        cwd: None,
        root: true,
    };
    let error = open_terminal(&app, "c1", &body, Some("dev"))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::BAD_REQUEST, "{}", error.1);
    assert!(app.db.list::<Terminal>("terminal").unwrap().is_empty());
}

#[test]
fn a_terminal_without_files_is_gone_once_its_runner_had_time_to_make_them() {
    let mut record = terminal("t", false, None);
    record.created_at = now();
    // A runner that has not made the folder yet.
    assert!(!gone(&record, &stopped()));
    record.created_at = now() - STARTING;
    assert!(gone(&record, &stopped()));
    assert!(gone(
        &record,
        &json!({"state": "unknown", "gone": true, "error": "Terminal files are not available"})
    ));
    // A terminal whose shell exited keeps its output until it is closed, and one the helper could
    // not be asked about stays too.
    assert!(!gone(&record, &json!({"state": "exited", "exit_code": 0})));
    assert!(!gone(&record, &json!({"state": "unknown"})));
}
