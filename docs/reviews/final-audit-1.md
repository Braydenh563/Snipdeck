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

- Windows rendering, real DPI scaling and Narrator. The minimum-width change relies on the window backend reading
  `min-width` again when the text size changes; on Windows that needs a manual check.
- Fixed #1-#4 are checked by a property test (`smallest_window_width_grows_with_text_size`) and by renders at 600 px
  at 2.25x, not by a golden of the real minimum-size window.

## Audit 1b: keyboard walk and 2.25x fill form

Method: `crates/snipdeck-ui/tests/audit_final1_keys.rs` presses Tab until the cycle closes, then Shift+Tab back,
then Esc on the first, a middle and the last stop. The focus holder is read from the window; its nearest
accessible ancestor is the "control". Every stop is rendered while focused and must paint at least 8 pixels of the
`focus.ring` colour (#2870D4 light, #74ACFF dark) in the 4 px band round its border. Help (4 tabs), item editor
(template, free and Supporter), Settings and the fill form (field, choice, multiline, date, tick box) at 1x and
2.25x, fill form also in dark. Tab off the last stop leaves the window (to the OS), the next Tab comes back.

| # | Finding | Measurement | Fixed? |
|---|---------|-------------|--------|
| 13 | Fill form, choice field: the Tab stop was the field's outer FocusScope, not the radio group, so the screen reader announced the group "Fields" and not "Tone" | focus item 408x110 at (16,116) had role Groupbox "Fields"; 0 ring pixels at the group | Yes: the list has its own FocusScope inside the radio group (`template_field.slint`) |
| 14 | Settings, all five dropdowns (Accent colour, Text size, Language, Panel opens, Dock to): the combobox role sat on the box, the FocusScope was a zero-height sibling in the layout, so focus announced the row's group and the focus ring was drawn on an element that was not the focus holder | focus items 200x0 at (412,700) at 1x, 450x0 at 2.25x, role Groupbox | Yes: scope moved inside the box (`form.slint` SelectField) |
| 15 | All walked windows: every stop has a name, paints the focus ring in both themes at 1x and 2.25x, Esc reaches the window from first, middle and last stop, no trap | 57 stops Settings, 13 / 18 item editor (free / Supporter), 8 fill form, 6 / 5 / 2 / 4 Help tabs | OK |
| 16 | Fill form at 2.25x with the new 608 px minimum: the three-line multiline box and the long date error show in full | date error is the only red text, its right edge is more than 16 px inside the window; three ink lines in the box (`fill-225-errors.png` opened) | OK, asserted |
| 17 | Settings: after the first lap Tab never returned to "Search settings" and "Jump to a section"; forward Tab skipped the "Licence key" field | Slint skips an item hidden by a clipping `Rectangle` that sits between it and a scroll view (`is_visible_or_clipped_by_flickable` only forgives a `Flickable` as the first clipping ancestor). `TextField` wrapped its `Flickable` in a `clip: true` Rectangle, so a field scrolled out of view dropped out of the Tab order. Reproduced by `focus_traversal_repro.rs` (`text_field_scrolled_out_of_a_flickable`) | Yes: `clip` removed from the field viewport (`text_field.slint`; the Flickable already clips). Walk is clean at 1x and 2.25x |
| 18 | Item editor at 2.25x: Shift+Tab from "Remove formatting" skipped the "Template text" field and went to "Kind" | same cause: the content slot was a `clip: true` Rectangle around the field. Pinned as Slint behaviour by `control_in_clipping_rectangle_in_a_flickable_is_skipped` | Yes: `clip` removed from the slot (`editors.slint`) |
| 19 | Help Tips tab: Tab stopped after the third "Try it" (the next Tab left no focused control) and Shift+Tab never reached the first cards | different cause: each card called `FormScroll.reveal(card.y, ...)` on focus, passing a content-relative y where the function expects a y relative to the visible area, which fought Slint's own scroll-to-focus. With the call removed all 10 buttons are visited in order in both directions (`offscreen_cards_are_tab_stops` shows off-screen cards are normal Tab stops) | Yes: manual `reveal` removed from the Tips cards (`help.slint`); Slint scrolls the focused button into view |

Not verified: a real keyboard on Windows (17-19 were root-caused and fixed in the .slint sources, not worked round in the
harness; Slint's scroll-to-focus on Windows is assumed to match the software backend), Narrator announcing the choice list and
dropdowns (13, 14 are fixed by the accessible tree, not heard), visual order on scrolling pages (Settings, Tips,
item editor at 2.25x): window y moves with the scroll so the order check is skipped there (focus order was read
from the names instead).
