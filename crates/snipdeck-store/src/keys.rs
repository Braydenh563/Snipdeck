//! The key ring on [`Store`] (review M6): the persisted key record, unlocking, and keeping
//! sensitive items encrypted. Item reads and writes encrypt and decrypt in `items.rs`.
//!
//! The data key lives in the `Store` while unlocked and is dropped (zeroized) by
//! [`Store::lock_key_ring`] or when the store is dropped. No OS-specific code is here: the
//! [`KeyRing`] brings a [`KeyProvider`](crate::crypto::KeyProvider) (DPAPI in milestone 2).

use rusqlite::{Connection, OptionalExtension};
use snipdeck_core::{BlobRef, Content, FormatKind, ItemId};

use crate::batch::Post;
use crate::codec::{id_bytes, id_from};
use crate::crypto::{DataKey, KeyRecord, KeyRing, Rotation, Slot, decrypt_payload};
use crate::items::{Written, load_formats, write_formats};
use crate::store::begin;
use crate::{Error, Result, Store};

/// Whether a key record is stored.
pub(crate) fn ring_exists(conn: &Connection) -> Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM key_ring WHERE id = 1", [], |_| Ok(()))
        .optional()?
        .is_some())
}

fn stored_record(conn: &Connection) -> Result<KeyRecord> {
    let bytes: Vec<u8> = conn
        .query_row("SELECT record FROM key_ring WHERE id = 1", [], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::NotFound {
            entity: "key ring",
            id: "1".to_owned(),
        })?;
    Ok(KeyRecord::from_bytes(bytes)?)
}

/// Encrypts every sensitive item whose content is still plain text (made sensitive before
/// encryption was set up), trashed ones included. Returns how many, and the blobs
/// that held their plaintext (to release and scrub after the commit).
fn encrypt_plain_sensitive(
    conn: &Connection,
    blobs: &mut crate::blobs::Blobs,
    key: &DataKey,
) -> Result<(usize, Vec<BlobRef>)> {
    let ids: Vec<Vec<u8>> = conn
        .prepare("SELECT id FROM items WHERE sensitive = 1 AND encrypted = 0")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut old = Vec::new();
    let count = ids.len();
    for raw in ids {
        let id: ItemId = id_from(&raw)?;
        let bytes = id_bytes(id);
        let plain = load_formats(conn, bytes)?;
        old.extend(blob_refs(&plain));
        // The stored blobs it consumes are already in `old`; none came from a `put`.
        write_formats(conn, blobs, id, &plain, Some(key))?;
        conn.execute("UPDATE items SET encrypted = 1 WHERE id = ?1", [bytes])?;
        // Sensitive items are indexed by title only already; nothing to change there.
    }
    Ok((count, old))
}

/// One transaction: stores the key record if given and encrypts what is still plain.
fn seal_in_transaction(
    conn: &mut Connection,
    blobs: &mut crate::blobs::Blobs,
    key: &DataKey,
    new_record: Option<&KeyRecord>,
) -> Result<(usize, Vec<BlobRef>, usize)> {
    let tx = begin(conn)?;
    if let Some(record) = new_record {
        tx.execute(
            "INSERT INTO key_ring (id, record) VALUES (1, ?1)",
            [record.as_bytes()],
        )?;
    }
    let (encrypted, old) = encrypt_plain_sensitive(&tx, blobs, key)?;
    let recent = crate::recent::encrypt_plain(&tx, key)?;
    tx.commit()?;
    Ok((encrypted, old, recent))
}

/// One transaction: re-seals every encrypted item and Recent entry from `old` to `new` and
/// replaces the key record. Returns the blobs that held old envelopes.
fn reseal_in_transaction(
    conn: &mut Connection,
    blobs: &mut crate::blobs::Blobs,
    old: &DataKey,
    new: &DataKey,
    record: &KeyRecord,
) -> Result<Vec<BlobRef>> {
    let tx = begin(conn)?;
    let ids: Vec<Vec<u8>> = tx
        .prepare("SELECT id FROM items WHERE encrypted = 1")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut stale = Vec::new();
    for raw in ids {
        let id: ItemId = id_from(&raw)?;
        let stored = load_formats(&tx, id_bytes(id))?;
        stale.extend(blob_refs(&stored));
        let plain = crate::items::decrypt_formats(blobs, old, id, &stored)?;
        write_formats(&tx, blobs, id, &plain, Some(new))?;
    }
    stale.extend(crate::recent::reseal(&tx, blobs, old, new)?);
    tx.execute(
        "UPDATE key_ring SET record = ?1, previous = NULL WHERE id = 1",
        [record.as_bytes()],
    )?;
    tx.commit()?;
    Ok(stale)
}

fn blob_refs(formats: &snipdeck_core::Formats) -> Vec<BlobRef> {
    let mut out = Vec::new();
    for format in formats {
        match format {
            snipdeck_core::Format::Files(files) => {
                out.extend(files.iter().filter_map(|f| blob_of(f.content())));
            }
            snipdeck_core::Format::Text(c)
            | snipdeck_core::Format::Html(c)
            | snipdeck_core::Format::Rtf(c)
            | snipdeck_core::Format::Png(c) => out.extend(blob_of(c)),
        }
    }
    out
}

