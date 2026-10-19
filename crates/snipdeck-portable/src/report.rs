//! What an import did: the same report for every importer.

use core::fmt;

use snipdeck_core::BoardId;

/// Most warnings a report keeps; the rest are only counted in
/// [`ImportReport::warnings_dropped`], so a folder of 100 000 bad files cannot exhaust memory.
pub const MAX_WARNINGS: usize = 200;

/// Something the import did not do, or did differently from what the source said. Warnings never
/// fail an import; the caller shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The archive's minor version is newer than this build's; unknown fields were ignored.
    NewerMinorVersion {
        /// The archive's `format_version`.
        found: String,
    },
    /// The archive holds files this version does not know; they were ignored.
    UnknownEntries {
        /// How many.
        count: usize,
    },
    /// The archive holds assets no item refers to; they were ignored.
    UnreferencedAssets {
        /// How many.
        count: usize,
    },
    /// Encrypted (sensitive) items were skipped because no key was given.
    SensitiveNeedKey {
        /// How many items.
        count: usize,
    },
    /// An encrypted item could not be decrypted with the given key (another computer's key, or
    /// the file was altered), so it was skipped.
    CannotDecrypt {
        /// The item's title.
        title: String,
    },
    /// The archive was exported without its sensitive items.
    SensitiveOmittedByExport {
        /// How many items the exporter left out.
        count: u64,
    },
    /// The import leaves more boards than the Free tier allows. Nothing was blocked; the caller
    /// decides what to tell the user.
    BoardLimit {
        /// The limit passed in the options.
        limit: usize,
        /// Boards after the import.
        total: usize,
    },
    /// A file was not valid UTF-8; invalid bytes were replaced with U+FFFD.
    LossyText {
        /// The file or item.
        name: String,
    },
    /// A file was larger than the 5 MB text limit and was skipped.
    TooLarge {
        /// The file or item.
        name: String,
        /// Its size in bytes.
        size: u64,
    },
    /// A file or entry was empty and was skipped.
    Empty {
        /// The file or item.
        name: String,
    },
    /// A file or entry could not be read and was skipped.
    Unreadable {
        /// The file or item.
        name: String,
        /// Why.
        reason: String,
    },
    /// A symbolic link was not followed.
    SymlinkSkipped {
        /// The link.
        name: String,
    },
    /// The source holds entries that are not text (images, files, ...), which are not imported.
    NonTextSkipped {
        /// How many.
        count: usize,
    },
    /// The importer stopped at its cap on items or bytes.
    Truncated {
        /// Why it stopped.
        reason: String,
    },
    /// Anything else worth telling the user.
    Note(String),
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NewerMinorVersion { found } => write!(
                f,
                "the archive uses format version {found}, newer than this Snipdeck knows; \
                 unknown fields were ignored"
            ),
            Self::UnknownEntries { count } => {
                write!(f, "{count} unknown file(s) in the archive were ignored")
            }
            Self::UnreferencedAssets { count } => {
                write!(f, "{count} asset(s) no item refers to were ignored")
            }
            Self::SensitiveNeedKey { count } => write!(
                f,
                "{count} sensitive item(s) are encrypted and were skipped because no key is available"
            ),
            Self::CannotDecrypt { title } => write!(
                f,
                "`{title}` could not be decrypted with this computer's key and was skipped"
            ),
            Self::SensitiveOmittedByExport { count } => write!(
                f,
                "{count} sensitive item(s) were left out when this file was exported"
            ),
            Self::BoardLimit { limit, total } => write!(
                f,
                "you now have {total} boards; the Free plan includes {limit}"
            ),
            Self::LossyText { name } => write!(
                f,
                "`{name}` is not valid UTF-8; unreadable bytes were replaced with \u{fffd}"
            ),
            Self::TooLarge { name, size } => write!(
                f,
                "`{name}` is {size} bytes, over the 5 MB text limit, and was skipped"
            ),
            Self::Empty { name } => write!(f, "`{name}` is empty and was skipped"),
            Self::Unreadable { name, reason } => {
                write!(f, "`{name}` could not be read ({reason}) and was skipped")
            }
            Self::SymlinkSkipped { name } => {
                write!(f, "`{name}` is a symbolic link and was not followed")
            }
            Self::NonTextSkipped { count } => {
                write!(
                    f,
                    "{count} entry(ies) without text (images, files) were skipped"
                )
            }
            Self::Truncated { reason } => write!(f, "the import stopped early: {reason}"),
            Self::Note(note) => f.write_str(note),
        }
    }
}

/// The outcome of an import, the same for every importer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Boards created.
    pub boards_added: usize,
    /// Lists created.
    pub lists_added: usize,
    /// Items created.
    pub items_added: usize,
    /// Items in the source that were not imported (duplicates, unreadable, undecryptable, ...).
    pub items_skipped: usize,
    /// How many of `items_skipped` were already in the library (content-identical).
    pub duplicates_skipped: usize,
    /// Tags created (existing tags with the same name are reused).
    pub tags_added: usize,
    /// The boards created, in order, so the UI can open the first one.
    pub boards: Vec<BoardId>,
    /// Up to [`MAX_WARNINGS`] warnings.
    pub warnings: Vec<Warning>,
    /// Warnings beyond [`MAX_WARNINGS`], which were counted but not kept.
    pub warnings_dropped: usize,
}

impl ImportReport {
    /// Records a warning, or only counts it once [`MAX_WARNINGS`] are kept.
    pub fn warn(&mut self, warning: Warning) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(warning);
        } else {
            self.warnings_dropped += 1;
        }
    }

    /// Whether anything was left out or changed that the user should hear about.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty() || self.warnings_dropped > 0
    }
}
