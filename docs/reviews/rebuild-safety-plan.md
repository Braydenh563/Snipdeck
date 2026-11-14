# Rebuild from backup: data-safety plan (v1.0.1)

Fixes the three findings in `.claude/inbox-archive.md:20-24` (review of 3f1f5cb). Design only; nothing here is built yet.

## Current flow

1. `rebuild_from_backup` (`crates/snipdeck-app/src/controller/settings_ctl.rs:445`) checks the copy lock, reads the key record and duplicates the data key twice (`:455-475`), previews the backup (`:477`), and exports a salvage archive `snipdeck.salvage.snipdeck` from the damaged store (`:481`).
2. `swap_in_rebuilt` (`:526`) picks a free stem `snipdeck.damaged-<stamp>[-n]` (`:537-547`), then closes the store by swapping in `Store::open_in_memory()` and dropping the old one (`:558-559`). That drop releases the folder lock (`Store::_lock`, `crates/snipdeck-store/src/store.rs:47`) and closes the connection without an explicit checkpoint: SQLite tries one on last close, and any error from it is ignored.
3. `move_db_files` (`settings_ctl.rs:855`) renames `snipdeck.db`, `-wal` and `-shm` one by one. On an error it undoes what it moved. A crash between renames leaves a half-moved library, and the next `Store::open` (`store.rs:90`) quietly creates an empty `snipdeck.db` next to the moved files.
4. `Store::open(data_dir)` (`:563`) makes the fresh database. Its blob folder is still `data_dir/blobs` (`store.rs:114`), the same folder the damaged copy points at. It restores the key record, imports the backup and then the salvage (`:563-600`).
5. If the import fails, the rollback (`:578-589`) deletes the fresh db files, moves the original back and calls `reopen_original` (`:609`).
6. Blob removal later: `Store::gc_blobs` (`store.rs:150`), run by the daily `sweep_blobs` (`main_ctl.rs:450`), and `release_blobs` (`store.rs:196`) after every delete. Both count references in the open database only. Blobs are content-addressed and shared (`blobs.rs:212`).
7. Startup: `open_library` calls `Store::open(&folder)` (`crates/snipdeck-app/src/main.rs:810`). An `Err` is returned as a string and stops the start. The Doctor offers `Fix::RebuildFromBackup` (`doctor.rs:222-226`, test `:578`).

## Chosen layout: the damaged library becomes one folder

The damaged copy is set aside as a data folder of its own, `snipdeck.damaged-<stamp>/`, holding `snipdeck.db`, `snipdeck.db-wal`, `snipdeck.db-shm` (when present) and `blobs/`. `Store::open` can open that folder directly (support can run `--data-dir` on it). The toast's `kept` value becomes the folder name.

### Finding 1: shared blob folder (crate `snipdeck-store`)
- **Fix:** move `blobs/` into the set-aside folder in the same step as the db. The fresh store starts with an empty `blobs/`. The backup and salvage archives carry their own assets (`snipdeck-portable/src/archive.rs:174`), so the import writes every blob the new library needs. After that, neither `gc_blobs` nor `release_blobs` can reach the damaged copy's files.
- **Why:** a directory rename costs O(1) and reuses the move we already do. Rejected: "skip the sweep while `snipdeck.damaged-*` exists". It misses `release_blobs`, which deletes on every item delete. It also stops sensitive-blob scrubbing and turns off GC with no end date.

### Finding 2: no WAL checkpoint on close (crate `snipdeck-store`)
- **Fix:** add `Store::close(self) -> Closed`. It runs `PRAGMA wal_checkpoint(TRUNCATE)` and reads the `(busy, log, checkpointed)` row, which the existing `checkpoint` (`store.rs:457`) throws away. Then it calls `Connection::close()` and returns `Closed { lock: FolderLock, self_contained: bool }`. When the checkpoint fails (a damaged page, busy), `self_contained` is false and `-wal`/`-shm` still move with the db. SQLite finds the log by name next to the db, so the pair stays consistent.
- `FolderLock` is a new public newtype around the lock `File`. It keeps `snipdeck.lock` held from close through the swap to the fresh open. Today the drop at `:559` frees the lock, and a headless CLI run could open the folder mid-swap.
- Add `Store::open_locked(folder, FolderLock) -> Result<Store>`. `Store::open` becomes `acquire_lock`, then `recover`, then `open_locked`.

