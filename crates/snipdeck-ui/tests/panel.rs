//! Screenshot tests for the Panel window in the states the design calls out (light, dark, 225% text,
//! empty, no results, long titles, 20+ rows scrolled, toast), plus pixel measurements of the
//! layout claims in DESIGN.md §5 and §6.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test panel`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

use slint::{ComponentHandle, ModelRc, VecModel};
use snipdeck_ui::{
    PanelView, PanelWindow, SystemLook, ToastKind,
    testing::{Frame, Harness, assert_golden},
};

#[path = "support/panel_fixture.rs"]
mod fixture;

const SIZE: (u32, u32) = (320, 560);

// Palette values from design/tokens/palette.json.
const APP_LIGHT: [u8; 3] = [0xF5, 0xF3, 0xF1];
const APP_DARK: [u8; 3] = [0x11, 0x0F, 0x0D];
const SURFACE_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];
const BORDER_SUBTLE_LIGHT: [u8; 3] = [0xDE, 0xDC, 0xD8];
const BORDER_SUBTLE_DARK: [u8; 3] = [0x20, 0x1E, 0x1B];
const BORDER_DEFAULT_LIGHT: [u8; 3] = [0xC6, 0xC4, 0xC0];
const BORDER_DEFAULT_DARK: [u8; 3] = [0x2D, 0x2B, 0x28];
const ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];

fn panel(size: (u32, u32), dark: bool) -> Harness<PanelWindow> {
    let h = Harness::new(PanelWindow::new, size.0, size.1);
    h.component.set_dark(dark);
    fixture::populate(&h.component);
    h
}

fn render(h: &Harness<PanelWindow>) -> Frame {
    // `changed` handlers (scroll-into-view) run during a frame; settle until nothing more changes.
    h.render_settled()
}

/// The List layout's row (Panel v2): the default `view` stays compact until the layout setting replaces it.
fn list_panel(size: (u32, u32), dark: bool) -> Harness<PanelWindow> {
    let h = panel(size, dark);
    h.component.set_view(PanelView::Comfortable);
    h
}

#[test]
fn panel_light() {
    let h = list_panel(SIZE, false);
    h.component.set_selected_index(1);
    assert_golden("panel-light", &render(&h));
}

#[test]
fn panel_dark() {
    let h = list_panel(SIZE, true);
    h.component.set_selected_index(1);
    assert_golden("panel-dark", &render(&h));
}

/// Top y of the first run of `colour` down column `x`.
fn first_y_of(f: &Frame, x: u32, colour: [u8; 3]) -> Option<u32> {
    (0..SIZE.1).find(|&y| f.pixel(x, y) == colour)
}

const SELECTED_LIGHT: [u8; 3] = [0xEB, 0xF2, 0xFE];
const HOVER_LIGHT: [u8; 3] = [0xEB, 0xE9, 0xE7];
const SELECTED_DARK: [u8; 3] = [0x05, 0x30, 0x6A];

#[test]
fn panel_row_hover_light() {
    let h = list_panel(SIZE, false);
    h.component.set_selected_index(1);
    let f = render(&h);
    // Row stride: a 56px row and the 6px gap of the comfortable list.
    let top = first_y_of(&f, 150, SELECTED_LIGHT).expect("the selected row is drawn");
    let hover_y = top + 2 * 62 + 28;
    h.component
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(150.0, hover_y as f32),
        });
    let f = render(&h);
    assert_eq!(
        f.pixel(150, hover_y - 20),
        HOVER_LIGHT,
        "the hovered row is filled with bg.hover"
    );
    assert_eq!(
        f.pixel(150, top + 62 + 50),
        [0xFB, 0xFA, 0xF7],
        "an idle row stays on the surface"
    );
    assert_golden("panel-row-hover-light", &f);
}

#[test]
fn panel_row_selected_sensitive_dark() {
    let h = list_panel(SIZE, true);
    h.component.set_selected_index(4);
    let f = render(&h);
    assert!(
        first_y_of(&f, 150, SELECTED_DARK).is_some(),
        "the sensitive row is selected"
    );
    assert_golden("panel-row-selected-sensitive-dark", &f);
}

