//! Pins down when the manager warns that it answers without a credential beyond this computer:
//! sign-in off or a token-less `/mcp`, while Docker publishes its port beyond loopback.
use super::*;
use crate::test_support::app;

fn bindings(addresses: &[&str]) -> Value {
    let bindings = addresses
        .iter()
        .map(|address| json!({"HostIp": address, "HostPort": "8080"}))
        .collect::<Vec<_>>();
    json!({"NetworkSettings": {"Ports": {"8080/tcp": bindings, "9000/tcp": null}}})
}

#[test]
fn only_a_port_published_beyond_loopback_counts() {
    assert!(published(&bindings(&["127.0.0.1"])).is_empty());
    assert!(published(&bindings(&["::1", "127.0.0.2"])).is_empty());
    assert!(published(&json!({})).is_empty());
    assert_eq!(published(&bindings(&["0.0.0.0", "::"])), ["*:8080"]);
    assert_eq!(published(&bindings(&[""])), ["*:8080"]);
    assert_eq!(
        published(&bindings(&["127.0.0.1", "192.168.1.20"])),
        ["192.168.1.20:8080"]
    );
    assert_eq!(published(&bindings(&["fd00::20"])), ["[fd00::20]:8080"]);
}

#[test]
fn what_answers_without_a_credential() {
    let mut app = app(true);
    let config = &mut Arc::get_mut(&mut app).unwrap().config;
    config.mcp_token = Some("token".into());
    config.root_mcp_enabled = true;
    assert!(open(config).is_empty());
    config.auth = false;
    assert_eq!(open(config), ["dashboard"]);
    config.mcp_token = None;
    assert_eq!(open(config), ["dashboard", "mcp"]);
    // With the root /mcp off, no token opens nothing.
    config.root_mcp_enabled = false;
    config.auth = true;
    assert!(open(config).is_empty());
}

#[test]
fn the_warning_names_what_is_open_and_where() {
    let text = warning(&json!({"open": ["dashboard"], "published": ["*:8080", "*:8443"]}));
    assert!(
        text.starts_with("Sign-in is off (AUTH_MODE=none) while"),
        "{text}"
    );
    assert!(text.contains("*:8080, *:8443"), "{text}");
    assert!(
        text.contains("every desktop with internet access"),
        "{text}"
    );
    let text = warning(&json!({"open": ["mcp"], "published": ["*:8080"]}));
    assert!(text.starts_with("/mcp takes no token"), "{text}");
}
