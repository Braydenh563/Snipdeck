//! Test-only token minting, so tests are self-contained. Never compiled into the library.

use ed25519_dalek::SigningKey;

/// The seed of the development key pair. Public on purpose: it is not a secret and protects
/// nothing (see `keys.rs`).
pub(crate) const DEVELOPMENT_SEED: [u8; 32] = *b"snipdeck development licence key";

/// The development signing key (its seed is public, see `DEVELOPMENT_SEED`).
pub(crate) fn dev_signing_key() -> SigningKey {
    SigningKey::from_bytes(&DEVELOPMENT_SEED)
}

/// A signing key unrelated to the development key.
pub(crate) fn other_signing_key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

/// A fully valid payload for order `order`.
pub(crate) fn payload(order: &str) -> String {
    format!(r#"{{"v":1,"tier":"pro","issued":1760000000,"order":"{order}"}}"#)
}
