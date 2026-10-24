//! The overlay scrollbar (`OverlayScrollbar`): shown only when the content overflows, thumb follows the scroll
//! position, dragging and clicking the track scroll, the wheel keeps working. Goldens in light, dark and
//! high contrast.
#![allow(
    clippy::float_cmp,
    reason = "positions are whole logical pixels from a frozen layout"
)]

use slint::{
    ComponentHandle, LogicalPosition,
    platform::{PointerEventButton, WindowEvent},
};
use snipdeck_ui::{
    ScrollbarGallery, SystemLook,
    testing::{Harness, assert_golden},
};

const SIZE: (u32, u32) = (280, 200);
/// A point on the right edge, where the bar sits.
const EDGE_X: f32 = 276.0;

fn harness(rows: i32) -> Harness<ScrollbarGallery> {
    let h = Harness::new(ScrollbarGallery::new, SIZE.0, SIZE.1);
    h.component.set_rows(rows);
    let _ = h.render_settled();
    h
}

fn at(y: f32) -> LogicalPosition {
    LogicalPosition::new(EDGE_X, y)
}

fn press(h: &Harness<ScrollbarGallery>, p: LogicalPosition) {
    let w = h.component.window();
    w.dispatch_event(WindowEvent::PointerMoved { position: p });
    w.dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
}

fn move_to(h: &Harness<ScrollbarGallery>, p: LogicalPosition) {
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
}

fn release(h: &Harness<ScrollbarGallery>, p: LogicalPosition) {
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: p,
            button: PointerEventButton::Left,
        });
}

#[test]
fn appears_when_the_content_overflows_and_not_when_it_fits() {
    assert!(harness(20).component.get_bar_shown());
    assert!(!harness(5).component.get_bar_shown());
    // Just under the viewport height (196 of 200): nothing to scroll.
    assert!(!harness(7).component.get_bar_shown());
    let h = harness(5);
    h.component.set_rows(30);
    let _ = h.render_settled();
    assert!(h.component.get_bar_shown());
}

#[test]
fn the_thumb_is_proportional_and_follows_the_scroll_position() {
    let h = harness(20); // 560px of content in a 200px viewport
    let track = 200.0 - 4.0;
    let thumb = h.component.get_thumb_h();
    assert!((thumb - track * 200.0 / 560.0).abs() < 1.0, "thumb {thumb}");
    let top = h.component.get_thumb_y();
    assert_eq!(top, 2.0);
    // Wheel down 100px: the thumb moves by 100/360 of its travel.
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerScrolled {
            position: LogicalPosition::new(100.0, 100.0),
            delta_x: 0.0,
            delta_y: -100.0,
        });
    let _ = h.render_settled();
    assert_eq!(h.component.get_scrolled(), -100.0);
    let moved = h.component.get_thumb_y();
    assert!(
        (moved - (2.0 + (track - thumb) * 100.0 / 360.0)).abs() < 1.0,
        "thumb at {moved}"
    );
}

#[test]
fn dragging_the_thumb_scrolls() {
    let h = harness(20);
    let y0 = h.component.get_thumb_y() + h.component.get_thumb_h() / 2.0;
    press(&h, at(y0));
    move_to(&h, at(y0 + 40.0));
    let _ = h.render_settled();
    let travel = 196.0 - h.component.get_thumb_h();
    let expected = -(40.0 * 360.0 / travel);
    assert!(
        (h.component.get_scrolled() - expected).abs() < 1.0,
        "scrolled {} expected {expected}",
        h.component.get_scrolled()
    );
    // Past the end clamps.
    move_to(&h, at(1000.0));
    assert_eq!(h.component.get_scrolled(), -360.0);
    release(&h, at(1000.0));
    // A released thumb no longer follows the pointer.
    move_to(&h, at(10.0));
    assert_eq!(h.component.get_scrolled(), -360.0);
}

#[test]
fn clicking_the_track_pages() {
    let h = harness(20);
    press(&h, at(190.0));
    release(&h, at(190.0));
    assert_eq!(h.component.get_scrolled(), -200.0);
    press(&h, at(190.0));
    release(&h, at(190.0));
    assert_eq!(h.component.get_scrolled(), -360.0);
    press(&h, at(3.0));
    release(&h, at(3.0));
    assert_eq!(h.component.get_scrolled(), -160.0);
}

#[test]
fn the_hit_strip_grows_to_24px_under_the_pointer() {
    let h = harness(20);
    assert_eq!(h.component.get_bar_w(), 8.0);
    move_to(&h, at(100.0));
    h.advance(std::time::Duration::from_millis(300));
    let _ = h.render_settled();
    assert_eq!(h.component.get_bar_w(), 24.0);
}

/// A gallery that has not been drawn yet, so the first render is a frame.
fn undrawn(dark: bool) -> Harness<ScrollbarGallery> {
    let h = Harness::new(ScrollbarGallery::new, SIZE.0, SIZE.1);
    h.component.set_dark(dark);
    h
}

#[test]
fn golden_light_dark_and_high_contrast() {
    assert_golden("scrollbar-light", &undrawn(false).render_settled());
    assert_golden("scrollbar-dark", &undrawn(true).render_settled());
    let h = undrawn(false);
    h.component
        .global::<SystemLook<'_>>()
        .set_high_contrast(true);
    assert_golden("scrollbar-hc", &h.render_settled());
}

/// Its own test: the frozen clock restarts with every harness, so one that advanced time cannot be followed by another.
#[test]
fn golden_hovered_dark() {
    let h = undrawn(true);
    let _ = h.render_settled();
    move_to(&h, at(100.0));
    h.advance(std::time::Duration::from_millis(300));
    assert_golden("scrollbar-dark-hover", &h.render_settled());
}
