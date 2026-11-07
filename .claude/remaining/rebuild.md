# rebuild agent

## Done (snipdeck-store, tested here: 115 lib + integration tests pass, clippy and rustdoc clean)
- `Store::close(self) -> Closed {lock: Option<FolderLock>, self_contained}`, checked `wal_checkpoint(TRUNCATE)`.
- `FolderLock` (public; `acquire`, `folder`), `Store::open_locked`, `Store::folder_lock()`.
- `src/aside.rs`: marker write/parse, `set_aside`, `undo_set_aside`, `finish_rebuild`, `recover` (called by `Store::open`), `Rename` seam + test-only `FailAt`.
- `Error::RebuildInterrupted { folder, aside }` (Trouble::Other).
- Nine plan tests in `aside.rs` tests module.

## Done but NOT compiled (snipdeck-app; coordinator must build + test)
- `crates/snipdeck-app/src/controller/settings_ctl.rs`: `swap_in_rebuilt` rewritten (close -> `set_aside` -> `open_locked` -> import -> `finish_rebuild`; `kept` is now the folder name `snipdeck.damaged-<stamp>[-n]`, free-name check is `!data_dir.join(stem).exists()`); `reopen_original(data_dir, lock, key)` now calls `undo_set_aside` then `open_locked`; `move_db_files` and the `remove_file` loop deleted; doc comment updated. Check: unused imports, the `rebuilt` closure borrowing `self`, `Swap::Stuck` path when `open_locked` fails (lock is dropped, marker stays for next start).

## Next (app side, not written)
- `crates/snipdeck-app/tests/rebuild.rs`: update `rebuilding_restores_the_backup_recovers_newer_items_and_keeps_the_damaged_file` (:92) and helper `damaged_files` (:70) to expect `snipdeck.damaged-<stamp>/` folder (opens with `Store::open`), no `snipdeck.rebuild` marker.
- Add `after_a_rebuild_deleting_and_sweeping_keeps_the_damaged_copys_blobs` (item over INLINE_LIMIT, rebuild, delete, `sweep_blobs(ZERO)`, aside store still reads the bytes).
- Extend `a_failure_before_the_swap_leaves_the_original_in_place` (:135): assert no marker and no aside folder.
- `ToastMessage::DatabaseRebuilt { kept }` wording (`wording.rs`, `tests/toast_texts.rs`, `toast.rs`): `kept` was `<stem>.db`, now `<stem>` (folder); adjust text if it says "file".
- `crates/snipdeck-app/src/doctor.rs:222-226` text, if it mentions the `.db` copy.
- `main.rs:810` already surfaces `Store::open` errors; `RebuildInterrupted` message names both folders. Nothing to change.
- Windows-only behaviour unverified (see plan Risks).

## Blocked
- nothing
