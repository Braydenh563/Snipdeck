//! Screenshot tests for the Main window in the states the design calls out (light, dark, 225% text, rail,
//! comfortable / compact / tile, empty states, long titles, menu, dialog, bulk bar), plus pixel measurements of the
//! layout claims in DESIGN.md §5 and ADR-0008.
//!
//! Goldens: `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test main_window`, then look at every PNG.
//! Goldens use only glyphs IBM Plex has (no CJK, no U+21B5: system fallback fonts differ per machine); the pixel
//! assertions below do not depend on glyph shapes.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "pixel geometry of a test frame: logical sizes are small and positive"
)]

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

    h.component.set_board_name("Work".into());
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
    // The theme's inset defaults to 0 (renders show the toolbar buttons); the app sets the real strip.
    h.component
        .global::<SystemLook<'_>>()
        .set_caption_inset(138.0);
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
        h.component
            .global::<SystemLook<'_>>()
            .set_caption_inset(138.0);
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

/// The (top, height, right edge) of every row (a `ListItem` right of the sidebar), in the order they are laid out. Rows are quiet: they
/// have no fill of their own at rest, so their place is read from the accessibility tree and not from pixels.
fn row_boxes(h: &Harness<MainWindow>) -> Vec<(f32, f32, f32)> {
    use i_slint_backend_testing::{AccessibleRole, ElementQuery};
    let mut boxes: Vec<(f32, f32, f32)> = ElementQuery::from_root(&h.component)
        .match_descendants()
        .match_accessible_role(AccessibleRole::ListItem)
        .find_all()
        .into_iter()
        .filter(|e| e.absolute_position().x > 235.0)
        .map(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            (p.y, s.height, p.x + s.width)
        })
        .collect();
    boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
    boxes.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5);
    boxes
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
            let boxes = row_boxes(&h);
            assert!(boxes.len() >= 3, "{view:?} cjk={cjk}: rows {boxes:?}");
            pitches.push(
                boxes
                    .windows(2)
                    .map(|w| w[1].0 - w[0].0)
                    .collect::<Vec<_>>(),
            );

            // The rows stop 8px short of the pane's right edge, and nothing is drawn in that padding: it is the
            // item area's own fill.
            for (_, _, right) in &boxes {
                assert!(
                    *right <= SIZE.0 as f32 - 8.0 + 0.5,
                    "{view:?} cjk={cjk}: row ends at {right}"
                );
            }
            for y in (110..SIZE.1 - 60).step_by(3) {
                assert_eq!(
                    f.pixel(SIZE.0 - 6, y),
                    SURFACE_LIGHT,
                    "{view:?} cjk={cjk}: ink at the right edge, y={y}"
                );
            }
            // The first row holds ink (the glyphs drew, tofu or not).
            let (top, height, _) = boxes[0];
            let (top, bottom) = (top as u32, (top + height) as u32);
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
                "{view:?} cjk={cjk}: only {ink} ink pixels in the first row"
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

// ---- Quiet rows ---------------------------------------------------------------------------------

const FOCUS_RING: [u8; 3] = [0x28, 0x70, 0xD4];
const SELECTED_LIGHT: [u8; 3] = [0xEB, 0xF2, 0xFE];
const HOVER_LIGHT: [u8; 3] = [0xF5, 0xF3, 0xF1];

fn move_pointer(h: &Harness<MainWindow>, at: slint::LogicalPosition) {
    use slint::platform::WindowEvent;
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: at });
}

fn click(h: &Harness<MainWindow>, at: slint::LogicalPosition) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let w = h.component.window();
    w.dispatch_event(WindowEvent::PointerMoved { position: at });
    w.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Left,
    });
    w.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Left,
    });
}

/// Pixels of the focus-ring colour inside the first row's box, along its top edge.
fn ring_on_top_edge(f: &Frame, row: (f32, f32, f32)) -> usize {
    let (top, _, right) = row;
    (260..right as u32 - 20)
        .filter(|x| f.pixel(*x, top as u32) == FOCUS_RING)
        .count()
}

