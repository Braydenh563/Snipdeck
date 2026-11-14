# Panel v2: build spec (2026-10-25)

Status: ready to build. Inputs: owner decisions (fixed, below), `docs/reviews/panel-v2-critique.md`
(findings cited as C1-C15), the D v3 mock-ups (`DV2.dc.html` states, `DV2-variants.dc.html`), DESIGN.md
and SPEC 5.1. All sizes are logical px at 100 % scale; colours are DESIGN.md semantic tokens. A token
marked **new** comes through `design/tools/palette.py` (no hex in `.slint`).

Owner decisions, not reopened: default = D v3 (B+C hybrid, search expands like A); one click on a row
pastes; the title is never crushed; a visible button closes the search; the type is an icon; Alt+Tab is
the existing `panel_in_taskbar` setting; layout and access options live in Settings.

## 0. What the research says (and what we take)

| Product | Quick pick | Takeaway for Snipdeck |
|---|---|---|
| Win+V | One click pastes; per-item `⋯` holds Pin/Delete; no search | Click = use, extras behind `⋯`: matches us |
| Ditto | Hotkey window opens at caret, cursor or previous position (setting); Enter or double-click pastes; number hotkeys; copies when it finds no target | Position setting is standard; our "Copies to clipboard" fallback matches |
| Raycast | Enter = primary action (user can change it); Ctrl+K opens an action panel (copy, edit, pin, delete) | Keep one primary action; extras in a menu; Ctrl+K stays |
| PowerToys Cmd Palette | One activation chord, preferred-monitor setting | Keep access settings few; no per-monitor matrix |

