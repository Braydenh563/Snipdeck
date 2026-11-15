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
    /// Largest sum of all uncompressed sizes, in bytes. Default 8 GiB: the app writes its own
    /// library (backups, exports) under this same limit and reads it back under it, so it is
    /// set well above what a library of attachments plausibly holds. Memory does not scale with
    /// it (entries are streamed and checked one at a time); only disk does.
    pub max_total_bytes: u64,
    /// Largest `uncompressed / compressed` ratio of an entry larger than [`TEXT_LIMIT`] (5 MiB,
    /// the biggest legitimate text payload, which is exempt). Default 500; deflate cannot
    /// exceed about 1032.
    pub max_ratio: u64,
    /// Largest uncompressed size of `items.json`, in bytes. Default 256 MiB
    /// ([`DEFAULT_MAX_JSON_BYTES`]): it is read into memory once, so an untrusted archive must not
    /// ask for more. Export refuses to write a larger one ([`Limits::export_json_cap`]), so
    /// the app's own archives always re-import under the default.
    pub max_json_bytes: u64,
    /// Most items in the archive. Default 100 000.
    pub max_items: usize,
}

/// The default [`Limits::max_total_bytes`]: 8 GiB.
const DEFAULT_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// The default [`Limits::max_json_bytes`]: 256 MiB.
pub const DEFAULT_MAX_JSON_BYTES: u64 = 256 * 1024 * 1024;

/// What encrypting a payload adds to it (`SNENC2` 6 + nonce 24 + tag 16 = 46 bytes), rounded up.
pub const ENVELOPE_OVERHEAD_BYTES: u64 = 64;

impl Limits {
    /// The largest attachment (one file, or one picture) that always fits in an entry, sensitive
    /// or not: [`Limits::max_entry_bytes`] less [`ENVELOPE_OVERHEAD_BYTES`].
    #[must_use]
    pub const fn max_attachment_bytes(&self) -> u64 {
        self.max_entry_bytes.saturating_sub(ENVELOPE_OVERHEAD_BYTES)
    }

    /// What the cap for the entry called `name` is called in a [`crate::Error::LimitExceeded`].
    pub(crate) fn entry_cap_what(&self, name: &str) -> &'static str {
        if name == crate::archive::ITEMS {
            "items.json size"
        } else {
            "uncompressed entry size"
        }
    }

    /// The size cap for the entry called `name`: `items.json` has its own cap
    /// ([`Limits::max_json_bytes`]); every other entry is held to [`Limits::max_entry_bytes`].
    pub(crate) fn entry_cap(&self, name: &str) -> u64 {
        if name == crate::archive::ITEMS {
            self.max_json_bytes
        } else {
            self.max_entry_bytes
        }
    }

    /// The largest `items.json` an export writes: these limits' cap, but never above the default
    /// import cap, so an archive this app writes can always be read back with default limits.
    #[must_use]
    pub const fn export_json_cap(&self) -> u64 {
        if self.max_json_bytes < DEFAULT_MAX_JSON_BYTES {
            self.max_json_bytes
        } else {
            DEFAULT_MAX_JSON_BYTES
        }
    }

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
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_ratio: 500,
            max_json_bytes: DEFAULT_MAX_JSON_BYTES,
            max_items: 100_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_total_holds_a_library_that_the_old_1_gib_total_refused() {
        // 30 recordings of 40 MiB (the audit's example) is 1.2 GiB.
        let library = 30 * 40 * 1024 * 1024_u64;
        assert!(library > 1024 * 1024 * 1024);
        assert!(Limits::default().max_total_bytes >= library * 4);
    }

    #[test]
    fn items_json_has_its_own_entry_cap_above_the_attachment_cap() {
        let l = Limits::default();
        assert_eq!(l.entry_cap("items.json"), l.max_json_bytes);
        assert_eq!(l.entry_cap("assets/abc"), l.max_entry_bytes);
        assert!(l.entry_cap("items.json") > l.entry_cap("assets/abc"));
    }

    #[test]
    fn export_never_writes_an_items_json_the_default_import_would_refuse() {
        assert_eq!(DEFAULT_MAX_JSON_BYTES, 256 * 1024 * 1024);
        assert_eq!(Limits::default().export_json_cap(), DEFAULT_MAX_JSON_BYTES);
        let roomy = Limits {
            max_json_bytes: u64::MAX,
            ..Limits::default()
        };
        assert_eq!(roomy.export_json_cap(), DEFAULT_MAX_JSON_BYTES);
        let tight = Limits {
            max_json_bytes: 100,
            ..Limits::default()
        };
        assert_eq!(tight.export_json_cap(), 100);
    }

    #[test]
    fn the_default_json_cap_holds_a_library_of_many_large_text_items() {
        // 100 items of a 2 MiB text each is 200 MiB of items.json; the old 64 MiB cap refused it.
        // The cap is 256 MiB, not more: an untrusted import is read into memory whole.
        assert!(Limits::default().max_json_bytes >= 100 * 2 * 1024 * 1024);
        assert!(Limits::default().max_json_bytes <= 256 * 1024 * 1024);
    }
}
