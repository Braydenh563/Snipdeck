# panel-binding (V1-PLAN step 4)

## Done
- settings: `panel_geometry: Vec<PanelGeometry>` (serde default, sanitised, one per monitor, max 16) + tests.
- `frame.rs`: Rect / Monitor / Edge and `clamp` (width 260..=480 logical, height to work area, inside it) + tests.
- `controller/panel.rs`: kebab menu (`APP_MENU`), edit / rename / delete (Trash + 5 s undo), resize, frame_changed / placement / restore_frame; unit tests in controller.rs.
- `binding.rs`: app-menu-*, edit/rename/delete, toast-undo, resize-requested wired; menu entries + undo label rendered; `PlaceWindow` applied; `Binding::frame_changed` / `restore_frame`.
- `main.rs`: Effect arms (resize via `drag_resize_window`, restore on show and start, tray check mark follows Pause hotkeys, not-built effects logged), winit Resized/Moved reported to the binding.
- `tests/panel_window.rs`: 12 new window tests (kebab hover scenario, Esc, outside click, Quit, Pause check, resize clamp + persist, restore, delete + undo, edit / rename).
- CHANGELOG line, CODEMAP regenerated, `Icons` exported from snipdeck-ui for the check mark.

## Next
- Owner on Windows: the checks listed in the report (OS resize, per-monitor restore, work area).
- Follow-ups: Help as a real sub-menu and a `checked` field on `MenuEntry` (needs the UI crate); work area without the taskbar (Win32 `GetMonitorInfo`); Rust-side menu text is English literals in wording.rs until the UI crate exposes translated texts; Panel row context menus and hold-to-reveal are still unbound.

## Blocked
- Nothing. `controller/main_ctl.rs::trash_items` is private to its module, so Panel delete repeats its 6-line Trash call instead of reusing it (making it `pub(super)` would de-duplicate; that file is another agent's).
