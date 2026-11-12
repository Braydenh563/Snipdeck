//! The Windows Hello copy of the data key (`docs/crypto.md` "Windows Hello").
//!
//! Hello signs a fixed [`HELLO_CHALLENGE`]; `KEK = SHA-256("snipdeck-hello-kek-v1" || signature)`
//! seals the data key with the same AEAD as items. The result, a [`SealedKey`], lives in memory
//! only and remembers when the password last unlocked the library. The signer is a closure so this
//! crate needs nothing from the platform crate: the app passes
//! `|challenge| hello.sign_challenge(challenge)`.
//!
//! Because the KEK comes from a signature, the signature must be deterministic. [`SealedKey::seal`]
//! signs twice and refuses a signer whose two answers differ.

use std::time::Duration;

use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use super::aead;
use super::key::{DataKey, KEY_LEN};

/// The fixed challenge Hello signs.
pub const HELLO_CHALLENGE: &[u8] = b"snipdeck-hello-unlock-challenge-v1";
/// The copy is unusable this long after the last password unlock (7 days).
pub const HELLO_MAX_AGE: Duration = Duration::from_hours(7 * 24);

const KEK_DOMAIN: &[u8] = b"snipdeck-hello-kek-v1";
const AAD: &[u8] = b"snipdeck-hello-seal-v1";

/// Why sealing or opening the Hello copy failed. `E` is the signer's own error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HelloSealError<E> {
    /// More than 7 days since the password unlock (or the clock went back); the signer was not
    /// asked.
    #[error("the Windows Hello copy has expired")]
    Expired,
    /// Two signatures of the same challenge differed: this Hello cannot be used for a key.
    #[error("Windows Hello signatures are not repeatable")]
    NotRepeatable,
    /// The signature did not open the copy, or it is damaged.
    #[error("the Windows Hello copy did not open")]
    Failed,
    /// The signer failed (cancelled, removed, ...).
    #[error("signing failed")]
    Sign(E),
}

/// `SHA-256(domain || signature)`, zeroized on drop.
fn kek(signature: &[u8]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut hasher = Sha256::new();
    hasher.update(KEK_DOMAIN);
    hasher.update(signature);
    let mut out = Zeroizing::new([0_u8; KEY_LEN]);
    hasher.finalize_into((&mut *out).into());
    out
}

/// Equality without an early exit.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0_u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// The data key sealed under Hello, plus when the password last unlocked the library.
#[derive(Clone)]
pub struct SealedKey {
    blob: Vec<u8>,
    password_unlock_at: Duration,
}

impl core::fmt::Debug for SealedKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SealedKey")
            .field("password_unlock_at", &self.password_unlock_at)
            .finish_non_exhaustive()
    }
}

impl SealedKey {
    /// Seals `key`. `sign` is asked for the signature of [`HELLO_CHALLENGE`] twice. `now` (since
    /// the Unix epoch) is the time of the password unlock that produced `key`.
    ///
    /// # Errors
    ///
    /// [`HelloSealError::Sign`] from `sign`; [`HelloSealError::NotRepeatable`] if the two
    /// signatures differ; [`HelloSealError::Failed`] if sealing fails.
    pub fn seal<E>(
        key: &DataKey,
        now: Duration,
        mut sign: impl FnMut(&[u8]) -> Result<Zeroizing<Vec<u8>>, E>,
    ) -> Result<Self, HelloSealError<E>> {
        let first = sign(HELLO_CHALLENGE).map_err(HelloSealError::Sign)?;
        let second = sign(HELLO_CHALLENGE).map_err(HelloSealError::Sign)?;
        if !same(&first, &second) {
            return Err(HelloSealError::NotRepeatable);
        }
        let kek = kek(&first);
        let blob =
            aead::seal(&kek, AAD, key.expose().as_slice()).map_err(|_| HelloSealError::Failed)?;
        Ok(Self {
            blob,
            password_unlock_at: now,
        })
    }

    /// Seals `key` with one signature of [`HELLO_CHALLENGE`]: one Hello prompt. Only for a Hello
    /// whose signatures [`seal`](Self::seal) already found repeatable (checked when the user turned
    /// Hello on); a signer that later changes simply fails to open the copy and the password is
    /// asked.
    ///
    /// # Errors
    ///
    /// [`HelloSealError::Sign`] from `sign`; [`HelloSealError::Failed`] if sealing fails.
    pub fn seal_once<E>(
        key: &DataKey,
        now: Duration,
        sign: impl FnOnce(&[u8]) -> Result<Zeroizing<Vec<u8>>, E>,
    ) -> Result<Self, HelloSealError<E>> {
        let signature = sign(HELLO_CHALLENGE).map_err(HelloSealError::Sign)?;
        let kek = kek(&signature);
        let blob =
            aead::seal(&kek, AAD, key.expose().as_slice()).map_err(|_| HelloSealError::Failed)?;
        Ok(Self {
            blob,
            password_unlock_at: now,
        })
    }

