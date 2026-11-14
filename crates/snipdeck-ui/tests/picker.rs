//! Screenshot tests for the Picker window in the states the design calls out (light, dark, 225% text, empty,
//! no results, long titles, scrolled), plus pixel measurements of the layout claims in SPEC 5.2 and DESIGN.md.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test picker`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

use slint::{ComponentHandle, LogicalSize};
use snipdeck_ui::{
    PickerRow, PickerScope, PickerWindow, SystemLook,
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
const SELECTED_LIGHT: [u8; 3] = [0xEB, 0xF2, 0xFE];
const SEARCH_MARK_LIGHT: [u8; 3] = [0xAB, 0xCD, 0xFE];

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
    let window = h.component.window();
    let width = window.size().to_logical(window.scale_factor()).width;
    window.set_size(LogicalSize::new(width, height));
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
    let (visible, scroll) = (9.0 * 36.0, h.component.get_scroll_y());
    assert!(
        scroll < -(15.0 * 36.0 - visible) + 1.0 && scroll > -15.0 * 36.0,
        "row 15 is the last visible one: scroll {scroll}"
    );
    assert_golden("picker-scrolled-light", &frame);
}

// ---- Measurements ----------------------------------------------------------------------------

// Layout of the populated Picker at 560 wide (ui/picker.slint): the search area is 44px (SPEC 5.2); the scope
// chips 24px + 8px below; then a hairline, 4px, the rows (36px each), 4px, and the 32px footer.
const SEARCH_H: u32 = 44;
const CHIPS_H: u32 = 32;
const LIST_TOP: u32 = SEARCH_H + CHIPS_H + 4;
const LIST_X: u32 = 8;
const ROW_H: u32 = 36;
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
            "44 + 32 + 4 + 7 rows of 36 + 4 + 32"
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
    // 44 + 32 + 4 + 9 * 36 + 4 + 32
    assert!(
        (h.component.get_content_height() - 440.0).abs() < 0.5,
        "up to nine rows are visible: {}",
        h.component.get_content_height()
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
    assert_eq!(
        f.pixel(x, top + 1),
        OVERLAY_LIGHT,
        "footer surface below it"
    );
    assert_ne!(f.pixel(x, top - 1), BORDER_SUBTLE_LIGHT, "and not above it");
}

#[test]
fn rows_are_36px_and_the_selected_row_is_a_fill_without_a_ring() {
    let h = picker(false);
    h.component.set_selected_index(1);
    let f = render(&h);
    let (top, bottom) = (LIST_TOP + ROW_H, LIST_TOP + 2 * ROW_H - 1);
    // Clear of the 2px accent bar at the left edge and of the glyphs: the right half of the row.
    for y in [top, top + 18, bottom] {
        assert_eq!(f.pixel(300, y), SELECTED_LIGHT, "selected fill at y {y}");
    }
    assert_ne!(
        f.pixel(300, top - 1),
        SELECTED_LIGHT,
        "fill starts at the row top"
    );
    assert_ne!(
        f.pixel(300, bottom + 1),
        SELECTED_LIGHT,
        "fill ends at the row bottom"
    );
    // No ring: the accent appears along no edge. The only accent is the 2px bar inside the left edge (the Panel's
    // selected-row marker), which is not an outline: it is gone 2px in.
    for (x, y) in [
        (300, top),
        (300, bottom),
        (WIDTH - 9, top + 18),
        (LIST_X + 2, top + 18),
    ] {
        assert_ne!(f.pixel(x, y), ACCENT, "no ring at ({x},{y})");
    }
    assert_eq!(f.pixel(LIST_X, top + 18), ACCENT, "the 2px bar");
    assert_ne!(
        f.pixel(300, LIST_TOP + 18),
        SELECTED_LIGHT,
        "row 1 is not filled"
    );
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
    assert_ne!(
        f.pixel(300, 8 + 27),
        OVERLAY_LIGHT,
        "box bottom edge at y=35"
    );
    assert_eq!(f.pixel(300, 8 + 28), OVERLAY_LIGHT, "padding below");
}

// ---- Rows ---------------------------------------------------------------------------------------

/// Whether `a` and `b` differ by at least `by` in some channel.
fn differs(a: [u8; 3], b: [u8; 3], by: u8) -> bool {
    a.iter().zip(b).any(|(x, y)| x.abs_diff(y) >= by)
}

