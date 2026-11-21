# Colour tool review (2026-11-12)

Independent review of origin/claude/colour-tool at a175a9f. Four opus reviewer lenses ran (correctness, Win32 safety, spec, design and a11y), plus a completeness critic. A sonnet refuter checked each finding, and these are the ones that survived. Some appear twice from different lenses.

## 1. [major] Brightness/hue sliders and arrow nudges recompute S and H from the rounded 8-bit colour: drag to 0 turns the colour grey, and any drag drifts

- **At:** `crates/snipdeck-app/src/binding/colours.rs:573`
- **Evidence:** on_bri_moved (573-578) takes `(h, sat, _) = panel::hsb(v.current, hue)` from the current Rgba8 on every move. on_hue_moved (563-569) and on_surface_nudged (554-559) do the same for sat and bri. to_hsv returns s=0 whenever max==min (model.rs:205-210), and black is such a case. So once brightness reaches 0 the saturation is lost: dragging back up gives a grey, and the remembered hue only keeps the H field. Above 0 the rounding drifts. I simulated the slider path in Python with the same HSV maths: 
- **Fix:** Keep the picker's own (h, s, b) as floats in Glue: update it from gestures and typed values, and only re-derive it when the colour changes from outside (typed literal, swatch, eyedropper, Open in Colours). The sliders and nudges then read s and b from that state, not from the Rgba8. Add a test: bri 84 -> 0 -> 84 keeps rgb(58,123,213).

## 2. [major] A typed or stepped hue is thrown away on greys and low-chroma colours; Up/Down on H does nothing

- **At:** `crates/snipdeck-app/src/binding/colours.rs:618`
- **Evidence:** apply_channel sets `pinned_hue = Some(hue)` (618) with the OLD remembered hue, then immediately overwrites it with `panel::hsb(c, hue).0` (619-620). On a grey that is again the old hue (hsb falls back to `hue` when s==0). So typing H=120 on rgb(128,128,128) leaves the field at the previous hue, which contradicts the comment 'The hue field of a grey stays where the user put it'. Separately, step() (colours_panel.rs:194-217) reads the rounded displayed text and commits text+delta. When the new hue
- **Fix:** In apply_channel, pin the hue the user typed or stepped (pass it back from commit/step, e.g. return (Rgba8, Option<f32>)). Keep float channel state for the Values row so that a step moves from the stored value rather than the re-derived 8-bit one. Add tests: H typed on a grey sticks, and Up on H of rgb(58,60,62) advances the field.

## 3. [major] Pick and cancel act on button-down and the overlay is destroyed before button-up, so the button-up (and right-click context menu) goes to the app underneath

- **At:** `crates/snipdeck-platform/src/windows/eyedrop.rs:1116`
- **Evidence:** `handle` picks on WM_LBUTTONDOWN (1116-1119) and cancels on WM_RBUTTONDOWN (1120). `feed` then calls PostQuitMessage (1001), the loop exits on the next GetMessageW (749) and `drop(slot_guard)` (768) destroys every overlay straight away, within microseconds. Nothing calls SetCapture, and WM_LBUTTONUP / WM_RBUTTONUP are not handled anywhere in the file. The physical button-up arrives about 100 ms later and goes to whatever window is now under the cursor. With right-click to cancel, that is a WM_RB
- **Fix:** Call SetCapture on button-down, finish the pick or cancel on WM_LBUTTONUP / WM_RBUTTONUP, and swallow both button-ups before the overlay is destroyed. Alternatively, keep the overlay until the matching button-up arrives.

## 4. [major] "Save to…" only lists the first 8 lists of the board on screen, and has no "New list…" (owner ask: save "anywhere else")

- **At:** `crates/snipdeck-ui/ui/colours.slint:1113`
- **Evidence:** Spec D6a says "Save to…" opens the Move to… chooser: every board's lists plus "New list…". In the branch, colour_ctl.rs:314-320 `colour_save_targets` returns only `self.model.tabs()`, which is the lists of the board the Panel shows. The popup at colours.slint:1119-1128 is 8 hard-coded entries `list-names[0..7]`, so a 9th list or a list on another board cannot be chosen. There is no "New list…" entry. The spec's chooser reuse (`panel_select.rs move_to_list`) is not used. .claude/remaining/colours
- **Fix:** Use the Move to… chooser (all boards plus New list…) for Save to…, or build an unbounded model from every board's lists. Add a test that saves to a list on another board.

## 5. [major] Copy any format from where a colour is stored works only in the Panel item menu: no card swatch, nothing in the Main window or the Picker

