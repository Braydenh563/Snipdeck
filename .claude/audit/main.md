# Deep audit: Main window (agent `main`)

Head audited: `origin/claude/compassionate-pascal-pod8ux` at `2400146c` (detached worktree). Read-only: nothing
committed; the one probe test (`crates/snipdeck-ui/tests/probe_main.rs`) ran once under the shared lock
(6 tests, 604 s, exit 0) and is deleted; `git status --short` is empty.

Short paths used below: `mw` = `crates/snipdeck-ui/ui/main_window.slint`, `ui/` = `crates/snipdeck-ui/ui/`,
`app/` = `crates/snipdeck-app/src/`.

Counts: 47 files in the map: 5 driven, 16 checked, 26 not reached. 35 interaction rows: 24 driven, 8 checked,
3 not reached. 62 findings: 4 high, 19 medium, 39 low (21 verified by a run, 41 by reading code).

Probe method (all `run` rows): `snipdeck_ui::testing::Harness` (headless software renderer), the
accessibility tree (`ElementHandle`: role, label, box), the focus item, rendered pixels for contrast, and
`std::time::Instant` around renders for timings (debug build: the numbers compare cases, they are not
release timings). The raw log is
`/tmp/claude-0/-home-user-Snipdeck/924dfb27-0b1b-5011-b96d-4161f926cf4e/scratchpad/deep-audit/probe_main.log`
(1,199 lines; `P|` problem, `I|` info, `C|` contrast, `T|` timing).

## 1. Coverage

### 1a. Files (the 47 `main` rows of the map)

| File | Status | How | Note |
|---|---|---|---|
| ui/main_window.slint | driven | run | Read in full (lines 272-3066); probe walked 4 sizes x 3 text scales x 16 scenes (5,443 interactive boxes), overlays, Tab order, keys, contrast in 3 themes, timings. |
| ui/onboarding.slint | driven | run | Steps 0-3 at 560x520 and 720x620 x 3 scales; strings and Esc handling read. |
| ui/editors.slint | driven | run | List editor, board editor and Tag manager rendered at 2 sizes x 3 scales; item editor read by outline only (strings, keys, sizes); its fit and Tab walk are in the existing `audit_final1*.rs`. |
| crates/snipdeck-ui/tests/support/main_fixture.rs | driven | run | Used by the probe (`populate`, `set_rows`, `sample_rows`, `numbered_rows`, `lists`, `tags`). |
| crates/snipdeck-ui/tests/support/editor_fixture.rs | driven | run | Used by the probe (`onboarding`, `list_editor_existing`, `tag_manager`). Does not set `icon-names`, so the icon radios read unnamed in the fixture only (the host sets them, app/main_binding.rs:1523). |
| app/controller/main_ctl.rs | checked | code | Read without comments. |
| app/controller/main_ctl/editors.rs | checked | code | Read; parked drafts re-read (lines 170-345). |
| app/controller/main_ctl/items.rs | checked | code | Read; actions and tag toggle re-read. |
| app/controller/main_ctl/lists.rs | checked | code | Read; export and reorder re-read. |
| app/controller/onboarding_ctl.rs | checked | code | Read in full (65 lines). |
| app/controller/toast_ctl.rs | checked | code | `show_toast`, `toast_tick`, `toast_hover` read; the rest by signature. |
| app/controller/settings_ctl.rs | checked | code | Lines 1-330 read (import sources, erase, export path, backup now, restore). The import and restore paths are another branch's scope and were not judged. |
| app/main_binding.rs | checked | code | `render` 1163-1368, `sync` 250-270, `main_row` 1433-1491, trace lines. |
| app/main_window.rs | checked | code | `reload`, `entries`, `rows`, search, `unfiltered_items`, `SEARCH_LIMIT`. |
| app/onboarding_binding.rs | checked | code | Window creation, sizing, skip and close wiring (lines 283-378, 425-475). |
| app/text_scale.rs | checked | code | Read in full. |
| ui/settings.slint | checked | code | Every control, callback and `@tr` string read (lines 414-1807). Not driven by this probe: fit, names, 24px targets and Tab walks at 1.0 and 2.25 are asserted by `ux_fit.rs` and `audit_final1*.rs`. |
| ui/help.slint | checked | code | Strings, sections, dialog read by outline. Same existing audits drive it. |
| ui/tip_texts.slint | checked | code | All 11 tips read against app/tips.rs. |
| ui/toast_window.slint | checked | code | Read in full (51 lines). |
| ui/menu_texts.slint | checked | code | First 67 lines read (tray and item menu names). |
| app/settings.rs | not reached | - | 2,557 lines; time. Only the text-size mapping was read through text_scale.rs. |
| app/settings_binding.rs | not reached | - | 2,541 lines; only its trace lines and `follow-windows-text` use were grepped. |
| app/help_binding.rs | not reached | - | Only the Tips card builder (lines 344-372) was read. |
| app/toast.rs | not reached | - | Only function names and test names listed. |
| app/toast_binding.rs | not reached | - | Time. |
| app/controller/tests/look_settings.rs | not reached | - | 16 tests counted; no real-clock call (grep). |
| app/controller/tests/onboarding.rs | not reached | - | 7 tests counted; no real-clock call. |
| app/controller/tests/toasts.rs | not reached | - | 40 tests counted; no real-clock call. |
| crates/snipdeck-app/tests/editor_window.rs | not reached | - | 35 tests counted; no real-clock call. |
| crates/snipdeck-app/tests/help_window.rs | not reached | - | 8 tests counted; no real-clock call. |
| crates/snipdeck-app/tests/main_controller.rs | not reached | - | 102 tests counted; no real-clock call. |
| crates/snipdeck-app/tests/main_window.rs | not reached | - | 51 tests counted; no real-clock call. |
| crates/snipdeck-app/tests/settings_window.rs | not reached | - | 78 tests counted; no real-clock call. |
| crates/snipdeck-ui/tests/editors.rs | not reached | - | 33 tests counted. |
| crates/snipdeck-ui/tests/editors_logic.rs | not reached | - | 1 test, 2,064 lines. |
| crates/snipdeck-ui/tests/group_header_measure.rs | not reached | - | 5 tests counted. |
| crates/snipdeck-ui/tests/main_window.rs | not reached | - | 28 tests counted; 4 uses of `text_scale`. |
| crates/snipdeck-ui/tests/main_window_logic.rs | not reached | - | 1 test, 2,888 lines; 1 use of `text_scale`. |
| crates/snipdeck-ui/tests/tray_texts.rs | not reached | - | 1 test. |
| ui/form_texts.slint | not reached | - | Time. |
| ui/import_texts.slint | not reached | - | Import is another branch's scope. |
| ui/icons.slint | not reached | - | Time. |
| ui/list-icons.slint | not reached | - | Time. |
| ui/shortcut_texts.slint | not reached | - | Only checked that Main's menus bypass it (MAIN-19). |
| ui/toast_texts.slint | not reached | - | Time. |
| ui/tray_texts.slint | not reached | - | Time. |

