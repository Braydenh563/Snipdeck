# Shared look: build spec (2026-11-07)

Status: ready to build. Carries the Panel v2 look (`docs/reviews/panel-v2-spec.md`, cited as P§) to the Main
window, Picker, command palette, editors, menus and Settings. Inputs: `docs/reviews/ux-gap-audit.md` §2 and §5
items 4, 6, 7, 12, 17 (cited as A4 ...), DESIGN.md §2-§5, `ui/theme.slint`, and the goldens `main-light`,
`main-tile-light`, `main-toast-dark`, `picker-light`, `panel-search-light`, `panel-cards-light`,
`item-editor-text-light`, `settings-light`. Sizes are logical px at 100 %; colours are `Theme.*` names.

## 0. Starting point (measured in code, 2026-11-07)

- Every Panel v2 token already exists in `theme.slint`: `kind-{text,template,code,link,sensitive,image,file}-{tile,glyph}`,
  `search-mark`, `search-mark-text`, `toast-action`, `ink`, `text-on-ink`, `focus-ring`. **No new tokens.**
- `ItemCard` (`components/item-card.slint`) already draws the quiet row, the type tile, title/preview marks and the
  where line behind `quiet: true`. Only `panel.slint` sets it; `main_window.slint` (`MainCard`, 3 uses) and
  `picker.slint` (1 use) still draw the old row (bare glyph, no glyph for text, "PDF" text + icon, kebab, `UseLabel`).
- Marks are computed in `snipdeck-app/src/panel.rs` (`title_mark`, `preview_mark`) and only for the Panel.
- `Toast` has an `ink` flag; only the Panel sets it. Main's toast is the old neutral one with an info icon.
- `MenuItem` shows disabled with `opacity: 0.45` (P§2.1 wants `text-tertiary`, no opacity) and `Radius.sm` (DESIGN
  §5 says menu item = `radius.md` 6). `Keycap` uses `border-default` unless `strong` is set (P§2.9 / C5 wants
  `border-strong` for ≥ 3:1).

## 1. Decisions (written once, with reasons)

1. **One row component, quiet everywhere.** Main and Picker set `quiet: true`; slice 8 then deletes `quiet` and the
   old path (`UseLabel`, the gutter rule, `has-glyph`/`has-ext`, the non-quiet kebab). Reason: replace, do not layer;
   three looks of one object is the audit's main complaint (A4, A6).
2. **Masking moves to the hosts.** `ItemCard` shows `masked-preview` as given; `panel_binding`/`picker_binding`
   send twelve dots, `main_binding` sends `•••• 7f3a`. Reason: the Picker opens over the app being shared (A4), Main
   keeps the tail (DESIGN §4), and a rule in Rust is testable in `*_logic` without pixels.
3. **Marks are shared Rust.** `title_mark`/`preview_mark` and the "tag: x" hit move from `panel.rs` to a new
   `snipdeck-app/src/search_mark.rs`; Panel, Picker, Main and the palette call it. Reason: one matcher, one test set.
4. **Picker rows = Panel Compact rows with the place inline.** 36 tall, tile 24, title then preview inline, and the
   list label ("Board · List", 6 px hue square, `type.xs`) right-aligned on the same line (new `where-inline: true`).
   Reason: the Picker is 560 wide and shows 9 rows; a where line (52 per row) would make it 468 + chrome tall.
   9 × 36 = 324 (was 9 × 28 = 252): accepted for 24 px tiles and ≥ 32 hit rows.
5. **Picker selection is a fill, not a ring.** Selected row = `bg-selected`; no `show-focus` ring. The keyboard
   lives in the search field, which shows the `focus-ring` (same model as the Panel). Reason: the always-on 2 px
   ring reads as focus where there is none, and differs from the Panel.
