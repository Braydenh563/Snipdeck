//! Snipdeck is free and unlocks nothing for a donation. No screen, tooltip or translation may talk
//! about a paid tier or a licence key. Third-party licence texts (the GPL, the libraries' licences)
//! are a different thing and are allowed: only the words of the old paid tier are forbidden.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::path::{Path, PathBuf};

/// What the old paid tier said, lower-cased.
const FORBIDDEN: &[&str] = &[
    "supporter",
    "licence key",
    "license key",
    "licence code",
    "enter your licence",
    "pro tier",
    "free plan",
    "paid plan",
    "upgrade to",
    "unlock with a licence",
];

fn files_under(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            files_under(&path, extensions, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| extensions.contains(&e))
        {
            out.push(path);
        }
    }
}

#[test]
fn no_ui_string_or_translation_mentions_the_paid_tier() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    files_under(&root.join("ui"), &["slint"], &mut files);
    files_under(&root.join("lang"), &["pot", "po"], &mut files);
    assert!(
        files.len() > 20,
        "the scan found only {} files",
        files.len()
    );
    let mut hits = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).expect("utf-8").to_lowercase();
        for (n, line) in text.lines().enumerate() {
            for word in FORBIDDEN {
                if line.contains(word) {
                    hits.push(format!("{}:{}: {word}", file.display(), n + 1));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the paid tier is still mentioned:\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_support_link_text_says_it_is_optional_and_unlocks_nothing() {
    let pot =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("lang/snipdeck-ui.pot"))
            .expect("pot");
    assert!(
        pot.contains("Support Snipdeck"),
        "the support row is missing"
    );
    assert!(
        pot.to_lowercase().contains("unlocks nothing"),
        "the support text must say it unlocks nothing"
    );
}
