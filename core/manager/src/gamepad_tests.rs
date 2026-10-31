//! Pins down which gamepad actions are refused, that a container has no gamepad unless it asks for
//! one, and that two requests to a desktop without a daemon start one between them.
use super::*;
use crate::test_support::spec;

/// Two requests share the container and both find no daemon, the second while the first is still
/// starting one: it waits for that start and finds the daemon running.
#[cfg(unix)]
#[tokio::test]
async fn two_requests_to_a_desktop_without_a_daemon_start_one() {
    let mut app = crate::test_support::app(false);
    let dir =
        std::env::temp_dir().join(format!("thoric-gamepad-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    // The daemon reports running once started; a status call takes long enough that two
    // unserialized requests would both see none.
    crate::test_support::write_script(
        &docker,
        &format!(
            "#!/bin/sh\ncase \"$2\" in\n  --detach) echo started >> '{dir}/starts'; touch '{dir}/running' ;;\n  --interactive) sleep 0.3; if [ -e '{dir}/running' ]; then echo '{{\"state\":\"running\"}}'; else echo '{{\"state\":\"absent\"}}'; fi ;;\n  *) exit 1 ;;\nesac\n",
            dir = dir.display()
        ),
    );
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    let (first, second) = tokio::join!(
        start_daemon(&app, "pad", "thoric-pad"),
        start_daemon(&app, "pad", "thoric-pad")
    );
    first.unwrap();
    second.unwrap();
    let starts = std::fs::read_to_string(dir.join("starts")).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(starts.lines().count(), 1);
}

#[test]
fn gamepad_requests_are_validated_and_default_off() {
    assert!(!spec().gamepad);
    let check = |value: serde_json::Value| {
        serde_json::from_value::<GamepadInput>(value)
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
