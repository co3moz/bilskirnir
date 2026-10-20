//! Pins down that a request holding a resource waits a bounded time for a job that has it and
//! then answers that the resource is busy, so no client hangs for the job's length.
use super::*;
use axum::http::StatusCode;
use std::time::Duration;

#[tokio::test]
async fn a_request_waits_a_bounded_time_for_a_held_resource() {
    let app = crate::tests::app(false);
    let wait = Duration::from_millis(100);
    // Free: held at once, and the guard releases it for the next request.
    assert!(wait < HOLD_WAIT, "the test waits less than a request does");
    drop(app.hold_for("container:free", wait).await.unwrap());
    let job = app.resource_lock("container:busy").lock_owned().await;
    let started = std::time::Instant::now();
    let error = app.hold_for("container:busy", wait).await.unwrap_err();
    assert_eq!(error.0, StatusCode::CONFLICT);
    assert!(error.1.contains("the container"), "{}", error.1);
    assert!(started.elapsed() >= wait && started.elapsed() < wait * 10);
    assert!(
        app.hold_for("volume:v", wait).await.is_ok(),
        "another resource is not held"
    );
    drop(job);
    assert!(app.hold_for("container:busy", wait).await.is_ok());
}
