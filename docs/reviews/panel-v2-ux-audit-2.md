# Panel v2 UX audit 2 (2026-11-07, measured from goldens; no live app, no cargo run)

Method: PIL on `crates/snipdeck-ui/tests/golden/` (software renderer). Contrast is WCAG 2.2 from rendered pixels against the measured fill. Coordinates are golden pixels (x, y). Items fixed since audit 1 are not repeated. Two items from audit 1 (5, 2) are still partly open and are marked "open".

## High
1. **225 % text: the board name collapses to "W…" (1 char)** (`panel-scale-light.png`): name ink x16-68 y20-42, caret ink x98-116, so 30 px of the picker stays empty. Breaks spec 2.1 (at least 6 characters) and WCAG 1.4.4 (Resize Text: the current board is no longer readable). Cause: `panel.slint:1161` has a fixed `min-width: 69px`, and `board-switcher.slint:91` sets the caret box to 16 x text-scale (36 px). Fix: `min-width: 69px * SystemLook.text-scale` (155 at 225 %), a caret box of `Space.s4 * chrome-k`, and at text-scale >= 1.5 move Keep on top into More to free 40 px.

## Medium
2. **Minimum height shows 1 full row** (`panel-min-height-light.png`, 320x296): the list viewport is y108-232 (124 px); row 1 is y112-168, and row 2 (y172-228) is cut by the DeckEdge at y222. The pinned strip (y76-104) still shows and QuickAdd takes y233-264 (32 px). Spec 2.12 calls for 3 rows at 296 and says pins hide when fewer than 2 rows show. The spec formula leaves out QuickAdd. Fix: hide pins below 2 full rows (+36 px gives 2 rows), and raise the List minimum to 328 (add 32 for QuickAdd), or fold "New item" into the header New menu at minimum height.
3. **Clipped tab labels with no ellipsis** (open from audit 1, item 5): in `panel-260-light.png`, x190-191 y52-58 is a 2 px sliver of "Code" next to the scroll caret (x205-211). In `panel-scale-light.png`, "Addres" is cut at x291 before the caret at x305. CARP alignment, WCAG 1.4.4. Fix in `components/tab.slint:258`: start the right patch at the end of the last fully visible tab so no partial tab shows, or elide that tab to at least 3 characters plus "…".
4. **225 % icons are not scaled to match the text** (open from audit 1, item 5): header glyphs are 15-16 px (search x149-164 y16-31) next to 22 px "W…" ink; the three row-action glyphs fit in x262-336 y315-327 (about 12 px each); the eye glyph is 15x5 at (324,614). The pin box (x232-271) is 40 px, so the scale is uneven. Fix: apply one `glyph-scale: chrome-k`, with chrome-k = min(text-scale, 1.75), to header, row-action and eye glyphs.

## Low
5. **Cards: the selected number stays `text.secondary`**: 4.56:1 on #05306A at (293,128) in `panel-cards-dark.png`, only 0.06 above 1.4.3 at 11 px. List and Compact switch it to primary (9.09:1). CARP repetition. Fix: use `text.primary` on selected cards too.
6. **No-results footer cuts off the destination** (`panel-no-results-light.png`): the text "Enter creates "zzz" in Email…" ends at x172, before the divider at x183. Spec 2.6 also asks for a keycap "Enter", but the golden shows plain text. Fix: render `Keycap` "Enter" (1 px `border.strong`) and drop " in <list>" when it would elide. The empty-state caption (y357-367) already names the list.
7. **App menu**: "Paste stack" is disabled with no reason given (y119), which hurts learnability. Fix: a caption or tooltip such as "Mark items first" or "Supporter feature". The fixture also leaves out the check that `snipdeck-app/src/binding.rs:1256` adds to "Show numbers", so the golden shows numbers on with no check. Fix: mirror the check in the fixture.
8. **Consistency with the Picker and Main window** (CARP repetition):
   - Selection: the Panel uses a 2 px left bar (#2870D4 at x8-9); the Picker uses a full 2 px outline (`picker-light.png` x8-551 y108-135).
   - Where-line separator: "Work · Email replies" in the Panel, "Work / Email replies" in the Picker.
   - Sensitive mask: 12 dots in the Panel, "•••• •••• 7f3a" in the Picker.
   - Type tiles: only the Panel draws them. Picker and Main use bare glyphs, and text items have no glyph at all (picker row 1, main row 1). DESIGN.md section 2 says "Every item row carries a type tile".
   Fix: choose one form for each and either write it into DESIGN.md or apply it on all surfaces.
9. **Goldens show states the app never renders on open**: `panel-search-light` and `panel-search-always-light` draw the field unfocused (1 px #7C7A76 at y44 and y79/87), but spec 2.11 puts focus in the field on open. The ring is covered by the test `the_search_field_with_a_query_wears_the_focus_ring`, not by a golden. Fix: set `show-focus` in those fixtures so the goldens show the real state.

## Passes (measured)
Titles 16.4-17.5:1 (light), 14.9-15.8 (dark). Previews 6.6 / 6.8, and 6.0 / 4.8 on selected. Tabs 6.3 / 7.2. Row numbers 6.2 (light) and 13.4 / 9.1 on selected (audit 1, item 15 fixed).
Keep on top "on": 1 px #2870D4 border (4.4:1 on bg.app), glyph 5.3 / 7.1 (audit 1, item 1 fixed).
Selected row: 2 px accent bar, 4.3:1 against bg.selected (audit 1, item 3 fixed).
Search mark #ABCDFE: 1.45:1 against bg.selected light and 1.37 dark, text 11.1 / 7.7. Note that spec 2.3 says #D0E3FE (1.16:1, would fail), so DESIGN.md is right and the spec text is stale.
Toasts: Undo is centred (message y496-507, Undo y496-504, in a toast y480-520) at 5.6:1; toast inset 12/12. QuickAdd no longer overlaps cards at 280 width.
Left edges are 16 for board, tab, QuickAdd and footer; the number is centred in its gutter at x21. The tab focus ring is 2 px #2870D4 (4.35:1), y40-69, x8-100.
Empty, no-lists and no-results states: heading 17.5, body 6.5, outline button 34 px tall with a #7C7A76 border, icon tile glyph 5.7.
Search-always field is 36 px (y44-79), placeholder 6.6. Footer glyph 6.2, "Pastes into" 5.5, app name 16.5.
`focus_order_matches_spec` (11 stops, Shift+Tab mirror) and `hit_targets_at_least_24` exist (audit 1, item 8 fixed).

## Not verified
- Windows-only: Mica, real DPI and font hinting (IBM Plex in the real renderer), UIA names and roles as Narrator reads them, focus-visible vs click on a real window, IME in the field, motion and reduced motion, tooltip timing.
- Not covered by a golden: focus order with search expanded (the Close-search stop), whether the tab-row scroll carets are Tab stops, toggle checked states in UIA.
- The awkward-comfortable fixture sets no match spans, so search marks for "w" there are not shown.