fn blob_of(content: &Content) -> Option<BlobRef> {
    match content {
        Content::Blob { blob, .. } => Some(*blob),
        Content::Inline(_) => None,
    }
}

impl Store {
    /// Whether encryption has been set up ([`Store::create_key_ring`]).
    pub fn key_ring_exists(&self) -> Result<bool> {
        ring_exists(&self.conn)
    }

    /// Whether any sensitive item, a trashed one included, is still stored as plain text
    /// (made sensitive before encryption was set up). Creating the key ring encrypts them.
    pub fn has_plain_sensitive(&self) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM items WHERE sensitive = 1 AND encrypted = 0 LIMIT 1",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Whether any Recent inbox entry is still stored as plain text (no key was available when
    /// it was written, or it predates schema version 5). Creating or unlocking the key ring
    /// encrypts them.
    pub fn has_plain_recent(&self) -> Result<bool> {
        crate::recent::has_plain(&self.conn)
    }

    /// The stored key record (the wrapped data key), if encryption was set up. It is safe to copy:
    /// useless without the OS binding and the master password. Erasing a library uses it to keep
    /// the key while everything else goes ([`Store::restore_key_record`]).
    pub fn key_record(&self) -> Result<Option<KeyRecord>> {
        if !ring_exists(&self.conn)? {
            return Ok(None);
        }
        stored_record(&self.conn).map(Some)
    }

    /// Stores `record` as this library's key record, locked. Fails with
    /// [`Error::AlreadyExists`] if a key ring exists; nothing changes on error.
    pub fn restore_key_record(&mut self, record: &KeyRecord) -> Result<()> {
        if self.key_ring_exists()? {
            return Err(Error::AlreadyExists {
                entity: "key ring",
                id: "1".to_owned(),
            });
        }
        self.conn.execute(
            "INSERT INTO key_ring (id, record) VALUES (1, ?1)",
            [record.as_bytes()],
        )?;
        Ok(())
    }

    /// Sets up encryption of sensitive items: makes a new data key, stores the record
    /// that protects it (OS binding through `ring`'s provider, plus the master password if
    /// given), unlocks, and encrypts every sensitive item that is still in plain text.
    ///
    /// Fails with [`Error::AlreadyExists`] if a key ring exists; nothing changes on error.
    pub fn create_key_ring(&mut self, ring: &KeyRing<'_>, password: Option<&str>) -> Result<()> {
        if self.key_ring_exists()? {
            return Err(Error::AlreadyExists {
                entity: "key ring",
                id: "1".to_owned(),
            });
        }
        let (key, record) = ring.create(password)?;
        self.install_key_ring(key, &record)
    }

    /// Finishes [`create_key_ring`](Self::create_key_ring) for a key and record made elsewhere
    /// (by [`KeyRing::create`], possibly on another thread): stores `record`, unlocks with `key`
    /// and encrypts every sensitive item still in plain text.
    ///
    /// Fails with [`Error::AlreadyExists`] if a key ring exists; nothing changes on error.
    pub fn install_key_ring(&mut self, key: DataKey, record: &KeyRecord) -> Result<()> {
        if self.key_ring_exists()? {
            return Err(Error::AlreadyExists {
                entity: "key ring",
                id: "1".to_owned(),
            });
        }
        self.seal_and_unlock(key, Some(record))
    }

    /// Finishes [`unlock_key_ring`](Self::unlock_key_ring) for a key opened elsewhere (by
    /// [`KeyRing::unlock`]): unlocks with `key` and encrypts any sensitive item still in plain
    /// text. Does nothing when the library is already open.
    pub fn open_with_key(&mut self, key: DataKey) -> Result<()> {
        if self.is_unlocked() {
            return Ok(());
        }
        self.seal_and_unlock(key, None)
    }

    /// Unlocks the stored key ring for this session, so sensitive items can be read and
    /// written. Fails with [`Error::NotFound`] if there is none and
    /// [`Error::Crypto`] for a wrong or missing password or a record this OS user cannot open.
    ///
    /// Any sensitive item still in plain text is encrypted on the way.
    pub fn unlock_key_ring(&mut self, ring: &KeyRing<'_>, password: Option<&str>) -> Result<()> {
        let record = stored_record(&self.conn)?;
        let key = ring.unlock(&record, password)?;
        self.seal_and_unlock(key, None)
    }

    /// Whether the stored key needs a master password to open. `false` when there is no key
    /// ring. Opens the OS binding but not the data key, so it works while locked.
    pub fn has_master_password(&self, ring: &KeyRing<'_>) -> Result<bool> {
        if !ring_exists(&self.conn)? {
            return Ok(false);
        }
        Ok(ring.has_password(&stored_record(&self.conn)?)?)
    }