- **At:** `crates/snipdeck-app/src/controller/panel_select.rs:425`
- **Evidence:** Spec D6a table and §3.3 ask for a swatch type tile on Panel cards (a button that opens Paste as), a Main window row swatch, a detail-pane Colour block with 7 copy buttons and Open in Colours, and Paste as / Open in Colours from a Picker row. The diff does not touch item-card.slint, main_window.slint, picker.rs or main_ctl.rs. main_binding.rs only gains two context arguments. Grep for swatch, snipdeck_colour and OpenInColours in those files finds nothing. The only route is Panel item menu → Paste
- **Fix:** Implement the §3.3 swatch tile and the Main and Picker entries, or add the missing parts (Picker row, Main item menu) to remaining/colours.md so they are tracked.

## 6. [major] Format rows cannot be reached or used by keyboard (no paste by keyboard, no Up/Down, no menu key)

- **At:** `crates/snipdeck-ui/ui/colours.slint:895`
- **Evidence:** Spec §3.1: format rows are one tab stop with Up/Down/Home/End, Enter pastes, Shift+Enter copies, and Menu/Shift+F10 opens the row menu. User story 7 asks for "all of it by keyboard". Each row at colours.slint:895-966 is a plain Rectangle with a TouchArea and no FocusScope. Only the IconButton copy (line 959) can take focus, and it only copies. Paste-into-Target, Make default format and Save from a row are mouse-only, apart from Narrator's default action. remaining/colours.md does not mention thi
- **Fix:** Wrap the format rows in a FocusScope with roving selection, as ChipGrid does, and handle Enter, Shift+Enter, Up/Down/Home/End and Menu/Shift+F10.

## 7. [major] Windows wizard for the eyedropper is missing, so the owner cannot run the only check of the eyedropper

- **At:** `tools/windows-test/tests/colour.json:0`
- **Evidence:** Spec §7 and slice 2's acceptance require `tools/windows-test/tests/colour.json` plus a test page with #3a7bd5, #ff0000 and #ffffff squares (wizard steps 1-10). `git ls-tree origin/claude/colour-tool tools/windows-test` has no colour file. remaining/colours.md says "owner must try it (`tools/windows-test/`)", but there is nothing there to run. The 1515-line WinEyedropper has never run.
- **Fix:** Add colour.json and the test page per §7 before asking the owner to verify.

## 8. [major] All 78 new Colours strings are untranslated in all six locales