6. **Main click still selects; Use becomes a visible action.** Main keeps `pick-mode` (click selects, Enter or
   double-click uses). Row actions = **Use, Copy, Edit, More** (Panel trio plus Use first). Use glyph
   `arrow-bend-down-left`, tooltip "Paste into Outlook (Enter)" / "Copy (Enter)" with no target; in Trash it is
   Restore (`arrow-counter-clockwise`). Copy hides when Use already copies (P§2.5 rule). Reason: A6 "no visible
   Use"; Panel decision 3 (click = paste) does not fit a manager whose click must select for bulk actions.
7. **Main tiles put the type tile inline with the title** when the card is ≥ 200 wide (Main); Panel Cards (149) keep
   the tile on top. Card height fits its content; a grid row takes its tallest card, min 96. Reason: A17 (half-empty
   150 px cards, glyph above title).
8. **Keycaps are always `border-strong`.** `Keycap.strong` is removed. Reason: C5 contrast; one look.
9. **Menus and the palette share one row**: h32 (`Space.s8`), radius 6, 16 icon column, keycaps right. Disabled =
   `text-tertiary` text and glyph, no opacity. Highlight = `bg-overlay-hover`. Menus (keyboard inside the menu) draw
   the 2 px `focus-ring` on the key-highlighted row only, never on hover; the palette (keyboard in its field) draws
   no row ring. Reason: P§2.1 disabled rule, DESIGN §5 radius, same focus model as Picker.
10. **Editors: Kind is visible at rest, Title stays under More options.** Reason: Kind changes how content is treated
    (fields, mono, masking); Title is optional (the content is the fallback).
11. **All toasts are ink toasts**; `Toast.ink` is removed after Main and `toast_window.slint` adopt it.
12. **Settings changes only through shared parts** (keycaps, menus/select popups, focus ring, toasts). Settings
    search and copy fixes are A15 (queue NEW), not this spec.

## 2. Picker (`picker.slint`, `picker_binding.rs`, `picker.rs`)

**Changes**
- Row: `ItemCard { quiet: true; variant: compact; where-inline: true; has-menu: false; }`, height 36
  (`max(Space.s8 + Space.s1, title-h + 2 * Space.s2)`, as `qc-h`). Number cell 16 + 4 gap, tile 24 radius 4
  (`kind-*-tile` / `kind-*-glyph`, image = 24 crop with 1 px `border-subtle`), 8 gap, title `type.base` 500
  `text-primary`, 8 gap, preview `type.sm` `text-secondary` (mono for code/link/sensitive), list label right.
  Title x is the same for every kind (fixes the 24 px ragged start, A4). File rows: no "PDF" text before the tile.
- Separator: `"Board · List"` (U+00B7 with spaces), built in `picker_binding.rs`, not in `.slint`.
- Sensitive: `••••••••••••` (decision 2); eye button as P§2.5 (Space hold reveals, if the Picker already supports it;
  otherwise the eye is not drawn and the hint "Hidden. Open it in the Panel to reveal." stays as tooltip).
- Marks: title and preview spans on `search-mark` with `search-mark-text`, radius 3, no underline; tag hits show
  "tag: x". Delete the "Limitation: not highlighted" header note.
- Selected row (and hover): `bg-selected` / `bg-hover`; actions **Copy, Edit** (28 ghost in 32 hit) at the right end,
  the list label moves left of them, the preview gives way first; no More. Edit fires the existing Ctrl+E path.
- Space order at the right end of a row: `[preview flex][16 gap][list label ≤ 40 % of row][6][actions]`; the label
  hides below 48 px, the title keeps ≥ 12 characters (DESIGN §8).

**Stays**: 560 width, search field (h36 radius 6, 2 px `focus-ring`), scope chips, footer key hints, keyboard table
in the header comment, placement rules.

## 3. Main window (`main_window.slint`, `main_binding.rs`, `main_window.rs`)

**List rows (Comfortable / Compact)**
- Item area fill `bg-surface`; list padding 8; rows 4 apart (Comfortable) / 0 with no dividers (Compact), radius 8,
  no outline (the 1 px `border-subtle` card outline goes). Rows: P§2.5 (Comfortable 56, tile 32, gap 10) and P§3
  Compact (36, tile 24). No number cell (`numbered: false` stays).
