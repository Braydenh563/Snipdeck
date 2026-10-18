# main-wiring (agent a7b01eee, branch worktree-agent-a7b01eee19bb3bcea)

Goal: wire MainWindow + ListEditor + BoardEditor into snipdeck-app (see task text in the session).

## Done
- Base fast-forwarded to claude/compassionate-pascal-pod8ux (has main_window.slint).
- Store: `TrashRepo::purge_entry` (delete one trash entry) + 4 tests.

## Next
- app: Library += TrashRepo; src/main_window.rs (MainModel); controller main_* methods + undo toast; slint callbacks
  (item-reorder-requested, list-reorder-requested); src/main_binding.rs; main.rs OpenMainWindow; tests/main_window.rs;
  stale comments; fmt/clippy/test/doc/i18n; gate.

## Blocked
- nothing.
