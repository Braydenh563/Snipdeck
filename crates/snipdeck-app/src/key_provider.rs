//! What wraps the library's encryption key (SPEC 5.7, `docs/crypto.md`): Windows DPAPI for the
//! current user. The first sensitive item on Supporter makes the key and asks this to protect it,
//! without a prompt; every later start asks it to open the key again.
//!
//! Off Windows there is no key protection to offer, so [`provider`] returns `None` and the item
//! editor says the key is unavailable instead of storing a secret under a key that anyone could
//! unwrap. The tests wrap the key with the store's in-memory provider.

use snipdeck_store::crypto::KeyProvider;

/// Mixed into every DPAPI blob, so another program running as the same user must also know it
/// (`docs/crypto.md`). Not a secret; changing it makes existing keys unreadable.
#[cfg(windows)]
const ENTROPY: &[u8] = b"Snipdeck library key v1";

#[cfg(windows)]
struct Dpapi;

#[cfg(windows)]
impl KeyProvider for Dpapi {
    fn wrap_key(&self, key: &[u8]) -> Result<Vec<u8>, snipdeck_store::crypto::KeyProviderError> {
        snipdeck_platform::windows::dpapi::protect(key, ENTROPY)
            .map_err(|e| snipdeck_store::crypto::KeyProviderError::Unavailable(e.to_string()))
    }

    fn unwrap_key(
        &self,
        wrapped: &[u8],
    ) -> Result<snipdeck_store::crypto::Zeroizing<Vec<u8>>, snipdeck_store::crypto::KeyProviderError>
    {
        // Any failure of the OS call means "this user cannot open it" (`docs/crypto.md`).
        snipdeck_platform::windows::dpapi::unprotect(wrapped, ENTROPY)
            .map_err(|_| snipdeck_store::crypto::KeyProviderError::Rejected)
    }
}

/// The key protection of this machine: DPAPI on Windows, none elsewhere.
#[must_use]
pub fn provider() -> Option<Box<dyn KeyProvider>> {
    #[cfg(windows)]
    {
        Some(Box::new(Dpapi))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(all(test, not(windows)))]
mod tests {
    #[test]
    fn off_windows_there_is_no_provider() {
        assert!(super::provider().is_none());
    }
}
