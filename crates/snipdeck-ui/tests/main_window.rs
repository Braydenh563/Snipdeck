//! Screenshot tests for the Main window in the states the design calls out (light, dark, 225% text, rail,
//! comfortable / compact / tile, empty states, long titles, menu, dialog, bulk bar), plus pixel measurements of the
//! layout claims in DESIGN.md §5 and ADR-0008.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test main_window`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

use slint::ComponentHandle;
use snipdeck_ui::{
    MainSection, MainView, MainWindow, SystemLook, ToastKind,
    testing::{Frame, Harness, assert_golden},
};

#[path = "support/main_fixture.rs"]
mod fixture;

const SIZE: (u32, u32) = (1040, 640);

// Palette values from design/tokens/palette.json.
const APP_LIGHT: [u8; 3] = [0xF5, 0xF3, 0xF1];
const APP_DARK: [u8; 3] = [0x11, 0x0F, 0x0D];
const BORDER_SUBTLE_LIGHT: [u8; 3] = [0xDE, 0xDC, 0xD8];
const BORDER_SUBTLE_DARK: [u8; 3] = [0x20, 0x1E, 0x1B];

fn window(size: (u32, u32), dark: bool) -> Harness<MainWindow> {
    let h = Harness::new(MainWindow::new, size.0, size.1);
    h.component.set_dark(dark);
    let _ = fixture::populate(&h.component);
    h
}

fn render(h: &Harness<MainWindow>) -> Frame {
    // `changed` handlers (scroll-into-view) run during a frame; settle until nothing more changes.
    h.render_settled()
}

#[test]
fn main_window_light_and_dark() {
    let h = window(SIZE, false);
    h.component.set_board_name("Work".into());
    assert_golden("main-light", &render(&h));
    drop(h);
    let h = window(SIZE, true);
    assert_golden("main-dark", &render(&h));
}

#[test]
fn main_window_compact_and_tile_views() {
    let h = window(SIZE, false);
    h.component.set_view(MainView::Compact);
    assert_golden("main-compact-light", &render(&h));
    drop(h);
    let h = window(SIZE, false);
    h.component.set_view(MainView::Tile);
    assert_golden("main-tile-light", &render(&h));
    drop(h);
    let h = window(SIZE, true);
    h.component.set_view(MainView::Tile);
    assert_golden("main-tile-dark", &render(&h));
}

#[test]
fn main_window_at_225_percent_text_size() {
    let h = window((1400, 900), false);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    assert_golden("main-scale-light", &render(&h));
}

#[test]
fn main_window_collapses_to_an_icon_rail_below_720px() {
    let h = window((640, 520), false);
    let f = render(&h);
    assert!(h.component.get_rail(), "rail below 720");
    assert_golden("main-rail-light", &f);
    drop(h);
    let h = window((760, 520), false);
    let _ = render(&h);
    assert!(!h.component.get_rail(), "full sidebar at 760");
}

#[test]
fn main_window_empty_states() {
    let h = window(SIZE, false);
    fixture::set_rows(&h.component, Vec::new());
    assert_golden("main-empty-all-light", &render(&h));

    h.component.set_query("zzz".into());
    assert_golden("main-no-results-light", &render(&h));
    h.component.set_query("".into());

    h.component.set_section(MainSection::Trash);
    assert_golden("main-empty-trash-light", &render(&h));
    drop(h);

    let dark = window(SIZE, true);
    fixture::set_rows(&dark.component, Vec::new());
    dark.component.set_section(MainSection::List);
    dark.component.set_section_index(1);
    assert_golden("main-empty-list-dark", &render(&dark));
}

#[test]
fn main_window_loading_in_rows_and_tiles() {
    let h = window(SIZE, false);
    fixture::set_rows(&h.component, Vec::new());
    h.component.set_loading(true);
    assert_golden("main-loading-light", &render(&h));
    h.component.set_view(MainView::Tile);
    assert_golden("main-loading-tile-light", &render(&h));
}

#[test]
fn main_window_long_titles_in_every_view() {
    for (view, name) in [
        (MainView::Compact, "main-awkward-compact-light"),
        (MainView::Comfortable, "main-awkward-comfortable-light"),
        (MainView::Tile, "main-awkward-tile-light"),
    ] {
        let h = window((820, 520), false);
        fixture::set_rows(&h.component, fixture::awkward_rows());
        h.component.set_view(view);
        h.component.set_query("w".into());
        assert_golden(name, &render(&h));
    }
}

