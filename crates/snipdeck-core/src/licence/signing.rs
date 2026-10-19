//! Minting licence tokens (the seller's side). Compiled for tests and for the
//! `snipdeck-licence-issuer` tool (cargo feature `licence-issuer`); the app never links it.
//!
//! The format is in `docs/licensing.md`. The private key is never in this repository: callers
//! pass it in.

use ed25519_dalek::Signer as _;
pub use ed25519_dalek::SigningKey;
use serde::Serialize;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// Longest order id [`validate_order_id`] accepts: the verifier's own limit.
pub const MAX_ORDER_BYTES: usize = super::MAX_ORDER_BYTES;

/// Why a token could not be minted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignError {
    /// The order id does not match the pattern in `docs/licensing.md`.
    #[error("order id must be 1 to 128 characters of A-Z a-z 0-9 . _ - (for example GIFT-alex)")]
    BadOrderId,
    /// The email hash is not lower-case hexadecimal.
    #[error("email hash must be 1 to 128 lower-case hexadecimal digits")]
    BadEmailHash,
}

/// The payload of a v1 Pro token, in the field order the documented vectors use.
#[derive(Serialize)]
struct PayloadOut<'a> {
    v: u64,
    tier: &'a str,
    issued: i64,
    order: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    email_hash: Option<&'a str>,
}

/// Checks an order id against the documented pattern: 1 to 128 bytes of `A-Z a-z 0-9 . _ -`.
///
/// The verifier accepts any 1 to 128 byte string; this is the stricter rule for ids the tool
/// issues, so they are safe to paste into chat, filenames and a future revocation list.
///
/// # Errors
///
/// [`SignError::BadOrderId`] when the id is empty, too long or has another character.
pub fn validate_order_id(order: &str) -> Result<(), SignError> {
    let ok = !order.is_empty()
        && order.len() <= MAX_ORDER_BYTES
        && order
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(SignError::BadOrderId)
    }
}

/// Builds the compact JSON payload of a v1 Pro licence.
///
/// `issued` is Unix seconds, supplied by the caller (no clock is read here).
///
/// # Errors
///
/// [`SignError`] when `order` or `email_hash` is malformed.
pub fn pro_payload(
    order: &str,
    issued: i64,
    email_hash: Option<&str>,
) -> Result<String, SignError> {
    validate_order_id(order)?;
    if let Some(hash) = email_hash
        && (hash.is_empty()
            || hash.len() > MAX_ORDER_BYTES
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(SignError::BadEmailHash);
    }
    let payload = PayloadOut {
        v: 1,
        tier: "pro",
        issued,
        order,
        email_hash,
    };
    // Serialising plain strings and integers cannot fail.
    Ok(serde_json::to_string(&payload).unwrap_or_default())
}

/// `SNIP1.<payload>.<signature>` with `payload_json` signed by `key`.
pub fn sign_token(key: &SigningKey, payload_json: &str) -> String {
    let head = format!("SNIP1.{}", URL_SAFE_NO_PAD.encode(payload_json));
    let signature = key.sign(head.as_bytes());
    format!("{head}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::licence::testing::dev_signing_key;
    use crate::licence::verify_licence;

    #[test]
    fn minimal_payload_matches_the_documented_vector() {
        let payload = pro_payload("ORD-1001", 1_760_000_000, None).unwrap();
        assert_eq!(
            payload,
            r#"{"v":1,"tier":"pro","issued":1760000000,"order":"ORD-1001"}"#
        );
        assert_eq!(
            sign_token(&dev_signing_key(), &payload),
            "SNIP1.eyJ2IjoxLCJ0aWVyIjoicHJvIiwiaXNzdWVkIjoxNzYwMDAwMDAwLCJvcmRlciI6Ik9SRC0xMDAxIn0.wvpQZiQQvgfQzNOTVrngZ0ehTw6UaUVb0MiAPH72Tc4KqdsLnT1Kq8rmfdbqXpvuGnaav0wuXAVfg_Xq4yW9AQ"
        );
    }

    #[test]
    fn email_hash_is_signed_and_verifies() {
        let key = dev_signing_key();
        let payload = pro_payload("GIFT-alex", 1, Some("ab01ff")).unwrap();
        let licence = verify_licence(&sign_token(&key, &payload), &key.verifying_key()).unwrap();
        assert_eq!(licence.order(), "GIFT-alex");
        assert_eq!(licence.email_hash(), Some("ab01ff"));
    }

    #[test]
    fn order_ids_follow_the_pattern() {
        for good in ["GIFT-alex", "a", "ORD_1.2-x", &"x".repeat(128)] {
            assert_eq!(validate_order_id(good), Ok(()), "{good}");
        }
        for bad in [
            "",
            "GIFT alex",
            "GIFT/alex",
            "gift-é",
            "a\"b",
            "a\nb",
            &"x".repeat(129),
        ] {
            assert_eq!(
                validate_order_id(bad),
                Err(SignError::BadOrderId),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn bad_email_hashes_are_refused() {
        for bad in ["", "XYZ", "AB01", "ab 01", &"a".repeat(129)] {
            assert_eq!(
                pro_payload("GIFT-a", 1, Some(bad)),
                Err(SignError::BadEmailHash),
                "{bad:?}"
            );
        }
    }
}
