//! Screenshot tests for the Panel window in the states the design calls out (light, dark, 225% text,
//! empty, no results, long titles and CJK, 20+ rows scrolled, toast), plus pixel measurements of the
//! layout claims in DESIGN.md §5 and §6.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test panel`, then look at every PNG.
//! Text (and the CJK sample, which falls back to a system font) can differ per machine; the pixel
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
    // `changed` handlers (scroll-into-view) run on the first frame; the second shows their effect.
    let _ = h.render();
    h.render()
}

#[test]
fn panel_light() {
    let h = panel(SIZE, false);
    h.component.set_selected_index(1);
    assert_golden("panel-light", &render(&h));
}

#[test]
fn panel_dark() {
    let h = panel(SIZE, true);
    h.component.set_selected_index(1);
    assert_golden("panel-dark", &render(&h));
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
fn panel_long_titles_and_cjk_in_both_views() {
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
    h.component.set_selected_index(12);
    assert_golden("panel-scrolled-light", &render(&h));
}

#[test]
fn panel_copy_route_with_a_toast_in_dark() {
    let h = panel(SIZE, true);
    h.component.set_target_name("".into());
    h.component.set_toast_text("Copied".into());
    h.component.set_toast_kind(ToastKind::Success);
    assert_golden("panel-copy-toast-dark", &render(&h));
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
