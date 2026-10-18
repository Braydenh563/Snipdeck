//! Offline Ed25519 licence verification returning [`Entitlements`] (ADR-0005).
//!
//! A licence key is a token `SNIP1.<payload>.<signature>`: `payload` is compact JSON and both
//! parts are base64url without padding. The signature is Ed25519 over the ASCII text
//! `SNIP1.<payload>` exactly as written in the token, which binds the format prefix and means
//! no re-encoding is ever needed to check it. `docs/licensing.md` has the full format, how
//! the production key is made and rotated, and the test vectors.
//!
//! Verification is pure: no network, no clock. Licences are perpetual in format v1; the
//! `issued` time is kept so a later revocation list can refuse keys issued before a cut-off,
//! which is why [`verify`] already takes `now`.
//!
//! The order of checks matters for safety: the token is split and the signature checked
//! **before** any JSON is parsed, so unauthenticated input only ever meets the base64 decoder
//! and the signature check, and every failure after that point is about signed data.

#[cfg(test)]
mod key_selection;
mod keys;
// The signer is only compiled for the `snipdeck-keygen` tool and for tests; the app never links it.
#[cfg(any(test, feature = "keygen"))]
pub mod signing;
#[cfg(test)]
mod testing;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
pub use ed25519_dalek::VerifyingKey;
use ed25519_dalek::{SIGNATURE_LENGTH, Signature};
pub use keys::{DEVELOPMENT_PUBLIC_KEY, IS_DEVELOPMENT_KEY, LICENCE_PUBLIC_KEY};
use serde::Deserialize;

use crate::{Entitlements, Feature, Tier, Timestamp};

/// The only token format this build understands.
const FORMAT_VERSION: u64 = 1;
/// Tokens are about 250 bytes; anything much longer is not one and is not decoded.
const MAX_TOKEN_BYTES: usize = 4096;
/// Longest order id accepted in a signed payload.
const MAX_ORDER_BYTES: usize = 128;

/// Why a licence token was not accepted.
///
/// Messages never contain any part of the token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LicenceError {
    /// The text is not shaped like a licence token, or its signed payload is not a licence.
    #[error("malformed licence key: {0}")]
    Malformed(&'static str),
    /// The token is well formed but was not signed by the key this build trusts.
    #[error("licence key signature is not valid")]
    BadSignature,
    /// The token uses a format version this build does not understand; update Snipdeck.
    #[error("licence key format version {0} is not supported")]
    UnsupportedVersion(u64),
    /// The token is validly signed but is not for the Pro tier.
    #[error("licence key is not for Snipdeck Pro")]
    WrongTier,
}

/// A licence that passed verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Licence {
    order: String,
    issued: Timestamp,
    email_hash: Option<String>,
    features: Vec<Feature>,
}

impl Licence {
    /// The tier this licence grants: always Pro in format v1.
    pub const fn tier(&self) -> Tier {
        Tier::Pro
    }

    /// What the licence unlocks.
    ///
    /// Pro unlocks every [`Feature`], so the token's `features` list does not narrow it; see
    /// [`Licence::features`].
    pub const fn entitlements(&self) -> Entitlements {
        Entitlements::new(Tier::Pro)
    }

    /// The seller's order id, for support and a future revocation list.
    pub fn order(&self) -> &str {
        &self.order
    }

    /// When the licence was issued (signed data, not the local clock).
    pub const fn issued(&self) -> Timestamp {
        self.issued
    }

    /// The buyer's hashed email, if the seller put one in the token.
    pub fn email_hash(&self) -> Option<&str> {
        self.email_hash.as_deref()
    }

    /// Features the token names explicitly. Informational in v1 (Pro already allows all of
    /// them); keys this build does not know are left out so newer tokens still verify.
    pub fn features(&self) -> &[Feature] {
        &self.features
    }
}

/// The signed payload. Unknown fields are ignored so a later minor addition does not break
/// older builds; they are signed, so they are trusted.
#[derive(Deserialize)]
struct Payload {
    tier: String,
    issued: i64,
    order: String,
    #[serde(default)]
    email_hash: Option<String>,
    #[serde(default)]
    features: Vec<String>,
}

