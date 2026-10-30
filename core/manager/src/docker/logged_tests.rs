//! Pins down what a logged command reports to a caller that reports it itself: its exit status,
//! or none once it ran out of its time and was killed, and the end of its own output in the order
//! it arrived, cut at a character's start, beside the job log that keeps all of it.
use super::*;
use crate::test_support::app;
use std::sync::Arc;

#[test]
fn the_tail_keeps_the_last_bytes_from_a_characters_start() {
    let mut tail = String::new();
    keep_tail(&mut tail, "é");
    keep_tail(&mut tail, &"a".repeat(TAIL_LIMIT - 1));
    // The limit falls inside the two bytes of é, so the tail starts after it.
    assert_eq!(tail, "a".repeat(TAIL_LIMIT - 1));
    keep_tail(&mut tail, "é!");
    assert_eq!(tail.len(), TAIL_LIMIT);
    assert!(tail.ends_with("é!"));
    keep_tail(&mut tail, &"b".repeat(TAIL_LIMIT));
    assert_eq!(tail, "b".repeat(TAIL_LIMIT));
}

/// A job whose command is the stand-in Docker CLI `script`.
#[cfg(unix)]
fn job(script: &str) -> (Arc<App>, String, std::path::PathBuf) {
    let mut app = app(false);
    let dir = std::env::temp_dir().join(format!("thoric-logged-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    crate::test_support::write_script(&docker, script);
    Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    let job = crate::model::Job {
        id: "j".into(),
        kind: "exec".into(),
        resource: None,
        name: None,
        status: "running".into(),
        created_at: 1,
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
        script: None,
    };
    app.db.put("job", "j", &job).unwrap();
    (app, "j".into(), dir)
}

#[cfg(unix)]
#[tokio::test]
async fn a_failed_command_reports_its_status_and_the_end_of_its_output() {
    let (app, id, dir) = job("#!/bin/sh\ncat\necho second >&2\nexit 3\n");
    let ran = app
        .docker_logged_outcome(&id, &["exec".into()], b"first\n".to_vec())
        .await
        .unwrap();
    assert_eq!(ran.status.and_then(|status| status.code()), Some(3));
    assert!(
        ran.tail.contains("first") && ran.tail.contains("second"),
        "{}",
        ran.tail
    );
    let log = app.db.job(&id).unwrap().log;
    assert!(log.contains("first") && log.contains("second"), "{log}");
    // A caller that fails on it says so in the log too.
    let error = app.docker_logged(&id, &["exec".into()]).await.unwrap_err();
    assert!(
        error.to_string().contains("Docker operation failed"),
        "{error:#}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_command_out_of_time_reports_no_status_and_is_killed() {
    let (app, id, dir) = job("#!/bin/sh\necho waiting\nexec sleep 30\n");
    let started = std::time::Instant::now();
    let ran = app
        .run_logged(
            &id,
            &["exec".into()],
            None,
            None,
            Duration::from_millis(500),
        )
        .await
        .unwrap();
    assert!(ran.status.is_none());
    assert!(ran.tail.contains("waiting"), "{}", ran.tail);
    assert!(started.elapsed() < Duration::from_secs(10));
    let error = ran.check(&app, &id, Duration::from_secs(120)).unwrap_err();
    assert!(error.to_string().contains("2-minute limit"), "{error:#}");
    std::fs::remove_dir_all(dir).unwrap();
}
