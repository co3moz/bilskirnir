//! Pins down which wait conditions are taken and which are refused before any polling.
use super::*;

#[test]
fn wait_conditions_are_validated() {
    let check =
        |value: serde_json::Value| serde_json::from_value::<WaitFor>(value).unwrap().validate();
    assert!(check(serde_json::json!({"window_title":"Game"})).is_ok());
    assert!(
        check(serde_json::json!({"application_id":"a","log_contains":"ready","timeout_ms":60000}))
            .is_ok()
    );
    assert!(check(serde_json::json!({"screen_stable_ms":1000,"region":"0,0,100,100"})).is_ok());
    // The screen can be waited on to change, and text to appear or go, in a region or not.
    assert!(check(serde_json::json!({"screen_changed":true})).is_ok());
    assert!(
        check(serde_json::json!({"screen_changed":true,"screen_change_percent":0.01,"region":"0,0,100,100"}))
            .is_ok()
    );
    assert!(
        check(serde_json::json!({"text":"Play","text_state":"gone","region":"0,0,1,1"})).is_ok()
    );
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"application_state":"running"}),
        serde_json::json!({"application_id":"a","application_state":"paused"}),
        serde_json::json!({"log_contains":"x"}),
        serde_json::json!({"window_title":"x","timeout_ms":60001}),
        serde_json::json!({"screen_stable_ms":20000,"timeout_ms":10000}),
        serde_json::json!({"window_title":"x","region":"0,0,1,1"}),
        serde_json::json!({"screen_changed":false}),
        serde_json::json!({"window_title":"x","screen_change_percent":5.0}),
        serde_json::json!({"screen_changed":true,"screen_change_percent":0.0}),
        serde_json::json!({"screen_changed":true,"screen_change_percent":100.5}),
        serde_json::json!({"text":"   "}),
        serde_json::json!({"text":"x".repeat(257)}),
        serde_json::json!({"window_title":"x","text_state":"gone"}),
        serde_json::json!({"text":"Play","text_state":"hidden"}),
    ] {
        assert!(check(bad).is_err());
    }
}
