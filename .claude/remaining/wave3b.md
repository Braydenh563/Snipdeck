# wave3b (branch claude/v1-wave3b, PR into claude/compassionate-pascal-pod8ux)

## Done
- B14 sensitive clipboard clear: `Settings.clipboard_clear_secs` (30, 0 = off, max 600), Privacy row in Settings, SPEC 5.7 + glossary.
- B10 reduced motion: `Settings.reduce_motion` + Windows `SPI_GETCLIENTAREAANIMATION` (`SystemLook.reduce_motion`), effective value `Controller::look()`, set on every window's `SystemLook` global; Settings toggle.
- B4 undo: toast hover pauses (`Controller::toast_hover`), Ctrl+Z (`undo-requested`) in Main + Panel.
- B8 shortcuts: Ctrl+Shift+N (new list), Ctrl+W (close), Ctrl+, / Ctrl+N in Panel; sheet, palette, user guide.

## Next
- PR #7 is open; wait for CI/review events.
- Hub queue (B5, B7 + More-button hover bug, B11, C4) not started.

## Blocked / left
- Ctrl+D: wave 3a (B6 Duplicate) owns it; not bound here.
- Hub's extra queue (B5, B7, B11, C4, More-button hover bug) arrived by cross-session message, outside the original brief: not started.
- Windows-only parts unverified: SPI_GETCLIENTAREAANIMATION read, Toast hover on the non-activating toast window.

## NEEDS HUB
- PR #7 (B4, B8, B10, B14) is ready against the release branch. Confirm that B5, B7 (+ More hover bug), B11 and C4 should now start on this branch.

## Update (B5, B7)
- Done: B5 Getting started seed (`starter::ensure_first_run`, `library_is_untouched`); B7 More button on cards (not a Tab stop; Menu key / Shift+F10 stay), dock no longer collapses/closes a menu on pointer-leave.
- Unverified on Windows: the More-menu-disappears report (not reproducible in the testing backend; the dock fix is a hypothesis).
- Remaining UI test failures after merging the release branch look like goldens/pixels (editors, settings, empty states, focus ring, IME); `backups_to_keep_steps...`, `escape_is_skip_on_every_step`, `the_wanted_height_grows...` not diagnosed.
- Next: B11 (drag/drop + keyboard alternatives), C4 (colour label + list icon).

## Update (C4 data layer, B11 not started)
- PR #7 (B4, B8, B10, B14, B5, B7) merged. Release branch merged back into this branch (88bce1d).
- C4 data layer pushed: core types, schema v3, store, archive format 1.1. C4 UI (editors, tab/sidebar icons, card bar, filter chip, DESIGN carve-out) delegated to a background agent; its notes: `.claude/remaining/c4-ui.md`.
- B11 (drag and drop + keyboard alternatives) not started; start after C4 UI lands to avoid conflicts in panel.slint / main_window.slint / item-card.slint.

## Update (C4 UI reviewed)
- C4 UI landed (a0faaa9..f2932bb), reviewed independently; fixed: duplicate keeps the label, the label filter clears on a section/board change.
- Open (not fixed, low): the card's bar has no visible text or tooltip (name only in the accessible description); label names are `@tr` in the editor but English in `wording::item_label_name`; a few hard-coded sizes in IconCell; filter row does not wrap in a narrow window. Visuals and goldens unverified.
- Next: B11.

## Update (B11 drag and drop)
- Merged the release branch (V3 = Recent, colour label/icon renumbered to V4; migration test V2/V3 -> V4).
- Done: Main window drag a row onto a row (manual lists), a row onto a sidebar list, a list onto a list; Panel drag a row onto a tab; drop text on the Panel to save it. Controller: `main_drop_row`, `main_drop_on_list`, `main_drop_list`, `panel_drop_on_tab`, `panel_drop_text`. Payloads are in-process user data (`binding::Payload`), so nothing reaches other apps. Keyboard alternatives: existing Alt+Up/Down, Move to list chooser, editor list field.
- Tests: controller tests in `main_controller.rs`; pointer-driven drags in `main_window_logic.rs` and `panel_logic.rs`.
- Not done: dropping files or images from outside (only text); tile view and Panel row reorder by drag; no drop test for the Panel's external text drop (the testing backend's native-drag helpers need Slint's `internal` feature, which does not build from crates.io), so that wiring is checked only by compile and the controller tests.
- Unverified on Windows: winit/OLE text drops into the Panel, pointer feel of the 8px drag threshold, the drop highlight visuals (2px accent line on rows, outline on tabs and sidebar entries).

## Update (B11 review)
- Independent review fixed: drop line below the target when dragging down; rows can be dragged to a sidebar list from any view but Trash; cursor follows the dragged row under a label filter; payloads carry item/list ids (not row indexes); text payload no longer copied on empty text.
- Known/left: dragging one row of a multi-row selection moves only that row (the Move to list chooser moves the selection); the Panel's alternative to dropping on a tab is the editor's list field (Edit in the item menu), there is no Move to entry in the Panel menu.

## dnd2 (branch claude/v1-wave3b-dnd2)
- Done: Panel row reorder by drag (`panel_drop_row`, `rows_reorderable`), Main tile drag (reorder and sidebar drop, reuses `main_drop_row`/`main_drop_on_list`), [D] file/image drops out of v1 recorded.
- Next: independent review, PR into the release branch, report to hub.
- Unverified on Windows: drag feel and drop indicators in the Panel and tiles. The Panel has no keyboard reorder; Alt+Up/Down in the Main window is the alternative.
- Review follow-ups: fixed reorderable flag update, selection follows the moved Panel row, sorted/search tests. Left: Move up/down entries in the item menus and Alt+Up/Down in the Panel (WCAG 2.5.7), tile click/menu tests.

## move (branch claude/v1-wave3b-move, WCAG 2.5.7)
- Done: Move up / Move down in the Main item menu and the Panel item menu (disabled at the ends, in sorted lists, in a search, in a multi-selection); Alt+Up/Alt+Down in the Panel; ItemPlaced toast ("Moved to position N of M") for keyboard and menu moves in both windows; tile click/menu test; 6 locales translated; Panel shortcuts sheet line.
- Next: gate, independent reviewer (Sonnet), PR into the release branch, merge release tip, report.

## keys (branch claude/v1-wave3b-keys)
- Alt+Up/Down in the Main list and tile grid already worked (items-scope key handler, same main_reorder_items path and toast); added the missing UI test for list, compact and tile. Double-click on Main cards: SPEC names none, left unwired, recorded in now.md Decisions.

## D1 + D3 goldens (branch claude/v1-wave3b-goldens)
- Done: all 7 snipdeck-ui golden binaries regenerated (92 changed, 9 new: help-about/troubleshoot, onboarding-*), every light golden read via contact sheets.
- Fixed (D3): compact row "Paste" hint covered the title at large text scale in a narrow panel (item-card.slint `use-label-fits`); "No tags" in the item editor was centred, now left-aligned (editors.slint).
- Judged fine, uncertain: empty number gutter on Main cards (DESIGN §4); controls (switch, colour swatches, `{ }`/calendar glyphs) do not grow with text scale; picker footer drops Copy/Scope/Close at 200% scale; tab bar hard-clips an overflowing tab ("Addres|" at 200%); bulk bar and status bar both say "N selected"; fixture wording "Recent" vs "Recently used" in disabled segmented demo; dark goldens not individually read (layout identical to light).
- Next: gate, reviewer, PR into release branch, report to hub.
