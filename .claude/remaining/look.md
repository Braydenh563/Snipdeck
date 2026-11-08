# look (shared-look slices 1-2)
## Done
- Slice 1 (1d25498): search_mark.rs; Panel uses it; Picker Entry.marks/tag hit + "Board · List" + 12-dot mask; Main Row.marks; commands::Entry.mark; main_mask(tail).
- Slice 2: ItemCard where-inline, use-action/use-glyph/use-tip, inline-tile, has-menu:false hides only More; Keycap always border-strong (strong prop removed); MenuItem disabled = text-tertiary, radius.md; QuietCardGallery + tests/quiet_card_logic.rs + goldens quiet-cards-*.
## Next
- Slices 3-8 of docs/reviews/shared-look-spec.md (not mine).
## Blocked / notes
- Main has no source for a secret's tail (store keeps none): main_mask(None) = 12 dots.
- Slint PickerRow/MainRow/PaletteRow have no mark fields yet (slices 3/4/6); ItemCard still forces 12 dots for quiet rows (decision 2 "as given" lands with the host slices; fixtures carry a tail).
- Goldens deck-*, main-* drift (focus ring/AA, unrelated to these changes): left at HEAD.
- Settings goldens did not change: settings uses its own private key legend (hotkey_recorder.slint), not Keycap.
- ItemCard `!pick-mode` gate on quiet actions stays until slice 4.
## Unfinished at this commit
- clippy -D warnings on snipdeck-ui (running; last fix: test binding names), rustdoc -D warnings, `python3 scripts/codemap.py`.
