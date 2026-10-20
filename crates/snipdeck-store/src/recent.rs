//! [`RecentRepo`] for [`Store`]: the opt-in Recent inbox (SPEC §5.6).
//!
//! Entries live in their own table, outside the library: they are not items, are never
//! indexed by the full-text search, and are not in backups of the library views. Text is kept
//! in the row (the app caps it at 64 KiB before it gets here) rather than in the blob folder,
//! because it is short-lived and a blob folder file per clipboard change would be churn.
//!
//! # Encryption at rest (schema version 5)
//!
//! Copied text and the source app are sealed with the same data key and `SNENC2` envelope as
//! sensitive items, so the rules are the same:
//!
//! * No key ring yet: rows are stored as plain text, flagged `encrypted = 0`. The app creates
//!   the ring (OS key protection, no password needed) before it records anything, so this is
//!   the state of a machine without key protection, and of rows written by version 4.
//! * Key ring unlocked: new rows are sealed. Rows still plain are sealed the moment the key
//!   first becomes available ([`Store::create_key_ring`], [`Store::unlock_key_ring`]); with a
//!   master password that is the first unlock, since the key does not exist before it. The
//!   migration itself never needs the key, so it cannot lose a row.
//! * Key ring locked: reads and writes fail with [`Error::EncryptionLocked`]; delete, clear
//!   and prune need no key and still work.
//!
//! The duplicate-detection hash is a keyed hash once a key is available (a plain BLAKE3 of the
//! text would let a stolen file confirm a guess). Each envelope is bound to its row's hash, so
//! a ciphertext copied onto another row fails to open. The inbox is never searched in SQL: the
//! app filters after decrypting, in memory. `secure_delete` is on, so replaced and deleted
//! rows are zeroed on disk.
//!
//! Every time-dependent call takes `now` from the caller; this module never reads the clock.
//! A Recent write leaves `ItemRepo::change_counter` alone (it never changes what the index
//! covers) and moves `Store::recent_revision`; a read moves neither.

use std::time::Duration;

use rusqlite::{Connection, params};
use snipdeck_core::{ItemId, Timestamp};
use uuid::Uuid;

use crate::codec::{from_nanos, nanos};
use crate::crypto::{DataKey, Slot, decrypt_payload, encrypt_payload};
use crate::store::begin;
use crate::{Error, Result, Store, keys};

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

/// Version 5: text and source app become BLOBs that can hold an envelope, and `encrypted` says
/// which rows do. Existing rows are copied over as plain; they are sealed when a key is first
/// available (see the module docs), because a migration has no key.
pub(crate) const V5: &str = r"
CREATE TABLE recent_entries_v5 (
    id         INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    hash       BLOB    NOT NULL UNIQUE CHECK (length(hash) = 32),
    text       BLOB    NOT NULL CHECK (length(text) > 0),
    source_app BLOB,
    copied_at  INTEGER NOT NULL,
    encrypted  INTEGER NOT NULL DEFAULT 0 CHECK (encrypted IN (0, 1))
) STRICT;
INSERT INTO recent_entries_v5 (id, hash, text, source_app, copied_at, encrypted)
    SELECT id, hash, CAST(text AS BLOB), CAST(source_app AS BLOB), copied_at, 0
    FROM recent_entries;
DROP TABLE recent_entries;
ALTER TABLE recent_entries_v5 RENAME TO recent_entries;
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
    /// entry to the top (matched by a keyed BLAKE3 of the text). Returns the new entry's id.
    ///
    /// # Errors
    ///
    /// [`Error::EncryptionLocked`] while the key ring exists and is locked.
    fn add_recent(&mut self, text: &str, source_app: Option<&str>, now: Timestamp) -> Result<i64>;

    /// Every entry, newest first.
    ///
    /// # Errors
    ///
    /// [`Error::EncryptionLocked`] while the key ring exists and is locked.
    fn recent(&self) -> Result<Vec<RecentEntry>>;

    /// A number that grows with every change to the library, Recent writes included: where
    /// [`ItemRepo::change_counter`](crate::ItemRepo::change_counter) ignores the inbox and
    /// counted uses, this moves on any write, so a cache of the Recent list can tell it is
    /// stale. Comparable only with values from the same open handle.
    fn recent_revision(&self) -> u64;

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

