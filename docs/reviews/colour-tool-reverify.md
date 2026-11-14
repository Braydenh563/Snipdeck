# Colour tool re-verify (62c299c)

Re-check of docs/reviews/colour-tool-review.md against 62c299c: 37 items fixed. Items still open or new, each confirmed by an independent refuter, numbered C1-C24. Deferred items the worker already listed (5 partial, 17, 18, 30, 33, 35, 49) are tracked separately unless worse than stated.

## C1. [major] Finding 2: typed or stepped hue thrown away on greys and low-chroma colours; Up/Down on H does nothing

**Evidence.** Fixed for the HSB model only. commit() returns the float state (colours_panel.rs:165-181), and channels() shows st.0 for H (63). A typed HSL hue on an exact grey also sticks now (194-198). Tests colours_panel.rs:576-595 and colours_window.rs:218-239 would fail on revert. Still broken, simulated with the crate's own maths:
(a) The HSL model still reads the 8-bit colour. When s>0, channels() shows to_hsl(c).h (line 61), and commit and step re-derive from to_hsl(c) (153-164). Up on H of rgb(58,60,62) in HSL rebuilds the same rgb(58,60,62), so the field stays at 210 forever. A typed HSL hue on that colour snaps back the same way.
(b) Other channels stall the same way. HSL S Up on rgb(30,20,20) stays at 20. CMYK C Up on rgb(13,20,25) (K about 90) stays at 48, because commit uses to_cmyk(c) at 183-191.
(c) Typed HSL L=0 or CMYK K=100 gives black and throws away S or C/M/Y. Typing L or K back gives a grey: the finding-1 bug, now in typed values.
(d) Real Up/Down keys never reach step(). ChannelField's key-pressed sits on the outer FocusScope (colours.slint:927-931). In Slint 1.18.1, TextInput maps UpArrow/DownArrow to Move(PreviousLine/NextLine) and returns EventAccepted (i-slint-core items/text.rs:1033-1041, 1378). Snipdeck's TextField handler rejects these keys (text_field.slint:248-254), so they fall through to that cursor move. No test presses a real key on a channel field: colours_window.rs:228 calls invoke_channel_stepped directly. (d) comes from reading the Slint source and was not run. It was already present at a175a9f.

