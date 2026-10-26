//! Screenshot tests for the Panel window in the states the design calls out (light, dark, 225% text,
//! empty, no results, long titles, 20+ rows scrolled, toast), plus pixel measurements of the
//! layout claims in DESIGN.md §5 and §6.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test panel`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

use slint::{ComponentHandle, ModelRc, VecModel};
use snipdeck_ui::{
    PanelLayout, PanelWindow, SystemLook, ToastKind,
    testing::{Frame, Harness, assert_golden},
};

#[path = "support/panel_fixture.rs"]
mod fixture;

const SIZE: (u32, u32) = (320, 560);

// Palette values from design/tokens/palette.json.
const APP_LIGHT: [u8; 3] = [0xF5, 0xF3, 0xF1];
const APP_DARK: [u8; 3] = [0x11, 0x0F, 0x0D];
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

/// The shipped Panel: the List layout is the default.
fn list_panel(size: (u32, u32), dark: bool) -> Harness<PanelWindow> {
    panel(size, dark)
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
    let hover_y = u16::try_from(top + 2 * 62 + 28).unwrap();
    h.component
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(150.0, f32::from(hover_y)),
        });
    let f = render(&h);
    assert_eq!(
        f.pixel(150, u32::from(hover_y) - 20),
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

    h.component.set_add_list_name("Email replies".into());
    h.component.set_query("zzz".into());
    assert_golden("panel-no-results-light", &render(&h));
    drop(h);

    let dark = panel(SIZE, true);
    fixture::set_rows(&dark.component, Vec::new());
    dark.component.set_add_list_name("Email replies".into());
    dark.component.set_query("zzz".into());
    assert_golden("panel-no-results-dark", &render(&dark));
    drop(dark);

    let none = panel(SIZE, false);
    fixture::set_rows(&none.component, Vec::new());
    none.component.set_tabs(ModelRc::new(VecModel::default()));
    none.component.set_pinned(ModelRc::new(VecModel::default()));
    assert_golden("panel-no-lists-light", &render(&none));
}

/// The Compact and Cards layouts at rest (Panel v2 spec 3): the second row selected, the first and the fourth idle.
#[test]
fn panel_compact_and_cards_layouts() {
    for (layout, size, dark, name) in [
        (PanelLayout::Compact, SIZE, false, "panel-compact-light"),
        (PanelLayout::Compact, SIZE, true, "panel-compact-dark"),
        (PanelLayout::Cards, SIZE, false, "panel-cards-light"),
        (PanelLayout::Cards, SIZE, true, "panel-cards-dark"),
        (
            PanelLayout::Cards,
            (280, 560),
            false,
            "panel-cards-280-light",
        ),
    ] {
        let h = panel(size, dark);
        h.component.set_layout(layout);
        h.component.set_selected_index(1);
        assert_golden(name, &render(&h));
    }
}

/// The Menu switcher (Panel v2 spec 3): no tab row, "Board · List" in the header, and its menu open.
#[test]
fn panel_with_the_menu_switcher_closed_and_open() {
    let h = panel(SIZE, false);
    h.component.set_lists_in_menu(true);
    assert_golden("panel-lists-menu-light", &render(&h));
    h.component.set_board_menu_open(true);
    assert_golden("panel-lists-menu-open-light", &render(&h));
}

