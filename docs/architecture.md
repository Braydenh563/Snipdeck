# Architecture

How the code is split and the decisions behind the `snipdeck-core` contract. Product behaviour
is in [`SPEC.md`](SPEC.md); decisions with wider scope are in [`adr/`](adr/).

## Crates

| Crate | Status | Owns | May depend on |
|---|---|---|---|
| `snipdeck-core` | M1, done | Domain types, template engine, fuzzy search, use-pipeline logic, entitlements, licence verification | nothing internal |
| `snipdeck-store` | M1, done | SQLite schema and migrations, repositories, blob store, FTS5, trash, backup, export/import, encryption at rest | core |
| `snipdeck-portable` | M1, done | `.snipdeck` export/import ([`format.md`](format.md)), folder/Ditto/CopyQ importers, daily backup | core, store |
| `snipdeck-platform` | M2, written; Windows backend unverified on Windows | Clipboard, hotkeys, Target tracking, paste injection, window styles, capture, OS OCR | core |
| `snipdeck-ocr-rapid` | M5, planned (crate not created) | Optional RapidOCR backend | core |
| `snipdeck-ui` | M3, in progress (components, Panel, Picker, fill form) | Slint tokens, components, windows | nothing internal (takes plain structs and models) |
| `snipdeck-app` | M3, in progress (Panel, tray, settings; no Picker, Main window or editors yet) | The binary: wiring, tray, settings, i18n, updater | all of the above (today it uses core, store, platform and ui) |

**Core is pure.** No file, network, clipboard or OS calls and no OS-specific dependencies, so
all of it is tested on Linux CI. Whatever needs the outside world takes it as an argument: a
`now: Timestamp`, the clipboard text for `{clipboard}`, the entitlements. The one exception is id
generation, which reads the clock and the OS random source through the portable `uuid` crate.

**Unsafe code** is forbidden in core and store (`#![forbid(unsafe_code)]`). The workspace lint is
`deny` so `snipdeck-platform` can allow it module by module, each block with a `// SAFETY:` note.

## Contract decisions

### Ids: UUIDv7 newtypes
`BoardId`, `ListId`, `ItemId`, `TagId` each wrap a `uuid::Uuid`, so ids can't be mixed up.
UUIDs can be minted offline with no coordination, so export/import and merging backups never
collide. Version 7 starts with a millisecond timestamp, so new rows append to the end of SQLite's
B-tree indexes instead of landing on random pages, and ids sort by creation time. Parsing accepts
any UUID version so imported ids survive.

### Timestamps: `jiff`
`snipdeck_core::Timestamp` is `jiff::Timestamp` (re-exported). Chosen over `time` and `chrono`
because the template engine needs both a strftime-style formatter for `{datetime:FORMAT}` and
time-zone-correct local dates for `{date}`, and jiff does both with a small, well-documented API.
Core enables only jiff's `std` feature; the app turns on the system time-zone features. Core never
reads the clock: callers pass `now`.

### Content: inline up to 64 KiB, else a blob
A payload of at most `INLINE_LIMIT` (64 KiB) bytes is `Content::Inline` and lives in the
database row. Anything larger is `Content::Blob { blob, len }`, where `BlobRef` is the BLAKE3
hash of the bytes and names the file in `blobs/` (ADR-0002). The rule is size-only, whatever the
format, so equal bytes always have one representation and a small PNG doesn't need a file.
`BlobStore::put` in the store is the only code that applies the rule and the only code that
hashes. `Inline`'s `Debug` prints only the length so sensitive content never reaches a log.

### Items don't carry their content
`Item` is metadata plus a short `preview`; its `Formats` are loaded on their own
(`ItemRepo::formats`) when an item is used, edited or exported. The panel and picker hold every
item of a board in memory, and with up to 64 KiB inline per format, 10 000 items with content
would break the 40 MB idle budget (SPEC §9). `Item::preview_of` builds the preview: up to 200
characters, which covers the 64 characters in-memory search matches on and three comfortable-view
lines.

