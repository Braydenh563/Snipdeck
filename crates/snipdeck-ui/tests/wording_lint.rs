//! DESIGN.md "Names": the words a person reads. The scan reads every `@tr("...")` literal under
//! `ui/` (comments are not text a person sees).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::path::{Path, PathBuf};

fn slint_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "slint") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("ui"), &mut out);
    out
}

/// Every `@tr("...")` literal of `file`, with its line number.
fn literals(file: &Path) -> Vec<(usize, String)> {
    let text = std::fs::read_to_string(file).expect("utf-8");
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find("@tr(\"") {
            rest = &rest[at + 5..];
            let mut end = 0;
            let bytes = rest.as_bytes();
            while end < bytes.len()
                && !(bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\'))
            {
                end += 1;
            }
            out.push((n + 1, rest[..end].to_string()));
            rest = &rest[end.min(rest.len())..];
        }
    }
    out
}

#[test]
fn no_text_says_database_or_the_old_names_for_recent() {
    const FORBIDDEN: &[&str] = &[
        "database",
        "recent inbox",
        "recent copies",
        "clipboard history",
        "pause history",
    ];
    let mut hits = Vec::new();
    let mut count = 0;
    for file in slint_files() {
        for (line, text) in literals(&file) {
            count += 1;
            let lower = text.to_lowercase();
            for word in FORBIDDEN {
                // The Windows feature keeps its own name where it is the feature (Win+V).
                if lower.contains(word) && !lower.contains("windows clipboard history") {
                    hits.push(format!("{}:{line}: {word}: {text}", file.display()));
                }
            }
        }
    }
    assert!(count > 500, "the scan found only {count} strings");
    assert_eq!(hits.len(), 0, "{hits:#?}");
}

/// A menu or command that needs a further answer ends in "…" (one character, never three dots);
/// one that acts at once, or opens a read-only window, does not. The set below is the whole list:
/// a new entry with the mark fails until it is added here on purpose.
#[test]
fn only_commands_that_ask_a_further_question_end_in_an_ellipsis() {
    const ASKS: &[&str] = &[
        "Commands…",
        "Edit…",
        "Group…",
        "Move to list…",
        "New board…",
        "Paste as…",
        "Rename board…",
        "Save as PNG…",
        "Save as SVG…",
        "Settings…",
        "Troubleshoot…",
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut marked = Vec::new();
    for name in ["menu_texts.slint", "command_texts.slint"] {
        for (line, text) in literals(&root.join(name)) {
            assert!(
                !text.contains("..."),
                "{name}:{line}: three dots in {text:?}"
            );
            if text.ends_with('…') {
                marked.push(text);
            }
        }
    }
    marked.sort();
    marked.dedup();
    let mut expected: Vec<String> = ASKS.iter().map(ToString::to_string).collect();
    expected.sort();
    assert_eq!(marked, expected);
}
