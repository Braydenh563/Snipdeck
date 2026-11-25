# Deep audit: `ux` (cross-surface consistency, tokens, WCAG 2.2 AA, learnability)

Head: `169ad183` (tip of `origin/claude/compassionate-pascal-pod8ux`). Agent model: Fable (owner-named).
Paths are relative to `crates/snipdeck-ui/` unless they start with `crates/` or `docs/`.
Already-recorded items in `.claude/audit/{extras,library,panel,main,data,tools}.md` are not repeated; privacy is out of scope.

Run summary: a temporary probe (`tests/probe_ux.rs`, deleted, never committed) drove the Slint harness. Sweep A: 715 window states (13 windows plus 4 galleries, each at its minimum and preferred size, light / dark / high contrast, text scale 1.0 / 1.5 / 2.25), 14,001 accessible elements, 4,500 text boxes sampled for contrast from pixels, 220 Tab walks (28 presses at 1.0 light, 10 elsewhere). Sweep B: 21 window states in 7 languages (en, de, fr, es, ja, pt-BR, zh-Hans) at 1.0 light, 4,982 elements. Main and Panel layout results at scale repeat MAIN-48..57 and the PANEL rows and are not listed again. The cargo lock was held by other agents for most of the time box, so the run finished late; rows marked `code` were not driven.

## 1. Coverage

Status words: `checked` = read in code; `driven` = exercised by the probe in the Slint harness; `not reached (why)`.

### 1a. Files (the 49 `ux` rows of the map)

| row | status | how |
|---|---|---|
| ui/components/button.slint | driven | run (every window) + code |
| ui/components/card-depth.slint | checked | code |
| ui/components/choice.slint | driven | run (editors, onboarding radios) + code |
| ui/components/chooser.slint | checked | code |
| ui/components/dialog.slint | driven | run (list delete, Help rebuild, gallery) + code |
| ui/components/empty_state.slint | checked | code |
| ui/components/feedback.slint | checked | code (toast states: see 1b) |
| ui/components/form.slint | driven | run (Settings, editors) + code |
| ui/components/group-header.slint | checked | code |
| ui/components/hotkey_health.slint | driven | run (Settings, onboarding step 1) + code |
| ui/components/hotkey_recorder.slint | checked | code |
| ui/components/icon-button.slint | driven | run (Panel, Main) + code |
| ui/components/icon.slint | checked | code |
| ui/components/item-card.slint | driven | run (Panel, Main) + code |
| ui/components/keycap.slint | driven | run (Picker footer, Help) + code |
| ui/components/label-chip.slint | checked | code |
| ui/components/menu.slint | driven | run (Panel app menu) + code |
| ui/components/scrollbar.slint | checked | code |
| ui/components/shortcuts-sheet.slint | checked | code |
| ui/components/sidebar-item.slint | driven | run (Main 1040x640) + code |
| ui/components/state-color.slint | checked | code |
| ui/components/tag.slint | checked | code |
| ui/components/text_field.slint | driven | run (editors, fill, unlock) + code |
| ui/components/tooltip.slint | checked | code |
| ui/components.slint | checked | code |
| ui/lib.slint | checked | code |
| ui/theme.slint | checked | code; contrast of every text/border/accent token against every background computed from `design/tokens/palette.json` |
| ui/gallery/buttons.slint | checked | code (token greps) |
| ui/gallery/focus_probe.slint | checked | code |
| ui/gallery/forms.slint | checked | code (token greps) |
| ui/gallery/inputs.slint | checked | code (token greps) |
| ui/gallery/primitives.slint | driven | run (DialogGallery, light, 1.0) + code |
| ui/gallery/review_probe.slint | checked | code |
| ui/gallery/scroll.slint | checked | code |
| tests/audit_final1.rs | checked | code (7 tests, 4 asserts: UX-39) |
| tests/audit_final1_keys.rs | checked | code |
| tests/audit_final1_tree.rs | checked | code (UX-37) |
| tests/button_logic.rs | checked | code |
| tests/buttons.rs | checked | code |
| tests/focus_traversal_repro.rs | checked | code |
| tests/inputs.rs | checked | code |
| tests/inputs_logic.rs | checked | code |
| tests/placeholder_align_logic.rs | checked | code |
| tests/primitives.rs | checked | code |
| tests/primitives_logic.rs | checked | code |
| tests/primitives_pixels.rs | checked | code |
| tests/review_fixes_logic.rs | checked | code |
| tests/review_fixes_pixels.rs | checked | code |
| tests/ux_fit.rs | checked | code |

