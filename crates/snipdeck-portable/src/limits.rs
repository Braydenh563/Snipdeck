//! Defensive limits for reading untrusted archives (and for writing archives that can be read
//! back under the same limits).

/// Largest text payload (`text`, `html`, `rtf`) an item may hold: SPEC §5.4's 5 MB limit.
pub const TEXT_LIMIT: u64 = 5 * 1024 * 1024;

/// Limits applied to every archive. The defaults suit any archive Snipdeck writes for a library
/// of up to 100 000 items; raise them only for files you trust.
///
/// A `.snipdeck` file is untrusted input: it can come from a download, a shared folder or a
/// forum post. These limits bound memory and disk use whatever the archive claims about itself.
/// Sizes are checked twice: against the sizes the zip headers declare (before anything is
/// decompressed) and against the bytes actually produced while decompressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Most entries (files and folders) in the archive. Default 200 000.
    pub max_entries: usize,
    /// Largest uncompressed size of one entry, in bytes. Default 64 MiB.
    pub max_entry_bytes: u64,
    /// Largest sum of all uncompressed sizes, in bytes. Default 1 GiB.
    pub max_total_bytes: u64,
    /// Largest `uncompressed / compressed` ratio of an entry larger than [`TEXT_LIMIT`] (5 MiB,
    /// the biggest legitimate text payload, which is exempt). Default 500; deflate cannot
    /// exceed about 1032.
    pub max_ratio: u64,
    /// Largest uncompressed size of `items.json`, in bytes. Default 64 MiB.
    pub max_json_bytes: u64,
    /// Most items in the archive. Default 100 000.
    pub max_items: usize,
}

impl Limits {
    /// Entries at or below this size are exempt from the ratio check: they cannot do harm, and
    /// a 5 MB text of repeated lines legitimately compresses by more than 500.
    pub(crate) const RATIO_FLOOR: u64 = TEXT_LIMIT;
    /// `manifest.json` is tiny; anything bigger is not a manifest.
    pub(crate) const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
    /// Most files one `files` format may hold.
    pub(crate) const MAX_FILES_PER_FORMAT: usize = 10_000;
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 200_000,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 1024 * 1024 * 1024,
            max_ratio: 500,
            max_json_bytes: 64 * 1024 * 1024,
            max_items: 100_000,
        }
    }
}
