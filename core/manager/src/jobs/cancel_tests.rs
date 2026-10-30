//! Pins down what a cancel reaches: a queued job of any kind never runs, a running script, build or
//! download is dropped and then stopped where it runs, a running change of a container finishes,
//! and a job that ended or is unknown is refused. A cancelled job ends as failed with
//! [`CANCELLED`].
use super::*;
use crate::{jobs, test_support::app};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

async fn status(app: &App, id: &str, status: &str) -> Job {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let job = app.db.job(id).unwrap();
            if job.status == status {
                return job;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the job never became {status}"))
}

fn launched(response: Result<Json<Value>>) -> String {
    response.unwrap().0["job_id"].as_str().unwrap().to_owned()
}

async fn cancel_job(app: &Arc<App>, id: &str) -> Result<Json<Value>> {
    cancel(State(app.clone()), Path(id.to_owned())).await
}

#[tokio::test]
async fn a_queued_job_of_any_kind_never_runs() {
    let app = app(false);
    let held = app.resource_lock("container:one").lock_owned().await;
    let ran = Arc::new(AtomicBool::new(false));
    let id = launched(jobs::launch(
        app.clone(),
        "reset",
        "container:one".into(),
        {
            let ran = ran.clone();
            |_, _| async move {
                ran.store(true, Ordering::SeqCst);
                Ok(json!({}))
            }
        },
    ));
    status(&app, &id, "queued").await;
    assert!(app.cancels.cancellable(&id));
    assert_eq!(cancel_job(&app, &id).await.unwrap().0["cancelled"], true);
    assert!(!app.cancels.cancellable(&id));
    let job = status(&app, &id, "failed").await;
    assert!(job.log.contains(CANCELLED), "{}", job.log);
    drop(held);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!ran.load(Ordering::SeqCst));
    // Its resource is free for the next job.
    drop(app.hold("container:one").await.unwrap());
    assert_eq!(
        cancel_job(&app, &id).await.unwrap_err().0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn a_running_script_is_dropped_and_stopped_where_it_runs() {
    let app = app(false);
    let dropped = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let id = launched(jobs::launch_shared(
        app.clone(),
        "exec",
        "container:one".into(),
        {
            let (dropped, stopped) = (dropped.clone(), stopped.clone());
            |app, id| async move {
                let _work = Dropped(dropped);
                app.cancels.on_stop(&id, async move {
                    stopped.store(true, Ordering::SeqCst);
                });
                std::future::pending::<()>().await;
                Ok(json!({}))
            }
        },
    ));
    status(&app, &id, "running").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(cancel_job(&app, &id).await.unwrap().0["cancelled"], true);
    let job = status(&app, &id, "failed").await;
    assert!(dropped.load(Ordering::SeqCst) && stopped.load(Ordering::SeqCst));
    assert!(
        job.log.contains(&format!("ERROR: {CANCELLED}")),
        "{}",
        job.log
    );
    assert!(job.finished_at.is_some());
}

#[tokio::test]
async fn a_running_change_of_a_container_finishes_and_an_unknown_job_is_not_found() {
    let app = app(false);
    let (end, ended) = tokio::sync::oneshot::channel::<()>();
    let id = launched(jobs::launch(
        app.clone(),
        "reset",
        "container:one".into(),
        |_, _| async {
            let _ = ended.await;
            Ok(json!({"reset": true}))
        },
    ));
    status(&app, &id, "running").await;
    assert!(!app.cancels.cancellable(&id));
    let refused = cancel_job(&app, &id).await.unwrap_err();
    assert_eq!(refused.0, StatusCode::CONFLICT);
    assert!(refused.1.contains("reset"), "{}", refused.1);
    end.send(()).unwrap();
    status(&app, &id, "completed").await;
    assert_eq!(
        cancel_job(&app, "missing").await.unwrap_err().0,
        StatusCode::NOT_FOUND
    );
}

#[test]
fn a_script_is_stopped_by_its_own_marker_as_its_user() {
    let args = crate::exec::stop_args("c0ffee", "1001:1001", "thoric-exec-abc");
    assert_eq!(
        args,
        [
            "exec",
            "--user",
            "1001:1001",
            "c0ffee",
            "pkill",
            "--signal",
            "TERM",
            "--full",
            "^timeout .* thoric-exec-abc$",
        ]
    );
}
