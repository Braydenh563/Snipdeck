//! Screenshot tests for the Picker window in the states the design calls out (light, dark, 225% text, empty,
//! no results, long titles, scrolled), plus pixel measurements of the layout claims in SPEC 5.2 and DESIGN.md.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test picker`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

use slint::{ComponentHandle, LogicalSize};
use snipdeck_ui::{
    PickerScope, PickerWindow, SystemLook,
    testing::{Frame, Harness, assert_golden},
};

#[path = "support/picker_fixture.rs"]
mod fixture;

// Palette values from design/tokens/palette.json.
const OVERLAY_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];
const OVERLAY_DARK: [u8; 3] = [0x20, 0x1E, 0x1B];
const BORDER_SUBTLE_LIGHT: [u8; 3] = [0xDE, 0xDC, 0xD8];
const BORDER_DEFAULT_LIGHT: [u8; 3] = [0xC6, 0xC4, 0xC0];
const BORDER_DEFAULT_DARK: [u8; 3] = [0x2D, 0x2B, 0x28];
const ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];

const WIDTH: u32 = 560;

/// A populated Picker sized to its own content, as the host sizes the window.
fn picker(dark: bool) -> Harness<PickerWindow> {
    let h = Harness::new(PickerWindow::new, WIDTH, 800);
    h.component.set_dark(dark);
    fixture::populate(&h.component);
    h
}

fn fit(h: &Harness<PickerWindow>) {
    // The layout settles over a couple of frames (row heights follow the text size).
    h.component.window().request_redraw();
    let _ = h.render_settled();
    let height = h.component.get_content_height();
    h.component
        .window()
        .set_size(LogicalSize::new(h.component.window().size().width as f32, height));
}

fn render(h: &Harness<PickerWindow>) -> Frame {
    fit(h);
    // Nothing may have changed since the last frame: ask for one anyway.
    h.component.window().request_redraw();
    h.render_settled()
}

#[test]
fn picker_light() {
    let h = picker(false);
    h.component.set_selected_index(1);
    assert_golden("picker-light", &render(&h));
}

#[test]
fn picker_dark() {
    let h = picker(true);
    h.component.set_selected_index(1);
    assert_golden("picker-dark", &render(&h));
}

#[test]
fn picker_copy_route_with_a_query_and_the_board_scope() {
    let h = picker(false);
    h.component.set_target_name("".into());
    h.component.set_scope(PickerScope::Board);
    h.component.set_query("re".into());
    assert_golden("picker-copy-board-light", &render(&h));
}

#[test]
fn picker_at_225_percent_text_size() {
    let h = Harness::new(PickerWindow::new, 760, 1200);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    fixture::populate(&h.component);
    h.component.set_selected_index(1);
    assert_golden("picker-scale-light", &render(&h));
}

#[test]
fn picker_empty_and_no_results_and_nothing_pinned() {
    let h = picker(false);
    fixture::set_rows(&h.component, Vec::new());
    assert_golden("picker-empty-light", &render(&h));

    h.component.set_query("zzz".into());
    assert_golden("picker-no-results-light", &render(&h));
    drop(h);

    let dark = picker(true);
    fixture::set_rows(&dark.component, Vec::new());
    dark.component.set_query("zzz".into());
    assert_golden("picker-no-results-dark", &render(&dark));
    drop(dark);

    let pinned = picker(false);
    fixture::set_rows(&pinned.component, Vec::new());
    pinned.component.set_scope(PickerScope::Pinned);
    assert_golden("picker-nothing-pinned-light", &render(&pinned));
}

#[test]
fn picker_long_titles_and_breadcrumbs_in_a_narrow_window() {
    let h = Harness::new(PickerWindow::new, 380, 800);
    fixture::populate(&h.component);
    fixture::set_rows(&h.component, fixture::awkward_rows());
    h.component.set_query("w".into());
    assert_golden("picker-awkward-light", &render(&h));
}

#[test]
fn picker_with_many_results_shows_nine_rows_and_scrolls_the_selection_into_view() {
    let h = picker(false);
    fixture::set_rows(&h.component, fixture::numbered_rows(30));
    let _ = render(&h);
    h.component.set_selected_index(14);
    let frame = render(&h);
    let (visible, scroll) = (9.0 * 28.0, h.component.get_scroll_y());
    assert!(
        scroll < -(15.0 * 28.0 - visible) + 1.0 && scroll > -15.0 * 28.0,
        "row 15 is the last visible one: scroll {scroll}"
    );
    assert_golden("picker-scrolled-light", &frame);
}

// ---- Measurements ----------------------------------------------------------------------------

// Layout of the populated Picker at 560 wide (ui/picker.slint): the search area is 44px (SPEC 5.2); the scope
// chips 24px + 8px below; then a hairline, 4px, the rows (28px each), 4px, and the 32px footer.
const SEARCH_H: u32 = 44;
const CHIPS_H: u32 = 32;
const LIST_TOP: u32 = SEARCH_H + CHIPS_H + 4;
const LIST_X: u32 = 8;
const ROW_H: u32 = 28;
const FOOTER_H: u32 = 32;

