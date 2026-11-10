//! Windows Hello unlock, portable half (v1.1 slice 8, `docs/crypto.md` "Windows Hello").
//!
//! Hello never holds the data key. After a password unlock the app asks Hello to sign a fixed
//! challenge ([`HelloAuthenticator::sign_challenge`]); `KEK = SHA-256(signature)` seals the data key
//! into an in-memory [`SealedKey`]. A later Hello prompt re-signs the same challenge (RSA PKCS#1
//! v1.5 is deterministic) and opens it. Nothing is written to disk, so a restart forgets the copy.
//! The copy also expires [`HELLO_MAX_AGE`] after the last password unlock.
//!
//! The platform crate does not depend on the store crate, so this module runs the same
//! XChaCha20-Poly1305 `nonce | ciphertext | tag` layout as `store/crypto/aead.rs` on raw 32-byte
//! keys; the caller converts to and from its `DataKey`.
//!
//! The clock is injected: `now` is a [`Duration`] since the Unix epoch.

use std::sync::Mutex;
use std::time::Duration;

use chacha20poly1305::aead::{Aead as _, Payload};
use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce};
use sha2::{Digest as _, Sha256};
use zeroize::{Zeroize as _, Zeroizing};

/// Length of the data key and of the KEK, in bytes.
pub const KEY_LEN: usize = 32;
/// The Hello copy is dropped this long after the last password unlock (7 days).
pub const HELLO_MAX_AGE: Duration = Duration::from_hours(7 * 24);
/// The fixed challenge Hello signs. Changing it orphans every sealed copy (harmless: memory only).
pub const HELLO_CHALLENGE: &[u8] = b"snipdeck-hello-unlock-challenge-v1";

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const AAD: &[u8] = b"snipdeck-hello-seal-v1";

/// Whether this PC can use Hello right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloAvailability {
    /// A Hello credential can be made or used.
    Ready,
    /// Hello exists on this PC but the user has no PIN, face or fingerprint set up.
    NotSetUp,
    /// No Hello on this PC (or not on this OS).
    Unsupported,
}

/// Why a Hello step did not give a signature. All of them fall back to the password and drop the
/// sealed copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HelloError {
    /// The user closed the Hello prompt.
    #[error("Windows Hello was cancelled")]
    Cancelled,
    /// Hello failed, or the signature did not open the sealed key.
    #[error("Windows Hello failed")]
    Failed,
    /// The Hello credential or PIN was removed.
    #[error("the Windows Hello credential was removed")]
    Removed,
}

/// The OS side of Hello. Methods block (a prompt may be on screen): call them from a worker.
pub trait HelloAuthenticator: Send + Sync {
    /// Whether Hello can be used.
    fn availability(&self) -> HelloAvailability;
    /// Makes the Snipdeck Hello credential if it does not exist yet (may prompt).
    ///
    /// # Errors
    ///
    /// [`HelloError`] when the user cancels or Hello fails.
    fn ensure_credential(&self) -> Result<(), HelloError>;
    /// Signs `challenge` with the credential (prompts the user). Deterministic for one credential.
    ///
    /// # Errors
    ///
    /// [`HelloError::Cancelled`], [`HelloError::Removed`] or [`HelloError::Failed`].
    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>, HelloError>;
}

/// The key-encryption key for a signature: its SHA-256.
fn kek(signature: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut out = Zeroizing::new([0_u8; KEY_LEN]);
    out.copy_from_slice(&Sha256::digest(signature));
    out
}

/// The data key sealed under Hello, plus when the password last unlocked the library. Memory only.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedKey {
    blob: Vec<u8>,
    password_unlock_at: Duration,
}

impl std::fmt::Debug for SealedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedKey")
            .field("password_unlock_at", &self.password_unlock_at)
            .finish_non_exhaustive()
    }
}

