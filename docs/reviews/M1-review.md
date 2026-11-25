# M1 review: `snipdeck-core` and `snipdeck-store`

Reviewer: Opus (review only, no code changed). Date: 2026-10-17. Commit reviewed: `ed3063d`
(working tree; `crates/snipdeck-portable/` was being written concurrently and is **out of scope**).

Read against `docs/SPEC.md`, `architecture.md`, `store.md`, `crypto.md`, `licensing.md` and
`docs/adr/*`, through the security, data-modelling, migration-safety, error-handling and testing
lenses.

**What was run.**

* `cargo test -p snipdeck-core -p snipdeck-store`: all green (core 110 unit + 15 licence; store
  81 unit + 87 integration; 3 ignored timing/fixture tests).
* `cargo clippy --all-targets -- -D warnings` and `cargo doc` with `-D warnings`: clean.
* The template proptests at `PROPTEST_CASES=20000` and the store model-based proptest at
  `PROPTEST_CASES=1500`: green.
* Throwaway probes in `/tmp/m1-review-probe/tests/{probe,remanence,files}.rs`, a separate crate that
  depends on both crates by path. Every finding marked **verified (probe)** failed or printed the
  stated result there. **Verified (read)** means confirmed by reading the code without running it.
  Nothing below is speculative unless it says so.

## Findings

