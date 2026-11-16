# fable2-last

## Done
- Merged origin/claude/compassionate-pascal-pod8ux (7b8c233).
- Part A (pushed as 0053a73, gate green): refusals while an archive runs (master password, restore, import), key-record recheck in finish_rebuild, blob release hold during jobs, busy toast/Cause::Busy, redraw on start/end, paste-file deletion join on quit, panic payload logged, `.part` removal on quit, SPEC notes, tests.

## Next
- Part B (G28 remainder): two-phase portable API (decode on worker -> Decoded plan; apply on UI thread in one transaction; previews from plan + snapshot).

## Blocked
- Nothing.

## Part B (G28 remainder): two-phase portable API
- Done: portable `Decoded`/`Known` (decode without a library, read-only preview, one-transaction apply); app `Work::Decode` on the archive worker, `Context::Import`, `begin_import_preview`/`begin_restore_preview`/`begin_apply_import`/`begin_restore_backup`, async Settings and Help bindings; tests in portable `two_phase.rs` and app `archive_jobs.rs`.
- Rebuild: now a worker job (`Work::Rebuild` reads the backup, cuts the salvage and reads it back; a stale copy is cut again via `ArchiveEnd::Again`, max 3 passes). Only the one-transaction apply into the fresh store stays on the UI thread. Busy state measured in the harness for Settings (`backup_busy`) and Help (`archive_busy`). Nothing open.

## Task 3: .claude/audit/data.md (DATA-1..35 + gaps)
- Done, tested, gated: DATA-1, 2, 3, 4 (core, import warning and the backup/export toasts naming the first skipped item), 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 22, 23, 26, 27, 28, 30, 31, 33, 35, the suspect-shrink notice (manual and daily).
- DATA-5 (review fix): `Store::open_or_set_aside` + `paths::open_library_folder`; an unopenable library is kept in `snipdeck.damaged-<stamp>/`, a fresh one opens, Danger notice `LibrarySetAside`. A recovery window before the app opens stays post-1.0.
- Recorded post-1.0 in docs/SPEC.md: 18, 19, 20, 21, 24, 25, 29, 32, 34, the unreached interactions, plus the known limits (pending blobs freed at restart; import apply and rebuild swap on the UI thread).
- Open: nothing. Unverified: Windows-only paths (moving a held-open db, DATA-29 not implemented); no run of the real app UI.