### 1b. Interaction walk

Sizes driven: 480x360 (minimum), 760x520, 1100x720 (default), 1920x1080. Text scales 1.0, 1.5, 2.25.

| Surface or act | Status | How | Note |
|---|---|---|---|
| Sidebar expanded: sections, lists, tags, counts | driven | run | Boxes at every size and scale; 50 lists + 200 tags: End, Up, Home, 40 x Down keep the cursor row in view at 1.0 and 2.25 (tag199 at y 646 of 720; List 36 at y 646). |
| Sidebar rail | driven | run | Same key walk on the rail; MAIN-16, MAIN-48, MAIN-52. |
| Splitter | driven | run | Box 8x720; Tab stop 20 of 21; Left/Right read in code. MAIN-15, MAIN-58. |
| Sidebar fold (Ctrl+B) and auto rail under 720px | driven | run | `rail=true` at 480 wide, false from 760. The missing expand control under 720px is on fix/sidebar-expand and not re-reported. |
| Board button, board menu, board delete dialog | driven | run | Menu and dialog opened at 3 scales; MAIN-7, MAIN-12, MAIN-44, MAIN-56. |
| Toolbar, every tier and width | driven | run | 12 size x scale cases; MAIN-48, MAIN-50, MAIN-51. |
| Search box: type, Down, Enter, Esc | driven | run | MAIN-1 to MAIN-4, MAIN-50. |
| Filters (colour-label chips) | driven | run | Chips scene at every size: no box outside the window. Filter logic read; MAIN-28. |
| Sort button and menu | driven | run | MAIN-18, MAIN-26, MAIN-51. |
| Views: comfortable, compact, tile | driven | run | Columns measured (4 at 1100x720@1, 2 at 1.5, 1 at 2.25; 6 at 1920x1080@1); MAIN-10, MAIN-13, MAIN-14. |
| Groups (headers, fold) | checked | code | Fixture rows with groups rendered; fold and group edit not driven. Group logic is in library.md. |
| Multi-select and bulk bar | driven | run | MAIN-5, MAIN-53. |
| Drag reorder and its keyboard alternative | checked | code | Pointer drag not driven (the harness has no drag helper in this probe); Alt+Up/Down read in code: MAIN-11, MAIN-12, MAIN-13. |
| Item context menu (mouse and Menu key) | driven | run | Keyboard-opened menu measured; MAIN-6, MAIN-19. |
| Move / Tag / Paste-as chooser | driven | run | MAIN-5, MAIN-23. |
| Quick add | driven | run | Ctrl+N focus measured; MAIN-9, MAIN-51. |
| Trash (rows, Empty Trash, restore, delete for good) | driven | run | Layout at every size; confirms read in code: MAIN-8, MAIN-51. |
| Recent section and its states (on, off, locked) | driven | run | Empty states rendered; menu logic read. |
| Empty states (all, list, pinned, tag, recent, trash, search) | driven | run | Six rendered at every size; MAIN-27, MAIN-28, MAIN-54. |
| Loading state | not reached | - | Never set by the host (EXTRAS-9, not re-reported). |
| Error state | checked | code | Errors arrive as danger toasts (toast_ctl.rs); no in-window error surface exists. MAIN-43. |
| Locked state | checked | code | Recent "locked" empty state rendered; master-password unlock flow not driven. |
| Toasts in Main | driven | run | Undo, Edit, Dismiss boxes at every size (inside the window everywhere); timing read in code: MAIN-43. |
| View menu, Help menu | driven | run | Opened and closed with Esc at 3 scales; MAIN-17, MAIN-18, MAIN-56. |
| Confirm dialogs | driven | run | Focus measured; MAIN-7, MAIN-8, MAIN-44, MAIN-57. |
| Keyboard only: Tab order | driven | run | 1100x720@1 (21 stops), 480x360@1 (14), 480x360@2.25 (6): MAIN-49, MAIN-56, MAIN-57. |
| Light, dark, high contrast | driven | run | 320 contrast samples from pixels at 1100x720; MAIN-59, MAIN-60. |
| Item editor window | checked | code | Outline only here; driven by existing audits. MAIN-39 to MAIN-42. |
| List editor, board editor, Tag manager | driven | run | Rendered at 2 sizes x 3 scales; no problem above the windows' minimum width. |
| Settings window (one scrolling page, 9 sections) | checked | code | Every control read; MAIN-33 to MAIN-38. |
| Help window (Troubleshoot, About, Shortcuts, Tips) | checked | code | MAIN-30 to MAIN-32, MAIN-37, MAIN-46. |
| Onboarding / first run / tour | driven | run | MAIN-30, MAIN-38, MAIN-47, MAIN-55. |
| Toast window (outside Main) | checked | code | ui/toast_window.slint read; width cap scales with text there. |
| Touch | not reached | - | Not driven. |
| Windows-only behaviour (native menus, caption inset, Narrator, real high contrast) | not reached | - | See section 3. |

