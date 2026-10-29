//! Pins down that pruning keeps the newest jobs and every unfinished one, whatever its age, with
//! the requests kept to retry them, and that the unfinished ones are found without reading the
//! logs of the others. A record that does not read leaves the others of its kind readable, while
//! the complete lists refuse without its data, and a copy of the database is whole.
use super::*;
use crate::model::Job;
use serde::Deserialize;

/// A record kind of the tests' own; a record without `name` does not read as one.
#[derive(Debug, Deserialize, PartialEq)]
struct Named {
    name: String,
}

/// The value that keeps the record from reading, which serde's own message would quote.
const SECRET: u64 = 424_242;

fn with_unreadable() -> Db {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.put("named", "a", &serde_json::json!({"name": "first"}))
        .unwrap();
    db.put("named", "broken", &serde_json::json!({"name": SECRET}))
        .unwrap();
    db.put("named", "b", &serde_json::json!({"name": "second"}))
        .unwrap();
    db
}

#[test]
fn a_record_that_does_not_read_is_left_out_of_the_lists() {
    let db = with_unreadable();
    let names = |list: Vec<Named>| list.into_iter().map(|n| n.name).collect::<Vec<_>>();
    assert_eq!(names(db.list("named").unwrap()), ["second", "first"]);
    // Said once, however often it is listed.
    db.list::<Named>("named").unwrap();
    assert_eq!(
        *db.unreadable.lock().unwrap(),
        HashSet::from([("named".to_owned(), "broken".to_owned())])
    );
    // Its id still counts as taken.
    assert_eq!(db.ids("named").unwrap(), ["a", "broken", "b"]);
}

#[test]
fn a_complete_list_names_the_record_that_does_not_read_without_its_data() {
    let db = with_unreadable();
    for error in [
        db.list_complete::<Named>("named").unwrap_err(),
        db.entries_complete::<Named>("named").unwrap_err(),
    ] {
        let error = format!("{error:#}");
        assert!(error.contains("named record broken"), "{error}");
        assert!(!error.contains(&SECRET.to_string()), "{error}");
        // What serde itself says of it does quote the value.
        let serde = serde_json::from_value::<Named>(serde_json::json!({"name": SECRET}));
        assert!(serde.unwrap_err().to_string().contains(&SECRET.to_string()));
    }
    db.remove("named", "broken").unwrap();
    assert_eq!(db.list_complete::<Named>("named").unwrap().len(), 2);
    let entries: Vec<(String, Named)> = db.entries_complete("named").unwrap();
    let ids: Vec<&str> = entries.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn a_job_that_does_not_read_leaves_the_others_listed() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.put("job", "good", &job("good", false)).unwrap();
    // A field renamed: running, but without the kind this release reads.
    let renamed = serde_json::json!({"id": "renamed", "type": "exec", "status": "running"});
    db.put("job", "renamed", &renamed).unwrap();
    let ids = |jobs: Vec<Job>| jobs.into_iter().map(|job| job.id).collect::<Vec<_>>();
    assert_eq!(ids(db.list("job").unwrap()), ["good"]);
    assert_eq!(ids(db.jobs_summary().unwrap()), ["good"]);
    assert_eq!(ids(db.active_jobs().unwrap()), ["good"]);
}

#[test]
fn a_copy_of_the_database_holds_its_records() {
    let dir = std::env::temp_dir().join(format!("thoric-db-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join(FILE)).unwrap();
    db.put("named", "a", &serde_json::json!({"name": "first"}))
        .unwrap();
    db.copy_to(&dir.join("copy.sqlite")).unwrap();
    // The copy is a database of its own, of the same version, and never overwritten.
    assert!(db.copy_to(&dir.join("copy.sqlite")).is_err());
    drop(db);
    let copy = Db::open(&dir.join("copy.sqlite")).unwrap();
    assert_eq!(copy.list::<Named>("named").unwrap()[0].name, "first");
    drop(copy);
    std::fs::remove_dir_all(&dir).unwrap();
}

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
        script: None,
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
    // A request kept to retry a job goes with it.
    let requests = crate::jobs::retry::REQUESTS;
    for id in ["new", "old"] {
        db.put(requests, id, &serde_json::json!({})).unwrap();
    }
    db.prune_jobs(2).unwrap();
    assert!(db.contains(requests, "new").unwrap());
    assert!(!db.contains(requests, "old").unwrap());
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
