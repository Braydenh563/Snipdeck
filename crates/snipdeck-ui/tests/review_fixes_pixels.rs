//! Pixel measurements for the independent component review (`.claude/reviews/ui-components.md`):
//! high-contrast glyphs on a selected row (item 5) and the tab focus ring against its underline (item 9).
//! Coordinates come from the fixed positions in `ui/gallery/review_probe.slint` and the tab-bar gallery.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a failure here is a failed test"
)]

use std::time::Duration;

use slint::ComponentHandle;
use snipdeck_ui::{
    ReviewProbe, SystemLook, TabsGallery,
    testing::{Frame, Harness},
};

/// `SystemLook` defaults: the Windows "High Contrast Black" highlight colour, and the text drawn on it.
const HC_HIGHLIGHT: [u8; 3] = [0x1A, 0xEB, 0xFF];

fn hc_probe() -> Frame {
    let h = Harness::new(ReviewProbe::new, 360, 300);
    h.component
        .global::<SystemLook<'_>>()
        .set_high_contrast(true);
    // Let the pulled card finish lifting and the colours settle.
    h.advance(Duration::from_millis(300));
    h.render_settled()
}

/// Whether any pixel of `x0..x1` x `y0..y1` clearly differs from `fill`: something is drawn on the fill.
/// A tint equal to the fill still rounds to within one step of it when the icon is blended, so "differs"
/// means a channel at least 48 steps away, which an invisible glyph never reaches.
fn drawn_on(frame: &Frame, (x0, x1): (u32, u32), (y0, y1): (u32, u32), fill: [u8; 3]) -> bool {
    (y0..y1).any(|y| {
        (x0..x1).any(|x| {
            let p = frame.pixel(x, y);
            (0..3).any(|c| p[c].abs_diff(fill[c]) >= 48)
        })
    })
}

#[test]
fn hc_the_selected_tags_check_is_visible_on_the_highlight_fill() {
    let frame = hc_probe();
    assert_eq!(
        frame.pixel(20, 28),
        HC_HIGHLIGHT,
        "the selected tag is filled with the system highlight"
    );
    assert!(
        drawn_on(&frame, (24, 36), (22, 34), HC_HIGHLIGHT),
        "the check is drawn in the highlight colour on the highlight fill, so it is invisible"
    );
}

#[test]
fn hc_the_selected_sidebar_glyph_is_visible_on_the_highlight_fill() {
    let frame = hc_probe();
    assert_eq!(
        frame.pixel(20, 70),
        HC_HIGHLIGHT,
        "the selected row is filled with the system highlight"
    );
    assert!(
        drawn_on(&frame, (24, 40), (62, 78), HC_HIGHLIGHT),
        "the board glyph is invisible on the selected row"
    );
}

#[test]
fn hc_the_pulled_selected_cards_check_and_glyph_are_visible() {
    let frame = hc_probe();
    assert!(
        drawn_on(&frame, (18, 38), (99, 120), HC_HIGHLIGHT),
        "the gutter check is invisible on the selected card"
    );
    assert!(
        drawn_on(&frame, (49, 65), (99, 120), HC_HIGHLIGHT),
        "the template glyph is invisible on the selected card"
    );
}

#[test]
fn hc_the_selected_choice_rows_check_is_visible() {
    let frame = hc_probe();
    // Measured from the running probe (2x dump): the list is black, its selected row ("Medium") spans
    // x 18..254, y 186..214 in the highlight fill.
    assert_eq!(
        frame.pixel(130, 200),
        HC_HIGHLIGHT,
        "the selected row is filled with the highlight"
    );
    assert_ne!(
        frame.pixel(130, 170),
        HC_HIGHLIGHT,
        "the unselected row above is not"
    );
    assert!(
        drawn_on(&frame, (228, 248), (192, 208), HC_HIGHLIGHT),
        "the check at the right of the selected choice is invisible"
    );
}

// ---- 9. The focus ring of the selected tab leaves its underline visible -------------------------------------

/// Light-theme blue-600 (the first tab's accent in the gallery) and the focus ring colour.
const BLUE_600: [u8; 3] = [0x28, 0x70, 0xD4];
const FOCUS_RING: [u8; 3] = [0x43, 0x8A, 0xF1];

#[test]
fn the_selected_tabs_focus_ring_does_not_cover_its_underline() {
    let h = Harness::new(TabsGallery::new, 700, 420);
    let frame = h.render_settled();
    // Second bar ("Lists focus"): first tab selected, forced keyboard focus. Measured from the running
    // gallery: the bar is 32px high and starts at y = 102; the underline is its last 2px.
    let (x, bottom) = (36, 102 + 32);
    assert_eq!(frame.pixel(x, bottom - 1), BLUE_600, "underline, last row");
    assert_eq!(frame.pixel(x, bottom - 2), BLUE_600, "underline, first row");
    assert_eq!(
        frame.pixel(x, bottom - 3),
        FOCUS_RING,
        "the ring sits just above the underline"
    );
}
