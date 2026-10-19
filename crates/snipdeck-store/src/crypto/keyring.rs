//! Creating, unlocking and re-wrapping the data key.
//!
//! # `SNKEY1` key record
//!
//! ```text
//! "SNKEY1" (6) | KeyProvider blob of the inner plaintext
//!
//! inner, mode 0 (OS binding only):   0x00 | DEK (32)
//! inner, mode 1 (+ master password): 0x01 | m_cost KiB (u32 LE) | t_cost (u32 LE) | p_cost (u32 LE)
//!                                    | salt (16) | nonce (24) | AEAD(DEK) (32 + tag 16)
//! ```
//!
//! The mode byte and the Argon2 parameters sit **inside** the provider blob, so they are
//! authenticated by it. In mode 1 the DEK is sealed with XChaCha20-Poly1305 under
//! `Argon2id(password, salt)`, with the mode byte, parameters and salt as associated data.
//! The AEAD tag is also the password verifier: a wrong password fails to open the DEK and is
//! reported as [`CryptoError::WrongPassword`] without any separate hash to attack.
//!
//! Changing the password re-seals the same DEK under a new salt, so no item is re-encrypted.
//! It does not rotate the DEK: someone who kept a copy of an old record and knows the old
//! password can still open that old record (re-wrapping is not revocation).

use zeroize::Zeroizing;

use super::aead::{NONCE_LEN, TAG_LEN, open, seal};
use super::key::{DataKey, KEY_LEN, random};
use super::{CryptoError, KeyProvider};

const MAGIC: &[u8; 6] = b"SNKEY1";
const MODE_OS_ONLY: u8 = 0;
const MODE_PASSWORD: u8 = 1;
const SALT_LEN: usize = 16;
/// mode byte + three u32 + salt: the part of the inner plaintext that is also AAD.
const HEADER_LEN: usize = 1 + 12 + SALT_LEN;
const OS_ONLY_INNER_LEN: usize = 1 + KEY_LEN;
const PASSWORD_INNER_LEN: usize = HEADER_LEN + NONCE_LEN + KEY_LEN + TAG_LEN;

/// Argon2id cost settings for the master password.
///
/// [`KdfParams::DEFAULT`] is what the app uses. Nothing below [`KdfParams::MIN_M_COST_KIB`]
/// and [`KdfParams::MIN_T_COST`] (OWASP's minimum) can be built or loaded in a production
/// build (review L6); cheaper settings exist only with the `testing` feature. The values are
/// stored in the key record, so records made with older settings keep unlocking after the
/// default changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
}

impl KdfParams {
    /// 64 MiB, 3 passes, 1 lane (about 0.3 s on a modern desktop; see `docs/crypto.md`).
    ///
    /// This is the RFC 9106 §4 second recommended option with one lane, which is the
    /// memory-constrained profile for software without a guaranteed number of cores. It
    /// is twice the memory and one more pass than OWASP's minimum (19 MiB, 2, 1).
    pub const DEFAULT: Self = Self {
        m_cost_kib: 64 * 1024,
        t_cost: 3,
        p_cost: 1,
    };

    /// Most memory a stored record may demand when unlocking (1 GiB): a damaged or hostile
    /// record must not be able to exhaust memory.
    pub const MAX_M_COST_KIB: u32 = 1024 * 1024;
    /// Most passes a stored record may demand.
    pub const MAX_T_COST: u32 = 16;
    /// Most lanes a stored record may demand.
    pub const MAX_P_COST: u32 = 16;
    /// Least memory accepted in a production build: 19 MiB, OWASP's minimum for Argon2id.
    pub const MIN_M_COST_KIB: u32 = 19 * 1024;
    /// Fewest passes accepted in a production build (OWASP's minimum).
    pub const MIN_T_COST: u32 = 2;