#[test]
fn panel_no_target_light() {
    let h = list_panel(SIZE, false);
    h.component.set_target_name("".into());
    h.component.set_target_title("".into());
    h.component.set_selected_index(1);
    assert_golden("panel-no-target-light", &render(&h));
}

#[test]
fn panel_at_225_percent_text_size() {
    let h = panel((360, 760), false);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    h.component.set_selected_index(1);
    assert_golden("panel-scale-light", &render(&h));
}

#[test]
fn panel_empty_list_and_no_results_and_no_lists() {
    let h = panel(SIZE, false);
    fixture::set_rows(&h.component, Vec::new());
    assert_golden("panel-empty-light", &render(&h));

    h.component.set_query("zzz".into());
    assert_golden("panel-no-results-light", &render(&h));
    drop(h);

    let dark = panel(SIZE, true);
    fixture::set_rows(&dark.component, Vec::new());
    dark.component.set_query("zzz".into());
    assert_golden("panel-no-results-dark", &render(&dark));
    drop(dark);

    let none = panel(SIZE, false);
    fixture::set_rows(&none.component, Vec::new());
    none.component.set_tabs(ModelRc::new(VecModel::default()));
    none.component.set_pinned(ModelRc::new(VecModel::default()));
    assert_golden("panel-no-lists-light", &render(&none));
}

#[test]
fn panel_long_titles_in_both_views() {
    for (view, name) in [
        (PanelView::Compact, "panel-awkward-compact-light"),
        (PanelView::Comfortable, "panel-awkward-comfortable-light"),
    ] {
        let h = panel((300, 480), false);
        fixture::set_rows(&h.component, fixture::awkward_rows());
        h.component.set_view(view);
        h.component.set_query("w".into());
        assert_golden(name, &render(&h));
    }
}

#[test]
fn panel_with_many_rows_scrolled_shows_both_deck_edges() {
    let h = panel(SIZE, false);
    fixture::set_rows(&h.component, fixture::numbered_rows(24));
    // Navigate after the first frame, as a person does: the jump lands on a row the list has not built.
    let _ = render(&h);
    h.component.set_selected_index(15);
    let frame = render(&h);
    assert!(
        h.component.get_scroll_y() < -100.0 && h.component.get_scroll_y() > -280.0,
        "scrolled mid-list, both edges showing: {}",
        h.component.get_scroll_y()
    );
    assert_golden("panel-scrolled-light", &frame);
}

#[test]
fn panel_copy_route_with_a_toast_in_dark() {
    let h = panel(SIZE, true);
    h.component.set_target_name("".into());
    h.component.set_toast_text("Copied".into());
    h.component.set_toast_kind(ToastKind::Success);
    assert_golden("panel-copy-toast-dark", &render(&h));
}

#[test]
fn panel_with_the_app_menu_open() {
    for (dark, name) in [
        (false, "panel-app-menu-light"),
        (true, "panel-app-menu-dark"),
    ] {
        let h = panel(SIZE, dark);
        h.component.set_app_menu_open(true);
        assert_golden(name, &render(&h));
    }
}

#[test]
fn panel_with_an_overflowing_pinned_strip_shows_scroll_buttons_not_a_fade() {
    let h = panel(SIZE, false);
    h.component
        .set_pinned(ModelRc::new(VecModel::from(fixture::pinned_n(12))));
    assert_golden("panel-pinned-overflow-light", &render(&h));
}

/// Hover the close button (the last 28px square in the header, 8px from the right edge) and look under it.
/// One Harness per test: the frozen clock restarts at zero with each one.
fn tooltip_under_the_close_button(dark: bool, ink: [u8; 3]) {
    use slint::{LogicalPosition, platform::WindowEvent};
    let h = panel(SIZE, dark);
    let _ = render(&h);
    let at = LogicalPosition::new(298.0, 18.0); // 320 - 8 - 14: the close button's centre
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: at });
    h.advance(std::time::Duration::from_millis(600));
    h.advance(std::time::Duration::from_millis(200));
    let f = render(&h);
    // Below the button (bottom edge 32), a 4px gap, then the tooltip body; its right edge is clamped 8px
    // from the window.
    assert_eq!(f.pixel(SIZE.0 - 12, 44), ink, "tooltip body (dark={dark})");
    let app = if dark { APP_DARK } else { APP_LIGHT };
    assert_eq!(
        f.pixel(SIZE.0 - 3, 44),
        app,
        "clear of the window edge (dark={dark})"
    );
}

