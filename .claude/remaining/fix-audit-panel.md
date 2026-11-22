# fix-audit-panel remaining

## Done
Pushed (head 0650c7c, gate green): 1-14, 16-18, 19 (dead keys + Caps Lock), 20-39, 40 (Paste as JSON/quote/code, Picker F3 preview), 41-45, 2 and 23 (Recent/tag Picker scopes), 22 (documented).
Also pushed (before this): 19's Settings note is committed after 429ff85 (see git log).
Also pushed: 15 (FileDescription names via `snipdeck_platform::app_names`), 10k probe (cold Panel load fixed afterwards, see docs/PERF.md).

## Next
1. (done) Cold Panel load fixed: 25-34 ms vs 50 ms.
2. (done)
3. Group colour + nesting indent: BLOCKED until the coordinator sends the head of PR #5 branch `claude/compassionate-pascal-pod8ux` (merge it into this branch first).
4. `retro`.

## Not done
- 40's optional hotkeys (paste-then-Enter/Tab, per-position global hotkeys).

## Notes
- Windows-only changes (15, 18, 19's hook, 21, 27, 28) compile and lint for `x86_64-pc-windows-msvc` (`cargo check/clippy --target`), but are unverified on Windows.
- Env: `source /tmp/env.sh`; gate and long cargo runs in the background; remove a stale `/tmp/gate.log` first.
- Clippy gotchas: `option_option`, `too_many_lines`, `assert_is_empty` (1.100), a Slint `if root.width` in a layout makes a binding loop (set a property from `changed width` instead).
