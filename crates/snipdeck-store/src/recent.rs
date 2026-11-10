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
//! # Rich entries (schema version 7)
//!
//! A row has a `kind`: `text` (the default, and every row written before version 7), `image` or
//! `files`. `text` of a non-text row is a short label ("Image 1280 x 720", "3 files: a.pdf, ...")
//! so the non-empty check and the in-memory search keep working. `rich` is a sealed envelope of
//! the extras: for text the HTML and RTF flavours, for an image its size and thumbnail, for
//! files the paths. An image's bytes are sealed with the data key **before** they reach the blob
//! folder, so `blobs/` holds only ciphertext (`blob`, `blob_len`; an image small enough to be
//! inline is kept inside `rich` instead). With no key ring images and files are not captured at
//! all, and a text entry keeps its plain text only. Copy-pasted files are never copied: only
//! their paths are kept.
//!
//! Every time-dependent call takes `now` from the caller; this module never reads the clock.
//! A Recent write leaves `ItemRepo::change_counter` alone (it never changes what the index
//! covers) and moves `Store::recent_revision`; a read moves neither.

use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{BlobRef, Content, ItemId, Timestamp};
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

/// Version 7: what an entry is (`kind`), its sealed extras (`rich`) and, for an image, the
/// blob holding the sealed picture (`blob`, `blob_len`). Existing rows are text. **Frozen**
/// once released.
pub(crate) const V7: &str = r"
ALTER TABLE recent_entries ADD COLUMN kind TEXT NOT NULL DEFAULT 'text'
    CHECK (kind IN ('text', 'image', 'files'));
ALTER TABLE recent_entries ADD COLUMN rich BLOB;
ALTER TABLE recent_entries ADD COLUMN blob TEXT;
ALTER TABLE recent_entries ADD COLUMN blob_len INTEGER NOT NULL DEFAULT 0;
";

/// The biggest total of image bytes the inbox keeps on disk.
pub const BLOB_BUDGET: u64 = 256 * 1024 * 1024;

/// What a Recent entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentKind {
    /// Copied text, possibly with HTML and RTF flavours.
    Text,
    /// A copied picture.
    Image,
    /// Copied files (their paths only).
    Files,
}

impl RecentKind {
    const fn column(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Files => "files",
        }
    }

    fn from_column(name: &str) -> Result<Self> {
        match name {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "files" => Ok(Self::Files),
            _ => Err(Error::Corrupt(
                "a recent entry has an unknown kind".to_owned(),
            )),
        }
    }
}

/// A small picture made once at capture, as raw RGBA (at most [`THUMB_MAX_BYTES`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentThumb {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// The most bytes a thumbnail may hold.
pub const THUMB_MAX_BYTES: usize = 64 * 1024;

/// A picture to keep.
#[derive(Debug, Clone, Copy)]
pub struct RecentImage<'a> {
    /// The PNG bytes.
    pub png: &'a [u8],
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// A digest of the decoded pixels, so the same picture from two PNG encoders is one entry.
    pub pixel_digest: [u8; 32],
    /// The list thumbnail.
    pub thumb: Option<&'a RecentThumb>,
    /// The searchable label stored as the row's text.
    pub label: &'a str,
}

/// What [`RecentRepo::add_recent`] stores.
#[derive(Debug, Clone, Copy)]
pub enum RecentContent<'a> {
    /// Text, with the formatted flavours when there are any.
    Text {
        /// The plain text (the duplicate key).
        text: &'a str,
        /// An HTML fragment.
        html: Option<&'a str>,
        /// A Rich Text Format document.
        rtf: Option<&'a [u8]>,
    },
    /// A picture.
    Image(RecentImage<'a>),
    /// Files, by path.
    Files {
        /// The paths (the duplicate key, sorted and case-folded).
        paths: &'a [String],
        /// The searchable label stored as the row's text.
        label: &'a str,
    },
}

impl<'a> RecentContent<'a> {
    /// Plain text with no extra flavours.
    #[must_use]
    pub const fn text(text: &'a str) -> Self {
        Self::Text {
            text,
            html: None,
            rtf: None,
        }
    }

