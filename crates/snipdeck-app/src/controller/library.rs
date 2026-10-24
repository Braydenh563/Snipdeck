//! The library bound the controller works over, and its implementation for the real store.

use snipdeck_store::{
    Batch, BlobStore, BoardRepo, ItemRepo, ListRepo, RecentRepo, Store, TagRepo, TrashRepo,
    crypto::{DataKey, KeyRecord, KeyRing},
};

/// What the controller needs from the library: every repository trait in one bound, and a way
/// to make several writes land together (or not at all).
pub trait Library:
    BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo + BlobStore + RecentRepo
{
    /// Runs `f` in one store transaction: what it does through the [`Batch`] commits together
    /// when `f` returns `Ok`, and none of it happens when it returns `Err`.
    ///
    /// # Errors
    ///
    /// What `f` returns, or the store failing to begin or commit.
    fn atomically<T>(
        &mut self,
        f: impl FnOnce(&mut Batch<'_>) -> snipdeck_store::Result<T>,
    ) -> snipdeck_store::Result<T>;

    /// Whether encryption of sensitive items has been set up.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn key_ring_exists(&self) -> snipdeck_store::Result<bool>;

    /// Whether any sensitive item, a trashed one included, is stored as plain text (it was made
    /// sensitive before a key existed). Setting up encryption encrypts it.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn has_plain_sensitive(&self) -> snipdeck_store::Result<bool>;

    /// Whether any Recent inbox entry is stored as plain text (kept before a key existed, or by
    /// version 4). Setting up encryption encrypts it.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn has_plain_recent(&self) -> snipdeck_store::Result<bool>;

    /// Sets up encryption, with a master password if `password` is `Some`, and leaves the library
    /// unlocked.
    ///
    /// # Errors
    ///
    /// The key could not be made or wrapped, or the store failed.
    fn create_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()>;

    /// Opens the stored key. `password` is the master password, `None` when none is set.
    ///
    /// # Errors
    ///
    /// The key does not open (a wrong or missing password, another user's key), or the store
    /// failed.
    fn unlock_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()>;

    /// Whether the stored key needs a master password (`false` with no key).
    ///
    /// # Errors
    ///
    /// The OS binding rejected the record, or the store failed.
    fn has_master_password(&self, ring: &KeyRing<'_>) -> snipdeck_store::Result<bool>;

    /// The stored key record, if encryption was set up. A master password step re-wraps a copy of
    /// it elsewhere ([`crate::master_job::MasterJob`]).
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn key_record(&self) -> snipdeck_store::Result<Option<KeyRecord>>;

    /// Finishes setting up encryption with a key and record made elsewhere, and leaves the library
    /// unlocked.
    ///
    /// # Errors
    ///
    /// A key ring exists already, or the store failed. Nothing changes on error.
    fn install_key_ring(&mut self, key: DataKey, record: &KeyRecord) -> snipdeck_store::Result<()>;

    /// Opens the library with a key opened elsewhere. Does nothing when it is open.
    ///
    /// # Errors
    ///
    /// The store failed.
    fn open_with_key(&mut self, key: DataKey) -> snipdeck_store::Result<()>;

    /// Stores a key record re-wrapped elsewhere (a changed or removed master password).
    ///
    /// # Errors
    ///
    /// There is no key ring, or the store failed. Nothing changes on error.
    fn replace_key_record(&mut self, record: &KeyRecord) -> snipdeck_store::Result<()>;

    /// Forgets the key and every decrypted payload held in memory (the idle lock).
    fn lock_key_ring(&mut self);

    /// Whether the key is open.
    fn is_unlocked(&self) -> bool;
}

impl Library for Store {
    fn atomically<T>(
        &mut self,
        f: impl FnOnce(&mut Batch<'_>) -> snipdeck_store::Result<T>,
    ) -> snipdeck_store::Result<T> {
        self.transaction(f)
    }

    fn key_ring_exists(&self) -> snipdeck_store::Result<bool> {
        Store::key_ring_exists(self)
    }

    fn has_plain_sensitive(&self) -> snipdeck_store::Result<bool> {
        Store::has_plain_sensitive(self)
    }

    fn has_plain_recent(&self) -> snipdeck_store::Result<bool> {
        Store::has_plain_recent(self)
    }

    fn create_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()> {
        Store::create_key_ring(self, ring, password)
    }

    fn unlock_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()> {
        Store::unlock_key_ring(self, ring, password)
    }

    fn has_master_password(&self, ring: &KeyRing<'_>) -> snipdeck_store::Result<bool> {
        Store::has_master_password(self, ring)
    }

    fn key_record(&self) -> snipdeck_store::Result<Option<KeyRecord>> {
        Store::key_record(self)
    }

    fn install_key_ring(&mut self, key: DataKey, record: &KeyRecord) -> snipdeck_store::Result<()> {
        Store::install_key_ring(self, key, record)
    }

    fn open_with_key(&mut self, key: DataKey) -> snipdeck_store::Result<()> {
        Store::open_with_key(self, key)
    }

    fn replace_key_record(&mut self, record: &KeyRecord) -> snipdeck_store::Result<()> {
        Store::replace_key_record(self, record)
    }

    fn lock_key_ring(&mut self) {
        Store::lock_key_ring(self);
    }

    fn is_unlocked(&self) -> bool {
        Store::is_unlocked(self)
    }
}
