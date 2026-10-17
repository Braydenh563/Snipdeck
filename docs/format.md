# The `.snipdeck` archive format

Version **1.0**. Code: `crates/snipdeck-portable` (`dto.rs` is this document in Rust,
`archive.rs` the zip rules). Product context: SPEC §5.9 and §5.7a.

A `.snipdeck` file is an ordinary **zip** archive. It holds a library (boards, lists, items and
their clipboard formats) in a form that any tool can read with a zip library and a JSON parser,
and that does not depend on Snipdeck's database layout.

```text
backup.snipdeck
├── manifest.json            what this is, which version, how much of it
├── items.json               boards > lists > items, in the user's order
└── assets/
    ├── 3b1f…(64 hex)        one file per distinct payload, named by its BLAKE3 hash
    └── …
```

Entry order inside the zip carries no meaning. Writers produce deflate or stored entries and no
folders; readers must accept both and may ignore folder entries.

## `manifest.json`

```json
{
  "format": "snipdeck-archive",
  "format_version": "1.0",
  "app_version": "0.1.0",
  "created_at": "2027-01-15T08:01:40Z",
  "counts": { "boards": 2, "lists": 5, "items": 6, "tags": 2, "assets": 9 },
  "encryption": "SNENC2",
  "sensitive_omitted": 0
}
```

| Field | Type | Meaning |
|---|---|---|
| `format` | string, required | Always `"snipdeck-archive"`. Anything else is not ours. |
| `format_version` | string `"MAJOR.MINOR"`, required | See *Versioning*. |
| `app_version` | string, required | The Snipdeck that wrote the file. Informational. |
| `created_at` | string, required | RFC 3339 instant. Informational. |
| `counts` | object, required | How many boards, lists, items, tags and assets (files under `assets/`) the archive holds. A reader **must** check them against the data; a mismatch means the file is damaged or was edited by hand. |
| `encryption` | string, optional | Present (`"SNENC2"`) when at least one payload is encrypted. A payload may be flagged encrypted only if this says so. |
| `sensitive_omitted` | integer, optional | How many sensitive items the exporter left out (the default). Absent means 0. |

## `items.json`

```json
{
  "tags": [ { "id": "018f…", "name": "mail" } ],
  "boards": [
    {
      "id": "018f…", "name": "Work",
      "created_at": "2027-01-15T08:00:01Z", "updated_at": "2027-01-15T08:00:01Z",
      "lists": [
        {
          "id": "018f…", "name": "Replies",
          "accent": "blue", "view": "compact", "sort": "most-used",
          "created_at": "…", "updated_at": "…",
          "items": [
            {
              "id": "018f…", "title": "Signature", "kind": "template",
              "pinned": true, "sensitive": false, "always_plain": false,
              "use_count": 5, "last_used_at": "2027-01-15T08:08:20Z",
              "source_app": "Outlook",
              "created_at": "…", "updated_at": "…",
              "tags": ["018f…"],
              "preview": "Hello {date}",
              "formats": [
                { "kind": "html", "asset": "3b1f…", "size": 20 },
                { "kind": "text", "asset": "9c0e…", "size": 12 }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

### Order

Boards, lists and items are **arrays in the user's manual order**; there is no position field and
no internal rank. The first format of an item is its primary one and the order of `formats` is the
preference order used when writing to the clipboard. Tags are listed by name and carry no order.
Only live records are exported: the trash is not part of an archive.

### Ids

`id` of a board, list, item and tag is a UUID in its canonical text form (any version; Snipdeck
writes UUIDv7). Ids must be unique among records of the same kind, compared as UUIDs, not as
text. Ids exist so the file can refer to itself (`tags` in an item) and so a reader can detect
duplicates; **an importer assigns new ids** to everything it creates.

### Fields

| Record | Field | Type | Notes |
|---|---|---|---|
| tag | `name` | string | 1 to 100 characters after trimming, no control characters. Tags whose names are equal ignoring case are the same tag. |
| board, list | `name` | string | Same rule. |
| board, list, item | `created_at`, `updated_at` | RFC 3339 string | Between the years 1677 and 2262 (storage limit). Fractional seconds allowed, offsets other than `Z` allowed. |
| list | `accent` | string or absent | `blue teal green amber orange red rose violet slate`. |
| list | `view` | string | `compact` or `comfortable`. |
| list | `sort` | string | `manual`, `most-used` or `recently-used`. |
| item | `title` | string | May be empty. |
| item | `kind` | string | `text template code link image file`. Describes content; sensitivity is separate. |
| item | `pinned`, `sensitive`, `always_plain` | boolean, default `false` | |
| item | `use_count` | integer 0 to 4 294 967 295, default 0 | |
| item | `last_used_at` | RFC 3339 string or absent | |
| item | `source_app` | string or absent | The app the content was captured from. |
| item | `tags` | array of tag ids, default `[]` | Every id must appear in the top-level `tags`. |
| item | `preview` | string or absent | An excerpt (at most 200 characters) for people reading the JSON. A reader may use it or recompute it from the text format. Absent for sensitive items: a secret has no plain excerpt. |
| item | `formats` | array, at least one | See below. At most one format of each kind. |

The enum keys above are stable: a released key is never renamed or reused.

### Formats

A format is one clipboard representation of an item. `kind` is one of:

| `kind` | Payload |
|---|---|
| `text` | UTF-8 text. At most 5 MiB (SPEC §5.4). |
| `html` | UTF-8 HTML fragment (no `CF_HTML` header). At most 5 MiB. |
| `rtf` | RTF bytes. At most 5 MiB. |
| `image/png` | A PNG file. |
| `files` | Copies of files; see below. |

Every kind but `files` has these fields:

| Field | Meaning |
|---|---|
| `asset` | The payload's file name under `assets/`: 64 lowercase hexadecimal digits. |
| `size` | The size in bytes of that file (the encrypted size when `encrypted`). Must equal the zip entry's size. |
| `encrypted` | Boolean, default `false`. See *Sensitive items*. |

`files` has `"files": [ { "name": "notes.txt", "asset": "…", "size": 10 } ]`, at least one entry
(and at most 10 000), where `name` is a single valid Windows path component (no `/ \ : * ? " < > |`,
no control characters, not `.` or `..`, not ending in a dot or space), so extracting it can never
write outside the target folder. Each file object may carry `encrypted` too.

