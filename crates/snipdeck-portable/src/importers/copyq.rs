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
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
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
    let scan = parse(&text, now, options.limits.max_items)?;
    report.items_skipped += scan.skipped;
    for warning in scan.report.warnings {
        report.warn(warning);
    }
    report.warnings_dropped += scan.report.warnings_dropped;
    let tab = scan.tab;
    let tags = scan.tags;
    let items = scan.items;

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

/// What one pass over the file collected. Items are turned into plan items as they are read, so
/// no tree of the whole file ever exists; the scan stops at `max_items` items, at four times
/// that many elements looked at, and at the memory budget.
struct Scan {
    items: Vec<PlanItem>,
    tags: Tags,
    report: ImportReport,
    skipped: usize,
    tab: Option<String>,
    now: Timestamp,
    max_items: usize,
    looked_at: usize,
    budget: u64,
    stopped: bool,
}

impl Scan {
    fn new(now: Timestamp, max_items: usize) -> Self {
        Self {
            items: Vec::new(),
            tags: Tags::default(),
            report: ImportReport::default(),
            skipped: 0,
            tab: None,
            now,
            max_items,
            looked_at: 0,
            budget: MEMORY_BUDGET,
            stopped: false,
        }
    }

    fn truncated(&mut self, reason: String) {
        self.report.warn(Warning::Truncated { reason });
        self.stopped = true;
    }

    /// Takes the next element. `false` means the scan is over (a cap was reached).
    fn take(&mut self, entry: &Entry) -> bool {
        if self.items.len() >= self.max_items {
            self.truncated(format!("only the first {} items were read", self.max_items));
            return false;
        }
        if self.looked_at >= self.max_items.saturating_mul(4) {
            self.truncated(format!(
                "stopped after looking at {} entries",
                self.looked_at
            ));
            return false;
        }
        self.looked_at += 1;
        let label = format!("CopyQ item {}", self.looked_at);
        let now = self.now;
        match item_from(entry, &label, now, &mut self.tags, &mut self.report) {
            Some(item) => {
                let len = item_len(&item);
                if len > self.budget {
                    self.truncated("reached the import memory limit".to_owned());
                    return false;
                }
                self.budget -= len;
                self.items.push(item);
            }
            None => self.skipped += 1,
        }
        true
    }
}

/// The error a visitor returns to end the scan early; the caller checks `Scan::stopped`.
fn stop<E: de::Error>() -> E {
    E::custom("scan stopped at a limit")
}

/// One array element as far as the importer cares: nothing of an unexpected shape is kept, and
/// an array of tags keeps only the first few strings.
enum Entry {
    Text(String),
    Object {
        text: Option<String>,
        title: Option<String>,
        tags: Vec<String>,
        pinned: bool,
    },
    Other,
}

/// A value of any shape, reduced to what an item field can use.
enum Lax {
    Str(String),
    Bool(bool),
    Strs(Vec<String>),
    Other,
}

/// An array element of `tags`: a string, or anything else (skipped, not kept).
struct Leaf(Option<String>);

fn drain_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> core::result::Result<(), A::Error> {
    while seq.next_element::<IgnoredAny>()?.is_some() {}
    Ok(())
}

fn drain_map<'de, A: MapAccess<'de>>(mut map: A) -> core::result::Result<(), A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(())
}

