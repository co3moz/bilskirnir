//! Pins down which gamepad actions are refused, and that a container has no gamepad unless it
//! asks for one.
use super::*;
use crate::test_support::spec;

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