No test in the list reads the real clock (0 uses of `Instant::now`, `SystemTime::now`, `Local::now`, `Utc::now`).

### 1b. Surfaces and lenses

| row | status | how |
|---|---|---|
| Same act across windows: names, chords, menus, confirm/undo | checked | code (UX-1..20) |
| Tokens: font sizes, weights, radii, literal colours, motion | checked | code (0 literal font sizes; UX-30, UX-31) |
| Token contrast, every text/border/accent on every background | checked | code, computed (UX-32, UX-33) |
| Pixel contrast, all windows x 3 themes x 3 scales | driven | run: 4,500 text boxes; no token-level failure outside Main/Panel rows already recorded; small 11px text peaks at 3.87-4.47 (section 3) |
| Target size 24px, all windows | driven | run: 18 hits, all the Main sidebar splitter (MAIN-58) |
| Accessible names on interactive elements | driven | run: only the icon radios of the list editor, a fixture gap already noted in main.md |
| Elements inside the window at min size and at scale | driven | run (UX-35, UX-43, UX-44) |
| Focus ring visible on every Tab stop | driven | run: 0 rings under 3:1; silent stops in Tag manager (UX-45) and Main (MAIN-57) |
| Focus after dialogs | driven | run for Help (UX-21); code for the rest |
| Translations on screen, 6 locales | driven | run: 0 off-window elements; overlaps UX-41, UX-42 |
| Live announcements | checked | code (UX-23); not reached by run (needs UIA) |
| Reduced motion | checked | code: every `Motion` token honours it; no test (UX-38) |
| 2.5.7 drag alternatives | not reached (PANEL-29/30, MAIN-15 own it) | - |
| Learnability, first run to daily use | checked | code (UX-5, UX-28, UX-29) |
| Dialogs: list delete, item discard, Help rebuild, Settings erase | driven | run (UX-21, UX-40) |
| Toast window, suggest hint, unlock, template fill | driven | run: clean at all 9 theme/scale pairs |
| Tag remove-x overlap at scale | not reached (no fixture shows a removable tag) | code (UX-36) |
| Windows-only: real high contrast themes, UIA, DPI | not reached (Linux harness) | section 3 |

## 2. Findings

Columns: id | severity | lens | evidence | verified | class-level fix | effort.

