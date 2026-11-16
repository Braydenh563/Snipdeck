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
Done + tested (core and app/UI, strings in .pot and all six .po): DATA-1, 2, 3, 4 (core + import warning; no toast naming the item yet), 6 (shrink guard only), 7, 8, 9, 10, 12, 13, 14, 15, 16, 17, 22, 23 (Cause::NotAllowed), 27 (last_seen + 1-day purge hold, fixed-clock tests).
Open: 5 (recovery window for unopenable DB), 6 (zero-length DB), 11 (add test: preview writes nothing), 18, 19, 20 (Restore from file..., board layout choice), 21 (export list/board/selection; CSV/JSON/Markdown/Espanso, neutralise =+-@), 24, 25, 26, 28, 29 (Windows durable rename; gnu check), 30, 31, 32, 33, 34 (backup before bulk acts, version history, selective restore or SPEC note), 35, toast naming skipped items / suspect-shrink notice, the 5 not-reached interactions.

## Task 3 progress (worker)
- Done, tested, gated, pushed (fd26661): DATA-6 (Error::EmptyDatabase), 11 (preview writes no file test), 26, 28 (worst-case draft test; editor cap is 16 KB so 256 KiB suffices), 30, 31 (DeeperFoldersSkipped), 33 (erase text), 35 (neutral fallbacks), toasts naming skipped items (backup, export) and suspect-shrink notice (manual + daily). Strings in .pot and 6 .po.
- Recorded post-1.0 in docs/SPEC.md: 5, 18, 19, 20, 21, 24, 25, 29, 32, 34, unreached interactions.
- Open: nothing. Unverified: Windows-only paths; no run of the real app UI.