### Formats
`Formats` is non-empty, holds at most one format per kind and keeps preference order (the first
is primary; the order is kept when writing back to the clipboard). Text and HTML are UTF-8; HTML
is the bare fragment (the platform adds and strips the Windows `CF_HTML` header); images are PNG.
`Format::Files` holds **copies** of the files, each a `FileEntry` whose name must be one valid
Windows path component (no reserved device names such as `CON` or `nul.txt`, at most 255 UTF-16
units, no trailing dot or space, no bidi controls), and the names within one item are distinct
ignoring case, so pasting an old item still works, no name can escape the folder it is written
to and no file overwrites another.

### Sensitive is a flag, not a kind
`ItemKind` (text, template, code, link, image, file) describes content; `Item::sensitive` is
separate. DESIGN.md's "Sensitive" leading row is a UI rule: draw the lock whenever the flag is
set. One source of truth, and clearing the flag restores the real kind. A sensitive item's
preview is empty, so no plaintext of it is stored outside its content, which Pro encrypts; the
`•••• 7f3a` hint in DESIGN.md is therefore not shown in v1.

### Order belongs to storage
Domain types have no position field. Repositories return siblings in manual order and take a
`Placement` (`First`, `Last`, `After(id)`) for inserts and moves. Internally each board, list and
item row has an `INTEGER` rank, unique within its parent:

- `First`/`Last`: the current minimum minus, or maximum plus, `1 << 16`.
- `After(x)`: the midpoint between `x` and its successor (or `x + (1 << 16)` if it is last).
- No integer left between two neighbours (after about 16 drops into the same gap): renumber that
  parent's children `1 << 16` apart in the same transaction, then retry.

Fractional string keys were considered: they never need renumbering, which matters when several
replicas edit concurrently and merge. Snipdeck has one writer (ADR-0002), so their benefit doesn't
apply, while their cost does: keys grow without bound when items are repeatedly added at one end.
Dense integers were rejected because every move rewrites every row after it. Sparse integers move
one row per drag in the common case, and a rare renumber of a 10 000-item list is one transaction.
Sorting by most used or recently used is done in memory from `use_count` and `last_used_at`.

### Entitlements are data
`Entitlements` wraps a `Tier`; `allows(Feature)`, `require(Feature)` and
`can_create_board(existing)` answer every SPEC §6 question. Losing paid features never hides data
(a Free user with five boards keeps them and can't add a sixth). Licence verification is in
`snipdeck_core::licence` and returns `Entitlements`; see [`licensing.md`](licensing.md).

### Stable keys, no serde in core
Stored enums (`ItemKind`, `FormatKind`, `ListAccent`, `ListView`, `ListSort`, `Feature`) have a
string `key()` and `FromStr`; storage and export write keys, never discriminants, and a released
key never changes. Core does not derive `serde`: the `.snipdeck` export is a versioned set of
DTOs in the store, so changing a domain type never silently changes the file format.

### Errors
Each crate has one `thiserror` enum. Core's `Error` is `Clone + PartialEq` so tests can compare
results; it has `ProRequired(Feature)` for the upsell path. The store's `Error` wraps core's.

### Synchronous storage API
Repositories are plain traits with `&self` reads and `&mut self` writes (what a `rusqlite`
transaction needs). Local SQLite answers in well under a millisecond, there is one writer, and
an async runtime would add memory and start-up cost for nothing. Anything slow (export, backup,
purge) runs on a worker thread owned by the app.

## Storage conventions

For the store implementation, so the schema matches the contract:

| Data | Column |
|---|---|
| Ids | `BLOB` of the 16 UUID bytes (`id.as_uuid().as_bytes()`) |
| Timestamps | `INTEGER` nanoseconds since the Unix epoch (`as_nanosecond()`; i64 covers 1677–2262), so values round-trip exactly and `Item == reloaded Item` |
| Enums | `TEXT` key |
| Names | `TEXT`; tag uniqueness on a lower-cased copy (SQLite's `NOCASE` folds ASCII only) |
| Order | `rank INTEGER`, unique per parent |
| Trash | `deleted_at INTEGER NULL`; `NULL` means live |
| Formats | one row per format with its preference index; inline bytes in a `BLOB`, or blob hash and length |
