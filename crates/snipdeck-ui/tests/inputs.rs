//! Screenshot tests for the input and navigation components (group B), light and dark.
//!
//! Create goldens with `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test inputs`, then look at each PNG.

use slint::ComponentHandle;
use snipdeck_ui::{
    DialogGallery, DialogVariant, EmptyStateGallery, FormGallery, InputsGallery, TabsGallery,
    testing::{Harness, assert_golden},
};

#[path = "support/mod.rs"]
mod support;

#[test]
fn inputs_light() {
    let h = Harness::new(InputsGallery::new, 700, 640);
    assert_golden("inputs-light", &h.render());
}

#[test]
fn inputs_dark() {
    let h = Harness::new(InputsGallery::new, 700, 640);
    h.component.set_dark(true);
    assert_golden("inputs-dark", &h.render());
}

#[test]
fn inputs_ime_composition_keeps_its_end_visible_light() {
    let h = Harness::new(InputsGallery::new, 700, 640);
    h.component.set_narrow_text("invoice".into());
    h.component.invoke_focus_narrow();
    h.component.invoke_set_narrow_cursor(7);
    support::compose(
        h.component.window(),
        "nihongo-tokyo-tokkyo-kyokakyoku-and-more",
    );
    assert_golden("inputs-composing-long-light", &h.render());
}

#[test]
fn tabs_light() {
    let h = Harness::new(TabsGallery::new, 360, 330);
    assert_golden("tabs-light", &h.render());
}

#[test]
fn tabs_dark() {
    let h = Harness::new(TabsGallery::new, 360, 330);
    h.component.set_dark(true);
    assert_golden("tabs-dark", &h.render());
}

#[test]
fn forms_light() {
    let h = Harness::new(FormGallery::new, 700, 680);
    assert_golden("forms-light", &h.render());
}

#[test]
fn forms_dark() {
    let h = Harness::new(FormGallery::new, 700, 680);
    h.component.set_dark(true);
    assert_golden("forms-dark", &h.render());
}

#[test]
fn dialog_light() {
    let h = Harness::new(DialogGallery::new, 420, 300);
    assert_golden("dialog-light", &h.render());
}

#[test]
fn dialog_dark() {
    let h = Harness::new(DialogGallery::new, 420, 300);
    h.component.set_dark(true);
    assert_golden("dialog-dark", &h.render());
}

#[test]
fn dialog_destructive_light() {
    let h = Harness::new(DialogGallery::new, 420, 300);
    h.component.set_variant(DialogVariant::Destructive);
    assert_golden("dialog-destructive-light", &h.render());
}

#[test]
fn dialog_destructive_dark() {
    let h = Harness::new(DialogGallery::new, 420, 300);
    h.component.set_dark(true);
    h.component.set_variant(DialogVariant::Destructive);
    assert_golden("dialog-destructive-dark", &h.render());
}

#[test]
fn empty_states_light() {
    let h = Harness::new(EmptyStateGallery::new, 700, 460);
    assert_golden("empty-light", &h.render());
}

#[test]
fn empty_states_dark() {
    let h = Harness::new(EmptyStateGallery::new, 700, 460);
    h.component.set_dark(true);
    assert_golden("empty-dark", &h.render());
}

// ---------------------------------------------------------------------------------------------
// Measured pixels (light theme). Palette values from design/tokens/palette.json:
// focus.ring #2870D4 (blue-600, 4.6:1; the same blue as the accent), accent #2870D4, border.strong #7C7A76, text.secondary #5D5A57,
// bg.app #F5F3F1, bg.surface #FBFAF7, border.subtle #DEDCD8, teal-600 #048484, teal-400 #59BDBD.
// Gallery geometry: the default field is at x 16..264, y 39..67; the forced-focus one at y 157..185;
// the forced-hover one at y 98..126.
// ---------------------------------------------------------------------------------------------

const FOCUS_RING: [u8; 3] = [0x28, 0x70, 0xD4];
const ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];
const BORDER_STRONG: [u8; 3] = [0x7C, 0x7A, 0x76];
const TEXT_SECONDARY: [u8; 3] = [0x5D, 0x5A, 0x57];
const BG_APP: [u8; 3] = [0xF5, 0xF3, 0xF1];
const BG_SURFACE: [u8; 3] = [0xFB, 0xFA, 0xF7];

#[test]
fn focus_ring_is_2px_with_a_2px_gap_around_a_1px_accent_border() {
    let h = Harness::new(InputsGallery::new, 700, 640);
    let f = h.render();
    // Across the left edge of the forced-focus field, at mid height.
    let row: Vec<_> = (10..18).map(|x| f.pixel(x, 171)).collect();
    assert_eq!(
        row,
        [
            BG_APP, BG_APP, FOCUS_RING, FOCUS_RING, BG_APP, BG_APP, ACCENT, BG_SURFACE
        ],
        "x = 10..18"
    );
    // Down the top edge: ring at y 153-154, gap 155-156, border 157. The box is 28px tall (157..185).
    assert_eq!(f.pixel(140, 154), FOCUS_RING);
    assert_eq!(f.pixel(140, 155), BG_APP);
    assert_eq!(f.pixel(140, 157), ACCENT);
    assert_eq!(f.pixel(140, 184), ACCENT);
    assert_eq!(f.pixel(140, 185), BG_APP);
}

#[test]
fn real_focus_draws_the_same_ring_as_the_forced_state() {
    let h = Harness::new(InputsGallery::new, 700, 640);
    assert_eq!(h.render().pixel(140, 39), BORDER_STRONG, "unfocused border");
    // The headless window is never "active" on its own; focus only shows while it is.
    h.component
        .window()
        .dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
    h.component.invoke_focus_name();
    h.advance(std::time::Duration::from_millis(200));
    let f = h.render();
    assert_eq!(f.pixel(140, 39), ACCENT, "focused border");
    assert_eq!(f.pixel(140, 35), FOCUS_RING);
    assert_eq!(f.pixel(140, 36), FOCUS_RING);
    assert_eq!(f.pixel(140, 37), BG_APP);
}

#[test]
fn real_hover_darkens_the_border_like_the_forced_state() {
    let h = Harness::new(InputsGallery::new, 700, 640);
    let before = h.render();
    assert_eq!(before.pixel(140, 39), BORDER_STRONG);
    assert_eq!(before.pixel(140, 98), TEXT_SECONDARY, "forced hover");
    h.component
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(140.0, 53.0),
        });
    h.advance(std::time::Duration::from_millis(200));
    assert_eq!(h.render().pixel(140, 39), TEXT_SECONDARY, "hovered border");
}

#[test]
fn the_active_tab_underline_is_2px_in_the_list_colour() {
    let teal_600 = [0x04, 0x84, 0x84];
    let h = Harness::new(TabsGallery::new, 360, 330);
    let f = h.render();
    // Bar 1 spans y 39..71; "Personal" (active) is around x 100.
    assert_eq!(f.pixel(100, 68), BG_APP);
    assert_eq!(f.pixel(100, 69), teal_600);
    assert_eq!(f.pixel(100, 70), teal_600);
    // An inactive tab only has the hairline (border.subtle).
    assert_eq!(f.pixel(30, 69), BG_APP);
    assert_eq!(f.pixel(30, 70), [0xDE, 0xDC, 0xD8]);

    h.component.set_dark(true);
    let teal_400 = [0x59, 0xBD, 0xBD];
    let f = h.render();
    assert_eq!(f.pixel(100, 69), teal_400);
    assert_eq!(f.pixel(100, 70), teal_400);
}
