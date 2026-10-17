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

    pub(super) fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(Zeroizing::new(bytes))
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
    fn debug_output_contains_no_key_bytes() {
        let key = DataKey::from_bytes([0xAB; KEY_LEN]);
        let text = format!("{key:?} {key:#?}");
        assert_eq!(text, "DataKey(<redacted>) DataKey(<redacted>)");
    }
}
