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
