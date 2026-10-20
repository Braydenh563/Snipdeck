//! [`Store`]: the SQLite database plus the blob folder, behind every repository trait.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use snipdeck_core::BlobRef;

use crate::blobs::Blobs;
use crate::codec::{blob_refs, is_valid_name};
use crate::crypto::DataKey;
use crate::migrations::{self, MIGRATIONS};
use crate::{Error, Result};

/// The database file inside the data folder.
const DB_FILE: &str = "snipdeck.db";
/// The single-writer lock file inside the data folder.
const LOCK_FILE: &str = "snipdeck.lock";
/// The blob folder inside the data folder.
const BLOB_DIR: &str = "blobs";

/// Snipdeck's storage: one SQLite database and one blob folder, implementing every repository
/// trait ([`BoardRepo`](crate::BoardRepo), [`ListRepo`](crate::ListRepo),
/// [`ItemRepo`](crate::ItemRepo), [`TagRepo`](crate::TagRepo), [`TrashRepo`](crate::TrashRepo)
/// and [`BlobStore`](crate::BlobStore)).
///
/// Opening a data folder takes its lock: the database is single-writer (ADR-0002), so a second
/// [`Store::open`] on the same folder, from this or another process, fails with
/// [`Error::Locked`] until the first one is dropped.
///
/// Every write runs in one transaction, so a failed call changes nothing. Work that follows a
/// commit (deleting blob files that are no longer used, folding the write-ahead log back into
/// the database) is best effort and never turns a committed change into an error; whatever it
/// leaves behind is collected by [`Store::gc_blobs`] or the next checkpoint.
pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) blobs: Blobs,
    /// The unlocked data key, when the key ring is unlocked (see `keys.rs`).
    pub(crate) key: Option<DataKey>,
    // Last, so the database is closed before the lock is released.
    _lock: Option<File>,
}

impl core::fmt::Debug for Store {
    /// No content and no key material (review L5).
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Store")
            .field("db", &self.conn.path().unwrap_or(":memory:"))
            .field("blobs", &self.blobs)
            .field("unlocked", &self.key.is_some())
            .finish_non_exhaustive()
    }
}

/// What [`Store::gc_blobs`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GcReport {
    /// Blobs that no item referenced.
    pub blobs: usize,
    /// Their total size in bytes.
    pub bytes: u64,
    /// Temporary files left by interrupted writes.
    pub leftovers: usize,
    /// Files that could not be deleted (for example held open by another program) and folders
    /// or files that could not be read; the next sweep tries again.
    pub failed: usize,
}

