# colours (branch claude/colour-tool)

## Done
- snipdeck-colour maths crate (earlier), settings fields, colour controller (`controller/colour_ctl.rs`) with fixed-clock tests.
- Colours tab UI (`ui/colours.slint`, additive hooks in `panel.slint`), glue in `src/binding/colours.rs`, pure helpers in `src/colours_panel.rs`.
- Eyedropper: platform trait and Win32 implementation (`snipdeck-platform/src/eyedrop.rs`, `windows/eyedrop.rs`), controller side `controller/eyedrop_ctl.rs`, wired in `main.rs` (Windows only).
- Palette commands, Settings toggles, item menu (Open in Colours, Paste as formats), list menu (Copy as CSS variables).
- Window tests `tests/colours_window.rs` (renders to `target/colours-shots`).

## Next
- Swatch tile on Panel cards/rows and Main window rows; Main window detail Colour block.
- Panel keys in the Colours tab: Ctrl+S, Ctrl+Shift+S, Ctrl+I.
- `hotkeys.eyedropper` global chord (skipped; the palette command covers it).
- UI goldens for the tab (the window test writes PNGs, nothing is compared yet).

## Blocked
- Eyedropper has never run on Windows; owner must try it (`tools/windows-test/`).
