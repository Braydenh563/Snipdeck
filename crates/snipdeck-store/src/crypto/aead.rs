//! The one place XChaCha20-Poly1305 is called: `nonce (24) | ciphertext | tag (16)` blobs.

use chacha20poly1305::aead::{Aead as _, Payload};
use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use super::CryptoError;
use super::key::{KEY_LEN, random};

pub(super) const NONCE_LEN: usize = 24;
pub(super) const TAG_LEN: usize = 16;

/// Encrypts `plaintext` under `key` with a fresh random nonce, authenticating `aad` too.
pub(super) fn seal(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let nonce = random::<NONCE_LEN>()?;
    let ciphertext = XChaCha20Poly1305::new(key.into())
        .encrypt(
            &XNonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CryptoError::Primitive)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Opens a blob made by [`seal`]. Every failure, including a blob too short to hold a nonce
/// and tag, is [`CryptoError::AuthenticationFailed`].
pub(super) fn open(
    key: &[u8; KEY_LEN],
    aad: &[u8],
    blob: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if blob.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::AuthenticationFailed);
    }
    let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
    let nonce = XNonce::try_from(nonce).map_err(|_| CryptoError::AuthenticationFailed)?;
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::AuthenticationFailed)
}