### 2a. One act, one chord (DESIGN.md "One chord means one thing in every window")

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-1 | high | consistency / shortcuts | `Ctrl+P` pins an item in Main and Panel but toggles Preview in the item editor (`ui/editors.slint:1009`; `crates/snipdeck-app/src/shortcuts.rs` catalogue has both `Ctrl+P` rows). DESIGN.md says one chord means one thing in every window. | code | One chord table in one module that every window's key handler reads; a test that fails when one chord maps to two acts. Move Preview to another chord. | S |
| UX-2 | medium | consistency / shortcuts | No editor accepts `Ctrl+S`. Save is `Ctrl+Enter` only (`ui/editors.slint:1005`, `:1804`; `ui/template_fill.slint:156`). `Ctrl+S` does exist, with a different meaning, in the Colours and Symbols tools (`ui/panel.slint:1297-1320`: save colour / save symbol). | code | Accept `Ctrl+S` as Save in every window with a Save button; list it in the catalogue. | S |
| UX-3 | medium | consistency / shortcuts | Closing a window uses three rules: Main closes on `Ctrl+W` and ignores `Esc`; Panel closes on `Esc` and `Ctrl+W`; Settings, Help, editors and Tag manager close on `Esc` only, `Ctrl+W` does nothing (`ui/settings.slint:545-557` handles only F1 and Esc). | code | One close rule (`Ctrl+W` everywhere, `Esc` for transient windows) in the shared chord table. | S |
| UX-4 | medium | consistency / shortcuts | `Ctrl+F` focuses search in Main and Panel but not in Settings, which has a "Search settings" field (`ui/settings.slint:545-557`), nor in Help > Shortcuts, which has a search field. `F1` opens Help from Main, Panel, Settings and editors but not from Tag manager, Onboarding or the unlock window. | code | Every window with a search field takes `Ctrl+F`; every window takes `F1`. Assert both in one table-driven test over all exported windows. | S |
| UX-5 | medium | learnability / shortcuts | Shortcuts that work but are missing from Help > Shortcuts (`crates/snipdeck-app/src/shortcuts.rs`, 84 rows): `Ctrl+C` copy item and `Ctrl+V` paste as new item (`ui/panel.slint:1333-1341`, `ui/main_window.slint:1876,1902`); Colours `Ctrl+S`, `Ctrl+Shift+S`, `Ctrl+I`, `Ctrl+C` and Symbols `Ctrl+S` (`ui/panel.slint:1297-1320`); `/` focuses search in Main (`ui/main_window.slint:1188`); `Ctrl+Shift+Z` redo; `Ctrl+Shift+Enter` full editor from quick add (`ui/quick-add.slint:133`); `Shift+F10` / Menu key; Picker `Ctrl+O`; `Delete` clears a recorded hotkey. | code | Generate the catalogue and the key handlers from one table, so a handled chord cannot be absent from Help. | M |
| UX-6 | medium | keyboard / WCAG 2.1.1 | Menus, the chooser and the command palette have no type-ahead and no PageUp/PageDown; the palette also lacks Home/End (`ui/components/menu.slint:216-245`; `ui/command-palette.slint:164-176` handles only Esc, Down, Up, Enter). The Panel app menu is 21 rows (PANEL-31), so the last entry is 20 key presses away. | code | One list-navigation helper (Up/Down/Home/End/PageUp/PageDown/type-ahead) used by Menu, Chooser, palette, Picker and lists. | M |

### 2b. One act, one name

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-7 | medium | consistency / wording | Deleting an item has four names: "Move to Trash" (Main menu, `ui/main_window.slint:957-972`), "Delete" (Panel item menu, `ui/menu_texts.slint`), "Deleted “x”" (Panel inline bar, `ui/panel.slint:2336-2355`), "Moved N items to the Trash" (toast). The act is the same (soft delete to Trash). | code | A glossary file of act names (`docs/` or one `*_texts.slint` global) that every surface reads; no surface defines its own verb. | M |
| UX-8 | low | consistency / wording | Moving an item: "Move to list…" (Main), "Move to…" (Panel menu), "Move" (bulk bar). Tagging: "Tags…" (Main) vs "Tag" (bulk). Creating: "New item", "New note" (`ui/panel.slint:628,2599`), "New". | code | Same glossary as UX-7. | S |
| UX-9 | low | consistency / wording | Bulk selection mode has four names: "Select or delete…" (`ui/menu_texts.slint:54`), "Select and delete", "Select snippets", "Delete snippets". Sort has three: "Sort: {}", "Sort by: {}", "Sort by". | code | Same glossary. | S |
| UX-10 | medium | consistency / wording | The Main window has no stable name: "Main" (tips), "Main window" (shortcut catalogue), "Open Snipdeck" (menus, tray), "the window" (tour), while its title bar reads "Snipdeck". A user told to "open Main" cannot find a control with that word. | code | Pick one user-facing name and use it in menus, tips, tour, catalogue and title. | S |
| UX-11 | medium | consistency / wording | The pin / stay-open switch has four names: "Keep panel on top", "Panel stays open", "Toggle keep Panel open" (palette), "Hide the panel when it loses focus" (Settings, inverted sense). | code | One name and one polarity. | S |
| UX-12 | medium | consistency / wording | Quick capture is "Save the clipboard as an item" (`ui/settings.slint:467`, `ui/onboarding.slint:234`), "Quick capture the clipboard" (`ui/shortcut_texts.slint:21`) and "Quick capture" (palette, tips). Paste-next is "Paste the next item" (`ui/settings.slint:468,474`), "Paste the next item of a stack" (`ui/shortcut_texts.slint:22`), "Paste stack", "Paste next item". | code | Feature names in the glossary; Settings hotkey rows, catalogue and palette read the same string. | S |
| UX-13 | medium | consistency / wording | Typed expansion is "Trigger" (item editor field), "Expand abbreviations as I type" (`ui/settings.slint:1331`), "short code" (`ui/tip_texts.slint:17`) and "code" (toast). Four nouns for one concept; searching Settings for "trigger" will not find the switch. | code | One noun; add the others as search keywords for the Settings filter. | S |
| UX-14 | high | consistency / wording | The Recent feature has six names: "Recent", "Recent copies", "Recent list" (`ui/panel.slint:3214`), "Recent inbox" (`ui/settings.slint:1088`), "recent history", "clipboard history". The Main empty state says "Turn on clipboard history in Settings" (`ui/main_window.slint:2479`) but no setting carries that name, and the tray says "Pause history". | code | One name; the empty state's action should open the exact setting (deep link), not describe it. | S |
| UX-15 | low | consistency / wording | The user's store is "library" (46 strings), "database" (6: `ui/toast_texts.slint:288-289`, `ui/help.slint:514`), "Snipdeck folder", "data folder" and "deck" (one tip). | code | "library" for the content, "data folder" for the location; remove "database" from user text. | S |
| UX-16 | low | consistency / wording | Ellipsis rule (an entry that opens a further step ends in "…") is applied unevenly: "Troubleshoot…", "Settings…", "Commands…" have it; "User guide", "About Snipdeck", "Open Snipdeck", "Take screenshot" (all open a window or start a mode) do not; the same act is "Edit" in Main and "Edit…" in the Panel. | code | State the rule in DESIGN.md and lint menu strings against a list of window-opening commands. | S |
| UX-17 | low | consistency / wording | Toast punctuation is mixed: "Copied", "Duplicated", "List added" have no full stop; "Library unlocked.", "Master password set." do; both "Saved {}." and "Saved {}" exist (`ui/toast_texts.slint`). | code | One rule (no full stop on single-clause toasts) plus a test over every `ToastTexts` function. | S |
| UX-18 | low | consistency / wording | Deleting a tag asks two different questions for the same act: Main "Delete tag “{}”?" / "The tag is removed from its items. The items themselves stay." (`ui/main_window.slint:804,810`); Tag manager "Delete this tag?" / "It is removed from {n} item(s)…" (`ui/editors.slint:2280`). Only one names the tag, only one gives the count. | code | One shared dialog text function taking name and count. | S |