### Finding 3: renames not atomic as a set (crate `snipdeck-store`, new `src/aside.rs`)
- **Fix:** a marker file written before the first rename, plus recovery inside `Store::open` under the lock. Every opener gets the recovery (app, headless, tests), and `Store::open` never creates an empty db over a half-moved library.
- New in `aside.rs`:
  - `pub fn set_aside(folder, &FolderLock, stem) -> Result<()>`: writes the marker (`phase=moving`), creates `<stem>/`, renames the four names in a fixed order, then rewrites the marker as `phase=fresh`.
  - `pub fn undo_set_aside(folder, &FolderLock) -> Result<()>`.
  - `pub fn finish_rebuild(folder, &FolderLock)`: deletes the marker and the salvage file.
  - `pub(crate) fn recover(folder, &FolderLock) -> Result<()>`.
- App side (`settings_ctl.rs`): `swap_in_rebuilt` calls `close`, then `set_aside`, `open_locked`, the import, and `finish_rebuild`. The rollback and `reopen_original` call `undo_set_aside`, then `open_locked`. Delete `move_db_files` and the manual `remove_file` loop (`:582-584`) instead of keeping both paths. `reopen_original` takes the `FolderLock`: a second `Store::open` would hit our own lock.
- New `Error::RebuildInterrupted { folder, aside }` in `snipdeck-store/src/error.rs`, mapped to `Trouble::Other`.

## Marker format and startup handling

`<data>/snipdeck.rebuild`, UTF-8 text, written to `snipdeck.rebuild.tmp`, then `sync_all`, then renamed over the marker (whole or absent):
```
snipdeck-rebuild 1
aside=snipdeck.damaged-20261107-1030
phase=moving        (or: fresh)
end
```
A file without `end`, with an unknown version, or with an `aside` that is not a plain `snipdeck.damaged-*` name (no separators, no `..`) counts as unreadable. The moved names are fixed in code (`snipdeck.db`, `-wal`, `-shm`, `blobs`), not read from the file.

`recover` (lock held, before any `Connection::open`):
- **No marker:** nothing to do.
- **`phase=moving`:** every name already in `aside/` is renamed back, unless the same name also exists at the top. That cannot happen in this phase, so the state counts as impossible. Then the empty `aside/`, the salvage file and the marker are removed.
- **`phase=fresh`:** first check that `aside/snipdeck.db` exists. Only then delete the partial fresh `snipdeck.db*` and `blobs/` at the top and move everything back as above. The fresh files hold only data that came from the backup and salvage archives, so deleting them loses nothing.
- **Why undo and never finish:** an interrupted import cannot be resumed at startup. The data key of an encrypted library is only in memory during the rebuild, and the backup choice was the user's. The marker's removal by `finish_rebuild` is the commit point. Undo puts back the damaged library, and the Doctor offers the rebuild again.
- **When neither finish nor undo is possible** (unreadable marker, a name at both places in `moving`, a missing `aside/snipdeck.db` in `fresh`, or a rename-back that fails): `Store::open` returns `RebuildInterrupted`. It deletes nothing, creates no db and leaves the marker for the next start. `main.rs:810` shows the error, which names both folders and says nothing was changed. Fixing it by hand means removing the marker after putting the files where the user wants them.
- **In-process failure:** `set_aside` that fails mid-way calls `undo_set_aside` itself. If that also fails, the result is `Swap::Stuck` (the existing restart toast), and the marker makes the next start retry the undo.

## Tests first

