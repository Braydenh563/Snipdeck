//! Screenshot tests for the button component.

#![allow(
    clippy::expect_used,
    reason = "test helpers: a failure here is a failed test"
)]

use std::time::Duration;

use slint::{
    ComponentHandle, LogicalPosition,
    platform::{PointerEventButton, WindowEvent},
};
use snipdeck_ui::{
    ButtonGallery,
    testing::{Frame, Harness, assert_golden},
};

#[test]
fn button_gallery_light() {
    let h = Harness::new(ButtonGallery::new, 420, 220);
    assert_golden("buttons-light", &h.render());
}

#[test]
fn button_gallery_dark() {
    let h = Harness::new(ButtonGallery::new, 420, 220);
    h.component.set_dark(true);
    assert_golden("buttons-dark", &h.render());
}

// ---- Pressed is distinct from hover (StateColor.pressed()) ----

const BORDER_STRONG: [u8; 3] = [0x7C, 0x7A, 0x76];
const SURFACE: [[u8; 3]; 2] = [[0xFB, 0xFA, 0xF7], [0x17, 0x16, 0x13]];
const HOVER: [[u8; 3]; 2] = [[0xEB, 0xE9, 0xE7], [0x20, 0x1E, 0x1B]];

/// The first and last row of `column` between `y0..y1` whose pixel is `colour`.
fn vertical_extent(frame: &Frame, column: u32, colour: [u8; 3], y0: u32, y1: u32) -> (u32, u32) {
    let hits: Vec<u32> = (y0..y1)
        .filter(|y| frame.pixel(column, *y) == colour)
        .collect();
    (
        *hits.first().expect("border found"),
        *hits.last().expect("border found"),
    )
}

/// One theme per test: the harness clock restarts at zero with each harness, and a thread's animation
/// clock must not run backwards.
fn pressed_differs_from_hover(dark: bool) {
    let n = usize::from(dark);
    {
        let h = Harness::new(ButtonGallery::new, 420, 220);
        h.component.set_dark(dark);
        let rest = h.render();
        // The large secondary button (second row, third column) sits around x 161..225.
        let (top, bottom) = vertical_extent(&rest, 190, BORDER_STRONG, 50, 110);
        let mid_y = u32::midpoint(top, bottom);
        // Walking left from the middle of the button, the first border pixel is its left edge.
        let left = (130..=190)
            .rev()
            .find(|x| rest.pixel(*x, mid_y) == BORDER_STRONG)
            .expect("left border");
        let probe = (left + 6, mid_y);
        assert_eq!(
            rest.pixel(probe.0, probe.1),
            SURFACE[n],
            "rest (dark={dark})"
        );

        #[allow(clippy::cast_precision_loss, reason = "a small pixel coordinate")]
        let at = LogicalPosition::new(190.0, mid_y as f32);
        let w = h.component.window();
        w.dispatch_event(WindowEvent::PointerMoved { position: at });
        h.advance(Duration::from_millis(200));
        let hover = h.render().pixel(probe.0, probe.1);
        assert_eq!(hover, HOVER[n], "hover (dark={dark})");

        w.dispatch_event(WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Left,
        });
        h.advance(Duration::from_millis(200));
        let pressed = h.render().pixel(probe.0, probe.1);
        assert_ne!(pressed, hover, "pressed differs from hover (dark={dark})");
        assert_ne!(pressed, SURFACE[n], "and from rest (dark={dark})");
        let sum = |c: [u8; 3]| c.iter().map(|v| u32::from(*v)).sum::<u32>();
        if dark {
            assert!(sum(pressed) > sum(hover), "dark steps lighter: {pressed:?}");
        } else {
            assert!(sum(pressed) < sum(hover), "light steps darker: {pressed:?}");
        }
    }
}

#[test]
fn a_pressed_secondary_button_is_a_step_beyond_hover_light() {
    pressed_differs_from_hover(false);
}

#[test]
fn a_pressed_secondary_button_is_a_step_beyond_hover_dark() {
    pressed_differs_from_hover(true);
}