### 2c. One act, one pattern (menus, confirmation, undo, focus)

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-19 | high | consistency / menus | The item context menu differs by window. Main (`ui/main_window.slint:957-972`): Move up/down, Move to list…, Tags…, Group…, Edit, Move to Trash; no Colour, Sensitive or paste stack. Panel (`crates/snipdeck-app/src/controller/panel.rs:82` `ItemMenuEntry`; `ui/menu_texts.slint`): Edit…, Move to…, Colour, Sensitive, paste stack, Delete; no Tags, no Move up/down. The starter card says "Right-click a card… tag it" (`crates/snipdeck-app/src/starter.rs`), which is false in the Panel, where the starter cards are first seen. | code | One item-menu model built in Rust and rendered by both windows; window-specific entries are the exception and are listed. | M |
| UX-20 | high | consistency / safety | Confirmation and undo follow no rule. Item delete, bulk delete and "Delete group and its items" act at once with an Undo toast (`crates/snipdeck-app/src/group_ctl.rs:546`, `panel_select.rs:472`). List delete, board delete and Clear list ask in a dialog and also go to Trash and also show Undo (`main_ctl/lists.rs:103`, `editors.rs:305`). Tag delete asks and has no undo. A group of 200 items goes without a question; an empty list asks. | code | Write the rule in DESIGN.md (reversible: act and offer Undo; irreversible: confirm) and route every destructive act through one function that enforces it. | M |
| UX-21 | medium | focus / WCAG 2.4.3 | The Help window's Rebuild dialog has no `closed =>` handler (`ui/help.slint:511`). `Dialog` calls `scope.clear-focus()` on close (`ui/components/dialog.slint:46-51`) and relies on the host to restore focus; the editors and Main do (`ui/editors.slint:1538,1921,1942,2283`; `ui/main_window.slint:3029,3048`), Help does not. After Cancel no element has focus, so Esc and F-keys in Help go nowhere until the user clicks. Run: Esc reached the window 1 time before the dialog, 0 times after Cancel, 1 time after one Tab. | run | `Dialog` should remember and restore the previously focused element itself; hosts override only when the opener is gone. | S |
| UX-22 | medium | keyboard / WCAG 2.1.1 | Toast action buttons other than Undo have no keyboard path. `ToastButton` has a FocusScope with `focus-on-click: false` (`ui/components/feedback.slint:14-60`) and nothing moves focus to a toast (no `F6`, no focus call in `ui/main_window.slint` or `ui/panel.slint`); Undo is reachable by `Ctrl+Z`, but "Edit" after a capture (`ui/main_window.slint:2514-2521`) is mouse-only and disappears on the toast timer (MAIN-43). | code | Every toast action gets a chord shown in the toast, or a documented key moves focus to the newest toast; toasts with an action do not time out while focused or hovered. | M |
| UX-23 | medium | a11y / WCAG 4.1.3 | Every toast kind, including danger, is announced `polite` (`ui/components/feedback.slint:138`). A failed save or failed paste is queued behind whatever the screen reader is saying. | code | `assertive` for danger, `polite` for the rest; test on the Toast component. | S |
| UX-24 | medium | a11y / WCAG 4.1.2 | Buttons that open a menu do not expose expanded state: `ToolbarButton` (`ui/main_window.slint:110-130`) and `IconButton` (`ui/components/icon-button.slint`) set no `accessible-expanded`; `IconButton.active` is exposed as a state only where the host adds `accessible-checked`. A screen reader user cannot tell a menu opened or which toggle is on. | code | `IconButton`/`ToolbarButton` take `menu-open` and `active` and map them to `accessible-expanded` / `accessible-checked` themselves. | S |
| UX-25 | medium | feedback | `Button.loading` (`ui/components/button.slint`) is used by no app window, only the galleries. Back up now, Export, Import and Rebuild have no busy state (only the master-password flow has `master-busy`), so a slow disk gives no feedback and allows a second click. `loading` also disables the FocusScope, which drops keyboard focus when it starts. | code | Every act that touches the disk takes a `busy` flag wired to `Button.loading`; loading keeps focus and sets `accessible-busy`/label. | M |
| UX-26 | medium | parity | A hidden (sensitive) item is revealed by holding Space in Main and Panel; in the Picker the row says "Hidden. Open it in the Panel to reveal." (`ui/picker.slint`). The Picker is the surface used while typing in another app, so the user must leave it to see what they are about to paste. | code | Same reveal gesture on every surface that shows a hidden item. | S |
| UX-27 | medium | error prevention / WCAG 3.3.1 | The hotkey recorder silently ignores keys it will not record: no modifier, bare function keys, reserved keys (`ui/components/hotkey_recorder.slint:262-278`). The prompt stays "Press the new shortcut". A user pressing `F9` sees nothing happen and no reason. | code | Recorder shows an inline reason for every rejected press ("Add Ctrl, Alt or Win") and announces it. | S |
| UX-28 | low | discoverability | The tray menu (`ui/tray_texts.slint`; `crates/snipdeck-app/src/wording.rs:40-52`) has Open Panel, Open Picker, Open Snipdeck, Tips, Take screenshot, Pause hotkeys, Pause history, Quit, but no Settings and no Help. When hotkeys fail (the common Windows first-run problem) the tray is the only entry point and it cannot reach the hotkey settings or Troubleshoot directly. | code | Tray gets Settings… and Help/Troubleshoot…. | S |

