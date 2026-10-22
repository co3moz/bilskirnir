use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Mutex;

pub struct Db(Mutex<Connection>);
impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL, id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(kind,id));")?;
        crate::activity::create_table(&conn)?;
        Ok(Self(Mutex::new(conn)))
    }
    /// The connection itself, for a module with a table of its own, such as the agent activity.
    pub(crate) fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap()
    }
    pub fn put<T: Serialize>(&self, kind: &str, id: &str, value: &T) -> Result<()> {
        self.0.lock().unwrap().execute("INSERT INTO records(kind,id,data) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET data=excluded.data", params![kind,id,serde_json::to_string(value)?])?;
        Ok(())
    }
    pub fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        let text: String = self.0.lock().unwrap().query_row(
            "SELECT data FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&text)?)
    }
    pub fn list<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<T>> {
        let conn = self.0.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT data FROM records WHERE kind=?1 ORDER BY rowid DESC")?;
        let rows = stmt.query_map([kind], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    /// Every record of `kind` with its id, oldest first, for records that do not carry their id.
    pub fn entries<T: DeserializeOwned>(&self, kind: &str) -> Result<Vec<(String, T)>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, data FROM records WHERE kind=?1 ORDER BY rowid")?;
        let rows = stmt.query_map([kind], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (id, data) = r?;
            Ok((id, serde_json::from_str(&data)?))
        })
        .collect()
    }
    /// Insert records that must arrive together, as `(kind, id, value)`, in one transaction: all
    /// of them or none. An existing record is never replaced; one of the same kind and id fails it.
    pub fn insert_all(&self, records: &[(&str, &str, serde_json::Value)]) -> Result<()> {
        let mut conn = self.0.lock().unwrap();
        let transaction = conn.transaction()?;
        for (kind, id, value) in records {
            transaction.execute(
                "INSERT INTO records(kind,id,data) VALUES(?1,?2,?3)",
                params![kind, id, serde_json::to_string(value)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn remove(&self, kind: &str, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
        )?;
        Ok(())
    }
    /// Remove the finished jobs beyond the newest `keep`. A job still running or queued stays
    /// whatever its age; its end is what marks it finished.
    pub fn prune_jobs(&self, keep: usize) -> Result<usize> {
        let removed = self.0.lock().unwrap().execute(
            "DELETE FROM records WHERE kind='job' AND json_extract(data, '$.finished_at') IS NOT NULL AND rowid NOT IN (SELECT rowid FROM records WHERE kind='job' ORDER BY rowid DESC LIMIT ?1)",
            params![keep as i64],
        )?;
        Ok(removed)
    }
    pub fn jobs_summary(&self) -> Result<Vec<crate::model::Job>> {
        let conn = self.0.lock().unwrap();
        // Strip potentially large logs in SQLite, before allocating/deserializing rows.
        let mut stmt = conn.prepare("SELECT json_set(data, '$.log', '') FROM records WHERE kind='job' ORDER BY rowid DESC LIMIT 100")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    /// The jobs that are queued or running, newest first, without their logs.
    pub fn active_jobs(&self) -> Result<Vec<crate::model::Job>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT json_set(data, '$.log', '') FROM records WHERE kind='job' AND json_extract(data, '$.status') IN ('queued', 'running') ORDER BY rowid DESC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
