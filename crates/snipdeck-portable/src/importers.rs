//! Importers for other tools' data. Each turns its source into a [`Plan`](crate::plan::Plan),
//! then shares the tail of the archive import: duplicate skipping, the board-limit warning and
//! the atomic write. They return the same [`ImportReport`](crate::ImportReport).
//!
//! Sources are untrusted too (a shared folder, a database someone sent): reads are capped per
//! item at the 5 MB text limit and in total at [`MEMORY_BUDGET`], symbolic links are not
//! followed, and nothing is executed or opened for writing.

mod copyq;
mod ditto;
mod folder;
mod history;

pub use copyq::import_copyq_json;
pub use ditto::import_ditto;
pub use folder::import_folder;
pub use history::{HistoryEntry, import_entries};

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

fn truncate_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

/// How bytes were turned into text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decoded {
    /// Valid UTF-8 (a BOM, if any, was removed).
    Utf8,
    /// UTF-16 with a byte-order mark, converted exactly.
    Utf16,
    /// Not valid text of either kind: invalid sequences were replaced with U+FFFD.
    Lossy,
}

/// Decodes a text file: UTF-8 (BOM stripped), UTF-16 with a BOM (Windows Notepad's "Unicode"),
/// otherwise UTF-8 with invalid bytes replaced.
pub(crate) fn decode_text(bytes: &[u8]) -> (String, Decoded) {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(rest);
    }
    let utf16 = |rest: &[u8], le: bool| -> (String, Decoded) {
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
                Decoded::Utf16
            } else {
                Decoded::Lossy
            },
        )
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => decode_utf8(bytes),
    }
}

fn decode_utf8(bytes: &[u8]) -> (String, Decoded) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_owned(), Decoded::Utf8),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), Decoded::Lossy),
    }
}

/// Adds the standard warning when `how` was lossy.
pub(crate) fn note_lossy(how: Decoded, name: &str, report: &mut ImportReport) {
    if how == Decoded::Lossy {
        report.warn(Warning::LossyText {
            name: name.to_owned(),
        });
    }
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
            ("héllo".to_owned(), Decoded::Utf8)
        );
        assert_eq!(
            decode_text(b"\xEF\xBB\xBFhi"),
            ("hi".to_owned(), Decoded::Utf8)
        );
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("hi é😀".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode_text(&le), ("hi é😀".to_owned(), Decoded::Utf16));
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("hi".encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_text(&be), ("hi".to_owned(), Decoded::Utf16));
        let (text, how) = decode_text(b"ok \xff\xfe bad");
        assert_eq!(how, Decoded::Lossy);
        assert!(text.contains('\u{fffd}'));
        // An odd byte count after a UTF-16 BOM is damaged data, not exact.
        assert_eq!(decode_text(&[0xFF, 0xFE, b'h']).1, Decoded::Lossy);
        // A lone surrogate is invalid UTF-16.
        assert_eq!(decode_text(&[0xFF, 0xFE, 0x00, 0xD8]).1, Decoded::Lossy);
    }
}
