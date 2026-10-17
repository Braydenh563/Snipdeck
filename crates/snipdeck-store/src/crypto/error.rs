//! Error types. Messages describe what failed, never the data involved.

/// Everything that can go wrong in [`crypto`](super).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    /// The bytes are not a `SNENC1` envelope (wrong header, or too short to be one).
    #[error("data is not an encrypted item")]
    NotEncrypted,
    /// Authentication failed: the key is wrong, the item id is wrong, or the data was changed.
    /// These cases are deliberately indistinguishable.
    #[error("decryption failed: wrong key, wrong item, or the data was modified")]
    AuthenticationFailed,
    /// The stored key record is not a valid `SNKEY1` record.
    #[error("the stored key record is damaged or from a newer version")]
    InvalidKeyRecord,
    /// The key record is protected by a master password and none was given.
    #[error("a master password is required")]
    PasswordRequired,
    /// A master password was given but the key record has none.
    #[error("no master password is set")]
    NoPasswordSet,
    /// The master password is wrong.
    #[error("the master password is wrong")]
    WrongPassword,
    /// A master password must not be empty.
    #[error("the master password is empty")]
    EmptyPassword,
    /// Argon2id parameters outside the supported range.
    #[error("unsupported password hashing parameters")]
    InvalidKdfParams,
    /// The OS-bound key provider failed.
    #[error("the OS key provider failed: {0}")]
    Provider(#[from] KeyProviderError),
    /// The operating system could not supply random bytes.
    #[error("the operating system could not provide random bytes")]
    Random,
    /// An internal primitive refused its input (for example, a message too large).
    #[error("encryption failed")]
    Primitive,
}

/// Why a [`KeyProvider`](super::KeyProvider) could not wrap or unwrap a key.
///
/// The text must never contain key material; providers pass OS error codes or short fixed
/// descriptions only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyProviderError {
    /// The blob was not produced by this provider for this user, or was modified.
    #[error("the wrapped key was rejected (different user or machine, or damaged)")]
    Rejected,
    /// The OS facility is unavailable or failed.
    #[error("key protection is unavailable: {0}")]
    Unavailable(String),
}