## 2. Findings

| Id | Severity | Lens | Evidence | Verified | Class-level fix | Effort |
|---|---|---|---|---|---|---|
| MAIN-1 | medium | Copy | mw:2729-2731 status "{n} matches in {board-name}", but a search covers every board (app/main_window.rs:716-723 takes hits over all items; test `searching_covers_every_board_and_ends_when_a_section_opens` :1414). The line names the wrong scope, and a hit from another board shows only a list name (ListLabel.board is always empty, main_window.rs:414). | code | Word the count by the real scope ("in all boards") or scope the search to the board; show the board name on hits from other boards. | S |
| MAIN-2 | medium | Missing | app/main_window.rs:27 `SEARCH_LIMIT = 200`: a search stops at 200 hits and the status line says "200 matches" with no sign that more exist. | code | Say "first 200" (or page the rest); one rule for every capped list. | S |
| MAIN-3 | medium | UX | mw:1537-1547 the Main search field handles only `edited`; there is no Enter or Down handler (Down is taken only while the suggestion drop-down is open, mw:1136). After typing a query the keyboard user must Tab to reach the results. Raycast, Alfred and TextExpander all move into the results with Down and act with Enter. | run (probe c_keyboard, 1100x720@1: after typing "re", Down left search-focused=true, cursor=0; Enter made no callback, log=[]) | Down moves the cursor into the results, Enter uses the first hit. | S |
| MAIN-4 | low | UX | mw:1838-2000 items scope: Esc with a query does not clear the search; typing a letter in the list does not start a search (only "/" and Ctrl+F, mw:1158, 1188). ui/tip_texts.slint:21 tells the user "Start typing in the Panel or Main to filter every list at once". | run (probe c_keyboard: Esc in the list with query "re" left query="re"; typing "x" in the list left query="re", search-focused=false) | Type-to-search in the list scope (or fix the tip); Esc clears the query first. | S |
| MAIN-5 | medium | Broken | mw:872-882 `open-chooser` places the Move / Tag / Paste-as chooser at `item-menu-x/y`, which the bulk bar's Move and Tag buttons never set (stale or 0,0), and clamps with unscaled `root.width - 240px`, `root.height - 300px` while the chooser is `240px * text-scale` wide (mw:2974): 540px at 2.25, wider than the 480px minimum window. | run (probe b_overlays: the bulk bar "Move" button at (676,654) on 1100x720@1 opened the chooser at (232,0), 654px above it; on 480x360@2.25 the chooser box is x 108..648 of 480, 168px outside the window) | One placement helper for every popup: anchor to the invoking control, clamp with the popup's real size. | S |
| MAIN-6 | low | UX | mw:1297, 1867, 1984: a context menu opened with the Menu key or Shift+F10 appears at a fixed spot under the toolbar, not at the cursor row; on a long list it is far from the row it acts on. | run (probe b_overlays, 1100x720@1: cursor on the sixth row, menu "Item actions" opened at (264,120); at 1.5 at (312,132); the y never follows the row) | Open at the cursor row's rectangle. | S |
| MAIN-7 | low | Copy | mw:809 "The list and its {} items move to the Trash", mw:812 "All {} recent copies are removed", mw:3025 "Its lists and {} items move to the Trash": no plural form, so "its 1 items" and "its 0 items". The list editor's own delete dialog (ui/editors.slint:2007, 2065) has plural forms and different words ("will move to Trash with it"). | code | One plural-aware string per confirmation, shared by both entry points. | S |
| MAIN-8 | low | Copy | mw:813 "The selected items are deleted for good" is also the body when one item is deleted from the Trash. | code | Plural form by count. | S |
| MAIN-9 | low | UX | mw:1720 the toolbar "New item" button calls `new-item-requested()` (full editor) while Ctrl+N calls `new-item()` (quick-add row in an open list, mw:1163): the same command behaves in two ways. The button also shows in the Trash. | run (probe c_keyboard: Ctrl+N in a list focused TextInput "New item" (252,80 828x76) with no callback; the button fired `new-item-requested`; in the Trash the "New item" button is still in the tree) | One command, one behaviour; hide or disable it in the Trash. | S |
| MAIN-10 | low | UX | mw:1694-1706 Tile view is disabled while the list has groups, with no reason given (tooltip stays "Tile view"), and then none of the three view toggles is shown as active (`accessible-checked` false on all three when the stored view is Tile). | code | Tooltip states why; mark the view that is really drawn. | S |
| MAIN-11 | medium | Accessibility | app/controller/main_ctl/lists.rs `main_reorder_items`: with more than one row selected Alt+Up/Down returns silently (`selected_count() > 1 => return`), while a drag moves the whole selection. No keyboard alternative for a multi-row reorder (WCAG 2.5.7 Dragging Movements, 2.1.1 Keyboard) and no feedback that the key was ignored. | code | Alt+Up/Down moves the selection as a block, or a toast says why not. | M |
| MAIN-12 | low | Accessibility | List reorder (Alt+Up/Down and drag in the sidebar) and board Move up / Move down give no toast or announcement, while an item reorder calls `announce_placed` (app/controller/toast_ctl.rs:61). The board menu closes after each Move up/down, so moving a board three places takes three menu openings. | code | Announce every reorder the same way; keep the menu open or give boards Alt+Up/Down. | S |
| MAIN-13 | low | UX | Tile view: Alt+Up/Down moves an item one step in reading order (left or right on screen); there is no Alt+Left/Right and no move by one row. | code | In tiles map Alt+Left/Right to one step and Alt+Up/Down to one row. | S |
| MAIN-14 | low | Performance | mw:2232 every tile builds a hidden `tile-probe` MainCard as well as the real card, so the tile view instantiates each visible item twice. | run (probe e_perf, debug build, software renderer, 1100x720; 1,000 / 10,000 rows: list first render 247 / 252 ms, Down 165 / 155 ms, one-row change 186 / 166 ms, model replace 214 / 219 ms, sidebar resize 194 / 203 ms; Tile first 385 / 464 ms, Down 236 / 240 ms, one-row change 340 / 348 ms, model replace 433 / 452 ms, sidebar resize 396 / 393 ms: Tile costs 1.5 to 2 times the list for the same rows; the row count does not matter, so both views are virtualised) | Measure with one shared probe or a computed height. | M |
| MAIN-15 | low | Missing | Sidebar: no drop target on the Trash or on a tag (drag an item to delete or tag it); F2 and Delete on a list, board or tag do nothing (context menu only, mw:1266-1302); the splitter has no Home/End and no double-click reset (mw:2665-2722: Left/Right 8px only). | code | Add the standard keys and drop targets. | M |
| MAIN-16 | medium | UX | Rail (collapsed sidebar): the "Add list" and "Manage tags" header buttons are not drawn; only Ctrl+Shift+N, a context menu or the palette reach them. A mouse user on a narrow window has no visible way to add a list. | run (probe c_keyboard Tab walk: 21 stops at 1100x720@1 with "Add list" (198,170 24x24) and "Manage tags" (198,290 24x24); 14 stops at 480x360@1 with neither) | Keep an Add control on the rail. | S |
| MAIN-17 | low | Copy | mw:2883-2889 Help menu: "Show Panel" never becomes "Hide Panel" although it toggles (the toolbar tooltip does, mw:1561-1563); it sits between "About Snipdeck" and "Keyboard shortcuts"; "Troubleshoot…" comes first, above "User guide"; there is no "Tips" entry though Help has a Tips tab (ui/help.slint:140). | code | Label by state; order by use; separate the Panel command. | S |
| MAIN-18 | medium | Accessibility | mw:2917-2926 (View menu) and mw:2955-2959 (Sort menu) mark the chosen entry with `icon: Icons.check` and never set `MenuEntry.checked`, so `accessible-checked` / `accessible-checkable` (ui/components/menu.slint:63-64) stay false: a screen reader cannot tell which view or sort is on (WCAG 4.1.2). | code | Use `checked:` for every chosen-row menu; a test that no menu passes `Icons.check` as an icon. | S |
| MAIN-19 | low | Copy | Shortcut labels in Main menus are literal English and bypass ui/shortcut_texts.slint: mw:953-973 "Enter", "Shift+Enter", "Ctrl+E", "Del", "Alt+Up"; mw:2885-2888 "F1", "Ctrl+/"; toolbar and empty-state chips ["Ctrl","K"], ["Ctrl","N"] (mw:2483-2486). German "Strg" and French "Suppr" never show here. | code | Route every key name through one translated formatter. | S |
| MAIN-20 | low | UX | The Main window title is always "Snipdeck" (no board or section), so the taskbar, Alt+Tab and a screen reader's window announcement carry no place. | code | "Section - Board - Snipdeck". | S |
| MAIN-21 | medium | Missing | app/main_binding.rs:1484 `image: slint::Image::default()` for every row: an image item never shows a thumbnail in Main although `MainRow.image` exists and the card can draw one. | code | Decode a bounded thumbnail per visible image row, cached by item id. | M |
| MAIN-22 | high | Missing | docs/SPEC.md 5.3 and DESIGN.md:307-312 specify a three-pane Main window with a Detail pane (preview, metadata: formats, use count, created and updated, source app). It is not built and not on the post-1.0 list. An item's full content, size, use count and dates can be seen nowhere without opening the editor; Raycast, Alfred and TextExpander all show a preview beside the list. | code | Build the Detail pane or cut it from SPEC and DESIGN with a recorded decision. | L |
| MAIN-23 | medium | Missing | A list cannot be moved to another board: `store.move_list` takes a board but the UI always passes the current one; the list editor has name, colour, icon and sort only (ui/editors.slint:1850-1886); the "Move to list" chooser offers only the current board's lists (mw:2976). | code | Board field in the list editor; group the Move chooser by board. | M |
| MAIN-24 | low | UX | app/controller/main_ctl/items.rs:335-338 (tag toggle) and `main_delete_tag` end with `finish(&done, None)`: tagging 50 selected rows or deleting a tag from Main shows no toast. | code | A toast with the count for every bulk change. | S |
| MAIN-25 | medium | UX | app/controller/main_ctl/lists.rs:54-63 "Export as text" leaves out sensitive items and items with no text (images, files) without saying so; the toast only names the file. | code | "Exported 12 of 15 items (3 sensitive left out)". | S |
| MAIN-26 | low | Missing | Sort offers Manual, Most used, Recently used only (mw:1114, 2955-2959; ui/editors.slint:1886): no A-Z, no date created or changed. Alfred and TextExpander sort by name; Raycast by last change. | code | Add name and date sorts. | M |
| MAIN-27 | low | Copy | Two nouns for the same thing: "snippet" (mw:2470-2482 "No snippets yet", "Add a snippet", "Tag a snippet"; mw:1539 "Search snippets"; mw:2018 list label "Snippets"; ui/components/quick-add.slint:150) and "item" ("New item", "{n} items", "move items here", "Edit item"). | code | One noun in the glossary and a test that the other does not appear. | S |
| MAIN-28 | low | UX | mw:2469-2482 the "No matches" empty state has no action (no "Clear search" button); and with a colour-label filter left on after its last item is gone the list branch reads "This list is empty / Add a snippet" although the list has items (the empty state does not look at the filter). | code | Empty state by cause: search, filter, truly empty; each with its way out. | S |
| MAIN-29 | low | Performance | app/controller/main_ctl.rs `main_screen()` runs on every `sync`: it rebuilds a `Vec<Row>` for all rows with cloned strings, and when any row differs `main_row` converts every row again (colour parsing per row, app/main_binding.rs:1433-1491); `keys()` / `entries()` are recomputed several times per action; `group_of` scans every group's items per item in `entries()` and `rows()`. Not timed (the lock allowed one probe target, in the UI crate). | code | Cache entries per reload; index group membership in a map; convert only changed rows. | M |
| MAIN-30 | low | Copy | ui/onboarding.slint:422 "Ten short tips" but `tips::TIPS` has 11 (app/tips.rs:70; ui/tip_texts.slint:7-17). | code | Drop the number or derive it. | S |
| MAIN-31 | medium | Copy | ui/tip_texts.slint:42 "A master password locks the whole deck." Settings says it protects sensitive items only (ui/settings.slint:1212-1214). The tip overstates the protection. | code | "A master password locks your sensitive items." | S |
| MAIN-32 | low | Copy | ui/tip_texts.slint:1-4: the tip pictures are crops of English renders, so the five other languages show pictures with English words. | code | Render the crops per language or use pictures with no words. | M |
| MAIN-33 | low | Copy | "panel" and "Panel" (also "picker" and "Picker") are mixed inside Settings and the welcome tour: ui/settings.slint:465-474, 747, 915-916, 940, 952 lower case; 773, 864, 886-887, 902, 927-928, 939, 1028 capital; ui/onboarding.slint:278, 309-314, 368 lower case. | code | One spelling per product noun, checked by a test. | S |
| MAIN-34 | low | Copy | ui/settings.slint:665 the Theme help says "Follow Windows switches with your system setting." but the option is labelled "Windows" (:669); :711 "100 %" has a space before the per cent sign. | code | Name the option in the help exactly as on the control. | S |
| MAIN-35 | low | UX | "Panel width" sits under Appearance (ui/settings.slint:746) although there is a "Panel layout" section; Text size stops at 175 % (:711) while Windows goes to 225 %. | code | Move the row; offer 200 % and 225 %. | S |
| MAIN-36 | low | UX | Two confirm patterns side by side in Privacy: "Clear recent history" then "Cancel" / "Clear history" (ui/settings.slint:1102-1120), and "Clear recent emoji and colours…" then "Keep them" / "Clear them" (:1138-1151); one label has an ellipsis, the other does not. | code | One inline-confirm component with one wording rule. | S |
| MAIN-37 | low | Copy | ui/settings.slint:1594 "{} item in {} list will be added" / "{} items in {} lists will be added" picks its form from the item count only: "5 items in 1 lists". ui/help.slint:514 "holds {} items on {} boards" has no plural form. | code | Two plural strings joined. | S |
| MAIN-38 | low | UX | ui/settings.slint:550-557 Esc closes the Settings window even while the Erase form or the master password form is open with typed text (only the import steps are handled). In the welcome tour Esc on step 1 is Skip, which sets `onboarded` for good (ui/onboarding.slint:248-254; app/controller/onboarding_ctl.rs finish_tour). | code | Esc closes the innermost open form first. | S |
| MAIN-39 | low | Missing | Tag manager (ui/editors.slint:2150-2169) can rename and delete but not create or merge tags. The delete dialog for an unused tag reads "It is removed from 0 items." (:2281). | code | Add "New tag"; word the zero case. | S |
| MAIN-40 | medium | Missing | ui/editors.slint:1168 "Anything over 16 KB opens read-only here, and the first 16 KB are shown": a long text item can be neither edited nor read in full anywhere in the app. | code | Edit long text in place or offer "Open in an external editor". | L |
| MAIN-41 | low | UI | Three colour sets with different names: item labels (ui/editors.slint:884: 8, no Rose), list and board colours (:1750-1751: 9, with Rose), Settings accent (ui/settings.slint:697: 6 plus Windows accent). | code | One named palette. | S |
| MAIN-42 | low | UX | List and board editors restore typed values from `parked_editors` (app/controller/main_ctl/editors.rs:181-187) without the "Restored the changes you had not saved." notice the item editor shows (ui/editors.slint:1096). | code | The same notice in every editor. | S |
| MAIN-43 | medium | Accessibility | Toast timing is fixed: 2.5 s plain, 4.88 s with Undo, 10 s errors (app/controller.rs:90-96), whatever the length of the text; only pointer hover extends it (app/controller/toast_ctl.rs:244); keyboard focus does not, there is no setting, and a toast cannot be read again. WCAG 2.2.1 Timing Adjustable. A newer toast replaces an error at once (toast_ctl.rs:42-56). | code | Hold by text length, pause on focus, keep the last messages readable. | M |
| MAIN-44 | low | UX | ui/components/dialog.slint:71-74: in a destructive dialog Enter does nothing and focus starts on the card, so every confirmation needs Tab, Tab, Space. Safe, but nothing says which key confirms. | run (probe b_overlays: with the confirm or board-delete dialog open the focus item has no accessible ancestor, box (0,0 1100x720); no button has focus) | Focus Cancel first so the keys are discoverable. | S |
| MAIN-45 | low | Copy | ui/menu_texts.slint:48 "Pause hotkeys" while Settings and the tour call them "Shortcuts". | code | One word. | S |
| MAIN-46 | low | UX | Six buttons in Help > About share the visible label "Open" (ui/help.slint:443-500); Settings > Support uses "Open" too. Speech-input users must say "Open" and pick by number. | code | "Open guide", "Open releases page". | S |
| MAIN-47 | low | Missing | The welcome tour has no import step and does not mention Import, though Snipdeck reads TextExpander, Espanso, Beeftext, PhraseExpress, Ditto and CopyQ data (ui/settings.slint:1518-1526). | code | A "Bring your snippets" step or link in the tour. | S |
| MAIN-48 | high | UI | At text scale 2.25 the toolbar runs off the window: 480x360: "View" x 468..540 of 480 and "New item" x 552..580 of 480; 760x520: "View" x 712..784 of 760, "New item" x 796..824 of 760 (wholly outside, in every scene). The sidebar foot is cut too: "Settings" and "Help" y 303..366 of 360 at 480x360, and on the rail y 663..726 of 720 and 1023..1086 of 1080; "Collapse sidebar" is y -4..68 at every size. The toolbar tiers (mw:661-669) divide the room by the text scale but have no tier that fits 2.25 under about 900px, and the window minimum (mw:275-276, 480x360) does not scale. | run (probe a_main_walk, every scene at those sizes) | Layout rule: nothing in a bar may have a box outside its parent; add a last tier (overflow menu) and scale the minimum size with text scale; assert it in one test over every size x scale. | M |
| MAIN-49 | high | Accessibility | At 480x360 and text scale 2.25 the Tab cycle has 6 stops (window root, "Board: Work", Navigation, Settings, Help, then no focus item) and wraps: the search field, every toolbar button, the quick-add row and the item list are never reached by Tab (WCAG 2.1.1). At 480x360@1 the same walk has 14 stops including the list. Ctrl+F and "/" still reach the search field (code, mw:1158, 1188). Not walked at 760x520@1.5, where the search field is 6px wide (MAIN-50). | run (probe c_keyboard) | A focus stop that cannot take focus must be skipped, not end the chain; a test walks Tab at every size x scale and asserts the list is reached. | M |
| MAIN-50 | medium | UI | The search field collapses before anything else in the toolbar: 75px wide at 480x360@1 with a 35px text area (about five characters; the placeholder "Search snippets" does not fit), and 6x38px at 760x520@1.5 in every scene, although 760x520 is well above the minimum size. | run (probe a_main_walk: `TextInput "Search snippets" 6.0x38.0 at (296,6)`; Tab walk box (64,8 75x28) inner (96,12 35x20)) | Give the search field a floor (for example 160px x scale) and drop toolbar buttons into the overflow first. | S |
| MAIN-51 | medium | UI | At text scale 2.25, at every window size up to 1920x1080, the section header controls overlap their neighbours: "Sort by: Manual" is 231x72 at y 60 while the toolbar buttons above are 72px tall at y 0 (12px shared), and it reaches y 132 while the quick-add row starts at y 127 (5px shared); in the Trash "Empty Trash…" (260x53 at y 70) shares 2px with the toolbar. The quick-add row is 28px tall at every scale (836x28 at 1.0, 376x28 at 2.25): its height does not grow with its text. | run (probe a_main_walk overlap lines, 1100x720@2.25 and 1920x1080@2.25) | Row heights from content (`max(token, preferred-height)`), never a fixed px beside scaled text; an overlap assertion over interactive boxes. | S |
| MAIN-52 | low | UI | Rail: the "Expand sidebar" button (12,1 48x48) overlaps the board button (15,45 42x42) by 4px at 1.5; at 2.25 it is (18,-4 72x72) against (22,50 63x63): 18px shared and 4px above the window. | run (probe a_main_walk, rail scene, 760x520 to 1920x1080) | Same rule as MAIN-51. | S |
| MAIN-53 | medium | UI | Bulk bar at 480px wide: the "Move" button is 58.5px wide at 1.0, 30.5px at 1.5 and 28px at 2.25 (it is 101px at 1100x720@2.25), so its label is squeezed out; "Clear selection" is x 423..486 of 480 at 2.25. | run (probe b_overlays `move button` lines; a_main_walk multi scene) | Bulk bar switches to icons or an overflow menu when its preferred width exceeds the room. | S |
| MAIN-54 | low | UI | Empty "All items" state at 480x360@1.5: the third action "Import…" is y 324..362 of 360 (2px below the window, and behind the status line); the state does not scroll. The 2.25 case was not examined beyond the box check. | run (probe a_main_walk empty-all) | Empty state in a scroll area, or fewer actions when short. | S |
| MAIN-55 | high | UI | The welcome window is sized to a fixed 560x540 (ui/onboarding.slint:198-199; app/onboarding_binding.rs:439-442 passes it to `size_before_show`) and has no scroll area (no Flickable in the file). At text scale 1.5 in 560x520: step 1 "Skip" / "Next" are y 551..589 and the "Also keep what I copy?" Yes / No are y 654..688; at 2.25 the Yes / No are y 1108..1157 and Next y 731..784 (in 720x620: 930..979 and 731..784); step 2 Next y 897..950; step 4 "Get started" y 837..890. A first run with Windows text size at 150 % cannot reach Next or the Recent consent without resizing, and 225 % needs about 1,160px of height. | run (probe f_other_windows) | Every window: content in a scroll area and wanted size = min(content at the current text scale, work area); one fit test over every window x scale. | M |
| MAIN-56 | medium | Accessibility | Focus does not return to the control that opened a menu: after Esc in the View menu the focus item is the item list scope (232,76 868x612), not the "View" button (801,6 70x32); after Esc in the Help menu it is the Navigation list, not the "Help" button (WCAG 2.4.3). | run (probe b_overlays, all sizes) | `restore-focus` remembers the invoking element for every popup. | S |
| MAIN-57 | medium | Accessibility | Three focus stops have no accessible element at or above the focus item: Tab stop 0 (the window root scope, box 0,0 1100x720, nothing drawn as focused), the item list (stop 19, box 232,108 868x580) and an open confirm dialog (box 0,0 1100x720). The list rows are `keyboard-focusable: false` and the cursor is a roving index, so a screen reader may get no name when focus enters the list or a dialog, and no row announcement as the cursor moves. | run for the tree (probe c_keyboard, b_overlays); what Narrator reads needs Windows | The scope that takes focus carries the role and label (list, dialog) and the cursor row is exposed (delegate focus to the row or set the list's current item); drop the root scope from the Tab order. | M |
| MAIN-58 | low | Accessibility | The sidebar splitter is 8px wide (228,0 8x720), under the 24px target of WCAG 2.5.8, and it is the last Tab stop (20 of 21) though it sits between the sidebar and the content. | run (probe a_main_walk, c_keyboard) | 24px hit area around the 8px line; Tab order by position. | S |
| MAIN-59 | low | Accessibility | Count text under 4.5:1: light theme sidebar count "9" is 4.18:1 ([120,116,114] on [245,243,241]); dark theme count "24" on the selected row is 4.22:1 ([147,148,149] on [5,48,106]). Disabled Undo, Redo and Tile view are 2.0 to 2.3:1 (exempt). The other samples of the 320 pass. | run (probe d_contrast, 1100x720) | Counts use the secondary text token that passes on every surface, selected included; a contrast test over the accessibility tree. | S |
| MAIN-60 | medium | Accessibility | High contrast, selected row: in the boxes of the row actions "Use (Enter)", "Copy" and "Edit" (960..1056, 212) the strongest ink is white [255,255,255] on the highlight [26,235,255]: 1.46:1, while "More" beside them is 10.8:1 and the row text is 14.4:1. Either those three icons are drawn in white on the highlight, or they are in the accessibility tree while not drawn (hover-only) and something white sits under them. | run (probe d_contrast; the probe did not hover the row) | Row action icons take the highlight-text colour on a selected row; hover-only controls leave the tree when hidden. | S |
| MAIN-61 | low | Security and privacy | The privacy text says "Logs are only written when Snipdeck crashes" (ui/settings.slint:1278), but `SNIPDECK_LOG=1` or `SNIPDECK_DIAG` writes a line per UI callback to stderr, "which the launcher saves to a file" (app/trace.rs:20-62). Strings are logged as `<n chars>` (app/binding.rs:1426-1428), so no item text leaks, but error lines such as `backup failed: {e}` (app/controller/settings_ctl.rs:378) can carry file paths. | code | State the opt-in diagnostic log in the privacy text; a test that no trace line formats a path or user string. | S |
| MAIN-62 | low | Broken (test gap) | The existing fit test `the_toolbar_fits_at_every_width_with_a_gap_between_controls` (crates/snipdeck-ui/tests/main_window.rs:869) checks (480, 1.0), (640, 1.0), (760, 1.0) and (640, 1.5) only. It misses 760 at 1.5, where the sidebar is expanded and the room is smallest (MAIN-50), and every width at 2.25 (the 2.25 golden is 1400x900, main_window.rs:71). No test walks Main's Tab order at a small size, and none sizes the welcome window against text scale. MAIN-48 to MAIN-55 all pass CI. | code (tests read) | One table-driven fit test: every window x size x text scale x scene asserts no box outside, no overlap and a Tab cycle that reaches the main content. | M |

## 3. Not verified, and what a Windows run must check

Nothing in this audit ran on Windows; every `run` row is the headless software renderer on Linux.

1. Text scale: whether Windows text size 150 % and 225 % reach `SystemLook.text-scale` as 1.5 and 2.25 in the
   real app, and then MAIN-48 (toolbar off the window), MAIN-49 (Tab cycle), MAIN-50, MAIN-51, MAIN-53 and
   MAIN-55 (welcome window) on a real 1920x1080 display at 100 % and 150 % DPI.
2. Narrator on the Main window: what is read when focus enters the item list, as the cursor moves between rows,
   when a confirm dialog opens, and whether the View and Sort menus say which entry is chosen (MAIN-18, MAIN-57).
3. Real Windows high contrast (Aquatic, Desert, Dusk, Night sky): the selected row's Use, Copy and Edit icons
   (MAIN-60). The probe only set `SystemLook.high-contrast` with the app's own palette.
4. `caption-inset`: the toolbar tiers subtract it; the probe ran with 0. With the real caption buttons the
   tiers switch earlier by the width of the inset, so MAIN-48 and MAIN-50 start at wider windows than measured.
5. Pointer acts not driven: drag reorder of items and lists, drag onto a list, splitter drag, hover-only row
   actions, tooltips, right-click placement of menus (only the keyboard-opened placement was measured), touch.
6. Timings: MAIN-14 and MAIN-29 need a release build on Windows with the seeded library (10,000 items, 50
   lists, 5 boards, 200 tags, 1 MB image items). The probe timed the UI crate only (debug build); the host side
   (`main_screen()`, `sync`, `main_row`) was read, not timed, because only one probe target was allowed.
7. Code-only rows (MAIN-1, 2, 7, 8, 10 to 13, 15, 17 to 43, 45 to 47, 61, 62) were not driven. The ones most
   worth a run: MAIN-11 (Alt+Up with a multi-selection), MAIN-25 (Export as text with sensitive items),
   MAIN-28 (label filter left on an emptied label), MAIN-38 (Esc in Settings with the Erase form open).
8. Settings, Help and the item editor were not re-driven here (existing audits assert their fit, names and
   Tab walks at 1.0 and 2.25); their findings above come from reading the strings and handlers.
9. Not reached at all: app/settings.rs, app/settings_binding.rs, app/help_binding.rs, app/toast.rs,
   app/toast_binding.rs, the seven text and icon globals listed in 1a, and the bodies of the 14 test files
   (only test counts and a grep for real-clock calls, which found none).
10. Logs lens: the probe ran in the UI crate, where the app's trace log does not exist, so no log lines were
    collected during the walk.