| ID | Sev | Where | What is wrong | Failing scenario / reproduction | Suggested fix |
|---|---|---|---|---|---|
| H1 | **High** | `store/src/store.rs:264` (`configure`), `store/src/schema.rs:133` (`item_fts`), `store/src/items.rs:307` | **Plaintext survives in the database after an item is made sensitive and encrypted.** SQLite leaves deleted cell contents in freed pages (no `secure_delete`). FTS5 deletes only add tombstones, so the old body's tokens stay in index segments until a merge. The `-wal` holds full page images until a checkpoint. `check_integrity()` still reports the store as healthy, because it only looks at the live `item_fts.body`. This defeats SPEC §5.7 "encrypted at rest" for the most common path: marking an *existing* item sensitive. **Verified (probe).** | Insert a text item `"my pin is ZQ9SecretPlainTextZQ9 ok"`. Then `update_item` it with `sensitive = true`, an empty preview and `Format::Text(encrypt_item(..))`, and close the store cleanly. A byte grep of `snipdeck.db` finds the original string 1 to 3 times and the lowercase FTS token 2 to 3 times (with 0, 50 and 500 filler items). After a simulated crash (`mem::forget`) the `-wal` holds up to 92 copies of the token. `VACUUM` alone leaves the FTS token; FTS `'optimize'` alone leaves freed pages; only `optimize` followed by `VACUUM` removes both (`remanence.rs`). | Set `PRAGMA secure_delete = ON` in `configure`. Turn on the FTS5 `secure-delete` option (`INSERT INTO item_fts(item_fts, rank) VALUES('secure-delete', 1)`) in a migration. After any write that clears plaintext (sensitive on, purge), run `PRAGMA wal_checkpoint(TRUNCATE)`. Add a disk-grep regression test like the probe. Document in `crypto.md` that backups taken earlier (`.pre-vN.bak`, the daily `.snipdeck`) still hold the plaintext. *(I verified `optimize` + `VACUUM`; I did not run the `secure_delete` + FTS `secure-delete` combination, so the new test must confirm it.)* |
| M1 | Medium | `store` trash restore | **A board limit was not enforced on restore.** Details omitted from the public tree. | | Fixed (see the status table). |
| M2 | Medium | `core` licence key selection | **The release pipeline had no hard guard against a development build key.** Details omitted from the public tree. | | Fixed (see the status table); the procedure is in `docs/licensing.md`. |
| M3 | Medium | `store/src/store.rs:136-153`, `store/src/items.rs:358`, `store/src/trash.rs:153` | **`release_blobs` deletes a blob that was just `put` but is not attached yet.** The `gc_blobs` grace period protects pending blobs; `release_blobs` ignores it, so the documented `put` then `insert_item` sequence is broken by any write that happens in between. **Verified (probe).** | `put(big)` gives B, used by item X. `put(big)` again for a new capture Y (same bytes, so it returns B and touches the file). The user edits X's content, or the 30-day purge runs, so B is released and deleted. `insert_item(Y, B)` then fails with `NotFound { entity: "blob" }` (`put_then_unrelated_update_deletes_the_pending_blob`). | Apply the same mtime grace in `release_blobs` (skip files modified within `grace`; the sweep collects them later). Alternatively, keep an in-memory set of hashes `put` in this session and not yet referenced. |
| M4 | Medium | `store/src/items.rs:357-358`, `store/src/trash.rs:152-153`, doc claim at `store/src/store.rs:31` | **Errors after commit.** `update_item` and `purge` commit first and then delete blob files. If a delete fails (on Windows a sharing violation from antivirus or indexer handles is routine), the call returns `Err(Io)` although the change is committed. That contradicts "a failed call changes nothing", invites a retry of a completed operation, and loses `purge`'s count. `gc_blobs` also stops the whole sweep on the first failed delete. **Verified (read).** | Hold an open handle on the old blob file (Windows, or `chattr +i` on Linux) and call `update_item(.., Some(new))`: the result is `Err(Io(..))`, yet `formats()` returns the new content. | After commit, treat delete failures as best effort: collect them and leave them to `gc_blobs`, or return them as a warning in the `Ok` value. In `gc_blobs`, skip and count failures instead of returning early. |
| M5 | Medium | `core/src/format.rs:69` (`FileEntry::new`), `Formats::new` | **"One Windows-valid path component" is not enforced.** It accepts reserved device names (`CON`, `nul.txt`, `COM1`, `LPT9.log`), 300-character names (over the 255 limit), bidi controls (`a\u{202E}gpj.exe` displays as `aexe.jpg`), and duplicate or case-insensitively equal names within one `Files` format. The `.snipdeck` importer being added now feeds this validator from untrusted files. **Verified (probe)** (`files.rs`: all return `Ok`). | Import or paste an item whose files are `CON` and `a.txt`/`A.TXT`. Materialising them on Windows writes to the console device and overwrites one file with the other. | Reject the reserved stems (case-insensitive, with any extension), names over 255 UTF-16 units, and `Cf` bidi controls (U+202A..U+202E, U+2066..U+2069). In `Formats::new`, reject duplicate names compared case-insensitively. Add the cases to `file_names_are_single_windows_valid_components`. |
| M6 | Medium | `store/src/crypto.rs`, `store/src/schema.rs` | **Encryption is a free-standing library, not wired into the store** (PLAN M1: "store ... encryption of sensitive items"). There is no table to persist the `KeyRecord` (its docs say "for example in the settings table", and none exists). No store API encrypts or decrypts formats, and nothing knows an envelope from plaintext. Clearing `sensitive` on an item whose content is an envelope makes `fts::index` index the ciphertext, lossily decoded as UTF-8. **Verified (read).** | `grep -rn KeyRecord crates/snipdeck-store/src` finds hits only in `crypto/`. Flip `sensitive` to false on an encrypted item with `update_item(item, None)`: `reindex` reads the envelope bytes and indexes them. | Before V1 is released, decide the storage contract: a `key_records` or `settings` table, plus either store-level `encrypt_formats`/`decrypt_formats` helpers or a documented rule ("sensitive ⇒ every payload `is_envelope`"), checked in `check_item`/`check_integrity`. Decide what clearing `sensitive` means (decrypt first, otherwise refuse). |
| L1 | Low | `core/src/template.rs:511` (`render`) | **No output bound.** Output size is (number of `{clipboard}` or prompt uses) × (clipboard or answer size), with no cap. **Verified (probe).** | 65 527-byte template of `{clipboard}` × 10 KB clipboard gives a 59.6 MB output. A 1 MB clipboard would be about 6 GB, and the paste aborts the process on out-of-memory. | Cap the rendered length (for example 16 MiB) and return a `RenderError`/truncation flag, or cap the number of dynamic placeholders at parse time. |
| L2 | Low | `core/src/template.rs:608` | **Extra parts are dropped silently.** `{field:Name\|default\|EXTRA\|MORE}` parses, and `EXTRA` and `MORE` vanish. `{choice:x\|a\|a\|a}` keeps duplicate options. This contradicts the module's "never silently kept" stance. **Verified (probe).** | `Template::parse("{field:Name\|default\|EXTRA}").prompts()` gives `default: Some("default")`. | Return an error kind (`TooManyParts`) for a field with more than 2 parts, and `DuplicateOption` for a choice. |
| L3 | Low | `core/src/template.rs:539` (`chars_after`) | **`cursor_from_end` counts scalar values, not graphemes.** It is documented as "how many Left-arrow presses", but editors move by grapheme cluster. **Verified (probe).** | `{cursor}e\u{301}👨\u{200D}👩` gives `Some(5)`; Left-arrow presses needed: 2. | Count extended grapheme clusters (with `unicode-segmentation`), or document the limitation and let the platform layer convert. |
| L4 | Low | `store/src/blobs.rs:80-84,126` | **A damaged blob of the right length is never repaired.** `has()` checks only the length, so re-`put` of the same bytes keeps a bit-rotted file, and every item using it stays unreadable (`BlobCorrupt`). A truncated file *is* repaired (an existing test covers that). **Verified (probe).** | `put(big)`, overwrite byte 0 of the file, `put(big)` again: `read` returns `Err(BlobCorrupt)`. | In `put`, when the file exists, hash it (put already holds the bytes; the cost only falls on the rare re-put), or compare it to `bytes` and rewrite atomically on mismatch. |
| L5 | Low | `store/src/blobs.rs:48-53`, `store/src/store.rs:32` | **`#[derive(Debug)]` on `Blobs`/`Store` prints every in-memory blob's bytes.** That contradicts the "no content in Debug" rule kept elsewhere (`Inline`, `DataKey`). **Verified (probe).** | `format!("{store:?}")` after one `put` of 64 KiB + 1 bytes gives a 262 KB string with the bytes in it. | Write `Debug` for `Blobs` by hand (count and total size only). |
| L6 | Low | `store/src/crypto/keyring.rs:76,181`, `store/src/crypto/provider.rs:56` | **Weak settings are reachable from production code.** `KdfParams::new(8, 1, 1)` is accepted, and `with_kdf_params` is public, so the app can create records with roughly free brute force. `MemoryKeyProvider` (its key dies with the process, so data cannot be recovered after a restart) is public in non-test builds. Stored parameters cannot be *downgraded* by editing a record: they sit inside the provider-authenticated blob and the AAD, which is tested. **Verified (probe/read).** | `KeyRing::new(&p).with_kdf_params(KdfParams::new(8,1,1)?)` creates a valid record. | Enforce a production floor (≥ 19 MiB, t ≥ 2) unless a `test-kdf` feature is on, and put `MemoryKeyProvider` behind a `testing` feature. |
| L7 | Low | `store/src/crypto/keyring.rs:217-220,310-314` | **DEK copies left on the stack.** `let bytes: [u8; 32] = inner[..].try_into()` copies the DEK onto the stack, and `DataKey::from_bytes(bytes)` takes the array by value, so un-zeroized copies remain in stack memory. **Verified (read).** | (Memory forensics; not observable from tests.) | Build in place: `let mut k = Zeroizing::new([0; 32]); k.copy_from_slice(&inner[1..]); DataKey(k)`. |
| L8 | Low | `store/src/crypto/envelope.rs:67` | **The AAD binds the item but not the slot.** Within one item, the HTML envelope can be swapped into the text row, a file's envelope into another file index, or an older envelope replayed. Needs write access to the database. **Verified (probe).** | `encrypt_item(k, id, html)` decrypts fine wherever it is stored for `id`. | AAD = item id ‖ format key ‖ file index (and bump the format byte to `0x02`). |
| L9 | Low | `store/src/boards.rs:36,137`, `tags.rs` read path, `store/src/store.rs:159` (`check_integrity`) | **One invalid stored name fails the whole listing**, and `check_integrity` does not notice. | Insert a board named `a\nb` directly: `boards()` gives `Err(Core(NameHasControlChar))`, and `check_integrity()` gives `Ok([])` (`one_bad_row_fails_the_whole_listing`). | Add `Name` validity to `check_integrity`, and either add matching `CHECK`s in the next migration or degrade (skip and report) instead of failing the list. |
| L10 | Low | `store/src/boards.rs:148`, `store/src/items.rs:245`, contract at `repo.rs:11` | **Missing ids give an empty list.** `lists(missing_board)` and `items(missing_list)` return `Ok([])` although the trait says missing ids fail with `NotFound`. **Verified (probe).** | `store.lists(BoardId::generate())` gives `Ok(0)`. | Return `NotFound` when the parent is not live, or narrow the rule in `repo.rs`. |
| L11 | Low | `store/src/items.rs:355` → `fts.rs:94` | **Metadata edits re-read the whole content.** Every `update_item(item, None)` (pin, rename, tag) reads the whole text blob from disk, BLAKE3-verifies it and rewrites the FTS row: O(content) work for an O(1) edit. **Verified (read).** | Toggling `pinned` on an item with a 50 MB text blob re-reads and re-hashes 50 MB. | Reindex only when the title or `sensitive` changed, and update only the title column otherwise. |
| L12 | Low | `store/src/items.rs:328-347` | **Stale-item edits roll back usage.** `update_item` writes `use_count`/`last_used_at` from the caller's `Item`, so an editor holding an item loaded before a `record_use` rolls the count back (lost update), which affects the most-used sort. **Verified (read).** | Load item (count 3), then `record_use` (count 4), then save an edit with the stale item: count 3. | Leave usage columns out of `update_item`; `record_use` owns them. |
| N1 | Nit | `core/src/licence.rs:209` | `SNIP01.…` reports `UnsupportedVersion(1)` ("format version 1 is not supported"). **Verified (probe).** | | Require no leading zero in the digits. |
| N2 | Nit | `core/src/licence.rs:127` | Only ASCII whitespace is trimmed; a pasted NBSP or BOM gives `Malformed("not a Snipdeck licence key")`. **Verified (probe).** | | Trim `char::is_whitespace` and `\u{FEFF}`. |
| N3 | Nit | `core/src/template/tests.rs:539`, `core/src/search/tests.rs:353` | `rendering_is_deterministic` renders the same context twice and compares, so it can only fail on hidden global state (a tautology for a pure function). The 10k search timing test asserts 100 ms while SPEC §9 is 16 ms. | | Replace the first with a model check (for example, rendering with no placeholders equals the unescaped text, and every prompt name appears in the output when answered). Assert the real budget in the release timing test. |
| N4 | Nit | `core/src/template.rs:557-567` | `{ date }` is rejected as unknown with no suggestion, although names and arguments are trimmed. | | Trim the key, or suggest the trimmed keyword. |
| N5 | Nit | `store/src/tags.rs:13` | Tag uniqueness uses `to_lowercase`, not case folding plus NFC: `Straße`/`STRASSE` and composed/decomposed `École` are distinct tags. | | Fold the case fully and NFC-normalise into `name_key`. |
| N6 | Nit | `store/src/store.rs:243` | `pragma_text` is a `#[doc(hidden)] pub` test hook on the production type that runs any argument-less pragma (`wal_checkpoint`, `optimize`, `shrink_memory`). | | Put it behind `#[cfg(any(test, feature = "testing"))]`. |
| N7 | Nit | `store/src/store.rs:87`, `migrations.rs:115-122` | `schema_version()` turns errors into `0`, and `Error::Migration { reason: String }` drops the source error's type and chain. | | Return `Result<u32>`; keep `#[source] rusqlite::Error`. |
| N8 | Nit | `docs/PLAN.md:18` | Says "migrations tested up/down"; the policy (`store.md`) is forward-only with backup restore. | | Fix the plan wording. |
| N9 | Nit | `core/src/template.rs` | An unchecked render primitive existed beside the entitlement-checked one. | | Fixed. |

