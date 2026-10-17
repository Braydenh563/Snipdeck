//! The public key Snipdeck builds trust.
//!
//! The key is chosen when the crate is *built* and is a plain constant afterwards; it is
//! deliberately never read from the environment or from disk at run time, because a key an
//! attacker can point at their own file is no check at all.
//!
//! `build.rs` takes the production key from the `SNIPDECK_LICENCE_PUBLIC_KEY` environment
//! variable (base64 of the 32 raw bytes) and falls back to the development key in debug
//! builds only. A release build with no production key does not compile, so the development
//! key cannot ship by forgetting a step. The release procedure is in `docs/licensing.md`.

// `LICENCE_PUBLIC_KEY` and `IS_DEVELOPMENT_KEY`, written by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/licence_key.rs"));

/// The development public key: the verifying key of the published development seed
/// (the ASCII text `snipdeck development licence key`).
///
/// Anybody can mint a licence for a build that embeds this key. Debug builds embed it by
/// default; release builds refuse to (see the module docs).
pub const DEVELOPMENT_PUBLIC_KEY: [u8; 32] = include!("development_key.in");

// A second, independent line of defence next to `build.rs`: a build without debug assertions
// (that is, a release-like build, however it was configured) that still embeds the
// development key does not compile.
#[cfg(not(debug_assertions))]
const _: () = assert!(
    !IS_DEVELOPMENT_KEY,
    "a release build must not embed the development licence key; see docs/licensing.md"
);

#[cfg(test)]
mod tests {
    use ed25519_dalek::VerifyingKey;

    use super::*;

    #[test]
    fn the_flag_matches_the_key() {
        assert_eq!(
            IS_DEVELOPMENT_KEY,
            LICENCE_PUBLIC_KEY == DEVELOPMENT_PUBLIC_KEY
        );
    }

    #[test]
    fn the_compiled_in_key_is_a_valid_curve_point() {
        assert!(VerifyingKey::from_bytes(&LICENCE_PUBLIC_KEY).is_ok());
    }

    #[test]
    fn release_builds_never_embed_the_development_key() {
        if !cfg!(debug_assertions) {
            // `black_box`: the value is a constant, and that is the point of checking it.
            assert!(!std::hint::black_box(IS_DEVELOPMENT_KEY));
        }
    }
}
