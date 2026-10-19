//! The crate's error type.
//!
//! Every variant says what was wrong and, where the user can act on it, what to do. Messages
//! never contain item content (only counts, hashes, titles and entry names), so an error is safe
//! to log. Entry names come from untrusted archives and are shown escaped and truncated.

use std::path::PathBuf;

use snipdeck_store::crypto::CryptoError;

/// Shorthand for results of portability calls.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Everything that can go wrong exporting, importing or backing up.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A storage call failed.
    #[error(transparent)]
    Store(#[from] snipdeck_store::Error),
    /// A value did not form a valid core value (a name, a file name, a format set).
    #[error(transparent)]
    Core(#[from] snipdeck_core::Error),
    /// Encrypting a sensitive item failed.
    #[error("could not encrypt a sensitive item: {0}")]
    Crypto(#[from] CryptoError),
    /// Reading a SQLite file failed.
    #[error("could not read the database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The file system reported an error.
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),
    /// A specific file or folder could not be read or written.
    #[error("cannot access `{}`: {source}", path.display())]
    File {
        /// The path that failed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The input is not a readable zip file (damaged, truncated or another format).
    #[error("this is not a readable .snipdeck file ({0})")]
    NotAnArchive(String),
    /// An archive entry is dangerous or unsupported: path traversal, an absolute path, a
    /// symbolic link, an encrypted entry, overlapping data or a repeated name.
    #[error("the archive was rejected because entry `{name}` is {reason}")]
    UnsafeEntry {
        /// The entry name, escaped and truncated for display.
        name: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// The archive (or input) exceeds a defensive limit. Raise the [`Limits`](crate::Limits)
    /// only for files you trust.
    #[error("the archive was rejected: {what} is over the limit of {limit}")]
    LimitExceeded {
        /// Which limit (`"entry count"`, `"uncompressed size"`, ...).
        what: &'static str,
        /// The limit, in the unit of `what`.
        limit: u64,
    },
    /// A required entry is missing from the archive.
    #[error("the archive has no `{0}`")]
    MissingEntry(&'static str),
    /// An entry's content is not what the format requires.
    #[error("`{entry}` is invalid: {detail}")]
    Malformed {
        /// The entry or input that is wrong.
        entry: String,
        /// What is wrong, without quoting content.
        detail: String,
    },
    /// The archive was written by a newer, incompatible major version of Snipdeck.
    #[error(
        "this archive uses format version {found}, but this Snipdeck understands major version \
         {supported_major}; update Snipdeck to import it"
    )]
    FormatTooNew {
        /// The `format_version` in the manifest.
        found: String,
        /// The newest major version this build reads.
        supported_major: u32,
    },
    /// The manifest is not a Snipdeck archive manifest, or its version is unusable.
    #[error("the archive's manifest is not a Snipdeck export ({0})")]
    UnsupportedFormat(String),
    /// A count in the manifest does not match the data.
    #[error("the manifest says {declared} {what} but the archive holds {actual}")]
    CountMismatch {
        /// What was counted.
        what: &'static str,
        /// The manifest's number.
        declared: u64,
        /// The real number.
        actual: u64,
    },
    /// An asset is missing, has the wrong size or does not match its hash.
    #[error("asset {asset} is damaged: {reason}")]
    AssetMismatch {
        /// The asset's hash name.
        asset: String,
        /// What did not match.
        reason: String,
    },
    /// Two records of one kind share an id.
    #[error("the archive repeats the {kind} id {id}")]
    DuplicateId {
        /// `"board"`, `"list"`, `"item"` or `"tag"`.
        kind: &'static str,
        /// The repeated id.
        id: String,
    },
    /// Internal: rolls a dry-run import back. Never returned to callers.
    #[error("dry run")]
    DryRun,
    /// The input is not a Ditto database.
    #[error("`{}` is not a Ditto database ({reason})", path.display())]
    NotDitto {
        /// The file.
        path: PathBuf,
        /// What was missing.
        reason: String,
    },
}

/// Escapes and truncates an untrusted name for use in a message.
pub(crate) fn display_name(name: &str) -> String {
    const MAX: usize = 80;
    let mut shown: String = name
        .chars()
        .take(MAX)
        .flat_map(char::escape_debug)
        .collect();
    if name.chars().count() > MAX {
        shown.push('…');
    }
    shown
}

impl Error {
    pub(crate) fn malformed(entry: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::Malformed {
            entry: entry.into(),
            detail: detail.into(),
        }
    }

    pub(crate) fn unsafe_entry(name: &str, reason: &'static str) -> Self {
        Self::UnsafeEntry {
            name: display_name(name),
            reason,
        }
    }
}