impl Store {
    /// Opens (creating it if needed) the data folder at `folder`: takes its lock, opens the
    /// database in WAL mode with foreign keys on, and migrates it to the current schema.
    ///
    /// A database written by a newer Snipdeck is refused with [`Error::SchemaTooNew`] and left
    /// untouched; a migrating open first copies the old database to `snipdeck.db.pre-vN.bak`.
    pub fn open(folder: impl AsRef<Path>) -> Result<Self> {
        let folder = folder.as_ref();
        fs::create_dir_all(folder)?;
        let lock = acquire_lock(folder)?;

        let db_path = folder.join(DB_FILE);
        let mut conn = Connection::open(&db_path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // Before migrating, so the migration's own deletes are scrubbed too. Changes no file.
        secure_delete(&conn)?;
        let outcome = migrations::migrate(&mut conn, MIGRATIONS, &db_path, true)?;
        configure(&conn)?;
        if outcome.from != outcome.to {
            // Pages of a dropped table (the plain Recent inbox, version 5) may sit in the log.
            checkpoint(&conn);
        }
        if outcome.from == 1 {
            // Version 1 ran without `secure_delete`, so freed pages may still hold old
            // content: rewrite the file once. Best effort, the migration itself has committed.
            let _ = conn.execute_batch("VACUUM;");
            checkpoint(&conn);
        }
        Ok(Self {
            conn,
            blobs: Blobs::on_disk(folder.join(BLOB_DIR)),
            key: None,
            _lock: Some(lock),
        })
    }

    /// An empty store that lives only in memory, blobs included: for tests and previews.
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        secure_delete(&conn)?;
        migrations::migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false)?;
        configure(&conn)?;
        Ok(Self {
            conn,
            blobs: Blobs::in_memory(),
            key: None,
            _lock: None,
        })
    }

    /// The schema version of the open database.
    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    /// Deletes blob files that no item references and that are at least `grace` old, plus
    /// leftovers of interrupted writes.
    ///
    /// Purging the trash and replacing an item's content already remove the blobs they
    /// orphan; this sweep is for what a crash leaves behind. The grace period protects a
    /// blob that [`BlobStore::put`](crate::BlobStore::put) has written but the caller has not
    /// yet attached to an item (`put` refreshes the age of a blob it finds already stored).
    pub fn gc_blobs(&mut self, grace: Duration) -> Result<GcReport> {
        let referenced: HashSet<BlobRef> = blob_refs(
            &self.conn,
            "SELECT blob_hash FROM item_formats WHERE blob_hash IS NOT NULL
             UNION SELECT blob_hash FROM item_files WHERE blob_hash IS NOT NULL",
            [],
        )?
        .into_iter()
        .collect();

        let now = SystemTime::now();
        let old_enough =
            |modified: SystemTime| now.duration_since(modified).is_ok_and(|age| age >= grace);
        let mut report = GcReport::default();
        let (blobs, leftovers, unreadable) = self.blobs.list()?;
        report.failed += unreadable;
        // A file that cannot be deleted now is counted and left for the next sweep, so one
        // locked file does not stop the rest (review M4).
        for stored in blobs {
            if !referenced.contains(&stored.blob) && old_enough(stored.modified) {
                if self.blobs.remove(&stored.blob, false).is_ok() {
                    report.blobs += 1;
                    report.bytes += stored.len;
                } else {
                    report.failed += 1;
                }
            }
        }
        for leftover in leftovers {
            if old_enough(leftover.modified) {
                if Blobs::remove_leftover(&leftover).is_ok() {
                    report.leftovers += 1;
                } else {
                    report.failed += 1;
                }
            }
        }
        Ok(report)
    }

    /// After a commit: deletes the blob files in `candidates` that no row references and that
    /// no `put` is still waiting to attach (review M3). With `scrub` (the blobs held plaintext
    /// of an item that is now sensitive) files are zeroed before they are deleted.
    ///
    /// Best effort (review M4): the change is committed, so a blob that cannot be checked or
    /// deleted now is left to [`Store::gc_blobs`], and the number left is returned for tests.
    pub(crate) fn release_blobs(&mut self, candidates: &[BlobRef], scrub: bool) -> usize {
        let mut left = 0;
        for blob in candidates {
            if self.blobs.is_pending(blob) {
                continue;
            }
            let used = self
                .conn
                .query_row(
                    "SELECT 1 FROM item_formats WHERE blob_hash = ?1
                     UNION ALL SELECT 1 FROM item_files WHERE blob_hash = ?1 LIMIT 1",
                    [&blob.as_bytes()[..]],
                    |_| Ok(()),
                )
                .optional();
            match used {
                Ok(Some(())) => {}
                Ok(None) => {
                    if self.blobs.remove(blob, scrub).is_err() {
                        left += 1;
                    }
                }
                Err(_) => left += 1,
            }
        }
        left
    }

    /// Checks the invariants the schema cannot express and returns a description of each
    /// violation (empty when healthy): SQLite's own integrity check, dangling references, the
    /// search index matching the items, every item having content, contiguous format order,
    /// every referenced blob being present, no plaintext of a sensitive item in the index,
    /// secure delete being on (database and index), encrypted items holding only envelopes,
    /// valid board, list and tag names, and tag keys that do not clash.
    pub fn check_integrity(&self) -> Result<Vec<String>> {
        let mut problems = Vec::new();
        let conn = &self.conn;

        let mut stmt = conn.prepare("PRAGMA integrity_check")?;
        for line in stmt.query_map([], |r| r.get::<_, String>(0))? {
            let line = line?;
            if line != "ok" {
                problems.push(format!("sqlite integrity_check: {line}"));
            }
        }
        drop(stmt);
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        for row in stmt.query_map([], |r| r.get::<_, String>(0))? {
            problems.push(format!("dangling reference from table `{}`", row?));
        }
        drop(stmt);
        if let Err(e) = conn.execute(
            "INSERT INTO item_fts (item_fts) VALUES ('integrity-check')",
            [],
        ) {
            problems.push(format!("full-text index is damaged: {e}"));
        }

        let counts: [(&str, &str); 7] = [
            (
                "items missing from the search index",
                "SELECT count(*) FROM items WHERE seq NOT IN (SELECT rowid FROM item_fts)",
            ),
            (
                "search index rows without an item",
                "SELECT count(*) FROM item_fts WHERE rowid NOT IN (SELECT seq FROM items)",
            ),
            (
                "search index titles out of date",
                "SELECT count(*) FROM item_fts f JOIN items i ON i.seq = f.rowid WHERE f.title <> i.title",
            ),
            (
                "sensitive items with content in the search index",
                "SELECT count(*) FROM item_fts f JOIN items i ON i.seq = f.rowid
                 WHERE i.sensitive = 1 AND f.body <> ''",
            ),
            (
                "items without any format",
                "SELECT count(*) FROM items i
                 WHERE NOT EXISTS (SELECT 1 FROM item_formats f WHERE f.item_id = i.id)",
            ),
            (
                "items whose format order has gaps",
                "SELECT count(*) FROM (SELECT item_id FROM item_formats
                 GROUP BY item_id HAVING max(idx) <> count(*) - 1)",
            ),
            (
                "files formats without files",
                "SELECT count(*) FROM item_formats f WHERE f.kind = 'files'
                 AND NOT EXISTS (SELECT 1 FROM item_files x WHERE x.item_id = f.item_id)",
            ),
        ];
        for (what, sql) in counts {
            let n: i64 = conn.query_row(sql, [], |r| r.get(0))?;
            if n > 0 {
                problems.push(format!("{n} {what}"));
            }
        }

        self.security_problems(&mut problems)?;
        name_problems(conn, &mut problems)?;

        let mut stmt = conn.prepare(
            "SELECT blob_hash, len FROM item_formats WHERE blob_hash IS NOT NULL
             UNION SELECT blob_hash, len FROM item_files WHERE blob_hash IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (hash, len) = row?;
            let blob = <[u8; 32]>::try_from(hash).map(BlobRef::from_bytes);
            match (blob, u64::try_from(len)) {
                (Ok(blob), Ok(len)) if self.blobs.has(&blob, len) => {}
                (Ok(blob), _) => problems.push(format!("blob {blob} is missing or the wrong size")),
                (Err(_), _) => problems.push("a blob hash is not 32 bytes".to_owned()),
            }
        }
        Ok(problems)
    }

    /// The encryption-at-rest part of [`Store::check_integrity`]: secure delete in the
    /// database and the index, envelopes in encrypted items, and tag keys.
    fn security_problems(&self, problems: &mut Vec<String>) -> Result<()> {
        let conn = &self.conn;
        // Deleted content must be overwritten (review H1, `docs/crypto.md`).
        let secure: i64 = conn.pragma_query_value(None, "secure_delete", |r| r.get(0))?;
        if secure != 1 {
            problems.push(
                "secure_delete is off: deleted content may stay readable in the database file"
                    .to_owned(),
            );
        }
        let fts_secure: Option<i64> = conn
            .query_row(
                "SELECT v FROM item_fts_config WHERE k = 'secure-delete'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if fts_secure != Some(1) {
            problems.push(
                "the full-text index does not use secure-delete: deleted text may stay in it"
                    .to_owned(),
            );
        }

        // Encrypted items hold envelopes only (review M6). Blob payloads are checked by their
        // first bytes, not read whole.
        let mut not_sealed = conn.query_row(
            "SELECT count(*) FROM (
                 SELECT f.inline_bytes AS b FROM item_formats f JOIN items i ON i.id = f.item_id
                  WHERE i.encrypted = 1 AND f.kind <> 'files'
                 UNION ALL
                 SELECT x.inline_bytes FROM item_files x JOIN items i ON i.id = x.item_id
                  WHERE i.encrypted = 1)
             WHERE b IS NOT NULL AND (length(b) < 46 OR substr(b, 1, 6) <> ?1)",
            [&crate::crypto::envelope_magic()[..]],
            |r| r.get::<_, i64>(0),
        )?;
        let mut stmt = conn.prepare(
            "SELECT f.blob_hash FROM item_formats f JOIN items i ON i.id = f.item_id
              WHERE i.encrypted = 1 AND f.blob_hash IS NOT NULL
             UNION ALL
             SELECT x.blob_hash FROM item_files x JOIN items i ON i.id = x.item_id
              WHERE i.encrypted = 1 AND x.blob_hash IS NOT NULL",
        )?;
        for hash in stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))? {
            if let Ok(hash) = <[u8; 32]>::try_from(hash?) {
                // A missing blob is reported below.
                if matches!(
                    self.blobs
                        .starts_with(&BlobRef::from_bytes(hash), crate::crypto::envelope_magic()),
                    Ok(false)
                ) {
                    not_sealed += 1;
                }
            }
        }
        drop(stmt);
        if not_sealed > 0 {
            problems.push(format!(
                "{not_sealed} encrypted items with a payload that is not an envelope"
            ));
        }

        // Tags whose key is not the folded name: they clash with another tag (review N5).
        let mut stmt = conn.prepare("SELECT name, name_key FROM tags")?;
        let mut clashing = 0_usize;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (name, key) = row?;
            if key != crate::tags::name_key(&name) {
                clashing += 1;
            }
        }
        drop(stmt);
        if clashing > 0 {
            problems.push(format!(
                "{clashing} tags whose names clash ignoring case with another tag; rename one"
            ));
        }

        Ok(())
    }

    /// The value of a SQLite pragma, for tests (review N6: not in production builds).
    #[cfg(any(test, feature = "testing"))]
    #[doc(hidden)]
    pub fn pragma_text(&self, name: &str) -> Result<String> {
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::Invalid(format!("`{name}` is not a pragma name")));
        }
        let value: rusqlite::types::Value =
            self.conn
                .query_row(&format!("PRAGMA {name}"), [], |r| r.get(0))?;
        Ok(match value {
            rusqlite::types::Value::Text(t) => t,
            rusqlite::types::Value::Integer(i) => i.to_string(),
            other => format!("{other:?}"),
        })
    }
}

