//! Which public key a build trusts: the policy behind `LICENCE_PUBLIC_KEY`.
//!
//! This file is compiled twice. `build.rs` runs [`select`] and writes the answer into
//! `OUT_DIR`, so the key is a compile-time constant; the library compiles it again, for
//! tests only, so the policy is unit tested with the rest of the crate. It therefore uses
//! nothing but `std`, `base64` and `ed25519-dalek`, which are both runtime and build
//! dependencies.
//!
//! The policy fails closed:
//!
//! * `SNIPDECK_LICENCE_PUBLIC_KEY` set to a valid key: that key, in every profile.
//! * Not set, debug profile: the public development key, so `cargo test` needs no set-up.
//! * Not set, release profile: **no build**. A release can never silently carry the
//!   development key, whose seed is public.
//! * The development key given explicitly, or anything that is not a 32-byte curve point:
//!   no build, in every profile.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::VerifyingKey;

/// The environment variable that carries the production public key at build time.
pub(crate) const ENV_VAR: &str = "SNIPDECK_LICENCE_PUBLIC_KEY";

/// The development public key: the verifying key of the published development seed.
pub(crate) const DEVELOPMENT_PUBLIC_KEY: [u8; 32] = include!("development_key.in");

/// The key a build embeds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedKey {
    /// The Ed25519 public key, 32 raw bytes.
    pub(crate) bytes: [u8; 32],
    /// Whether this is the development key.
    pub(crate) development: bool,
}

/// Picks the key from the value of [`ENV_VAR`] (`None` if unset) and the build profile.
///
/// # Errors
///
/// A message for the build log. It never contains the supplied value.
pub(crate) fn select(configured: Option<&str>, release: bool) -> Result<SelectedKey, String> {
    let configured = configured.map(str::trim).filter(|text| !text.is_empty());
    let Some(text) = configured else {
        return if release {
            Err(format!(
                "release builds must embed the production licence public key, and {ENV_VAR} is \
                 not set. Set it to the base64 of the 32-byte Ed25519 public key; see \
                 docs/licensing.md. The development key is never allowed in a release."
            ))
        } else {
            Ok(SelectedKey {
                bytes: DEVELOPMENT_PUBLIC_KEY,
                development: true,
            })
        };
    };
    let bad = |why: &str| format!("{ENV_VAR} is not a usable public key: {why}");
    // Accept the standard alphabet and padding too: that is what `base64` on a command line
    // and most key tools print.
    let url_safe: String = text
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            other => other,
        })
        .collect();
    let decoded = URL_SAFE_NO_PAD
        .decode(url_safe)
        .map_err(|_| bad("it is not base64"))?;
    let bytes: [u8; 32] = decoded
        .try_into()
        .map_err(|_| bad("it must decode to exactly 32 bytes"))?;
    if bytes == DEVELOPMENT_PUBLIC_KEY {
        return Err(format!(
            "{ENV_VAR} is the development key, whose private seed is public; use the \
             production key"
        ));
    }
    VerifyingKey::from_bytes(&bytes).map_err(|_| bad("it is not a valid Ed25519 point"))?;
    Ok(SelectedKey {
        bytes,
        development: false,
    })
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use ed25519_dalek::{SigningKey, VerifyingKey};

    use super::*;

    const DEBUG: bool = false;
    const RELEASE: bool = true;

    /// A real, valid public key that is not the development key.
    fn production_key() -> [u8; 32] {
        SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes()
    }

    fn not_a_curve_point() -> [u8; 32] {
        (0..=255_u8)
            .map(|b| [b; 32])
            .find(|bytes| VerifyingKey::from_bytes(bytes).is_err())
            .expect("some byte pattern is not a curve point")
    }

    #[test]
    fn debug_builds_without_a_configured_key_use_the_development_key() {
        let key = select(None, DEBUG).unwrap();
        assert_eq!(key.bytes, DEVELOPMENT_PUBLIC_KEY);
        assert!(key.development);
    }

    #[test]
    fn release_builds_without_a_configured_key_fail_and_say_how_to_fix_it() {
        let message = select(None, RELEASE).unwrap_err();
        assert!(message.contains(ENV_VAR), "{message}");
        assert!(message.contains("docs/licensing.md"), "{message}");
    }

    #[test]
    fn a_blank_variable_counts_as_unset_so_an_empty_ci_secret_cannot_ship_the_development_key() {
        for blank in ["", " ", "\n\t "] {
            assert!(select(Some(blank), RELEASE).is_err(), "{blank:?}");
            assert!(select(Some(blank), DEBUG).unwrap().development, "{blank:?}");
        }
    }

    #[test]
    fn a_configured_production_key_is_used_in_every_profile_in_either_alphabet() {
        let key = production_key();
        let encodings = [
            STANDARD.encode(key),
            URL_SAFE_NO_PAD.encode(key),
            format!("  {}\n", STANDARD.encode(key)),
        ];
        for profile in [DEBUG, RELEASE] {
            for text in &encodings {
                let selected = select(Some(text), profile).unwrap();
                assert_eq!(selected.bytes, key, "{text:?}");
                assert!(!selected.development);
            }
        }
    }

    #[test]
    fn the_development_key_is_refused_even_when_configured_explicitly() {
        for profile in [DEBUG, RELEASE] {
            for text in [
                STANDARD.encode(DEVELOPMENT_PUBLIC_KEY),
                URL_SAFE_NO_PAD.encode(DEVELOPMENT_PUBLIC_KEY),
            ] {
                let message = select(Some(&text), profile).unwrap_err();
                assert!(message.contains("development key"), "{message}");
            }
        }
    }

    #[test]
    fn malformed_keys_are_refused_without_echoing_them() {
        let short = STANDARD.encode([5_u8; 31]);
        let long = STANDARD.encode([5_u8; 33]);
        let off_curve = STANDARD.encode(not_a_curve_point());
        for text in [
            "not base64!!",
            "AAAA",
            short.as_str(),
            long.as_str(),
            off_curve.as_str(),
        ] {
            for profile in [DEBUG, RELEASE] {
                let message = select(Some(text), profile).unwrap_err();
                assert!(message.contains(ENV_VAR), "{text}: {message}");
                assert!(!message.contains(text), "{text}: {message}");
            }
        }
    }
}
