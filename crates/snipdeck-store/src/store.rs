//! [`Store`]: the SQLite database plus the blob folder, behind every repository trait.

use std::cell::Cell;
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
    /// Rows changed by "quiet" writes (a use counted, the Recent inbox), which are not part
    /// of [`ItemRepo::change_counter`](crate::ItemRepo::change_counter).
    pub(crate) quiet: Quiet,
    /// While held, blob files an edit orphans are not deleted but queued here (with whether to
    /// scrub them), so a reader's snapshot taken before the edit can still open them.
    pub(crate) hold: BlobHold,
    // Last, so the database is closed before the lock is released.
    lock: Option<FolderLock>,
}

/// Blob releases deferred while a worker reads a snapshot ([`Store::hold_blob_release`]).
#[derive(Debug, Default)]
pub(crate) struct BlobHold {
    on: bool,
    queued: Vec<(BlobRef, bool)>,
}

/// The held single-writer lock of a data folder (`snipdeck.lock`).
///
/// A [`Store`] holds one while open. [`Store::close`] hands it back so a caller can keep the
/// folder to itself between closing one store and opening the next ([`Store::open_locked`]).
/// Dropping it releases the lock.
#[derive(Debug)]
pub struct FolderLock {
    _file: File,
    folder: PathBuf,
}

impl FolderLock {
    /// The data folder this lock guards.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Takes the exclusive lock for `folder`, which must exist.
    pub fn acquire(folder: &Path) -> Result<Self> {
        acquire_lock(folder)
    }
}

/// [`Store::open_locked`] failed; the folder lock is handed back.
#[derive(Debug)]
pub struct OpenLockedError {
    /// Why the open failed.
    pub error: Error,
    /// The lock, still held.
    pub lock: FolderLock,
}

impl core::fmt::Display for OpenLockedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for OpenLockedError {}

/// What [`Store::close`] hands back.
#[derive(Debug)]
pub struct Closed {
    /// The folder lock, still held; `None` for an in-memory store.
    pub lock: Option<FolderLock>,
    /// True when the write-ahead log was folded into `snipdeck.db` and the connection closed
    /// cleanly, so the database file alone holds every commit. When false, `-wal` and `-shm`
    /// must move along with the database file.
    pub self_contained: bool,
}

/// Counts of rows changed by writes that do not move `change_counter`.
#[derive(Debug, Default)]
pub(crate) struct Quiet {
    /// Every quiet write: counted uses and the Recent inbox.
    pub all: Cell<u64>,
    /// Counted uses alone.
    pub uses: Cell<u64>,
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

/// A zero-length database next to backups, attachments or a log is damage, not a new library.
fn refuse_empty_database(folder: &Path, db_path: &Path) -> Result<()> {
    let empty = fs::metadata(db_path).is_ok_and(|m| m.is_file() && m.len() == 0);
    if !empty {
        return Ok(());
    }
    let has_files = |dir: &Path| fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some());
    let log = fs::metadata(folder.join("snipdeck.db-wal")).is_ok_and(|m| m.len() > 0);
    if log || has_files(&folder.join(BLOB_DIR)) || has_files(&folder.join("backups")) {
        return Err(Error::EmptyDatabase {
            folder: folder.to_path_buf(),
        });
    }
    Ok(())
}

