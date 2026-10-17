//! Licence verification from the outside: published test vectors and hostile-input fuzzing.
//!
//! The vectors are minted with the DEVELOPMENT seed (public by design), so they double as
//! the format specification in `docs/licensing.md`: any other implementation of the signer
//! must reproduce them byte for byte (Ed25519 signatures are deterministic).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use proptest::prelude::*;
use snipdeck_core::licence::{
    DEVELOPMENT_PUBLIC_KEY, LICENCE_PUBLIC_KEY, LicenceError, verify, verify_embedded,
    verify_licence,
};
use snipdeck_core::{Feature, Tier, Timestamp};

const SEED: [u8; 32] = *b"snipdeck development licence key";

/// Vector 1: the minimal payload.
const MINIMAL_PAYLOAD: &str = r#"{"v":1,"tier":"pro","issued":1760000000,"order":"ORD-1001"}"#;
const MINIMAL_TOKEN: &str = "SNIP1.eyJ2IjoxLCJ0aWVyIjoicHJvIiwiaXNzdWVkIjoxNzYwMDAwMDAwLCJvcmRlciI6Ik9SRC0xMDAxIn0.wvpQZiQQvgfQzNOTVrngZ0ehTw6UaUVb0MiAPH72Tc4KqdsLnT1Kq8rmfdbqXpvuGnaav0wuXAVfg_Xq4yW9AQ";

/// Vector 2: every optional field.
const FULL_PAYLOAD: &str = r#"{"v":1,"tier":"pro","email_hash":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08","issued":1760000000,"order":"ORD-1002","features":["paste-stack","floating-lists"]}"#;
const FULL_TOKEN: &str = "SNIP1.eyJ2IjoxLCJ0aWVyIjoicHJvIiwiZW1haWxfaGFzaCI6IjlmODZkMDgxODg0YzdkNjU5YTJmZWFhMGM1NWFkMDE1YTNiZjRmMWIyYjBiODIyY2QxNWQ2YzE1YjBmMDBhMDgiLCJpc3N1ZWQiOjE3NjAwMDAwMDAsIm9yZGVyIjoiT1JELTEwMDIiLCJmZWF0dXJlcyI6WyJwYXN0ZS1zdGFjayIsImZsb2F0aW5nLWxpc3RzIl19.JqlGbYSb7J_4oPpnfW4NEaN4XfKpDN6uHA_D6egQTCFpi1UaiUoTnd_-VRMUnS5nJSU1bl2opdLbNWUUoLEeDg";

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&SEED)
}

fn verifying_key() -> VerifyingKey {
    signing_key().verifying_key()
}