Sources: [Raycast manual](https://manual.raycast.com/clipboard-history),
[Ditto features](https://deepwiki.com/sabrogden/Ditto/1.2-features), [Ditto site](https://ditto-cp.sourceforge.io),
[Win+V guide](https://www.digitalcitizen.life/clipboard-history-windows-11-shortcut/),
[Command Palette](https://learn.microsoft.com/en-us/windows/powertoys/command-palette/overview).

## 1. Decisions taken here (written once, with reasons)

1. **Rows are quiet; the tile carries the type.** Row fill is transparent at rest (the list sits on
   `bg.surface`), `bg.hover` on hover, `bg.selected` when selected. Reason: per-type row tints measured
   1.07:1 against the surface (C6): they add noise and no information.
2. **Every kind gets a type icon, plain text included** (owner: "type shown as an icon"). This replaces
   DESIGN.md "no generic leading icons on plain text". Amend DESIGN.md §4 in slice 1.
3. **No Paste button on the row.** The row click is Paste; the footer says where it goes and the row
   tooltip says "Click to paste into Outlook. Shift+click copies." Hover/selected actions are Copy, Edit,
   More. This replaces DESIGN.md's "Paste ↵" ghost button and its "no row of three icon buttons" rule
   for the Panel (amend §4/§7). C2 is answered by the footer copy plus the card-pull check mark.
4. **Actions sit on the preview line only**, never on the title line (owner: title never crushed).
5. **Panel search covers every board** and shows where each match lives (D v3 state 4). SPEC 5.1 says
   "current board": amend it. Reason: the Panel is the quick window; a miss on another board sent
   people to the Main window.
6. **The layout is one global setting**, not per list. `snipdeck-core`'s `ListView` stays in the store
   (persisted data) but the Panel stops reading and writing it; SPEC 5.1 "Per-list choice" is removed.
   Reason: a layout that changes when you switch tabs is a surprise in a 2-second window.
7. **Masked previews show no trailing characters** in the Panel (`••••••••••••`, 12 dots) (C15). The
   Panel is always on top and visible in screen shares. The Main window keeps `•••• 7f3a`.
8. **Undo/Redo leave the header** for the More menu (Ctrl+Z / Ctrl+Y unchanged) (C7, C8).
9. **No "Row preview on/off" setting**: Compact already is the no-preview layout. Three new layout
   settings, not four.

## 2. Default layout: List (320 × 560)

Vertical budget at rest: header 40 + tabs 32 + [pinned 36 when the board has pins] + list + footer 32.
Chrome = 104 px (19 %) without pins, 140 (25 %) with pins (C8: was 185, 33 %). All content shares one
left inset: **x = 8** from the window edge (C13).

### 2.1 Header (40)
Left to right, controls 32 tall, 2 px gaps, padding 0 4 0 8:

| Control | Size | Content | Action |
|---|---|---|---|
| Board picker | h32, padding 0 8, max width = what is left (≥ 69 at 260) | board name `type.md` 600 + caret-down 16 | opens the board Chooser |
| spacer | flex | | drag region (`drag-requested`) |
| Search | 32×32 | magnifying glass 16 | expands search (§2.3) |
| New | h32, padding 0 8 0 6 | plus 16 + "New" `type.base` 500; icon-only 32×32 below 300 width | menu: Save from clipboard, New note (Ctrl+N), New list |
| divider | 1×16, margin 0 4 | `border.default` | |
| Keep on top | 32×32 | push-pin 16; on = `bg.selected` fill + `accent.text` glyph, `aria-pressed` | Ctrl+Shift+P |
| More | 32×32 | dots-three 16 | Undo, Redo (real disabled state, `text.tertiary`, not opacity), Paste stack, Dock to edge, Keyboard shortcuts, Settings |
| Close | 32×32 | x 16 | hide (Esc with nothing open) |

Board name truncates with an ellipsis, never below 6 characters; under that the name goes to the tooltip.

### 2.2 List tabs (32, Board switcher = Tabs)
Padding 0 4 0 8, 1 px `border.subtle` bottom. Recent clock tab (when history is on) at the far left,
24×32. Each list tab: padding 0 8, `type.base`; active = `text.primary` 500 + 2 px underline in the list
hue (600 light / 400 dark); inactive `text.secondary` 400. Optional list icon 16 before the name.
Overflow: tabs scroll horizontally (wheel, Ctrl+Tab brings the active one into view) and a 24×32
caret-down at the right lists all lists. "New list" 32×32 plus at the far right.

### 2.3 Search
**Collapsed (default, Search = "When you type")**: only the header icon. Any printable key (the existing
capture rule), Ctrl+F or the icon expands it with that character in the query.

**Expanded**: the block replaces the tab row and the pinned strip (the A pattern). Padding 8 8 4,
1 px `border.subtle` bottom, total 72:
- Row 1 (36): field flex, h36, radius 6, `bg.surface`, 2 px `focus.ring` border while focused,
  magnifying glass 16 `text.secondary`, text `type.md`. Then 6 gap, **Close** button h36, padding 0 10,
  1 px `border.strong`, x 16 + "Close" `type.base` 500 + keycap "Esc" (`type.xs` mono, 1 px
  `border.strong`, radius 4). Close = Esc: clears the query and collapses in one step.
- Row 2 (20): status, `type.sm` `text.secondary`, `role=status`: "3 matches in all boards" /
  "No matches in all boards".
- Matches: title 600 on a `search.mark` (**new**: light `#D0E3FE` with `text.primary`, dark `blue-800` with
  `neutral-0`) span, radius 3, no underline (C10). The mark must differ from `bg.selected` by ≥ 1.3:1 (palette.py check).
- Result rows add a where line (16): 6 px list-hue square + "Board · List" `type.xs` `text.secondary`;
  row height 68.

**Always shown** (setting): a 44 strip under the header (field h36, padding 4 8), no Close button; a
24×24 clear button (x) inside the field when it holds text. Tabs and pins stay visible while searching.
IME users: Ctrl+F or this setting, because a collapsed field cannot host an IME composition.

### 2.4 Pinned strip (36, only when the board has pins, hidden while searching)
Padding 0 4 0 8, chips h28, radius 6, `bg.hover` fill, type glyph 16 + title `type.sm`, gap 6. At 320,
up to 2 chips, then a visible text button "+N more" (`accent.text`, 500) (C9). One click pastes. Docked
full height: wraps to 2 rows (DESIGN §8).

### 2.5 Row anatomy (List, 56 tall)
List area: `bg.surface`, padding 8, rows 4 apart, radius 8. Horizontal, at 320 wide:

| Part | x / width | Spec |
|---|---|---|
| Label bar | 0-4 inside the row, 4×(row-16) | only when the user set a colour label |
| Row padding | 8 left, 4 right | |
| Number | 16 wide | 1-9, `type.xs` mono `text.tertiary`; empty for 10+; absent when `show_numbers` is off (text gains 20) |
| gap | 4 | |
| Type tile | 32×32, radius 6 | `kind.<k>.tile` fill (**new**, 7 pairs from the mock's `tile`/`glyph` values), Phosphor 16 glyph in `kind.<k>.glyph`; glyph ≥ 4.5:1 on its tile; tooltip + accessible name = kind name |
| gap | 10 | |
| Text column | 230 at 320 (flex) | line 1 title, line 2 preview |

Glyphs: text `TextT`, template `BracketsCurly`, code `Code`, link `Link`, sensitive `Lock`, file `File`.
Image: the tile is a 32×32 centre crop of the image, radius 6, 1 px `border.subtle`.

- **Title** (20 line): `type.base` 500 `text.primary`, one line, end ellipsis. Its width is never
  reduced by actions. ≥ 12 characters always fit at 260 wide and 225 % text scale (DESIGN §8).
- **Preview** (16 line, 32 tall hit row): `type.sm` `text.secondary`; mono for code, link, sensitive;
  template placeholders mono `accent.text`; file = "PDF · name · 2.4 MB"; image = "PNG · 480 × 120".
- **Actions** (hover, or the selected row): on the preview line, right-aligned, solid row fill behind
  (no fade): Copy, Edit, More, each a 28×28 ghost icon button (radius 6, glyph 16 `text.primary`,
  hover `bg.overlay-hover`) in a 32×32 hit box. Group width 96 plus 6 left padding; the preview keeps
  ≥ 128 at 320. With no target Copy is hidden (the click already copies). Copy = Shift+click,
  Edit = Ctrl+E, More = the row context menu (Pin, Move, Duplicate, Rename, Paste as plain text, Delete).
- **Sensitive**: a 28×28 eye button (32 hit) always at the preview line's right end, press and hold to
  reveal (Space hold too); actions sit left of it (C3).
- **States**: hover `bg.hover`; selected `bg.selected`; keyboard focus adds a 2 px inset `focus.ring`
  only on focus-visible (C11); pressed = card pull (2 px lift 140 ms, number becomes a check in
  `accent.text`, settle 200 ms after 700 ms; reduced motion: check only).
- **Hit**: whole row is the paste target; action buttons stop propagation. Right-click = More menu.
- Overflow: DeckEdge (two 4 px card edges at the bottom, top once scrolled). No gradient.

### 2.6 Footer (32)
1 px `border.subtle` top, padding 0 4 0 8, `type.sm`:
- Target known: app glyph 16 + "Pastes into **Outlook**" (`text.secondary`, app name `text.primary` 500).
  Tooltip: "Click or Enter pastes. Shift+click or Shift+Enter copies."
- No target: clipboard 16 + "No target: **click copies**".
- Search with no matches: keycap "Enter" + creates "**query**".
- Right: 1×16 divider + "Open Snipdeck" ghost h28 (sidebar glyph 16 + text); icon-only 28×28 below 300.
- Paste stack running or ≥ 2 rows marked: that bar takes the footer slot (40 tall, same insets),
  4 px above the window edge, never overlapping the list (C13).

### 2.7 Empty states (EmptyState component, one button style: outline h36, 1 px `border.strong`) (C4)
| State | Heading (`type.md` 600) | Body | Actions |
|---|---|---|---|
| List empty | This list is empty | Copy something, then save it here. | Save from clipboard; ghost "New note" |
| Board has no lists | No lists yet | Lists hold your snippets. | New list |
| Search, no match | Nothing matches "q" | Searched every board and list. | Create "q" (+ caption "Adds a text snippet to Board · List"); ghost "Clear search" |
| Loading | skeleton rows at the layout's row height | | |
Icon tile 40×40 radius 8 `bg.hover`, glyph 20 `text.secondary`; block centred, padding 24 24 48.

### 2.8 Toasts
`ink.default` fill, `text.on-ink`, radius 8, h40, inset 12 left/right, bottom 8 above the footer,
`type.sm`, one at a time. Action button h28 padding 0 10, text `blue-300` light / `blue-700` dark (the ink flips)
(contrast ≥ 4.5:1 on ink; palette.py check). Texts: "Moved "Q3 report" to Addresses · Undo",
"Deleted "x" · Undo", "Copied. Paste with Ctrl+V." (no target), SPEC 5.1a elevated-target text.
Timing: the existing `Toast` phases, unchanged.

### 2.9 Tokens used
Light/dark straight from DESIGN.md §2: `bg.app` (chrome), `bg.surface` (list), `bg.hover`,
`bg.selected`, `bg.overlay(-hover)`, `border.subtle/default/strong`, `text.primary/secondary/tertiary`,
`accent.text`, `focus.ring`, `ink.*`, list hues 600/400. New: `kind.{text,template,code,link,sensitive,
image,file}.{tile,glyph}` and `search.mark`. Non-text contrast ≥ 3:1 for every control boundary or
glyph that identifies it: keycaps, outline buttons and chips use `border.strong` (C5).

### 2.10 Keyboard (every key in `panel.slint`'s header comment is kept, unchanged)
| Key | Action |
|---|---|
| type | filters; expands the search when collapsed |
| Up / Down, PageUp / PageDown | move selection (Cards: ± one visual row) |
| Left / Right | Cards only, empty search: ± one tile |
| Enter / Shift+Enter / Alt+Enter | paste / copy / paste as plain text |
| Alt+1-9 | use the Nth row (Cards: reading order) |
| Ctrl+Tab / Ctrl+Shift+Tab | next / previous list (works in Menu switcher mode) |
| Ctrl+F | expand and select the search |
| Menu, Shift+F10 | row context menu (on the More button: Panel menu) |
| Ctrl+K, Ctrl+/, F1 | commands, shortcuts, guide |
| Ctrl+A, Shift+Up/Down | select all, extend |
| Ctrl+D, Ctrl+P, Ctrl+Shift+P | duplicate, pin row, keep on top |
| Ctrl+E, F2 | edit, rename |
| Delete | delete (empty search only) |
| Ctrl+N, Ctrl+Z, Ctrl+Y / Ctrl+Shift+Z, Alt+Up/Down | new note, undo, redo, reorder (existing) |
| Space (hold) | reveal the selected sensitive row (existing) |
| Esc | close menu; else clear and collapse search; else hide the Panel |

### 2.11 Focus order (Tab / Shift+Tab, cyclic)
Board picker, Search (or the field when expanded), Close search, New, Keep on top, More, Close,
tab row (one stop; Left/Right inside), pinned strip (one stop), list (one stop, roving selection),
footer "Open Snipdeck". On open, focus goes to the field if search is expanded or always shown, else to
the list. Row actions are mouse affordances; keys reach them via Ctrl+E, Shift+Enter and Menu.

### 2.12 Narrow and short
- **Width 320** (default): numbers in §2.1/§2.5 hold; title column 230.
- **Width < 300**: New and Open Snipdeck go icon-only; Compact drops the Edit action (in More); Cards
  go to 1 column below 288.
- **Width 260** (min): board picker ≥ 69; title column 170 (130 with actions on line 2, title untouched).
- **Min height** = 40 + tabs 32 (0 with Menu switcher) + 8 + 3 × row + 2 × gap + 8 + 32, clamped to
  ≥ 240: List 296, Compact 240, Cards 240. The pinned strip hides when the list area would show < 2 rows.
  Expanded search at min height shows ≥ 1 full result plus the DeckEdge.

## 3. Settings > Panel > "Panel layout" (3 new, 1 reused)

| Setting | Values (default first) | Rust (`settings.rs`, `#[serde(default)]`) | Slint |
|---|---|---|---|
| Layout | List / Cards / Compact | `panel_layout: PanelLayout { List, Cards, Compact }` kebab | `in property <PanelLayout> layout` (replaces `PanelView view` and `compact-rows`) |
| Search field | Appears when you type / Always shown | `panel_search: PanelSearch { OnType, Always }` | `in property <bool> search-always` |
| Lists | Tabs / Menu | `panel_switcher: PanelSwitcher { Tabs, Menu }` | `in property <bool> lists-in-menu` |
| Quick-use numbers | On / Off | existing `show_numbers` (moved into this group) | existing `show-numbers` |

One `PanelWindow`, one row component: `ItemCard` keeps its `ItemCardVariant { compact, comfortable,
tile }`; `layout` picks the variant (list = comfortable, cards = tile, compact = compact) and the list
container (ListView column vs a 2-column grid built from rows of 2). No second Panel file.
`row_density` stays the Main window's setting (its description already says so); remove the Panel's
unused `compact-rows` path (replace, do not layer).

**Compact** (direction C): rows 36, gap 0, 1 px `border.subtle` dividers, tile 24×24 radius 4 glyph 16,
title then preview inline (`type.sm` secondary, 8 gap); the preview hides under 48 px. Actions overlay
the right end; on hover the preview gives way first, the title only shrinks past it and keeps ≥ 12
characters at 260. Search rows: 52 (title + where line).

**Cards** (direction B): 2 columns at ≥ 288 wide, tile (320 − 16 − 6) / 2 = 149 × 96, gap 6, radius 8,
`bg.surface` + 1 px `border.subtle` + depth 1. Inside, padding 8: type tile 24 + number (top right),
title up to 2 lines `type.base` 500, preview 2 lines `type.sm`. Actions: bottom-right 28×28 trio over the
last preview line. Hover = depth 2. Search uses List rows (where lines need the width).

**Menu switcher**: the tab row goes (−32 px). The header board picker reads "Board · List" (both
truncate, the list name keeps ≥ 6 characters) and opens a Chooser: boards, then the board's lists with
their hue marks, then New list. Recent becomes the Chooser's first entry.

## 4. Settings > Panel > "Opening the Panel"

Reused, unchanged: Panel shortcut (`hotkeys.toggle_panel`, Ctrl+Shift+Y for new installs, see docs/reviews/default-chords.md) and Picker shortcut
(`hotkeys.picker`, Ctrl+Shift+Space): "what the hotkey opens" is already two chords, so no new setting;
Alt+Tab / taskbar (`panel_in_taskbar`); Hide when it loses focus (`panel_hide_on_blur`); Dock edge and
reveal (`dock`, `panel_geometry.edge`); Start with Windows (`autostart`); Picker side (`picker_side`).

New:

| Setting | Values (default first) | Rust | Notes |
|---|---|---|---|
| Tray icon click opens | Panel / Main window | `tray_click: TrayClick { Panel, Main }` | `TrayModel` left-click decision becomes a parameter |
| Panel opens | Where you left it / At the mouse / At the text cursor | `panel_open_at: PanelOpenAt { Last, Mouse, Caret }` | Caret falls back to mouse (`placement::place`, `Anchor::Caret`/`Point`, `picker_side`); disabled with an explanation while docked |
| After a paste or copy | Hide the Panel / Keep the Panel open | `panel_hide_after_use: bool` (true for a new install; a settings file without the key keeps false) | a floating Panel that is not pinned hides after a paste or a copy (it was shown by the hotkey or the tray); a pinned Panel stays, and the dock already collapses after a paste |
| When Windows starts | Stay in the tray / Show the Panel | `panel_start_visible: bool` (false) | only with `autostart`; drives the existing `--hidden` flag |

## 5. Build plan (each slice = one commit, tests first, `scripts/gate.sh --staged` before it)

1. **Tokens and docs.** palette.py: `kind.*.tile/glyph`, `search.mark`, toast action colour; checks
   (glyph ≥ 4.5:1 on tile both themes; mark vs `bg.selected` ≥ 1.3:1; mark text ≥ 4.5:1). Regenerate
   tokens. Amend DESIGN.md §4/§7/§8 and SPEC 5.1 per §1. Test: palette checks run in CI.
2. **Settings model.** Six new fields, enums, `sanitize`, serde defaults; `VERSION` stays 1 (additive).
   Tests: defaults; round-trip; a v1 file without the fields loads defaults; unknown enum value falls
   back to default; `ListView` in the store is left untouched.
3. **List row.** `ItemCard` comfortable variant per §2.5 (tile, actions trio, reveal, masked preview,
   focus-visible ring, quiet fills). Goldens: `panel-light`, `panel-dark` (regenerated),
   `panel-row-hover-light`, `panel-row-selected-sensitive-dark`, `panel-no-target-light`.
4. **Header, search, footer, chrome.** §2.1-2.4, 2.6-2.8; `layout`/`search-always`/`lists-in-menu`
   properties; remove `PanelView` and `compact-rows`. Goldens: `panel-search-light`, `panel-search-dark`,
   `panel-no-results-light/dark` (regenerated), `panel-search-always-light`, `panel-undo-toast-dark`,
   `panel-focus-tab-light`, `panel-pinned-overflow-light` (regenerated).
5. **Compact and Cards.** Goldens: `panel-compact-light`, `panel-compact-dark`, `panel-cards-light`,
   `panel-cards-dark`, `panel-cards-280-light`, `panel-awkward-compact-light` and
   `panel-awkward-comfortable-light` (regenerated).
6. **Menu switcher.** Golden: `panel-lists-menu-light`, `panel-lists-menu-open-light`.
7. **Settings page.** Both groups in `settings.slint`, wired in `settings_binding.rs`. Goldens:
   `settings-panel-layout-light`, `settings-panel-layout-dark`.
8. **Access wiring.** Controller effects for `tray_click`, `panel_open_at`, `panel_hide_after_use`,
   `panel_start_visible` (autostart command line). No new goldens.
9. Sizes: `panel-260-light`, `panel-min-height-light`, `panel-scale-light` (regenerated, 150 %).

Each UI slice gets an independent `ux-auditor` check (measured bounding boxes and contrast from the
rendered goldens); slices 2 and 8 get `reviewer`.

## 6. Tests (by slice; UI tests in `crates/snipdeck-ui/tests/panel*.rs`, app in `controller/tests/`)

- `row_click_pastes_once` (one real click fires `row-picked` once, no expand state); `shift_click_copies`;
  `action_click_does_not_paste` (Copy/Edit/More each fire only their callback).
- `title_width_unchanged_on_hover` (title box width equal hovered vs not, List and Cards; Compact keeps
  ≥ 12 chars at 260); `actions_on_preview_line_only` (action y range inside the preview line).
- `copy_action_hidden_without_target`; `sensitive_reveal_always_visible`; `masked_preview_has_no_tail`.
- `typing_expands_search_with_char`; `close_search_button_clears_and_collapses`;
  `esc_order_menu_then_search_then_hide`; `search_always_has_no_close_button`.
- `search_marks_are_not_underlined` (pixel under the match = `search.mark`, no 1 px line).
- `every_header_key_still_works` (table-driven over the header comment's keys).
- `focus_order_matches_spec`; `focus_ring_only_on_keyboard_focus`.
- `hit_targets_at_least_24` (walk every TouchArea in each golden state; ≥ 24×24, rows ≥ 32).
- `cards_columns_by_width` (2 at 320, 1 at 280); `cards_arrow_keys_move_by_row`.
- `min_height_per_layout` (296 / 240 / 240; −32 with Menu switcher); `pinned_strip_hides_when_short`.
- `chrome_height_at_rest` (104 without pins, 140 with).
- Controller: `tray_click_main_opens_main`; `open_at_caret_falls_back_to_mouse`;
  `open_at_ignored_while_docked`; `hide_after_use_hides`; `start_visible_drops_hidden_flag`;
  `layout_setting_reaches_panel`. Recent ages use the injected clock, never the real one.

## 7. Windows-only (unverified until the owner runs `tools/windows-test/`)

Caret rectangle for "At the text cursor" (UIA/`GetGUIThreadInfo`, same path as the Picker); tray
left-click routing on the real shell; `--hidden` in the autostart entry; taskbar/Alt+Tab entry; Mica on
the chrome; per-monitor DPI at 150 % and 225 %; pasting after `panel_hide_after_use` hides (the Target
must keep focus); IME composition in the expanded field. Everything else is measurable on Linux in the
software-renderer goldens.
