//! Job logs, kept apart from the job records in the chunks they were written in, so adding to a
//! log writes the new text alone instead of the whole record again.
//!
//! A log keeps its last [`LOG_LIMIT`] bytes, as [`Job::trim_log`] always kept them. Each chunk
//! records where the log ends after it, and a chunk that ends before the part of the log that is
//! kept goes; the one that crosses into that part stays whole. The chunks therefore always hold
//! at least the bytes the log keeps, and reading a job puts the record's own log, which managers
//! before the table wrote into it, and its chunks together and trims the whole: the same text the
//! record would hold had every addition trimmed it. A log started again goes on counting where it
//! ended, so a position a reader holds never comes round again.
use super::Db;
use crate::model::{Job, LOG_LIMIT};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

/// The database kind of job records.
pub(super) const JOB: &str = "job";

impl Db {
    /// [`Db::job_and_end`] without where the log ends, as the tests read a job.
    #[cfg(test)]
    pub fn job(&self, id: &str) -> Result<Job> {
        Ok(self.job_and_end(id)?.0)
    }

    /// A job with its whole log, as the API shows it, and where that log ends in the job's whole
    /// output: every byte ever added to it, of which the log keeps the last [`LOG_LIMIT`], so a
    /// reader can ask for what follows. A record read here is never saved again, or its log would
    /// land in the record: read the one to change with [`Db::get`].
    pub fn job_and_end(&self, id: &str) -> Result<(Job, u64)> {
        let conn = self.connection();
        let data: String = conn.query_row(
            "SELECT data FROM records WHERE kind=?1 AND id=?2",
            params![JOB, id],
            |row| row.get(0),
        )?;
        let mut job: Job = serde_json::from_str(&data)?;
        // The record's own log, of a manager before the table, comes before every chunk.
        let mut end = job.log.len() as u64;
        let mut chunks =
            conn.prepare_cached("SELECT text, ends_at FROM job_log WHERE job=?1 ORDER BY ends_at")?;
        let mut last = 0;
        for chunk in chunks.query_map([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })? {
            let (text, ends_at) = chunk?;
            job.log.push_str(&text);
            last = ends_at;
        }
        end += u64::try_from(last).unwrap_or_default();
        drop(chunks);
        drop(conn);
        job.trim_log();
        Ok((job, end))
    }

    /// Add `text` to a job's log as it is. A job that is not there fails it, as reading it does.
    pub fn append_job_log(&self, id: &str, text: &str) -> Result<()> {
        let mut conn = self.connection();
        let transaction = conn.transaction()?;
        let saved: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM records WHERE kind=?1 AND id=?2)",
            params![JOB, id],
            |row| row.get(0),
        )?;
        if !saved {
            return Err(rusqlite::Error::QueryReturnedNoRows.into());
        }
        append(&transaction, id, text)?;
        transaction.commit()?;
        Ok(())
    }

    /// Save a job's record and add `text` to its log in one transaction, so nobody reads the one
    /// without the other. With `own_line`, `text` begins on a line of its own. The record keeps
    /// the log it was read with, from [`Db::get`].
    pub fn put_job_logged(&self, job: &Job, text: &str, own_line: bool) -> Result<()> {
        let mut conn = self.connection();
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO records(kind,id,data) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET data=excluded.data",
            params![JOB, job.id, serde_json::to_string(job)?],
        )?;
        if own_line && open_line(&transaction, job)? {
            append(&transaction, &job.id, "\n")?;
        }
        append(&transaction, &job.id, text)?;
        transaction.commit()?;
        Ok(())
    }

    /// Start a job's log again with `text`: the log its record carries and its chunks go, while
    /// its positions go on from where it ended, so a reader that asks from its last end, as the
    /// dashboard and `get_job` do, reads `text` and what follows it instead of a position the
    /// new log reaches again.
    pub fn restart_job_log(&self, id: &str, text: &str) -> Result<()> {
        let mut conn = self.connection();
        let transaction = conn.transaction()?;
        // Where the log ends, as `job_and_end` counts it: the record's own log, then its chunks.
        let ended: i64 = transaction.query_row(
            "SELECT coalesce(length(CAST(json_extract(data, '$.log') AS BLOB)), 0)
                  + coalesce((SELECT max(ends_at) FROM job_log WHERE job=?2), 0)
             FROM records WHERE kind=?1 AND id=?2",
            params![JOB, id],
            |row| row.get(0),
        )?;
        transaction.execute(
            "UPDATE records SET data=json_set(data, '$.log', '') WHERE kind=?1 AND id=?2",
            params![JOB, id],
        )?;
        transaction.execute("DELETE FROM job_log WHERE job=?1", [id])?;
        // A chunk without text holds that end, which the next chunk counts on from.
        transaction.execute(
            "INSERT INTO job_log(job, text, ends_at) VALUES(?1, '', ?2)",
            params![id, ended],
        )?;
        append(&transaction, id, text)?;
        if !text.is_empty() {
            transaction.execute(
                "DELETE FROM job_log WHERE job=?1 AND ends_at=?2 AND text=''",
                params![id, ended],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

/// Add a chunk, and drop the chunks that now end before the part of the log that is kept. Of a
/// chunk longer than the log keeps, only its last [`LOG_LIMIT`] bytes are saved, from the start
/// of the character they begin in, since no earlier chunk is kept then either.
fn append(conn: &Connection, id: &str, text: &str) -> rusqlite::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let mut from = text.len().saturating_sub(LOG_LIMIT);
    while !text.is_char_boundary(from) {
        from -= 1;
    }
    let end: i64 = conn.query_row(
        "INSERT INTO job_log(job, text, ends_at)
         VALUES(?1, ?2, ?3 + coalesce((SELECT max(ends_at) FROM job_log WHERE job=?1), 0))
         RETURNING ends_at",
        params![id, &text[from..], text.len() as i64],
        |row| row.get(0),
    )?;
    let kept_from = end - LOG_LIMIT as i64;
    if kept_from > 0 {
        conn.execute(
            "DELETE FROM job_log WHERE job=?1 AND ends_at <= ?2",
            params![id, kept_from],
        )?;
    }
    Ok(())
}

/// Whether the log so far ends inside a line: its last chunk with text, or else the record's own
/// log.
fn open_line(conn: &Connection, job: &Job) -> rusqlite::Result<bool> {
    let last: Option<String> = conn
        .query_row(
            "SELECT substr(text, -1) FROM job_log WHERE job=?1 AND text<>'' ORDER BY ends_at DESC LIMIT 1",
            [&job.id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(match last {
        Some(last) => last != "\n",
        None => !job.log.is_empty() && !job.log.ends_with('\n'),
    })
}

/// Drop the chunks of jobs that are gone, such as those a manager before the table pruned.
pub(super) fn opened(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM job_log WHERE job NOT IN (SELECT id FROM records WHERE kind=?1)",
        [JOB],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "job_log_tests.rs"]
mod tests;