/// Whether an error from opening proves the database file is not a usable library, so that
/// setting it aside loses nothing and keeping it cannot help: SQLite says it is not a database
/// or is corrupt, the schema holds what Snipdeck could not have written, the file is empty
/// beside other library files, or it is another program's database. Everything else (I/O
/// errors, busy or locked, a full disk, an out-of-memory or interrupted open, a failed
/// migration, a newer database) can happen to a healthy library and moves nothing.
#[must_use]
pub fn is_damaged(error: &Error) -> bool {
    use rusqlite::ErrorCode;
    match error {
        Error::EmptyDatabase { .. } | Error::ForeignDatabase { .. } | Error::Corrupt(_) => true,
        Error::Sqlite(e) => matches!(
            e.sqlite_error_code(),
            Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt)
        ),
        _ => false,
    }
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
        // Before any connection: a rebuild that was interrupted is undone first, so a missing
        // database is never replaced by an empty one while a damaged library sits set aside.
        crate::aside::recover(folder, &lock)?;
        Self::open_locked(folder, lock).map_err(|failed| failed.error)
    }

    /// Like [`Store::open`], but a library that is damaged (see `is_damaged`) is moved aside
    /// with [`set_aside`](crate::set_aside) under `stem` (`snipdeck.damaged-<stamp>`) and a fresh
    /// one is opened, so the app can start and "Rebuild from backup" can restore the newest
    /// backup. Returns the set-aside folder's name when that happened.
    ///
    /// Nothing moves unless the error proves the file is not a usable Snipdeck database: not a
    /// database, corrupt, empty beside other library files, or another program's. A failed
    /// migration of an intact database, a transient I/O error (antivirus, a sync client, a
    /// network folder), a full or read-only disk, a folder another Snipdeck holds, a newer
    /// database and an interrupted rebuild all return their error with nothing moved. If the
    /// fresh open fails too, the move is undone.
    pub fn open_or_set_aside(
        folder: impl AsRef<Path>,
        stem: &str,
    ) -> Result<(Self, Option<String>)> {
        Self::open_or_set_aside_with(folder, stem, |f, lock| Self::open_locked(f, lock))
    }

    fn open_or_set_aside_with(
        folder: impl AsRef<Path>,
        stem: &str,
        fresh: impl FnOnce(&Path, FolderLock) -> core::result::Result<Self, Box<OpenLockedError>>,
    ) -> Result<(Self, Option<String>)> {
        let folder = folder.as_ref();
        fs::create_dir_all(folder)?;
        let lock = acquire_lock(folder)?;
        crate::aside::recover(folder, &lock)?;
        let failed = match Self::open_locked(folder, lock) {
            Ok(store) => return Ok((store, None)),
            Err(failed) => failed,
        };
        let OpenLockedError { error, lock } = *failed;
        if !is_damaged(&error) || crate::aside::set_aside(folder, &lock, stem).is_err() {
            return Err(error);
        }
        match fresh(folder, lock) {
            Ok(store) => {
                // The fresh library is the result, so the marker (which would undo the move at
                // the next start) must go, or be sealed so that the next start only finishes it.
                // If neither works, the next start would delete this library and put the
                // damaged one back: refuse to run on it (it holds nothing yet).
                let lock = store.lock.as_ref().ok_or_else(|| {
                    Error::Invalid("the fresh library holds no folder lock".to_owned())
                })?;
                if crate::aside::finish_rebuild(folder, lock).is_err()
                    && !crate::aside::marker_is_settled(folder)
                {
                    return Err(Error::RebuildInterrupted {
                        folder: folder.to_owned(),
                        aside: folder.join(stem),
                    });
                }
                Ok((store, Some(stem.to_owned())))
            }
            Err(failed) => {
                let OpenLockedError { error, lock } = *failed;
                let _ = crate::aside::undo_set_aside(folder, &lock);
                Err(error)
            }
        }
    }

    /// Opens an existing library for reading only: no folder or database is created, no lock is
    /// taken, nothing is migrated and the connection cannot write. For diagnostics that may run
    /// beside a running Snipdeck.
    ///
    /// A folder with no library is [`Error::Invalid`]; a library of another schema version is
    /// refused ([`Error::SchemaTooNew`] for a newer one, [`Error::Invalid`] for an older one,
    /// which only the app's own start-up migrates).
    pub fn open_read_only(folder: impl AsRef<Path>) -> Result<Self> {
        let folder = folder.as_ref();
        let db_path = folder.join(DB_FILE);
        if !db_path.is_file() {
            return Err(Error::Invalid(format!(
                "no library in {}",
                folder.display()
            )));
        }
        let conn = Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // A connection setting, not a write (the integrity check asks for it).
        secure_delete(&conn)?;
        let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let latest = migrations::latest(MIGRATIONS);
        if found > latest {
            return Err(Error::SchemaTooNew {
                found,
                supported: latest,
            });
        }
        if found < latest {
            return Err(Error::Invalid(
                "the library is from an older version; start Snipdeck once to update it".to_owned(),
            ));
        }
        Ok(Self {
            conn,
            blobs: Blobs::on_disk(folder.join(BLOB_DIR)),
            key: None,
            quiet: Quiet::default(),
            lock: None,
            hold: BlobHold::default(),
        })
    }

    /// Like [`Store::open`], for a caller that already holds the folder's lock (from
    /// [`Store::close`]); it does not look for an interrupted rebuild.
    ///
    /// When the open fails the lock comes back in the error, still held, so the caller can
    /// roll back (for example [`undo_set_aside`](crate::undo_set_aside)) before releasing it.
    /// A `lock` for another folder is refused.
    pub fn open_locked(
        folder: impl AsRef<Path>,
        lock: FolderLock,
    ) -> core::result::Result<Self, Box<OpenLockedError>> {
        let folder = folder.as_ref();
        if !crate::aside::same_folder(folder, lock.folder()) {
            return Err(Box::new(OpenLockedError {
                error: Error::Invalid("the folder lock is for another folder".to_owned()),
                lock,
            }));
        }
        match Self::open_unlocked(folder) {
            Ok(mut store) => {
                store.lock = Some(lock);
                Ok(store)
            }
            Err(error) => Err(Box::new(OpenLockedError { error, lock })),
        }
    }

    /// Opens the database and blob folder; the caller attaches the lock.
    fn open_unlocked(folder: &Path) -> Result<Self> {
        let db_path = folder.join(DB_FILE);
        refuse_empty_database(folder, &db_path)?;
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
            quiet: Quiet::default(),
            hold: BlobHold::default(),
            lock: None,
        })
    }

    /// A second, read-only view of the open database in `folder`, for work that must not hold
    /// the UI thread (an export, a backup, the Help window's checks): its own connection, no
    /// folder lock, no migration and no write of any kind. `key` is a copy of the unlocked data
    /// key ([`Store::duplicate_data_key`]), so encrypted items read as they do from the main
    /// store; a reader that decrypts nothing passes `None`.
    ///
    /// The database must already be open and migrated by a [`Store`] (WAL mode lets this reader
    /// run beside its writer); a write through the result fails. A caller that needs one
    /// consistent picture reads inside [`Store::begin_snapshot`].
    pub fn open_reader(folder: impl AsRef<Path>, key: Option<DataKey>) -> Result<Self> {
        let folder = folder.as_ref();
        let conn = Connection::open_with_flags(
            folder.join(DB_FILE),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // A per-connection setting: the integrity check reads it back, so it must match the owner's.
        secure_delete(&conn)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        Ok(Self {
            conn,
            blobs: Blobs::on_disk(folder.join(BLOB_DIR)),
            key,
            quiet: Quiet::default(),
            hold: BlobHold::default(),
            lock: None,
        })
    }

    /// How many live items are marked sensitive (one count query, nothing is decrypted).
    pub fn count_sensitive(&self) -> Result<usize> {
        crate::items::count_sensitive(&self.conn)
    }

    /// Starts a read transaction on a reader, so every read after the first sees the same
    /// moment of the database. Ends when the reader is dropped. Does nothing useful on the main
    /// store (which writes).
    pub fn begin_snapshot(&self) -> Result<()> {
        self.conn
            .execute_batch("BEGIN; SELECT 1 FROM sqlite_master LIMIT 1;")?;
        Ok(())
    }

    /// Closes the store: folds the write-ahead log into the database
    /// (`wal_checkpoint(TRUNCATE)`, checked) and closes the connection, keeping the folder
    /// lock held in the returned [`Closed`].
    pub fn close(self) -> Closed {
        let Self { conn, lock, .. } = self;
        let folded = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            // (busy, log frames, checkpointed frames); -1 frames means no log (not WAL mode).
            .is_ok_and(|(busy, log, done)| busy == 0 && log == done);
        let closed = conn.close().is_ok();
        Closed {
            lock,
            self_contained: folded && closed,
        }
    }

    /// The folder lock this store holds (`None` for an in-memory store).
    #[must_use]
    pub fn folder_lock(&self) -> Option<&FolderLock> {
        self.lock.as_ref()
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
            quiet: Quiet::default(),
            hold: BlobHold::default(),
            lock: None,
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
        if self.hold.on {
            // A worker's snapshot may still name files the sweep would remove: the next sweep
            // (or the end of the hold) collects them.
            return Ok(GcReport::default());
        }
        let referenced: HashSet<BlobRef> = blob_refs(
            &self.conn,
            "SELECT blob_hash FROM item_formats WHERE blob_hash IS NOT NULL
             UNION SELECT blob_hash FROM item_files WHERE blob_hash IS NOT NULL",
            [],
        )?
        .into_iter()
        .chain(crate::recent::blobs_of(
            &self.conn,
            "SELECT blob FROM recent_entries WHERE blob IS NOT NULL",
            [],
        )?)
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
            // A blob a `put` handed out and no row has taken yet (an editor holding a pasted
            // image) is not an orphan, however old its file is.
            if !referenced.contains(&stored.blob)
                && !self.blobs.is_pending(&stored.blob)
                && old_enough(stored.modified)
            {
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

    /// Starts or ends a hold on deleting blob files. While held, files that edits orphan stay on
    /// disk, so an archive worker reading a snapshot taken earlier never meets a missing file;
    /// ending the hold releases what was queued. Returns how many files could not be deleted
    /// then (left for [`Store::gc_blobs`]).
    pub fn hold_blob_release(&mut self, hold: bool) -> usize {
        self.hold.on = hold;
        if hold {
            return 0;
        }
        let queued = std::mem::take(&mut self.hold.queued);
        let (scrubbed, plain): (Vec<_>, Vec<_>) = queued.into_iter().partition(|(_, s)| *s);
        let scrubbed: Vec<BlobRef> = scrubbed.into_iter().map(|(b, _)| b).collect();
        let plain: Vec<BlobRef> = plain.into_iter().map(|(b, _)| b).collect();
        self.release_blobs(&scrubbed, true) + self.release_blobs(&plain, false)
    }

    /// After a commit: deletes the blob files in `candidates` that no row references and that
    /// no `put` is still waiting to attach (review M3). With `scrub` (the blobs held plaintext
    /// of an item that is now sensitive) files are zeroed before they are deleted.
    ///
    /// Best effort (review M4): the change is committed, so a blob that cannot be checked or
    /// deleted now is left to [`Store::gc_blobs`], and the number left is returned for tests.
    pub(crate) fn release_blobs(&mut self, candidates: &[BlobRef], scrub: bool) -> usize {
        if self.hold.on {
            self.hold
                .queued
                .extend(candidates.iter().map(|b| (*b, scrub)));
            return 0;
        }
        let mut left = 0;
        for blob in candidates {
            if self.blobs.is_pending(blob) {
                continue;
            }
            let used = self
                .conn
                .query_row(
                    "SELECT 1 FROM item_formats WHERE blob_hash = ?1
                     UNION ALL SELECT 1 FROM item_files WHERE blob_hash = ?1
                     UNION ALL SELECT 1 FROM recent_entries WHERE blob = ?2 LIMIT 1",
                    rusqlite::params![&blob.as_bytes()[..], blob.to_string()],
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
        const FTS_CHECK: &str = "INSERT INTO item_fts (item_fts) VALUES ('integrity-check')";
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
        // FTS5 runs its check as an INSERT, which a read-only handle (the Help window's) cannot
        // do. The check changes nothing, so run it on a short-lived writable connection instead,
        // rather than call a damaged index healthy. Only a corruption error means damage: a busy
        // database or a connection that will not open means the index was not checked.
        let result = match conn.execute(FTS_CHECK, []) {
            Err(rusqlite::Error::SqliteFailure(f, _))
                if f.code == rusqlite::ErrorCode::ReadOnly =>
            {
                conn.path().map(|path| {
                    Connection::open_with_flags(
                        path,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    )
                    .and_then(|c| {
                        c.busy_timeout(Duration::from_secs(1))?;
                        c.execute(FTS_CHECK, []).map(|_| ())
                    })
                })
            }
            other => Some(other.map(|_| ())),
        };
        if let Some(Err(e)) = result
            && matches!(
                &e,
                rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::DatabaseCorrupt
            )
        {
            problems.push(format!("full-text index is damaged: {e}"));
        }

        let counts: [(&str, &str); 6] = [
            (
                "items missing from the search index",
                "SELECT count(*) FROM items WHERE seq NOT IN (SELECT rowid FROM item_fts)",
            ),
            (
                "search index rows without an item",
                "SELECT count(*) FROM item_fts WHERE rowid NOT IN (SELECT seq FROM items)",
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

        let stale = crate::fts::stale_titles(conn)?;
        if stale > 0 {
            problems.push(format!("{stale} search index titles out of date"));
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
    // SQLite's own temporary files (sorting, indexes, big statements, VACUUM) can hold row
    // content outside the database file and its log: keep them in memory (2 = MEMORY).
    conn.pragma_update(None, "temp_store", 2)?;
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

/// Takes the exclusive lock on `<folder>/snipdeck.lock`, held until it is dropped.
fn acquire_lock(folder: &Path) -> Result<FolderLock> {
    let path: PathBuf = folder.join(LOCK_FILE);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(FolderLock {
            _file: file,
            folder: folder.to_owned(),
        }),
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

    /// Temporary files (sorts, indexes, large statements) hold row content: keep them in memory.
    #[test]
    fn temp_store_is_memory_on_every_open_path() {
        let temp_store = |s: &Store| -> i64 {
            s.conn
                .pragma_query_value(None, "temp_store", |r| r.get(0))
                .unwrap()
        };
        assert_eq!(temp_store(&Store::open_in_memory().unwrap()), 2);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(temp_store(&Store::open(dir.path()).unwrap()), 2);
    }

    #[test]
    fn the_sweep_keeps_a_pending_blob_however_old() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let held = vec![7u8; INLINE_LIMIT + 10];
        let snipdeck_core::Content::Blob { blob, .. } = store.blobs.put(held).unwrap() else {
            panic!("a large payload is a blob")
        };
        // Grace of zero: every file counts as old enough.
        let report = store.gc_blobs(Duration::ZERO).unwrap();
        assert_eq!(report.blobs, 0, "a pending blob is not an orphan");
        assert!(store.blobs.is_pending(&blob));
        store.blobs.attached([blob]);
        assert_eq!(
            store.gc_blobs(Duration::ZERO).unwrap().blobs,
            1,
            "now it is"
        );
    }

    #[test]
    fn a_reader_sees_the_open_database_beside_its_writer_and_cannot_write() {
        use crate::BoardRepo as _;
        use snipdeck_core::{Board, Name, Placement, Timestamp};
        let add = |store: &mut Store, name: &str| {
            let board = Board::new(Name::new(name).unwrap(), Timestamp::from_second(0).unwrap());
            store.insert_board(&board, Placement::Last).unwrap();
        };
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let reader = Store::open_reader(dir.path(), None).unwrap();
        let before = reader.boards().unwrap().len();
        add(&mut store, "Later");
        assert_eq!(
            reader.boards().unwrap().len(),
            before + 1,
            "a commit shows without reopening"
        );
        reader.begin_snapshot().unwrap();
        let held = reader.boards().unwrap().len();
        add(&mut store, "After the snapshot");
        assert_eq!(reader.boards().unwrap().len(), held, "one moment");
        assert!(reader.conn.execute("DELETE FROM boards", []).is_err());
        // The writer's lock is untouched: a reader takes none.
        assert!(matches!(Store::open(dir.path()), Err(Error::Locked { .. })));
    }

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
        for fill in [1, 2] {
            let snipdeck_core::Content::Blob { blob, .. } =
                store.put(vec![fill; INLINE_LIMIT + 1]).unwrap()
            else {
                panic!()
            };
            // Attached once, then unreferenced: an orphan, not a pending blob.
            store.blobs.attached([blob]);
        }
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
    fn a_hold_keeps_orphaned_blobs_until_it_ends() {
        let mut store = Store::open_in_memory().unwrap();
        let content = store.put(vec![4; INLINE_LIMIT + 1]).unwrap();
        let snipdeck_core::Content::Blob { blob, .. } = content else {
            panic!()
        };
        store.blobs.attached([blob]);
        store.hold_blob_release(true);
        assert_eq!(store.release_blobs(&[blob], false), 0);
        assert_eq!(store.gc_blobs(Duration::ZERO).unwrap().blobs, 0, "held");
        assert!(store.blobs.list().unwrap().0.iter().any(|b| b.blob == blob));
        assert_eq!(store.hold_blob_release(false), 0);
        assert!(store.blobs.list().unwrap().0.is_empty(), "released");
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
    fn sqlite(code: i32) -> Error {
        Error::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            None,
        ))
    }

    #[test]
    fn only_a_file_that_is_not_a_library_counts_as_damaged() {
        use rusqlite::ffi;
        for code in [ffi::SQLITE_NOTADB, ffi::SQLITE_CORRUPT] {
            assert!(is_damaged(&sqlite(code)), "{code}");
        }
        assert!(is_damaged(&Error::Corrupt("x".into())));
        assert!(is_damaged(&Error::EmptyDatabase { folder: ".".into() }));
        assert!(is_damaged(&Error::ForeignDatabase { path: ".".into() }));
        // What a healthy library meets now and then (antivirus, a sync client, a network folder).
        for code in [
            ffi::SQLITE_IOERR,
            ffi::SQLITE_IOERR_SHORT_READ,
            ffi::SQLITE_NOMEM,
            ffi::SQLITE_INTERRUPT,
            ffi::SQLITE_PROTOCOL,
            ffi::SQLITE_BUSY,
            ffi::SQLITE_LOCKED,
            ffi::SQLITE_CANTOPEN,
            ffi::SQLITE_FULL,
            ffi::SQLITE_READONLY,
            ffi::SQLITE_PERM,
        ] {
            assert!(!is_damaged(&sqlite(code)), "{code}");
        }
        assert!(!is_damaged(&Error::Io(std::io::Error::other("x"))));
        assert!(!is_damaged(&Error::Migration {
            version: 9,
            name: "n",
            reason: "r".into(),
            source: None,
        }));
        assert!(!is_damaged(&Error::SchemaTooNew {
            found: 99,
            supported: 9
        }));
        assert!(!is_damaged(&Error::Locked { folder: ".".into() }));
    }

    #[test]
    fn a_failed_migration_of_an_intact_database_is_not_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        drop(Store::open(dir.path()).unwrap());
        // Looks like version 8 but already holds version 9's changes: the migration fails.
        let raw = Connection::open(dir.path().join(DB_FILE)).unwrap();
        raw.pragma_update(None, "user_version", 8).unwrap();
        drop(raw);
        let before = std::fs::read(dir.path().join(DB_FILE)).unwrap();
        let err = Store::open_or_set_aside(dir.path(), "snipdeck.damaged-m")
            .err()
            .unwrap();
        assert!(
            matches!(err, Error::Migration { .. } | Error::Sqlite(_)),
            "{err:?}"
        );
        assert!(!dir.path().join("snipdeck.damaged-m").exists());
        assert!(!dir.path().join("snipdeck.rebuild").exists());
        assert!(dir.path().join(DB_FILE).is_file());
        assert_eq!(std::fs::read(dir.path().join(DB_FILE)).unwrap(), before);
    }

    fn damaged_folder() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DB_FILE), vec![0x42u8; 4096]).unwrap();
        dir
    }

    #[test]
    fn a_failed_fresh_open_puts_the_damaged_library_back() {
        let dir = damaged_folder();
        let err = Store::open_or_set_aside_with(dir.path(), "snipdeck.damaged-u", |_, lock| {
            Err(Box::new(OpenLockedError {
                error: Error::Io(std::io::Error::other("disk went away")),
                lock,
            }))
        })
        .err()
        .unwrap();
        assert!(matches!(err, Error::Io(_)), "{err:?}");
        assert_eq!(
            std::fs::read(dir.path().join(DB_FILE)).unwrap(),
            vec![0x42u8; 4096]
        );
        assert!(!dir.path().join("snipdeck.damaged-u").exists());
        assert!(!dir.path().join("snipdeck.rebuild").exists());
    }

    #[test]
    fn a_marker_that_cannot_be_removed_or_sealed_refuses_the_start_instead_of_looping() {
        let dir = damaged_folder();
        let err = Store::open_or_set_aside_with(dir.path(), "snipdeck.damaged-v", |f, lock| {
            let store = Store::open_locked(f, lock)?;
            // The marker becomes something that can be neither read, sealed nor removed.
            let marker = f.join("snipdeck.rebuild");
            std::fs::remove_file(&marker).unwrap();
            std::fs::create_dir(&marker).unwrap();
            Ok(store)
        })
        .err()
        .unwrap();
        assert!(matches!(err, Error::RebuildInterrupted { .. }), "{err:?}");
    }

    #[test]
    fn a_marker_is_settled_when_gone_or_sealed_and_not_when_it_would_undo_the_move() {
        let dir = damaged_folder();
        assert!(crate::aside::marker_is_settled(dir.path()));
        let fresh = "snipdeck-rebuild 1\naside=snipdeck.damaged-w\nphase=fresh\nend\n";
        std::fs::write(dir.path().join("snipdeck.rebuild"), fresh).unwrap();
        assert!(!crate::aside::marker_is_settled(dir.path()));
        let sealed = "snipdeck-rebuild 1\naside=snipdeck.damaged-w\nphase=fresh\nsealed=1/2\nend\n";
        std::fs::write(dir.path().join("snipdeck.rebuild"), sealed).unwrap();
        assert!(crate::aside::marker_is_settled(dir.path()));
        std::fs::write(dir.path().join("snipdeck.rebuild"), "junk").unwrap();
        assert!(!crate::aside::marker_is_settled(dir.path()));
    }
}
