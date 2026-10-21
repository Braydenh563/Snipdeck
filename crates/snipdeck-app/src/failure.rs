//! Why something failed, in the few cases the code can tell apart and the person can act on.
//!
//! A failure toast names the cause and ends with what to do ("the disk is full. Free up some
//! space, then try again."). Only causes the error types distinguish are listed; everything
//! else is [`Cause::Other`], which gets a plain "try again".

use snipdeck_portable::Error as Portable;
use snipdeck_store::{Error as Store, Trouble, crypto::CryptoError};

/// The cause of a failed use, backup, import or export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The disk is full.
    DiskFull,
    /// The file is open in another program, or may not be written (permission, read-only).
    InUse,
    /// The library is locked behind the master password.
    Locked,
    /// The file is not a Snipdeck archive (or is damaged).
    NotArchive,
    /// The file was written by a newer Snipdeck.
    TooNew,
    /// Nothing more specific is known.
    Other,
}

impl Cause {
    /// The number the Slint texts switch on (0 for [`Cause::Other`]).
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Other => 0,
            Self::DiskFull => 1,
            Self::InUse => 2,
            Self::Locked => 3,
            Self::NotArchive => 4,
            Self::TooNew => 5,
        }
    }

    /// The cause behind a file system error.
    #[must_use]
    pub fn of_io(error: &std::io::Error) -> Self {
        Self::of_trouble(Trouble::of_io(error))
    }

    /// The cause behind a storage error.
    #[must_use]
    pub fn of_store(error: &Store) -> Self {
        match error {
            Store::EncryptionLocked | Store::Crypto(CryptoError::PasswordRequired) => Self::Locked,
            Store::SchemaTooNew { .. } => Self::TooNew,
            other => Self::of_trouble(other.trouble()),
        }
    }

    /// The cause behind an export, import or backup error.
    #[must_use]
    pub fn of_portable(error: &Portable) -> Self {
        match error {
            Portable::Store(e) => Self::of_store(e),
            Portable::Io(e) | Portable::File { source: e, .. } => Self::of_io(e),
            Portable::Sqlite(e) => Self::of_trouble(Trouble::of_sqlite(e)),
            Portable::FormatTooNew { .. } => Self::TooNew,
            Portable::NotAnArchive(_)
            | Portable::UnsafeEntry { .. }
            | Portable::LimitExceeded { .. }
            | Portable::MissingEntry(_)
            | Portable::Malformed { .. }
            | Portable::UnsupportedFormat(_)
            | Portable::CountMismatch { .. }
            | Portable::AssetMismatch { .. }
            | Portable::DuplicateId { .. }
            | Portable::NotDitto { .. } => Self::NotArchive,
            _ => Self::Other,
        }
    }

    fn of_trouble(trouble: Trouble) -> Self {
        match trouble {
            Trouble::DiskFull => Self::DiskFull,
            Trouble::InUse => Self::InUse,
            Trouble::Other => Self::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Error as Io, ErrorKind};

    use super::*;

    #[test]
    fn the_causes_the_errors_distinguish_are_told_apart() {
        assert_eq!(
            Cause::of_io(&Io::from(ErrorKind::StorageFull)),
            Cause::DiskFull
        );
        assert_eq!(
            Cause::of_io(&Io::from(ErrorKind::PermissionDenied)),
            Cause::InUse
        );
        assert_eq!(Cause::of_io(&Io::from(ErrorKind::NotFound)), Cause::Other);
        assert_eq!(Cause::of_store(&Store::EncryptionLocked), Cause::Locked);
        assert_eq!(
            Cause::of_store(&Store::SchemaTooNew {
                found: 9,
                supported: 1
            }),
            Cause::TooNew
        );
        assert_eq!(
            Cause::of_portable(&Portable::Io(Io::from(ErrorKind::StorageFull))),
            Cause::DiskFull
        );
        assert_eq!(
            Cause::of_portable(&Portable::NotAnArchive("short".into())),
            Cause::NotArchive
        );
        assert_eq!(
            Cause::of_portable(&Portable::FormatTooNew {
                found: "9.0".into(),
                supported_major: 1
            }),
            Cause::TooNew
        );
        assert_eq!(
            Cause::of_portable(&Portable::Store(Store::EncryptionLocked)),
            Cause::Locked
        );
    }

    #[test]
    fn every_cause_has_its_own_code() {
        let codes = [
            Cause::Other,
            Cause::DiskFull,
            Cause::InUse,
            Cause::Locked,
            Cause::NotArchive,
            Cause::TooNew,
        ]
        .map(Cause::code);
        assert_eq!(codes, [0, 1, 2, 3, 4, 5]);
    }
}
