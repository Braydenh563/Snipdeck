# Final audit 1: design and accessibility (WCAG 2.2 AA, CARP)

Scope: Settings, command palette, Shortcuts help, toasts, Help (incl. Tips), item editor (template chips,
Supporter badges, "Formatted" notice and Remove formatting), template fill form, Panel "Formatted" mark.
Not audited (other workers): Panel group headers, typed-trigger/suggest settings, Windows Hello, Main toolbar
Show Panel icon, tips pictures.

Method: `crates/snipdeck-ui/tests/audit_final1.rs` renders each window (light/dark, smallest size, 1.5x and
2.25x text size, frozen clock) and writes PNGs when `SNIPDECK_AUDIT_DIR` is set; contrast and positions were
measured from the pixels. `audit_final1_tree.rs` walks the accessible tree. Only a software renderer was
used: nothing here is verified on Windows (DirectWrite text, real DPI scaling, narrator).

| # | Finding | Measurement | Severity | Fixed? |
|---|---------|-------------|----------|--------|
| 1 | At 2.25x text and the smallest window, the item editor footer is clipped ("Cancel", "Save" cut off at the right edge; "Move to Trash" runs into them) and the Kind chips lose their labels | 420 px wide at 2.25x: Save button ends beyond 420; fits at 600 | High (1.4.4 / 1.4.10 reflow) | Yes: minimum width = max(420, 270 x text scale) |
| 2 | Same size: Help tab bar labels overlap ("Troubleshoot / About / Shortcuts / Tips" drawn over each other) and a Shortcuts line is clipped top and bottom | 420 px at 2.25x broken; 600 px clean | High | Yes: same rule on Help |
| 3 | Same size: template fill footer buttons overlap ("Can", "Ins" clipped) and the key hint crowds them | 320 px at 2.25x broken; 600 px clean | High | Yes: minimum width = max(320, 270 x text scale) |
| 4 | Settings at 2.25x in a 420-600 px window: each row's title wraps one word per line, the description letter by letter, and the shortcut recorder runs past the right edge | 420 and 600 px broken; 800 px clean (765 px used) | High | Yes: minimum width = max(420, 340 x text scale) |
| 5 | Item editor placeholder help: the Supporter badge and help text start at a different x on every row (they follow the chip, whose width follows the token) | help text starts at x = 77, 77, 150, 75, 102, 85 on the six free rows and 213, 246, 280, 287 on the Supporter rows (badge left edge 136, 169, 203, 210, 225); at 560 px wide | Medium (CARP alignment) | No: needs a shared chip column; Slint has no max over a model, so either a measured width passed in by the host or a grid. For the orchestrator |
| 6 | Chip, badge and help text vertical centring (Supporter rows) | chip and badge both rows 524-547 (24 px), help text 532-541 (centre 536.5 vs 535.5) | OK | n/a |
| 7 | Contrast: Supporter badge text 5.24:1 on its fill; placeholder help 6.19:1; "Formatted" mark text (92,89,86) on (235,233,231) about 5.8:1; "Saving changed text removes the formatting" notice 16.2:1 light, 10.3:1 dark; toast Undo 11.1:1 | computed from rendered pixels | OK (>= 4.5) | n/a |
| 8 | Disabled (free plan) placeholder chips are faint | disabled controls are exempt from 1.4.3; the Supporter badge next to each says why in text | Info | n/a |
| 9 | Accessible tree: every shown Button, Checkbox, Switch, RadioButton, Tab, Combobox, TextInput and Spinbox in Help (4 tabs), item editor (template, Formatted), fill form and Settings has a name, is at least 24 x 24 px, and stays inside the window, at 1x and at 2.25x with the new minimum widths | `audit_final1_tree.rs` (asserts the problem list is empty and that more than 100 elements were checked) | OK | n/a |
| 10 | Visible label is contained in the accessible name ("Remove formatting" / "Remove formatting, keep the text"; "Try it" / "Try it: <title>"; "Insert {x}") | read from the .slint sources | OK (2.5.3) | n/a |
| 11 | Panel "Formatted" mark at 1x, 1.5x, 2.25x | mark sits on the preview line, no clipping at 320 px; at 2.25x it wraps under the title, left aligned | OK | n/a |
| 12 | Command palette and toast at 480 px, 2.25x | no clipping; key chips stay on their row | OK | n/a |

## Not verified

- Focus order by keyboard and keyboard traps were not walked in this audit (existing `*_logic.rs` tests cover
  Tab order for the editors, Settings and Help); not re-measured here.
- Windows rendering, real DPI scaling and Narrator. The minimum-width change relies on the window backend reading
  `min-width` again when the text size changes; on Windows that needs a manual check.
- Fixed #1-#4 are checked by a property test (`smallest_window_width_grows_with_text_size`) and by renders at 600 px
  at 2.25x, not by a golden of the real minimum-size window.
