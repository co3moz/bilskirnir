//! Pins down which application launches and window geometries are refused, and that metric
//! sampling refuses a duration out of its range before any Docker work.
use super::*;
use crate::test_support::app;
use crate::windows;

#[test]
fn application_launch_requests_are_validated() {
    let launch =
        |value: serde_json::Value| serde_json::from_value::<Launch>(value).unwrap().validate();
    assert!(
        launch(serde_json::json!({"command":"glxgears","args":["-info"],"cwd":"/home/desktop/game","env":{"MY_VAR":"1"}}))
            .is_ok()
    );
    for bad in [
        serde_json::json!({"command":""}),
        serde_json::json!({"command":"app","cwd":"relative"}),
        serde_json::json!({"command":"app","cwd":"/home/desktop/../root"}),
        serde_json::json!({"command":"app","env":{"1BAD":"x"}}),
        serde_json::json!({"command":"app","env":{"DISPLAY":":1"}}),
        serde_json::json!({"command":"app","args":(0..101).map(|_| "x").collect::<Vec<_>>()}),
    ] {
        assert!(launch(bad).is_err());
    }
    assert!(
        serde_json::from_value::<Launch>(serde_json::json!({"command":"app","unexpected":true}))
            .is_err()
    );
    assert!(
        windows::Geometry {
            x: Some(1),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    assert!(
        windows::Geometry {
            width: Some(640),
            height: Some(480),
            ..Default::default()
        }
        .validate()
        .is_ok()
    );
}

#[tokio::test]
async fn metric_sampling_rejects_bad_durations_before_docker() {
    let app = app(false);
    assert!(sample_metrics(&app, "missing", "app", 50).await.is_err());
    assert!(
        sample_metrics(&app, "missing", "app", 10_001)
            .await
            .is_err()
    );
}
