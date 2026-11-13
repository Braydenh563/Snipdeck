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

## Open fixes from the hub re-verify of 48495ee1 (none started; stopped at the 97% usage line)
Write each test first and watch it fail; never weaken a test.
1. (medium) `binding/colours.rs` ~347: `contrast_row` passes `view.other` as the bg and `contrast_ratio` treats bg as opaque. Flatten a translucent second colour over opaque white before the ratio (a pure helper in `colours_panel.rs`, e.g. `over_white`), and say so in the row name via a new `@tr` ("On this colour (on white)": add a `ColourTexts` function next to `other()` in `colours.slint`, then i18n extract/apply in 5 locales). Tests: `#ffffff10` and `transparent`.
2. `controller/colour_ctl.rs` ~252-273: if the item insert fails after `save_colour` made a new list, the empty list stays. Remove it on failure, or keep it tracked for Undo (pick one, comment why). Test with a failing insert.
3. `binding/colours.rs` ~228: the same-name list replacement (`s.lists` assigned before the key compare) has no test. Pin it.
4. `settings.rs` ~1077: a `colours_recent` entry with an invalid hex (`{"hex":"zzz"}`) is kept at load; drop it there. The test asserting `len()==2` contradicts its comment: correct it to the intended 1 (a spec fix).
5. `binding/colours.rs` ~812: `on_hue_moved` clamps only the upper bound. Clamp to [0, 359.999], NaN to 0, like bri and alpha. Add a test.
Then: `scripts/gate.sh --full`, `cargo +1.99.1 clippy --workspace --all-targets --locked -- -D warnings`, push, report head SHA and the gate lines.

## Blocked
- Eyedropper has never run on Windows; owner must try it (`tools/windows-test/`).
