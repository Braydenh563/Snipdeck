use crate::{Feature, FormatKind};

/// Everything that can go wrong when building or parsing a core value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A [`Name`](crate::Name) was empty after trimming.
    #[error("name is empty")]
    EmptyName,
    /// A [`Name`](crate::Name) was longer than [`Name::MAX_CHARS`](crate::Name::MAX_CHARS).
    #[error("name is longer than {max} characters")]
    NameTooLong {
        /// The limit that was exceeded.
        max: usize,
    },
    /// A [`Name`](crate::Name) contained a control character such as a newline.
    #[error("name contains a control character")]
    NameHasControlChar,
    /// [`Formats`](crate::Formats) was given no formats.
    #[error("an item needs at least one format")]
    NoFormats,
    /// [`Formats`](crate::Formats) was given two formats of the same kind.
    #[error("an item has more than one `{0}` format")]
    DuplicateFormat(FormatKind),
    /// A [`Format::Files`](crate::Format::Files) held no files.
    #[error("a `files` format needs at least one file")]
    NoFiles,
    /// Bytes passed to [`Inline::new`](crate::Inline::new) exceed [`INLINE_LIMIT`](crate::INLINE_LIMIT).
    #[error("{len} bytes is over the {max}-byte inline limit")]
    InlineTooLarge {
        /// Length of the rejected bytes.
        len: usize,
        /// The limit, [`INLINE_LIMIT`](crate::INLINE_LIMIT).
        max: usize,
    },
    /// A [`FileEntry`](crate::FileEntry) name was not a single, Windows-valid file name.
    #[error("`{0}` is not a valid file name")]
    InvalidFileName(String),
    /// A [`Format::Files`](crate::Format::Files) held two files whose names Windows treats as
    /// the same (they differ at most in letter case).
    #[error("`{0}` has the same name as another file in this item")]
    DuplicateFileName(String),
    /// A string could not be parsed as the named kind of value (an id, a key, a blob hash).
    #[error("`{value}` is not a valid {what}")]
    Parse {
        /// What was being parsed, e.g. `"item id"`.
        what: &'static str,
        /// The rejected input.
        value: String,
    },
    /// The action needs Snipdeck Supporter; the UI answers this with the upsell sheet (SPEC §6).
    #[error("`{0}` needs Snipdeck Supporter")]
    ProRequired(Feature),
}