#[test]
fn panel_long_titles_in_both_layouts() {
    for (view, name) in [
        (PanelLayout::Compact, "panel-awkward-compact-light"),
        (PanelLayout::List, "panel-awkward-comfortable-light"),
    ] {
        let h = panel((300, 480), false);
        fixture::set_rows(&h.component, fixture::awkward_rows());
        h.component.set_layout(view);
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
        h.component.get_scroll_y() < -100.0 && h.component.get_scroll_y() > -1100.0,
        "scrolled mid-list (the deck fades into both edges): {}",
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

/// Search results as the app hands them over: every board is searched, each row says where it lives, and the match
/// in the title is marked.
fn search_rows() -> Vec<snipdeck_ui::PanelRow> {
    let mut rows = fixture::sample_rows();
    rows.truncate(3);
    for (row, (before, found, list)) in rows.iter_mut().zip([
        ("Thanks, ", "re", "Work · Email replies"),
        ("Invoice ", "re", "Work · Email replies"),
        ("Retry helper", "", "Home · Code"),
    ]) {
        row.mark_before = before.into();
        row.mark_text = found.into();
        row.list_name = list.into();
        row.list_color = fixture::TEAL_600;
    }
    // The first row also matches in its preview (cut around the match); the third only through a tag.
    rows[0].preview = "\u{2026}ver. I'll review it today and get back to you.".into();
    rows[0].preview_before = "\u{2026}ver. I'll ".into();
    rows[0].preview_match = "re".into();
    rows[2].tag_hit = "regex".into();
    rows
}

#[test]
fn panel_search_expanded_replaces_the_tabs_and_pins_and_marks_matches() {
    for (dark, name) in [(false, "panel-search-light"), (true, "panel-search-dark")] {
        let h = panel(SIZE, dark);
        fixture::set_rows(&h.component, search_rows());
        h.component.set_query("re".into());
        h.component.set_selected_index(0);
        let f = render(&h);
        // Header 40, then the 72px search block; the list starts right under it (no tab row, no pinned strip).
        let hairline = if dark {
            BORDER_SUBTLE_DARK
        } else {
            BORDER_SUBTLE_LIGHT
        };
        assert_eq!(
            f.pixel(160, HEADER_H + 72 - 1),
            hairline,
            "the block's bottom hairline sits at y={} (dark={dark})",
            HEADER_H + 71
        );
        assert_golden(name, &f);
    }
}

/// WCAG relative luminance of an sRGB colour.
fn luminance(c: [u8; 3]) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[test]
fn the_header_more_glyph_reads_at_3_to_1_against_the_header() {
    for (dark, bg) in [(false, APP_LIGHT), (true, APP_DARK)] {
        let h = panel(SIZE, dark);
        let f = render(&h);
        // The kebab button spans x 254..278 in the 320 wide header; its glyph is three dots.
        let best = (256..278)
            .flat_map(|x| (8..32).map(move |y| (x, y)))
            .map(|(x, y)| contrast(f.pixel(x, y), bg))
            .fold(0.0_f64, f64::max);
        assert!(
            best >= 3.0,
            "dark={dark}: the darkest glyph pixel is {best:.2}:1 against the header"
        );
    }
}

#[test]
fn a_search_marks_the_preview_match_and_a_tag_hit_reads_tag_name() {
    let h = panel(SIZE, false);
    fixture::set_rows(&h.component, search_rows());
    h.component.set_query("re".into());
    h.component.set_selected_index(2);
    let f = render(&h);
    let top = HEADER_H + 72;
    let row_h: u32 = 74;
    let marked_in = |row: u32| {
        (top + row * row_h..top + (row + 1) * row_h)
            .filter(|&y| (8..312).any(|x| f.pixel(x, y) == SEARCH_MARK_LIGHT))
            .count()
    };
    // Title mark (row 0 has both): taller than the preview span alone, so compare against row 2, which has neither.
    assert_eq!(marked_in(2), 0, "a tag hit marks nothing");
    assert!(
        marked_in(0) > 14,
        "row 0 marks the title and the preview match"
    );
    let mut quiet = search_rows();
    quiet[0].preview_match = "".into();
    quiet[0].preview_before = "".into();
    fixture::set_rows(&h.component, quiet);
    let without = render(&h);
    let title_only = (top..top + row_h)
        .filter(|&y| (8..312).any(|x| without.pixel(x, y) == SEARCH_MARK_LIGHT))
        .count();
    assert!(
        marked_in(0) > title_only,
        "the preview adds a span of its own"
    );
}

#[test]
fn a_search_match_is_a_filled_span_not_an_underline() {
    let h = panel(SIZE, false);
    fixture::set_rows(&h.component, search_rows());
    h.component.set_query("re".into());
    h.component.set_selected_index(2);
    let f = render(&h);
    let top = HEADER_H + 72;
    let (mut filled, mut lines) = (0, 0);
    for y in top..top + 30 {
        let run = (8..312)
            .filter(|&x| f.pixel(x, y) == SEARCH_MARK_LIGHT)
            .count();
        if run > 6 {
            filled += 1;
        }
        // An underline would be a thin full-width-of-the-word run on one row only.
        if run > 6 && (f.pixel(0, y) == SEARCH_MARK_LIGHT) {
            lines += 1;
        }
    }
    assert!(
        filled >= 12,
        "the span is a block at least 12px tall behind the match: {filled}"
    );
    assert_eq!(lines, 0);
}

#[test]
fn panel_search_always_shows_a_44px_strip_and_keeps_tabs_and_pins() {
    let h = panel(SIZE, false);
    h.component.set_search_always(true);
    let f = render(&h);
    // Tabs bottom hairline sits under the 44px strip, header 40 + strip 44 + tabs 32.
    assert_eq!(
        f.pixel(160, HEADER_H + 44 + 32 - 1),
        BORDER_SUBTLE_LIGHT,
        "tab row follows the strip"
    );
    assert_golden("panel-search-always-light", &f);
}

#[test]
fn panel_undo_toast_in_dark() {
    let h = panel(SIZE, true);
    h.component
        .set_toast_text("Moved “Q3 report” to Addresses".into());
    h.component.set_toast_undo_label("Undo".into());
    h.component.set_toast_kind(ToastKind::Neutral);
    let f = render(&h);
    // The ink toast: 40 tall, inset 12 from both sides, 8 above the footer. Dark ink is the light neutral.
    let (left, y) = (12 + 2, SIZE.1 - FOOTER_H - 8 - 20);
    assert_eq!(
        f.pixel(left, y),
        [0xEB, 0xE9, 0xE7],
        "ink fill at the toast's left inset"
    );
    assert_ne!(
        f.pixel(8, y),
        [0xEB, 0xE9, 0xE7],
        "nothing left of the 12px inset"
    );
    assert_golden("panel-undo-toast-dark", &f);
}

// ---- Keyboard focus ---------------------------------------------------------------------------

const FOCUS_RING: [u8; 3] = [0x28, 0x70, 0xD4];
/// Palette `search.mark`, light: blue-300.
const SEARCH_MARK_LIGHT: [u8; 3] = [0xAB, 0xCD, 0xFE];

/// Key press and release into the Panel's window, as the platform sends them.
fn tap(h: &Harness<PanelWindow>, key: slint::platform::Key) {
    use slint::platform::WindowEvent;
    let text: slint::SharedString = key.into();
    h.component
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    h.component
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

/// Focus-ring coloured pixels within `reach` px outside the box `(left, top, width, height)`.
fn ring_pixels(frame: &Frame, bounds: (u32, u32, u32, u32), reach: u32) -> usize {
    let (left, top, width, height) = bounds;
    let mut count = 0;
    for row in top.saturating_sub(reach)..(top + height + reach).min(SIZE.1) {
        for col in left.saturating_sub(reach)..(left + width + reach).min(SIZE.0) {
            let inside = col >= left && col < left + width && row >= top && row < top + height;
            if !inside && frame.pixel(col, row) == FOCUS_RING {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn focus_ring_only_on_keyboard_focus() {
    use slint::{
        LogicalPosition,
        platform::{PointerEventButton, WindowEvent},
    };
    let h = panel(SIZE, false);
    let w = h.component.window();
    w.dispatch_event(WindowEvent::WindowActiveChanged(true));
    // The keep-on-top button: 32 square at x 216, y 4 (measured from the accessibility tree).
    let pin = (216, 4, 32, 32);
    assert_eq!(ring_pixels(&render(&h), pin, 4), 0, "no ring at rest");

    // A pointer click gives the button the focus but draws no ring.
    let at = LogicalPosition::new(232.0, 20.0);
    w.dispatch_event(WindowEvent::PointerMoved { position: at });
    w.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    w.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
    w.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(92.0, 20.0),
    });
    assert_eq!(
        ring_pixels(&render(&h), pin, 4),
        0,
        "a click leaves no focus ring"
    );

    // Tab to it: the ring shows.
    let mut seen = false;
    for _ in 0..12 {
        tap(&h, slint::platform::Key::Tab);
        if ring_pixels(&render(&h), pin, 4) > 0 {
            seen = true;
            break;
        }
    }
    assert!(
        seen,
        "Tab reaches the keep-on-top button and draws its ring"
    );
}

#[test]
fn panel_focus_ring_on_a_list_tab_from_the_keyboard() {
    use slint::platform::{Key, WindowEvent};
    let h = panel(SIZE, false);
    h.component
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    let _ = render(&h);
    // Header: board, search, New, keep on top, More, close; then the list tabs.
    for _ in 0..7 {
        tap(&h, Key::Tab);
    }
    let f = render(&h);
    // The ring sits above the selected tab's underline and never covers it: the tab row is y 40..72, its underline the
    // last 2px (y 70, 71) in the accent colour, the ring just above it.
    let rows: Vec<u32> = (41..70)
        .filter(|&y| (4..300).any(|x| f.pixel(x, y) == FOCUS_RING))
        .collect();
    assert!(!rows.is_empty(), "a focus ring is drawn on the tab row");
    // The selected tab's underline is its list colour (teal-600).
    assert_eq!(
        f.pixel(30, 71),
        [0x04, 0x84, 0x84],
        "the underline stays whole"
    );
    assert_golden("panel-focus-tab-light", &f);
}

/// Hover the close button (the last 28px square in the header, 8px from the right edge) and look under it.
/// One Harness per test: the frozen clock restarts at zero with each one.
fn tooltip_under_the_close_button(dark: bool, ink: [u8; 3]) {
    use slint::{LogicalPosition, platform::WindowEvent};
    let h = panel(SIZE, dark);
    let _ = render(&h);
    let at = LogicalPosition::new(300.0, 20.0); // 320 - 4 - 16: the 32px close button's centre
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

// Layout of the populated Panel at 320 wide (ui/panel.slint, spec 2.1-2.6): header 40, tabs 32, pinned strip 36,
// then the list; the footer is the last 32px. The search is collapsed to nothing until it is used.
const HEADER_H: u32 = 40;
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
            f.pixel(1, HEADER_H / 2),
            app,
            "chrome inside the outline (dark={dark})"
        );
        // Header: nothing but the app background between the board picker and the search button.
        assert_eq!(
            f.pixel(92, HEADER_H / 2),
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
