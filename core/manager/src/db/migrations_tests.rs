//! Pins down the schema versions: a new database and one from before versions both end at the
//! latest version with what they held, a database a newer release migrated is refused and left
//! as it was, and a migration that fails changes nothing.
use super::*;
use crate::db::Db;
use std::path::{Path, PathBuf};

fn folder() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("thoric-schema-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// A database as releases up to 0.8.4 left it: their tables, without a version.
fn before_versions(path: &Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL, id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(kind,id));
        CREATE TABLE IF NOT EXISTS activity(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            at INTEGER NOT NULL,
            endpoint TEXT,
            credential TEXT NOT NULL,
            client TEXT,
            source TEXT NOT NULL,
            tool TEXT NOT NULL,
            server TEXT,
            container_id TEXT,
            container_name TEXT,
            kind TEXT NOT NULL,
            arguments TEXT NOT NULL,
            summary TEXT NOT NULL,
            outcome TEXT NOT NULL,
            error TEXT,
            duration_ms INTEGER NOT NULL,
            job_id TEXT
        );
        CREATE INDEX IF NOT EXISTS activity_container ON activity(container_id, id);
        CREATE INDEX IF NOT EXISTS activity_endpoint ON activity(endpoint, id);
        INSERT INTO records(kind, id, data) VALUES('volume-note', 'v', '{\"name\":\"kept\"}');
        INSERT INTO activity(at, credential, source, tool, kind, arguments, summary, outcome, duration_ms)
            VALUES(1, 'mcp_token', 'tool', 'list_containers', 'read', '{}', '', 'ok', 3);",
    )
    .unwrap();
    assert_eq!(version(&conn), 0);
}

#[test]
fn a_new_database_starts_at_the_latest_version() {
    let db = Db::open(Path::new(":memory:")).unwrap();
    let conn = db.connection();
    assert_eq!(version(&conn), MIGRATIONS.len() as i64);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('records', 'activity')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 2);
}

#[test]
fn a_database_from_before_versions_keeps_what_it_held() {
    let dir = folder();
    let path = dir.join(crate::db::FILE);
    before_versions(&path);
    for _ in 0..2 {
        let db = Db::open(&path).unwrap();
        assert_eq!(version(&db.connection()), MIGRATIONS.len() as i64);
        let note: serde_json::Value = db.get("volume-note", "v").unwrap();
        assert_eq!(note["name"], "kept");
        let calls: i64 = db
            .connection()
            .query_row("SELECT count(*) FROM activity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(calls, 1);
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_database_a_newer_release_migrated_is_refused_and_left_as_it_was() {
    let dir = folder();
    let path = dir.join(crate::db::FILE);
    before_versions(&path);
    let newer = MIGRATIONS.len() as i64 + 1;
    Connection::open(&path)
        .unwrap()
        .pragma_update(None, "user_version", newer)
        .unwrap();
    let error = format!("{:#}", Db::open(&path).err().unwrap());
    assert!(
        error.contains(&format!("schema version {newer}"))
            && error.contains(&format!("versions up to {}", MIGRATIONS.len()))
            && error.contains("Go back to an earlier release"),
        "{error}"
    );
    assert_eq!(version(&Connection::open(&path).unwrap()), newer);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_migration_that_fails_changes_nothing() {
    let mut conn = Connection::open_in_memory().unwrap();
    let error = run_these(
        &mut conn,
        &[records_and_activity, |transaction| {
            transaction.execute_batch("CREATE TABLE half(x); SELECT * FROM nowhere;")
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("migration 2 failed"), "{error}");
    assert_eq!(version(&conn), 0);
    let tables: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))
        .unwrap();
    assert_eq!(tables, 0);
}