**Counts:** Critical 0 · High 1 · Medium 6 · Low 12 · Nit 9.

## Checked and found sound (leave alone)

* **Licence verification.** The token is split, base64 is decoded canonically (padding and
  non-zero trailing bits are rejected; the probe checked every trailing-bit variant) and
  `verify_strict` runs **before** any JSON is parsed. Duplicate JSON keys (`"tier":"free","tier":"pro"`),
  a BOM before the JSON, and `"v":1.0` are all rejected. The signature covers the encoded text,
  so there is no canonicalisation step to attack. Errors never echo the token. Inputs are size
  capped. Free-to-Pro without a valid signature: not possible in `licence.rs`.
* **Key hierarchy.** The Argon2 parameters, salt and mode sit inside the provider-authenticated
  blob *and* in the AAD of the password seal, so editing a record cannot downgrade the KDF
  (every modified byte is rejected). Stored parameters are bounds-checked before hashing. The
  AEAD tag is the only password verifier. `DataKey`, `KeyRecord`, `KeyRing` and the provider
  redact `Debug`, and the crypto errors carry no data.
* **Envelope.** It uses random 192-bit nonces (with a uniqueness and bit-balance test), and the AAD
  binds the item id. Header, nonce, ciphertext and tag are covered by exhaustive bit-flip and
  truncation tests.
