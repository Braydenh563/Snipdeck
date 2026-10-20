//! The data key and random number plumbing.

use zeroize::Zeroizing;

use super::CryptoError;

/// Length of the data key and of every key-encryption key, in bytes.
pub(super) const KEY_LEN: usize = 32;

/// Fills a zeroizing array with OS randomness.
pub(super) fn random<const N: usize>() -> Result<Zeroizing<[u8; N]>, CryptoError> {
    let mut bytes = Zeroizing::new([0_u8; N]);
    getrandom::fill(bytes.as_mut_slice()).map_err(|_| CryptoError::Random)?;
    Ok(bytes)
}

/// The 32-byte key that encrypts items (the "DEK").
///
/// Zeroed on drop, not `Clone`, and `Debug` prints nothing of it.
pub struct DataKey(Zeroizing<[u8; KEY_LEN]>);

impl DataKey {
    /// A fresh random key.
    ///
    /// # Errors
    ///
    /// [`CryptoError::Random`] if the OS has no randomness to give.
    pub fn generate() -> Result<Self, CryptoError> {
        random().map(Self)
    }

    /// A key from exactly [`KEY_LEN`] bytes, copied straight into the zeroizing buffer so no
    /// un-zeroized copy is left on the stack (review L7). `None` for any other length.
    pub(super) fn from_slice(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != KEY_LEN {
            return None;
        }
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        key.copy_from_slice(bytes);
        Some(Self(key))
    }

    #[cfg(test)]
    pub(super) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// A 32-byte subkey bound to `context` (BLAKE3 `derive_key`), for values that must not be
    /// computable without the data key, such as the Recent inbox's duplicate hash.
    pub(crate) fn derive(&self, context: &str) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(blake3::derive_key(context, &self.0[..]))
    }

    pub(super) fn expose(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl core::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("DataKey(<redacted>)")
    }
}

/// Test key material computed from `tag`, so no key is written as a byte literal in a test.
#[cfg(test)]
pub(super) fn test_bytes<const N: usize>(tag: u8) -> [u8; N] {
    let seed = blake3::derive_key("snipdeck test key material", &[tag]);
    std::array::from_fn(|i| seed[i % seed.len()])
}

/// A test password computed from `label`, so no password is written in a test.
#[cfg(test)]
pub(super) fn pw(label: &str) -> &'static str {
    let key = blake3::derive_key("snipdeck test password", label.as_bytes());
    let hex = blake3::Hash::from_bytes(key).to_hex();
    Box::leak(hex.as_str()[..24].to_owned().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_differ() {
        let a = DataKey::generate().unwrap();
        let b = DataKey::generate().unwrap();
        assert_ne!(a.expose(), b.expose());
        assert_ne!(a.expose(), &[0; KEY_LEN]);
    }

    #[test]
    fn keys_are_built_from_exactly_32_bytes() {
        let bytes = test_bytes::<KEY_LEN>(7);
        assert_eq!(DataKey::from_slice(&bytes).unwrap().expose(), &bytes);
        assert!(DataKey::from_slice(&bytes[..KEY_LEN - 1]).is_none());
        let long = test_bytes::<{ KEY_LEN + 1 }>(7);
        assert!(DataKey::from_slice(&long).is_none());
    }

    #[test]
    fn debug_output_contains_no_key_bytes() {
        let key = DataKey::from_bytes(test_bytes(0xAB));
        let text = format!("{key:?} {key:#?}");
        assert_eq!(text, "DataKey(<redacted>) DataKey(<redacted>)");
    }
}
