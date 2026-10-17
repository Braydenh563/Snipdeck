//! The JSON documents of the `.snipdeck` format, version 1. `docs/format.md` is the normative
//! description; these types are its implementation.
//!
//! Compatibility rule: **no `deny_unknown_fields` anywhere.** A reader of major version 1 must
//! ignore fields it does not know, so a later 1.x can add fields without breaking it. Fields
//! that were required in 1.0 stay required.

use serde::{Deserialize, Serialize};

/// The `format` value of every manifest.
pub(crate) const FORMAT_NAME: &str = "snipdeck-archive";
/// The newest major version this build reads and the one it writes.
pub(crate) const MAJOR: u32 = 1;
/// The minor version this build writes.
pub(crate) const MINOR: u32 = 0;
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
    pub view: String,
    pub sort: String,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<ItemDto>,
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