    /// Custom costs, at least as strong as the production floor.
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidKdfParams`] if a value is below [`KdfParams::MIN_M_COST_KIB`],
    /// [`KdfParams::MIN_T_COST`] or 1 lane, or above the `MAX_*` limits.
    pub const fn new(m_cost_kib: u32, t_cost: u32, p_cost: u32) -> Result<Self, CryptoError> {
        Self::check(m_cost_kib, t_cost, p_cost, PRODUCTION_FLOOR)
    }

    /// Costs below the production floor, down to Argon2's own minimum (8 KiB per lane, 1
    /// pass): only for tests, where the default would make every unlock take seconds.
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidKdfParams`] outside Argon2's range or the `MAX_*` limits.
    #[cfg(any(test, feature = "testing"))]
    pub const fn insecure_for_tests(
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
    ) -> Result<Self, CryptoError> {
        Self::check(m_cost_kib, t_cost, p_cost, ARGON2_FLOOR)
    }

    /// The costs read from a stored record: the production floor applies, except in test
    /// builds, so a record cannot ask for cheap hashing.
    const fn stored(m_cost_kib: u32, t_cost: u32, p_cost: u32) -> Result<Self, CryptoError> {
        #[cfg(any(test, feature = "testing"))]
        let floor = ARGON2_FLOOR;
        #[cfg(not(any(test, feature = "testing")))]
        let floor = PRODUCTION_FLOOR;
        Self::check(m_cost_kib, t_cost, p_cost, floor)
    }

    const fn check(
        m_cost_kib: u32,
        t_cost: u32,
        p_cost: u32,
        (min_m_cost_kib, min_t_cost): (u32, u32),
    ) -> Result<Self, CryptoError> {
        if p_cost < 1
            || p_cost > Self::MAX_P_COST
            || t_cost < min_t_cost
            || t_cost > Self::MAX_T_COST
            || m_cost_kib < 8 * p_cost
            || m_cost_kib < min_m_cost_kib
            || m_cost_kib > Self::MAX_M_COST_KIB
        {
            return Err(CryptoError::InvalidKdfParams);
        }
        Ok(Self {
            m_cost_kib,
            t_cost,
            p_cost,
        })
    }

    /// Memory in KiB.
    pub const fn m_cost_kib(self) -> u32 {
        self.m_cost_kib
    }

    /// Number of passes.
    pub const fn t_cost(self) -> u32 {
        self.t_cost
    }

    /// Number of lanes.
    pub const fn p_cost(self) -> u32 {
        self.p_cost
    }

    fn derive(
        self,
        password: &str,
        salt: &[u8; SALT_LEN],
    ) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
        use argon2::{Algorithm, Argon2, Params, Version};
        let params = Params::new(self.m_cost_kib, self.t_cost, self.p_cost, Some(KEY_LEN))
            .map_err(|_| CryptoError::InvalidKdfParams)?;
        let mut kek = Zeroizing::new([0_u8; KEY_LEN]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), salt, kek.as_mut_slice())
            .map_err(|_| CryptoError::Primitive)?;
        Ok(kek)
    }
}

/// Least memory and passes for settings that protect real data.
const PRODUCTION_FLOOR: (u32, u32) = (KdfParams::MIN_M_COST_KIB, KdfParams::MIN_T_COST);
/// Argon2's own minimum (the 8 KiB-per-lane rule is checked separately).
#[cfg(any(test, feature = "testing"))]
const ARGON2_FLOOR: (u32, u32) = (8, 1);

/// The persisted, wrapped data key: opaque bytes, stored by
/// [`Store::create_key_ring`](crate::Store::create_key_ring) in the `key_ring` table. Safe to store and to back up: it is useless without the OS binding and,
/// if set, the master password. Contains no plaintext key.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyRecord(Vec<u8>);

