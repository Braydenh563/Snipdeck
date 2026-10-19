# learnability agent

## Done (committed)
- Main window command palette: Ctrl+K, Ctrl+Shift+P, toolbar keyboard-icon button "Commands (Ctrl+K)"; command_texts.slint (@tr words + synonyms), components/command-palette.slint, commands.rs (catalogue, filter, caps), command_binding.rs, controller/commands_ctl.rs (run_command), Effect::OpenCommands
- Help window "Shortcuts" tab (components/shortcuts-sheet.slint, configured chords via commands::chord_caps); Ctrl+/ in Main and Panel; Ctrl+, Settings in Main; Help menu "Keyboard shortcuts"
- Panel: Ctrl+K, kebab "All commands…" + "Keyboard shortcuts", shortcut caps in kebab and item menus, footer Picker-hotkey hint (configured chord), "Close panel (Esc)" tooltip
- Main empty-state hint (EmptyState.hint)
- Tests written: ui logic (main_window_logic.rs, panel_logic.rs), app (main_controller.rs, commands.rs unit tests, controller.rs menu test)

## Unverified (nothing was run to completion; local builds took 50+ min each)
- `cargo check` of snipdeck-ui passed once (before the viewport-y -> content-y fix); snipdeck-app had one error (Global::get needs a concrete window) which is fixed but not re-checked
- No test run at all. Expected golden diffs: main-*.png (new toolbar button, empty-state hint) -> UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test main_window, then look
- Panel/Picker `Close panel` rename updated in panel_logic.rs and panel_window.rs by sed
- clippy 1.99 pedantic not run (command_binding.rs, commands.rs, commands_ctl.rs are new)
- i18n extract/check ran (warnings only)
- Picker opened from the Main window palette uses the Target as read from Main (behaviour untested)

## Next
- Fix whatever CI reports; regenerate main goldens; tray "All commands…" not done
