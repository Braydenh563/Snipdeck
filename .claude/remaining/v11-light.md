# v11-light
- Done: slice 9 (c66a7b7); slice 3a groups (store v8, GroupRepo, export/import minor 1.2, docs/format.md).
- Next: nothing in light lane; 3b (UI) may use GroupRepo; 5a not requested.
- Blocked: gate.sh clippy builds whole workspace (snipdeck-ui); ran scoped -p fmt/clippy/doc/test instead.

## Slice 5a (light2 builder)
- Done: core Trigger + Matcher; store v9 `triggers`, TriggerRepo, tests/fixtures/v8.db; portable: ItemDto.trigger (format 1.2), import/export, importers keep abbreviations as triggers; docs/format.md.
- Next: nothing in 5a. UI lane (5b): trigger field, matcher use, Sensitive/never-paste/password checks, app-side Library impl needs no change.
- Blocked: nothing. gate.sh not run (builds UI); scoped fmt/clippy/doc/test for core, store, portable all pass.
- Review fixes done: trash keeps the trigger row (live-only reads, a trashed claim gives way, restore returns it if free), NFC + grapheme Backspaces, Matcher::after_expansion, prefix overlap returned by set_trigger (Vec<Trigger>) and noted by importers, Espanso word/padded triggers kept in title, bad stored row skipped.