    /// Opens the key. Expired (checked first, so Hello is not prompted for nothing) or not, `sign`
    /// is only called when the copy is still valid at `now`.
    ///
    /// # Errors
    ///
    /// [`HelloSealError::Expired`], [`HelloSealError::Sign`] from `sign`, or
    /// [`HelloSealError::Failed`] when the signature does not open the copy.
    pub fn open<E>(
        &self,
        now: Duration,
        sign: impl FnOnce(&[u8]) -> Result<Zeroizing<Vec<u8>>, E>,
    ) -> Result<DataKey, HelloSealError<E>> {
        if self.is_expired(now) {
            return Err(HelloSealError::Expired);
        }
        let signature = sign(HELLO_CHALLENGE).map_err(HelloSealError::Sign)?;
        let kek = kek(&signature);
        let plain = aead::open(&kek, AAD, &self.blob).map_err(|_| HelloSealError::Failed)?;
        DataKey::from_slice(&plain).ok_or(HelloSealError::Failed)
    }

    /// When the password last unlocked the library.
    #[must_use]
    pub fn password_unlock_at(&self) -> Duration {
        self.password_unlock_at
    }

    /// Whether the copy is unusable at `now`: [`HELLO_MAX_AGE`] has passed (exactly 7 days counts)
    /// or the clock is earlier than the password unlock.
    #[must_use]
    pub fn is_expired(&self, now: Duration) -> bool {
        match now.checked_sub(self.password_unlock_at) {
            Some(age) => age >= HELLO_MAX_AGE,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::crypto::key::test_bytes;

    const T0: Duration = Duration::from_hours(500_000);

    fn key() -> DataKey {
        DataKey::from_bytes(test_bytes::<KEY_LEN>(1))
    }

    fn signer(secret: &'static [u8]) -> impl Fn(&[u8]) -> Result<Zeroizing<Vec<u8>>, ()> {
        move |challenge| {
            let mut v = secret.to_vec();
            v.extend_from_slice(challenge);
            Ok(Zeroizing::new(v))
        }
    }

    #[test]
    fn seal_then_open_round_trips() {
        let sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        let opened = sealed.open(T0, signer(b"a")).unwrap();
        assert!(opened.same_as(&key()));
    }

    #[test]
    fn seal_once_signs_once_and_opens_like_seal() {
        let calls = Cell::new(0_u8);
        let counted = |c: &[u8]| {
            calls.set(calls.get() + 1);
            signer(b"a")(c)
        };
        let sealed = SealedKey::seal_once(&key(), T0, counted).unwrap();
        assert_eq!(calls.get(), 1);
        assert!(sealed.open(T0, signer(b"a")).unwrap().same_as(&key()));
        assert_eq!(
            sealed.open(T0, signer(b"b")).unwrap_err(),
            HelloSealError::Failed
        );
    }

    #[test]
    fn a_different_signature_does_not_open() {
        let sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        let err = sealed.open(T0, signer(b"b")).unwrap_err();
        assert_eq!(err, HelloSealError::Failed);
    }

    #[test]
    fn seal_signs_twice_and_refuses_a_changing_signature() {
        let calls = Cell::new(0_u8);
        let flip = |_: &[u8]| -> Result<Zeroizing<Vec<u8>>, ()> {
            calls.set(calls.get() + 1);
            Ok(Zeroizing::new(vec![calls.get()]))
        };
        let err = SealedKey::seal(&key(), T0, flip).unwrap_err();
        assert_eq!(err, HelloSealError::NotRepeatable);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn expires_at_exactly_seven_days_and_when_the_clock_goes_back() {
        let sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        assert!(!sealed.is_expired(T0));
        assert!(!sealed.is_expired(T0 + Duration::from_hours(167)));
        assert!(sealed.is_expired(T0 + HELLO_MAX_AGE));
        assert!(sealed.is_expired(Duration::from_hours(499_999)));
        assert_eq!(HELLO_MAX_AGE, Duration::from_hours(168));
    }

    #[test]
    fn an_expired_copy_does_not_prompt() {
        let sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        let asked = Cell::new(false);
        let err = sealed
            .open(T0 + HELLO_MAX_AGE, |c| {
                asked.set(true);
                signer(b"a")(c)
            })
            .unwrap_err();
        assert_eq!(err, HelloSealError::Expired);
        assert!(!asked.get());
    }

    #[test]
    fn signer_errors_pass_through() {
        let sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        let err = sealed.open(T0, |_| Err("cancelled")).unwrap_err();
        assert_eq!(err, HelloSealError::Sign("cancelled"));
        let err = SealedKey::seal(&key(), T0, |_| Err::<Zeroizing<Vec<u8>>, _>("x")).unwrap_err();
        assert_eq!(err, HelloSealError::Sign("x"));
    }

    #[test]
    fn a_truncated_blob_fails() {
        let mut sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        sealed.blob.truncate(10);
        assert_eq!(
            sealed.open(T0, signer(b"a")).unwrap_err(),
            HelloSealError::Failed
        );
    }

    #[test]
    fn fresh_nonce_and_no_plain_key_in_the_blob() {
        let a = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        let b = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        assert_ne!(a.blob, b.blob);
        let plain = key();
        assert!(
            !a.blob
                .windows(KEY_LEN)
                .any(|w| w == plain.expose().as_slice())
        );
    }

    #[test]
    fn debug_shows_no_blob_bytes() {
        let sealed = SealedKey::seal(&key(), T0, signer(b"a")).unwrap();
        let text = format!("{sealed:?} {sealed:#?}");
        assert!(!text.contains("blob"));
        let hex = sealed.blob[..8]
            .iter()
            .fold(String::new(), |acc, b| acc + &format!("{b:02x}"));
        let dec = format!("{:?}", &sealed.blob[..4]);
        assert!(!text.contains(&hex) && !text.contains(&dec));
    }
}
