use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Mutex;

pub struct Db(Mutex<Connection>);
impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL, id TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY(kind,id));")?;
        Ok(Self(Mutex::new(conn)))
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
    pub fn remove(&self, kind: &str, id: &str) -> Result<()> {
        self.0.lock().unwrap().execute(
            "DELETE FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
        )?;
        Ok(())
    }
    pub fn jobs_summary(&self) -> Result<Vec<crate::model::Job>> {
        let conn = self.0.lock().unwrap();
        // Strip potentially large logs in SQLite, before allocating/deserializing rows.
        let mut stmt = conn.prepare("SELECT json_set(data, '$.log', '') FROM records WHERE kind='job' ORDER BY rowid DESC LIMIT 100")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
}