/// Just enough of the payload to dispatch on the format version.
#[derive(Deserialize)]
struct VersionOnly {
    v: u64,
}

/// Verifies `token` against `key` and returns the full [`Licence`].
///
/// # Errors
///
/// See [`LicenceError`]. Hostile input of any length or content returns an error; it never
/// panics.
pub fn verify_licence(token: &str, key: &VerifyingKey) -> Result<Licence, LicenceError> {
    // Pasting from a browser or chat app brings NBSP and the like; a BOM is not whitespace.
    let token = token.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if token.len() > MAX_TOKEN_BYTES {
        return Err(LicenceError::Malformed("too long"));
    }
    let (head, signature_b64) = token
        .rsplit_once('.')
        .ok_or(LicenceError::Malformed("expected three parts"))?;
    let (prefix, payload_b64) = head
        .split_once('.')
        .ok_or(LicenceError::Malformed("expected three parts"))?;
    if payload_b64.is_empty() || payload_b64.contains('.') {
        return Err(LicenceError::Malformed("expected three parts"));
    }
    check_prefix(prefix)?;

    let signature = decode_signature(signature_b64)?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| LicenceError::Malformed("payload is not base64url"))?;
    // `verify_strict` also rejects the malleable and small-order encodings plain `verify`
    // tolerates; the library compares in constant time.
    key.verify_strict(head.as_bytes(), &signature)
        .map_err(|_| LicenceError::BadSignature)?;

    // From here on the payload is authentic.
    let VersionOnly { v } = serde_json::from_slice(&payload)
        .map_err(|_| LicenceError::Malformed("payload is not a licence"))?;
    if v != FORMAT_VERSION {
        return Err(LicenceError::UnsupportedVersion(v));
    }
    let payload: Payload = serde_json::from_slice(&payload)
        .map_err(|_| LicenceError::Malformed("payload is not a licence"))?;
    if payload.tier != "pro" {
        return Err(LicenceError::WrongTier);
    }
    if payload.order.is_empty() || payload.order.len() > MAX_ORDER_BYTES {
        return Err(LicenceError::Malformed("order id is empty or too long"));
    }
    let issued = Timestamp::from_second(payload.issued)
        .map_err(|_| LicenceError::Malformed("issue time is out of range"))?;
    Ok(Licence {
        order: payload.order,
        issued,
        email_hash: payload.email_hash,
        features: payload
            .features
            .iter()
            .filter_map(|key| key.parse().ok())
            .collect(),
    })
}

/// Verifies `token` against `key` and returns what it entitles the user to.
///
/// `now` is unused in format v1 (licences never expire) and exists so callers are already
/// passing the time when a revocation list arrives.
///
/// # Errors
///
/// See [`LicenceError`].
pub fn verify(
    token: &str,
    key: &VerifyingKey,
    _now: Timestamp,
) -> Result<Entitlements, LicenceError> {
    verify_licence(token, key).map(|licence| licence.entitlements())
}

/// Like [`verify`], against the public key compiled into this build ([`LICENCE_PUBLIC_KEY`]).
///
/// # Errors
///
/// See [`LicenceError`]; also [`LicenceError::Malformed`] if the compiled-in key is not a
/// valid curve point, which a unit test rules out for every build.
pub fn verify_embedded(token: &str, now: Timestamp) -> Result<Entitlements, LicenceError> {
    let key = VerifyingKey::from_bytes(&LICENCE_PUBLIC_KEY)
        .map_err(|_| LicenceError::Malformed("compiled-in public key is invalid"))?;
    verify(token, &key, now)
}

/// Accepts `SNIP1`; reports `SNIP<n>` for other numbers as an unsupported version and
/// anything else as malformed.
fn check_prefix(prefix: &str) -> Result<(), LicenceError> {
    if prefix == "SNIP1" {
        return Ok(());
    }
    match prefix.strip_prefix("SNIP") {
        Some(digits)
            if !digits.is_empty()
                && digits.len() <= 18
                && digits.bytes().all(|b| b.is_ascii_digit())
                // Canonical numbers only: `SNIP01` is not another spelling of `SNIP1`.
                && (digits == "0" || !digits.starts_with('0')) =>
        {
            digits
                .parse()
                .map_or(Err(LicenceError::Malformed("bad version")), |version| {
                    Err(LicenceError::UnsupportedVersion(version))
                })
        }
        _ => Err(LicenceError::Malformed("not a Snipdeck licence key")),
    }
}