impl SealedKey {
    /// Asks Hello to sign [`HELLO_CHALLENGE`] and seals `key` under it. `now` is the time of the
    /// password unlock that produced `key`.
    ///
    /// # Errors
    ///
    /// Whatever Hello returns, or [`HelloError::Failed`] if sealing fails.
    pub fn seal(
        auth: &dyn HelloAuthenticator,
        key: &[u8; KEY_LEN],
        now: Duration,
    ) -> Result<Self, HelloError> {
        let mut signature = auth.sign_challenge(HELLO_CHALLENGE)?;
        let kek = kek(&signature);
        signature.zeroize();
        let mut nonce = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| HelloError::Failed)?;
        let ciphertext = XChaCha20Poly1305::new((&*kek).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: key.as_slice(),
                    aad: AAD,
                },
            )
            .map_err(|_| HelloError::Failed)?;
        let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        Ok(Self {
            blob,
            password_unlock_at: now,
        })
    }

    /// Asks Hello to sign again and opens the key.
    ///
    /// # Errors
    ///
    /// Whatever Hello returns; [`HelloError::Failed`] when the signature does not open the copy.
    pub fn open(
        &self,
        auth: &dyn HelloAuthenticator,
    ) -> Result<Zeroizing<[u8; KEY_LEN]>, HelloError> {
        let mut signature = auth.sign_challenge(HELLO_CHALLENGE)?;
        let kek = kek(&signature);
        signature.zeroize();
        if self.blob.len() < NONCE_LEN + TAG_LEN {
            return Err(HelloError::Failed);
        }
        let (nonce, ciphertext) = self.blob.split_at(NONCE_LEN);
        let nonce = XNonce::try_from(nonce).map_err(|_| HelloError::Failed)?;
        let plain = XChaCha20Poly1305::new((&*kek).into())
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: AAD,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| HelloError::Failed)?;
        let mut key = Zeroizing::new([0_u8; KEY_LEN]);
        if plain.len() != KEY_LEN {
            return Err(HelloError::Failed);
        }
        key.copy_from_slice(&plain);
        Ok(key)
    }

    /// When the password last unlocked the library.
    #[must_use]
    pub fn password_unlock_at(&self) -> Duration {
        self.password_unlock_at
    }

    /// Whether [`HELLO_MAX_AGE`] has passed since the password unlock (at exactly 7 days: yes). A
    /// clock set back counts as not expired.
    #[must_use]
    pub fn is_expired(&self, now: Duration) -> bool {
        now.saturating_sub(self.password_unlock_at) >= HELLO_MAX_AGE
    }
}

/// A deterministic Hello for tests: set the outcomes, count the prompts.
#[derive(Debug)]
pub struct FakeHello {
    state: Mutex<FakeState>,
}

#[derive(Debug)]
struct FakeState {
    availability: HelloAvailability,
    ensure: Result<(), HelloError>,
    sign: Result<(), HelloError>,
    secret: Vec<u8>,
    prompts: usize,
}