#[test]
fn a_hovered_header_button_shows_an_ink_tooltip_under_it_inside_the_window_light() {
    tooltip_under_the_close_button(false, [0x17, 0x16, 0x13]);
}

#[test]
fn a_hovered_header_button_shows_an_ink_tooltip_under_it_inside_the_window_dark() {
    tooltip_under_the_close_button(true, [0xEB, 0xE9, 0xE7]);
}

// ---- Measurements ----------------------------------------------------------------------------

// Layout of the populated Panel at 320 wide (ui/panel.slint): header 40, search 28 + 8 below, tab bar 32,
// pinned strip 24 + 2 x 4, then the list 4px in, 8px from each side; the footer is the last 32px.
const HEADER_H: u32 = 40;
const SEARCH_H: u32 = 36;
const TABS_H: u32 = 32;
const PINNED_H: u32 = 32;
const LIST_TOP: u32 = HEADER_H + SEARCH_H + TABS_H + PINNED_H + 4;
const LIST_X: u32 = 8;
const ROW_H: u32 = 28;
const FOOTER_H: u32 = 32;

#[test]
fn the_window_is_outlined_with_a_1px_default_border_and_solid_app_chrome() {
    for (dark, outline, app) in [
        (false, BORDER_DEFAULT_LIGHT, APP_LIGHT),
        (true, BORDER_DEFAULT_DARK, APP_DARK),
    ] {
        let h = panel(SIZE, dark);
        let f = render(&h);
        for (x, y) in [(0, 200), (SIZE.0 - 1, 200), (160, 0), (160, SIZE.1 - 1)] {
            assert_eq!(f.pixel(x, y), outline, "outline at ({x},{y}) dark={dark}");
        }
        assert_eq!(
            f.pixel(1, 200),
            app,
            "chrome inside the outline (dark={dark})"
        );
        // Header: nothing but the app background between the switcher and the buttons.
        assert_eq!(
            f.pixel(160, HEADER_H / 2),
            app,
            "header is chrome (dark={dark})"
        );
    }
}

#[test]
fn the_footer_is_32px_with_a_subtle_hairline_on_top() {
    for (dark, hairline, app) in [
        (false, BORDER_SUBTLE_LIGHT, APP_LIGHT),
        (true, BORDER_SUBTLE_DARK, APP_DARK),
    ] {
        let h = panel(SIZE, dark);
        let f = render(&h);
        let top = SIZE.1 - FOOTER_H;
        // Right of the text and left of the hints, clear of glyphs.
        let x = 20;
        assert_eq!(f.pixel(x, top), hairline, "footer hairline (dark={dark})");
        assert_eq!(
            f.pixel(x, top + 1),
            app,
            "footer chrome below it (dark={dark})"
        );
        assert_ne!(
            f.pixel(x, top - 1),
            hairline,
            "and not above it (dark={dark})"
        );
    }
}

