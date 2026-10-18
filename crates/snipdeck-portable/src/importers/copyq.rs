//! Importing from `CopyQ`.
//!
//! # Why not the `.cpq` file
//!
//! `CopyQ`'s tab export (`.cpq`) is a Qt `QDataStream` of nested, versioned maps: a `CopyQ v4` or
//! `v5` header, a `QVariantMap` of settings and one map of serialised tab bytes per tab, items
//! stored as mime-to-bytes maps with an abbreviated mime table, optionally stored out of line in
//! `.dat` files or encrypted. It has no published specification and the format has changed
//! between releases. A parser written from reading the source and never run against real files
//! would silently misread some of them, and every parse of a binary format is an attack surface,
//! so this importer does **not** read `.cpq`.
//!
//! # What it reads instead
//!
//! JSON produced by `CopyQ`'s own command line, using its documented scripting API. The command
//! below was written from that documentation and has **not been run against a live `CopyQ`**
//! (none was available when this was written); the importer is tested against the JSON shapes
//! it is designed to read:
//!
//! ```text
//! copyq eval "tab('&clipboard'); var out = []; for (var i = 0; i < size(); ++i) out.push({text: str(read(i))}); print(JSON.stringify({tab: 'clipboard', items: out}))" > copyq.json
//! ```
//!
//! (`tab(name)` selects the tab, `size()` counts its items, `read(i)` returns the item's
//! `text/plain` bytes.) The importer accepts, in this order of preference:
//!
//! 1. an object `{"tab": "name", "items": [...]}`;
//! 2. an array of items;
//! 3. one JSON value per line (JSON Lines), each an item.
//!
//! An item is a string, or an object with `text` (required), and optionally `title`, `tags`
//! (an array of strings; at most 32 are read per item and 5000 distinct ones per import) and
//! `pinned` (a boolean). Unknown fields are ignored. Only text is
//! imported; `CopyQ`'s other formats (images, HTML) are not part of this path.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde_json::Value;
use snipdeck_core::{Name, Timestamp};
use snipdeck_store::name_key;

use crate::error::{Error, Result};
use crate::import::{ImportOptions, board_name, finish};
use crate::limits::TEXT_LIMIT;
use crate::plan::{Library, Plan, PlanBoard, PlanItem, PlanList};
use crate::report::{ImportReport, Warning};

use super::{MEMORY_BUDGET, clean_label, decode_text, note_lossy, title_from};

/// The most tags read from one item; the rest are ignored.
const MAX_TAGS_PER_ITEM: usize = 32;

/// The most distinct tags one import creates; later new names are ignored.
const MAX_TAGS: usize = 5_000;

/// The tag names met so far, found again by the store's own key (`name_key`), so a lookup does
/// not walk the list and agrees with what the store will accept.
#[derive(Default)]
struct Tags {
    names: Vec<Name>,
    by_key: HashMap<String, usize>,
    /// Whether the user has been told that a cap cut tags off.
    warned: bool,
}

impl Tags {
    /// The index of `name`, adding it unless the total cap is reached.
    fn index_of(&mut self, name: Name, report: &mut ImportReport) -> Option<usize> {
        let key = name_key(name.as_str());
        if let Some(index) = self.by_key.get(&key) {
            return Some(*index);
        }
        if self.names.len() >= MAX_TAGS {
            self.warn(report);
            return None;
        }
        self.names.push(name);
        self.by_key.insert(key, self.names.len() - 1);
        Some(self.names.len() - 1)
    }

    fn warn(&mut self, report: &mut ImportReport) {
        if !self.warned {
            self.warned = true;
            report.warn(Warning::Truncated {
                reason: format!(
                    "only {MAX_TAGS_PER_ITEM} tags per item and {MAX_TAGS} tags in all were read"
                ),
            });
        }
    }
}

/// Imports the JSON file written by the `copyq eval` command shown in the module docs into a new
/// board (named `Imported from CopyQ` unless the options say otherwise) with one list named after
/// the `CopyQ` tab.
///
/// # Errors
///
/// [`Error::File`] if the file cannot be read, [`Error::Malformed`] if it is not one of the
/// accepted shapes, [`Error::LimitExceeded`] if it is over 256 MiB, and storage errors. Items
/// that are empty, too large or of the wrong type are skipped with a warning.
pub fn import_copyq_json<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let file_error = |source| Error::File {
        path: path.to_owned(),
        source,
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(file_error)?
        .take(MEMORY_BUDGET + 1)
        .read_to_end(&mut bytes)
        .map_err(file_error)?;
    if bytes.len() as u64 > MEMORY_BUDGET {
        return Err(Error::LimitExceeded {
            what: "CopyQ file size",
            limit: MEMORY_BUDGET,
        });
    }
    import_copyq_bytes(lib, &bytes, now, options)
}

