//! Pins down that an input request waits for a container held by a job without taking one of
//! the input slots every container shares, and gives up with a busy answer in bounded time.
use super::*;

#[tokio::test]
async fn a_held_container_answers_busy_without_taking_an_input_slot() {
    let app = crate::tests::app(false);
    let lock = app.resource_lock("container:held");
    let _job = lock.lock().await;
    let waiting = tokio::spawn({
        let app = app.clone();
        async move { perform(&app, "held", &[]).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Every slot stays free for the other containers while this request waits.
    assert_eq!(app.desktop_inputs.available_permits(), 4);
    let error = tokio::time::timeout(LOCK_WAIT + Duration::from_secs(2), waiting)
        .await
        .expect("the wait is bounded")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.0, StatusCode::CONFLICT);
    assert_eq!(app.desktop_inputs.available_permits(), 4);
}