* **FTS query sanitiser.** Every alphanumeric code point (U+0000 to U+10FFFF), alone, after a `"`,
  or with a combining mark, plus zero-width, BOM, `NEAR/2`, `title:"` and `^` probes, produced no
  error. Operators are searched as text.
* **SQL injection.** All user values are bound. The only `format!`ed SQL identifiers are
  `'static` table and column names, and `pragma_text` restricts names to `[A-Za-z0-9_]`.
* **Ranks.** The sparse-integer scheme with a parked two-phase renumber keeps the unique index
  valid mid-update. The model-based proptest (`tests/properties.rs`) passed 1 500 cases of mixed
  insert, move, trash, restore and purge.
* **Migrations.** `user_version` plus `application_id` refuse foreign and newer files before any
  write. Each step runs in its own transaction with `foreign_key_check`. `VACUUM INTO` takes a
  backup before the first step. Foreign keys are restored on failure. There is a real v1 fixture
  and a table-rebuild test. A crash mid-step rolls back.
* **Single-writer lock.** It is an OS lock (`File::try_lock`), so a crashed process leaves no
  stale lock. `Store` drops the connection before the lock.
* **Blob files.** They are named by hash (no traversal possible), written as temp file, `fsync`,
  rename, folder `fsync`. Reads are verified by length and BLAKE3, and crash leftovers are swept.