/// Board, list and tag names that are not what [`Name::new`](snipdeck_core::Name::new)
/// would store.
fn name_problems(conn: &Connection, problems: &mut Vec<String>) -> Result<()> {
    // Names the schema cannot validate (review L9): listings show them repaired.
    for table in ["boards", "lists", "tags"] {
        let mut stmt = conn.prepare(&format!("SELECT name FROM {table}"))?;
        let mut bad = 0_usize;
        for name in stmt.query_map([], |r| r.get::<_, String>(0))? {
            if !is_valid_name(&name?) {
                bad += 1;
            }
        }
        if bad > 0 {
            problems.push(format!("{bad} {table} with an invalid name"));
        }
    }

    Ok(())
}

/// Starts a write transaction that takes the write lock up front.
pub(crate) fn begin(conn: &mut Connection) -> Result<Transaction<'_>> {
    Ok(conn.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

/// Deleted content is overwritten with zeros (review H1, `docs/crypto.md`).
///
/// `ON` rather than `FAST`: `FAST` only clears content when that costs no extra I/O, which
/// leaves it on pages moved to the freelist; `ON` also zeroes those. The cost is extra
/// writes when rows are deleted or shrink, which for a clipboard library is small.
fn secure_delete(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "secure_delete", "ON")?;
    Ok(())
}