- **At:** `crates/snipdeck-ui/lang/de/LC_MESSAGES/snipdeck-ui.po:10`
- **Evidence:** I compared each branch .po file with lang/snipdeck-ui.pot using a small parser (no msgfmt on this machine). de, es, fr, ja, pt-BR and zh-Hans each have 78 entries with empty msgstr. On claude/compassionate-pascal-pod8ux the same check gives 0 empty entries. Examples: "Colours" (the tab and the list name the spec calls a 'translated name'), "Hue", "Alpha", "Contrast", "AA large", "On white", "Open in Colours", "Copy as CSS variables", "Show Colours tab" and the Settings descriptions. `scripts/i18
- **Fix:** Run `python3 scripts/i18n.py todo <LOCALE>` for each locale and translate the 78 entries. Use the terms the catalogue already has (Copy, Save to…, Paste as, list). Leave the CSS keyword values untranslated, as spec §2 says.

## 9. [major] Format rows cannot be used from the keyboard; their menu is right-click only

- **At:** `crates/snipdeck-ui/ui/colours.slint:895`
- **Evidence:** Each format row (lines 895-966) is a Rectangle with a TouchArea and has no FocusScope. Keyboard focus can only reach the per-row Copy IconButton, which makes 7 tab stops instead of one group. 'Paste into the Target', 'Make default format' and 'Save to Colours' run only from `pointer-event` with the left or right button (lines 943-956). Spec §3.1 requires Up/Down, Home/End, Enter (paste), Shift+Enter (copy) and Menu/Shift+F10 for the row menu. A keyboard-only user cannot paste a format into the t
- **Fix:** Make the format rows one roving list, like ChipGrid: a FocusScope with a selected index, Up/Down/Home/End, Enter as format-used, Shift+Enter as format-copied, Menu or Shift+F10 opening popup 2 at the row, and a 2px focus-ring on the selected row while focused.

## 10. [major] Swatch menu and Remove from recents have no keyboard path

- **At:** `crates/snipdeck-ui/ui/colours.slint:240`
- **Evidence:** ChipGrid's key-pressed handler (lines 240-255) handles only the arrows, Home/End and Enter/Space. Popup 1 ('Make current · Copy · Save to Colours · Remove from recents') opens only on a right-button pointer-up (line 306). Spec §3.1 lists 'right-click, Menu' for swatches and 'Delete' for recents. No key saves a single swatch or removes a recent.
- **Fix:** In ChipGrid's FocusScope, handle Key.Menu and Shift+F10 by calling `menu(selected, <chip centre>)`, and handle Key.Delete in the recents group by calling `swatch-removed`.

## 11. [major] Tooltips advertise Ctrl+S, Ctrl+Shift+S and Ctrl+I, but none of these shortcuts exists

- **At:** `crates/snipdeck-ui/ui/colours.slint:854`
- **Evidence:** The tooltips at line 854 ('Save to Colours (Ctrl+S)'), line 861 ('Save to a list (Ctrl+Shift+S)') and line 682 ('Pick a colour from the screen (Ctrl+I)') name shortcuts that nothing handles. ColoursView has no key handler. In panel.slint the Ctrl branch (around line 1214) handles 's' only when `symbols-open`. `.claude/remaining/colours.md` lists 'Panel keys in the Colours tab: Ctrl+S, Ctrl+Shift+S, Ctrl+I' under Next. So a user who presses Ctrl+S as the tooltip says gets nothing.
- **Fix:** Wire the three shortcuts in the panel key handler when `colours-open`. Until then, remove the shortcuts from the tooltips.

## 12. [major] The Values row is wider than the default Panel and gets clipped

- **At:** `crates/snipdeck-ui/ui/colours.slint:807`
- **Evidence:** The row is a HorizontalLayout: SelectField `width: 76px`, plus one ChannelField per channel with `min-width: Space.s12 + Space.s2` (56px), `spacing: Space.s2`, and padding of 8+8. For CMYK that is 76+4*56+4*8+16 = 348px. CMYK on a translucent colour adds A (colours_panel.rs:93): 76+5*56+5*8+16 = 412px. The Panel default is 320px and its minimum is 260px (panel.slint:189,193). Even RGB at 260 needs 284px. FormScroll sets `content-width: self.viewport-width` (line 706), so nothing scrolls sideways
- **Fix:** Wrap the channel fields onto a second line under the model select, or shrink them below 56px with an elided label. Add a 260px and 320px window test with CMYK plus alpha that checks every field's bounding box sits inside the viewport.

## 13. [major] "View list" (open_saved_colours) never shows the list, even after a save: the Colours tab stays over it

- **At:** `crates/snipdeck-app/src/controller/colour_ctl.rs:346`
- **Evidence:** open_saved_colours only calls model.select_board/select_list and returns [Effect::ShowPanel]. In panel.slint, colours-open is written only by open-colours()/close-colours()/pick-tab()/cycle-tab() and `changed colours-available` (lines 937-1115). No `changed current-tab` handler exists (grep is empty). Binding render() only does set_current_tab (binding.rs:1590), and Effect::ShowPanel only shows and raises the window (binding.rs:443-458). So ColoursView (`if root.colours-open`, panel.slint:2322) 
- **Fix:** Have the binding close the Colours tab and pick the list's tab, for example with a new Effect::ShowList or by calling invoke_pick_tab after the select. Add a window test: save, click View list, assert !colours_open and current_tab is the Colours list.

## 14. [major] With HSB or CMYK as the default format, exact colours save as hsb()/cmyk() text that the app does not see as a colour

- **At:** `crates/snipdeck-colour/src/saved.rs:11`
- **Evidence:** The format row menu offers 'Make default format' for every format except Name (colours.slint:1095; can_be_default excludes only Name). saved_text keeps format(c,f) whenever parse() gives the colour back exactly, and parse accepts hsb()/cmyk() (parse.rs:132,140). So #ff0000 or #ffffff with a CMYK/HSB default is stored as 'cmyk(0%, 100%, 100%, 0%)' or 'hsb(0, 0%, 100%)'. colour_of() uses literal(), which accepts only #hex, rgb, rgba, hsl, hsla and oklch (parse.rs:259-273). The saved item is then m
- **Fix:** In saved_text, store hex when f is Hsb or Cmyk (or whenever literal(text) != Some(c)). Add a test that saves #ff0000 with Cmyk and Hsb defaults and finds it in saved_colours().

## 15. [minor] typing_input / typing_other flags go stale when a typed literal equals the current colour, so the next swatch click leaves the input line showing the old text

- **At:** `crates/snipdeck-app/src/binding/colours.rs:171`
- **Evidence:** on_input_edited sets `typing_input = true` (370) and calls act -> sync. If the literal is the colour already current (e.g. '#fff' -> '#ffff' -> '#ffffff'), set_colour is a no-op, the drawn key is unchanged, and sync returns early at 171-172, before `s.typing_input = false` (189). The flag survives. On the next real change (a recent/scheme chip click, or Open in Colours while the tab is open), `changed && !s.typing_input` (185) is false and the input field keeps the stale text while every format 
- **Fix:** Clear typing_input/typing_other before the early return, or include them in the drawn key; alternatively only set them when the colour actually changes.

## 16. [minor] Paste as on a colour item bypasses run_use: no use count or last-used stamp for the item

- **At:** `crates/snipdeck-app/src/controller/panel.rs:413`
- **Evidence:** The new branch `if let Some(c) = self.colour_of_item(id) { ... self.use_colour(c, format, UseMode::Paste) }` goes through use_text (symbol_ctl.rs), which only writes the clipboard and toasts. The old path use_item_as -> run_use 'counts the use' (use_ctl.rs:47-58). A colour snippet pasted via Paste as no longer moves in Most used / Recently used sorts, unlike every other Paste as. No test covers choosing a colour Paste as row through choose_item_menu: the tests at controller/tests/colours.rs:440-
- **Fix:** Record the item use (the same bookkeeping run_use does) after a successful colour Paste as, or route it through run_use with the formatted text. Add a choose_item_menu test that asserts the pasted text and the use count.

## 17. [minor] Eyedropper result is applied after the frame's view was rendered; its toasts can stay invisible until an unrelated redraw

- **At:** `crates/snipdeck-app/src/binding.rs:536`
- **Evidence:** sync() builds and renders `view` (507-509) and only then calls poll_eyedropper (536-539). colour_picked shows a 'Copied' toast and possibly the HDR toast, but the toast check further down uses the stale `view.toast`. apply(ShowColours) only triggers another redraw when the tab was closed (open-colours returns early if colours-open, so colours-toggled/act never runs). The redraw_from_any_thread wake was spent producing this frame. I could not verify this at runtime: it needs Windows and the overl
- **Fix:** Poll the eyedropper before computing the view in sync, or call redraw_all() after applying picked effects.

## 18. [minor] Undo of the first save leaves an empty 'Colours' list behind; a failed item insert after list creation leaves the Panel unrefreshed

- **At:** `crates/snipdeck-app/src/controller/colour_ctl.rs:253`
- **Evidence:** home_list() inserts a new list (280-286) outside any history step. show_saved_toast's Undo only trashes the item (toast_ctl.rs:88-95). Spec D6 says saving uses 'same path, toast, Undo' as quick_add, where nothing extra is created. If insert_item then fails (231-234), the function returns before library_changed(), so the new list is in the store and in colour_save_list but not on the Panel tabs.
- **Fix:** Create the list and the item in one store.atomically batch, record it as one undoable step, and call library_changed on the failure path too.

## 19. [minor] Hue field shows '360' for hues >= 359.5, against the spec rounding rule

- **At:** `crates/snipdeck-app/src/colours_panel.rs:58`
- **Evidence:** channels() prints `whole(v.h)` = `format!("{}", v.round())`. For rgb(255,0,2) the hue is 359.53, so the H field reads '360' while the HSL/HSB format rows print 0 (spec §5: 'a hue that rounds to 360 is 0').
- **Fix:** Map a rounded 360 to 0 for the H label in channels().

## 20. [minor] Window test asserts almost nothing about the HSL row, and the Values-row binding paths are untested

- **At:** `crates/snipdeck-app/tests/colours_window.rs:107`
- **Evidence:** `assert!(hsl.value.contains("0"))` passes for nearly any HSL string. No test drives channel_committed/stepped for HSL/HSB/CMYK and then checks that the other format rows update. No test covers a hue typed on a grey, the slider round trip, or channel_stepped at all. These are the paths where the bugs above live. No existing test was weakened: toast_texts.rs only adds two variants and bumps VARIANTS. I found no real-clock use: new code reads self.clock, and the only thread::sleep is in the Windows
- **Fix:** Assert the exact HSL text (hsl(0, 100%, 50%)). Add binding-level tests for channel commit/step in each model, for a hue on a grey, and for the bri 0 round trip.

## 21. [minor] Parts of the spec scope are not implemented: Ctrl+S / Ctrl+Shift+S / Ctrl+I in the tab, the card swatch tile, the Main window Colour block

- **At:** `.claude/remaining/colours.md:10`
- **Evidence:** The remaining note lists these under Next. panel.slint key handling only has the Symbols `s` branch (line ~1223); nothing handles Ctrl+S, Ctrl+Shift+S or Ctrl+I while colours-open. main_window.slint and main_binding.rs gain no colour block. colour-spec §3.1 and D6a require all of them. Saving works only through the Save button and chip menus.
- **Fix:** Track these as open scope before calling the feature done; wire the tab keys at minimum.

## 22. [minor] HDR check treats SDR displays with Auto Color Management / wide colour as HDR

- **At:** `crates/snipdeck-platform/src/windows/eyedrop.rs:505`
- **Evidence:** `let enabled = unsafe { colour.Anonymous.value } & 0b10 != 0;` reads only advancedColorEnabled (bit 1). On Windows 11, an SDR display with ACM (wide colour gamut enforced) also reports advancedColorEnabled = 1, with wideColorEnforced (bit 2) set. The code never checks bit 2, so those monitors would show the 'HDR is on' note in the overlay and the EyedropHdr toast (colour_ctl.rs:182-187). This rests on the documented bitfield layout and could not be checked on a real machine.
- **Fix:** Treat a monitor as HDR only when bit 1 is set and bit 2 (wideColorEnforced) is clear. On 24H2 and later, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO_2.activeColorMode == HDR is the more direct check.

## 23. [minor] hdr_devices doc comment is attached to active_paths

- **At:** `crates/snipdeck-platform/src/windows/eyedrop.rs:426`
- **Evidence:** Lines 426-428 put two doc paragraphs together: 'The GDI device names ... in HDR mode ... not marked approximate.' and 'The active display paths, None when ...'. Both land on `fn active_paths`. `fn hdr_devices` (469) has no doc, and rustdoc shows the HDR text on the wrong function.
- **Fix:** Move the first paragraph (426-427) to sit above `fn hdr_devices`.

## 24. [minor] The Windows test plan the module cites does not exist

- **At:** `crates/snipdeck-platform/src/windows/eyedrop.rs:29`
- **Evidence:** The module doc says '**Unverified on Windows** (`tools/windows-test/tests/colour.json`)', and .claude/remaining/colours.md lists 'owner must try it (`tools/windows-test/`)' as blocked. `git ls-tree origin/claude/colour-tool -- tools/windows-test` has no colour.json, only m2/m3/quick-check/s1/s2/s4. The owner has no script for the Windows-only eyedropper checks (multi-monitor, mixed DPI, HDR, foreground fallback, Esc and right-click cancel).
- **Fix:** Add tools/windows-test/tests/colour.json with the cases listed, or remove the reference.

## 25. [minor] Colours-tab keys missing: Ctrl+I, Ctrl+S, Ctrl+Shift+S, Ctrl+C, and typing into the input field

- **At:** `crates/snipdeck-ui/ui/panel.slint:1515`
- **Evidence:** Spec §3.1 asks for Ctrl+I (eyedropper), Ctrl+S / Ctrl+Shift+S (save), Ctrl+C (copy in the default format) and "a printable character goes to the input field". panel.slint has no Colours handling for these keys. Line 1515 now just suppresses type-into-search while colours-open, without sending the character to the colour input. The tooltips at colours.slint:682/854/861 still advertise "(Ctrl+I)", "(Ctrl+S)" and "(Ctrl+Shift+S)", so they promise keys that do nothing. remaining/colours.md notes Ctr
- **Fix:** Add the key handling in the Colours view's FocusScope, or remove the shortcut text from the tooltips until it exists, and record Ctrl+C and typed characters in the remaining file.

## 26. [minor] Swatch and format-row menus are cut down: no Copy as ▸ formats, no Save to…, no Delete or menu key on swatches

- **At:** `crates/snipdeck-ui/ui/colours.slint:1066`
- **Evidence:** Spec §3.1 swatch menu: Make current · Copy as ▸ (6 formats + Name) · Save to Colours · Save to… · Remove. The branch has Make current · Copy (default format only) · Save to Colours · Remove (lines 1066-1071). The format-row menu (1092-1097) has no Save to…: `format_saved` ignores its list argument and the slint always passes -1. ChipGrid's key handler (239-256) has no Delete (remove from recents), no Menu/Shift+F10, and no Ctrl+S / Ctrl+Shift+S. Not in remaining/colours.md.
- **Fix:** Add Copy as (per format) and Save to… to both menus, and Delete plus the menu key to ChipGrid.

## 27. [minor] Typed text applies live only for strict literals; names, bare hex, hsb(), cmyk() and bare triples wait for Enter

- **At:** `crates/snipdeck-app/src/binding/colours.rs:369`
- **Evidence:** Spec §3.1: "typing applies live whenever the text parses" and "Ctrl+V of a valid colour applies at once". Spec §3.0a says the field takes a CSS name. `on_input_edited` calls `literal()`, the strict card rule that rejects bare names, hex without #, hsb(), cmyk() and "58, 123, 213", instead of `parse()`. So typing or pasting `royalblue`, `3A7BD5` or `hsb(215, 73%, 84%)` does not update the formats until Enter. The "other" field has the same problem (line 396).
- **Fix:** Use `parse()` for live apply, keeping the error message for Enter only.

## 28. [minor] Contrast row's accessible label leaves out AA large, AAA large and UI results

- **At:** `crates/snipdeck-ui/ui/colours.slint:80`
- **Evidence:** Spec §3.2: one label "On white: 4.22 to 1. AA text fail. AA large text pass. AAA text fail. AAA large text fail. Interface parts pass." In the branch, `contrast-speak(name, ratio, aa, aaa)` says only AA and AAA, and binding/colours.rs:95 passes only m.aa and m.aaa. Narrator users do not hear three of the five WCAG marks.
- **Fix:** Pass all five marks and speak each one.

## 29. [minor] Tints & shades are not laid out light to dark

- **At:** `crates/snipdeck-app/src/binding/colours.rs:201`
- **Evidence:** Spec §3 / §5: 9 swatches shown "4 tints, base, 4 shades", and "the UI lays them out light to dark". `scheme()` returns base, tint1..4 (getting lighter), then shade1..4 (scheme.rs:51-57). The binding puts `scheme_colours` into the chip grid as-is, so the order is base, slightly lighter … lightest, then slightly darker … darkest.
- **Fix:** For TintsShades, reorder to tints 4..1, base, shades 1..4 before building the chips.

## 30. [minor] Paste as for a colour item always pastes (no copy with Shift), and its Name row is labelled in untranslated English

- **At:** `crates/snipdeck-app/src/controller/panel.rs:381`
- **Evidence:** Spec D6a: "Enter pastes, Shift+Enter copies". `choose_item_menu` calls `use_colour(c, format, UseMode::Paste)` with no copy route. panel_select.rs:419 builds `ChooseLabel::Verbatim(format!("{}  {}", f.label(), ...))`, and `Format::Name.label()` returns English "Name" (format.rs:55). The spec says Name's label is @tr in Slint.
- **Fix:** Carry the Shift modifier into choose_item_menu for colour Paste as, and translate the Name label.

## 31. [minor] Wheel keyboard: Page Up/Down change saturation, not brightness; the surface's accessible value is the hex, not the channel values

- **At:** `crates/snipdeck-ui/ui/colours.slint:428`
- **Evidence:** Spec §3.0a: "Page Up/Down step brightness by 10", and Narrator should read "Saturation 73 percent, brightness 84 percent" (square) or "Hue 215 degrees, saturation 73 percent" (wheel). In the branch, PageUp sends `nudged(0, 10)`. In Wheel mode, colours_panel.rs:287 applies dy to saturation. At line 776 the surface's `value-text` is `ColourState.current-label` ("#hex, name").
- **Fix:** Route PageUp/PageDown to the brightness slider value in Wheel mode, and build the spec's spoken value text.

## 32. [minor] "View list" does nothing before the first save

- **At:** `crates/snipdeck-app/src/controller/colour_ctl.rs:346`
- **Evidence:** `open_saved_colours` returns no effects when `colour_save_list` is None. The tab's "View list" button (colours.slint:869-875) is always shown, and a click does nothing with no feedback.
- **Fix:** Disable or hide the button until a home list exists, or create the list (or show a toast) on click.

## 33. [minor] Eyedropper always hides and restores the Panel and always waits 150 ms; global hotkey not done

- **At:** `crates/snipdeck-app/src/controller/eyedrop_ctl.rs:80`
- **Evidence:** Spec §6 App side: return HidePanel only when the Panel is visible (settle 150 ms, else 0), and on Cancelled send ShowPanel only if it was visible. The branch always returns `[HidePanel]` with SETTLE, and Cancelled always returns `[ShowPanel, ShowColours]` (line 104). D10 / §4 `hotkeys.eyedropper` / `HotkeyAction::Eyedropper` is not implemented. remaining/colours.md says the hotkey was skipped, so "pick from anywhere" needs the Panel open first.
- **Fix:** Track Panel visibility at start, and add HotkeyAction::Eyedropper (off by default) per D10.

## 34. [minor] Contrast marks are in one non-wrapping row; footer hover text is missing

- **At:** `crates/snipdeck-ui/ui/colours.slint:572`
- **Evidence:** Spec §3: "marks wrap under 360 px". The five Mark items are in a single `HorizontalLayout` with no wrapping. Whether they clip at the 260-320 px Panel width is unverified: no running-app measurement. Spec §3.1 footer-left shows the hovered or selected row/swatch text. The footer at lines 1039-1052 shows only `use-hint`.
- **Fix:** Wrap the marks into two rows below 360 px and add the hover text to the footer. Measure at 260 px and at 175% text size.

## 35. [minor] No Slint goldens or measurements for the tab (noted in remaining)

- **At:** `crates/snipdeck-app/tests/colours_window.rs:73`
- **Evidence:** Spec §7 asks for `crates/snipdeck-ui/tests/colours.rs` goldens (280/320/420 px, dark mode, invalid input, tints, 175%, a swatch card) and pixel measurements. The branch has only `snipdeck-app/tests/colours_window.rs`, which writes PNGs to target/colours-shots and compares nothing. remaining/colours.md notes this as Next.
- **Fix:** Add the golden tests and measurements before claiming UI conformance.

## 36. [minor] Swatch grids show no focus ring when tabbed into

- **At:** `crates/snipdeck-ui/ui/colours.slint:271`
- **Evidence:** The ring is drawn only when `root.selected == i && scope.has-focus` (line 271). `recent-sel`, `saved-sel` and `scheme-sel` start at -1 (lines 641-643), so the first Tab into Recently used, Saved or a scheme shows no visible focus. Enter and Space also do nothing until an arrow key is pressed (line 247 requires `selected >= 0`). This fails WCAG 2.4.7 and the DESIGN.md 'visible 2px ring' rule.
- **Fix:** When the grid gains focus with `selected < 0`, set `selected = 0`, or draw a ring on chip 0 in that case.

## 37. [minor] The Picker disclosure is not in the tab order

- **At:** `crates/snipdeck-ui/ui/colours.slint:717`
- **Evidence:** The Picker header (lines 717-748) is a Rectangle with `accessible-role: button` and a TouchArea, but it has no FocusScope. Tab skips it, and Enter or Space cannot open or close the picker. Spec §3.1 puts the disclosure in the focus order. If a mouse user closes it, the closed state is remembered, and a keyboard-only user then has no way to reopen the square or wheel.
- **Fix:** Use a focusable control: a FocusScope with Enter/Space and a focus ring, or the shared disclosure or Button component.

## 38. [minor] ErrorLine copies FieldError but drops the live region and uses a glyph instead of an icon

- **At:** `crates/snipdeck-ui/ui/colours.slint:617`
- **Evidence:** ErrorLine (lines 617-631) draws the text "⚠ " + text with `accessible-role: text` and no `accessible-live-region`. The shared FieldError (text_field.slint:7-27) uses the Phosphor warning icon and `accessible-live-region: assertive`, and TextField also puts the error in the field's accessible-description (line 129). The input, Other colour and Values errors (lines 700, 824, 1000) are therefore not announced to Narrator, and the field is not marked invalid. DESIGN.md:165 also forbids emoji or glyp
- **Fix:** Remove ErrorLine. Pass the errors through TextField's `error` property (input-error, other-error). For the Values row use FieldError.

## 39. [minor] The hue, brightness and alpha sliders are 20px tall, under the 24px minimum

- **At:** `crates/snipdeck-ui/ui/colours.slint:325`
- **Evidence:** TrackSlider sets `height: Space.s5` (20px), and its TouchArea fills it. DESIGN.md:283 and spec §3.2 require hit targets of at least 24x24 (`Space.hit-min`, theme.slint:87).
- **Fix:** Use `height: Space.hit-min` or more, and keep the 12px painted track centred in it.

## 40. [minor] The square or wheel announces as a slider but has no slider semantics

- **At:** `crates/snipdeck-ui/ui/colours.slint:417`
- **Evidence:** Surface sets `accessible-role: slider` and `accessible-value: root.value-text`. At line 776 that value is `ColourState.current-label` ("#3a7bd5, near royalblue"). It sets no min, max or step and has no increment or decrement action, so Narrator's slider commands do nothing. Spec §3.2 asks for "Saturation 73 %, brightness 84 %" (square) or "Hue 215 degrees, saturation 73 percent" (wheel).
- **Fix:** Pass a value text that names both axes. Add accessible-action-increment and decrement that nudge the primary axis, or expose the surface as two sliders for assistive tech.

## 41. [minor] The spoken contrast result leaves out AA large, AAA large and UI

- **At:** `crates/snipdeck-ui/ui/colours.slint:80`
- **Evidence:** contrast-speak takes only `aa` and `aaa` (line 80), and binding/colours.rs:95 passes only `m.aa, m.aaa`. The row shows five pass/fail marks (lines 578-582) but Narrator reads two. Spec §3.2 gives the full sentence: "AA text fail. AA large text pass. AAA text fail. AAA large text fail. Interface parts pass."
- **Fix:** Add the aa-large, aaa-large and ui results to contrast-speak and its msgid.

## 42. [minor] Contrast marks never wrap, and fixed-width labels overflow at large text sizes

- **At:** `crates/snipdeck-ui/ui/colours.slint:572`
- **Evidence:** The five Marks sit in one HorizontalLayout (lines 572-583) with no wrap, although spec §3.2 says 'marks wrap'. At 100% in English they take about 272px of the 304px available, so more text scale or longer translations overflow the row. Other columns ignore text scale too: the format label column is a fixed `Space.s12 + Space.s1` (52px, line 909) with no elide, so semibold 'OKLCH' runs into the value at 150-175% (spec: 'the label column grows'). Several widths are raw px: 'default' 64px (line 931
- **Fix:** Split the marks into two rows (or a wrapping layout) under 360px. Size the label column from the widest label's preferred-width. Scale the fixed widths by SystemLook.text-scale or derive them from content.

## 43. [minor] The focus state on the square or wheel marker can vanish on blue areas

- **At:** `crates/snipdeck-ui/ui/colours.slint:474`
- **Evidence:** When focused, the marker draws a 2px `Theme.focus-ring` (#2870D4 in light theme) on top of the 1px black outer ring, at the same position. On blue parts of the square or wheel the ring blends into the surface. The white ring looks the same focused and unfocused, so the focus state is hard to see (WCAG 2.4.11). The TrackSlider thumb has the same issue on the blue part of the hue track (line 378).
- **Fix:** Draw the focus ring outside the black ring with a gap, for example a black, white and focus-ring stack, so it contrasts with any colour beneath.

## 44. [minor] Save to… shows only the first 8 lists

- **At:** `crates/snipdeck-ui/ui/colours.slint:1119`
- **Evidence:** Popup 3 hard-codes eight entries over `ColourState.list-names[0..7]` (lines 1120-1127). A user with more lists cannot save a colour to the ninth or later, although the spec says 'Save to… any list'.
- **Fix:** Build the entries from list-names in Rust (or use a repeater), and rely on the Menu's existing scroll cue.

## 45. [minor] The window tests skip keyboard, focus and accessibility

- **At:** `crates/snipdeck-app/tests/colours_window.rs:84`
- **Evidence:** All seven tests call ColourState callbacks directly (for example invoke_channel_committed and invoke_input_edited). None sends key events, checks Tab order, reads accessible roles or labels, or checks hit-box sizes, which spec §6 lists ('Tab order = §3.1; hit boxes ≥ 24 px'). This is why the keyboard gaps above went unnoticed. I did not run any tests, per the instruction not to run cargo.
- **Fix:** Add window tests that dispatch Tab, arrows, Enter and Shift+F10 through the window, and check the accessible tree and bounding boxes, following the audit_final1_tree.rs pattern.

## 46. [minor] The home Colours list is tracked only by an id in settings.json, so imports, settings loss or a downgrade lead to a second 'Colours' list

- **At:** `crates/snipdeck-app/src/controller/colour_ctl.rs:245`
- **Evidence:** home_list() and saved_colours() find the home list only through settings.colour_save_list. Import makes new ids by default (snipdeck-portable/src/import.rs:47,65). settings.json is not part of the library export. An older build drops unknown settings keys when it saves (from_json reads known fields only). In each case an imported or existing 'Colours' list full of colour items is ignored: the Saved strip is empty, and the next Ctrl+S/Save creates another list with the same name. List names have 
- **Fix:** When colour_save_list is None or stale, use an existing list on the board with the translated 'Colours' name before creating a new one, and test it.

## 47. [minor] The home list name and the eyedropper overlay texts keep the language the app started in

- **At:** `crates/snipdeck-app/src/binding/colours.rs:353`
- **Evidence:** wire_colours calls set_colours_list_name(...invoke_colours_list()) and set_eyedrop_texts(...) once, when the Panel is bound. For the same reason the Panel re-reads set_group_name on every sync (binding.rs:527-530, comment 'A new group's name follows the language the window is in now'). After a language change in Settings, the first save creates 'Colours' in the old language, and the overlay hint and HDR note stay in the old language.
- **Fix:** Refresh both in sync_colours or Core::sync, as set_group_name is.

## 48. [minor] Palette command 'Saved colours' from spec D6a is missing

- **At:** `crates/snipdeck-app/src/commands.rs:27`
- **Evidence:** Spec D6a: 'its Saved button (and palette command "Saved colours") switches the Panel to that list'. The diff adds only Command::Colours and Command::PickColour (ALL grows from 43 to 45). The confirmed scope-gap finding lists Ctrl+S/Ctrl+Shift+S/Ctrl+I, the card swatch and the Main Colour block, but not this command.
- **Fix:** Add Command::SavedColours (available when colour_save_list resolves) that runs open_saved_colours, once that path closes the tab. Add the texts to CommandTexts.all.

## 49. [minor] Each picker or slider pointer event runs a full settings update, a trigger re-read and a list query (not measured)

- **At:** `crates/snipdeck-app/src/controller/colour_ctl.rs:96`
- **Evidence:** set_colour() calls update_settings() on every surface, hue or brightness move. update_settings clones Settings, sanitizes, calls saver.changed, dock_resync and sync_recent_gate, then sync_typing(true), which calls store.triggers() (trigger_ctl.rs:150). The sync that follows calls colour_view(), which calls saved_colours() (store.items(home list)), and colour_save_targets(), on every redraw. The cost on a large library on Windows is not measured. CLAUDE.md requires a measurement for UI claims.
- **Fix:** Keep the current colour in the controller while dragging and write colour_current once on pointer-up (surface-moved already passes `done`). Cache saved_colours until library_changed.
