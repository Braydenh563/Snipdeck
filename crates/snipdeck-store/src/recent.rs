//! [`RecentRepo`] for [`Store`]: the opt-in Recent inbox (SPEC §5.6).
//!
//! Entries live in their own table, outside the library: they are not items, are never
//! indexed by the full-text search, and are not in backups of the library views. Text is kept
//! in the row (the app caps it at 64 KiB before it gets here) rather than in the blob folder,
//! because it is short-lived and a blob folder file per clipboard change would be churn.
//! Sensitive and password-manager copies are never captured (the policy in `snipdeck-app`),
//! so nothing here is encrypted; `secure_delete` is on, so deleted text is zeroed on disk.
//!
//! Every time-dependent call takes `now` from the caller; this module never reads the clock.
//! A write is a database change, so it moves `ItemRepo::change_counter`; a read does not.

use std::time::Duration;

use rusqlite::{Connection, params};
use snipdeck_core::Timestamp;

use crate::codec::{from_nanos, nanos};
use crate::store::begin;
use crate::{Result, Store};

/// Version 3 of the schema. **Frozen** once released.
pub(crate) const V3: &str = r"
CREATE TABLE recent_entries (
    id         INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    hash       BLOB    NOT NULL UNIQUE CHECK (length(hash) = 32),
    text       TEXT    NOT NULL CHECK (length(text) > 0),
    source_app TEXT,
    copied_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX recent_entries_newest ON recent_entries (copied_at DESC, id DESC);
";

/// One captured clipboard text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntry {
    /// Stable until the entry is deleted or re-copied (a re-copy gets a new id).
    pub id: i64,
    /// The copied text.
    pub text: String,
    /// The executable the copy came from, when known.
    pub source_app: Option<String>,
    /// When it was last copied.
    pub copied_at: Timestamp,
}

/// The Recent inbox.
pub trait RecentRepo {
    /// Adds `text`. If the same text is already there it is replaced, so a re-copy moves the
    /// entry to the top (matched by BLAKE3 of the text). Returns the new entry's id.
    fn add_recent(&mut self, text: &str, source_app: Option<&str>, now: Timestamp) -> Result<i64>;

    /// Every entry, newest first.
    fn recent(&self) -> Result<Vec<RecentEntry>>;

    /// Deletes one entry; a missing id is not an error.
    fn delete_recent(&mut self, id: i64) -> Result<()>;

    /// Deletes everything; returns how many entries went.
    fn clear_recent(&mut self) -> Result<usize>;

    /// Deletes entries copied before `now - max_age` (an entry exactly `max_age` old stays),
    /// then all but the newest `max_entries`. Returns how many went.
    fn prune_recent(
        &mut self,
        now: Timestamp,
        max_entries: usize,
        max_age: Duration,
    ) -> Result<usize>;
}

fn hash(text: &str) -> [u8; 32] {
    *blake3::hash(text.as_bytes()).as_bytes()
}

fn row(conn: &Connection, sql: &str) -> Result<Vec<RecentEntry>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, text, source_app, at)| {
            Ok(RecentEntry {
                id,
                text,
                source_app,
                copied_at: from_nanos(at)?,
            })
        })
        .collect()
}

impl RecentRepo for Store {
    fn add_recent(&mut self, text: &str, source_app: Option<&str>, now: Timestamp) -> Result<i64> {
        let at = nanos(now)?;
        let tx = begin(&mut self.conn)?;
        let h = hash(text);
        tx.execute(
            "DELETE FROM recent_entries WHERE hash = ?1",
            params![&h[..]],
        )?;
        tx.execute(
            "INSERT INTO recent_entries (hash, text, source_app, copied_at) VALUES (?1, ?2, ?3, ?4)",
            params![&h[..], text, source_app, at],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    fn recent(&self) -> Result<Vec<RecentEntry>> {
        row(
            &self.conn,
            "SELECT id, text, source_app, copied_at FROM recent_entries
             ORDER BY copied_at DESC, id DESC",
        )
    }

    fn delete_recent(&mut self, id: i64) -> Result<()> {
        let tx = begin(&mut self.conn)?;
        tx.execute("DELETE FROM recent_entries WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    fn clear_recent(&mut self) -> Result<usize> {
        let tx = begin(&mut self.conn)?;
        let n = tx.execute("DELETE FROM recent_entries", [])?;
        tx.commit()?;
        Ok(n)
    }

    fn prune_recent(
        &mut self,
        now: Timestamp,
        max_entries: usize,
        max_age: Duration,
    ) -> Result<usize> {
        let age = i64::try_from(max_age.as_nanos()).unwrap_or(i64::MAX);
        let cutoff = nanos(now)?.saturating_sub(age);
        let keep = i64::try_from(max_entries).unwrap_or(i64::MAX);
        let tx = begin(&mut self.conn)?;
        let old = tx.execute(
            "DELETE FROM recent_entries WHERE copied_at < ?1",
            params![cutoff],
        )?;
        let extra = tx.execute(
            "DELETE FROM recent_entries WHERE id NOT IN
               (SELECT id FROM recent_entries ORDER BY copied_at DESC, id DESC LIMIT ?1)",
            params![keep],
        )?;
        tx.commit()?;
        Ok(old + extra)
    }
}
