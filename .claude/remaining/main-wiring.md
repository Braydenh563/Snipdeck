# main-wiring (agent a7b01eee, branch worktree-agent-a7b01eee19bb3bcea)

Goal: wire MainWindow + ListEditor + BoardEditor into snipdeck-app (see task text in the session).

## Done
- Base fast-forwarded to claude/compassionate-pascal-pod8ux (has main_window.slint).
- Store: `TrashRepo::purge_entry` (delete one trash entry) + 4 tests.
- ui: `item-reorder-requested(int)` / `list-reorder-requested(int, int)` on MainWindow (Alt+Up/Down); lint fixes in two ui tests.
- app: `main_window.rs` (MainModel), `controller/main_ctl.rs` (every ItemAction, Trash, Undo, lists, boards, editors,
  reorder, Free cap), `main_binding.rs` (MainBinding, MainUi, editors), shared toast timer + redraw registry in
  `binding.rs`, `Effect::OpenMainWindow` opens the window (main.rs), 30-day Trash purge at start-up.
- Tests: main_window.rs (19 unit), main_controller.rs (29), main_window.rs window tests (26).

## Next (not in this PR)
- Item editor binding (New item, Edit), TagManager, SettingsPage, Recent inbox.
- Panel goldens (`panel_*` in snipdeck-ui) fail at the base commit (push-pin-simple header icon): regenerate in the panel agent's work.
- Windows: caption inset is 0 (OS title bar) until the ADR-0008 client-area chrome exists; taskbar/activation for the
  Main window is by `main_binding::RegularWindow` + the backend hook: unverified on Windows.

## Blocked
- nothing.