### Assets

`assets/<hash>` holds the raw bytes of a payload. `<hash>` is the **BLAKE3** hash (32 bytes, 64
lowercase hex digits) **of the file's bytes as stored in the archive**. A reader must verify it.
Identical payloads share one asset. Small payloads are assets too; there is only one place to
look. An asset no item refers to is ignored.

## Sensitive items

A sensitive item (`"sensitive": true`) is a password, card number, token and the like. Archives
are files people e-mail and upload, so Snipdeck never writes their content in plain text:

* **Default: omitted.** The item is not in the archive at all, not even its title, and
  `sensitive_omitted` counts it.
* **Option: encrypted.** The item is exported with **every payload encrypted** (`"encrypted":
  true`) and no `preview`. Titles, tags and other metadata stay readable, as in the application's
  own database (`docs/crypto.md`: titles and tags are not covered).

An encrypted payload is a `SNENC2` envelope of the library's data key, as defined in
`docs/crypto.md`: `"SNENC2" | nonce (24) | XChaCha20-Poly1305 ciphertext | tag (16)`. Its
associated data binds it to the item and the payload's place in it: the archive item's `id` and
the slot (`Format(kind)` for a single-payload format, `File(index)` for the n-th file). Moving an
envelope to another item, format or file therefore fails to authenticate.

The data key is the library's own, bound to the operating-system user (Windows DPAPI) and
optionally a master password. **An archive with encrypted items can be opened only where that key
is available**, normally the same user on the same machine: it is a backup and migration format
for that library, not a way to hand secrets to another person. An importer without the key (or
with a different one) skips those items and says so; it never imports ciphertext as content.

## Versioning and compatibility

`format_version` is `MAJOR.MINOR`.

* **Readers refuse a newer major.** If `MAJOR` is greater than the reader's, nothing else in the
  file is interpreted (a new major may reshape `items.json`), and the user is told to update.
  Major 0 does not exist and is refused.
* **Readers tolerate unknown fields within a major.** Any object may contain fields the reader
  does not know; they are ignored. Within a major, a field that was required stays required with
  the same meaning, and enum keys are never renamed. A reader of 1.0 reads 1.7 and mentions that
  the file is newer; if a later minor adds a new enum value the reader does not know (a new
  format kind, say), the file is refused as invalid rather than guessed at.
* **Unknown entries** in the zip (other than names the next section rejects) are ignored.
* **Writers** write the newest minor they know, and never remove or retype a 1.0 field in 1.x.
* A change that breaks these rules is a new **major**; the old major is then still readable by
  the new application (import is the migration path, so there is no in-place upgrade).

## Reading untrusted archives

