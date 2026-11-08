# look (shared-look slices 1-2)
## Done
- Slice 1: search_mark.rs (marks, title_mark, preview_mark, word_mark); Panel uses it; Picker Entry.marks + "Board · List" + 12-dot mask; Main Row.marks; commands::Entry.mark; main_mask(tail).
## Next
- Slice 2: ItemCard/Keycap/MenuItem props, tests, goldens.
## Blocked / notes
- Main has no source for a secret's tail (store keeps none): main_mask(None) = 12 dots. Slint structs (PickerRow/MainRow/PaletteRow) not yet given mark fields: slices 3/4/6.
