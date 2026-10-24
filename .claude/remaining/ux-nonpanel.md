# ux-nonpanel (agent: Sonnet 5.5, branch claude/compassionate-pascal-pod8ux)
Order: 1 shortcuts search, 2 Ctrl+Shift+P, 3 theme quick switch, 4 row density, 5 visibility audit, 6 palette check. One commit per item.
Build: CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/target-main, -j1 always (a snipdeck-app test build is ~4.5 min).
## Done
1 shortcuts search: 0d86a5e (gate --staged: fast-checks ok, codemap stale on purpose until the last commit, i18n ok).
2 Ctrl+Shift+P keep-on-top: panel.slint 4 lines + tests (panel_logic, panel_window) pass.
## Next
3 theme quick switch (View menu button in the Main toolbar, palette, toast), then 4, 5, 6.
## Blocked
(none)