/// The first column at or right of `from` where row `index` (36px) has anything but its own fill, in the band
/// between 8px below its top and 8px above its bottom (clear of the corners).
fn first_ink(f: &Frame, index: u32, from: u32, by: u8) -> Option<u32> {
    let top = LIST_TOP + index * ROW_H;
    let fill = f.pixel(from, top + 2);
    (from..WIDTH - 8)
        .find(|x| (top + 8..top + ROW_H - 8).any(|y| differs(f.pixel(*x, y), fill, by)))
}

#[test]
fn titles_share_one_x_for_every_kind() {
    let h = picker(false);
    h.component.set_selected_index(1);
    let f = render(&h);
    // The tile is 24px; the title starts 8px after it. Letters differ by a pixel of side bearing, nothing more.
    let tiles_at: Vec<u32> = (0..7).map(|i| first_ink(&f, i, 28, 6).unwrap()).collect();
    assert!(
        tiles_at.iter().all(|x| *x == tiles_at[0]),
        "tiles start together: {tiles_at:?}"
    );
    let ink_starts: Vec<u32> = (0..7)
        .map(|i| first_ink(&f, i, tiles_at[0] + 24 + 2, 60).unwrap())
        .collect();
    let (lo, hi) = (
        ink_starts.iter().min().unwrap(),
        ink_starts.iter().max().unwrap(),
    );
    assert!(
        hi - lo <= 2,
        "one title x for text, template, code, link, secret and file rows: {ink_starts:?}"
    );
    assert!(
        (tiles_at[0] + 32..=tiles_at[0] + 33).contains(lo),
        "tile 24, gap 8, then the glyph's bearing: {ink_starts:?}"
    );
}

#[test]
fn file_row_has_no_extension_text() {
    let h = picker(false);
    h.component.set_selected_index(1);
    let f = render(&h);
    let tile_x = first_ink(&f, 5, 28, 6).unwrap();
    // The 'PDF' text used to sit between the number and the tile; now the gap holds nothing.
    for y in LIST_TOP + 5 * ROW_H + 4..LIST_TOP + 6 * ROW_H - 4 {
        for x in 28..tile_x {
            assert!(
                !differs(f.pixel(x, y), OVERLAY_LIGHT, 6),
                "ink at ({x},{y}) before the file tile"
            );
        }
    }
    assert_eq!(
        Some(tile_x),
        first_ink(&f, 0, 28, 6),
        "the file tile starts where the text tile does"
    );
}

fn marked_rows() -> Vec<PickerRow> {
    let mut rows = fixture::sample_rows();
    rows[0].mark_before = "Thanks, ".into();
    rows[0].mark_text = "re".into();
    rows[1].preview = "A gentle reminder that invoice {Number} is due".into();
    rows[1].preview_before = "A gentle ".into();
    rows[1].preview_match = "re".into();
    rows[2].tag_hit = "retry".into();
    rows
}

#[test]
fn search_mark_under_match() {
    let h = picker(false);
    fixture::set_rows(&h.component, marked_rows());
    h.component.set_query("re".into());
    h.component.set_selected_index(3);
    let f = render(&h);
    let top = LIST_TOP;
    // Count mark-coloured pixels in the title band of row 0 and find a column's vertical run.
    let columns: Vec<u32> = (60..400)
        .filter(|x| (top..top + ROW_H).any(|y| f.pixel(*x, y) == SEARCH_MARK_LIGHT))
        .collect();
    assert!(
        !columns.is_empty(),
        "the title match sits on the search-mark fill"
    );
    let widest = columns
        .iter()
        .map(|x| {
            (top..top + ROW_H)
                .filter(|y| f.pixel(*x, *y) == SEARCH_MARK_LIGHT)
                .count()
        })
        .max()
        .unwrap();
    assert!(
        widest >= 14,
        "a filled box, not a 1px underline: {widest}px tall"
    );
    // The box is inside the row and leaves its bottom edge alone.
    for x in 60..WIDTH {
        assert_ne!(
            f.pixel(x, top + ROW_H - 1),
            SEARCH_MARK_LIGHT,
            "nothing on the row's last line at x {x}"
        );
    }
    assert_golden("picker-search-light", &f);
}
