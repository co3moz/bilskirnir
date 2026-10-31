//! A folder's download: which paths download at all, and the archive's file name.
use super::*;

#[test]
fn files_download_as_they_are_and_folders_as_archives() {
    let file = json!({"name": "notes.txt", "type": "file", "size": 42});
    assert_eq!(target(&file).unwrap(), Target::File(42));
    let folder = json!({"name": "Documents", "type": "directory", "size": 4096});
    assert_eq!(target(&folder).unwrap(), Target::Folder);
}

#[test]
fn links_and_special_files_are_refused() {
    for kind in ["symlink", "special"] {
        let refused = target(&json!({"name": "x", "type": kind, "size": 0})).unwrap_err();
        assert_eq!(refused.0, StatusCode::BAD_REQUEST, "{kind}");
    }
}

#[test]
fn the_archive_is_named_after_its_folder() {
    assert_eq!(
        disposition("Documents"),
        "attachment; filename=\"Documents.tar.gz\"; filename*=UTF-8''Documents.tar.gz"
    );
    // A name a quoted string cannot carry keeps only safe characters there, and all of itself
    // in `filename*`.
    assert_eq!(
        disposition("Masaüstü \"x\";y"),
        "attachment; filename=\"Masa_st_ _x__y.tar.gz\"; \
         filename*=UTF-8''Masa%C3%BCst%C3%BC%20%22x%22%3By.tar.gz"
    );
}

#[cfg(unix)]
#[tokio::test(start_paused = true)]
async fn a_download_nobody_reads_stops_and_frees_its_slot() {
    let mut app = crate::test_support::app(false);
    // tar's stand-in writes an archive that never ends.
    Arc::get_mut(&mut app).unwrap().config.docker_bin = "sh".into();
    let mut events = app.events.subscribe();
    let source = Source {
        args: ["-c", "exec cat /dev/zero"].map(str::to_owned).to_vec(),
        helper: None,
        lock: None,
        label: "/home/desktop/project in Unity".into(),
        path: "/home/desktop/project".into(),
    };
    let started = tokio::time::Instant::now();
    // Held and never read, as by a browser that paused the download; a transfer link has no
    // session that could end it.
    let response = respond(
        app.clone(),
        "container:c1".into(),
        "project",
        source,
        auth::Session::Link,
    )
    .unwrap();
    let job_id = response.headers()["x-job-id"].to_str().unwrap().to_owned();
    loop {
        let event: Value = serde_json::from_str(&events.recv().await.unwrap()).unwrap();
        if event["type"] == "job"
            && event["job"]["id"] == job_id
            && event["job"]["status"] != "running"
            && event["job"]["status"] != "queued"
        {
            break;
        }
    }
    assert!(started.elapsed() >= stall::STALL);
    assert_eq!(app.archives.available_permits(), 2);
    let job = app.db.job(&job_id).unwrap();
    assert_eq!(job.status, "failed");
    assert!(
        job.log.contains("the client took nothing for 5 minutes"),
        "{}",
        job.log
    );
    // What was handed over before still arrives, then the error, never a clean end.
    assert!(
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .is_err()
    );
}
