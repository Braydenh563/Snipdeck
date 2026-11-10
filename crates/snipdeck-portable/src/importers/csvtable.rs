//! A small CSV reader for the text-expander exports.
//!
//! RFC 4180 with the leniency real exports need: quoted cells may hold the delimiter, quotes
//! (`""`) and line breaks; `\r\n`, `\n` and `\r` all end a row; an unterminated quote runs to the
//! end of the input; rows of only empty cells are dropped; a row may be shorter or longer than
//! the header. The delimiter is the one of `,` `;` and tab that is most common in the first
//! row. There is no recursion and nothing is held except the rows kept, and reading stops at
//! `max_rows` rows.

/// The rows read, and whether reading stopped at the cap.
pub(crate) struct Table {
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
    /// Some row had more than [`MAX_CELLS`] cells; the rest were dropped.
    pub cells_cut: bool,
}

/// Most cells kept per row. Real exports have a handful of columns; a line of a million commas
/// must not become a million empty strings.
const MAX_CELLS: usize = 256;

fn detect_delimiter(text: &str) -> char {
    let (mut commas, mut semis, mut tabs, mut quoted) = (0usize, 0usize, 0usize, false);
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            '\n' | '\r' if !quoted => break,
            ',' if !quoted => commas += 1,
            ';' if !quoted => semis += 1,
            '\t' if !quoted => tabs += 1,
            _ => {}
        }
    }
    if tabs > commas && tabs >= semis {
        '\t'
    } else if semis > commas {
        ';'
    } else {
        ','
    }
}

/// Splits `text` into rows of cells, keeping at most `max_rows` non-empty rows.
pub(crate) fn parse(text: &str, max_rows: usize) -> Table {
    let delimiter = detect_delimiter(text);
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let mut cell_started = false;
    let mut cells_cut = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if !cell_started => {
                in_quotes = true;
                cell_started = true;
            }
            c if c == delimiter => {
                push_cell(&mut row, &mut cell, &mut cells_cut);
                cell_started = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_row(&mut row, &mut cell, &mut rows, &mut cells_cut);
                cell_started = false;
                if rows.len() >= max_rows {
                    let truncated = chars.peek().is_some();
                    return Table {
                        rows,
                        truncated,
                        cells_cut,
                    };
                }
            }
            c => {
                cell.push(c);
                cell_started = true;
            }
        }
    }
    if cell_started || !row.is_empty() || !cell.is_empty() {
        end_row(&mut row, &mut cell, &mut rows, &mut cells_cut);
    }
    Table {
        rows,
        truncated: false,
        cells_cut,
    }
}

fn push_cell(row: &mut Vec<String>, cell: &mut String, cut: &mut bool) {
    let cell = std::mem::take(cell);
    if row.len() < MAX_CELLS {
        row.push(cell);
    } else {
        *cut = true;
    }
}

fn end_row(row: &mut Vec<String>, cell: &mut String, rows: &mut Vec<Vec<String>>, cut: &mut bool) {
    push_cell(row, cell, cut);
    let done = std::mem::take(row);
    if done.iter().any(|c| !c.trim().is_empty()) {
        rows.push(done);
    }
}

/// Which column holds what, found from a header row by name, or `None` when the first row does
/// not look like a header (it has no content column).
pub(crate) struct Columns {
    pub abbreviation: Option<usize>,
    pub content: usize,
    pub name: Option<usize>,
    pub group: Option<usize>,
}

/// Synonyms, lower case, for each column.
pub(crate) struct Names {
    pub abbreviation: &'static [&'static str],
    pub content: &'static [&'static str],
    pub name: &'static [&'static str],
    pub group: &'static [&'static str],
}

impl Columns {
    pub(crate) fn from_header(header: &[String], names: &Names) -> Option<Self> {
        let find = |wanted: &[&str]| {
            header.iter().position(|h| {
                let h = h.trim().to_lowercase();
                wanted.contains(&h.as_str())
            })
        };
        Some(Self {
            content: find(names.content)?,
            abbreviation: find(names.abbreviation),
            name: find(names.name),
            group: find(names.group),
        })
    }
}

/// A cell, trimmed of surrounding blanks; `None` when missing or blank.
pub(crate) fn cell(row: &[String], index: Option<usize>) -> Option<&str> {
    let value = row.get(index?)?.as_str();
    (!value.trim().is_empty()).then_some(value)
}

use snipdeck_core::Timestamp;
use std::path::Path;

use crate::error::Result;
use crate::import::ImportOptions;
use crate::plan::Library;
use crate::report::{ImportReport, Warning};

use super::expander::{Body, Snip, import_snips, read_capped};
use super::{decode_text, note_lossy};

/// What differs between the CSV importers.
pub(crate) struct Spec {
    /// The file, for messages and the size-limit error.
    pub file: &'static str,
    pub names: Names,
    /// The columns to use when the first row is not a header.
    pub positional: Columns,
    /// Board and default list names.
    pub defaults: (&'static str, &'static str),
    /// Turns a source's variables into placeholders.
    pub convert: fn(&str, &str, &mut ImportReport) -> Body,
    /// Turns a folder cell into a list name.
    pub group: fn(&str) -> String,
}

/// Reads `path` as a CSV of snippets and imports it.
pub(crate) fn import_csv<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
    spec: &Spec,
) -> Result<ImportReport> {
    let bytes = read_capped(path, spec.file, options)?;
    let mut report = ImportReport::default();
    let (text, how) = decode_text(&bytes);
    note_lossy(how, spec.file, &mut report);
    drop(bytes);
    // Rows looked at are capped at four times the item cap, as for CopyQ, so a file of
    // millions of junk rows is cut off early.
    let table = parse(
        &text,
        options.limits.max_items.saturating_mul(4).saturating_add(1),
    );
    if table.truncated {
        report.warn(Warning::Truncated {
            reason: "too many rows".to_owned(),
        });
    }
    if table.cells_cut {
        report.warn(Warning::Note(format!(
            "{}: some rows have more than 256 columns; the rest were ignored",
            spec.file
        )));
    }
    let mut rows = table.rows.into_iter().peekable();
    let columns = match rows
        .peek()
        .and_then(|h| Columns::from_header(h, &spec.names))
    {
        Some(columns) => {
            rows.next();
            columns
        }
        None => Columns { ..spec.positional },
    };
    let mut snips = Vec::new();
    for (index, row) in rows.enumerate() {
        let label = format!("row {}", index + 1);
        let Some(content) = cell(&row, Some(columns.content)) else {
            report.items_skipped += 1;
            report.warn(Warning::Empty { name: label });
            continue;
        };
        let name = cell(&row, columns.name).map(str::to_owned);
        let abbreviation = cell(&row, columns.abbreviation).map(str::to_owned);
        let shown = name
            .as_deref()
            .or(abbreviation.as_deref())
            .unwrap_or(&label)
            .to_owned();
        snips.push(Snip {
            group: cell(&row, columns.group).map(|g| (spec.group)(g)),
            name,
            abbreviations: abbreviation.into_iter().collect(),
            typed: true,
            body: (spec.convert)(content, &shown, &mut report),
        });
    }
    import_snips(lib, snips, spec.defaults, now, options, report)
}