#[test]
fn main_window_many_rows_scrolled_shows_both_deck_edges() {
    let h = window(SIZE, false);
    fixture::set_rows(&h.component, fixture::numbered_rows(40));
    let _ = render(&h);
    h.component.set_cursor(20);
    assert_golden("main-scrolled-light", &render(&h));
}

#[test]
fn main_window_toast_in_dark() {
    let h = window(SIZE, true);
    h.component.set_toast_text("Deleted 2 items".into());
    h.component.set_toast_undo_label("Undo".into());
    h.component.set_toast_kind(ToastKind::Neutral);
    assert_golden("main-toast-dark", &render(&h));
}

#[test]
fn main_window_bulk_bar_with_a_selection() {
    let h = window(SIZE, false);
    let rows = fixture::set_rows(&h.component, fixture::sample_rows());
    for i in 1..4 {
        let mut row = slint::Model::row_data(&*rows, i).expect("row");
        row.selected = true;
        slint::Model::set_row_data(&*rows, i, row);
    }
    h.component.set_selected_count(3);
    assert_golden("main-bulk-light", &render(&h));
}

// ---- Measurements ----------------------------------------------------------------------------

#[test]
fn the_sidebar_is_232px_wide_with_a_subtle_hairline_and_app_chrome() {
    for (dark, hairline, app) in [
        (false, BORDER_SUBTLE_LIGHT, APP_LIGHT),
        (true, BORDER_SUBTLE_DARK, APP_DARK),
    ] {
        let h = window(SIZE, dark);
        let f = render(&h);
        assert_eq!(f.pixel(231, 300), hairline, "rule at x=231 (dark={dark})");
        assert_eq!(f.pixel(230, 600), app, "chrome left of it (dark={dark})");
        assert_ne!(f.pixel(232, 600), hairline, "and not right of it");
    }
}

#[test]
fn the_toolbar_leaves_the_caption_button_strip_free() {
    let h = window(SIZE, false);
    let f = render(&h);
    // ADR-0008: 138px (3 x 46) at the top right belong to the OS caption buttons; nothing is drawn there.
    for x in (SIZE.0 - 138)..SIZE.0 {
        assert_eq!(f.pixel(x, 22), APP_LIGHT, "caption strip at x={x}");
    }
    // The New item button ends 8px left of the strip.
    h.component
        .global::<SystemLook<'_>>()
        .set_caption_inset(200.0);
    let f = render(&h);
    for x in (SIZE.0 - 200)..SIZE.0 {
        assert_eq!(f.pixel(x, 22), APP_LIGHT, "wider strip at x={x}");
    }
    assert_eq!(
        rightmost_ink(&f, SIZE.0 - 200),
        SIZE.0 - 200 - 8 - 1,
        "the toolbar's right edge follows the strip"
    );
}

/// The x of the last pixel in the toolbar's middle row that is not the app background, looking left from `from`.
fn rightmost_ink(f: &Frame, from: u32) -> u32 {
    (0..from)
        .rev()
        .find(|&x| f.pixel(x, 22) != APP_LIGHT)
        // No ink at all reads as 0, which fails every comparison below.
        .unwrap_or(0)
}

#[test]
fn the_new_item_button_ends_at_the_toolbars_right_padding() {
    // Right-aligned with the view toggles beside it, not centred: its right edge is the 8px padding left of the
    // caption strip, at 100% and at 225% text size, and the toggles sit directly left of it.
    for (scale, size) in [(1.0_f32, SIZE), (2.25, (1400, 900))] {
        let h = window(size, false);
        h.component.global::<SystemLook<'_>>().set_text_scale(scale);
        let f = render(&h);
        let edge = size.0 - 138 - 8 - 1;
        assert_eq!(
            rightmost_ink(&f, size.0 - 138),
            edge,
            "button right edge at scale {scale}"
        );
        // The button is dark: its fill reaches the edge's neighbours, so this is the button and not a toggle.
        assert_ne!(
            f.pixel(edge - 4, 22),
            APP_LIGHT,
            "button body at scale {scale}"
        );
    }
}