macro_rules! scalars_are_other {
    ($result:expr) => {
        fn visit_bool<E: de::Error>(self, _: bool) -> core::result::Result<Self::Value, E> {
            Ok($result)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> core::result::Result<Self::Value, E> {
            Ok($result)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> core::result::Result<Self::Value, E> {
            Ok($result)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> core::result::Result<Self::Value, E> {
            Ok($result)
        }
        fn visit_unit<E: de::Error>(self) -> core::result::Result<Self::Value, E> {
            Ok($result)
        }
        fn visit_none<E: de::Error>(self) -> core::result::Result<Self::Value, E> {
            Ok($result)
        }
    };
}

impl<'de> Deserialize<'de> for Leaf {
    fn deserialize<D: Deserializer<'de>>(d: D) -> core::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Leaf;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> core::result::Result<Leaf, E> {
                Ok(Leaf(Some(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> core::result::Result<Leaf, E> {
                Ok(Leaf(Some(v)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> core::result::Result<Leaf, A::Error> {
                drain_seq(seq)?;
                Ok(Leaf(None))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> core::result::Result<Leaf, A::Error> {
                drain_map(map)?;
                Ok(Leaf(None))
            }
            scalars_are_other!(Leaf(None));
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for Lax {
    fn deserialize<D: Deserializer<'de>>(d: D) -> core::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Lax;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> core::result::Result<Lax, E> {
                Ok(Lax::Str(v.to_owned()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> core::result::Result<Lax, E> {
                Ok(Lax::Str(v))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> core::result::Result<Lax, E> {
                Ok(Lax::Bool(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> core::result::Result<Lax, A::Error> {
                let mut out = Vec::new();
                while let Some(Leaf(leaf)) = seq.next_element::<Leaf>()? {
                    if let Some(tag) = leaf
                        && out.len() <= MAX_TAGS_PER_ITEM
                    {
                        out.push(tag);
                    }
                }
                Ok(Lax::Strs(out))
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> core::result::Result<Lax, E> {
                Ok(Lax::Other)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> core::result::Result<Lax, E> {
                Ok(Lax::Other)
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> core::result::Result<Lax, E> {
                Ok(Lax::Other)
            }
            fn visit_unit<E: de::Error>(self) -> core::result::Result<Lax, E> {
                Ok(Lax::Other)
            }
            fn visit_none<E: de::Error>(self) -> core::result::Result<Lax, E> {
                Ok(Lax::Other)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> core::result::Result<Lax, A::Error> {
                drain_map(map)?;
                Ok(Lax::Other)
            }
        }
        d.deserialize_any(V)
    }
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> core::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Entry;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> core::result::Result<Entry, E> {
                Ok(Entry::Text(v.to_owned()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> core::result::Result<Entry, E> {
                Ok(Entry::Text(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> core::result::Result<Entry, A::Error> {
                drain_seq(seq)?;
                Ok(Entry::Other)
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> core::result::Result<Entry, A::Error> {
                let (mut text, mut title, mut tags, mut pinned) = (None, None, Vec::new(), false);
                while let Some(key) = map.next_key::<String>()? {
                    match (key.as_str(), map.next_value::<Lax>()?) {
                        ("text", Lax::Str(v)) => text = Some(v),
                        ("title", Lax::Str(v)) => title = Some(v),
                        ("tags", Lax::Strs(v)) => tags = v,
                        ("pinned", Lax::Bool(v)) => pinned = v,
                        _ => {}
                    }
                }
                Ok(Entry::Object {
                    text,
                    title,
                    tags,
                    pinned,
                })
            }
            scalars_are_other!(Entry::Other);
        }
        d.deserialize_any(V)
    }
}

fn scan_seq<'de, A: SeqAccess<'de>>(
    scan: &mut Scan,
    mut seq: A,
) -> core::result::Result<(), A::Error> {
    while let Some(entry) = seq.next_element::<Entry>()? {
        if !scan.take(&entry) {
            return Err(stop());
        }
    }
    Ok(())
}

/// The items array: each element is handled as it is read.
struct Items<'a>(&'a mut Scan);

impl<'de> DeserializeSeed<'de> for Items<'_> {
    /// Whether the value was an array.
    type Value = bool;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> core::result::Result<bool, D::Error> {
        struct V<'a>(&'a mut Scan);
        impl<'de> Visitor<'de> for V<'_> {
            type Value = bool;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> core::result::Result<bool, A::Error> {
                scan_seq(self.0, seq)?;
                Ok(true)
            }
            fn visit_str<E: de::Error>(self, _: &str) -> core::result::Result<bool, E> {
                Ok(false)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> core::result::Result<bool, A::Error> {
                drain_map(map)?;
                Ok(false)
            }
            scalars_are_other!(false);
        }
        d.deserialize_any(V(self.0))
    }
}

/// What the whole file turned out to be.
enum Shape {
    /// An object; `items` says whether it had an `items` array.
    Object {
        items: bool,
    },
    Array,
    /// A bare string: one item.
    Text,
    Scalar,
}

struct Top<'a>(&'a mut Scan);

impl<'de> Visitor<'de> for Top<'_> {
    type Value = Shape;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> core::result::Result<Shape, A::Error> {
        scan_seq(self.0, seq)?;
        Ok(Shape::Array)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> core::result::Result<Shape, A::Error> {
        let mut items = false;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "tab" => {
                    if let Lax::Str(v) = map.next_value::<Lax>()? {
                        self.0.tab = Some(v);
                    }
                }
                "items" => items = map.next_value_seed(Items(self.0))?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(Shape::Object { items })
    }
    fn visit_str<E: de::Error>(self, v: &str) -> core::result::Result<Shape, E> {
        self.0.take(&Entry::Text(v.to_owned()));
        Ok(Shape::Text)
    }
    scalars_are_other!(Shape::Scalar);
}

/// The tab name and the items of whichever accepted shape `text` has, read in one pass.
fn parse(text: &str, now: Timestamp, max_items: usize) -> Result<Scan> {
    let mut scan = Scan::new(now, max_items);
    let mut de = serde_json::Deserializer::from_str(text);
    let first = Deserializer::deserialize_any(&mut de, Top(&mut scan));
    let first = match first {
        Ok(shape) => de.end().map(|()| shape),
        Err(e) => Err(e),
    };
    match first {
        Ok(Shape::Object { items: true } | Shape::Array | Shape::Text) => Ok(scan),
        Ok(Shape::Object { .. }) => Err(Error::malformed(
            "the CopyQ file",
            "an object needs an `items` array",
        )),
        Ok(Shape::Scalar) => Err(Error::malformed(
            "the CopyQ file",
            "expected an object with `items`, an array, or one JSON value per line",
        )),
        // A cap ended the scan on purpose: what was collected stands.
        Err(_) if scan.stopped => Ok(scan),
        Err(whole_error) => {
            // JSON Lines: every non-blank line is a value.
            let mut scan = Scan::new(now, max_items);
            let mut any = false;
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                any = true;
                match serde_json::from_str::<Entry>(line) {
                    Ok(entry) => {
                        if !scan.take(&entry) {
                            break;
                        }
                    }
                    Err(_) => {
                        return Err(Error::malformed(
                            "the CopyQ file",
                            format!("not valid JSON ({whole_error})"),
                        ));
                    }
                }
            }
            if !any {
                return Err(Error::malformed("the CopyQ file", "it is empty"));
            }
            Ok(scan)
        }
    }
}

fn item_from(
    entry: &Entry,
    label: &str,
    now: Timestamp,
    tags: &mut Tags,
    report: &mut ImportReport,
) -> Option<PlanItem> {
    let (text, title, item_tags, pinned) = match entry {
        Entry::Text(s) => (s.as_str(), None, &[][..], false),
        Entry::Object {
            text,
            title,
            tags,
            pinned,
        } => {
            let Some(text) = text.as_deref() else {
                report.warn(Warning::Unreadable {
                    name: label.to_owned(),
                    reason: "no `text` string".to_owned(),
                });
                return None;
            };
            (text, title.as_deref(), tags.as_slice(), *pinned)
        }
        Entry::Other => {
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
    for tag in item_tags.iter().take(MAX_TAGS_PER_ITEM) {
        let Ok(name) = Name::new(&clean_label(tag, Name::MAX_CHARS)) else {
            continue;
        };
        if let Some(index) = tags.index_of(name, report) {
            item.tags.insert(index);
        }
    }
    Some(item)
}