const HASH_CONTEXT: &str = "snipdeck recent inbox duplicate hash v1";

/// The duplicate key of `text`: keyed by the data key when there is one.
fn hash(text: &str, key: Option<&DataKey>) -> [u8; 32] {
    match key {
        Some(key) => *blake3::keyed_hash(&key.derive(HASH_CONTEXT), text.as_bytes()).as_bytes(),
        None => *blake3::hash(text.as_bytes()).as_bytes(),
    }
}

/// The id an envelope of the row with this hash is bound to.
fn bound_id(hash: &[u8]) -> Result<ItemId> {
    Uuid::from_slice(hash.get(..16).unwrap_or_default())
        .map(ItemId::from_uuid)
        .map_err(|_| Error::Corrupt("a recent entry hash is not 32 bytes".to_owned()))
}

fn seal(key: &DataKey, hash: &[u8], slot: Slot, plain: &str) -> Result<Vec<u8>> {
    Ok(encrypt_payload(
        key,
        &bound_id(hash)?,
        slot,
        plain.as_bytes(),
    )?)
}

fn open(key: &DataKey, hash: &[u8], slot: Slot, stored: &[u8]) -> Result<String> {
    let plain = decrypt_payload(key, &bound_id(hash)?, slot, stored)?;
    String::from_utf8(plain.to_vec())
        .map_err(|_| Error::Corrupt("a recent entry is not valid text".to_owned()))
}

fn plain_text(stored: Vec<u8>) -> Result<String> {
    String::from_utf8(stored)
        .map_err(|_| Error::Corrupt("a recent entry is not valid text".to_owned()))
}

/// Source apps are sealed in their own slot, so they cannot be swapped with the text.
const APP_SLOT: Slot = Slot::File(0);

type Raw = (i64, Vec<u8>, Vec<u8>, Option<Vec<u8>>, i64, bool);

fn rows(conn: &Connection, key: Option<&DataKey>) -> Result<Vec<RecentEntry>> {
    // Locked: the rows cannot be read, whatever state they are in.
    if key.is_none() && keys::ring_exists(conn)? {
        return Err(Error::EncryptionLocked);
    }
    let mut stmt = conn.prepare(
        "SELECT id, hash, text, source_app, copied_at, encrypted FROM recent_entries
         ORDER BY copied_at DESC, id DESC",
    )?;
    let raw = stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get::<_, i64>(5)? == 1,
            ))
        })?
        .collect::<rusqlite::Result<Vec<Raw>>>()?;
    let mut entries = Vec::with_capacity(raw.len());
    for (id, hash, text, app, at, encrypted) in raw {
        let opened = (|| {
            let (text, source_app) = if encrypted {
                let key = key.ok_or(Error::EncryptionLocked)?;
                (
                    open(key, &hash, Slot::Detached, &text)?,
                    app.map(|a| open(key, &hash, APP_SLOT, &a)).transpose()?,
                )
            } else {
                (plain_text(text)?, app.map(plain_text).transpose()?)
            };
            Ok::<_, Error>(RecentEntry {
                id,
                text,
                source_app,
                copied_at: from_nanos(at)?,
            })
        })();
        match opened {
            Ok(entry) => entries.push(entry),
            // Locked is not damage: the whole list is unavailable, not one row.
            Err(Error::EncryptionLocked) => return Err(Error::EncryptionLocked),
            // One damaged row must not hide the inbox: it is skipped (and left for pruning).
            Err(_) => {}
        }
    }
    Ok(entries)
}

/// Whether any row is still plain text.
pub(crate) fn has_plain(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM recent_entries WHERE encrypted = 0)",
        [],
        |r| r.get(0),
    )?)
}