#[test]
fn the_window_fits_its_content_and_is_outlined_with_a_1px_default_border() {
    for (dark, outline, overlay) in [
        (false, BORDER_DEFAULT_LIGHT, OVERLAY_LIGHT),
        (true, BORDER_DEFAULT_DARK, OVERLAY_DARK),
    ] {
        let h = picker(dark);
        let f = render(&h);
        assert_eq!(
            f.height,
            SEARCH_H + CHIPS_H + 4 + 7 * ROW_H + 4 + FOOTER_H,
            "44 + 32 + 4 + 7 rows + 4 + 32"
        );
        for (x, y) in [(0, 100), (WIDTH - 1, 100), (300, 0), (300, f.height - 1)] {
            assert_eq!(f.pixel(x, y), outline, "outline at ({x},{y}) dark={dark}");
        }
        assert_eq!(
            f.pixel(1, 100),
            overlay,
            "popover surface inside the outline (dark={dark})"
        );
    }
}

#[test]
fn nine_rows_fit_and_the_tenth_scrolls() {
    let h = picker(false);
    fixture::set_rows(&h.component, fixture::numbered_rows(12));
    let _ = render(&h);
    assert_eq!(
        h.component.get_content_height(),
        (SEARCH_H + CHIPS_H + 4 + 9 * ROW_H + 4 + FOOTER_H) as f32,
        "up to nine rows are visible"
    );
}

#[test]
fn the_footer_is_32px_with_a_subtle_hairline_on_top() {
    let h = picker(false);
    let f = render(&h);
    let top = f.height - FOOTER_H;
    // Right of the hints, clear of glyphs.
    let x = WIDTH - 40;
    assert_eq!(f.pixel(x, top), BORDER_SUBTLE_LIGHT, "footer hairline");
    assert_eq!(f.pixel(x, top + 1), OVERLAY_LIGHT, "footer surface below it");
    assert_ne!(f.pixel(x, top - 1), BORDER_SUBTLE_LIGHT, "and not above it");
}

#[test]
fn rows_are_28px_and_the_selected_row_has_an_accent_border() {
    let h = picker(false);
    h.component.set_selected_index(1);
    let f = render(&h);
    let (top, bottom) = (LIST_TOP + ROW_H, LIST_TOP + 2 * ROW_H - 1);
    for (x, y, what) in [
        (300, top, "top edge"),
        (300, bottom, "bottom edge"),
        (LIST_X, top + ROW_H / 2, "left edge"),
    ] {
        assert_eq!(f.pixel(x, y), ACCENT, "selected row {what}");
    }
    assert_ne!(f.pixel(300, top - 1), ACCENT, "nothing above the outline");
    assert_ne!(f.pixel(300, bottom + 1), ACCENT, "nothing below the outline");
    assert_ne!(f.pixel(LIST_X, LIST_TOP + 14), ACCENT, "row 1 has no outline");
}

#[test]
fn the_selected_scope_chip_has_an_accent_border_and_the_others_do_not() {
    let h = picker(false);
    let f = render(&h);
    // Chip row: y 44..68 (24px chips). The first chip starts 8px in; its top edge is the accent.
    assert_eq!(f.pixel(12, SEARCH_H), ACCENT, "All is selected");
    // "This board" starts after the All chip and 4px of spacing: find it by scanning the top edge.
    let accents: Vec<u32> = (0..WIDTH)
        .filter(|x| f.pixel(*x, SEARCH_H) == ACCENT)
        .collect();
    assert!(
        accents.iter().all(|x| *x < 80),
        "only the first chip is outlined in accent: {accents:?}"
    );
    h.component.set_scope(PickerScope::Pinned);
    let f = render(&h);
    let accents: Vec<u32> = (0..WIDTH)
        .filter(|x| f.pixel(*x, SEARCH_H) == ACCENT)
        .collect();
    assert!(
        accents.iter().all(|x| *x > 100),
        "the Pinned chip is outlined now: {accents:?}"
    );
    assert!(!accents.is_empty(), "and it is outlined");
}

#[test]
fn the_search_field_sits_in_a_44px_area_with_a_28px_box() {
    let h = picker(false);
    let f = render(&h);
    // Box top edge at y = 8 (border-strong), bottom at 8 + 28 - 1.
    let top = f.pixel(300, 8);
    assert_ne!(top, OVERLAY_LIGHT, "box top edge at y=8");
    assert_eq!(f.pixel(300, 7), OVERLAY_LIGHT, "padding above");
    assert_ne!(f.pixel(300, 8 + 27), OVERLAY_LIGHT, "box bottom edge at y=35");
    assert_eq!(f.pixel(300, 8 + 28), OVERLAY_LIGHT, "padding below");
}
