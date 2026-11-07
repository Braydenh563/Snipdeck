//! Importing from Beeftext.
//!
//! Reads the JSON file of Beeftext's *Export* command: an object with `groups` (each with a
//! `uuid` and a `name`) and `combos` (each with `keyword`, `snippet`, `name`, and `group`, the
//! uuid of its group). A bare array of combos is accepted too, and the older key `text` is read
//! when `snippet` is missing. This is written from the shape of Beeftext's own exports as
//! remembered; no Beeftext was available to produce a file, so unknown fields are ignored and
//! missing ones tolerated.
//!
//! Groups become lists. Variables map where Snipdeck has an equal: `#{clipboard}`,
//! `#{cursor}`, `#{date}` (Beeftext's locale date, as Snipdeck's `{date}`), `#{time}` and
//! `#{input:Description}` (as `{field:Description}`, a Supporter feature). Everything else
//! (`#{dateTime:...}`, whose format is Qt's, `#{powershell:...}`, `#{combo:...}`,
//! `#{envVar:...}`, `#{upper:...}`, ...) stays as written, with a note.
//!
//! The file is read whole (at most `Limits::max_json_bytes`); `serde_json`'s recursion limit
//! turns deep nesting into an error.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;
use snipdeck_core::Timestamp;

use crate::error::{Error, Result};
use crate::import::ImportOptions;
use crate::plan::Library;
use crate::report::{ImportReport, Warning};

use super::expander::{Snip, Tpl, import_snips, placeholder_name, read_capped};
use super::{decode_text, note_lossy};

/// Imports a Beeftext JSON export into a new board named `Imported from Beeftext` (unless the
/// options say otherwise), one list per group.
///
/// # Errors
///
/// [`Error::File`] if the file cannot be read, [`Error::LimitExceeded`] if it is over the size
/// limit, [`Error::Malformed`] if it is not an export, and storage errors. Combos with no
/// snippet, an empty snippet or one over 5 MB are skipped with a warning.
pub fn import_beeftext<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let bytes = read_capped(path, "Beeftext file size", options)?;
    import_beeftext_bytes(lib, &bytes, now, options)
}

fn import_beeftext_bytes<L: Library>(
    lib: &mut L,
    bytes: &[u8],
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let (text, how) = decode_text(bytes);
    note_lossy(how, "the Beeftext file", &mut report);
    let root: Value = serde_json::from_str(&text)
        .map_err(|_| Error::malformed("the Beeftext file", "it is not valid JSON"))?;
    let (groups, combos) = match &root {
        Value::Array(combos) => (None, combos),
        Value::Object(map) => match map.get("combos") {
            Some(Value::Array(combos)) => (map.get("groups"), combos),
            _ => {
                return Err(Error::malformed(
                    "the Beeftext file",
                    "it has no list of combos",
                ));
            }
        },
        _ => {
            return Err(Error::malformed(
                "the Beeftext file",
                "it is not a Beeftext export",
            ));
        }
    };
    let mut names: HashMap<&str, &str> = HashMap::new();
    if let Some(Value::Array(groups)) = groups {
        for group in groups {
            if let (Some(uuid), Some(name)) = (str_of(group, "uuid"), str_of(group, "name")) {
                names.insert(uuid, name);
            }
        }
    }

    let mut snips = Vec::new();
    for (index, combo) in combos.iter().enumerate() {
        let label = format!("combo {}", index + 1);
        let Some(snippet) = str_of(combo, "snippet").or_else(|| str_of(combo, "text")) else {
            report.items_skipped += 1;
            report.warn(Warning::Empty { name: label });
            continue;
        };
        let name = str_of(combo, "name").filter(|n| !n.trim().is_empty());
        let body = convert(snippet, name.unwrap_or(&label), &mut report);
        snips.push(Snip {
            group: str_of(combo, "group")
                .and_then(|g| names.get(g))
                .map(|n| (*n).to_owned()),
            name: name.map(str::to_owned),
            abbreviation: str_of(combo, "keyword").map(str::to_owned),
            body,
        });
    }
    import_snips(
        lib,
        snips,
        ("Imported from Beeftext", "Beeftext"),
        now,
        options,
        report,
    )
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str()
}

/// Beeftext's `#{variable}` syntax to Snipdeck placeholders.
fn convert(src: &str, label: &str, report: &mut ImportReport) -> super::expander::Body {
    let mut tpl = Tpl::default();
    let mut rest = src;
    let mut unmapped: Vec<String> = Vec::new();
    while let Some(start) = rest.find("#{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else { break };
        let inner = &after[..end];
        let (key, arg) = match inner.split_once(':') {
            Some((k, a)) => (k.trim(), Some(a)),
            None => (inner.trim(), None),
        };
        let placeholder = match (key.to_ascii_lowercase().as_str(), arg) {
            ("clipboard", None) => Some("{clipboard}".to_owned()),
            ("cursor", None) => Some("{cursor}".to_owned()),
            ("date", None) => Some("{date}".to_owned()),
            ("time", None) => Some("{time}".to_owned()),
            ("input", Some(description)) => {
                placeholder_name(description).map(|n| format!("{{field:{n}}}"))
            }
            _ => None,
        };
        tpl.lit(&rest[..start]);
        if let Some(p) = placeholder {
            tpl.ph(&p);
        } else {
            tpl.lit(&rest[start..=start + 2 + end]);
            if !unmapped.iter().any(|u| u == key) {
                unmapped.push(key.to_owned());
            }
        }
        rest = &after[end + 1..];
    }
    tpl.lit(rest);
    if !unmapped.is_empty() {
        report.warn(Warning::Note(format!(
            "{label}: no Snipdeck equivalent for #{{{}}}; left as written",
            unmapped.join("}, #{")
        )));
    }
    tpl.finish(src, label, report)
}
