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
