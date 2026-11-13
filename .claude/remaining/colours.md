# colours (branch claude/colour-tool)

## Done
- snipdeck-colour maths crate (earlier), settings fields, colour controller (`controller/colour_ctl.rs`) with fixed-clock tests.
- Colours tab UI (`ui/colours.slint`, additive hooks in `panel.slint`), glue in `src/binding/colours.rs`, pure helpers in `src/colours_panel.rs`.
- Eyedropper: platform trait and Win32 implementation (`snipdeck-platform/src/eyedrop.rs`, `windows/eyedrop.rs`), controller side `controller/eyedrop_ctl.rs`, wired in `main.rs` (Windows only).
- Palette commands, Settings toggles, item menu (Open in Colours, Paste as formats), list menu (Copy as CSS variables).
- Window tests `tests/colours_window.rs` (renders to `target/colours-shots`).

## Next
- Merge origin/claude/compassionate-pascal-pod8ux (dock redesign; every drawn Text needs `font-family: Fonts.sans`), regenerate goldens, i18n extract/check, codemap, `gate --full`, clippy 1.99.1.
- Not done (review #33): the global `hotkeys.eyedropper` chord / `HotkeyAction::Eyedropper` (D10); "pick from anywhere" needs the Panel or the palette first. The eyedropper always hides and restores the Panel (see `eyedrop_ctl.rs`).
- Not done: a per-swatch Ctrl+S (a swatch saves through its menu; the Panel's Ctrl+S saves the current colour).
- Not done (C15 leftover): the card swatch tile is a passive image, not a button; reachable through the item menu.
- Scroll-into-view on focus: chip grids and the format list only, not the sliders, selects or buttons.
- C4, by design: the HSB and CMYK format texts have no alpha part, so they describe the opaque colour (alpha shows in HEX/RGB/HSL/OKLCH).
- C14 deferred: a Win32 key press during an eyedropper press leaves the overlay destroyed while the mouse button is still held. Narrow, and not testable without Windows; do it with the Windows run of `tools/windows-test/`.
- Windows-only and never run: the Win32 eyedropper (button-up capture, HDR bit logic, the wizard in `tools/windows-test/`).

## Review items C1..C24 (docs/reviews/colour-tool-reverify.md)
All done and tested except C14 (above), C4 (documented) and the C15 leftover. Slint side: window tests in `tests/colours_window.rs` (40, including the Colours-tab goldens `colours-{light,dark}-x{100,150}` and a no-clip measurement; the harness now sets the text scale through `text_scale::apply`, since a bare `SystemLook` write is overwritten on redraw) and `tests/panel_logic.rs`. The main_window.slint binding loop is gone (`tall-enough` property set from `changed height`).

## Blocked
- Eyedropper has never run on Windows; owner must try it (`tools/windows-test/`).