    /// Sets, changes or removes the master password (`new`; `None` removes it) and **rotates
    /// the data key**: every encrypted item and Recent entry is re-sealed under a fresh key in
    /// one transaction. `current` is the present password. Slow (key derivation); the app runs
    /// [`KeyRing::rotate`] on a worker and finishes with [`Store::rotate_key_ring`].
    pub fn change_master_password(
        &mut self,
        ring: &KeyRing<'_>,
        current: Option<&str>,
        new: Option<&str>,
    ) -> Result<()> {
        let record = stored_record(&self.conn)?;
        let rotation = ring.rotate(&record, current, new)?;
        self.rotate_key_ring(rotation)
    }

    /// Finishes a [`KeyRing::rotate`] made elsewhere: re-seals every encrypted payload from
    /// `rotation.old` to `rotation.new`, stores `rotation.record`, and leaves the library open
    /// under the new key. One transaction, so a crash leaves either the old key and record with
    /// the old payloads or the new ones, never a mix. No older key record is kept, and the
    /// blobs that held old envelopes are scrubbed.
    ///
    /// `rotation.old` must be the key the library is sealed under: the one held while unlocked
    /// or, while locked, one that opens an encrypted payload. Otherwise [`Error::Invalid`] and
    /// nothing changes. Fails with [`Error::NotFound`] if there is no key ring.
    pub fn rotate_key_ring(&mut self, rotation: Rotation) -> Result<()> {
        let Rotation { old, new, record } = rotation;
        stored_record(&self.conn)?;
        let matches = match &self.key {
            Some(held) => held.same_as(&old),
            None => self.opens_a_payload(&old)?,
        };
        if !matches {
            return Err(Error::Invalid(
                "the old key does not belong to this library".to_owned(),
            ));
        }
        let mark = self.blobs.mark();
        let resealed = reseal_in_transaction(&mut self.conn, &mut self.blobs, &old, &new, &record);
        let stale = match resealed {
            Ok(stale) => {
                self.blobs.end_log();
                stale
            }
            Err(e) => {
                self.blobs.rewind(mark);
                self.blobs.end_log();
                return Err(e);
            }
        };
        self.blobs.clear_transient();
        self.key = Some(new);
        let mut post = Post::default();
        post.written(Written::default(), stale, true);
        self.finish(post);
        Ok(())
    }

    /// Whether `key` opens one inline payload of an encrypted item; `true` when there is none
    /// to try (nothing is at risk then).
    fn opens_a_payload(&self, key: &DataKey) -> Result<bool> {
        let probe: Option<(Vec<u8>, Vec<u8>, String)> = self
            .conn
            .query_row(
                "SELECT i.id, f.inline_bytes, f.kind FROM items i
                 JOIN item_formats f ON f.item_id = i.id
                 WHERE i.encrypted = 1 AND f.kind <> 'files' AND f.inline_bytes IS NOT NULL
                 LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((id, bytes, kind)) = probe else {
            return Ok(true);
        };
        let kind: FormatKind = kind.parse()?;
        let id: ItemId = id_from(&id)?;
        Ok(decrypt_payload(key, &id, Slot::Format(kind), &bytes).is_ok())
    }

    /// Forgets the data key and every decrypted payload held in memory. Encrypted content
    /// cannot be read or written until [`Store::unlock_key_ring`] (the idle timeout of
    /// SPEC §5.7).
    pub fn lock_key_ring(&mut self) {
        self.key = None;
        self.blobs.clear_transient();
    }

    /// Whether the data key is available.
    pub fn is_unlocked(&self) -> bool {
        self.key.is_some()
    }

    /// The unlocked data key, for encrypting an export with the same key.
    pub fn data_key(&self) -> Option<&DataKey> {
        self.key.as_ref()
    }

    /// A second copy of the unlocked data key, for a library that is moved to a new database
    /// file ("Rebuild from backup") and must open it with the same key. `None` while locked.
    #[doc(hidden)]
    pub fn duplicate_data_key(&self) -> Option<DataKey> {
        self.key.as_ref().and_then(DataKey::duplicate)
    }

    /// Holds `bytes` for a new sensitive item without writing them to disk: small payloads
    /// are inline as usual, larger ones stay in memory until the item they are attached to is
    /// written (encrypted, or as plain text if the item is not sensitive after all). Use it
    /// instead of [`BlobStore::put`](crate::BlobStore::put) for sensitive captures; `put`
    /// would write the plaintext to the blob folder first.
    pub fn put_sensitive(&mut self, bytes: Vec<u8>) -> Result<Content> {
        self.blobs.put_transient(bytes)
    }

    fn seal_and_unlock(&mut self, key: DataKey, new_record: Option<&KeyRecord>) -> Result<()> {
        // The envelope blobs are files: when the transaction fails they must go with it, as in
        // `Store::transaction`.
        let mark = self.blobs.mark();
        let sealed = seal_in_transaction(&mut self.conn, &mut self.blobs, &key, new_record);
        let (encrypted, old, recent) = match sealed {
            Ok(done) => {
                self.blobs.end_log();
                done
            }
            Err(e) => {
                self.blobs.rewind(mark);
                self.blobs.end_log();
                return Err(e);
            }
        };
        self.key = Some(key);
        if encrypted > 0 || recent > 0 {
            let mut post = Post::default();
            post.written(Written::default(), old, true);
            self.finish(post);
        }
        Ok(())
    }
}