Store unit tests (`aside.rs`, `store.rs`). Each takes a tempdir. `gc_blobs` is called with `Duration::ZERO`, so the real clock inside it cannot change the outcome.
- `closing_checkpoints_so_the_database_file_alone_holds_every_commit`: after writes and `close`, `self_contained` is true and `-wal` is absent or empty. A copy of `snipdeck.db` alone opens with every item.
- `set_aside_moves_the_database_its_log_and_the_blobs_into_one_folder`: nothing named remains at the top, the marker says `fresh`, and `Store::open(aside)` passes `check_integrity` with every blob present.
- `a_fresh_library_sweeping_and_deleting_leaves_the_set_aside_blobs_alone`: the fresh store imports a shared blob, deletes its item (`release_blobs`) and runs `gc_blobs(ZERO)`. The aside store's `check_integrity` is still empty.
- `a_rename_that_fails_midway_puts_back_what_moved`: for n in 0..4, a fake renamer fails at call n. Afterwards the layout is byte-identical to before, there is no marker and no aside folder, and the original opens.
- `a_crash_while_moving_is_undone_at_the_next_open`: for each k in 0..=4, the test lays out by hand `phase=moving` with the first k names in `aside/`. `Store::open` gives back the original items, with no marker and no aside folder.
- `a_crash_after_moving_drops_the_partial_fresh_library_and_restores_the_original`: `phase=fresh` with a half-imported fresh db and blobs at the top.
- `an_impossible_recovery_changes_nothing_and_refuses_to_open`: three cases (an unreadable marker, a name at both places, a missing aside db). Each gives `Err(RebuildInterrupted)`, a directory listing with sizes identical to the one before, and no new `snipdeck.db`.
- `an_aside_name_with_a_path_is_an_unreadable_marker`: `aside=../x` and `aside=a/b` are refused.
- `a_held_folder_lock_keeps_a_second_open_out_during_the_swap`: while `FolderLock` is held, `Store::open` gives `Error::Locked` and the marker is untouched.
- **Seam to add:** none exists for filesystem faults (renames call `fs::rename` directly). Add `pub(crate) trait Rename { fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> }` in `aside.rs`, with `RealRename` used by the public functions and a test-only `FailAt(n)`. Crash states need no seam: each one is a filesystem layout the test writes.

App tests (`crates/snipdeck-app/tests/rebuild.rs`):
- Update `rebuilding_restores_the_backup_recovers_newer_items_and_keeps_the_damaged_file` (`:92`) and its `damaged_files` helper (`:70`) to expect the `snipdeck.damaged-<stamp>/` folder, no marker, and an aside folder that opens. This changes the expected layout. It does not weaken the test.
- `after_a_rebuild_deleting_and_sweeping_keeps_the_damaged_copys_blobs`: an item over `INLINE_LIMIT`, then rebuild, delete it, `sweep_blobs(ZERO)`, and check that the aside store reads that item's bytes.
- Extend `a_failure_before_the_swap_leaves_the_original_in_place` (`:135`) to assert that no marker and no aside folder exist.
- An import failure after the swap needs a fault seam on the app side that we do not have. Its undo is covered by the store tests above. The app wiring is checked by review, as `reopen_original` is today (`settings_ctl.rs:604-608`).

## Risks, and what stays unverified on Windows

- **Directory rename on Windows** (`MoveFileExW`) fails with access denied or a sharing violation while any file under `blobs/` or the db is open in another process: antivirus, the Search indexer, OneDrive or backup agents. Expected result: the rebuild undoes and shows `RestoreFailed`. That is safe, but the rebuild can fail more often than on Linux. Not verified until the owner runs it in `tools/windows-test/` with a file held open under `blobs/`.
- **Windows-only, also unverified:** whether `Connection::close` reliably deletes `-wal`/`-shm` (the mapped `-shm` must be unmapped first), whether the rename-over of `snipdeck.rebuild.tmp` replaces an existing marker, and whether our own held `FolderLock` stops the in-process reopen (LockFileEx is per handle). That last one is why `open_locked` takes the lock it already holds.
- The checkpoint on a damaged db may fail. Then `-wal` moves along (`self_contained=false`), the same as today but now inside one folder.
- `phase=fresh` recovery deletes files at the top of the data folder. It deletes only after `aside/snipdeck.db` is confirmed present, and only the fixed names.
- Disk use is unchanged: renames copy nothing. The fresh `blobs/` duplicates only the assets that the backup and salvage carry.
- A user who deletes the aside folder by hand loses only the damaged copy. The fresh library no longer depends on it.