- Mixed views (All items, Pinned, Recent, search): where line under the preview, 6 px hue square + "List"
  (`type.xs` `text-secondary`), or "Board · List" when results span boards; row 68 / Compact 52. The host fills
  `list-name` in those views (A6 "doesn't say which list").
- Actions (decision 6) on the preview line, hover or selected: group 4 × 32 + 6 = 134; at the 320 minimum the
  preview keeps ≥ 128. Sensitive: eye always shown, actions left of it; mask `•••• 7f3a`.
- States: hover `bg-hover`, selected `bg-selected` (multi-select uses the same fill), keyboard focus = 2 px inset
  `focus-ring` on focus-visible only (not after a mouse click). Card pull unchanged.
- Search: marks as §2; the status line (bottom, `type.sm`) reads "3 matches in Work" / "No matches in Work".

**Tiles** (decision 7): `variant: tile; quiet: true; inline-tile: true`. Padding 8; row 1 = tile 24 + 8 + title
(`type.base` 500, up to 2 lines); preview `type.sm` up to 3 lines starting at x = 40; action group bottom-right over
the last preview line; hover depth 2 (`CardShadow`). Card = `bg-surface` + 1 px `border-subtle` + depth 1, radius 8,
gap 6. Height = content (min 96); empty half-cards gone.

**Toolbar** (A6): order stays (sidebar, undo, redo | commands, view | view modes | New item). Changes: Commands =
ghost h32 "Commands" `type.base` 500 + Keycap "Ctrl" "K"; View = ghost h32 "View" + caret-down 16 (eye glyph goes).
Below 560 px item-area width both go icon-only (Commands = `keyboard`, View = `rows`) with the existing tooltips.
Search field: the shared field at h32 (toolbar control height), radius 6, glyph 16 `text-secondary`, 2 px
`focus-ring` while focused, 24 × 24 clear (x) when it holds text.

**Empty states** (A12): All items empty = EmptyState "No snippets yet" / "Save what you paste often." / outline
"Save from clipboard", ghost "New item", ghost "Import…". List empty and Trash empty keep their text, one outline
button style (P§2.7).

**Toasts**: ink toast (P§2.8), h40 radius 8, centred, 8 above the status line, max width 480, action in
`toast-action`; kind icon only for success/warning.

**Stays**: 3-pane layout, sidebar (`SidebarItem`), headings (`type.lg` 600), selection model, bulk bar, keyboard
table, `•••• 7f3a`.

## 4. Command palette (`components/command-palette.slint`, `palette.rs`)

Field = the Picker field (h36, radius 6, glyph, `focus-ring`). Rows = the shared menu row (decision 9): h32, radius 6,
16 icon column (commands without an icon keep the column), label `type.base` `text-primary` with the match on
`search-mark`, keycaps right (`border-strong`). Highlight `bg-overlay-hover`, no row ring. No match = EmptyState
"No commands match "q"". `PaletteRow` gains `mark-before`, `mark-text` from `search_mark`. Stays: scrim 40 %, size,
footer live region, wrap-around keys.

## 5. Menus (`components/menu.slint`, used by Panel, Main, Settings select popups)

`MenuItem`: h32, radius 6 (`Radius.md`), padding 0 8, icon 16 + 8, label `type.base`, keycaps right; disabled =
`text-tertiary` (remove `opacity: 0.45`), `accessible-enabled: false` kept. Highlight `bg-overlay-hover`; key
highlight adds the 2 px `focus-ring` (hover alone does not). Container: `bg-overlay`, 1 px `border-subtle`, radius 8,
padding 4, depth 2; separator 1 px `border-subtle`, margin 4 0. Checked items: `check` 16 in `accent-text` in the
icon column.

## 6. Editors (`editors.slint`, `editor_binding.rs`, `item_editor.rs`)

- **Fit content** (A7): the window opens at form height + footer (already intended, ≤ 700); goldens render at
  `preferred-height`, not a fixed 760. On Windows the fit is unverified until the owner runs `tools/windows-test/`.