* **Template engine.** The `strftime` subset is hand-written, so `{datetime:FMT}` cannot reach a
  format string. It does not panic at `Timestamp::MIN`/`MAX`, and errors carry byte spans. The
  proptests passed 20 000 cases.
* **Lints and docs.** The schema puts invariants in `CHECK`s and composite FKs (sensitive implies
  an empty preview; files rows cannot outlive their format). `clippy::pedantic` and rustdoc
  `-D warnings` are clean.

## Verdict

**Merge after fixes.** H1 must be fixed (with a disk-level regression test) before anything
claims "encrypted at rest". M1, M2 and M3 are small and should land with this PR. M5 should
land before the `snipdeck-portable` importer merges, since it validates imported names. M6
needs a decision on the schema slot for the key record before V1 is declared frozen; the
encryption wiring itself can follow in M5. The Lows and Nits can be tracked as issues.

## Resolution (core)

Core findings only; the store agent appends its own table. Tests are in `crates/snipdeck-core/`.

| ID | Status | Covering test(s) / change |
|---|---|---|
| M2 | **Fixed** | Release builds must be given a production public key and cannot compile without one; covered by unit tests and `tools/check-licence-key-guard.sh`. Procedure: `docs/licensing.md`. |
| M5 | **Fixed** | `format.rs`: `file_names_are_single_windows_valid_components`, `reserved_windows_device_names_are_rejected_with_or_without_an_extension`, `names_are_limited_to_255_utf16_code_units`, `bidi_and_format_controls_are_rejected`, `files_in_one_format_need_distinct_names_ignoring_case`. New `Error::DuplicateFileName`. |
| L1 | **Fixed** | `template/tests.rs`: `rendering_stops_at_the_output_limit...`, `output_exactly_at_the_limit_is_allowed`, `the_reviewers_amplification_case_is_bounded`, `truncation_never_splits_a_character...`, `prompt_answers_count_towards_the_limit`. 16 MiB cap (`MAX_RENDERED_BYTES`); `try_render` errors, `render` cuts. |
| L2 | **Fixed** | `a_field_takes_at_most_a_name_and_a_default` (`TooManyParts`), `a_choice_may_not_repeat_an_option` (`DuplicateOption`). |
| L3 | **Fixed** | `cursor_counts_grapheme_clusters_like_left_arrow_presses` (`unicode-segmentation`, already in the tree via `nucleo-matcher`; `cargo deny` clean). |
| N1 | **Fixed** | `version_digits_have_no_leading_zeros`. |
| N2 | **Fixed** | `pasted_unicode_whitespace_and_a_byte_order_mark_are_ignored`, `only_surrounding_whitespace_is_ignored`. |
| N3 | **Fixed** | `rendering_is_deterministic` replaced by the model check `rendering_matches_a_model`. The ignored 10k timing test now asserts the 16 ms SPEC budget on the median (release: 0.6 to 3.6 ms medians over several runs, so about 4x headroom). |
| N4 | **Fixed** | `placeholder_keys_are_trimmed_like_names_and_options`, `spans_stay_exact_when_the_key_is_padded`. Keys are trimmed, so `{ date }` renders. |
| N8 | **Fixed** | `docs/PLAN.md` M1 row now says forward-only migrations with a pre-migration backup. |
| N9 | **Fixed** | Callers use the checked render; tests added in `template.rs`. |

