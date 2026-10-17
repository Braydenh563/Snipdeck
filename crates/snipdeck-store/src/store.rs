//! [`Store`]: the SQLite database plus the blob folder, behind every repository trait.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use snipdeck_core::BlobRef;

use crate::blobs::Blobs;
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
/// Every write runs in one transaction, so a failed call changes nothing.
#[derive(Debug)]
pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) blobs: Blobs,
    // Last, so the database is closed before the lock is released.
    _lock: Option<File>,
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
        migrations::migrate(&mut conn, MIGRATIONS, &db_path, true)?;
        configure(&conn)?;
        Ok(Self {
            conn,
            blobs: Blobs::on_disk(folder.join(BLOB_DIR)),
            _lock: Some(lock),
        })
    }

    /// An empty store that lives only in memory, blobs included: for tests and previews.
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrations::migrate(&mut conn, MIGRATIONS, Path::new(":memory:"), false)?;
        configure(&conn)?;
        Ok(Self {
            conn,
            blobs: Blobs::in_memory(),
            _lock: None,
        })
    }

    /// The schema version of the open database.
    pub fn schema_version(&self) -> u32 {
        self.conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap_or(0)
    }

    /// Deletes blob files that no item references and that are at least `grace` old, plus
    /// leftovers of interrupted writes.
    ///
    /// Purging the trash and replacing an item's content already remove the blobs they
    /// orphan; this sweep is for what a crash leaves behind. The grace period protects a
    /// blob that [`BlobStore::put`](crate::BlobStore::put) has written but the caller has not
    /// yet attached to an item (`put` refreshes the age of a blob it finds already stored).
    pub fn gc_blobs(&mut self, grace: Duration) -> Result<GcReport> {
        let mut referenced = HashSet::new();
        let mut stmt = self.conn.prepare(
            "SELECT blob_hash FROM item_formats WHERE blob_hash IS NOT NULL
             UNION SELECT blob_hash FROM item_files WHERE blob_hash IS NOT NULL",
        )?;
        for hash in stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))? {
            let hash: [u8; 32] = hash?
                .try_into()
                .map_err(|_| Error::Corrupt("a blob hash is not 32 bytes".to_owned()))?;
            referenced.insert(BlobRef::from_bytes(hash));
        }
        drop(stmt);

        let now = SystemTime::now();
        let old_enough =
            |modified: SystemTime| now.duration_since(modified).is_ok_and(|age| age >= grace);
        let mut report = GcReport::default();
        let (blobs, leftovers) = self.blobs.list()?;
        for stored in blobs {
            if !referenced.contains(&stored.blob) && old_enough(stored.modified) {
                self.blobs.remove(&stored.blob)?;
                report.blobs += 1;
                report.bytes += stored.len;
            }
        }
        for leftover in leftovers {
            if old_enough(leftover.modified) {
                Blobs::remove_leftover(&leftover)?;
                report.leftovers += 1;
            }
        }
        Ok(report)
    }

    /// Deletes the blob files in `candidates` that no row references any more.
    pub(crate) fn release_blobs(&mut self, candidates: Vec<BlobRef>) -> Result<()> {
        for blob in candidates {
            let used = self
                .conn
                .query_row(
                    "SELECT 1 FROM item_formats WHERE blob_hash = ?1
                     UNION ALL SELECT 1 FROM item_files WHERE blob_hash = ?1 LIMIT 1",
                    [&blob.as_bytes()[..]],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !used {
                self.blobs.remove(&blob)?;
            }
        }
        Ok(())
    }

    /// Checks the invariants the schema cannot express and returns a description of each
    /// violation (empty when healthy): SQLite's own integrity check, dangling references, the
    /// search index matching the items, every item having content, contiguous format order,
    /// every referenced blob being present, and no plaintext of a sensitive item in the index.
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

    /// The value of a SQLite pragma, for tests.
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

/// Starts a write transaction that takes the write lock up front.
pub(crate) fn begin(conn: &mut Connection) -> Result<Transaction<'_>> {
    Ok(conn.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

/// Connection settings that apply to every open.
fn configure(conn: &Connection) -> Result<()> {
    // The result row (the mode actually in effect) is `memory` for in-memory databases and
    // may be `delete` on file systems without shared memory; both still work, just slower.
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get::<_, String>(0))?;
    // With WAL, NORMAL cannot corrupt the database; a power cut may lose the last commit.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
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
