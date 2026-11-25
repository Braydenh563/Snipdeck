//! Where a payload's bytes live: inline in the database row, or in the blob folder.

use core::fmt;
use core::str::FromStr;

use crate::Error;

/// Payloads up to this many bytes are stored inline; larger ones go to the content-addressed
/// blob folder (ADR-0002). The rule is size-only, so the same bytes always get the same
/// [`Content`], whatever their format.
pub const INLINE_LIMIT: usize = 64 * 1024;

/// The bytes of one format payload, or a reference to them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// At most [`INLINE_LIMIT`] bytes, held directly.
    Inline(Inline),
    /// More than [`INLINE_LIMIT`] bytes, held in the blob store.
    Blob {
        /// The BLAKE3 hash naming the blob.
        blob: BlobRef,
        /// The blob's length in bytes.
        len: u64,
    },
}

impl Content {
    /// The payload's length in bytes.
    pub fn len(&self) -> u64 {
        match self {
            // `usize` is at most 64 bits on every supported target.
            Self::Inline(inline) => inline.0.len() as u64,
            Self::Blob { len, .. } => *len,
        }
    }

    /// Whether the payload has no bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Bytes small enough to store inline: at most [`INLINE_LIMIT`].
///
/// `Debug` prints only the length, so sensitive items never end up in logs.
#[derive(Clone, PartialEq, Eq)]
pub struct Inline(Vec<u8>);

impl Inline {
    /// Wraps `bytes` if they fit within [`INLINE_LIMIT`].
    ///
    /// # Errors
    ///
    /// [`Error::InlineTooLarge`] if they don't; store them as a blob instead.
    pub fn new(bytes: Vec<u8>) -> Result<Self, Error> {
        if bytes.len() > INLINE_LIMIT {
            return Err(Error::InlineTooLarge {
                len: bytes.len(),
                max: INLINE_LIMIT,
            });
        }
        Ok(Self(bytes))
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Unwraps the bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Debug for Inline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Inline({} bytes)", self.0.len())
    }
}

/// The BLAKE3 hash of a blob's bytes, which is also its name in the blob folder.
///
/// Displays and parses as 64 lowercase hex digits; that is the only accepted spelling, so a
/// hash has exactly one file name. Hashing happens in `snipdeck-store`, which owns the bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlobRef([u8; 32]);

impl BlobRef {
    /// Wraps a 32-byte BLAKE3 digest.
    pub const fn from_bytes(hash: [u8; 32]) -> Self {
        Self(hash)
    }

    /// The raw digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for BlobRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for BlobRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlobRef({self})")
    }
}

impl FromStr for BlobRef {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let invalid = || Error::Parse {
            what: "blob hash",
            value: s.to_owned(),
        };
        let nibble = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        let digits = s.as_bytes();
        if digits.len() != 64 {
            return Err(invalid());
        }
        let mut hash = [0; 32];
        for (byte, &[high, low]) in hash.iter_mut().zip(digits.as_chunks::<2>().0) {
            let (high, low) = nibble(high).zip(nibble(low)).ok_or_else(invalid)?;
            *byte = high << 4 | low;
        }
        Ok(Self(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_accepts_up_to_the_limit() {
        assert_eq!(
            Inline::new(vec![0; INLINE_LIMIT]).unwrap().as_bytes().len(),
            INLINE_LIMIT
        );
        assert_eq!(
            Inline::new(vec![0; INLINE_LIMIT + 1]),
            Err(Error::InlineTooLarge {
                len: INLINE_LIMIT + 1,
                max: INLINE_LIMIT
            })
        );
    }

    #[test]
    fn inline_debug_hides_the_bytes() {
        let secret = Inline::new(b"hunter2".to_vec()).unwrap();
        assert_eq!(format!("{secret:?}"), "Inline(7 bytes)");
    }

    #[test]
    fn content_len_covers_both_representations() {
        let inline = Content::Inline(Inline::new(b"abc".to_vec()).unwrap());
        assert_eq!(inline.len(), 3);
        let blob = Content::Blob {
            blob: BlobRef::from_bytes([7; 32]),
            len: 1 << 20,
        };
        assert_eq!(blob.len(), 1 << 20);
        assert_eq!(Content::Inline(Inline::new(Vec::new()).unwrap()).len(), 0);
    }

    #[test]
    fn blob_ref_hex_round_trips() {
        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(i * 8 + 1).unwrap();
        }
        let blob = BlobRef::from_bytes(bytes);
        let hex = blob.to_string();
        assert_eq!(&hex[..6], "010911");
        assert_eq!(hex.len(), 64);
        assert_eq!(hex.parse::<BlobRef>(), Ok(blob));
    }

    #[test]
    fn blob_ref_rejects_non_canonical_hex() {
        let upper = "AB".repeat(32);
        let short = "ab".repeat(31);
        let junk = "zz".repeat(32);
        for bad in [upper.as_str(), short.as_str(), junk.as_str(), ""] {
            assert!(matches!(
                bad.parse::<BlobRef>(),
                Err(Error::Parse {
                    what: "blob hash",
                    ..
                })
            ));
        }
    }
}
