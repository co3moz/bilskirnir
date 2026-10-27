//! Pins down that pruning keeps the newest jobs and every unfinished one, whatever its age, and
//! that the unfinished ones are found without reading the logs of the others.
use super::*;
use crate::model::Job;

fn job(id: &str, finished: bool) -> Job {
    Job {
        id: id.into(),
        kind: "exec".into(),
        resource: None,
        status: if finished { "completed" } else { "running" }.into(),
        created_at: 1,
        finished_at: finished.then_some(2),
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
        name: None,
    }
}

#[test]
fn pruning_keeps_the_newest_jobs_and_the_unfinished_ones() {
    let dir = std::env::temp_dir().join(format!("thoric-db-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join("state.sqlite")).unwrap();
    for (id, finished) in [("old", true), ("running", false), ("new", true)] {
        db.put("job", id, &job(id, finished)).unwrap();
    }
    assert_eq!(db.prune_jobs(2).unwrap(), 1);
    let ids = |db: &Db| {
        let mut ids: Vec<String> = db
            .list::<Job>("job")
            .unwrap()
            .into_iter()
            .map(|j| j.id)
            .collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(&db), ["new", "running"]);
    // The running job is older than the one kept, and stays.
    assert_eq!(db.prune_jobs(1).unwrap(), 0);
    assert_eq!(ids(&db), ["new", "running"]);
    drop(db);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn active_jobs_are_the_queued_and_running_ones_without_their_logs() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    for (id, status) in [
        ("queued", "queued"),
        ("done", "completed"),
        ("running", "running"),
        ("failed", "failed"),
    ] {
        let mut record = job(id, false);
        record.status = status.into();
        record.log = "a long log".into();
        db.put("job", id, &record).unwrap();
    }
    let active = db.active_jobs().unwrap();
    let ids: Vec<&str> = active.iter().map(|job| job.id.as_str()).collect();
    assert_eq!(ids, ["running", "queued"]);
    assert!(active.iter().all(|job| job.log.is_empty()));
}