### 2d. Learnability

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-29 | medium | learnability | The four-step tour (`ui/onboarding.slint`) covers hotkeys and the Panel but never introduces boards, lists, tags, the Main window or how to add a first item; `finish_tour` only stores `onboarded` (`crates/snipdeck-app/src/onboarding_ctl.rs:61`) and opens nothing. No tip covers the Colours tab, boards or backup (`ui/tip_texts.slint`). | code | Tour ends by opening the Panel on the starter list with the "new item" control highlighted; tips cover each top-level feature once. | M |

### 2e. Design tokens

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-30 | low | tokens | Off-token radii. DESIGN.md locks radii to 4/6/8 (`ui/theme.slint` `Radius`); literal 1px, 2px, 3px radii at `ui/panel.slint:169`, `ui/command-palette.slint:64`, `ui/components/item-card.slint:608,759,788`. | code | Add `Radius.xs` (2px) for bars and pips, or use `Radius.sm`; a grep test for `border-radius: \d` outside `theme.slint`. | S |
| UX-31 | low | tokens | Literal `font-weight: 600` at `ui/components/group-header.slint:101,113`; every other weight uses `Type.medium` / `Type.semibold`. | code | Use the token; grep test for `font-weight: \d`. | S |
| UX-32 | medium | contrast / WCAG 1.4.3 | `text.tertiary` `#7C7A76` (`design/tokens/palette.json`) is below 4.5:1 on every background: light 3.54-4.10:1, dark 2.99-4.46:1 (computed from the palette). It is used as text for the "default" caption, 11px (`ui/colours.slint:579`). | code (computed) | Tertiary is for non-text only; a token test that computes contrast of every text token against every background token. | S |
| UX-33 | medium | contrast / WCAG 1.4.11, 1.4.3 | On the dark selected-row background (`bg.selected`) three tokens fall under 3:1: `accent.default` 2.66:1, `danger.solid` 2.44:1, `border.strong` 2.99:1; `accent.default` on dark overlay-hover is 2.93:1. `border.strong` on light surfaces is 3.54:1 (passes 3:1 for controls but is the input border at its minimum). Accent text or icons inside a selected row in dark theme are affected. | code (computed) | The same token contrast test, including "token on selected / hover" pairs. | M |
| UX-34 | medium | high contrast | High-contrast colours are built from system colours that Windows does not guarantee as a pair (`ui/theme.slint`): the danger button is `hc-highlight-text` on `hc-window-text`; accent text is `hc-highlight` on `hc-window`. These hold in the four stock themes but not in a custom one. | code | Use only the documented pairs (WindowText/Window, HighlightText/Highlight, ButtonText/ButtonFace). Needs a Windows run. | M |

