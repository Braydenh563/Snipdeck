//! Importers for other tools' data. Each turns its source into a [`Plan`](crate::plan::Plan),
//! then shares the tail of the archive import: duplicate skipping and
//! the atomic write. They return the same [`ImportReport`](crate::ImportReport).
//!
//! Sources are untrusted too (a shared folder, a database someone sent): reads are capped per
//! item at the 5 MB text limit and in total at [`MEMORY_BUDGET`], symbolic links are not
//! followed, and nothing is executed or opened for writing.

mod beeftext;
mod copyq;
mod csvtable;
mod ditto;
mod espanso;
mod expander;
mod folder;
mod history;
mod phraseexpress;
mod textexpander;

pub use beeftext::decode_beeftext;
pub use copyq::decode_copyq_json;
pub use ditto::decode_ditto;
pub use espanso::decode_espanso;
pub use folder::decode_folder;
pub use history::{HistoryEntry, decode_entries};
pub use phraseexpress::decode_phraseexpress_csv;
pub use textexpander::decode_textexpander_csv;

use std::path::Path;

use snipdeck_core::Timestamp;

use crate::error::Result;
use crate::import::{ImportOptions, one_pass};
use crate::plan::Library;
use crate::report::{ImportReport, Warning};

/// Most bytes of item content an importer holds in memory at once (256 MiB). Importers stop
/// with a [`Warning::Truncated`] when they reach it.
pub const MEMORY_BUDGET: u64 = 256 * 1024 * 1024;

/// The longest title an importer derives, in characters.
const TITLE_CHARS: usize = 80;

/// A title from text: its first non-blank line, with whitespace runs collapsed and the length
/// capped. Empty text gives `"Untitled"`.
pub(crate) fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let collapsed = collapse(line);
    if collapsed.is_empty() {
        "Untitled".to_owned()
    } else {
        truncate_chars(&collapsed, TITLE_CHARS)
    }
}

/// A file or folder name made safe to use as a title or list name: control characters become
/// spaces, whitespace is collapsed, and the length is capped.
pub(crate) fn clean_label(raw: &str, max_chars: usize) -> String {
    let spaced: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    truncate_chars(&collapse(&spaced), max_chars)
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

/// How bytes were turned into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextDecoding {
    /// Valid UTF-8 (a BOM, if any, was removed).
    Utf8,
    /// UTF-16 with a byte-order mark, converted exactly.
    Utf16,
    /// Not valid text of either kind: invalid sequences were replaced with U+FFFD.
    Lossy,
}

/// Decodes a text file: UTF-8 (BOM stripped), UTF-16 with a BOM (Windows Notepad's "Unicode"),
/// otherwise UTF-8 with invalid bytes replaced.
pub(crate) fn decode_text(bytes: &[u8]) -> (String, TextDecoding) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(rest);
    }
    let utf16 = |rest: &[u8], le: bool| -> (String, TextDecoding) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                if le {
                    u16::from_le_bytes(*p)
                } else {
                    u16::from_be_bytes(*p)
                }
            })
            .collect();
        let exact = rest.len().is_multiple_of(2) && String::from_utf16(&units).is_ok();
        (
            String::from_utf16_lossy(&units),
            if exact {
                TextDecoding::Utf16
            } else {
                TextDecoding::Lossy
            },
        )
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => decode_utf8(bytes),
    }
}

fn decode_utf8(bytes: &[u8]) -> (String, TextDecoding) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_owned(), TextDecoding::Utf8),
        Err(_) => (
            String::from_utf8_lossy(bytes).into_owned(),
            TextDecoding::Lossy,
        ),
    }
}

/// Adds the standard warning when `how` was lossy.
pub(crate) fn note_lossy(how: TextDecoding, name: &str, report: &mut ImportReport) {
    if how == TextDecoding::Lossy {
        report.warn(Warning::LossyText {
            name: name.to_owned(),
        });
    }
}

/// [`decode_beeftext`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_beeftext`], and storage errors.
pub fn import_beeftext<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_beeftext(path, now, options)
    })
}

/// [`decode_copyq_json`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_copyq_json`], and storage errors.
pub fn import_copyq_json<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_copyq_json(path, now, options)
    })
}

/// [`decode_ditto`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_ditto`], and storage errors.
pub fn import_ditto<L: Library>(
    lib: &mut L,
    db_path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_ditto(db_path, now, options)
    })
}

/// [`decode_espanso`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_espanso`], and storage errors.
pub fn import_espanso<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_espanso(path, now, options)
    })
}

/// [`decode_folder`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_folder`], and storage errors.
pub fn import_folder<L: Library>(
    lib: &mut L,
    folder: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_folder(folder, now, options)
    })
}

/// [`decode_entries`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_entries`], and storage errors.
pub fn import_entries<L: Library>(
    lib: &mut L,
    entries: &[HistoryEntry],
    default_board_name: &str,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_entries(entries, default_board_name, now, options)
    })
}

/// [`decode_phraseexpress_csv`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_phraseexpress_csv`], and storage errors.
pub fn import_phraseexpress_csv<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_phraseexpress_csv(path, now, options)
    })
}

/// [`decode_textexpander_csv`] and then write the result into `lib` (or, with `options.dry_run`, only
/// report what it would add): the one-call form for callers that hold the library.
///
/// # Errors
///
/// See [`decode_textexpander_csv`], and storage errors.
pub fn import_textexpander_csv<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    one_pass(lib, options, |_, options| {
        decode_textexpander_csv(path, now, options)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_the_first_non_blank_line() {
        assert_eq!(title_from("\n\n  Dear   team,\nbody"), "Dear team,");
        assert_eq!(title_from(""), "Untitled");
        assert_eq!(title_from(" \n\t "), "Untitled");
        assert_eq!(title_from(&"é".repeat(200)).chars().count(), TITLE_CHARS);
    }

    #[test]
    fn labels_lose_control_characters_and_length() {
        assert_eq!(clean_label("a\u{7}b\nc", 10), "a b c");
        assert_eq!(clean_label(&"x".repeat(500), 100).len(), 100);
        assert_eq!(clean_label("\u{1b}\u{1b}", 10), "");
    }

    #[test]
    fn decoding_handles_boms_and_garbage() {
        assert_eq!(
            decode_text("héllo".as_bytes()),
            ("héllo".to_owned(), TextDecoding::Utf8)
        );
        assert_eq!(
            decode_text(b"\xEF\xBB\xBFhi"),
            ("hi".to_owned(), TextDecoding::Utf8)
        );
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("hi é😀".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode_text(&le), ("hi é😀".to_owned(), TextDecoding::Utf16));
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("hi".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_text(&be), ("hi".to_owned(), TextDecoding::Utf16));
        let (text, how) = decode_text(b"ok \xff\xfe bad");
        assert_eq!(how, TextDecoding::Lossy);
        assert!(text.contains('\u{fffd}'));
        // An odd byte count after a UTF-16 BOM is damaged data, not exact.
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h']).1, TextDecoding::Lossy);
        // A lone surrogate is invalid UTF-16.
        assert_eq!(
            decode_text(&[0xFF, 0xFE, 0x00, 0xD8]).1,
            TextDecoding::Lossy
        );
    }
}
