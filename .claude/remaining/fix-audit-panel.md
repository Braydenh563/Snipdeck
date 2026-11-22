# fix-audit-panel remaining

## Done
Pushed (head 5883d08, gate green): 1-14, 16-18, 19 (dead keys + Caps Lock), 20-39, 40 (Paste as JSON/quote/code, Picker F3 preview), 41-45, 2 and 23 (Recent/tag Picker scopes), 22 (documented).
Committed, not yet pushed: 15 (FileDescription names via `snipdeck_platform::app_names`, one function `friendly_app_name`).

## Next
1. 10k probe numbers into `docs/PERF.md` (test exists: `budget.rs measure_picker_and_palette_budgets`; run with `CARGO_PROFILE_DEV_OPT_LEVEL=3 CARGO_PROFILE_DEV_DEBUG=0`, in the background: the build outlasts 600 s).
2. Gate, clippy (stable + 1.100), push, report to the coordinator.
3. Group colour + nesting indent: BLOCKED until the coordinator sends the head of PR #5 branch `claude/compassionate-pascal-pod8ux` (merge it into this branch first).
4. `retro`.

## Not done
- 40's optional hotkeys (paste-then-Enter/Tab, per-position global hotkeys).
- Settings help text: triggers need committed text (19).

## Notes
- Windows-only changes (15, 18, 19's hook, 21, 27, 28) compile and lint for `x86_64-pc-windows-msvc` (`cargo check/clippy --target`), but are unverified on Windows.
- Env: `source /tmp/env.sh`; gate and long cargo runs in the background; remove a stale `/tmp/gate.log` first.
- Clippy gotchas: `option_option`, `too_many_lines`, `assert_is_empty` (1.100), a Slint `if root.width` in a layout makes a binding loop (set a property from `changed width` instead).
