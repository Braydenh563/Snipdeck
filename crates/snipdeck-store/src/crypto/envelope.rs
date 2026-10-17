//! The `SNENC1` per-item envelope.
//!
//! ```text
//! "SNENC1" (6) | nonce (24, random per message) | ciphertext | Poly1305 tag (16)
//! ```
//!
//! Associated data is the 16 raw bytes of the item id followed by the format byte `0x01`, so
//! a ciphertext copied onto another item, or re-labelled as another format version, fails
//! authentication.

use snipdeck_core::ItemId;
use zeroize::Zeroizing;

use super::CryptoError;
use super::aead::{NONCE_LEN, TAG_LEN, open, seal};
use super::key::DataKey;

const MAGIC: &[u8; 6] = b"SNENC1";
const FORMAT_BYTE: u8 = 1;

/// Encrypts `plaintext` for the item `item_id`.
///
/// # Errors
///
/// [`CryptoError::Random`] if no nonce could be drawn; [`CryptoError::Primitive`] if the
/// plaintext is too large for the cipher (never in practice).
pub fn encrypt_item(
    key: &DataKey,
    item_id: &ItemId,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let body = seal(key.expose(), &associated_data(item_id), plaintext)?;
    let mut out = Vec::with_capacity(MAGIC.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decrypts an envelope produced by [`encrypt_item`] for the same `item_id` and key.
///
/// # Errors
///
/// [`CryptoError::NotEncrypted`] if `envelope` does not start with the `SNENC1` header or is
/// too short; [`CryptoError::AuthenticationFailed`] for a wrong key, a wrong item id or any
/// modification.
pub fn decrypt_item(
    key: &DataKey,
    item_id: &ItemId,
    envelope: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if !is_envelope(envelope) {
        return Err(CryptoError::NotEncrypted);
    }
    open(
        key.expose(),
        &associated_data(item_id),
        &envelope[MAGIC.len()..],
    )
}

/// Whether `bytes` have the shape of an envelope: the `SNENC1` header and room for a nonce
/// and tag. Says nothing about whether it will authenticate.
pub fn is_envelope(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() + NONCE_LEN + TAG_LEN && bytes.starts_with(MAGIC)
}

fn associated_data(item_id: &ItemId) -> [u8; 17] {
    let mut aad = [0_u8; 17];
    aad[..16].copy_from_slice(item_id.as_uuid().as_bytes());
    aad[16] = FORMAT_BYTE;
    aad
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chacha20poly1305::aead::{Aead as _, Payload};
    use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce};

    use super::*;

    fn key() -> DataKey {
        DataKey::from_bytes([0x11; 32])
    }

    fn id() -> ItemId {
        ItemId::generate()
    }

    #[test]
    fn round_trips_every_size() {
        let id = id();
        for len in [0, 1, 15, 16, 17, 255, 4096, 1_000_000] {
            let plaintext: Vec<u8> = (0..len).map(|i| u8::try_from(i % 251).unwrap()).collect();
            let envelope = encrypt_item(&key(), &id, &plaintext).unwrap();
            assert_eq!(
                *decrypt_item(&key(), &id, &envelope).unwrap(),
                plaintext,
                "{len}"
            );
        }
    }

    #[test]
    fn envelope_layout_is_header_nonce_ciphertext_tag() {
        let envelope = encrypt_item(&key(), &id(), b"hello").unwrap();
        assert_eq!(&envelope[..6], b"SNENC1");
        assert_eq!(envelope.len(), 6 + 24 + 5 + 16);
        assert!(is_envelope(&envelope));
    }

    #[test]
    fn plaintext_does_not_appear_in_the_envelope() {
        let secret = b"correct horse battery staple";
        let envelope = encrypt_item(&key(), &id(), secret).unwrap();
        assert!(!envelope.windows(secret.len()).any(|w| w == secret));
    }

    #[test]
    fn encrypting_twice_gives_different_envelopes() {
        let id = id();
        let a = encrypt_item(&key(), &id, b"same").unwrap();
        let b = encrypt_item(&key(), &id, b"same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn wrong_key_fails_authentication() {
        let id = id();
        let envelope = encrypt_item(&key(), &id, b"secret").unwrap();
        let other = DataKey::from_bytes([0x22; 32]);
        assert_eq!(
            decrypt_item(&other, &id, &envelope).unwrap_err(),
            CryptoError::AuthenticationFailed
        );
    }

    #[test]
    fn envelope_cannot_be_moved_to_another_item() {
        let envelope = encrypt_item(&key(), &id(), b"secret").unwrap();
        assert_eq!(
            decrypt_item(&key(), &id(), &envelope).unwrap_err(),
            CryptoError::AuthenticationFailed
        );
    }

    #[test]
    fn the_format_byte_is_part_of_the_associated_data() {
        // An envelope sealed with the item id alone (no format byte) must not open.
        let id = id();
        let nonce = [5_u8; 24];
        let ciphertext = XChaCha20Poly1305::new((&[0x11_u8; 32]).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: b"secret",
                    aad: id.as_uuid().as_bytes(),
                },
            )
            .unwrap();
        let mut forged = b"SNENC1".to_vec();
        forged.extend_from_slice(&nonce);
        forged.extend_from_slice(&ciphertext);
        assert_eq!(
            decrypt_item(&key(), &id, &forged).unwrap_err(),
            CryptoError::AuthenticationFailed
        );
    }

    #[test]
    fn flipping_any_bit_of_any_byte_is_detected() {
        let id = id();
        let envelope = encrypt_item(&key(), &id, b"the quick brown fox").unwrap();
        for index in 0..envelope.len() {
            for mask in [0x01, 0x80, 0xFF] {
                let mut tampered = envelope.clone();
                tampered[index] ^= mask;
                let error = decrypt_item(&key(), &id, &tampered).unwrap_err();
                let expected = if index < 6 {
                    CryptoError::NotEncrypted // header
                } else {
                    CryptoError::AuthenticationFailed // nonce, ciphertext, tag
                };
                assert_eq!(error, expected, "byte {index} mask {mask:#x}");
            }
        }
    }

    #[test]
    fn truncation_and_extension_are_detected() {
        let id = id();
        let envelope = encrypt_item(&key(), &id, b"the quick brown fox").unwrap();
        for len in 0..envelope.len() {
            assert!(
                decrypt_item(&key(), &id, &envelope[..len]).is_err(),
                "len {len}"
            );
        }
        let mut longer = envelope;
        longer.push(0);
        assert!(decrypt_item(&key(), &id, &longer).is_err());
    }

    #[test]
    fn non_envelopes_are_not_encrypted() {
        for bytes in [
            &b""[..],
            b"SNENC1",
            b"plain text that is long enough to have a tag in it",
        ] {
            assert!(!is_envelope(bytes));
            assert_eq!(
                decrypt_item(&key(), &id(), bytes).unwrap_err(),
                CryptoError::NotEncrypted
            );
        }
    }

    #[test]
    fn nonces_do_not_repeat_over_100k_encryptions() {
        const COUNT: u32 = 100_000;
        let id = id();
        let mut seen = HashSet::new();
        let mut ones_per_bit = [0_u32; 24 * 8];
        for _ in 0..COUNT {
            let envelope = encrypt_item(&key(), &id, b"x").unwrap();
            let nonce: [u8; 24] = envelope[6..30].try_into().unwrap();
            for (byte_index, byte) in nonce.iter().enumerate() {
                for bit in 0..8 {
                    ones_per_bit[byte_index * 8 + bit] += u32::from((byte >> bit) & 1);
                }
            }
            assert!(seen.insert(nonce), "nonce repeated");
        }
        // Each nonce bit is a fair coin: ~50 000 ones, standard deviation ~158. A window of
        // +-1 000 is more than six sigma, so a false alarm is vanishingly unlikely while a
        // stuck or biased generator is caught.
        for (bit, ones) in ones_per_bit.iter().enumerate() {
            assert!(
                ones.abs_diff(COUNT / 2) < 1_000,
                "nonce bit {bit} set {ones} times"
            );
        }
    }
}