impl KeyRecord {
    /// Reads a record back from storage.
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidKeyRecord`] if the bytes do not start with the `SNKEY1` header.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, CryptoError> {
        if bytes.len() > MAGIC.len() && bytes.starts_with(MAGIC) {
            Ok(Self(bytes))
        } else {
            Err(CryptoError::InvalidKeyRecord)
        }
    }

    /// The bytes to store.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl core::fmt::Debug for KeyRecord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "KeyRecord({} bytes)", self.0.len())
    }
}

/// Creates, unlocks and re-wraps the [`DataKey`] using a [`KeyProvider`].
pub struct KeyRing<'p> {
    provider: &'p dyn KeyProvider,
    kdf: KdfParams,
}

impl core::fmt::Debug for KeyRing<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KeyRing")
            .field("kdf", &self.kdf)
            .finish_non_exhaustive()
    }
}

impl<'p> KeyRing<'p> {
    /// A key ring on `provider` using [`KdfParams::DEFAULT`] for new passwords.
    pub fn new(provider: &'p dyn KeyProvider) -> Self {
        Self {
            provider,
            kdf: KdfParams::DEFAULT,
        }
    }

    /// Uses `kdf` for passwords set from now on. Existing records keep their own settings.
    #[must_use]
    pub fn with_kdf_params(mut self, kdf: KdfParams) -> Self {
        self.kdf = kdf;
        self
    }

    /// Makes a new random data key and the record that protects it, with a master password if
    /// `password` is `Some`.
    ///
    /// # Errors
    ///
    /// [`CryptoError::EmptyPassword`]; [`CryptoError::Random`]; [`CryptoError::Provider`].
    pub fn create(&self, password: Option<&str>) -> Result<(DataKey, KeyRecord), CryptoError> {
        let key = DataKey::generate()?;
        let record = self.protect(&key, password)?;
        Ok((key, record))
    }

    /// Recovers the data key from `record`.
    ///
    /// # Errors
    ///
    /// [`CryptoError::InvalidKeyRecord`]; [`CryptoError::Provider`] if the OS binding
    /// rejects the record; [`CryptoError::PasswordRequired`] / [`CryptoError::NoPasswordSet`]
    /// if `password` does not match the record's mode; [`CryptoError::WrongPassword`].
    pub fn unlock(
        &self,
        record: &KeyRecord,
        password: Option<&str>,
    ) -> Result<DataKey, CryptoError> {
        let inner = self
            .provider
            .unwrap_key(&record.0[MAGIC.len()..])
            .map_err(CryptoError::Provider)?;
        // Shape first, so a damaged record is never mistaken for a password problem.
        match (inner.first().copied(), inner.len(), password) {
            (Some(MODE_OS_ONLY), OS_ONLY_INNER_LEN, None) => {
                DataKey::from_slice(&inner[1..]).ok_or(CryptoError::InvalidKeyRecord)
            }
            (Some(MODE_OS_ONLY), OS_ONLY_INNER_LEN, Some(_)) => Err(CryptoError::NoPasswordSet),
            (Some(MODE_PASSWORD), PASSWORD_INNER_LEN, None) => Err(CryptoError::PasswordRequired),
            (Some(MODE_PASSWORD), PASSWORD_INNER_LEN, Some(password)) => {
                unseal_with_password(&inner, password)
            }
            _ => Err(CryptoError::InvalidKeyRecord),
        }
    }

    /// Whether `record` needs a master password to open (the mode byte inside the OS-wrapped
    /// blob). Opens the OS binding, never the data key.
    ///
    /// # Errors
    ///
    /// [`CryptoError::Provider`] if the OS binding rejects the record;
    /// [`CryptoError::InvalidKeyRecord`] for a damaged one.
    pub fn has_password(&self, record: &KeyRecord) -> Result<bool, CryptoError> {
        let inner = self
            .provider
            .unwrap_key(&record.0[MAGIC.len()..])
            .map_err(CryptoError::Provider)?;
        match (inner.first().copied(), inner.len()) {
            (Some(MODE_OS_ONLY), OS_ONLY_INNER_LEN) => Ok(false),
            (Some(MODE_PASSWORD), PASSWORD_INNER_LEN) => Ok(true),
            _ => Err(CryptoError::InvalidKeyRecord),
        }
    }

    /// Re-wraps the data key in `record` under a new master password (`None` removes it).
    /// The data key is unchanged, so encrypted items stay readable and are not rewritten.
    /// `current` must be the present password (`None` if there is none). On any error the old
    /// record is still the one to keep.
    ///
    /// # Errors
    ///
    /// As [`KeyRing::unlock`] for `current`; [`CryptoError::EmptyPassword`] for `new`.
    pub fn change_password(
        &self,
        record: &KeyRecord,
        current: Option<&str>,
        new: Option<&str>,
    ) -> Result<KeyRecord, CryptoError> {
        self.rekey(record, current, new).map(|(_, record)| record)
    }

    /// As [`change_password`](Self::change_password), and also returns the data key it opened on
    /// the way, so a caller that must open the library after the change does not derive the
    /// password twice.
    ///
    /// # Errors
    ///
    /// As [`change_password`](Self::change_password).
    pub fn rekey(
        &self,
        record: &KeyRecord,
        current: Option<&str>,
        new: Option<&str>,
    ) -> Result<(DataKey, KeyRecord), CryptoError> {
        let key = self.unlock(record, current)?;
        let changed = self.protect(&key, new)?;
        Ok((key, changed))
    }

    fn protect(&self, key: &DataKey, password: Option<&str>) -> Result<KeyRecord, CryptoError> {
        let inner: Zeroizing<Vec<u8>> = match password {
            None => {
                let mut inner = Zeroizing::new(Vec::with_capacity(1 + KEY_LEN));
                inner.push(MODE_OS_ONLY);
                inner.extend_from_slice(key.expose());
                inner
            }
            Some(password) => seal_with_password(key, password, self.kdf)?,
        };
        let blob = self
            .provider
            .wrap_key(&inner)
            .map_err(CryptoError::Provider)?;
        let mut record = Vec::with_capacity(MAGIC.len() + blob.len());
        record.extend_from_slice(MAGIC);
        record.extend_from_slice(&blob);
        Ok(KeyRecord(record))
    }
}

fn seal_with_password(
    key: &DataKey,
    password: &str,
    kdf: KdfParams,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if password.is_empty() {
        return Err(CryptoError::EmptyPassword);
    }
    let salt = random::<SALT_LEN>()?;
    let mut inner = Vec::with_capacity(PASSWORD_INNER_LEN);
    inner.push(MODE_PASSWORD);
    inner.extend_from_slice(&kdf.m_cost_kib.to_le_bytes());
    inner.extend_from_slice(&kdf.t_cost.to_le_bytes());
    inner.extend_from_slice(&kdf.p_cost.to_le_bytes());
    inner.extend_from_slice(salt.as_slice());
    let kek = kdf.derive(password, &salt)?;
    let sealed = seal(&kek, &inner, key.expose())?;
    inner.extend_from_slice(&sealed);
    Ok(Zeroizing::new(inner))
}

fn unseal_with_password(inner: &[u8], password: &str) -> Result<DataKey, CryptoError> {
    if password.is_empty() {
        return Err(CryptoError::EmptyPassword);
    }
    if inner.len() != PASSWORD_INNER_LEN {
        return Err(CryptoError::InvalidKeyRecord);
    }
    let (header, sealed) = inner.split_at(HEADER_LEN);
    let word = |at: usize| {
        u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    // Bounds first: a hostile record must not pick the work factor.
    let kdf =
        KdfParams::stored(word(1), word(5), word(9)).map_err(|_| CryptoError::InvalidKeyRecord)?;
    let salt: [u8; SALT_LEN] = header[13..]
        .try_into()
        .map_err(|_| CryptoError::InvalidKeyRecord)?;
    let kek = kdf.derive(password, &salt)?;
    let plain = open(&kek, header, sealed).map_err(|_| CryptoError::WrongPassword)?;
    DataKey::from_slice(&plain).ok_or(CryptoError::InvalidKeyRecord)
}

#[cfg(test)]
mod tests {
    use snipdeck_core::ItemId;

    use super::super::{KeyProviderError, MemoryKeyProvider, decrypt_item, encrypt_item};
    use super::*;

    /// Cheap settings so the suite runs in milliseconds; the format is identical.
    fn fast() -> KdfParams {
        KdfParams::insecure_for_tests(64, 1, 1).unwrap()
    }

    fn provider() -> MemoryKeyProvider {
        MemoryKeyProvider::new().unwrap()
    }

    fn same_key(a: &DataKey, b: &DataKey) -> bool {
        a.expose() == b.expose()
    }

    #[test]
    fn os_only_record_round_trips() {
        let provider = provider();
        let ring = KeyRing::new(&provider);
        let (key, record) = ring.create(None).unwrap();
        assert!(same_key(&key, &ring.unlock(&record, None).unwrap()));
    }

    #[test]
    fn password_record_round_trips() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (key, record) = ring.create(Some("hunter2 but longer")).unwrap();
        let unlocked = ring.unlock(&record, Some("hunter2 but longer")).unwrap();
        assert!(same_key(&key, &unlocked));
    }

    #[test]
    fn records_have_the_snkey1_header_and_hide_the_key() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        for password in [None, Some("pw")] {
            let (key, record) = ring.create(password).unwrap();
            assert!(record.as_bytes().starts_with(b"SNKEY1"));
            let needle = &key.expose()[..8];
            assert!(!record.as_bytes().windows(8).any(|w| w == needle));
            assert_eq!(
                KeyRecord::from_bytes(record.as_bytes().to_vec()).unwrap(),
                record
            );
        }
    }

    #[test]
    fn wrong_password_is_a_clean_error() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (_, record) = ring.create(Some("right")).unwrap();
        for wrong in ["wrong", "Right", "right ", "rights"] {
            assert_eq!(
                ring.unlock(&record, Some(wrong)).unwrap_err(),
                CryptoError::WrongPassword,
                "{wrong}"
            );
        }
    }

    #[test]
    fn password_mode_mismatches_are_reported() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (_, locked) = ring.create(Some("pw")).unwrap();
        let (_, open) = ring.create(None).unwrap();
        assert_eq!(
            ring.unlock(&locked, None).unwrap_err(),
            CryptoError::PasswordRequired
        );
        assert_eq!(
            ring.unlock(&open, Some("pw")).unwrap_err(),
            CryptoError::NoPasswordSet
        );
    }

    #[test]
    fn empty_passwords_are_refused() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        assert_eq!(
            ring.create(Some("")).unwrap_err(),
            CryptoError::EmptyPassword
        );
        let (_, record) = ring.create(Some("pw")).unwrap();
        assert_eq!(
            ring.unlock(&record, Some("")).unwrap_err(),
            CryptoError::EmptyPassword
        );
        assert_eq!(
            ring.change_password(&record, Some("pw"), Some(""))
                .unwrap_err(),
            CryptoError::EmptyPassword
        );
    }

    #[test]
    fn a_different_os_binding_cannot_unlock() {
        let ring_a_provider = provider();
        let (_, record) = KeyRing::new(&ring_a_provider).create(None).unwrap();
        let other = provider();
        assert_eq!(
            KeyRing::new(&other).unlock(&record, None).unwrap_err(),
            CryptoError::Provider(KeyProviderError::Rejected)
        );
    }

    #[test]
    fn changing_the_password_keeps_the_data_key_and_old_items() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (key, record) = ring.create(Some("old")).unwrap();
        let id = ItemId::generate();
        let envelope = encrypt_item(&key, &id, b"my secret").unwrap();

        let changed = ring
            .change_password(&record, Some("old"), Some("new"))
            .unwrap();

        assert_ne!(changed, record);
        assert_eq!(
            ring.unlock(&changed, Some("old")).unwrap_err(),
            CryptoError::WrongPassword
        );
        let unlocked = ring.unlock(&changed, Some("new")).unwrap();
        assert!(same_key(&key, &unlocked));
        assert_eq!(
            *decrypt_item(&unlocked, &id, &envelope).unwrap(),
            b"my secret"
        );
    }

    #[test]
    fn passwords_can_be_added_and_removed() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (key, plain) = ring.create(None).unwrap();

        let locked = ring.change_password(&plain, None, Some("pw")).unwrap();
        assert_eq!(
            ring.unlock(&locked, None).unwrap_err(),
            CryptoError::PasswordRequired
        );
        assert!(same_key(&key, &ring.unlock(&locked, Some("pw")).unwrap()));

        let unlocked = ring.change_password(&locked, Some("pw"), None).unwrap();
        assert!(same_key(&key, &ring.unlock(&unlocked, None).unwrap()));
    }

    #[test]
    fn a_failed_change_reveals_nothing_and_changes_nothing() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (key, record) = ring.create(Some("pw")).unwrap();
        assert_eq!(
            ring.change_password(&record, Some("nope"), Some("new"))
                .unwrap_err(),
            CryptoError::WrongPassword
        );
        assert!(same_key(&key, &ring.unlock(&record, Some("pw")).unwrap()));
    }

    #[test]
    fn every_password_set_uses_a_fresh_salt_and_nonce() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (key, first) = ring.create(Some("pw")).unwrap();
        let second = ring.protect(&key, Some("pw")).unwrap();
        let salt = |record: &KeyRecord| {
            let inner = provider.unwrap_key(&record.as_bytes()[6..]).unwrap();
            inner[13..13 + SALT_LEN].to_vec()
        };
        assert_ne!(salt(&first), salt(&second));
    }

    #[test]
    fn records_unlock_with_their_own_parameters_not_the_current_ones() {
        let provider = provider();
        let (key, record) = KeyRing::new(&provider)
            .with_kdf_params(fast())
            .create(Some("pw"))
            .unwrap();
        let stronger = KdfParams::insecure_for_tests(128, 2, 1).unwrap();
        let ring = KeyRing::new(&provider).with_kdf_params(stronger);
        assert!(same_key(&key, &ring.unlock(&record, Some("pw")).unwrap()));
        // Re-setting the password adopts the new settings.
        let upgraded = ring
            .change_password(&record, Some("pw"), Some("pw"))
            .unwrap();
        let inner = provider.unwrap_key(&upgraded.as_bytes()[6..]).unwrap();
        assert_eq!(u32::from_le_bytes(inner[1..5].try_into().unwrap()), 128);
        assert_eq!(u32::from_le_bytes(inner[5..9].try_into().unwrap()), 2);
    }

    #[test]
    fn every_modified_byte_of_a_record_is_rejected() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        for password in [None, Some("pw")] {
            let (_, record) = ring.create(password).unwrap();
            for index in 0..record.as_bytes().len() {
                let mut bytes = record.as_bytes().to_vec();
                bytes[index] ^= 0x01;
                let result =
                    KeyRecord::from_bytes(bytes).and_then(|bad| ring.unlock(&bad, password));
                assert!(
                    result.is_err(),
                    "byte {index}, with a password: {}",
                    password.is_some()
                );
            }
        }
    }

    #[test]
    fn truncated_or_foreign_records_are_invalid() {
        for bytes in [
            vec![],
            b"SNKEY1".to_vec(),
            b"SNKEY2xxxxxxxx".to_vec(),
            vec![0; 100],
        ] {
            assert_eq!(
                KeyRecord::from_bytes(bytes).unwrap_err(),
                CryptoError::InvalidKeyRecord
            );
        }
    }

    /// A record whose provider blob authenticates but whose contents are hostile.
    fn record_with_inner(provider: &MemoryKeyProvider, inner: &[u8]) -> KeyRecord {
        let mut bytes = b"SNKEY1".to_vec();
        bytes.extend_from_slice(&provider.wrap_key(inner).unwrap());
        KeyRecord::from_bytes(bytes).unwrap()
    }

    #[test]
    fn hostile_work_factors_are_refused_before_any_hashing() {
        let provider = provider();
        let ring = KeyRing::new(&provider);
        for (m, t, p) in [
            (u32::MAX, 1, 1),
            (KdfParams::MAX_M_COST_KIB + 1, 1, 1),
            (64, u32::MAX, 1),
            (64, 1, u32::MAX),
            (0, 1, 1),
            (64, 0, 1),
            (64, 1, 0),
            (8, 1, 2),
        ] {
            let mut inner = vec![MODE_PASSWORD];
            for word in [m, t, p] {
                inner.extend_from_slice(&word.to_le_bytes());
            }
            inner.resize(PASSWORD_INNER_LEN, 0);
            let record = record_with_inner(&provider, &inner);
            assert_eq!(
                ring.unlock(&record, Some("pw")).unwrap_err(),
                CryptoError::InvalidKeyRecord,
                "{m} {t} {p}"
            );
        }
    }

    #[test]
    fn malformed_inner_plaintext_is_invalid() {
        let provider = provider();
        let ring = KeyRing::new(&provider);
        for inner in [&b""[..], &[2; 33], &[0; 5], &[0; 34], &[1; 10]] {
            let record = record_with_inner(&provider, inner);
            assert_eq!(
                ring.unlock(&record, None).unwrap_err(),
                CryptoError::InvalidKeyRecord,
                "{inner:?}"
            );
        }
    }

    #[test]
    fn the_password_header_is_authenticated() {
        // Same cost with a flipped salt bit inside an otherwise valid record: wrong key.
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (_, record) = ring.create(Some("pw")).unwrap();
        let mut inner = provider
            .unwrap_key(&record.as_bytes()[6..])
            .unwrap()
            .to_vec();
        inner[20] ^= 1;
        let tampered = record_with_inner(&provider, &inner);
        assert_eq!(
            ring.unlock(&tampered, Some("pw")).unwrap_err(),
            CryptoError::WrongPassword
        );
    }

    #[test]
    fn kdf_params_are_validated() {
        let weak = KdfParams::insecure_for_tests;
        assert!(weak(64, 1, 1).is_ok());
        assert!(weak(7, 1, 1).is_err());
        assert!(weak(64, 0, 1).is_err());
        assert!(weak(64, 1, 0).is_err());
        assert!(weak(KdfParams::MAX_M_COST_KIB + 1, 1, 1).is_err());
        assert!(weak(64, KdfParams::MAX_T_COST + 1, 1).is_err());
        assert!(weak(1024, 1, KdfParams::MAX_P_COST + 1).is_err());
        assert_eq!(KdfParams::DEFAULT.m_cost_kib(), 65_536);
        assert_eq!(KdfParams::DEFAULT.t_cost(), 3);
        assert_eq!(KdfParams::DEFAULT.p_cost(), 1);
    }

    /// Review L6: production code cannot build weak settings, and a stored record asking for
    /// them is refused outside test builds.
    #[test]
    fn production_settings_have_a_floor() {
        assert_eq!(
            KdfParams::new(8, 1, 1).unwrap_err(),
            CryptoError::InvalidKdfParams
        );
        assert!(KdfParams::new(KdfParams::MIN_M_COST_KIB - 1, 2, 1).is_err());
        assert!(KdfParams::new(KdfParams::MIN_M_COST_KIB, 1, 1).is_err());
        assert!(KdfParams::new(KdfParams::MIN_M_COST_KIB, 2, 1).is_ok());
        assert!(KdfParams::new(65_536, 3, 1).is_ok());
        // What `unlock` applies to a stored record in a production build.
        assert!(KdfParams::check(64, 1, 1, PRODUCTION_FLOOR).is_err());
        assert!(KdfParams::check(19 * 1024, 2, 1, PRODUCTION_FLOOR).is_ok());
    }

    #[test]
    fn argon2id_output_is_stable() {
        // Guards the stored format: if this changes, existing users' passwords stop working.
        // Reference: libargon2 (argon2-cffi) Argon2id v1.3, m=64 KiB, t=1, p=1, 32-byte output, password "password",
        // salt 0x00..0x0f.
        let salt: [u8; SALT_LEN] = core::array::from_fn(|i| u8::try_from(i).unwrap());
        let kek = fast().derive("password", &salt).unwrap();
        assert_eq!(hex(kek.as_slice()), GOLDEN_KEK);
    }

    const GOLDEN_KEK: &str = "9c493022877348e034719bce110c99301e5d4d16acacc6a829000ff2044b1b0c";

    fn hex(bytes: &[u8]) -> String {
        use core::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    /// `cargo test -p snipdeck-store --release -- --ignored --nocapture default_params`
    #[test]
    #[ignore = "benchmark: run in release mode with --nocapture"]
    #[allow(clippy::print_stderr, reason = "a benchmark reports its measurement")]
    fn default_params_timing() {
        let salt = [7_u8; SALT_LEN];
        let mut millis = Vec::new();
        for _ in 0..7 {
            let start = std::time::Instant::now();
            let kek = KdfParams::DEFAULT
                .derive("a reasonably long passphrase", &salt)
                .unwrap();
            millis.push(start.elapsed().as_millis());
            assert_eq!(kek.len(), KEY_LEN);
        }
        millis.sort_unstable();
        eprintln!(
            "Argon2id 64 MiB x3 x1: min {} ms, median {} ms, max {} ms",
            millis[0], millis[3], millis[6]
        );
    }

    #[test]
    fn debug_output_shows_no_secrets() {
        let provider = provider();
        let ring = KeyRing::new(&provider).with_kdf_params(fast());
        let (key, record) = ring.create(Some("super-secret-password")).unwrap();
        let text = format!("{key:?} {record:?} {ring:?} {provider:?} {:?}", fast());
        assert!(!text.contains("super-secret-password"));
        assert!(text.contains("DataKey(<redacted>)"));
        assert!(text.contains(&format!("KeyRecord({} bytes)", record.as_bytes().len())));
        for error in [
            CryptoError::WrongPassword,
            CryptoError::AuthenticationFailed,
            CryptoError::Provider(KeyProviderError::Rejected),
        ] {
            let shown = format!("{error} {error:?}");
            assert!(!shown.contains("super-secret-password"));
        }
    }
}
