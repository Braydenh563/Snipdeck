# v11-light
- Done: slice 9 (c66a7b7); slice 3a groups (store v8, GroupRepo, export/import minor 1.2, docs/format.md).
- Next: nothing in light lane; 3b (UI) may use GroupRepo; 5a not requested.
- Blocked: gate.sh clippy builds whole workspace (snipdeck-ui); ran scoped -p fmt/clippy/doc/test instead.

## Slice 5a (light2 builder)
- Done: core Trigger + Matcher (trigger.rs), store v9 `triggers` table, TriggerRepo, fixtures/v8.db (from v7.db via MIGRATIONS[..8]).
- Next: portable (ItemDto.trigger, export/import, format doc), importers keep abbreviations.
- Blocked: nothing.
