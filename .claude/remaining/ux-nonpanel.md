# ux-nonpanel (agent: Sonnet 5.5, branch claude/compassionate-pascal-pod8ux)
Order: 1 shortcuts search, 2 Ctrl+Shift+P, 3 theme quick switch, 4 row density, 5 visibility audit, 6 palette check. One commit per item.
Build: CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/target-main, -j1 always (a snipdeck-app test build is ~4.5 min).
## Done
1 shortcuts search: 0d86a5e (gate --staged: fast-checks ok, codemap stale on purpose until the last commit, i18n ok).
2 Ctrl+Shift+P keep-on-top: 32cfc06.
3 theme quick switch: View eye button in the Main toolbar (menu), 3 palette commands, toast ThemeSet; 19 Main goldens regenerated and looked at (toolbar crops old vs new, rail and 225% checked).
4 row density: bd22039 (Settings.row_density, View menu rows, palette "Row density: ...", Settings > Appearance row, View.compact_rows and Panel `compact-rows` for the redesign). 4 settings goldens regenerated and looked at (new row only).
5 audit: bd22039, docs/reviews/shortcut-visibility.md. Added palette Paste next item and Select all items. Picker Ctrl+E has no visible control: open.
6 palette check: it is searchable (SearchField, host filter). Bug found by measurement: the list narrowed for the scrollbar was centred (unset x), so rows still ran 12 px under the strip. Fixed with `x: 0px`; test a_long_palette_list_keeps_its_rows_clear_of_the_scrollbar_strip.
## Next
Final gate (--changed) and the report. Translations (all 6 locales) need a native review: see the list in the commit messages.
## Blocked
Picker Ctrl+E button needs a Picker design decision.