/// Seals every plain row with `key` (and re-keys its duplicate hash). Returns how many. A row
/// that cannot be sealed is deleted (not counted).
/// Idempotent: sealed rows are not touched.
pub(crate) fn encrypt_plain(conn: &Connection, key: &DataKey) -> Result<usize> {
    let plain: Vec<(i64, Vec<u8>, Option<Vec<u8>>)> = conn
        .prepare("SELECT id, text, source_app FROM recent_entries WHERE encrypted = 0")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut sealed_rows = 0;
    for (id, text, app) in plain {
        let sealed = (|| {
            let text = plain_text(text)?;
            let new_hash = hash(&text, Some(key));
            let sealed = seal(key, &new_hash, Slot::Detached, &text)?;
            let app = app
                .map(|a| seal(key, &new_hash, APP_SLOT, &plain_text(a)?))
                .transpose()?;
            conn.execute(
                "UPDATE recent_entries SET hash = ?1, text = ?2, source_app = ?3, encrypted = 1
                 WHERE id = ?4",
                params![&new_hash[..], sealed, app, id],
            )?;
            Ok::<_, Error>(())
        })();
        if sealed.is_ok() {
            sealed_rows += 1;
        } else {
            // A convenience row that cannot be sealed (damaged text, a clash) is deleted rather
            // than left plain or allowed to keep the library locked.
            conn.execute("DELETE FROM recent_entries WHERE id = ?1", params![id])?;
        }
    }
    Ok(sealed_rows)
}

impl RecentRepo for Store {
    fn add_recent(&mut self, text: &str, source_app: Option<&str>, now: Timestamp) -> Result<i64> {
        self.quietly(|s| s.add_recent_inner(text, source_app, now))
    }

    fn recent_revision(&self) -> u64 {
        self.conn.total_changes()
    }

    fn recent(&self) -> Result<Vec<RecentEntry>> {
        rows(&self.conn, self.key.as_ref())
    }

    fn delete_recent(&mut self, id: i64) -> Result<()> {
        self.quietly(|s| {
            let tx = begin(&mut s.conn)?;
            tx.execute("DELETE FROM recent_entries WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(())
        })
    }

    fn clear_recent(&mut self) -> Result<usize> {
        self.quietly(|s| {
            let tx = begin(&mut s.conn)?;
            let n = tx.execute("DELETE FROM recent_entries", [])?;
            tx.commit()?;
            Ok(n)
        })
    }

    fn prune_recent(
        &mut self,
        now: Timestamp,
        max_entries: usize,
        max_age: Duration,
    ) -> Result<usize> {
        self.quietly(|s| s.prune_recent_inner(now, max_entries, max_age))
    }
}

impl Store {
    /// Runs a write that must not move `change_counter` (see [`Store::quiet`]).
    fn quietly<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let before = self.conn.total_changes();
        let out = f(self);
        let delta = self.conn.total_changes() - before;
        self.quiet.all.set(self.quiet.all.get() + delta);
        out
    }

    fn add_recent_inner(
        &mut self,
        text: &str,
        source_app: Option<&str>,
        now: Timestamp,
    ) -> Result<i64> {
        let at = nanos(now)?;
        let key = self.key.as_ref();
        if key.is_none() && keys::ring_exists(&self.conn)? {
            return Err(Error::EncryptionLocked);
        }
        let h = hash(text, key);
        let (stored, app) = match key {
            Some(key) => (
                seal(key, &h, Slot::Detached, text)?,
                source_app.map(|a| seal(key, &h, APP_SLOT, a)).transpose()?,
            ),
            None => (
                text.as_bytes().to_vec(),
                source_app.map(|a| a.as_bytes().to_vec()),
            ),
        };
        let tx = begin(&mut self.conn)?;
        tx.execute(
            "DELETE FROM recent_entries WHERE hash = ?1",
            params![&h[..]],
        )?;
        tx.execute(
            "INSERT INTO recent_entries (hash, text, source_app, copied_at, encrypted)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&h[..], stored, app, at, i64::from(key.is_some())],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    fn prune_recent_inner(
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