#[test]
fn the_empty_state_keeps_the_list_pane_free_of_cards() {
    let h = window(SIZE, false);
    fixture::set_rows(&h.component, Vec::new());
    let f = render(&h);
    // No card fill (bg-surface) anywhere in the middle of the list pane.
    for y in (120..560).step_by(40) {
        assert_ne!(f.pixel(260, y), [0xFB, 0xFA, 0xF7], "no card at y={y}");
    }
}

// ---- CJK (measured, never a golden: system fallback fonts differ per machine) ----------------------

const SURFACE_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];

/// y of every card top on the column `x`: where the surface fill starts below something else.
fn card_tops(f: &Frame, x: u32, height: u32) -> Vec<u32> {
    let mut tops = Vec::new();
    for y in 100..height - 40 {
        if f.pixel(x, y) == SURFACE_LIGHT && f.pixel(x, y - 1) != SURFACE_LIGHT {
            tops.push(y);
        }
    }
    tops
}

fn long_rows(cjk: bool) -> Vec<snipdeck_ui::MainRow> {
    let (title, preview) = if cjk {
        (
            "請求書のお願い",
            "お世話になっております。先月分の請求書をお送りいただけますでしょうか。ご確認のほどよろしくお願いいたします。先月分の請求書をお送りいただけますでしょうか。",
        )
    } else {
        (
            "Invoice request",
            "Thank you for your help so far. Could you send last month's invoice when you have a moment? Payment reminder attached, thanks again.",
        )
    };
    // Three times over, so both scripts need more lines than a row allows: the cap (two preview lines in
    // comfortable, one in compact) is what is compared. A Japanese sentence has no spaces to break at.
    let preview = preview.repeat(3);
    (0..4)
        .map(|_| fixture::row(snipdeck_ui::ItemKind::Text, title, &preview))
        .collect()
}

#[test]
fn cjk_rows_keep_the_card_pitch_and_stay_inside_the_list_pane() {
    for view in [MainView::Comfortable, MainView::Compact] {
        let mut pitches = Vec::new();
        for cjk in [false, true] {
            let h = window(SIZE, false);
            fixture::set_rows(&h.component, long_rows(cjk));
            h.component.set_view(view);
            let f = render(&h);
            let tops = card_tops(&f, 252, SIZE.1);
            assert!(tops.len() >= 3, "{view:?} cjk={cjk}: card tops {tops:?}");
            pitches.push(tops.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());

            // Nothing is drawn in the right padding of the list pane.
            for y in (110..SIZE.1 - 60).step_by(3) {
                assert_eq!(
                    f.pixel(SIZE.0 - 6, y),
                    APP_LIGHT,
                    "{view:?} cjk={cjk}: ink at the right edge, y={y}"
                );
            }
            // The first card holds ink (the glyphs drew, tofu or not).
            let (top, bottom) = (tops[0], tops[1]);
            let mut ink = 0;
            for y in top + 2..bottom - 2 {
                for x in (256..SIZE.0 - 40).step_by(2) {
                    if f.pixel(x, y) != SURFACE_LIGHT {
                        ink += 1;
                    }
                }
            }
            assert!(
                ink > 50,
                "{view:?} cjk={cjk}: only {ink} ink pixels in the first card"
            );
        }
        assert_eq!(
            pitches[0], pitches[1],
            "{view:?}: CJK text changes the row height"
        );
    }
}

#[test]
fn the_cjk_family_setting_reaches_every_text_of_the_window() {
    use snipdeck_ui::Fonts;
    let h = window(SIZE, false);
    let base = render(&h);
    // Plex Mono is bundled, so the effect is the same on every machine: all UI text changes family.
    h.component
        .global::<Fonts<'_>>()
        .set_cjk_family("IBM Plex Mono".into());
    let changed = render(&h);
    let mut differing = 0;
    for y in (0..SIZE.1).step_by(2) {
        for x in (0..SIZE.0).step_by(2) {
            if base.pixel(x, y) != changed.pixel(x, y) {
                differing += 1;
            }
        }
    }
    assert!(differing > 1500, "only {differing} pixels changed");
    h.component.global::<Fonts<'_>>().set_cjk_family("".into());
    let back = render(&h);
    for y in (0..SIZE.1).step_by(7) {
        for x in (0..SIZE.0).step_by(7) {
            assert_eq!(
                back.pixel(x, y),
                base.pixel(x, y),
                "resetting the family restores Plex Sans at ({x}, {y})"
            );
        }
    }
}