    const fn kind(&self) -> RecentKind {
        match self {
            Self::Text { .. } => RecentKind::Text,
            Self::Image(_) => RecentKind::Image,
            Self::Files { .. } => RecentKind::Files,
        }
    }

    /// The text column and the bytes the duplicate hash covers. Text keeps the version 5 key
    /// (no tag) so earlier rows still match a re-copy.
    fn label_and_identity(&self) -> (&str, Vec<u8>) {
        match self {
            Self::Text { text, .. } => (text, text.as_bytes().to_vec()),
            Self::Image(i) => {
                let mut id = b"image\0".to_vec();
                id.extend_from_slice(&i.width.to_le_bytes());
                id.extend_from_slice(&i.height.to_le_bytes());
                id.extend_from_slice(&i.pixel_digest);
                (i.label, id)
            }
            Self::Files { paths, label } => {
                let mut folded: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
                folded.sort();
                (
                    label,
                    [b"files\0".as_slice(), folded.join("\0").as_bytes()].concat(),
                )
            }
        }
    }
}

/// The extras of an entry, read when it is used or saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecentPayload {
    /// Text flavours beyond the plain text.
    Text {
        /// An HTML fragment.
        html: Option<String>,
        /// A Rich Text Format document.
        rtf: Option<Vec<u8>>,
    },
    /// The picture's PNG bytes.
    Image(Vec<u8>),
    /// The paths of the copied files.
    Files(Vec<String>),
}

/// One captured clipboard entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentEntry {
    /// Stable until the entry is deleted or re-copied (a re-copy gets a new id).
    pub id: i64,
    /// What it is.
    pub kind: RecentKind,
    /// The copied text; a label for an image or files.
    pub text: String,
    /// An image's size in pixels.
    pub image_size: Option<(u32, u32)>,
    /// An image's list thumbnail.
    pub thumb: Option<RecentThumb>,
    /// The copied files' paths (files entries).
    pub files: Vec<String>,
    /// The executable the copy came from, when known.
    pub source_app: Option<String>,
    /// When it was last copied.
    pub copied_at: Timestamp,
}

/// The Recent inbox.
pub trait RecentRepo {
    /// Adds `content`. If the same content is already there it is replaced, so a re-copy moves
    /// the entry to the top (matched by a keyed BLAKE3, see the module docs). Returns the new
    /// entry's id.
    ///
    /// # Errors
    ///
    /// [`Error::EncryptionLocked`] while the key ring exists and is locked;
    /// [`Error::Invalid`] for an image or files with no key ring to seal them with.
    fn add_recent(
        &mut self,
        content: RecentContent<'_>,
        source_app: Option<&str>,
        now: Timestamp,
    ) -> Result<i64>;

    /// Whether images and files can be kept now (a data key is available).
    fn recent_seals_rich(&self) -> bool;

    /// The extras of entry `id` (the HTML and RTF, the picture, the paths); `None` for a
    /// missing id.
    ///
    /// # Errors
    ///
    /// [`Error::EncryptionLocked`] while locked; [`Error::Corrupt`] or a blob error when the
    /// stored extras cannot be read.
    fn recent_payload(&self, id: i64) -> Result<Option<RecentPayload>>;

