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
use crate::crypto::{DataKey, KeyRecord, KeyRing, Slot, decrypt_payload};
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
/// encryption was set up, on Free), trashed ones included. Returns how many, and the blobs
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

    /// Sets up encryption of sensitive items (Supporter): makes a new data key, stores the record
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

    /// Re-wraps the data key under a new master password (`None` removes it); no item is
    /// re-encrypted. `current` is the present password. The old record is overwritten in the
    /// database, but a backup taken earlier still opens with the old password
    /// (`docs/crypto.md`).
    pub fn change_master_password(
        &mut self,
        ring: &KeyRing<'_>,
        current: Option<&str>,
        new: Option<&str>,
    ) -> Result<()> {
        let record = stored_record(&self.conn)?;
        let changed = ring.change_password(&record, current, new)?;
        self.replace_key_record(&changed)
    }

    /// Overwrites the stored key record with `record`, made by [`KeyRing::rekey`] (possibly on
    /// another thread) from the record [`Store::key_record`] returned, and keeps the record it
    /// replaces ([`Store::previous_key_record`]). `opened` is the data key that call returned.
    /// The store does not derive the password itself, so it checks `opened` instead: it must be
    /// the key held while unlocked or, while locked, one that opens an encrypted payload.
    /// Otherwise [`Error::Invalid`] and nothing changes (a wrong record would make every
    /// encrypted item unreadable). Fails with [`Error::NotFound`] if there is no key ring.
    /// Nothing is re-encrypted: the data key is unchanged.
    ///
    /// This does not prove `record` itself opens to `opened`; the caller made it from that key.
    pub fn replace_key_record_checked(
        &mut self,
        record: &KeyRecord,
        opened: &DataKey,
    ) -> Result<()> {
        stored_record(&self.conn)?;
        let matches = match &self.key {
            Some(held) => held.same_as(opened),
            None => self.opens_a_payload(opened)?,
        };
        if !matches {
            return Err(Error::Invalid(
                "the key record does not belong to this library's data key".to_owned(),
            ));
        }
        self.replace_key_record(record)
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

    /// The key record the last replacement overwrote, if any: the way back from a wrong one.
    pub fn previous_key_record(&self) -> Result<Option<KeyRecord>> {
        let bytes: Option<Vec<u8>> = self
            .conn
            .query_row("SELECT previous FROM key_ring WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()?
            .flatten();
        bytes.map(|b| Ok(KeyRecord::from_bytes(b)?)).transpose()
    }

    /// As [`Store::replace_key_record_checked`] without the check on the data key: the caller
    /// vouches that `record` protects the key the library is encrypted under. The old record is
    /// still kept. Prefer the checked form.
    pub fn replace_key_record(&mut self, record: &KeyRecord) -> Result<()> {
        stored_record(&self.conn)?;
        let tx = begin(&mut self.conn)?;
        tx.execute(
            "UPDATE key_ring SET previous = record, record = ?1 WHERE id = 1",
            [record.as_bytes()],
        )?;
        tx.commit()?;
        crate::store::checkpoint(&self.conn);
        Ok(())
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
