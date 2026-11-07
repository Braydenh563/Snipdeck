//! Importing from `PhraseExpress`.
//!
//! Reads a CSV export of phrases. The columns are found by header name, case-insensitively:
//! description (or name, title) for the title, content (or phrase, text) for the text, autotext
//! (or abbreviation, shortcut) for the abbreviation and folder (or path, group) for the list. A
//! folder path with `\` or `/` between levels becomes one list named `A / B`. A file with no
//! recognisable header is read as description, content, autotext, folder by position. Comma,
//! semicolon (what `PhraseExpress` uses where the decimal separator is a comma) and tab
//! separated files are read, UTF-8 or UTF-16.
//!
//! **Unverified**: the header names and the macro syntax below come from the public manual as
//! remembered, not from a file written by `PhraseExpress`. The header lookup is deliberately
//! wide for that reason. The `.pxp` / XML export is not read.
//!
//! Macros: `{#clipboard}`, `{#date}`, `{#time}` and `{#caret}` / `{#cursor}` become the
//! Snipdeck placeholders. Every other `{#...}` macro (`{#shell ...}`, `{#system}`, `{#input
//! ...}`, ...) is left as written, with a note.

use std::path::Path;

use snipdeck_core::Timestamp;

use crate::error::Result;
use crate::import::ImportOptions;
use crate::plan::Library;
use crate::report::{ImportReport, Warning};

use super::csvtable::{Columns, Names, Spec, import_csv};
use super::expander::{Body, Tpl};

/// Imports a `PhraseExpress` CSV export into a new board named `Imported from PhraseExpress`
/// (unless the options say otherwise), one list per folder, or one list `PhraseExpress`.
///
/// # Errors
///
/// [`Error::File`](crate::Error::File) if the file cannot be read,
/// [`Error::LimitExceeded`](crate::Error::LimitExceeded) if it is over the size limit, and
/// storage errors. Rows with no content, or over 5 MB, are skipped with a warning.
pub fn import_phraseexpress_csv<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    import_csv(
        lib,
        path,
        now,
        options,
        &Spec {
            file: "PhraseExpress file size",
            names: Names {
                abbreviation: &["autotext", "abbreviation", "abbrev", "shortcut", "trigger"],
                content: &["content", "phrase", "text", "phrase content", "body"],
                name: &[
                    "description",
                    "name",
                    "title",
                    "phrase description",
                    "label",
                ],
                group: &["folder", "path", "group", "category", "phrase folder"],
            },
            positional: Columns {
                abbreviation: Some(2),
                content: 1,
                name: Some(0),
                group: Some(3),
            },
            defaults: ("Imported from PhraseExpress", "PhraseExpress"),
            convert,
            group: |g| {
                g.split(['\\', '/'])
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join(" / ")
            },
        },
    )
}

fn convert(src: &str, label: &str, report: &mut ImportReport) -> Body {
    let mut tpl = Tpl::default();
    let mut rest = src;
    let mut unmapped: Vec<String> = Vec::new();
    while let Some(at) = rest.find("{#") {
        let after = &rest[at + 2..];
        let Some(end) = after.find('}') else { break };
        let inner = after[..end].trim();
        let word = inner.split_whitespace().next().unwrap_or("").to_lowercase();
        let placeholder = match (word.as_str(), inner.split_whitespace().count()) {
            ("clipboard", 1) => Some("{clipboard}"),
            ("caret" | "cursor", 1) => Some("{cursor}"),
            ("date", 1) => Some("{date}"),
            ("time", 1) => Some("{time}"),
            _ => None,
        };
        tpl.lit(&rest[..at]);
        if let Some(p) = placeholder {
            tpl.ph(p);
        } else {
            tpl.lit(&rest[at..=at + 2 + end]);
            if !unmapped.contains(&word) {
                unmapped.push(word);
            }
        }
        rest = &after[end + 1..];
    }
    tpl.lit(rest);
    if !unmapped.is_empty() {
        report.warn(Warning::Note(format!(
            "{label}: no Snipdeck equivalent for {{#{}}}; left as written",
            unmapped.join("}, {#")
        )));
    }
    tpl.finish(src, label, report)
}
