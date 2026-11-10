//! Windows Hello unlock, portable half (v1.1 slice 8, `docs/crypto.md` "Windows Hello").
//!
//! This module is only the OS seam: something that can sign a challenge. The sealing of the data
//! key under that signature, the 7-day expiry and the repeatability check live in the store crate
//! (`snipdeck_store::crypto::SealedKey`), which takes the signer as a closure so neither crate
//! depends on the other. The app wires them: `|c| hello.sign_challenge(c)`.
//! [`crate::testing::FakeHello`] is the test double.

use zeroize::Zeroizing;

/// Whether this PC can use Hello right now.
///
/// `NotSetUp` and `Unsupported` both mean: hide the Hello option. They differ only for a hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloAvailability {
    /// A Hello credential can be made or used.
    Ready,
    /// Hello exists on this PC but the user has no PIN, face or fingerprint set up.
    NotSetUp,
    /// No Hello on this PC (or not on this OS).
    Unsupported,
}

/// Why a Hello step did not give a signature. All of them fall back to the password and drop the
/// sealed copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HelloError {
    /// The user closed the Hello prompt (or chose the password).
    #[error("Windows Hello was cancelled")]
    Cancelled,
    /// Hello failed.
    #[error("Windows Hello failed")]
    Failed,
    /// The Hello credential or PIN was removed.
    #[error("the Windows Hello credential was removed")]
    Removed,
}

/// The OS side of Hello. Methods block (a prompt may be on screen): call them from a worker.
pub trait HelloAuthenticator: Send + Sync {
    /// Whether Hello can be used. `NotSetUp` and `Unsupported` both mean hide the option.
    fn availability(&self) -> HelloAvailability;
    /// Makes the Snipdeck Hello credential if it does not exist yet (may prompt).
    ///
    /// # Errors
    ///
    /// [`HelloError`] when the user cancels or Hello fails.
    fn ensure_credential(&self) -> Result<(), HelloError>;
    /// Signs `challenge` with the credential (prompts the user). Deterministic for one credential.
    /// The signature is zeroized when dropped.
    ///
    /// # Errors
    ///
    /// [`HelloError::Cancelled`], [`HelloError::Removed`] or [`HelloError::Failed`].
    fn sign_challenge(&self, challenge: &[u8]) -> Result<Zeroizing<Vec<u8>>, HelloError>;
}