### 2f. Sizes and text scale

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-35 | medium | text scale / WCAG 1.4.4 | Window minimum sizes are fixed pixel values that do not grow with text scale: List editor and Board editor 380 wide, min-height `min(300px, …)` (`ui/editors.slint:1961,2024`); Tag manager 360x280 (`:2084`); Help min-height 360; item editor min-height 380; Settings wants 640x760. Run: Tag manager at 360x280 and scale 2.25 puts Close at y 231..284 of 280 (4px cut); see also UX-41..44. | run + code | Minimum size = content minimum at the current scale (`Space.* * text-scale`), clamped to the work area, with a scroll view as the fallback. | M |
| UX-36 | medium | target size / WCAG 2.5.8 | Tag remove-x overlaps the tag body above scale 1.0: `ui/components/tag.slint:100` subtracts the unscaled `Space.hit-min` from the body's touch width while the x cell is `Space.hit-min * text-scale` wide (`:60`); the later TouchArea wins, so at 2.25 a strip of (2.25-1) x hit-min of the x cell activates the tag instead of removing it. | code | Derive both from one scaled property. | S |

### 2g. Test gaps (class-level)

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-37 | medium | test coverage | The accessibility tree test (`tests/audit_final1_tree.rs`) covers Help, item editor, template fill and Settings at 1.0 and 2.25, light only. Not covered: Panel, Main, Picker, List/Board editor, Tag manager, Onboarding, unlock, toast, and dark or high contrast for any window. | code | One table: every exported window x 3 scales x 3 themes, asserting names, target sizes, on-window bounds. | M |
| UX-38 | low | test coverage | `set_high_contrast(true)` appears once in each of only 6 UI test files; `set_reduced_motion` appears in no test, so the reduced-motion path of every `Motion` token is unverified by any test. | code | Add to the table in UX-37. | S |
| UX-39 | low | test coverage | `tests/audit_final1.rs` has 6 `dump_*` tests that assert nothing (they write PNGs when `SNIPDECK_AUDIT_DIR` is set and otherwise pass). They count as passing tests in the gate. | code | Mark `#[ignore]` or move to an example binary. | S |

### 2h. Found by the run

