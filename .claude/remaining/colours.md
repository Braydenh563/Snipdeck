# colours (branch claude/colour-tool)

## Done
- snipdeck-colour maths crate (earlier), settings fields, colour controller (`controller/colour_ctl.rs`) with fixed-clock tests.
- Colours tab UI (`ui/colours.slint`, additive hooks in `panel.slint`), glue in `src/binding/colours.rs`, pure helpers in `src/colours_panel.rs`.
- Eyedropper: platform trait and Win32 implementation (`snipdeck-platform/src/eyedrop.rs`, `windows/eyedrop.rs`), controller side `controller/eyedrop_ctl.rs`, wired in `main.rs` (Windows only).
- Palette commands, Settings toggles, item menu (Open in Colours, Paste as formats), list menu (Copy as CSS variables).
- Window tests `tests/colours_window.rs` (renders to `target/colours-shots`).

## Next
- Not done (review #33): the global `hotkeys.eyedropper` chord / `HotkeyAction::Eyedropper` (D10); "pick from anywhere" needs the Panel or the palette first. The eyedropper always hides and restores the Panel (see the comment in `eyedrop_ctl.rs`).
- Not done: a per-swatch Ctrl+S (the Panel's Ctrl+S saves the current colour first; a swatch saves through its menu).
- Not done: the R/G/B/C/M... channel letters are spoken (TextField `label`) but not drawn.
- Scroll-into-view on focus is wired for the chip grids and the format list only, not the sliders, selects or buttons.
- Windows-only and never run: the Win32 eyedropper (button-up capture, HDR bit logic, the wizard in `tools/windows-test/`).

## Blocked
- Eyedropper has never run on Windows; owner must try it (`tools/windows-test/`).