#[test]
fn rows_are_28px_with_a_gutter_rule_and_the_selected_row_has_an_accent_border() {
    let h = panel(SIZE, false);
    h.component.set_selected_index(1);
    let f = render(&h);
    let (top, bottom) = (LIST_TOP + ROW_H, LIST_TOP + 2 * ROW_H - 1);
    // Selected row: a 1px accent outline exactly one row tall, inset 8px from the window.
    for (x, y, what) in [
        (100, top, "top edge"),
        (100, bottom, "bottom edge"),
        (LIST_X, top + ROW_H / 2, "left edge"),
    ] {
        assert_eq!(f.pixel(x, y), ACCENT, "selected row {what}");
    }
    assert_ne!(f.pixel(100, top - 1), ACCENT, "nothing above the outline");
    assert_ne!(
        f.pixel(100, bottom + 1),
        ACCENT,
        "nothing below the outline"
    );
    // Unselected rows are plain surface with no outline.
    assert_eq!(f.pixel(200, LIST_TOP + 4), SURFACE_LIGHT, "first row fill");
    assert_ne!(
        f.pixel(LIST_X, LIST_TOP + 14),
        ACCENT,
        "first row has no outline"
    );
    // The gutter rule: a subtle 1px line 24px in, on selected and unselected rows alike.
    let rule = LIST_X + 24;
    for y in [LIST_TOP + 14, top + 14] {
        assert_eq!(
            f.pixel(rule, y),
            BORDER_SUBTLE_LIGHT,
            "gutter rule at y={y}"
        );
    }
}

// ---- The collapsed dock handle (DESIGN.md §5 Panel: a 6px tab in the list's accent colour) ----------

const ON_ACCENT_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];

fn handle(size: (u32, u32), vertical: bool) -> Harness<PanelWindow> {
    let h = panel(size, false);
    h.component.set_dock_vertical(vertical);
    h.component.set_dock_collapsed(true);
    h
}

#[test]
fn a_vertical_handle_is_the_accent_colour_with_a_three_dot_grabber_in_the_middle() {
    let (w, hgt) = (6, 480);
    let h = handle((w, hgt), true);
    let f = render(&h);
    // Edge to edge, no outline and none of the panel behind it.
    for (x, y) in [(0, 0), (5, 0), (0, 479), (5, 479), (2, 100), (3, 300)] {
        assert_eq!(f.pixel(x, y), ACCENT, "strip at ({x}, {y})");
    }
    // Three 2px dots, 2px apart, centred: the group is 10px tall, so y 235-236, 239-240 and 243-244.
    for (y, dot) in [
        (235, true),
        (236, true),
        (237, false),
        (238, false),
        (240, true),
        (244, true),
        (245, false),
    ] {
        let wanted = if dot { ON_ACCENT_LIGHT } else { ACCENT };
        assert_eq!(f.pixel(2, y), wanted, "x=2 y={y}");
        assert_eq!(f.pixel(3, y), wanted, "x=3 y={y}");
    }
    assert_eq!(f.pixel(1, 240), ACCENT, "the dot is 2px wide, centred");
    assert_eq!(f.pixel(4, 240), ACCENT, "the dot is 2px wide, centred");
}

#[test]
fn a_horizontal_handle_runs_its_dots_along_the_strip() {
    let h = handle((480, 6), false);
    let f = render(&h);
    assert_eq!(f.pixel(0, 0), ACCENT);
    assert_eq!(f.pixel(479, 5), ACCENT);
    for (x, dot) in [
        (235, true),
        (236, true),
        (238, false),
        (240, true),
        (244, true),
        (245, false),
    ] {
        let wanted = if dot { ON_ACCENT_LIGHT } else { ACCENT };
        assert_eq!(f.pixel(x, 2), wanted, "x={x} y=2");
        assert_eq!(f.pixel(x, 3), wanted, "x={x} y=3");
    }
}

#[test]
fn the_handle_wears_the_list_colour_unless_high_contrast_is_on() {
    let teal = slint::Color::from_rgb_u8(0x04, 0x84, 0x84);
    let h = handle((6, 480), true);
    h.component.set_dock_list_color(teal);
    h.component.set_dock_has_list_color(true);
    assert_eq!(render(&h).pixel(0, 100), [0x04, 0x84, 0x84]);

    h.component.set_dock_has_list_color(false);
    assert_eq!(render(&h).pixel(0, 100), ACCENT, "no accent: the theme's");

    h.component.set_dock_has_list_color(true);
    let look = h.component.global::<SystemLook<'_>>();
    look.set_high_contrast(true);
    assert_ne!(
        render(&h).pixel(0, 100),
        [0x04, 0x84, 0x84],
        "high contrast keeps the system colours"
    );
}
