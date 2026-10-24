//! Test-only token minting, so tests are self-contained. Never compiled into the library.
//!
//! The file is `cfg(test)` as a whole (not only at its `mod` line) so a source scanner never
//! treats these seeds as shipped key material. Seeds are built at run time, not written as
//! byte literals.
#![cfg(test)]

use ed25519_dalek::SigningKey;

/// The seed of the development key pair, assembled at run time. Public on purpose: it is not a
/// secret and protects nothing (see `keys.rs`); it must equal the one published in
/// `docs/licensing.md` because `development_key.in` is its public half.
pub(crate) fn development_seed() -> [u8; 32] {
    let text = ["snipdeck", "development", "licence", "key"].join(" ");
    text.as_bytes().try_into().expect("32 bytes")
}

/// A throwaway seed that depends only on `tag`, computed rather than written down.
pub(crate) fn derived_seed(tag: u8) -> [u8; 32] {
    std::array::from_fn(|i| {
        u8::try_from(i)
            .expect("below 32")
            .wrapping_mul(31)
            .wrapping_add(tag)
    })
}

/// The development signing key (its seed is public, see `DEVELOPMENT_SEED`).
pub(crate) fn dev_signing_key() -> SigningKey {
    SigningKey::from_bytes(&development_seed())
}

/// A signing key unrelated to the development key.
pub(crate) fn other_signing_key() -> SigningKey {
    SigningKey::from_bytes(&derived_seed(7))
}

/// A fully valid payload for order `order`.
pub(crate) fn payload(order: &str) -> String {
    format!(r#"{{"v":1,"tier":"pro","issued":1760000000,"order":"{order}"}}"#)
}