| id | sev | lens | evidence | verified | class fix | effort |
|---|---|---|---|---|---|---|
| UX-40 | medium | a11y / modal | With a dialog open the window behind it stays in the accessibility tree: the list editor with its delete dialog open exposes 49 elements, including the 24 icon radios and the form fields under the scrim (`ui/components/dialog.slint`; probe state `list-delete 380x300`). A screen reader can leave the modal. | run | Each window binds one `modal-open` flag that removes its content from the tree (or `Dialog` becomes a real popup). | M |
| UX-41 | medium | i18n layout | Tag manager at its minimum 360x280 in German, French and Spanish: the Rename and Delete buttons of a tag row overlap the footer Close button by 15px vertically and 18-28px horizontally. English: no overlap. | run | Footer outside the scroll area; minimum height from content (UX-35). | S |
| UX-42 | medium | i18n layout | Tour step 1 at 560x540 in German, French and Brazilian Portuguese: the Yes/No radios overlap the Skip and Next buttons by 6px vertically (54-103px wide). English: no overlap. Adds the locale case to MAIN-55. | run | Tour body scrolls above a fixed footer; window height from content. | S |
| UX-43 | medium | text scale / WCAG 1.4.10 | List editor and Board editor at their preferred 460x440 and scale 2.25: the last colour swatch ("Red") ends at x=468 and the last icon radio at x=481 of 460 (8px and 21px outside the window). | run | Swatch and icon grids wrap by available width, not by a fixed column count. | S |
| UX-44 | low | text scale | Picker at 420px high and scale 2.25: the "Hold to reveal" button spans y 392..428 of 420 (8px cut). Help > Tips at 420x360, scale 1.0: the "Try it" button spans y 334..362 of 360 (2px cut). | run | Same content-driven minimum as UX-35. | S |
| UX-45 | low | focus / WCAG 2.4.7 | Tag manager has one Tab stop that changes no pixel, at both sizes and in all three themes (27 visible of 28 presses at 1.0; 9 of 10 elsewhere). | run | The table test of UX-37 asserts every Tab press changes the frame. | S |

## 3. Not verified; what a Windows run must check

Not verified here:

1. UX-1..20, 22..34, 36..39 are from code; none was driven. UX-36 (tag x overlap) needs a fixture with a removable tag at scale 2.25.
2. Small text. 11px keycap and hint text (token pair `text.secondary` on `bg.sunken`, 5.66:1 computed) measures 3.87-4.47:1 at its darkest pixel in the software renderer (Picker "Enter" 4.39, "Alt+5" 4.22; Help keycaps at scale 1.5: 3.87 light, 4.46 dark). This is anti-aliasing of thin strokes, not a token failure; whether it is legible depends on ClearType.
3. Focus rings were measured without advancing the mock clock (the harness resets it per window), so an animated ring is measured at its first frame. 0 rings were under 3:1.
4. Settings at 480px wide and scale 2.25 puts "Jump to a section" at x 16..556; 480 is below the window's own scaled minimum, so this only matters on a work area narrower than that minimum.
5. The Panel and Main rows of the sweep (tabs and close button off the right edge at 1.5 and 2.25, menu rows starting at x=-64) were not compared line by line with PANEL-12 and MAIN-48..57.
6. Onboarding at scale 2.25: all 10 Tab presses on step 1 change no pixel (the footer is off the bottom, MAIN-55).

A Windows run must check:

1. High contrast with the four stock themes and one custom theme: danger button and accent text pairs (UX-34), selected rows, focus rings.
2. Narrator / NVDA: toast announcements from the separate toast window, polite versus assertive (UX-23); whether a modal dialog confines the virtual cursor (UX-40); expanded state of menu buttons (UX-24).
3. Focus after the Help rebuild dialog closes (UX-21: Esc reached the window 1 time before the dialog, 0 times after Cancel, 1 time after one Tab).
4. Text scale from Windows Settings at 150% and 225% combined with display scaling of 150%: the editors, Tag manager, Picker and tour at their minimum sizes (UX-35, UX-41..44).
5. Whether the toast window takes activation, and so whether any key can reach a toast action (UX-22).
6. Tray menu entries as the OS shows them (UX-28) and ClearType rendering of 11px text (item 2 above).
