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

## Task 3: .claude/audit/data.md (DATA-1..35 + gaps). Merged origin/claude/compassionate-pascal-pod8ux.
Status per finding (updated as work lands; "core" = portable crate done and tested, "app" = wired in app/UI):
- DATA-4 export degrades per item (skipped list, manifest `skipped_unreadable`, import warning): core done (tests/degrade.rs). App toast/Settings naming the item: open. i18n for `skipped-by-export`: slint text added, .po fill open.
- DATA-14 daily+weekly+monthly keeping; DATA-15 manual backup own file; DATA-16 damaged today file rewritten; DATA-17 `apply_retention`; DATA-6 shrink guard (`Shrunk`, nothing pruned); read-back check of every new backup (`check_archive_path`): core done (tests/backup_policy.rs). App: folder setting field + readers wired; UI row, retention-on-change, suspect/last-backup line: open.
- Everything else: open (see list in the audit). Order planned: DATA-7, 1, 2, 3, 8-10, 11, 12 (portable); 20/21/22/13 (UI); 5, 6, 18, 19 (start-up recovery); 23-30; 31-35.
