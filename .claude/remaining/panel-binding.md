# panel-binding (V1-PLAN step 4)

## Done
- settings: `panel_geometry: Vec<PanelGeometry>` (serde default, sanitised, one per monitor, max 16) + tests.
- `frame.rs`: Rect / Monitor / Edge and `clamp` (width 260..=480 logical, height to work area, inside it) + tests.
- `controller/panel.rs`: kebab menu (`APP_MENU`), edit / rename / delete (Trash + 5 s undo), resize, frame_changed / placement / restore_frame; unit tests in controller.rs.
- `binding.rs`: app-menu-*, edit/rename/delete, toast-undo, resize-requested wired; menu entries + undo label rendered; `PlaceWindow` applied; `Binding::frame_changed` / `restore_frame`.
- `main.rs`: Effect arms (resize via `drag_resize_window`, restore on show and start, tray check mark follows Pause hotkeys, not-built effects logged), winit Resized/Moved reported to the binding.

## Next
- tests/panel_window.rs window-level tests (kebab hover scenario, Esc / outside click, Quit, resize clamp, delete + undo, a11y).
- gate / fmt / clippy; final report.

## Blocked
- Windows-only: `drag_resize_window`, winit Resized/Moved reaching the binding, monitor name / scale / work area, restore on start (owner on Windows).