- **Kind row at rest** (decision 10), above Content: label "Kind" `type.sm` 500; chips h32, radius 6, padding
  0 8 0 4, 1 px `border-default`, each = tile 24 radius 4 (`kind-*-tile`/`-glyph`) + name `type.sm` 500; selected =
  `bg-selected` + 1 px `accent`; radio group (arrows move, Space picks). Image and File items show one read-only chip.
- **More options** = disclosure row, not a box: h32, full width, transparent, hover `bg-hover` radius 6, caret-right
  16 (caret-down when open) + "More options" `type.base` 500 + summary "Title, colour, tags, privacy" `type.sm`
  `text-secondary`; `accessible-expanded`.
- Fields: h36 radius 6 inputs, 2 px `focus-ring` while focused (TextField already); keycaps `border-strong`.
- **Stays**: footer (Move to Trash, Cancel, Save), field order, validation, template chips (A5 caret insert is NEW).
  List editor, board editor, tag manager change only through shared parts (menus, keycaps, toasts).

## 7. Settings (`settings.slint`, `settings_binding.rs`)

Changes only through shared parts: hotkey keycaps `border-strong` (slice 2), select popups use the §5 row (slice
6), toasts ink (slice 5). Stays: layout, section headings, hairlines, Segmented, copy (A15 is NEW).

## 8. Slice plan (one commit each, tests first, `scripts/gate.sh --staged` before it)

Order: 1 → 2 first (shared). Then 3, 4 and 7 touch different `.slint` files and can run in parallel (at most two
cargo agents on one target dir); 5 then 6 follow 4 (all three edit `main_window.slint`). 8 last. Each UI slice gets a `ux-auditor` check (bounding boxes and contrast
measured from the regenerated goldens); slices 1 and 8 get `reviewer`. Regenerate with
`UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test <file>` and look at every PNG.

1. **Shared search marks and masks (Rust only).** New `snipdeck-app/src/search_mark.rs` (moved from `panel.rs`,
   which imports it); `picker_binding.rs` sends twelve dots and "Board · List"; `main_binding.rs` keeps the tail and
   fills marks; `palette.rs` fills label marks. Tests first (unit, in `search_mark.rs` and the bindings):
   `title_mark_case_insensitive`, `preview_mark_first_line_only`, `tag_hit_when_only_tag_matches`,
   `no_mark_for_sensitive`, `picker_mask_has_no_tail`, `main_mask_keeps_tail`, `picker_place_uses_middle_dot`,
   `palette_marks_label_prefix`. No goldens (Panel goldens must stay byte-identical: run `--test panel`).
2. **ItemCard and Keycap props.** `item-card.slint`: `where-inline`, `use-action` (+ `use-glyph`, `use-tip`,
   callback reuses `use`), `inline-tile`, `has-menu: false` hides More in quiet rows; `keycap.slint`: drop `strong`,
   always `border-strong`. Tests first in `tests/primitives.rs` / `primitives_logic.rs`:
   `where_inline_label_on_title_line`, `use_action_fires_use_only`, `no_more_action_without_menu`,
   `inline_tile_title_x_is_40`, `keycap_border_is_strong` (pixel ≥ 3:1 vs fill). Goldens regenerated: `surfaces-*`,
   `inputs-light`, `settings-light/dark`, `settings-recording-light`, `panel-search-light` (Esc keycap) only.
3. **Picker quiet rows** (§2). `picker.slint`. Tests first in `tests/picker.rs` / `picker_logic.rs`:
   `titles_share_one_x_for_every_kind`, `file_row_has_no_extension_text`, `selected_row_shows_copy_and_edit`,
   `edit_action_requests_edit`, `selected_row_has_no_focus_ring`, `row_height_is_36`, `search_mark_under_match`
   (pixel = `search-mark`, no 1 px underline), `title_keeps_12_chars_at_scale`, `hit_targets_at_least_24`.
   Goldens regenerated: `picker-light`, `picker-dark`, `picker-awkward-light`, `picker-copy-board-light`,
   `picker-scale-light`, `picker-scrolled-light`; new `picker-search-light`.