#[test]
fn focus_ring_only_on_keyboard_focus() {
    use slint::platform::{Key, WindowEvent};
    let h = window(SIZE, false);
    h.component
        .window()
        .dispatch_event(WindowEvent::WindowActiveChanged(true));
    let _ = render(&h);
    let boxes = row_boxes(&h);
    // A pointer click on the second row takes the focus but draws no ring on it.
    let (top, height, _) = boxes[1];
    click(&h, slint::LogicalPosition::new(500.0, top + height / 2.0));
    let f = render(&h);
    assert_eq!(
        ring_on_top_edge(&f, boxes[1]),
        0,
        "a click leaves no focus ring"
    );
    // A key brings it.
    let text: slint::SharedString = Key::DownArrow.into();
    h.component
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    h.component
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
    let f = render(&h);
    let boxes = row_boxes(&h);
    let rings: usize = boxes.iter().map(|b| ring_on_top_edge(&f, *b)).sum();
    assert!(
        rings > 100,
        "the keyboard shows a 2px ring on the cursor row: {rings}px"
    );
}

/// The columns (first, last) with ink in a row's title line: 6px below its top to 20px below, from x 292 to the
/// last column before the actions.
fn title_ink(f: &Frame, row: (f32, f32, f32)) -> Option<(u32, u32)> {
    let top = row.0 as u32;
    let bg = f.pixel(900, top + 10);
    let ink = |x: u32| {
        (top + 6..top + 20).any(|y| {
            f.pixel(x, y)
                .iter()
                .zip(bg)
                .any(|(a, b)| a.abs_diff(b) > 40)
        })
    };
    let first = (292..900).find(|x| ink(*x))?;
    let last = (292..900).rev().find(|x| ink(*x))?;
    Some((first, last))
}

#[test]
fn title_width_unchanged_on_hover() {
    let h = window(SIZE, false);
    let rest = render(&h);
    let boxes = row_boxes(&h);
    let before = title_ink(&rest, boxes[2]).expect("title ink");
    let (top, height, _) = boxes[2];
    move_pointer(&h, slint::LogicalPosition::new(600.0, top + height / 2.0));
    let hovered = render(&h);
    assert_ne!(
        hovered.pixel(900, top as u32 + 10),
        rest.pixel(900, top as u32 + 10),
        "the row is hovered: it has a fill"
    );
    // Antialiasing on a different fill may move the last column by one pixel; the title does not move more.
    let after = title_ink(&hovered, boxes[2]).expect("title ink while hovered");
    assert!(
        after.0.abs_diff(before.0) <= 1 && after.1.abs_diff(before.1) <= 1,
        "the actions appear, the title does not move: {before:?} -> {after:?}"
    );
    assert_golden("main-row-hover-light", &hovered);
}

#[test]
fn main_window_search_results_light_and_dark() {
    for (dark, name) in [(false, "main-search-light"), (true, "main-search-dark")] {
        let h = window(SIZE, dark);
        fixture::set_rows(&h.component, fixture::search_rows());
        h.component.set_query("re".into());
        h.component.set_selected_count(1);
        assert_golden(name, &render(&h));
    }
}

#[test]
fn the_selected_row_is_a_fill_and_rows_have_no_outline() {
    let h = window(SIZE, false);
    fixture::set_rows(&h.component, fixture::search_rows());
    h.component.set_selected_count(1);
    let f = render(&h);
    let boxes = row_boxes(&h);
    let (top, height, right) = boxes[1];
    let mid = top as u32 + height as u32 / 2;
    assert_eq!(f.pixel(700, mid), SELECTED_LIGHT, "selection fill");
    assert_eq!(f.pixel(700, top as u32), SELECTED_LIGHT, "to the top edge");
    assert_eq!(
        f.pixel(700, (top + height) as u32 - 1),
        SELECTED_LIGHT,
        "to the bottom edge"
    );
    // The resting rows are the item area's surface: no outline, no fill of their own.
    let (top0, height0, _) = boxes[0];
    for (x, y) in [
        (300, top0 as u32),
        (300, top0 as u32 + height0 as u32 - 1),
        (right as u32 - 1, top0 as u32 + 10),
    ] {
        assert_eq!(f.pixel(x, y), SURFACE_LIGHT, "row 0 at ({x},{y})");
    }
    let _ = HOVER_LIGHT;
}

