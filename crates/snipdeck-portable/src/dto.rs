//! The JSON documents of the `.snipdeck` format, version 1. `docs/format.md` is the normative
//! description; these types are its implementation.
//!
//! Compatibility rule: **no `deny_unknown_fields` anywhere.** A reader of major version 1 must
//! ignore fields it does not know, so a later 1.x can add fields without breaking it. Fields
//! that were required in 1.0 stay required.

use std::cell::Cell;

use serde::de::{Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// The `format` value of every manifest.
pub(crate) const FORMAT_NAME: &str = "snipdeck-archive";
/// The newest major version this build reads and the one it writes.
pub(crate) const MAJOR: u32 = 1;
/// The minor version this build writes.
pub(crate) const MINOR: u32 = 1;
/// The `encryption` value when payloads are `SNENC2` envelopes (`docs/crypto.md`).
pub(crate) const ENCRYPTION: &str = "SNENC2";

/// `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub format: String,
    pub format_version: String,
    pub app_version: String,
    pub created_at: String,
    pub counts: Counts,
    /// `"SNENC2"` when at least one payload is encrypted; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption: Option<String>,
    /// Sensitive items the exporter left out; absent when none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub sensitive_omitted: u64,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if signature"
)]
fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// What the archive claims to hold; the importer checks it against the data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct Counts {
    pub boards: u64,
    pub lists: u64,
    pub items: u64,
    pub tags: u64,
    pub assets: u64,
}

/// Just enough of the manifest to decide whether the rest can be read at all.
#[derive(Debug, Deserialize)]
pub(crate) struct Header {
    pub format: Option<String>,
    pub format_version: Option<String>,
}

/// `items.json`.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ItemsDoc {
    #[serde(default)]
    pub tags: Vec<TagDto>,
    pub boards: Vec<BoardDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TagDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct BoardDto {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub lists: Vec<ListDto>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ListDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub accent: Option<String>,
    /// Added in 1.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub view: String,
    pub sort: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(deserialize_with = "counted_items")]
    pub items: Vec<ItemDto>,
}

thread_local! {
    /// How many more items [`parse_items_doc`] accepts on this thread.
    static ITEM_BUDGET: Cell<usize> = const { Cell::new(usize::MAX) };
    /// Set when [`counted_items`] stopped the parse for lack of budget.
    static ITEM_BUDGET_SPENT: Cell<bool> = const { Cell::new(false) };
}

/// Why [`parse_items_doc`] failed.
#[derive(Debug)]
pub(crate) enum ItemsParseError {
    /// More than the allowed number of items; the rest of the document was not parsed.
    TooManyItems,
    /// Not valid JSON for [`ItemsDoc`].
    Malformed(String),
}

/// Parses `items.json`, stopping at the first item past `max_items` instead of building the
/// whole (up to 64 MiB) document first.
pub(crate) fn parse_items_doc(bytes: &[u8], max_items: usize) -> Result<ItemsDoc, ItemsParseError> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ITEM_BUDGET.with(|b| b.set(usize::MAX));
        }
    }
    let _reset = Reset;
    ITEM_BUDGET.with(|b| b.set(max_items));
    ITEM_BUDGET_SPENT.with(|s| s.set(false));
    serde_json::from_slice(bytes).map_err(|e| {
        if ITEM_BUDGET_SPENT.with(Cell::get) {
            ItemsParseError::TooManyItems
        } else {
            ItemsParseError::Malformed(e.to_string())
        }
    })
}

/// Reads a list's items, charging each against the budget [`parse_items_doc`] set (no budget
/// elsewhere, so other readers of [`ListDto`] are not limited).
fn counted_items<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<ItemDto>, D::Error> {
    struct Counted;
    impl<'de> Visitor<'de> for Counted {
        type Value = Vec<ItemDto>;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a list of items")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut items = Vec::new();
            while let Some(item) = seq.next_element::<ItemDto>()? {
                let left = ITEM_BUDGET.with(Cell::get);
                if left == 0 {
                    ITEM_BUDGET_SPENT.with(|s| s.set(true));
                    return Err(A::Error::custom("too many items"));
                }
                ITEM_BUDGET.with(|b| b.set(left.saturating_sub(1)));
                items.push(item);
            }
            Ok(items)
        }
    }
    d.deserialize_seq(Counted)
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ItemDto {
    pub id: String,
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub always_plain: bool,
    /// The colour label; added in 1.1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub use_count: u32,
    #[serde(default)]
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub source_app: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Tag ids, from the top-level `tags`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// A readable excerpt for people reading the JSON; the importer ignores it. Absent for
    /// sensitive items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    pub formats: Vec<FormatDto>,
}

/// One clipboard format of an item. `kind` is `text`, `html`, `rtf`, `image/png` or `files`.
/// Every kind but `files` has `asset`, `size` and `encrypted`; `files` has `files`.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FormatDto {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub encrypted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileDto>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FileDto {
    pub name: String,
    pub asset: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub encrypted: bool,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if signature"
)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str =
        r#"{"id":"i","title":"t","kind":"text","created_at":"c","updated_at":"u","formats":[]}"#;

    fn doc(items: usize, tail: &str) -> String {
        let items = vec![ITEM; items].join(",");
        format!(
            r#"{{"boards":[{{"id":"b","name":"n","created_at":"c","updated_at":"u","lists":[{{"id":"l","name":"n","view":"v","sort":"s","created_at":"c","updated_at":"u","items":[{items}{tail}]}}]}}]}}"#
        )
    }

    #[test]
    fn the_item_cap_stops_the_parse_at_the_first_item_over_it() {
        let ok = parse_items_doc(doc(3, "").as_bytes(), 3).unwrap();
        assert_eq!(ok.boards[0].lists[0].items.len(), 3);
        // Four tiny items; the garbage after them is never reached.
        let over = doc(4, ",}}}garbage that would be a syntax error");
        assert!(matches!(
            parse_items_doc(over.as_bytes(), 3),
            Err(ItemsParseError::TooManyItems)
        ));
        // Without the cap in play, the same garbage is plainly malformed.
        assert!(matches!(
            parse_items_doc(over.as_bytes(), 100),
            Err(ItemsParseError::Malformed(_))
        ));
    }

    #[test]
    fn the_budget_does_not_leak_to_later_parses() {
        let _ = parse_items_doc(doc(4, "").as_bytes(), 1);
        let again = parse_items_doc(doc(4, "").as_bytes(), 10).unwrap();
        assert_eq!(again.boards[0].lists[0].items.len(), 4);
        assert!(serde_json::from_str::<ItemsDoc>(&doc(9, "")).is_ok());
    }
}
