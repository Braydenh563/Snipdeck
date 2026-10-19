# Storage (`snipdeck-store`)

How the data folder is laid out, what the schema holds, and how to change it. The column
conventions are fixed in [`architecture.md`](architecture.md#storage-conventions); the decision
to use SQLite plus a blob folder is [ADR-0002](adr/0002-local-first-sqlite.md).

## Data folder

| Path | What |
|---|---|
| `snipdeck.db` (+ `-wal`, `-shm`) | The SQLite database: WAL mode, `foreign_keys = ON`, `synchronous = NORMAL`, `secure_delete = ON` (see [crypto.md](crypto.md#what-is-left-on-disk)). |
| `snipdeck.lock` | Held exclusively (`File::try_lock`) for as long as a `Store` is open. A second `Store::open` fails with `Error::Locked`. The OS releases the lock if the process dies. |
| `blobs/ab/cdef...` | Payloads over `INLINE_LIMIT` (64 KiB), named by the BLAKE3 hex of their bytes: two digits of folder, 62 of file name. |
| `snipdeck.db.pre-vN.bak` | Written before a migration away from version N (`VACUUM INTO`). Safe to delete once the new version has run well. It is a full copy: plaintext of items made sensitive later stays in it until it is deleted. |

`Store::open_in_memory()` keeps database and blobs in memory, for tests and previews.

## Schema (version 4: v3 added the Recent inbox, v4 the item colour label and list icon)

Ids are 16-byte `BLOB`s, timestamps `INTEGER` nanoseconds, enums `TEXT` keys, booleans 0/1. Every
table is `STRICT`. `rank` is a sparse integer, unique within the parent.

| Table | Columns (besides the obvious) | Constraints and keys |
|---|---|---|
| `boards` | `rank`, `deleted_at` | `rank` unique |
| `lists` | `board_id`, `accent`, `list_view`, `list_sort`, `rank`, `deleted_at` | FK `board_id` cascade; unique `(board_id, rank)` (also the FK index); `CHECK` on the three enum keys |
| `items` | `seq`, `list_id`, `title`, `kind`, `preview`, `pinned`, `sensitive`, `always_plain`, `use_count`, `last_used_at`, `source_app`, `rank`, `deleted_at`, `encrypted` (v2) | `seq` is an internal rowid (the FTS row id; stable across `VACUUM`); `id` unique; FK `list_id` cascade; unique `(list_id, rank)`; `CHECK` on `kind`, the booleans, `use_count` in `u32` range, `sensitive = 0 OR preview = ''`, and `encrypted = 0 OR sensitive = 1` |
| `item_formats` | `idx` (preference order), `kind`, `inline_bytes`, `blob_hash`, `len` | PK `(item_id, idx)`; unique `(item_id, kind)`; FK `item_id` cascade; `CHECK` that a payload is inline xor blob, and `files` has none; partial index on `blob_hash` |
| `item_files` | `idx`, `name`, `inline_bytes`, `blob_hash`, `len` | PK `(item_id, idx)`; composite FK `(item_id, 'files')` to `item_formats`, cascade, so file rows cannot outlive or precede their format |
| `tags` | `name`, `name_key` | `name_key` unique: the name decomposed, case-folded (upper then lower case, so `ß` = `SS`) and recomposed to NFC (`tags::name_key`; v2) |
| `item_tags` | | PK `(item_id, tag_id)`; both FKs cascade; index on `tag_id` |
| `item_fts` | FTS5 `title`, `body` | `unicode61 remove_diacritics 2`; rowid = `items.seq`; trigger `items_fts_delete`; FTS5 `secure-delete` on (v2) |
| `key_ring` (v2) | `id` (always 1), `record` | At most one row: the `SNKEY1` key record ([crypto.md](crypto.md)); `CHECK` that it starts with `SNKEY1` |

Every foreign key column is the leading column of an index (a test checks this).

`items.encrypted = 1` means every payload of the item (each `item_formats` and `item_files`
row, inline or blob) is an `SNENC2` envelope bound to the item, the format and the file
index. Storage records this instead of guessing from the bytes, so plaintext that happens to
start like an envelope is never taken for one. `check_integrity` verifies that encrypted
items hold envelopes only.

### Reads that meet bad data

A board, list or tag name that is not a valid `Name` (written by another tool) is shown with
its control characters replaced and its length capped instead of failing the whole listing,
and `check_integrity` reports it; saving the record stores the repaired name. Listing the
lists of a board, or the items of a list, that is missing or in the trash is `NotFound`.

### Order

`First`/`Last` take the current minimum minus, or maximum plus, `1 << 16`; `After(x)` takes the
midpoint to the next sibling. When no integer is left, the parent's children are renumbered
`1 << 16` apart with two `UPDATE`s in the same transaction (rows are parked at `-2^62 - rank` first so
the unique index is never violated) and the placement is retried. Trashed rows keep their rank
and count as neighbours, so a restore returns an item to exactly where it was. An `After`
anchor must be a live sibling in the target container; anchoring a row after itself is a no-op.

### Trash

`deleted_at` is set only on the record that was trashed; what is inside is hidden because its
parent is (reads join up to the board).

- `restore` clears the flag on the record and on its trashed ancestors. Records inside that
  were trashed earlier on their own stay in the trash.
- `Store::restore_with(target, max_live_boards)` does the same, returns the records it brought
  back, and refuses (`Error::Core(ProRequired(UnlimitedBoards))`, nothing changed) when a board
  would come back while `max_live_boards` boards are already live. Restoring a list or item
  inside a trashed board revives the board, so on Free the app passes the board limit here;
  otherwise trash, create, restore would exceed it (review M1). Storage still knows no
  entitlements: the limit is the caller's number.
- `trashed()` lists the records whose parent is not trashed, newest first.
- `purge(cutoff)` deletes records with `deleted_at < cutoff`. A purged list or board takes
  everything inside it (FK cascade), whatever the inner records' own timestamps. The return
  value counts entries as `trashed()` shows them. Blobs that become unreferenced are zeroed
  and deleted after the commit, and the WAL is checkpointed, so purged content does not linger
  (best effort, see Blobs).
- Updates, moves, inserts into, and `record_use` on a trashed or hidden record fail with `NotFound`.

### Search

`ItemRepo::search` runs FTS5 over the item title and the `text` format (blob-stored text is
read and indexed too). The index row is written by the item repository in the same transaction
as the item (the text may live in a blob file, which a trigger cannot read); only the delete
is a trigger, so purges and cascades never leave ghost rows. Trashed items stay in the index and
are filtered at query time, which makes restore free. Sensitive items are indexed by
**title only** (the `ItemRepo::search` contract), never their content, encrypted or not; the
title is not encrypted either ([crypto.md](crypto.md#limits-and-non-goals)). An edit that changes
only metadata updates the title column and never reads the content; making an item sensitive
removes its body from the index (FTS5 `secure-delete` removes the tokens, not just a tombstone). User input is turned
into `"word"*` phrases (see `fts::match_expression`), so it cannot cause an FTS syntax error;
all words must match and the last-typed prefix works. Ranking is `bm25` with the title weighted
10 to 1.

Known limit: `unicode61` treats a run of CJK characters as one word, so Japanese and Chinese
match from the start of a run only. Revisit with a `trigram` index or a segmenter.

### Blobs

`Store::put` is the only place that applies `INLINE_LIMIT` and hashes. Writes go to a temp file
in the target folder, are flushed, and renamed over the final name; identical bytes share a file
and a `put` of bytes already stored refreshes the file's age and rewrites the file if its
bytes differ (a damaged blob of the right length is repaired). `read` checks length and hash.
Rows may only reference blobs that exist (checked on write).

Blobs are released when an item's content is replaced and when the trash is purged:

- A blob that `put` handed out and no committed row has used since is **pending** and is never
  released, so `put` then `insert_item` survives an unrelated edit or purge of the same bytes
  in between (review M3). The count is per `put`, in memory: a crash forgets it, and a blob
  never attached is collected by `gc_blobs`.
- Deleting happens **after the commit and is best effort** (review M4): a file that cannot be
  deleted (an antivirus handle on Windows) is left for `gc_blobs`, never turned into an error
  for a change that is already committed. `gc_blobs` likewise skips and counts
  (`GcReport::failed`) files it cannot delete instead of stopping.
- Blobs that held plaintext of an item that is now sensitive, or of purged items, are
  overwritten with zeros before they are deleted (best effort; see crypto.md).

`Store::put_sensitive` holds a sensitive capture in memory only (never in the blob folder) until
it is written encrypted. `Store::gc_blobs(grace)` sweeps what a crash leaves (unreferenced
files and `.tmp-*` leftovers older than `grace`). `Store::check_integrity()` reports
violations the schema cannot express, including secure delete being off, encrypted items with
a payload that is not an envelope, invalid names and clashing tag keys.

## Migrations

The schema version is SQLite's `user_version`: it changes in the same transaction as the
migration, needs no table of its own, and every SQLite tool can read it. `application_id`
(`0x534E444B`) marks the file as Snipdeck's, so a stray `.db` is refused instead of adopted.

Policy:

- Migrations are an embedded, ordered, **forward-only** list in `migrations.rs`
  (`MIGRATIONS`); a released entry is never edited, reordered or removed. There are no down
  migrations: restore the `.pre-vN.bak` instead.
- Each runs in its own transaction with foreign keys off (needed to rebuild tables), followed
  by `PRAGMA foreign_key_check`; any failure rolls that step back and the database stays at
  the previous version. A step may have a Rust `post` function for data changes SQL cannot
  express (v2 recomputes tag keys), run in the same transaction. `Error::Migration` keeps the
  SQLite error as its `source`.
- A database from a **newer** version is refused (`Error::SchemaTooNew`) before anything is
  written, including the journal mode.
- Stored enum keys never change. Because the keys are `CHECK` constraints, adding a variant to
  a core enum needs a table-rebuild migration; `schema` tests insert every core `ALL` key, so
  forgetting fails a test.
- Prefer expand/contract: add the new column or table in one release and drop the old one in a
  later one, so a downgrade-by-restore never meets a schema it cannot read.

### Version 2 (encryption at rest, secure delete, folded tag keys)

Forward-only and additive (expand only): `ALTER TABLE items ADD COLUMN encrypted ... DEFAULT 0`
(every existing item is plain, which is what it is), `CREATE TABLE key_ring`, FTS5
`secure-delete` on plus one `optimize` (merges the index so entries deleted under v1 leave its
segments), and `tags::rekey` recomputing every `name_key`. Two tags that were distinct under
lower-casing but clash under folding (`Straße`, `STRASSE`) are both kept: the later one by id
gets its key suffixed with its id and `check_integrity` asks for a rename. The usual
`.pre-v1.bak` is written first. After the migration commits, `Store::open` runs `VACUUM` and a
`TRUNCATE` checkpoint once (best effort), because v1 ran without `secure_delete` and its freed
pages may still hold deleted content. A v2 database cannot be opened by a v1 build
(`SchemaTooNew`); downgrade by restoring the backup. External SQLite tools older than 3.42
cannot read the FTS index once `secure-delete` is on. Tested on the real v1 fixture
(`tests/fixture.rs`, `the_v1_fixture_migrates_to_v2_...`).

To add a migration:

1. Write `pub(crate) const V5: &str = "..."` in `schema.rs` (SQL only; no `BEGIN`, no `PRAGMA user_version`).
2. Append `Migration { version: 5, name: "...", sql: schema::V5, post: None }` to `MIGRATIONS`.
3. Add a test that migrates `tests/fixtures/v1.db` (a real v1 file) through every step and
   checks rows survive; extend `tests/fixture.rs` if it should assert new data.
4. Update the table above and keep the frozen `V1` and `V2` text unchanged.
