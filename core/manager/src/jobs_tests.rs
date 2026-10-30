//! Pins down that a request holding a resource waits a bounded time for a job that has it and then
//! answers that the resource is busy, so no client hangs for the job's length. Also that a job
//! waiting for a busy container holds up no job of another, and that work that only uses a
//! container shares it while work that changes or replaces it waits for the sharers and goes first.
//! A job that fails with a result of its own, such as a script's exit code, keeps it.
use super::*;
use crate::test_support::app;
use axum::http::StatusCode;
use std::time::Duration;

#[tokio::test]
async fn a_request_waits_a_bounded_time_for_a_held_resource() {
    let app = crate::test_support::app(false);
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

#[tokio::test]
async fn unrelated_jobs_progress_while_a_container_queue_is_blocked() {
    let app = app(false);
    let lock = app.resource_lock("container:one");
    let guard = lock.lock().await;
    let queued = launch(app.clone(), "stop", "container:one".into(), |_, _| async {
        Ok(serde_json::json!({}))
    })
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let free = launch(app.clone(), "start", "container:two".into(), |_, _| async {
        Ok(serde_json::json!({}))
    })
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.db.job(&free).unwrap().status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(app.db.job(&queued).unwrap().status, "queued");
    drop(guard);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if app.db.job(&queued).unwrap().status == "completed" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn work_that_uses_a_container_shares_it_and_work_that_replaces_it_waits() {
    let app = app(false);
    let wait = Duration::from_millis(100);
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let script = launch_shared(app.clone(), "exec", "container:one".into(), |_, _| async {
        let _ = released.await;
        Ok(serde_json::json!({}))
    })
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    tokio::time::timeout(Duration::from_secs(1), async {
        while app.db.job(&script).unwrap().status != "running" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // Input, files and applications go on while the script runs.
    drop(app.share_for("container:one", wait).await.unwrap());
    // A request that would change the container answers busy, and a job that would waits.
    assert_eq!(
        app.hold_for("container:one", wait).await.unwrap_err().0,
        StatusCode::CONFLICT
    );
    let reset = launch(app.clone(), "reset", "container:one".into(), |_, _| async {
        Ok(serde_json::json!({}))
    })
    .unwrap()
    .0["job_id"]
        .as_str()
        .unwrap()
        .to_owned();
    tokio::time::sleep(wait).await;
    assert_eq!(app.db.job(&reset).unwrap().status, "queued");
    // A sharer that comes after the waiting reset waits behind it.
    assert!(app.share_for("container:one", wait).await.is_err());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while app.db.job(&reset).unwrap().status != "completed" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(app.db.job(&script).unwrap().status, "completed");
    drop(app.hold_for("container:one", wait).await.unwrap());
}

#[tokio::test]
async fn a_job_that_fails_with_a_result_keeps_it_beside_its_error() {
    let app = app(false);
    let (id, outcome) = start(
        app.clone(),
        "exec",
        "container:one".into(),
        true,
        |_, _| async {
            Err(Failed::new(
                "Script exited with code 2",
                serde_json::json!({"exit_code": 2}),
            )
            .into())
        },
    )
    .unwrap();
    assert_eq!(
        outcome.await.unwrap().unwrap_err(),
        "Script exited with code 2"
    );
    let job = app.db.job(&id).unwrap();
    assert_eq!(job.status, "failed");
    assert_eq!(job.result, Some(serde_json::json!({"exit_code": 2})));
    assert!(
        job.log.contains("ERROR: Script exited with code 2"),
        "{}",
        job.log
    );
    // Any other failure leaves no result.
    let (id, outcome) = start(
        app.clone(),
        "exec",
        "container:one".into(),
        true,
        |_, _| async { anyhow::bail!("Start the container before executing a script") },
    )
    .unwrap();
    assert!(outcome.await.unwrap().is_err());
    assert_eq!(app.db.job(&id).unwrap().result, None);
}