**Fix.** Use capture-key-pressed on ChannelField (or handle Up/Down in TextField's key-pressed and expose a stepped callback). Keep float state for the HSL and CMYK fields the way HSB does: step and commit from the stored value, not from to_hsl/to_cmyk of the Rgba8. Add a real-key window test (focus the H field, press Up, check the text advances) and step tests for HSL H on rgb(58,60,62), HSL S on rgb(30,20,20) and CMYK C on rgb(13,20,25).

## C2. [minor] New: Save to… can use stale list ids when the list names have not changed

**Evidence.** binding/colours.rs:213-220: the early return when the drawn key is unchanged comes before `s.lists = lists.iter().map(|l| l.id)`. The key compares only the display names (key_of, 417), so the ids the Save to… menu maps to (save_target, 487-498) are refreshed only when some name or board name changes. Scenario: the Colours tab is open; in the Main window the user deletes list 'Ideas' and makes a new list called 'Ideas' in the same place, or swaps two lists with the same name. The names vector is unchanged, so Save to… > Ideas resolves to the old id. A trashed list is no longer found by store.list (see colours.rs:214-223), so the user gets a 'failed' toast. With two same-named lists the colour goes into the wrong one.

**Fix.** Assign s.lists before the early return, or put the ids in the Drawn key.

## C3. [minor] Values fields have no visible channel letter (worse now that they wrap)

**Evidence.** ChannelField passes ch.label only to TextField.label (colours.slint:933-935), and that is just the accessible name: 'A text field has no visible label of its own' (components/text_field.slint:40-41, 125). The missing letter dates from a175a9f. The finding-12 fix makes it worse: at 260px CMYK plus alpha now shows as '73 42 0' over '16 67', so a sighted user cannot tell K from alpha, or which number is which, when typing values (an owner ask). Read from the code only; no render was inspected.

**Fix.** Draw the channel letter (R/G/B, H/S/L/B, C/M/Y/K, A) as a prefix inside each field or as a small caption above it, and check the grid width again at 260px.

## C4. [minor] Round-trips across all formats with typed values

**Evidence.** Property tests: hex is exact for any RGBA, rgb exact when opaque, oklch within 1, HSL/HSB/CMYK texts within 4, and saved_text is lossless for all 7 formats (snipdeck-colour/tests/properties.rs:30-58). The input line takes any parse() literal, live and on Enter (binding/colours.rs:519-545). Gaps: (a) the HSB and CMYK texts drop alpha (format.rs:114-129), so pasting a translucent colour's HSB or CMYK row back into the input line gives an opaque colour, with no test or note. (b) The Values row for HSL and CMYK still re-derives from the 8-bit colour (see finding 2), so a typed value can come back as a different number (C=50 at K=90 shows 53) and some steps stall.

**Fix.** Either add the alpha to the hsb() and cmyk() texts (and to parse), or document that they are opaque-only. Keep float state for the HSL and CMYK fields.

## C5. [minor] 20. Window test weak on HSL; Values-row binding paths untested

**Evidence.** colours_window.rs:113 now asserts the exact `hsl(0, 100%, 50%)`. :199 covers the round trip to brightness 0 and :219 covers a hue typed on a grey and then stepped. Still missing: no test commits or steps a channel in HSL, HSB or CMYK and then checks that the other format rows update (:155 only commits RGB). The test at :199 is named '...and_the_values_row_says_so' but never reads the channels: it computes hue/sat and throws them away with `let _ = (hue, sat);`.

**Fix.** In the :199 test, assert the channel texts after the round trip. Add commit/step tests for HSL and CMYK that check the format rows.

## C6. [minor] 21. Spec scope not implemented (tab keys, card swatch tile, Main window Colour block)

**Evidence.** The tab keys are done (panel.slint:1238-1257). The card swatch tile is done: item-card.slint:676-681 with binding.rs swatch_fields (1858), used for Panel rows and for main_binding.rs:1412. The Main window detail-pane Colour block (spec docs/reviews/colour-spec.md:111: swatch, 7 formats with copy buttons, Open in Colours) is not built: main_window.slint only gains the row fields at :61-63/:204-205. .claude/remaining/colours.md was last changed in a175a9f and is stale. It still lists the tab keys and the swatch tile under Next.

**Fix.** Build the Main window Colour block or keep it as open scope. Update the remaining file.

## C7. [minor] NEW: Deleting the last recent by keyboard leaves no focus item; Esc, Ctrl+S and typing go dead

**Evidence.** The recents grid is conditional: `if ColourState.recents.length > 0: ChipGrid` (colours.slint:1229). Delete (:352) or the menu's 'Remove from recents' on the only recent empties the model, which destroys the grid that holds focus. The popup-close path then bumps recents-focus (:981) into a grid that no longer exists. In Slint 1.18 i-slint-core window.rs:1303-1340, a dead focus item means an empty item_list, so the Panel's capture-key-pressed never runs. Only Tab recovers. Esc does not hide the Panel, and Ctrl+S, Ctrl+C and typing do nothing.

**Fix.** When removing the last recent, move focus first, e.g. to the Saved grid or the input, or keep the grid mounted while it is empty. Add a test: one recent, Delete, then Esc still closes the panel.

## C8. [minor] NEW: Save to… opened from the button or Ctrl+Shift+S returns focus to the input line, not to the invoker

**Evidence.** Both paths set popup-from 0: the Save to… button at colours.slint:1264-1272 and Ctrl+Shift+S at :992-995. On close, `if (root.popup-from == 0) { input.focus(); }` (:987). A keyboard user who activates the Save to… button, or presses Ctrl+Shift+S from the Hue slider or the Formats list, lands in the colour input at the top after Esc or after picking a list, and loses their place. The test colours_window.rs:495-503 asserts this behaviour ('the keyboard is in the input line' after starting from Hue).

**Fix.** Record the invoker (save-to button, or the control that had focus) and refocus it on close, as the chip and format menus already do. Update the test to expect the origin.

## C9. [minor] NEW (not fixed by the popup guard): Esc with the 'Values as' or 'Scheme' dropdown open hides the whole Panel

**Evidence.** The guard added at panel.slint:1176-1182 covers only ColourState.popup. SelectField's dropdown is inline and not a PopupWindow (components/form.slint:451-489, Esc closes it at :459). The Panel's capture handler runs first, and its plain Esc branch (panel.slint:1402-1410) calls close-requested. So Esc on an open SelectField list in the Colours tab (colours.slint ~1194 'Values as', ~1360 'Scheme') closes the Panel instead of the list. This predates the fix commits but is the same class of bug the guard was meant to stop.

**Fix.** Expose an 'open' flag from the tab's SelectFields (or a ColourState.dropdown-open) and include it in the guard at panel.slint:1176.

## C10. [minor] NEW (data safety, predates fixes but now more reachable): Ctrl+Z / Ctrl+Y in the colour input or channel fields run the library Undo/Redo

**Evidence.** panel.slint:1303-1310 handles Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y in the capture phase whenever root.query == "". The Panel search query is always empty while the Colours tab hides the search, and nothing checks ColourState.text-focused. A user who fixes a typo in the colour input, or in a channel field, with Ctrl+Z undoes the last library change instead (for example a Save to Colours, or an item delete). The fix commits made the input the main typing target (the printable redirect at :1546), which makes this more likely.

**Fix.** Skip the Undo/Redo branches when root.colours-open && ColourState.text-focused, as the Ctrl+C branch already does (panel.slint:1254).

## C11. [minor] 34. Contrast marks in one non-wrapping row; footer hover text missing

**Evidence.** The marks now wrap as a flow layout (colours.slint:879-911), and the line height follows m5 (line 837). tests/colours_window.rs:651-679 asserts that the 'On white' line is at least 16px taller at 260px than at 480px, so it fails if the wrap is reverted. The footer (colours.slint:1407) now shows hover-text, then focus-text, then use-hint. However, only the chip grids set hover-text (lines 1245, 1299, 1390). The FormatList `tip` handler (line 1319) calls only root.tip, so hovering a format row does not show its text in the footer, which spec §3.1 asks for ('hovered ... row'). No test covers the footer. The 175% text-size measurement was not done.

**Fix.** In FormatList's tip handler, also set `root.hover-text = on ? t : ""`, and add a window test for the footer text.

## C12. [minor] 40. Square/wheel announces as a slider with no slider semantics

**Evidence.** The value text now names both axes (colours.slint:1156-1158), and accessible-action-increment/decrement were added (722-723). However, Surface sets no accessible-value-minimum, maximum or step (contrast TrackSlider at 624-626). Slint 1.18.1's AccessKit bridge (i-slint-backend-winit accesskit.rs:826-852) then gives the node no numeric range, and the non-numeric value goes out only as a string. On top of that, increment/decrement call paged(±1), which changes brightness in both modes. In Wheel mode the spoken value is 'Hue X degrees, saturation Y percent', so after an increment Narrator reads the same value back while the colour has changed: the action and the announced value do not match. No test covers the surface's accessible value or actions.

**Fix.** In Wheel mode, add brightness to the wheel's value text, or make increment/decrement step saturation. Set accessible-value-minimum/maximum/step for the axis the actions change. Add a window test that reads accessible_value after invoking increment.

## C13. [minor] New: deleting the last recent with the keyboard destroys the focused grid and leaves stale footer text

**Evidence.** Delete on a recent is new (colours.slint:352-355, binding/colours.rs:638-644). The Recently used grid is created only while `ColourState.recents.length > 0` (colours.slint:1228-1229). Deleting the last recent while the grid has focus therefore destroys the focused FocusScope, so keyboard focus leaves the tab with no fallback target. root.focus-text is set only by the grids' `changed focus-label` callbacks (1248, 1302, 1321, 1393), and a destroyed grid fires none. The footer (1407) then keeps showing the removed colour's '#hex, name' until another grid takes focus. The menu path (Remove from recents, then Esc) has the same problem: focus restores to 'Recently used', which no longer exists.

**Fix.** After a remove that empties recents, move focus to the next stop (Formats or the input line) and clear root.focus-text. Alternatively, keep the grid alive with zero chips while it has focus.

## C14. [minor] Eyedropper Win32 button-up change (in scope: Win32 code)

**Evidence.** windows/eyedrop.rs:1180-1213 now takes capture on button-down and decides only on the matching button-up. Every button-up is swallowed, with no DefWindowProc and so no WM_CONTEXTMENU. Lost capture resets the press (WM_CAPTURECHANGED). The position comes from GetCursorPos, so a held drag across overlays works. Press unit tests at 1598-1624 cover the state machine, but not the handle() wiring. A remaining leak: if a key ends the run while a mouse button is held (Esc or Enter during a press), the overlay is destroyed with the button still down, and the later button-up goes to the window underneath. The test page would count that as a 'left up'. This is narrow, and wizard step 6/10 does not exercise it. The HDR check (is_hdr, 518-525) and the doc-comment move (finding 23) are correct by the DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO bit layout. None of this has run on Windows.

**Fix.** On a key decision while Press.held is Some, defer destroying the overlay until the button-up arrives, or keep the overlay until WM_xBUTTONUP (with a short timeout).

## C15. [minor] 5 (card swatch part: PanelRow/MainRow swatch fields, Main row swatch, palette)

**Evidence.** Panel and Main cards now fill the type tile with the colour: binding.rs:1858 swatch_fields (returns no swatch for sensitive rows, checked twice since a sensitive preview is already empty), panel_row binding.rs:1918, main_row main_binding.rs:1412, PanelRow panel.slint:71-73 and 2124-2125, MainRow main_window.slint:61-63 and 204-205, ItemCard item-card.slint:669-688. However, the tile is still a passive `accessible-role: image` labelled "Colour" (item-card.slint:679-680). It is not the spec §3.3 button that opens Paste as. Paste as and Open in Colours are still reachable only from the Panel item menu (callers exist only at controller/panel.rs:418 and :470). There is no Main item menu entry, no Main detail Colour block and no Picker row. .claude/remaining/colours.md was not updated: it still lists 'Swatch tile on Panel cards/rows and Main window rows' under Next, and it does not track the Picker row, the Main item menu or the tile-as-button gap. Tests: only the unit test binding.rs:2032 (swatch_fields). Reverting the Slint or the panel_row/main_row wiring would fail no test.

**Fix.** Make the tile a button (Enter/click opens the Paste as menu) or record the gap. Update remaining/colours.md with the Main item menu, the Main detail block and the Picker row. Add a binding test asserting panel_row/main_row set has_swatch for a colour item and clear it for a sensitive item.

## C16. [minor] 42 Contrast marks never wrap; fixed-width labels overflow at large text

**Evidence.** Fixed: the marks now wrap one by one (colours.slint:879-911), and the line height follows m5 (:838). The format label column comes from the widest label (label-measure, :440 and :449-456) and the 'default' column from default-measure (:441 and :457-462). Test colours_window.rs:650-680 would fail if wrapping were reverted. Not fixed: raw px widths remain, none scaled by text-scale. Picker disclosure 120px (:1085), Picker style Segmented 160px (:1132), Values-as SelectField 120px (:1192). Mark height is a fixed Space.s5 = 20px (:802) while Type.xs grows to about 19px at 175%. Worked from the code (not measured): the disclosure covers x 4-124 and the Segmented sits at x = width - 168, so they overlap whenever the row is narrower than about 292px, which includes the 260px width the tests use. No test covers the label column or these widths.

**Fix.** Size the disclosure, Segmented and SelectField from their content or scale them by SystemLook.text-scale. Put the Segmented on its own line when the row is too narrow. Add a bounding-box test at 260px that the disclosure and the Segmented do not overlap.

## C17. [minor] 48 Palette command 'Saved colours' missing

**Evidence.** Added Command::SavedColours (commands.rs:29-30, ALL[46] at :119). It runs open_saved_colours (commands_ctl.rs:72-77), has texts in CommandTexts.all in the same order (command_texts.slint:21), translations in all six .po files, and test tests/colours.rs:466-475. But availability is `Command::Colours | Command::SavedColours => ctx.colours` (commands.rs:350), not 'when the home list resolves' as the fix asked. So before the first save the command is listed and does nothing: open_saved_colours returns an empty Vec (colour_ctl.rs:436-438) with no toast. The tab's View list button is disabled in the same state (has_saved_list, test tests/colours.rs:278). No test covers running the command before a save.

**Fix.** Add a has_saved_list flag to the command Context and gate SavedColours on it (or show a 'Nothing saved yet' toast). Test that the command is unavailable before the first save.

## C18. [minor] New: undo or failure of 'Save to… → New list…' leaves an empty 'New list N' behind

**Evidence.** SaveTarget::NewList calls new_list_for_save, then make_list_for_save (colour_ctl.rs:329-381). That inserts the list with store.insert_list and no history step. The Saved toast's Undo (show_saved_toast(..., id), :269-277) removes only the item. A failed item insert (:255-260) also keeps the new list. Each Save to → New list followed by Undo leaves another empty 'New list 1/2/…'. This is the known deferred #18 spreading to the new Save to path. It is user-visible clutter, not data loss.

**Fix.** Record the list creation and the item insert as one history step, or delete the made list when Undo empties it. Test it under the fixed clock.

## C19. [minor] New: translucent colour items get a see-through or empty card swatch

**Evidence.** swatch_fields keeps alpha (slint::Color::from_argb_u8(c.a, …), binding.rs:1862-1864), and the tile paints `background: root.swatch` with no checker (item-card.slint:676). The Colours tab draws a Checker for translucent colours (colours.slint:226-229). On a card, an item reading `#3a7bd580` shows a blend with the card background. An item reading `#ff000000` or `rgba(255, 0, 0, 0)` shows an empty tile with only the 1px border, which looks like a missing icon. The kind glyph is hidden (`if !root.has-swatch`, :681).

**Fix.** Draw the tile as opaque colour plus a checker (as Tile does in colours.slint), or show the opaque colour with a translucency mark.

## C20. [minor] Panel 'Paste as' on a colour item shows the untranslated label 'Name'

**Evidence.** panel_select.rs:425-436 builds each row as format!("{}  {}", f.label(), value). Format::label() (snipdeck-colour/src/format.rs:47-56) returns the English 'Name'. The Colours tab translates the same label (colours.slint:452 and 539 use ColourTexts.format-name()), but this Panel menu does not. In de, fr, ja and the other locales the Panel item menu shows 'Name  royalblue'.

**Fix.** Send the format index and let the binding word the label, or use ChooseLabel::Format(f) and translate 'Name' in binding.rs render like the Colours tab does.

## C21. [minor] Escape and footer act on a search query hidden behind the Colours tab

**Evidence.** open-colours (panel.slint:939-950) does not clear the query, and the search box is hidden while the tab is open (panel.slint:573-576). Esc then reaches panel.slint:1400-1403, where 'root.searching' calls clear-query on a query the user cannot see, so the first Esc seems to do nothing. With a query that has no matches, the footer (panel.slint:2639-2655, gated only on rows.length == 0 && searching) shows 'Enter creates "q" in List' under the Colours tab, but Enter is blocked there by list-hidden. The Symbols tab has its own Esc branch (panel.slint:1356); Colours has none.

**Fix.** In open-colours, clear the query the way pick-tab does (panel.slint:1027-1031), and gate the no-results footer on !list-hidden.

## C22. [minor] Swatch tile replaces the item kind in the accessible label

**Evidence.** item-card.slint:680: accessible-label: root.has-swatch ? @tr("Colour") : root.kind-name. A Code or Text card that holds a colour loses its kind name for screen readers. The label also does not say which colour, although the preview line does.

**Fix.** Use kind-name plus 'colour', or keep kind-name and add the hex value.

## C23. [minor] Card swatch and the colour item rules disagree on which items are colours

**Evidence.** binding.rs:1858-1865 (swatch_fields) checks only !sensitive && literal(preview) and ignores the kind. colour_of (colour_ctl.rs:514-518) also requires ItemKind::Text or ItemKind::Code. An item of another text-bearing kind (Template, rich or HTML text) whose text is '#3a7bd5' shows the colour swatch on its Panel and Main cards. Its item menu then lacks 'Open in Colours' (panel.rs:343), its Paste as offers no formats, and Copy as CSS skips it.

**Fix.** Pass the kind to swatch_fields and apply the same rule as colour_of, or share one predicate.

## C24. [minor] New command gating and Ctrl+Tab stop have no failing test

**Evidence.** commands.rs:350-351 gate Colours and SavedColours on ctx.colours, and PickColour on colours && eyedropper. No test turns colours or eyedropper off; the only Context literal (commands.rs:476-477) sets both true. Reverting the gate to 'true' would pass every test. Symbols has this test (commands.rs:761-766 and panel_window.rs:2038-2042). The cycle-tab rewrite (panel.slint:1041-1056) has no test that Ctrl+Tab reaches Colours or wraps past it: panel_logic.rs:706 and 1630 run with colours-available false.

**Fix.** Add a commands.rs test with colours=false and eyedropper=false, plus a panel_logic test with symbols and colours on that walks Ctrl+Tab and Ctrl+Shift+Tab.

# Refuted

- Paste as on a colour text item no longer offers the text transforms
