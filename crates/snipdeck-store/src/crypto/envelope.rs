//! The `SNENC2` per-payload envelope.
//!
//! ```text
//! "SNENC2" (6) | nonce (24, random per message) | ciphertext | Poly1305 tag (16)
//! ```
//!
//! Associated data binds the envelope to where it is stored (review L8):
//!
//! ```text
//! version 0x02 | item id (16) | slot
//! slot: 0x00                                   detached (not stored in an item, e.g. an export)
//!     | 0x01 | key length (1) | format key     a single-payload format ("text", "html", ...)
//!     | 0x02 | file index (u32 LE)             one file of the `files` format
//! ```
//!
//! So a ciphertext copied onto another item, into another format or file of the same item,
//! or between a stored slot and a detached use, fails authentication. Version 1 (`SNENC1`,
//! item id only) never shipped in a release, so there is no reader for it: such bytes are
//! simply not an envelope.

use snipdeck_core::{FormatKind, ItemId};
use zeroize::Zeroizing;

use super::CryptoError;
use super::aead::{NONCE_LEN, TAG_LEN, open, seal};
use super::key::DataKey;

/// The header every envelope starts with.
const MAGIC: &[u8; 6] = b"SNENC2";

/// [`MAGIC`], for storage's integrity check.
pub(crate) const fn magic() -> &'static [u8; 6] {
    MAGIC
}
/// The envelope format version, first byte of the associated data.
const VERSION: u8 = 2;

/// Which payload of an item an envelope belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    /// The payload of a single-payload format (text, HTML, RTF, PNG).
    Format(FormatKind),
    /// The file at this index of the item's `files` format.
    File(u32),
    /// Not stored in an item, for example a payload in an export file.
    Detached,
}

/// Encrypts `plaintext` for the slot `slot` of the item `item_id`.
///
/// # Errors
///
/// [`CryptoError::Random`] if no nonce could be drawn; [`CryptoError::Primitive`] if the
/// plaintext is too large for the cipher (never in practice).
pub fn encrypt_payload(
    key: &DataKey,
    item_id: &ItemId,
    slot: Slot,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let body = seal(key.expose(), &associated_data(item_id, slot), plaintext)?;
    let mut out = Vec::with_capacity(MAGIC.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decrypts an envelope produced by [`encrypt_payload`] for the same item, slot and key.
///
/// # Errors
///
/// [`CryptoError::NotEncrypted`] if `envelope` does not start with the `SNENC2` header or is
/// too short; [`CryptoError::AuthenticationFailed`] for a wrong key, item or slot, or any
/// modification.
pub fn decrypt_payload(
    key: &DataKey,
    item_id: &ItemId,
    slot: Slot,
    envelope: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if !is_envelope(envelope) {
        return Err(CryptoError::NotEncrypted);
    }
    open(
        key.expose(),
        &associated_data(item_id, slot),
        &envelope[MAGIC.len()..],
    )
}

/// [`encrypt_payload`] for a [`Slot::Detached`] payload of `item_id`.
///
/// # Errors
///
/// As [`encrypt_payload`].
pub fn encrypt_item(
    key: &DataKey,
    item_id: &ItemId,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    encrypt_payload(key, item_id, Slot::Detached, plaintext)
}

/// [`decrypt_payload`] for a [`Slot::Detached`] payload of `item_id`.
///
/// # Errors
///
/// As [`decrypt_payload`].
pub fn decrypt_item(
    key: &DataKey,
    item_id: &ItemId,
    envelope: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    decrypt_payload(key, item_id, Slot::Detached, envelope)
}

/// Whether `bytes` have the shape of an envelope: the `SNENC2` header and room for a nonce
/// and tag. Says nothing about whether it will authenticate, and plaintext can have this
/// shape too: storage records which payloads are encrypted instead of guessing from bytes.
pub fn is_envelope(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() + NONCE_LEN + TAG_LEN && bytes.starts_with(MAGIC)
}

fn associated_data(item_id: &ItemId, slot: Slot) -> Vec<u8> {
    let mut aad = Vec::with_capacity(32);
    aad.push(VERSION);
    aad.extend_from_slice(item_id.as_uuid().as_bytes());
    match slot {
        Slot::Detached => aad.push(0),
        Slot::Format(kind) => {
            let key = kind.key().as_bytes();
            aad.push(1);
            // Keys are short ASCII words; the length prefix keeps the encoding unambiguous.
            aad.push(u8::try_from(key.len()).unwrap_or(u8::MAX));
            aad.extend_from_slice(key);
        }
        Slot::File(index) => {
            aad.push(2);
            aad.extend_from_slice(&index.to_le_bytes());
        }
    }
    aad
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chacha20poly1305::aead::{Aead as _, Payload};
    use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce};

    use super::super::key::test_bytes;
    use super::*;

    fn key() -> DataKey {
        DataKey::from_bytes(test_bytes(0x11))
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
        assert_eq!(&envelope[..6], b"SNENC2");
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
        let other = DataKey::from_bytes(test_bytes(0x22));
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

    /// Review L8: within one item, an envelope only opens in the slot it was made for.
    #[test]
    fn an_envelope_cannot_move_to_another_slot_of_the_same_item() {
        let id = id();
        let slots = [
            Slot::Format(FormatKind::Text),
            Slot::Format(FormatKind::Html),
            Slot::Format(FormatKind::Png),
            Slot::File(0),
            Slot::File(1),
            Slot::Detached,
        ];
        for made in slots {
            let envelope = encrypt_payload(&key(), &id, made, b"secret").unwrap();
            for opened in slots {
                let result = decrypt_payload(&key(), &id, opened, &envelope);
                if made == opened {
                    assert_eq!(*result.unwrap(), b"secret");
                } else {
                    assert_eq!(
                        result.unwrap_err(),
                        CryptoError::AuthenticationFailed,
                        "{made:?} opened as {opened:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_version_1_envelope_is_not_an_envelope() {
        let mut old = encrypt_item(&key(), &id(), b"secret").unwrap();
        old[..6].copy_from_slice(b"SNENC1");
        assert!(!is_envelope(&old));
        assert_eq!(
            decrypt_item(&key(), &id(), &old).unwrap_err(),
            CryptoError::NotEncrypted
        );
    }

    #[test]
    fn the_format_byte_is_part_of_the_associated_data() {
        // An envelope sealed with the item id alone (no format byte) must not open.
        let id = id();
        // A nonce that differs from the one the real code draws at random.
        let nonce: [u8; 24] = test_bytes(5);
        let ciphertext = XChaCha20Poly1305::new((&test_bytes::<32>(0x11)).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: b"secret",
                    aad: id.as_uuid().as_bytes(),
                },
            )
            .unwrap();
        let mut forged = b"SNENC2".to_vec();
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
            b"SNENC2",
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