    /// Deletes the oldest entries that hold image bytes until those bytes total at most
    /// `budget`. Returns how many went.
    fn prune_recent_blobs(&mut self, budget: u64) -> Result<usize>;

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

/// The duplicate key of `identity`: keyed by the data key when there is one.
fn hash(identity: &[u8], key: Option<&DataKey>) -> [u8; 32] {
    match key {
        Some(key) => *blake3::keyed_hash(&key.derive(HASH_CONTEXT), identity).as_bytes(),
        None => *blake3::hash(identity).as_bytes(),
    }
}

/// The id an envelope of the row with this hash is bound to.
fn bound_id(hash: &[u8]) -> Result<ItemId> {
    Uuid::from_slice(hash.get(..16).unwrap_or_default())
        .map(ItemId::from_uuid)
        .map_err(|_| Error::Corrupt("a recent entry hash is not 32 bytes".to_owned()))
}

fn seal_bytes(key: &DataKey, hash: &[u8], slot: Slot, plain: &[u8]) -> Result<Vec<u8>> {
    Ok(encrypt_payload(key, &bound_id(hash)?, slot, plain)?)
}

fn seal(key: &DataKey, hash: &[u8], slot: Slot, plain: &str) -> Result<Vec<u8>> {
    seal_bytes(key, hash, slot, plain.as_bytes())
}

fn open_bytes(
    key: &DataKey,
    hash: &[u8],
    slot: Slot,
    stored: &[u8],
) -> Result<zeroize::Zeroizing<Vec<u8>>> {
    Ok(decrypt_payload(key, &bound_id(hash)?, slot, stored)?)
}

fn open(key: &DataKey, hash: &[u8], slot: Slot, stored: &[u8]) -> Result<String> {
    let plain = open_bytes(key, hash, slot, stored)?;
    String::from_utf8(plain.to_vec())
        .map_err(|_| Error::Corrupt("a recent entry is not valid text".to_owned()))
}

fn plain_text(stored: Vec<u8>) -> Result<String> {
    String::from_utf8(stored)
        .map_err(|_| Error::Corrupt("a recent entry is not valid text".to_owned()))
}

/// Source apps are sealed in their own slot, so they cannot be swapped with the text.
const APP_SLOT: Slot = Slot::File(0);
/// The extras (`rich`) of a row.
const RICH_SLOT: Slot = Slot::File(1);
/// The picture of an image row.
const IMAGE_SLOT: Slot = Slot::File(2);

/// The extras of one row, before they are sealed. A flat list of `(tag, length, bytes)`
/// sections, so a reader skips what it does not know.
#[derive(Debug, Default, PartialEq, Eq)]
struct Rich {
    html: Option<String>,
    rtf: Option<Vec<u8>>,
    paths: Vec<String>,
    thumb: Option<RecentThumb>,
    size: Option<(u32, u32)>,
    /// The sealed picture, when it was small enough to be inline instead of a blob.
    inline_image: Option<Vec<u8>>,
}

const T_HTML: u8 = 1;
const T_RTF: u8 = 2;
const T_PATH: u8 = 3;
const T_THUMB: u8 = 4;
const T_SIZE: u8 = 5;
const T_INLINE: u8 = 6;

impl Rich {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn encode(&self) -> Vec<u8> {
        fn section(out: &mut Vec<u8>, tag: u8, bytes: &[u8]) {
            out.push(tag);
            out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(u32::MAX).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        let mut out = Vec::new();
        if let Some(html) = &self.html {
            section(&mut out, T_HTML, html.as_bytes());
        }
        if let Some(rtf) = &self.rtf {
            section(&mut out, T_RTF, rtf);
        }
        for path in &self.paths {
            section(&mut out, T_PATH, path.as_bytes());
        }
        if let Some(t) = &self.thumb {
            let mut body = t.width.to_le_bytes().to_vec();
            body.extend_from_slice(&t.height.to_le_bytes());
            body.extend_from_slice(&t.rgba);
            section(&mut out, T_THUMB, &body);
        }
        if let Some((w, h)) = self.size {
            let mut body = w.to_le_bytes().to_vec();
            body.extend_from_slice(&h.to_le_bytes());
            section(&mut out, T_SIZE, &body);
        }
        if let Some(inline) = &self.inline_image {
            section(&mut out, T_INLINE, inline);
        }
        out
    }

