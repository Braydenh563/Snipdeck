//! A list written out as one text file ("Export as text…" in the list menu).
//!
//! The file starts with the list's name; each item follows as its title (when it has one) and its
//! text, items apart by a line of dashes. Items without text (images, files) and sensitive items
//! are left out: the file is plain text on disk, outside the encrypted library.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use jiff::civil::Date;

/// One item of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The item's title, empty when it has none.
    pub title: String,
    /// The item's text.
    pub text: String,
}

/// The line that sets items apart: three dashes, or more when an item has a line of exactly that
/// many (then it could not be told from the separator).
fn separator(entries: &[Entry]) -> String {
    let is_dashes = |line: &str| !line.is_empty() && line.trim().chars().all(|c| c == '-');
    let mut width = 3;
    while entries
        .iter()
        .flat_map(|e| e.title.lines().chain(e.text.lines()))
        .any(|line| is_dashes(line) && line.trim().len() == width)
    {
        width += 1;
    }
    "-".repeat(width)
}

/// The file's content for list `name`, with lines ended by `\r\n` when `crlf` (Windows) and by
/// `\n` otherwise. Items are apart by a line of dashes no item has a line of.
#[must_use]
pub fn render(name: &str, entries: &[Entry], crlf: bool) -> String {
    let rule = separator(entries);
    let mut out = format!("{name}\n");
    for (i, entry) in entries.iter().enumerate() {
        if i == 0 {
            out.push('\n');
        } else {
            let _ = write!(out, "\n{rule}\n\n");
        }
        if !entry.title.is_empty() {
            out.push_str(&entry.title);
            out.push('\n');
        }
        out.push_str(entry.text.trim_end());
        out.push('\n');
    }
    if crlf {
        out.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        out
    }
}

/// A file name in `folder` for the export of list `name` made on `date` that no file has yet:
/// `Code-20261019.txt`, then `Code-20261019-2.txt` and so on. Characters a file name cannot hold
/// become `_`.
#[must_use]
pub fn path(folder: &Path, name: &str, date: Date) -> PathBuf {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().trim_end_matches('.');
    let clean = if clean.is_empty() { "List" } else { clean };
    let stem = format!(
        "{clean}-{:04}{:02}{:02}",
        date.year(),
        date.month(),
        date.day()
    );
    let first = folder.join(format!("{stem}.txt"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|n| folder.join(format!("{stem}-{n}.txt")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, text: &str) -> Entry {
        Entry {
            title: title.into(),
            text: text.into(),
        }
    }

    #[test]
    fn items_follow_the_name_apart_by_dashes() {
        let text = render(
            "Code",
            &[entry("Hello", "echo hi\n"), entry("", "second\nline")],
            false,
        );
        assert_eq!(text, "Code\n\nHello\necho hi\n\n---\n\nsecond\nline\n");
    }

    #[test]
    fn the_separator_is_longer_than_any_line_of_dashes_in_an_item() {
        let text = render(
            "Code",
            &[
                entry("", "a\n---\nb"),
                entry("", "c\n----\nd"),
                entry("", "e"),
            ],
            false,
        );
        assert!(text.contains("\n-----\n\ne\n"), "{text:?}");
        assert_eq!(text.matches("\n-----\n").count(), 2);
    }

    #[test]
    fn windows_text_ends_every_line_with_crlf_once() {
        let text = render("Code", &[entry("T", "a\r\nb\nc")], true);
        assert_eq!(text, "Code\r\n\r\nT\r\na\r\nb\r\nc\r\n");
        assert!(!text.replace("\r\n", "").contains('\n'));
    }

    #[test]
    fn an_empty_list_is_just_its_name() {
        assert_eq!(render("Code", &[], false), "Code\n");
    }

    #[test]
    fn the_file_name_is_safe_dated_and_never_overwrites() {
        let dir = tempfile::tempdir().expect("dir");
        let date = Date::constant(2026, 10, 19);
        let first = path(dir.path(), "Work: a/b?", date);
        assert_eq!(first.file_name().unwrap(), "Work_ a_b_-20261019.txt");
        std::fs::write(&first, "x").expect("write");
        let second = path(dir.path(), "Work: a/b?", date);
        assert_eq!(second.file_name().unwrap(), "Work_ a_b_-20261019-2.txt");
        assert_eq!(
            path(dir.path(), " .. ", date).file_name().unwrap(),
            "List-20261019.txt"
        );
    }
}
