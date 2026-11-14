//! Importing from `TextExpander`.
//!
//! Reads the CSV that `TextExpander` exports (columns *Abbreviation*, *Content*, *Label*; an
//! optional *Group* column is used for lists). The columns are found by their header names,
//! case-insensitively; a file with no recognisable header is read as abbreviation, content,
//! label, group by position. Comma, semicolon and tab separated files are all read, UTF-8 or
//! UTF-16.
//!
//! Not read: `TextExpander`'s own `.textexpander` / plist / XML formats. Only the CSV layout
//! above was used; it comes from what is publicly described, not a file produced by
//! `TextExpander`, so the header names accepted are deliberately generous.
//!
//! Macros: `%clipboard` becomes `{clipboard}` and `%|` becomes `{cursor}`. Fill-ins
//! (`%fill...`), snippet includes (`%snippet:`), key presses (`%key:`), `%URL`, `%calc` and the
//! date and time macros (`%Y`, `%m`, `%d`, ...) have no safe equivalent (a lone `%d` is also
//! ordinary text) and are left as written, with a note.

use std::path::Path;

use snipdeck_core::Timestamp;

use crate::error::Result;
use crate::import::ImportOptions;
use crate::plan::Library;
use crate::report::{ImportReport, Note, Warning};

use super::csvtable::{Columns, Names, Spec, import_csv};
use super::expander::{Body, Tpl};

/// Imports a `TextExpander` CSV export into a new board named `Imported from TextExpander`
/// (unless the options say otherwise), one list per group, or one list `TextExpander`.
///
/// # Errors
///
/// [`Error::File`](crate::Error::File) if the file cannot be read,
/// [`Error::LimitExceeded`](crate::Error::LimitExceeded) if it is over the size limit, and
/// storage errors. Rows with no content, or over 5 MB, are skipped with a warning.
pub fn import_textexpander_csv<L: Library>(
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
            file: "TextExpander file size",
            names: Names {
                abbreviation: &["abbreviation", "abbrev", "shortcut", "trigger"],
                content: &[
                    "content",
                    "snippet",
                    "text",
                    "expansion",
                    "body",
                    "replacement",
                ],
                name: &["label", "name", "title", "description"],
                group: &["group", "folder", "category"],
            },
            positional: Columns {
                abbreviation: Some(0),
                content: 1,
                name: Some(2),
                group: Some(3),
            },
            defaults: ("Imported from TextExpander", "TextExpander"),
            convert,
            group: |g| g.trim().to_owned(),
        },
    )
}

/// Macro prefixes with no Snipdeck equivalent.
const UNMAPPED: &[&str] = &[
    "%fill",
    "%snippet:",
    "%key:",
    "%URL",
    "%calc",
    "%Y",
    "%y",
    "%m",
    "%d",
    "%e",
    "%H",
    "%I",
    "%M",
    "%S",
    "%p",
    "%B",
    "%b",
    "%A",
    "%a",
    "%j",
    "%z",
    "%D",
    "%T",
    "%F",
    "%@",
];

fn convert(src: &str, label: &str, report: &mut ImportReport) -> Body {
    let mut tpl = Tpl::default();
    let mut rest = src;
    let mut unmapped: Vec<&str> = Vec::new();
    while let Some(at) = rest.find('%') {
        let after = &rest[at + 1..];
        let clip = after
            .strip_prefix("clipboard")
            .filter(|r| !r.starts_with(|c: char| c.is_alphanumeric() || c == '_'));
        if let Some(r) = clip {
            tpl.lit(&rest[..at]);
            tpl.ph("{clipboard}");
            rest = r;
        } else if let Some(r) = after.strip_prefix('|') {
            tpl.lit(&rest[..at]);
            tpl.ph("{cursor}");
            rest = r;
        } else {
            if let Some(m) = UNMAPPED
                .iter()
                .find(|m| rest[at..].starts_with(**m) && !unmapped.contains(m))
            {
                unmapped.push(m);
            }
            tpl.lit(&rest[..=at]);
            rest = after;
        }
    }
    tpl.lit(rest);
    if !unmapped.is_empty() {
        report.warn(Warning::Note(Note::UnmappedVariables {
            label: label.to_string(),
            tokens: format!("{}...", unmapped.join("..., ")),
        }));
    }
    tpl.finish(src, label, report)
}
