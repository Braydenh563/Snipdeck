# G28 remaining (sonnet)

Done: daily backup, Back up now, Export library and the rebuild's salvage export run on a
worker (archive_job.rs, controller/archive_ctl.rs). Edits during a rebuild's salvage are
included: finish_rebuild re-cuts the salvage inline when `change_counter` moved.

Next (not started): restore apply, import apply, their preview dry-runs, the rebuild's backup
preview. Blocker, needs a decision: `import_*` and the dry run drive a `Library` that parses and
writes in one pass (plan.rs `apply`; dry run is a rolled-back write transaction), so they need a
writable store, and the live Store is owned by the UI thread (folder lock, change counter,
caches). Options:
1. Two-phase portable API: worker parses/decodes the source into an owned plan (no Library),
   UI thread applies it in one transaction (fast). Duplicate checks run at apply. Previews could
   count from the plan plus a read-only snapshot for duplicates. Cleanest; changes snipdeck-portable.
2. Worker runs the import into a scratch Store in a temp folder, the UI thread merges the scratch
   library into the live one. Doubles the write work; changes dedupe semantics.
3. Second writer connection on a worker (WAL allows it). Breaks the folder-lock/change-counter
   invariants; not recommended.
Busy state, refusal while running, and lock/quit handling reuse ArchiveState/archive_busy.

## Review of 854ea80 (coordinator, MERGE-WITH-FIXES), all still open
Medium
1. Rebuild stale state during salvage: only erase checks archive_busy(). While busy, refuse library
   writes, master-password steps, restore, import (toast), or a modal busy state; re-read the key
   record in finish_rebuild and fail safely if it changed. Tests: password change and item add
   during a rebuild. (Item adds are now re-cut into the salvage via change_counter, ee28f84.)
2. gc_blobs during export/backup breaks the worker snapshot (NotFound; salvage silently dropped,
   archive_ctl.rs ~515). Defer blob GC while archive_busy(), run it at job end. Test: replace an
   image item's content between begin and run.
3. Busy state: push archive_busy to every window on start/end (Help Back up now, rebuild, daily
   backup); toast on Begun::Refused; own Cause/log text for the busy case (not Locked). New
   strings: .pot plus all six .po translated.
Low
4. Join the forget-paste-files JoinHandle on quit (windows/clipboard.rs, lock_ctl.rs, use_ctl.rs).
5. Log the panic payload in archive_job.rs `guarded`.
6. history_ctl readable() checks length only: record the trade-off in SPEC.md.
7. Quit mid-export leaves <name>.part: remove our own stale .part on next start/quit, or document.
Tests: make an_export_cut_on_a_worker... change the library between begin and run; add tests
for the Threads delivery seam, the panic path, spawn_backup updating Maintenance, the busy
property. Plus restore/import apply + previews off-thread (options above).
Then merge origin/claude/compassionate-pascal-pod8ux if it moved, gate --changed, push.