An archive can come from anywhere. A conforming reader checks all of the following **before
changing anything**, and a failure leaves the library exactly as it was:

1. **Zip structure.** No more than a fixed number of entries; entry names are relative,
   `/`-separated, ASCII letters, digits and `._-/` only; no `..`, empty or `.` components, no
   leading `/`, no backslash, no drive letters; no name repeated (the end-of-directory entry
   count must match the names found); no symbolic links, devices or encrypted entries; only
   *stored* and *deflate* compression; no entries whose data overlaps another's. Nothing is
   ever extracted to disk.
2. **Sizes.** Each entry's declared uncompressed size, the total of all of them and the
   compression ratio of large entries are checked against limits *before* decompression, and the
   bytes actually produced are checked again while decompressing, because headers can lie.
   The reference reader's defaults: 200 000 entries, 64 MiB per entry, 1 GiB in total, ratio
   500:1 for entries over 5 MiB, 64 MiB for `items.json`, 1 MiB for `manifest.json`, 100 000
   items.
3. **Manifest.** `format` and `format_version` first; then the counts against the data.
4. **Document.** Every id parses and is unique; names, enum keys and timestamps are valid; every
   tag reference resolves; every item has at least one format, with unique kinds; file names are
   valid; every `asset` is well formed, exists, and has the declared `size`.
5. **Assets.** Each referenced asset's bytes hash to its name. Text and HTML payloads are valid
   UTF-8 and within 5 MiB. Encrypted payloads decrypt (otherwise their item is skipped, not
   imported as ciphertext).

The reference importer then writes everything in one logical step: if writing fails part-way,
what it had added is removed again.

## Importing

* **New ids, new board (default).** Everything is imported as new records under one new board
  named after the file; the archive's lists become that board's lists (named `Board / List` when
  the archive has several boards). The archive's boards can instead be recreated under their own
  names (the layout used to restore a backup). Importing never modifies or deletes existing
  records, and importing the same file twice creates two copies.
* **Skip duplicates (option).** An item is skipped if its *content* (every format, in order,
  byte for byte, but not its title or flags) equals an item already in the library or one
  imported earlier in the same file. The content hash is
  `BLAKE3-derive-key("snipdeck content hash v1")` over, for each format, the kind key, a zero
  byte, the BLAKE3 hash and the length (8 bytes, little endian) of the decrypted payload; `files`
  formats hash a file count and then each file's name, zero byte, hash and length. A board
  that would have no new item is not created.
* **Tags** are matched by name ignoring case; existing tags are reused, missing ones are created.
* **Free tier.** The importer does not enforce plan limits. It reports a warning when the
  library would exceed the number of boards the caller says its plan allows.
* **Report.** Boards, lists, items and tags added, items skipped, and warnings (sensitive items
  that need a key, unknown entries, unreferenced assets, a newer minor version, the board limit).

## Other importers

All return the same report and share the duplicate and board-limit behaviour above.

| Source | Reads | Skips |
|---|---|---|
| Folder | `.txt` and `.md` files (any case) directly in the folder, and in its immediate subfolders (one list each). Title = file name. UTF-8, UTF-16 with BOM, otherwise lossy with a warning. | Over 5 MiB, empty, binary (NUL bytes), names starting with `.`, symbolic links, anything not a regular file, deeper folders. |
| Ditto | `Data.db` opened read-only. Text from `Data` (`CF_UNICODETEXT`, then `CF_TEXT`) or `Main.mText`; groups become lists; starred clips are pinned; `lDate` and `lastPasteDate` become the creation and last-use times. Newest first. | Clips with no text (images, files), other formats of text clips (HTML, RTF), shortcuts, sticky ordering. Details in `importers/ditto.rs`. |
| CopyQ | JSON written by `copyq eval` (see `importers/copyq.rs` for the one-liner, which is written from CopyQ's scripting docs and not yet run against a live CopyQ): strings or objects with `text`, `title`, `tags`, `pinned`; one list named after the tab. | CopyQ's binary `.cpq` export (an undocumented, versioned `QDataStream`) is deliberately not parsed. Only text. |
| Windows clipboard history | Not implemented (SPEC §5.9 lists it; it needs the platform crate). | |

## Backups

`<data folder>/backups/snipdeck-backup-YYYYMMDD.snipdeck` (local date), at most one per local
day, newest 7 kept, written to `<name>.part` and renamed when complete. Only files whose names
match that pattern exactly (a real date) are ever pruned. Sensitive items are encrypted with the
library key or left out. Restoring is an import (`backup::restore`): boards come back under
their own names and items already in the library are skipped.
