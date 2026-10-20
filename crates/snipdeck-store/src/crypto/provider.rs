//! The OS-bound outer layer of the key hierarchy.

use zeroize::Zeroizing;

use super::KeyProviderError;
#[cfg(any(test, feature = "testing"))]
use super::aead::{open, seal};
#[cfg(any(test, feature = "testing"))]
use super::key::DataKey;

/// Wraps and unwraps small secrets (keys) so they are only usable by the current OS user.
///
/// The Windows implementation (DPAPI, `CryptProtectData` / `CryptUnprotectData` with
/// `CRYPTPROTECT_UI_FORBIDDEN`) lives in milestone 2. Every implementation must meet this
/// contract.
///
/// * **Round trip:** `unwrap_key(wrap_key(k)) == k` for any `k` from 1 to 4096 bytes, for the
///   same user on the same machine, across process restarts and app updates. A provider that
///   cannot guarantee that for the lifetime of the user's data must not be used: the wrapped
///   key is the only way back to the encrypted items.
/// * **Confidentiality and integrity:** the output must not reveal `k`, and `unwrap_key` must
///   fail with [`KeyProviderError::Rejected`], never return different bytes, for a blob that
///   was modified, truncated, empty, made by another provider or user, or made for another
///   machine (when the provider is machine-bound). DPAPI blobs are authenticated, so this
///   holds; a provider built on an unauthenticated cipher is not acceptable.
/// * **No UI, no network, no prompts.** Unlock happens at app start, possibly while the
///   screen is locked. Return [`KeyProviderError::Unavailable`] instead of asking.
/// * **No secrets in errors or logs.** Error text carries OS error codes or fixed strings
///   only.
/// * **Zeroize** every intermediate copy of `key` and of the unwrapped result; return the
///   result in [`Zeroizing`].
/// * **Thread safety:** methods take `&self`; implementations are `Send + Sync` so the app can
///   share one behind an `Arc` and run a master password's slow key derivation on another thread.
/// * Wrapping the same key twice may (and normally does) give different blobs.
pub trait KeyProvider: Send + Sync {
    /// Protects `key` for the current user and returns an opaque blob to store.
    ///
    /// # Errors
    ///
    /// [`KeyProviderError::Unavailable`] if the OS facility fails.
    fn wrap_key(&self, key: &[u8]) -> Result<Vec<u8>, KeyProviderError>;

    /// Recovers the key from a blob made by [`KeyProvider::wrap_key`].
    ///
    /// # Errors
    ///
    /// [`KeyProviderError::Rejected`] if the blob is not valid for this user; or
    /// [`KeyProviderError::Unavailable`] if the OS facility fails.
    fn unwrap_key(&self, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyProviderError>;
}

/// A provider that keeps its key-encryption key in process memory.
///
/// For tests only (the `testing` feature; review L6), until the DPAPI provider exists. It is **not** OS-bound:
/// the key dies with the process, so anything wrapped by it is unrecoverable afterwards.
/// It does use real authenticated encryption, so it honours the rest of the
/// [`KeyProvider`] contract.
#[cfg(any(test, feature = "testing"))]
pub struct MemoryKeyProvider {
    kek: DataKey,
}

#[cfg(any(test, feature = "testing"))]
impl MemoryKeyProvider {
    /// A provider with a fresh random key.
    ///
    /// # Errors
    ///
    /// [`KeyProviderError::Unavailable`] if the OS has no randomness to give.
    pub fn new() -> Result<Self, KeyProviderError> {
        DataKey::generate()
            .map(|kek| Self { kek })
            .map_err(|_| KeyProviderError::Unavailable("no randomness".to_owned()))
    }
}

#[cfg(any(test, feature = "testing"))]
impl core::fmt::Debug for MemoryKeyProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("MemoryKeyProvider(<redacted>)")
    }
}

#[cfg(any(test, feature = "testing"))]
impl KeyProvider for MemoryKeyProvider {
    fn wrap_key(&self, key: &[u8]) -> Result<Vec<u8>, KeyProviderError> {
        seal(self.kek.expose(), AAD, key)
            .map_err(|_| KeyProviderError::Unavailable("encryption failed".to_owned()))
    }

    fn unwrap_key(&self, wrapped: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyProviderError> {
        open(self.kek.expose(), AAD, wrapped).map_err(|_| KeyProviderError::Rejected)
    }
}

#[cfg(any(test, feature = "testing"))]
const AAD: &[u8] = b"snipdeck/MemoryKeyProvider/1";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_then_unwrap_returns_the_key() {
        let provider = MemoryKeyProvider::new().unwrap();
        for key in [&[1_u8][..], &[7; 32], &[9; 4096]] {
            let wrapped = provider.wrap_key(key).unwrap();
            assert_eq!(*provider.unwrap_key(&wrapped).unwrap(), key);
        }
    }

    #[test]
    fn wrapped_blob_hides_the_key_and_is_not_deterministic() {
        let provider = MemoryKeyProvider::new().unwrap();
        let key = [0x5A_u8; 32];
        let a = provider.wrap_key(&key).unwrap();
        let b = provider.wrap_key(&key).unwrap();
        assert_ne!(a, b);
        assert!(!a.windows(8).any(|w| w == &key[..8]));
    }

    #[test]
    fn another_provider_cannot_unwrap() {
        let wrapped = MemoryKeyProvider::new()
            .unwrap()
            .wrap_key(&[3; 32])
            .unwrap();
        assert_eq!(
            MemoryKeyProvider::new()
                .unwrap()
                .unwrap_key(&wrapped)
                .unwrap_err(),
            KeyProviderError::Rejected
        );
    }

    #[test]
    fn modified_truncated_or_empty_blobs_are_rejected() {
        let provider = MemoryKeyProvider::new().unwrap();
        let wrapped = provider.wrap_key(&[3; 32]).unwrap();
        for index in 0..wrapped.len() {
            let mut bad = wrapped.clone();
            bad[index] ^= 0x01;
            assert_eq!(
                provider.unwrap_key(&bad).unwrap_err(),
                KeyProviderError::Rejected
            );
        }
        for len in 0..wrapped.len() {
            assert_eq!(
                provider.unwrap_key(&wrapped[..len]).unwrap_err(),
                KeyProviderError::Rejected
            );
        }
    }

    #[test]
    fn debug_output_shows_no_key() {
        assert_eq!(
            format!("{:?}", MemoryKeyProvider::new().unwrap()),
            "MemoryKeyProvider(<redacted>)"
        );
    }
}
