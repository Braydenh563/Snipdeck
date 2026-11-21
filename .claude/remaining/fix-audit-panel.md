# fix-audit-panel remaining

## Done and pushed (head f676441, gate green)
1, 2 (static + Import/Export/Back up now/Empty Trash), 4-11, 13, 14, 15 (table), 18, 21, 22, 23 (List scope + cut notice), 27, 28, 30, 32-39, 41-44; F2 on Recent (opens Save-to menu).
SPEC post-1.0 notes removed except the eyedropper hotkey, per-swatch Ctrl+S and OLE drop (not mine).

## Next (order)
31 (More menu groups), 2 (dynamic board/list/tag rows), 3, 12, 16, 17, 19, 20, 24, 25, 26, 29, 40, 45, 10k timing probe, 23 (Recent/tag Picker scopes), 15 (version-resource name), 22 (paste-next order).

## Blocked
none

## Notes
- Windows-only changes (18, 21, 27, 28) cross-checked only; unverified.
- 27: clipboard stays on "copied instead" by design (the toast promises the copy).

## Handover notes for the next session (context ceiling reached at cf2070b)
Env: `source /tmp/env.sh`; gate in the background (see CLAUDE.md lean workflow; one poll per ~590 s); clippy with `timeout` hides errors (rc 124): run it in the background and read `rc`.
Clippy gotchas hit: `too_many_lines` (run_command, effect_handler), `semicolon_if_nothing_returned`, `match_same_arms`.
Per item, where to start (all unstarted):
- 16 undo of expansion: `typing_worker.rs` `Typist::event` clears after a fire (`after_fire`); needs the expanded text length from the UI side (`Out::Expand` -> `run_use_erasing` result) and a Backspace-as-next-key path; Windows keystrokes => unverified.
- 17 skipped-trigger reason: record the reason where `trigger_ctl.rs::insert_typed`/`typed_allowed` return `TypedOutcome::Skipped`; add a `doctor.rs` row (Troubleshoot) + i18n.
- 20 typing worker polling / N+1: `typing_worker.rs` `run`, `trigger_ctl.rs`.
- 24 Picker perf: persist the positions map, avoid cloning items (`picker.rs`, `controller.rs` open_picker).
- 45 + 10k probe: extend `crates/snipdeck-app/tests/budget.rs` (ignored, release) with the audit seed (5 boards, 50 lists, 200 tags, 10,000 items, seeded RNG, fixed clock); add Picker open, per-key Picker search, palette filter timings.
- 19 dead keys / IME in the typing hook (`snipdeck-platform/src/windows/typing_hook.rs`): Windows only, cross-check only.
- 25 dismiss on window deactivation + click tests; 26 shared item key map (Panel, Main, Picker); 29 keyboard Move/Size for the Panel; 40 Markdown/JSON transforms, Picker preview pane, Paste as; 31 More menu groups (`APP_MENU` in controller/panel.rs:186, 21 flat rows); 3 palette context in the Panel; 12 min-width vs text scale (measure first); 2 dynamic board/list/tag palette rows; 23 Picker Recent/tag scopes; 15 version-resource app names (Windows); 22 paste-next order (documented behaviour, decide).
Per batch: tests first (fixed clock), i18n via scripts/i18n.py, codemap, fmt, gate --changed, push, update this file.
