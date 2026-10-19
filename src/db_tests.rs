//! Pins down that pruning keeps the newest jobs and every unfinished one, whatever its age.
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
    }
}

#[test]
fn pruning_keeps_the_newest_jobs_and_the_unfinished_ones() {
    let dir = std::env::temp_dir().join(format!("bilskirnir-db-{}", uuid::Uuid::new_v4().simple()));
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