impl Default for FakeHello {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeHello {
    /// Ready, everything succeeds, one fixed credential.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(FakeState {
                availability: HelloAvailability::Ready,
                ensure: Ok(()),
                sign: Ok(()),
                secret: b"fake-credential-1".to_vec(),
                prompts: 0,
            }),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut FakeState) -> T) -> T {
        f(&mut self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// Sets what [`HelloAuthenticator::availability`] says.
    pub fn set_availability(&self, value: HelloAvailability) {
        self.with(|s| s.availability = value);
    }

    /// Sets the outcome of [`HelloAuthenticator::ensure_credential`].
    pub fn set_ensure(&self, value: Result<(), HelloError>) {
        self.with(|s| s.ensure = value);
    }

    /// Sets the outcome of [`HelloAuthenticator::sign_challenge`] (`Ok` signs normally).
    pub fn set_sign(&self, value: Result<(), HelloError>) {
        self.with(|s| s.sign = value);
    }

    /// Replaces the credential: later signatures differ, like a re-made Hello credential.
    pub fn replace_credential(&self, secret: &[u8]) {
        self.with(|s| s.secret = secret.to_vec());
    }

    /// How many times a signature was asked for.
    #[must_use]
    pub fn prompts(&self) -> usize {
        self.with(|s| s.prompts)
    }
}

impl HelloAuthenticator for FakeHello {
    fn availability(&self) -> HelloAvailability {
        self.with(|s| s.availability)
    }

    fn ensure_credential(&self) -> Result<(), HelloError> {
        self.with(|s| s.ensure)
    }

    fn sign_challenge(&self, challenge: &[u8]) -> Result<Vec<u8>, HelloError> {
        self.with(|s| {
            s.prompts += 1;
            s.sign?;
            let mut hasher = Sha256::new();
            hasher.update(&s.secret);
            hasher.update(challenge);
            Ok(hasher.finalize().to_vec())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; KEY_LEN] = [7; KEY_LEN];
    const T0: Duration = Duration::from_hours(500_000);

    #[test]
    fn seal_then_open_round_trips() {
        let hello = FakeHello::new();
        let sealed = SealedKey::seal(&hello, &KEY, T0).unwrap();
        assert_eq!(*sealed.open(&hello).unwrap(), KEY);
        assert_eq!(hello.prompts(), 2);
    }

    #[test]
    fn a_different_signature_does_not_open() {
        let hello = FakeHello::new();
        let sealed = SealedKey::seal(&hello, &KEY, T0).unwrap();
        hello.replace_credential(b"another credential");
        assert_eq!(sealed.open(&hello).unwrap_err(), HelloError::Failed);
    }

    #[test]
    fn the_blob_holds_no_plain_key_and_is_fresh_each_time() {
        let hello = FakeHello::new();
        let a = SealedKey::seal(&hello, &KEY, T0).unwrap();
        let b = SealedKey::seal(&hello, &KEY, T0).unwrap();
        assert_ne!(a.blob, b.blob);
        assert!(!a.blob.windows(KEY_LEN).any(|w| w == KEY));
    }

    #[test]
    fn a_truncated_blob_fails() {
        let hello = FakeHello::new();
        let mut sealed = SealedKey::seal(&hello, &KEY, T0).unwrap();
        sealed.blob.truncate(10);
        assert_eq!(sealed.open(&hello).unwrap_err(), HelloError::Failed);
    }

    #[test]
    fn expires_at_exactly_seven_days() {
        let sealed = SealedKey::seal(&FakeHello::new(), &KEY, T0).unwrap();
        assert!(!sealed.is_expired(T0));
        assert!(!sealed.is_expired(T0 + Duration::from_hours(167)));
        assert!(sealed.is_expired(T0 + HELLO_MAX_AGE));
        assert!(!sealed.is_expired(Duration::from_hours(499_999)));
        assert_eq!(HELLO_MAX_AGE, Duration::from_hours(168));
    }

    #[test]
    fn cancel_and_removed_surface_as_errors() {
        let hello = FakeHello::new();
        hello.set_sign(Err(HelloError::Cancelled));
        assert_eq!(
            SealedKey::seal(&hello, &KEY, T0).unwrap_err(),
            HelloError::Cancelled
        );
        hello.set_sign(Ok(()));
        let sealed = SealedKey::seal(&hello, &KEY, T0).unwrap();
        hello.set_sign(Err(HelloError::Removed));
        assert_eq!(sealed.open(&hello).unwrap_err(), HelloError::Removed);
    }

    #[test]
    fn availability_and_ensure_follow_the_fake() {
        let hello = FakeHello::new();
        assert_eq!(hello.availability(), HelloAvailability::Ready);
        hello.set_availability(HelloAvailability::NotSetUp);
        assert_eq!(hello.availability(), HelloAvailability::NotSetUp);
        hello.set_ensure(Err(HelloError::Cancelled));
        assert_eq!(hello.ensure_credential(), Err(HelloError::Cancelled));
    }

    #[test]
    fn debug_shows_no_key_material() {
        let sealed = SealedKey::seal(&FakeHello::new(), &KEY, T0).unwrap();
        assert!(!format!("{sealed:?}").contains("blob"));
    }
}
