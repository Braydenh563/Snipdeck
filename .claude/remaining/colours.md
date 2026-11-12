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

## Open review items (docs/reviews/colour-tool-reverify.md C1..C24), as of the stop for budget
Done and tested (lib tests): C1 Rust side (HSL/CMYK keep float state, `colours_panel::Memo`), C2, C17, C18, C20, C23, C24 (commands part), Main Colour strip + Picker Paste as (C6, C15 partly).
Slint agent was stopped mid-work: its edits to colours.slint / text_field.slint / item-card.slint / panel.slint / tests/colours_window.rs are in the tree UNTESTED (colours_window.rs had clippy cast errors at ~983 and a dropped `let line =` binding). Re-check each, then finish: C1(d) real Up/Down keys via capture-key-pressed plus a real-key window test, C3 channel letters, C5 window tests, C7/C13 focus after removing the last recent, C8 Save to… focus return, C9 Esc with a SelectField open, C10 Ctrl+Z/Y in the input must not undo the library, C11 format row hover text, C12 surface accessible range, C16 fixed widths at 260px/large text, C19 translucent swatch checker, C21 clear query on open-colours, C22 swatch accessible label, C24 panel_logic Ctrl+Tab test.
Also open: C4 (HSB/CMYK texts drop alpha: document), C14 (Win32 key during a press), #35 goldens (Colours tab light/dark x 100%/150% plus a no-clip measurement), a Slint binding-loop warning in main_window.slint (search field / root layoutinfo, seen in the clippy build log), then merge origin/claude/compassionate-pascal-pod8ux (regenerate goldens), codemap, `gate --full`.

## Blocked
- Eyedropper has never run on Windows; owner must try it (`tools/windows-test/`).
