# Review 8175019..8fbd4ce (Opus, 2026-11-07) -> fixes
App (next builder, with Symbols slice 6):
- H1 Save as (prepare/finish_icon_save) and copy_icon_svg have no caller: wire the Symbols context menu / Save as submenu (borrow, prepare, drop, ask, finish).
- M3 hotkey test swallows a real press: stop the test on Settings window close (X: window().on_close_requested), on the ownership closer (settings_binding.rs:2149), and on HotkeysPaused; health line shows Cancel while waiting even when paused.
- M4 tour one-click alternative leaves Failed: onboarding_binding.rs:225-252 call ctl.clear_hotkey_problem(action) before update_settings.
- M5 Recent use skips after_use + note_activity (binding.rs:601-604, recent_ctl.rs:223): return UseResult, wire_effects! + after_use.
- M6 save extension: append the kind's extension only when missing; never rewrite an accepted name (symbol_ctl.rs:192).
- L10 alternatives() ignores action (hotkey_health.rs:51): drop the param or use it.
- L9 Recent default-on + preselected Yes in the tour: decided 2026-10-26 (on by default); tour wording should say "on; turn off here" -> keep, wording check.
Portable (light agent):
- H2 Espanso date format injects placeholders (espanso.rs:477): refuse { or } in formats / require exactly one Datetime part; test.
- M7 csvtable.rs:37-90 no cells-per-row cap: cap 256, skip rest; test.
- M8 espanso.rs:97-105 no total byte budget across files; read_file uses uncapped fs::read after metadata check (:206): running budget + read_capped; test.
Tests to add: X-close / pause-resume never swallow the next press; tour alternative clears Failed; Recent paste runs after_use; Espanso format with braces; wide CSV row; Save name with a dot.