4. **Main list rows** (§3 list rows, search status). `main_window.slint`, `main_window.rs`. Tests first in
   `tests/main_window.rs` / `main_window_logic.rs`: `click_selects_not_uses`, `use_action_uses_once`,
   `copy_hidden_when_use_copies`, `trash_use_action_restores`, `where_line_in_mixed_views_only`,
   `focus_ring_only_on_keyboard_focus`, `title_width_unchanged_on_hover`, `actions_on_preview_line_only`,
   `preview_keeps_128_at_320`, `search_status_counts`. Goldens regenerated: `main-light`, `main-dark`,
   `main-compact-light`, `main-bulk-light`, `main-scrolled-light`, `main-awkward-comfortable-light`,
   `main-awkward-compact-light`, `main-no-results-light`, `main-scale-light`, `main-rail-light`; new
   `main-row-hover-light`, `main-search-light`, `main-search-dark`.
5. **Main tiles, empty states, ink toasts.** `main_window.slint`, `panel.slint` (hide "+ New item" while the list is
   empty, A12), `toast_window.slint`, `components/feedback.slint` (remove `ink`). Tests first: `tile_height_fits_content`,
   `tile_glyph_inline_with_title`, `empty_all_offers_clipboard_new_import`, `panel_empty_has_two_create_actions`,
   `toast_fill_is_ink` (pixel), `toast_action_contrast_4_5`. Goldens regenerated: `main-tile-light/dark`,
   `main-awkward-tile-light`, `main-loading-tile-light`, `main-empty-all-light`, `main-empty-list-dark`,
   `main-empty-trash-light`, `main-toast-dark`, `panel-empty-light`; new `main-tile-hover-light`.
6. **Toolbar, menus, palette.** `main_window.slint` toolbar + search field, `components/menu.slint`,
   `components/command-palette.slint`, `form.slint` (SelectField popup rows). Run after 5 (same file). Tests first:
   `toolbar_commands_and_view_are_labelled`, `toolbar_icon_only_below_560`, `menu_disabled_uses_tertiary_not_opacity`,
   `menu_ring_on_key_highlight_only`, `menu_item_radius_6_height_32`, `palette_marks_match`,
   `palette_no_match_empty_state`. Goldens regenerated: `menu-light/dark`, `panel-app-menu-light/dark`,
   `panel-lists-menu-open-light`, `formctl-select-open-light`, `main-light` (toolbar); new `palette-light`,
   `palette-dark`.
7. **Editors** (§6). `editors.slint`, `editor_binding.rs`. Tests first in `tests/editors.rs` /
   `editors_logic.rs`: `editor_height_is_form_height` (≤ 700, no blank band > 24 px above the footer),
   `kind_chips_visible_at_rest`, `kind_chips_arrow_keys`, `more_options_is_disclosure` (`accessible-expanded`, no
   border), `image_kind_chip_read_only`. Goldens regenerated: every `item-editor-*`, `list-editor-*`,
   `board-editor-*`, `tag-manager-*`.
8. **Remove the old row path; docs.** `item-card.slint`: delete `quiet` and every `!root.q` branch (`UseLabel`,
   gutter rule, `has-glyph`, `has-ext`, the old kebab), `deck` gallery updates. Amend DESIGN.md §4 (drop "Main
   window: unchanged by Panel v2"; Main actions = Use/Copy/Edit/More; Picker row; Keycap; menu row), §5 Picker
   (36 rows). Tests: every existing golden test passes with no regeneration except `deck-*`, `surfaces-*` if the
   gallery shows the old row; `grep -n 'quiet' crates/snipdeck-ui/ui` returns nothing. Coordinator runs
   `scripts/gate.sh --full`.

## 9. Windows-only (unverified until the owner runs `tools/windows-test/`)

Editor window fitting its height at 100/150/225 % DPI; Picker height at the caret with 36 rows near the screen
bottom (flip rule); toolbar label/icon-only switch at real Main widths; ink toast over Mica.
