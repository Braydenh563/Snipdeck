# look (shared-look slices 1-4)
## Done
- Slice 1 (1d25498): search_mark.rs; Panel uses it; Picker/Main/palette marks and masks.
- Slice 2 (c26c202): ItemCard where-inline, use-action/use-glyph/use-tip, inline-tile; Keycap always border-strong; menu disabled text-tertiary.
- Slice 3 (7e76158 swept in by a concurrent commit, 806589f): Picker quiet 36px rows, place inline "Board · List", bg-selected fill (no show-focus), Copy + Edit on the selected row, PickerRow mark fields, marks_fields() in search_mark.rs; tests row_height_is_36, selected_row_shows_copy_and_edit, edit_action_requests_edit, titles_share_one_x_for_every_kind, file_row_has_no_extension_text, search_mark_under_match, rows_are_36px_and_the_selected_row_is_a_fill_without_a_ring; goldens picker-* (+ picker-search-light).
- Slice 4 (c0c4bd0): Main list rows quiet (MainRow mark fields, bg-surface item area, list-pad 8, gap 4), Use/Copy/Edit/More, Restore in Trash, actions hidden when >1 selected, action tooltips, kbd-only focus ring, "N matches in Board" status; ItemCard `actions-hidden` replaces the `!pick-mode` gate (Panel passes marked-count > 0). Strings in 6 locales.
## Next
- Slices 5-8 of docs/reviews/shared-look-spec.md (tiles/toasts, toolbar/menus/palette, editors, remove old path).
## Blocked / notes
- Main tooltip "Paste into <app> (Enter)": Main has no target name; reads "Use (Enter)" / "Copy (Enter)".
- "Board · List when results span boards": Main rows carry the list only (one board open), so the board part is empty.
- Main masks still 12 dots (no tail source); ItemCard forces dots in quiet rows.
- Tiles keep the old card look until slice 5 (MainCard non-quiet).
- Goldens main-* toolbar drift at (837,10)-(861,34) is pre-existing AA; left at HEAD where the test tolerates it.
- title_keeps_12_chars_at_scale (Picker) and preview_keeps_128_at_320 not tested separately: the Compact/Comfortable floors are ItemCard's (min-title-w, qr-preview-reserve) and the Panel tests cover them.