fn mint(key: &SigningKey, payload: &str) -> String {
    let head = format!("SNIP1.{}", URL_SAFE_NO_PAD.encode(payload));
    let signature = key.sign(head.as_bytes());
    format!("{head}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

const NOW: Timestamp = Timestamp::UNIX_EPOCH;

#[test]
fn vectors_are_reproducible_from_the_published_seed() {
    assert_eq!(mint(&signing_key(), MINIMAL_PAYLOAD), MINIMAL_TOKEN);
    assert_eq!(mint(&signing_key(), FULL_PAYLOAD), FULL_TOKEN);
}

#[test]
fn the_compiled_in_key_is_the_published_development_key() {
    assert_eq!(verifying_key().to_bytes(), DEVELOPMENT_PUBLIC_KEY);
    assert_eq!(LICENCE_PUBLIC_KEY, DEVELOPMENT_PUBLIC_KEY);
}

#[test]
fn minimal_vector_verifies_as_pro() {
    let entitlements = verify(MINIMAL_TOKEN, &verifying_key(), NOW).unwrap();
    assert_eq!(entitlements.tier(), Tier::Pro);
    let licence = verify_licence(MINIMAL_TOKEN, &verifying_key()).unwrap();
    assert_eq!(licence.order(), "ORD-1001");
    assert_eq!(licence.issued().as_second(), 1_760_000_000);
    assert_eq!(licence.email_hash(), None);
    assert_eq!(licence.features(), []);
}

#[test]
fn full_vector_verifies_and_exposes_every_field() {
    let licence = verify_licence(FULL_TOKEN, &verifying_key()).unwrap();
    assert_eq!(licence.order(), "ORD-1002");
    assert_eq!(licence.issued().as_second(), 1_760_000_000);
    assert_eq!(
        licence.email_hash(),
        Some("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08")
    );
    assert_eq!(
        licence.features(),
        &[Feature::PasteStack, Feature::FloatingLists]
    );
}

#[test]
fn vectors_verify_against_the_compiled_in_key() {
    assert_eq!(
        verify_embedded(MINIMAL_TOKEN, NOW).unwrap().tier(),
        Tier::Pro
    );
    assert_eq!(verify_embedded(FULL_TOKEN, NOW).unwrap().tier(), Tier::Pro);
}

#[test]
fn vectors_do_not_verify_under_another_key() {
    let stranger = SigningKey::from_bytes(&[9; 32]).verifying_key();
    assert_eq!(
        verify(MINIMAL_TOKEN, &stranger, NOW),
        Err(LicenceError::BadSignature)
    );
}

#[test]
fn a_token_minted_by_a_stranger_is_rejected_by_the_compiled_in_key() {
    let forged = mint(&SigningKey::from_bytes(&[9; 32]), MINIMAL_PAYLOAD);
    assert_eq!(
        verify_embedded(&forged, NOW),
        Err(LicenceError::BadSignature)
    );
}

#[test]
fn swapping_in_a_different_payload_keeps_the_old_signature_invalid() {
    let (_, signature) = MINIMAL_TOKEN.rsplit_once('.').unwrap();
    let (full_head, _) = FULL_TOKEN.rsplit_once('.').unwrap();
    assert_eq!(
        verify_licence(&format!("{full_head}.{signature}"), &verifying_key()).map(|_| ()),
        Err(LicenceError::BadSignature)
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text_never_panics_and_never_verifies(text in any::<String>()) {
        prop_assert!(verify_licence(&text, &verifying_key()).is_err());
    }

    #[test]
    fn arbitrary_bytes_never_panic_and_never_verify(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        let text = String::from_utf8_lossy(&bytes);
        prop_assert!(verify_licence(&text, &verifying_key()).is_err());
    }

    #[test]
    fn token_shaped_garbage_never_verifies(
        prefix in prop_oneof![Just("SNIP1".to_owned()), "SNIP[0-9]{0,20}", "[A-Za-z0-9]{0,8}"],
        payload in "[A-Za-z0-9_=.-]{0,300}",
        signature in "[A-Za-z0-9_=.-]{0,120}",
    ) {
        let token = format!("{prefix}.{payload}.{signature}");
        prop_assert!(verify_licence(&token, &verifying_key()).is_err());
    }

    #[test]
    fn well_encoded_random_payloads_and_signatures_never_verify(
        payload in proptest::collection::vec(any::<u8>(), 1..300),
        signature in proptest::collection::vec(any::<u8>(), 64),
    ) {
        let token = format!(
            "SNIP1.{}.{}",
            URL_SAFE_NO_PAD.encode(&payload),
            URL_SAFE_NO_PAD.encode(&signature)
        );
        prop_assert_eq!(
            verify_licence(&token, &verifying_key()).map(|_| ()),
            Err(LicenceError::BadSignature)
        );
    }

    #[test]
    fn any_single_byte_change_to_a_valid_token_is_rejected(
        index in 0..FULL_TOKEN.len(),
        replacement in any::<u8>(),
    ) {
        let mut bytes = FULL_TOKEN.as_bytes().to_vec();
        prop_assume!(bytes[index] != replacement);
        bytes[index] = replacement;
        let mutated = String::from_utf8_lossy(&bytes);
        prop_assert!(verify_licence(&mutated, &verifying_key()).is_err());
    }

    #[test]
    fn truncating_or_extending_a_valid_token_is_rejected(
        cut in 0..FULL_TOKEN.len(),
        extra in "[A-Za-z0-9_.-]{1,20}",
    ) {
        prop_assert!(verify_licence(&FULL_TOKEN[..cut], &verifying_key()).is_err());
        let extended = format!("{FULL_TOKEN}{extra}");
        prop_assert!(verify_licence(&extended, &verifying_key()).is_err());
    }

    #[test]
    fn signed_arbitrary_payloads_never_panic(payload in proptest::collection::vec(any::<u8>(), 0..400)) {
        // Authentic but hostile: the JSON layer must cope with anything the seller could sign.
        let head = format!("SNIP1.{}", URL_SAFE_NO_PAD.encode(&payload));
        let signature = signing_key().sign(head.as_bytes());
        let token = format!("{head}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()));
        let _ = verify_licence(&token, &verifying_key());
    }
}