pub(crate) fn import_copyq_bytes<L: Library>(
    lib: &mut L,
    bytes: &[u8],
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let (text, how) = decode_text(bytes);
    note_lossy(how, "the CopyQ file", &mut report);
    let (tab, values) = parse(&text)?;

    let mut tags = Tags::default();
    let mut items = Vec::new();
    let mut budget = MEMORY_BUDGET;
    for (number, value) in values.iter().enumerate() {
        if items.len() >= options.limits.max_items {
            report.warn(Warning::Truncated {
                reason: format!(
                    "only the first {} items were read",
                    options.limits.max_items
                ),
            });
            break;
        }
        let label = format!("CopyQ item {}", number + 1);
        match item_from(value, &label, now, &mut tags, &mut report) {
            Some(item) => {
                let len = item_len(&item);
                if len > budget {
                    report.warn(Warning::Truncated {
                        reason: "reached the import memory limit".to_owned(),
                    });
                    break;
                }
                budget -= len;
                items.push(item);
            }
            None => report.items_skipped += 1,
        }
    }

    let mut plan = Plan {
        tags: tags.names,
        ..Plan::default()
    };
    if !items.is_empty() {
        let list_name = tab
            .map(|t| clean_label(&t, 100))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "CopyQ".to_owned());
        let mut list = PlanList::new(board_name(&list_name, "CopyQ")?, now);
        list.items = items;
        plan.boards.push(PlanBoard {
            name: board_name(
                options
                    .board_name
                    .as_deref()
                    .unwrap_or("Imported from CopyQ"),
                "Imported from CopyQ",
            )?,
            created_at: now,
            updated_at: now,
            lists: vec![list],
        });
    }
    finish(lib, plan, options, report)
}

fn item_len(item: &PlanItem) -> u64 {
    item.formats
        .iter()
        .map(|f| match f {
            crate::plan::PlanFormat::Single {
                source: crate::plan::Source::Bytes(b),
                ..
            } => b.len() as u64,
            _ => 0,
        })
        .sum()
}

/// The tab name and the item values of whichever accepted shape `text` has.
fn parse(text: &str) -> Result<(Option<String>, Vec<Value>)> {
    let whole: core::result::Result<Value, _> = serde_json::from_str(text);
    match whole {
        Ok(Value::Object(mut map)) => {
            let tab = map.get("tab").and_then(Value::as_str).map(str::to_owned);
            match map.remove("items") {
                Some(Value::Array(items)) => Ok((tab, items)),
                _ => Err(Error::malformed(
                    "the CopyQ file",
                    "an object needs an `items` array",
                )),
            }
        }
        Ok(Value::Array(items)) => Ok((None, items)),
        Ok(other @ Value::String(_)) => Ok((None, vec![other])),
        Ok(_) => Err(Error::malformed(
            "the CopyQ file",
            "expected an object with `items`, an array, or one JSON value per line",
        )),
        Err(whole_error) => {
            // JSON Lines: every non-blank line is a value.
            let mut values = Vec::new();
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                match serde_json::from_str::<Value>(line) {
                    Ok(v) => values.push(v),
                    Err(_) => {
                        return Err(Error::malformed(
                            "the CopyQ file",
                            format!("not valid JSON ({whole_error})"),
                        ));
                    }
                }
            }
            if values.is_empty() {
                return Err(Error::malformed("the CopyQ file", "it is empty"));
            }
            Ok((None, values))
        }
    }
}

fn item_from(
    value: &Value,
    label: &str,
    now: Timestamp,
    tags: &mut Tags,
    report: &mut ImportReport,
) -> Option<PlanItem> {
    let (text, title, item_tags, pinned) = match value {
        Value::String(s) => (s.as_str(), None, Vec::new(), false),
        Value::Object(map) => {
            let Some(text) = map.get("text").and_then(Value::as_str) else {
                report.warn(Warning::Unreadable {
                    name: label.to_owned(),
                    reason: "no `text` string".to_owned(),
                });
                return None;
            };
            let item_tags = map
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .take(MAX_TAGS_PER_ITEM + 1)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (
                text,
                map.get("title").and_then(Value::as_str),
                item_tags,
                map.get("pinned").and_then(Value::as_bool).unwrap_or(false),
            )
        }
        _ => {
            report.warn(Warning::Unreadable {
                name: label.to_owned(),
                reason: "not a string or an object".to_owned(),
            });
            return None;
        }
    };
    if text.trim().is_empty() {
        report.warn(Warning::Empty {
            name: label.to_owned(),
        });
        return None;
    }
    if text.len() as u64 > TEXT_LIMIT {
        report.warn(Warning::TooLarge {
            name: label.to_owned(),
            size: text.len() as u64,
        });
        return None;
    }
    let title = title
        .map(|t| clean_label(t, 200))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| title_from(text));
    let mut item = PlanItem::text(title, text.to_owned(), now);
    item.pinned = pinned;
    if item_tags.len() > MAX_TAGS_PER_ITEM {
        tags.warn(report);
    }
    for tag in item_tags.into_iter().take(MAX_TAGS_PER_ITEM) {
        let Ok(name) = Name::new(&clean_label(&tag, Name::MAX_CHARS)) else {
            continue;
        };
        if let Some(index) = tags.index_of(name, report) {
            item.tags.insert(index);
        }
    }
    Some(item)
}