/// Folds the write-ahead log into the database and truncates it to zero bytes, so page images
/// of content that was just deleted or encrypted do not linger in `snipdeck.db-wal`.
///
/// Best effort (it runs after a commit): if it cannot finish, the next checkpoint (at the
/// latest when the store is closed) does the same.
pub(crate) fn checkpoint(conn: &Connection) {
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
}

/// Connection settings that apply to every open.
fn configure(conn: &Connection) -> Result<()> {
    // The result row (the mode actually in effect) is `memory` for in-memory databases and
    // may be `delete` on file systems without shared memory; both still work, just slower.
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
    // FULL: the WAL is synced at every commit. With NORMAL a power cut could lose the last
    // commit while the blob files that commit released (`Store::finish`) are already unlinked, so
    // the surviving row would point at a missing blob. Writes are rare and small, so the extra
    // fsync per commit is cheap.
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

/// Takes the exclusive lock on `<folder>/snipdeck.lock`, held until the file is dropped.
fn acquire_lock(folder: &Path) -> Result<File> {
    let path: PathBuf = folder.join(LOCK_FILE);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(Error::Locked {
            folder: folder.to_owned(),
        }),
        Err(TryLockError::Error(e)) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use snipdeck_core::INLINE_LIMIT;

    use super::*;
    use crate::BlobStore as _;
    use crate::blobs::FAIL_REMOVES;

    #[test]
    fn the_integrity_check_reports_secure_delete_being_off() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.check_integrity().unwrap(), Vec::<String>::new());
        store
            .conn
            .pragma_update(None, "secure_delete", "OFF")
            .unwrap();
        let problems = store.check_integrity().unwrap().join("\n");
        assert!(problems.contains("secure_delete is off"), "{problems}");
    }

    /// Review M4: one file that cannot be deleted neither stops the sweep nor fails it.
    #[test]
    fn gc_counts_failed_deletes_and_carries_on() {
        let mut store = Store::open_in_memory().unwrap();
        store.put(vec![1; INLINE_LIMIT + 1]).unwrap();
        store.put(vec![2; INLINE_LIMIT + 1]).unwrap();
        FAIL_REMOVES.with(|f| f.set(true));
        let report = store.gc_blobs(Duration::ZERO);
        FAIL_REMOVES.with(|f| f.set(false));
        let report = report.unwrap();
        assert_eq!((report.blobs, report.failed), (0, 2));
        assert_eq!(
            store.gc_blobs(Duration::ZERO).unwrap().blobs,
            2,
            "the next sweep"
        );
    }

    #[test]
    fn released_blobs_that_cannot_be_deleted_are_left_for_gc() {
        let mut store = Store::open_in_memory().unwrap();
        let content = store.put(vec![3; INLINE_LIMIT + 1]).unwrap();
        let snipdeck_core::Content::Blob { blob, .. } = content else {
            panic!()
        };
        store.blobs.attached([blob]);
        FAIL_REMOVES.with(|f| f.set(true));
        let left = store.release_blobs(&[blob], false);
        FAIL_REMOVES.with(|f| f.set(false));
        assert_eq!(left, 1);
        assert_eq!(store.gc_blobs(Duration::ZERO).unwrap().blobs, 1);
    }

    #[test]
    fn debug_output_names_no_key_or_content() {
        let store = Store::open_in_memory().unwrap();
        let shown = format!("{store:?}");
        assert!(
            shown.starts_with("Store {") && shown.contains("unlocked: false"),
            "{shown}"
        );
    }
}
