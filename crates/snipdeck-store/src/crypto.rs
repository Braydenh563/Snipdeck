//! Encryption of sensitive items at rest (SPEC §5.7, Pro).
//!
//! # Key hierarchy
//!
//! ```text
//! item content ──XChaCha20-Poly1305──▶ SNENC1 envelope        (key: the data key, DEK)
//! DEK ──(optional) Argon2id master password──▶ password-sealed DEK
//!     ──KeyProvider (OS-bound, DPAPI on Windows)──▶ SNKEY1 key record, stored by the app
//! ```
//!
//! * [`DataKey`] is 32 random bytes that encrypt items. It never leaves memory unwrapped and
//!   never changes when the master password does.
//! * [`KeyProvider`] binds the key record to the OS user (Windows DPAPI in milestone 2).
//! * The optional master password adds a second factor *inside* the OS binding, so an
//!   attacker needs both the user's OS session and the password, and cannot brute-force the
//!   password offline from a stolen copy of the record.
//! * [`KeyRing`] creates, unlocks and re-wraps the record; [`encrypt_item`] and
//!   [`decrypt_item`] do the per-item work.
//!
//! All key material is held in zeroizing buffers and no type here prints it in `Debug`.

mod aead;
mod envelope;
mod error;
mod key;
mod keyring;
mod provider;

pub use envelope::{decrypt_item, encrypt_item, is_envelope};
pub use error::{CryptoError, KeyProviderError};
pub use key::DataKey;
pub use keyring::{KdfParams, KeyRecord, KeyRing};
pub use provider::{KeyProvider, MemoryKeyProvider};