    fn decode(mut bytes: &[u8]) -> Result<Self> {
        let bad = || Error::Corrupt("a recent entry's extras are damaged".to_owned());
        let le =
            |b: &[u8]| -> Option<u32> { Some(u32::from_le_bytes(b.get(..4)?.try_into().ok()?)) };
        let mut rich = Self::default();
        while let Some((&tag, rest)) = bytes.split_first() {
            let len = le(rest).ok_or_else(bad)? as usize;
            let body = rest.get(4..4 + len).ok_or_else(bad)?;
            bytes = &rest[4 + len..];
            match tag {
                T_HTML => rich.html = Some(String::from_utf8(body.to_vec()).map_err(|_| bad())?),
                T_RTF => rich.rtf = Some(body.to_vec()),
                T_PATH => rich
                    .paths
                    .push(String::from_utf8(body.to_vec()).map_err(|_| bad())?),
                T_THUMB => {
                    let (width, height) = (
                        le(body).ok_or_else(bad)?,
                        le(&body[4.min(body.len())..]).ok_or_else(bad)?,
                    );
                    let rgba = body.get(8..).ok_or_else(bad)?.to_vec();
                    let expected = u64::from(width) * u64::from(height) * 4;
                    if rgba.len() as u64 != expected {
                        return Err(bad());
                    }
                    rich.thumb = Some(RecentThumb {
                        width,
                        height,
                        rgba,
                    });
                }
                T_SIZE => {
                    rich.size = Some((
                        le(body).ok_or_else(bad)?,
                        le(&body[4.min(body.len())..]).ok_or_else(bad)?,
                    ));
                }
                T_INLINE => rich.inline_image = Some(body.to_vec()),
                _ => {}
            }
        }
        Ok(rich)
    }
}

type Raw = (
    i64,
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    i64,
    bool,
    String,
    Option<Vec<u8>>,
);

fn rows(conn: &Connection, key: Option<&DataKey>) -> Result<Vec<RecentEntry>> {
    // Locked: the rows cannot be read, whatever state they are in.
    if key.is_none() && keys::ring_exists(conn)? {
        return Err(Error::EncryptionLocked);
    }
    let mut stmt = conn.prepare(
        "SELECT id, hash, text, source_app, copied_at, encrypted, kind, rich FROM recent_entries
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
                r.get(6)?,
                r.get(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<Raw>>>()?;
    let mut entries = Vec::with_capacity(raw.len());
    for (id, hash, text, app, at, encrypted, kind, rich) in raw {
        let opened = (|| {
            let kind = RecentKind::from_column(&kind)?;
            let (text, source_app) = if encrypted {
                let key = key.ok_or(Error::EncryptionLocked)?;
                (
                    open(key, &hash, Slot::Detached, &text)?,
                    app.map(|a| open(key, &hash, APP_SLOT, &a)).transpose()?,
                )
            } else {
                (plain_text(text)?, app.map(plain_text).transpose()?)
            };
            // The list needs the thumbnail, the size and the paths; the picture itself and the
            // text flavours are read only when an entry is used.
            let extras = match (kind, &rich) {
                (RecentKind::Text, _) => Rich::default(),
                (_, Some(rich)) if encrypted => {
                    let key = key.ok_or(Error::EncryptionLocked)?;
                    Rich::decode(&open_bytes(key, &hash, RICH_SLOT, rich)?)?
                }
                _ => return Err(Error::Corrupt("a rich entry has no extras".to_owned())),
            };
            Ok::<_, Error>(RecentEntry {
                id,
                kind,
                text,
                image_size: extras.size,
                thumb: extras.thumb,
                files: extras.paths,
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
            let new_hash = hash(text.as_bytes(), Some(key));
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
    fn add_recent(
        &mut self,
        content: RecentContent<'_>,
        source_app: Option<&str>,
        now: Timestamp,
    ) -> Result<i64> {
        self.quietly(|s| s.add_recent_inner(&content, source_app, now))
    }

    fn recent_seals_rich(&self) -> bool {
        self.key.is_some()
    }

    fn recent_payload(&self, id: i64) -> Result<Option<RecentPayload>> {
        self.recent_payload_inner(id)
    }

    fn prune_recent_blobs(&mut self, budget: u64) -> Result<usize> {
        self.quietly(|s| s.prune_recent_blobs_inner(budget))
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
            let blobs = blobs_of(
                &tx,
                "SELECT blob FROM recent_entries WHERE id = ?1 AND blob IS NOT NULL",
                params![id],
            )?;
            tx.execute("DELETE FROM recent_entries WHERE id = ?1", params![id])?;
            tx.commit()?;
            s.release_blobs(&blobs, false);
            Ok(())
        })
    }

    fn clear_recent(&mut self) -> Result<usize> {
        self.quietly(|s| {
            let tx = begin(&mut s.conn)?;
            let blobs = blobs_of(
                &tx,
                "SELECT blob FROM recent_entries WHERE blob IS NOT NULL",
                [],
            )?;
            let n = tx.execute("DELETE FROM recent_entries", [])?;
            tx.commit()?;
            s.release_blobs(&blobs, false);
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

/// The blobs named by the rows a query selects (its one column is a `BlobRef` text).
pub(crate) fn blobs_of(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<BlobRef>> {
    let mut stmt = conn.prepare_cached(sql)?;
    let texts = stmt
        .query_map(params, |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    texts
        .iter()
        .map(|t| {
            t.parse::<BlobRef>()
                .map_err(|_| Error::Corrupt("a recent entry's blob is not a hash".to_owned()))
        })
        .collect()
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
        content: &RecentContent<'_>,
        source_app: Option<&str>,
        now: Timestamp,
    ) -> Result<i64> {
        let at = nanos(now)?;
        let key = self.key.as_ref();
        if key.is_none() && keys::ring_exists(&self.conn)? {
            return Err(Error::EncryptionLocked);
        }
        let kind = content.kind();
        if kind != RecentKind::Text && key.is_none() {
            return Err(Error::Invalid(
                "an image or files in Recent need the key ring to be sealed".to_owned(),
            ));
        }
        let (label, identity) = content.label_and_identity();
        let h = hash(&identity, key);
        let mut rich = Rich::default();
        let mut blob: Option<(BlobRef, u64)> = None;
        match content {
            RecentContent::Text { html, rtf, .. } => {
                // Formatted flavours are only ever kept sealed.
                if key.is_some() {
                    rich.html = html.map(str::to_owned);
                    rich.rtf = rtf.map(<[u8]>::to_vec);
                }
            }
            RecentContent::Image(image) => {
                rich.size = Some((image.width, image.height));
                rich.thumb = image.thumb.cloned();
                if let Some(key) = key {
                    let sealed = seal_bytes(key, &h, IMAGE_SLOT, image.png)?;
                    match self.blobs.put(sealed)? {
                        Content::Blob { blob: b, len } => blob = Some((b, len)),
                        Content::Inline(inline) => {
                            rich.inline_image = Some(inline.as_bytes().to_vec());
                        }
                    }
                }
            }
            RecentContent::Files { paths, .. } => rich.paths = paths.to_vec(),
        }
        let (stored, app, extras) = match key {
            Some(key) => (
                seal(key, &h, Slot::Detached, label)?,
                source_app.map(|a| seal(key, &h, APP_SLOT, a)).transpose()?,
                if rich.is_empty() {
                    None
                } else {
                    Some(seal_bytes(key, &h, RICH_SLOT, &rich.encode())?)
                },
            ),
            None => (
                label.as_bytes().to_vec(),
                source_app.map(|a| a.as_bytes().to_vec()),
                None,
            ),
        };
        let tx = begin(&mut self.conn)?;
        let replaced = blobs_of(
            &tx,
            "SELECT blob FROM recent_entries WHERE hash = ?1 AND blob IS NOT NULL",
            params![&h[..]],
        )?;
        tx.execute(
            "DELETE FROM recent_entries WHERE hash = ?1",
            params![&h[..]],
        )?;
        tx.execute(
            "INSERT INTO recent_entries
                 (hash, text, source_app, copied_at, encrypted, kind, rich, blob, blob_len)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &h[..],
                stored,
                app,
                at,
                i64::from(key.is_some()),
                kind.column(),
                extras,
                blob.map(|(b, _)| b.to_string()),
                blob.map_or(0, |(_, len)| i64::try_from(len).unwrap_or(i64::MAX)),
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        if let Some((b, _)) = blob {
            self.blobs.attached([b]);
        }
        // A re-copy replaces the row; its old picture was sealed with another nonce, so it is a
        // different blob and goes now.
        self.release_blobs(&replaced, false);
        Ok(id)
    }

    fn recent_payload_inner(&self, id: i64) -> Result<Option<RecentPayload>> {
        let key = self.key.as_ref();
        if key.is_none() && keys::ring_exists(&self.conn)? {
            return Err(Error::EncryptionLocked);
        }
        type Stored = (Vec<u8>, bool, String, Option<Vec<u8>>, Option<String>, i64);
        let row: Option<Stored> = self
            .conn
            .query_row(
                "SELECT hash, encrypted, kind, rich, blob, blob_len FROM recent_entries
                 WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get::<_, i64>(1)? == 1,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((hash, encrypted, kind, rich, blob, blob_len)) = row else {
            return Ok(None);
        };
        let kind = RecentKind::from_column(&kind)?;
        let extras = match (&rich, key) {
            (Some(rich), Some(key)) if encrypted => {
                Rich::decode(&open_bytes(key, &hash, RICH_SLOT, rich)?)?
            }
            (None, _) if kind == RecentKind::Text => Rich::default(),
            (_, None) => return Err(Error::EncryptionLocked),
            _ => return Err(Error::Corrupt("a rich entry has no extras".to_owned())),
        };
        Ok(Some(match kind {
            RecentKind::Text => RecentPayload::Text {
                html: extras.html,
                rtf: extras.rtf,
            },
            RecentKind::Files => RecentPayload::Files(extras.paths),
            RecentKind::Image => {
                let key = key.ok_or(Error::EncryptionLocked)?;
                let sealed = match (blob, extras.inline_image) {
                    (Some(blob), _) => {
                        let blob = blob.parse::<BlobRef>().map_err(|_| {
                            Error::Corrupt("a recent entry's blob is not a hash".to_owned())
                        })?;
                        let len = u64::try_from(blob_len).unwrap_or(0);
                        self.blobs.read(&Content::Blob { blob, len })?.into_owned()
                    }
                    (None, Some(inline)) => inline,
                    (None, None) => {
                        return Err(Error::Corrupt("an image entry has no picture".to_owned()));
                    }
                };
                RecentPayload::Image(open_bytes(key, &hash, IMAGE_SLOT, &sealed)?.to_vec())
            }
        }))
    }

    fn prune_recent_blobs_inner(&mut self, budget: u64) -> Result<usize> {
        let tx = begin(&mut self.conn)?;
        let held: Vec<(i64, String, i64)> = tx
            .prepare(
                "SELECT id, blob, blob_len FROM recent_entries WHERE blob IS NOT NULL
                 ORDER BY copied_at ASC, id ASC",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut total: u64 = held
            .iter()
            .map(|(_, _, len)| u64::try_from(*len).unwrap_or(0))
            .sum();
        let mut gone = Vec::new();
        for (id, blob, len) in &held {
            if total <= budget {
                break;
            }
            tx.execute("DELETE FROM recent_entries WHERE id = ?1", params![id])?;
            total = total.saturating_sub(u64::try_from(*len).unwrap_or(0));
            if let Ok(b) = blob.parse::<BlobRef>() {
                gone.push(b);
            }
        }
        tx.commit()?;
        self.release_blobs(&gone, false);
        Ok(gone.len())
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
        let blobs = blobs_of(
            &tx,
            "SELECT blob FROM recent_entries WHERE blob IS NOT NULL AND (copied_at < ?1 OR id NOT IN
               (SELECT id FROM recent_entries ORDER BY copied_at DESC, id DESC LIMIT ?2))",
            params![cutoff, keep],
        )?;
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
        self.release_blobs(&blobs, false);
        Ok(old + extra)
    }
}