fn decode_signature(text: &str) -> Result<Signature, LicenceError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| LicenceError::Malformed("signature is not base64url"))?;
    let bytes: [u8; SIGNATURE_LENGTH] = bytes
        .try_into()
        .map_err(|_| LicenceError::Malformed("signature has the wrong length"))?;
    Ok(Signature::from_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    use super::signing::sign_token as sign;
    use super::testing::{dev_signing_key, other_signing_key, payload};
    use super::*;
    use crate::Tier;

    const NOW: Timestamp = Timestamp::UNIX_EPOCH;

    fn dev_key() -> VerifyingKey {
        dev_signing_key().verifying_key()
    }

    fn valid_token() -> String {
        sign(&dev_signing_key(), &payload("ORD-1"))
    }

    fn check(token: &str) -> Result<Licence, LicenceError> {
        verify_licence(token, &dev_key())
    }

    #[test]
    fn valid_token_grants_pro_with_every_feature() {
        let entitlements = verify(&valid_token(), &dev_key(), NOW).unwrap();
        assert_eq!(entitlements.tier(), Tier::Pro);
        for &feature in Feature::ALL {
            assert!(entitlements.allows(feature));
        }
    }

    #[test]
    fn licence_exposes_the_signed_fields() {
        let json = r#"{"v":1,"tier":"pro","email_hash":"ab12","issued":1760000000,"order":"ORD-9","features":["paste-stack","from-the-future"]}"#;
        let licence = check(&sign(&dev_signing_key(), json)).unwrap();
        assert_eq!(licence.order(), "ORD-9");
        assert_eq!(
            licence.issued(),
            Timestamp::from_second(1_760_000_000).unwrap()
        );
        assert_eq!(licence.email_hash(), Some("ab12"));
        // Feature keys this build does not know are skipped, not fatal.
        assert_eq!(licence.features(), &[Feature::PasteStack]);
        assert_eq!(licence.entitlements().tier(), Tier::Pro);
    }

    #[test]
    fn optional_fields_may_be_absent() {
        let licence = check(&valid_token()).unwrap();
        assert_eq!(licence.email_hash(), None);
        assert_eq!(licence.features(), []);
    }

    #[test]
    fn surrounding_whitespace_from_pasting_is_ignored() {
        let token = format!("  \n{}\r\n\t", valid_token());
        assert!(check(&token).is_ok());
    }

    #[test]
    fn pasted_unicode_whitespace_and_a_byte_order_mark_are_ignored() {
        // Browsers and chat apps put NBSP, ideographic spaces and a BOM around copied text.
        for (before, after) in [
            ("\u{a0}", "\u{a0}"),
            ("\u{feff}", ""),
            ("\u{3000}\u{feff}", "\u{2003}\u{feff}\n"),
            ("\u{feff} ", " \u{feff}"),
        ] {
            let token = format!("{before}{}{after}", valid_token());
            assert!(check(&token).is_ok(), "{before:?} {after:?}");
        }
    }

    #[test]
    fn only_surrounding_whitespace_is_ignored() {
        let good = valid_token();
        let (head, tail) = good.split_at(10);
        assert!(matches!(
            check(&format!("{head}\u{a0}{tail}")),
            Err(LicenceError::Malformed(_))
        ));
    }

    #[test]
    fn version_digits_have_no_leading_zeros() {
        let parts = valid_token();
        let rest = parts.split_once('.').unwrap().1;
        for prefix in ["SNIP01", "SNIP001", "SNIP00", "SNIP0100"] {
            assert_eq!(
                check(&format!("{prefix}.{rest}")),
                Err(LicenceError::Malformed("not a Snipdeck licence key")),
                "{prefix}"
            );
        }
        // Real version numbers still report as unsupported, including 0.
        for (prefix, version) in [("SNIP0", 0), ("SNIP2", 2), ("SNIP10", 10)] {
            assert_eq!(
                check(&format!("{prefix}.{rest}")),
                Err(LicenceError::UnsupportedVersion(version)),
                "{prefix}"
            );
        }
    }

    #[test]
    fn structurally_broken_tokens_are_malformed() {
        let good = valid_token();
        let parts: Vec<&str> = good.split('.').collect();
        let cases = [
            String::new(),
            "SNIP1".to_owned(),
            "garbage".to_owned(),
            format!("SNIP1.{}", parts[1]),
            format!("{good}.extra"),
            format!("SNIP1..{}", parts[2]),
            format!("SNIP1.{}.", parts[1]),
            format!("SNIP1.!!!.{}", parts[2]),
            format!("SNIP1.{}.!!!", parts[1]),
            format!("snip1.{}.{}", parts[1], parts[2]),
            format!("OTHER.{}.{}", parts[1], parts[2]),
            // Padding is not part of the format.
            format!("SNIP1.{}=.{}", parts[1], parts[2]),
            // Signature one byte short / long.
            format!("SNIP1.{}.{}", parts[1], URL_SAFE_NO_PAD.encode([0_u8; 63])),
            format!("SNIP1.{}.{}", parts[1], URL_SAFE_NO_PAD.encode([0_u8; 65])),
            "\u{0}\u{1}\u{2}".to_owned(),
            "SNIP1.é.é".to_owned(),
        ];
        for case in cases {
            assert!(
                matches!(check(&case), Err(LicenceError::Malformed(_))),
                "{case:?} -> {:?}",
                check(&case)
            );
        }
    }

    #[test]
    fn oversized_tokens_are_rejected_without_being_decoded() {
        let token = format!("SNIP1.{}.{}", "A".repeat(100_000), "A".repeat(86));
        assert!(matches!(check(&token), Err(LicenceError::Malformed(_))));
    }

    #[test]
    fn signed_payloads_that_are_not_valid_licences_are_malformed() {
        let key = dev_signing_key();
        let cases = [
            "not json",
            "[]",
            "null",
            "{}",
            r#"{"v":1}"#,
            r#"{"v":1,"tier":"pro","issued":1,"order":""}"#,
            r#"{"v":1,"tier":"pro","issued":"yesterday","order":"o"}"#,
            r#"{"v":1,"tier":"pro","issued":1.5,"order":"o"}"#,
            r#"{"v":1,"tier":"pro","order":"o"}"#,
            r#"{"v":1,"tier":"pro","issued":1,"order":7}"#,
            r#"{"v":1,"tier":"pro","issued":1,"order":"o","features":"all"}"#,
            r#"{"v":1,"tier":"pro","issued":9223372036854775807,"order":"o"}"#,
            r#"{"v":"1","tier":"pro","issued":1,"order":"o"}"#,
            r#"{"v":-1,"tier":"pro","issued":1,"order":"o"}"#,
        ];
        for case in cases {
            let result = check(&sign(&key, case));
            assert!(
                matches!(result, Err(LicenceError::Malformed(_))),
                "{case} -> {result:?}"
            );
        }
    }

    #[test]
    fn newer_format_versions_are_reported_as_unsupported() {
        let key = dev_signing_key();
        // Signed payload that says v2, whatever else it holds.
        let v2 = sign(&key, r#"{"v":2,"something":"new"}"#);
        assert_eq!(check(&v2), Err(LicenceError::UnsupportedVersion(2)));
        let v0 = sign(&key, r#"{"v":0,"tier":"pro","issued":1,"order":"o"}"#);
        assert_eq!(check(&v0), Err(LicenceError::UnsupportedVersion(0)));
        // A future token prefix is recognised as such before the signature is looked at.
        let good = valid_token();
        let rest = good.strip_prefix("SNIP1").unwrap();
        assert_eq!(
            check(&format!("SNIP2{rest}")),
            Err(LicenceError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn other_tiers_are_rejected_after_the_signature_checks_out() {
        let key = dev_signing_key();
        for tier in ["free", "Pro", "enterprise", ""] {
            let json = format!(r#"{{"v":1,"tier":"{tier}","issued":1,"order":"o"}}"#);
            assert_eq!(
                check(&sign(&key, &json)),
                Err(LicenceError::WrongTier),
                "{tier}"
            );
        }
        // The same payload signed by a stranger is a bad signature, not a wrong tier.
        let forged = sign(
            &other_signing_key(),
            r#"{"v":1,"tier":"free","issued":1,"order":"o"}"#,
        );
        assert_eq!(check(&forged), Err(LicenceError::BadSignature));
    }

    #[test]
    fn a_token_signed_by_another_key_is_a_bad_signature() {
        let forged = sign(&other_signing_key(), &payload("ORD-1"));
        assert_eq!(check(&forged), Err(LicenceError::BadSignature));
    }

    #[test]
    fn unauthenticated_payloads_are_never_parsed() {
        // Garbage "JSON" under a wrong signature must not leak a parse error.
        let forged = sign(&other_signing_key(), "not json at all");
        assert_eq!(check(&forged), Err(LicenceError::BadSignature));
        let forged_v2 = sign(&other_signing_key(), r#"{"v":2}"#);
        assert_eq!(check(&forged_v2), Err(LicenceError::BadSignature));
    }

    #[test]
    fn changing_any_payload_or_signature_byte_invalidates_the_token() {
        let good = valid_token();
        let (head, signature) = good.rsplit_once('.').unwrap();
        let payload_b64 = head.strip_prefix("SNIP1.").unwrap();
        // Replace each character with a different valid base64url character.
        for (segment_start, text) in [(6, payload_b64), (head.len() + 1, signature)] {
            for index in 0..text.len() {
                let original = text.as_bytes()[index];
                let replacement = if original == b'A' { b'B' } else { b'A' };
                let mut bytes = good.clone().into_bytes();
                bytes[segment_start + index] = replacement;
                let tampered = String::from_utf8(bytes).unwrap();
                assert!(
                    check(&tampered).is_err(),
                    "byte {index} of segment at {segment_start}"
                );
            }
        }
    }

    #[test]
    fn a_signature_does_not_transfer_between_payloads() {
        let a = sign(&dev_signing_key(), &payload("A"));
        let b = sign(&dev_signing_key(), &payload("B"));
        let sig_a = a.rsplit_once('.').unwrap().1;
        let head_b = b.rsplit_once('.').unwrap().0;
        assert_eq!(
            check(&format!("{head_b}.{sig_a}")),
            Err(LicenceError::BadSignature)
        );
    }

    #[test]
    fn the_signature_must_cover_the_prefix_and_the_encoded_payload() {
        use ed25519_dalek::Signer as _;
        // A signature over just the decoded payload bytes (a plausible but different format)
        // is not accepted.
        let json = payload("ORD-1");
        let signature = dev_signing_key().sign(json.as_bytes());
        let token = format!(
            "SNIP1.{}.{}",
            URL_SAFE_NO_PAD.encode(&json),
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        );
        assert_eq!(check(&token), Err(LicenceError::BadSignature));
    }

    #[test]
    fn the_development_public_key_matches_the_published_seed() {
        assert_eq!(dev_key().to_bytes(), DEVELOPMENT_PUBLIC_KEY);
    }

    #[test]
    fn embedded_key_accepts_development_tokens_only_in_a_development_build() {
        // A build with a production key (every release) must reject tokens signed with the
        // public development seed; a development build accepts exactly those.
        if IS_DEVELOPMENT_KEY {
            assert_eq!(
                verify_embedded(&valid_token(), NOW).unwrap().tier(),
                Tier::Pro
            );
        } else {
            assert_eq!(
                verify_embedded(&valid_token(), NOW),
                Err(LicenceError::BadSignature)
            );
        }
        let forged = sign(&other_signing_key(), &payload("x"));
        assert_eq!(
            verify_embedded(&forged, NOW),
            Err(LicenceError::BadSignature)
        );
    }

    #[test]
    fn errors_never_echo_the_token() {
        let secret = "SECRET-ORDER-TEXT";
        let token = sign(&other_signing_key(), &payload(secret));
        let message = format!("{} {:?}", check(&token).unwrap_err(), check(&token));
        assert!(!message.contains(secret));
        assert!(!message.contains(&token));
    }
}
