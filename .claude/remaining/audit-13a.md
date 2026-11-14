# Step 13a de-slop audit (Haiku scout, 2026-10-18 23:25; read-only, no cargo, no screenshots)

28 findings: shared components 14, main window 5, editors 4, panel 2, picker/fill 2, settings 1.

## Worst three
1. components/empty_state.slint:25-26 swaps glyphs (list=stack, board=squares-four). DESIGN §4: list=rows, board=stack, squares-four only for card view.
2. main_window.slint has no dots-three "More" entry point for the item menu (menu only via right-click/keys at 1039, 1106, 1176). DESIGN §4 ItemCard: "⋯ in the main window". Add a ⋯ IconButton, tooltip "More", on hover/focus.
3. Duplicated components + literal sizes: HotkeyChip (hotkey_recorder.slint:9) clones Keycap; PinnedChip (pinned-strip.slint:11) clones Tag; NoColour (editors.slint:559) and its 22/20px cell (editors.slint:582) clone Swatch; StepButton (form.slint:433) clones ghost IconButton. Literal 440px at editors.slint:901/951/1001 and template_fill.slint:51; 220/100px at main_window.slint:312-313; disabled opacity 0.45 repeated 18x. DESIGN §7, §9.1: tokens only.

## Rest
4. Icons not in the table: list-bullets (main_window.slint:877 compact view), keyboard (hotkey_recorder.slint:174), calendar-blank (template_field.slint:137); `x` removes an app at settings.slint:331 (x is never delete).
5. Labels: "Delete" with trash at main_window.slint:448, 1319 and editors.slint:524 -> "Move to Trash". "Empty Trash…" secondary at main_window.slint:934 -> danger.
6. Untranslated English: key names hotkey_recorder.slint:39-52, modifiers 279-280; keycap labels picker.slint:476/481/486, panel.slint:766; accessible-label joins item-card.slint:278, template_field.slint:47.
7. Derived colours (tokens-only rule): button.slint:33, state-color.slint:8/13, tag.slint:27, feedback.slint:69, dialog.slint:30 -> add tokens via palette.py.
8. Off-scale glyph sizes 10/12/14px: choice.slint:71/175, tag.slint:52/80, editors.slint:591, swatch.slint:56; literal 56x36 thumb item-card.slint:181-182. DESIGN §4: 16px rows, 20px toolbars (small controls may be intentional: check).
9. Menu highlight fill-only, no 2px ring (menu.slint:39). 13 generated icons unused (icons.slint:6,14,18,20-22,24,35,41,42,45,49,52) -> scripts/ui_icons.py.
10. Goldens: 116 files in crates/snipdeck-ui/tests/golden/; regenerate empty, main-compact, menu, list-editor, settings-recording, panel-pinned-overflow after 1-3.

Verified clean: no hex/rgb literals outside theme.slint; no placeholder copy; every IconButton has a tooltip; no hover row of icon buttons; primary buttons ink; accent only on selection, focus, toggles. Onboarding screen absent (step 7a).