## Resolution (store)

Store findings; tests are in `crates/snipdeck-store/`. Schema is now version 2 (forward-only,
pre-migration backup kept; see `docs/store.md`). Remanence guarantees and limits are in
`docs/crypto.md`, "What is left on disk".

| ID | Status | Covering test(s) / change |
|---|---|---|
| H1 | **Fixed** | `PRAGMA secure_delete = ON` (not `FAST`, which leaves freelist pages) on every connection before migrating; FTS5 `secure-delete` plus one `optimize` in migration v2; `wal_checkpoint(TRUNCATE)` after sensitive transitions and purges; plaintext blobs zeroed before deletion; one `VACUUM` after migrating from v1. `tests/remanence.rs`: `no_plaintext_remains_after_a_clean_close`, `..._after_a_crash` (`mem::forget`), `..._after_a_purge` (0/50/500 filler items; scans `.db`, `-wal`, `-shm`, blobs). Each of the three SQLite measures was switched off once and the test failed. `check_integrity` reports either being off (`the_integrity_check_reports_when_secure_delete_is_off`, `store::tests::the_integrity_check_reports_secure_delete_being_off`). What SQLite cannot guarantee (earlier backups, CoW/SSD, temp files) is documented. |
| M1 | **Fixed** | Restore now takes the caller's board limit and reports what it revived; covered by tests in `trash.rs`. |
| M3 | **Fixed** | Per-`put` pending count in memory; released blobs that are pending are kept. `items.rs`: `put_then_unrelated_update_keeps_the_pending_blob`, `put_then_purge_keeps_the_pending_blob`, `an_attached_blob_is_still_deleted_when_released`; `blobs::tests::a_blob_is_pending_from_put_until_attached_once_per_put`. |
| M4 | **Fixed** | Deletes after commit are best effort, left for `gc_blobs`; `gc_blobs` skips and counts (`GcReport::failed`). `items.rs`: `a_failed_blob_delete_after_commit_is_not_an_error`, `a_failed_blob_delete_after_purge_still_reports_the_count`; `store::tests::gc_counts_failed_deletes_and_carries_on`, `released_blobs_that_cannot_be_deleted_are_left_for_gc` (injected failures). |
| M6 | **Fixed** | Migration v2: `items.encrypted` (`CHECK encrypted = 0 OR sensitive = 1`) and a single-row `key_ring` table. `Store::{create_key_ring, unlock_key_ring, change_master_password, lock_key_ring, key_ring_exists, is_unlocked, data_key, put_sensitive}` over the `KeyRing`/`KeyProvider` trait. Sensitive items are written encrypted and read back decrypted, recognised by the column (not the bytes); clearing `sensitive` decrypts first, and while locked it is refused (`Error::EncryptionLocked`); body never indexed (title only). `tests/crypto.rs` (14 tests); `fixture.rs`: `the_v1_fixture_migrates_to_v2_keeping_every_row_and_leaving_a_backup`. |
| L4 | **Fixed** | Re-`put` compares the stored file and rewrites it on mismatch. `blobs::tests::putting_again_repairs_a_file_damaged_without_changing_length`. |
| L5 | **Fixed** | Hand-written `Debug` for `Blobs` and `Store` (counts only). `items.rs`: `debug_output_of_a_store_holds_no_content`; `blobs::tests::debug_shows_counts_not_bytes`, `store::tests::debug_output_names_no_key_or_content`. |
| L6 | **Fixed** | `KdfParams::new` and loading a record enforce 19 MiB / t = 2; `KdfParams::insecure_for_tests` and `MemoryKeyProvider` only with the `testing` feature. `keyring::tests::production_settings_have_a_floor`. |
| L7 | **Fixed** | `DataKey::from_slice` copies into the `Zeroizing` buffer in place. `key::tests::keys_are_built_from_exactly_32_bytes`. |
| L8 | **Fixed** | `SNENC2`: AAD = version, item id, format key or file index (`Slot`); no `SNENC1` reader. `envelope::tests::an_envelope_cannot_move_to_another_slot_of_the_same_item`, `a_version_1_envelope_is_not_an_envelope`. |
| L9 | **Fixed** | Bad names are shown repaired, not failing the list; `check_integrity` reports them. `blobs.rs`: `one_bad_row_does_not_fail_the_whole_listing`. No new `CHECK` (degrade and report instead). |
| L10 | **Fixed** | `lists`/`items` of a missing or trashed parent are `NotFound`. `ordering.rs`: `listing_lists_of_a_missing_or_trashed_board_is_not_found`; `items.rs`: `listing_items_of_a_missing_or_trashed_list_is_not_found`; model check in `properties.rs`. |
| L11 | **Fixed** | Metadata edits update only the FTS title. `items.rs`: `metadata_edits_do_not_read_the_content` (damaged blob, edit still succeeds and stays searchable). |
| L12 | **Fixed** | `update_item` no longer writes usage columns. `items.rs`: `saving_a_stale_item_does_not_roll_back_usage`. |
| N5 | **Fixed** | `name_key` = NFD, upper then lower case, NFC (`unicode-normalization`); v2 rekeys existing tags, suffixing clashes and reporting them. `tags.rs`: `names_clash_after_full_case_folding_and_normalisation`; `tags::tests::*`. |
| N6 | **Fixed** | `pragma_text` behind `cfg(any(test, feature = "testing"))`. |
| N7 | **Fixed** | `schema_version() -> Result<u32>`; `Error::Migration` keeps `#[source] Option<rusqlite::Error>`. `migrations::tests::a_failing_migration_rolls_back_completely`. |