// ---- Toast (ink) ---------------------------------------------------------------------------------

const INK_LIGHT: [u8; 3] = [0x17, 0x16, 0x13];
const INK_DARK: [u8; 3] = [0xEB, 0xE9, 0xE7];
/// `toast.action` on ink: `blue-300` in light, `blue-700` in dark.
const TOAST_ACTION_LIGHT: [u8; 3] = [0xAB, 0xCD, 0xFE];
const TOAST_ACTION_DARK: [u8; 3] = [0x19, 0x5A, 0xB3];

fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    fn lum(c: [u8; 3]) -> f64 {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    }
    let (hi, lo) = (lum(a).max(lum(b)), lum(a).min(lum(b)));
    (hi + 0.05) / (lo + 0.05)
}

/// The bounding box (x0, y0, x1, y1; inclusive) of the pixels equal to `ink` in the bottom band, right of the sidebar.
fn ink_box(f: &Frame, ink: [u8; 3]) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for y in 520..SIZE.1 {
        for x in 240..SIZE.0 {
            if f.pixel(x, y) == ink {
                b = Some(match b {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                });
            }
        }
    }
    b
}

#[test]
fn toast_fill_is_ink() {
    for (dark, ink) in [(false, INK_LIGHT), (true, INK_DARK)] {
        let h = window(SIZE, dark);
        h.component.set_toast_text("Deleted 2 items".into());
        h.component.set_toast_undo_label("Undo".into());
        h.component.set_toast_kind(ToastKind::Neutral);
        let f = render(&h);
        let (x0, y0, x1, y1) = ink_box(&f, ink).expect("an ink toast");
        assert_eq!(y1 - y0 + 1, 40, "h40 (dark {dark})");
        assert!(x1 - x0 + 1 <= 480, "max width 480 (dark {dark})");
        // Centred in the item area (right of the 232px sidebar).
        let mid = f64::from(x0 + x1) / 2.0;
        let area_mid = (232.0 + f64::from(SIZE.0)) / 2.0;
        assert!((mid - area_mid).abs() < 4.0, "centred: {mid} vs {area_mid}");
        // The colour above the toast is not ink, and the toast is not a tinted neutral card.
        assert_ne!(f.pixel(x0 + 4, y0 - 3), ink);
        assert_eq!(f.pixel(x0 + 20, y0 + 4), ink);
    }
}

#[test]
fn toast_action_contrast_4_5() {
    assert!(contrast(TOAST_ACTION_LIGHT, INK_LIGHT) >= 4.5);
    assert!(contrast(TOAST_ACTION_DARK, INK_DARK) >= 4.5);
    // The rendered action carries that colour on the ink, not the text colour.
    for (dark, ink, action) in [
        (false, INK_LIGHT, TOAST_ACTION_LIGHT),
        (true, INK_DARK, TOAST_ACTION_DARK),
    ] {
        let h = window(SIZE, dark);
        h.component.set_toast_text("Deleted 2 items".into());
        h.component.set_toast_undo_label("Undo".into());
        let f = render(&h);
        let (x0, y0, x1, y1) = ink_box(&f, ink).expect("an ink toast");
        let near = |p: [u8; 3]| p.iter().zip(action).all(|(a, b)| a.abs_diff(b) <= 24);
        let found = (y0..=y1).any(|y| (x0..=x1).any(|x| near(f.pixel(x, y))));
        assert!(
            found,
            "the Undo action is drawn in toast.action (dark {dark})"
        );
    }
}
