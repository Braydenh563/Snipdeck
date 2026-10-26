//! Pixel measurements for the group-A primitives: hover/pressed fills, focus ring geometry,
//! list-accent colours and contrast, selected rings, reduced motion.
//!
//! Coordinates come from the gallery layout (16px padding, 72px caption column, 28px rows with a
//! 12px gap), confirmed against the accessibility tree: `IconButton` row y=16, Tag y=96, Toggle y=136,
//! Checkbox y=176, Radio y=216, Swatch y=256; the first control of a row starts at x=96 (`IconButton`,
//! Tag), 104 (Toggle, Checkbox, Radio) or 92 (Swatch, 28px each at a 32px pitch).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers: a failure here is a failed test"
)]

use std::time::Duration;

use slint::{
    ComponentHandle, LogicalPosition, SharedString,
    platform::{Key, PointerEventButton, WindowEvent},
};
use snipdeck_ui::{
    MenuGallery, PrimitivesGallery, SystemLook,
    testing::{Frame, Harness},
};

/// Literal sRGB values from `design/tokens/palette.json`, to compare against rendered pixels.
mod px {
    pub const APP_LIGHT: [u8; 3] = [0xF5, 0xF3, 0xF1];
    pub const APP_DARK: [u8; 3] = [0x11, 0x0F, 0x0D];
    pub const HOVER_LIGHT: [u8; 3] = [0xEB, 0xE9, 0xE7];
    pub const FOCUS_LIGHT: [u8; 3] = [0x28, 0x70, 0xD4];
    pub const FOCUS_DARK: [u8; 3] = [0x74, 0xAC, 0xFF];
    pub const ACCENT: [u8; 3] = [0x28, 0x70, 0xD4];
    pub const INK_LIGHT: [u8; 3] = [0x17, 0x16, 0x13];
    pub const INK_DARK: [u8; 3] = [0xEB, 0xE9, 0xE7];
    pub const SURFACE_LIGHT: [u8; 3] = [0xFB, 0xFA, 0xF7];
    pub const SURFACE_DARK: [u8; 3] = [0x17, 0x16, 0x13];
    pub const HOVER_DARK: [u8; 3] = [0x20, 0x1E, 0x1B];
    pub const BORDER_DEFAULT_DARK: [u8; 3] = [0x2D, 0x2B, 0x28];
    /// blue, teal, green, amber, orange, red, rose, violet, slate at step 600.
    pub const HUES_LIGHT: [[u8; 3]; 9] = [
        [0x28, 0x70, 0xD4],
        [0x04, 0x84, 0x84],
        [0x0A, 0x8A, 0x3F],
        [0x9A, 0x67, 0x04],
        [0xB6, 0x51, 0x03],
        [0xCA, 0x31, 0x34],
        [0xB9, 0x41, 0x6F],
        [0x7C, 0x5B, 0xC5],
        [0x66, 0x75, 0x84],
    ];
    /// The same hues at step 400.
    pub const HUES_DARK: [[u8; 3]; 9] = [
        [0x74, 0xAC, 0xFF],
        [0x59, 0xBD, 0xBD],
        [0x6B, 0xC1, 0x7F],
        [0xD9, 0x9E, 0x43],
        [0xF3, 0x8C, 0x53],
        [0xFF, 0x7E, 0x77],
        [0xEF, 0x84, 0xA7],
        [0xB1, 0x98, 0xF7],
        [0x9F, 0xAC, 0xBB],
    ];
}

fn gallery(dark: bool) -> Harness<PrimitivesGallery> {
    let h = Harness::new(PrimitivesGallery::new, 560, 290);
    h.component.set_dark(dark);
    h
}

fn pointer<C: ComponentHandle>(h: &Harness<C>, event: WindowEvent) {
    h.component.window().dispatch_event(event);
}

fn move_to<C: ComponentHandle>(h: &Harness<C>, x: f32, y: f32) {
    pointer(
        h,
        WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        },
    );
}

fn press<C: ComponentHandle>(h: &Harness<C>, x: f32, y: f32) {
    pointer(
        h,
        WindowEvent::PointerPressed {
            position: LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
    );
}

fn release<C: ComponentHandle>(h: &Harness<C>, x: f32, y: f32) {
    pointer(
        h,
        WindowEvent::PointerReleased {
            position: LogicalPosition::new(x, y),
            button: PointerEventButton::Left,
        },
    );
}

fn tab<C: ComponentHandle>(h: &Harness<C>, times: usize) {
    for _ in 0..times {
        let text: SharedString = Key::Tab.into();
        pointer(h, WindowEvent::KeyPressed { text: text.clone() });
        pointer(h, WindowEvent::KeyReleased { text });
    }
}

fn luma(p: [u8; 3]) -> u32 {
    u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114
}

/// WCAG 2.2 contrast ratio between two sRGB colours.
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

fn settle<C: ComponentHandle>(h: &Harness<C>) -> Frame {
    h.advance(Duration::from_millis(300));
    h.render()
}

/// One theme per test: the frozen clock restarts with every `Harness`, and Slint's animation clock
/// must not run backwards within a thread.
fn check_icon_button_states(dark: bool, app: [u8; 3], hover: [u8; 3]) {
    let h = gallery(dark);
    // Left of the glyph, inside the 24px Copy button (x 96..120, y 16..40).
    let probe = (98, 28);
    assert_eq!(h.render().pixel(probe.0, probe.1), app, "rest");

    move_to(&h, 108.0, 28.0);
    assert_eq!(settle(&h).pixel(probe.0, probe.1), hover, "hover");

    press(&h, 108.0, 28.0);
    let pressed = settle(&h).pixel(probe.0, probe.1);
    assert_ne!(pressed, hover, "pressed must differ from hover");
    assert_ne!(pressed, app, "pressed must differ from rest");
    // One step further from the surface: darker in light, lighter in dark.
    assert_eq!(luma(pressed) < luma(hover), !dark, "pressed direction");
    release(&h, 108.0, 28.0);

    // A disabled button (More, x 268..296) never reacts.
    move_to(&h, 282.0, 30.0);
    assert_eq!(settle(&h).pixel(270, 30), app, "disabled ignores hover");
}

#[test]
fn icon_button_hover_and_pressed_are_three_distinct_fills_light() {
    check_icon_button_states(false, px::APP_LIGHT, px::HOVER_LIGHT);
}

#[test]
fn icon_button_hover_and_pressed_are_three_distinct_fills_dark() {
    check_icon_button_states(true, px::APP_DARK, px::HOVER_DARK);
}

#[test]
fn hover_is_instant_under_reduced_motion_and_animated_otherwise() {
    let calm = gallery(false);
    calm.component
        .global::<SystemLook<'_>>()
        .set_reduced_motion(true);
    let _ = calm.render();
    move_to(&calm, 108.0, 28.0);
    assert_eq!(
        calm.render().pixel(98, 28),
        px::HOVER_LIGHT,
        "no delay under reduced motion"
    );

    let moving = gallery(false);
    let _ = moving.render();
    move_to(&moving, 108.0, 28.0);
    assert_eq!(
        moving.render().pixel(98, 28),
        px::APP_LIGHT,
        "the 90ms fade has not progressed on a frozen clock"
    );
}

/// The 2px focus ring, drawn 2px outside the control: `x` is the control's left edge.
fn assert_ring(frame: &Frame, ring: [u8; 3], bg: [u8; 3], x: u32, y: u32, what: &str) {
    assert_eq!(
        frame.pixel(x - 5, y),
        bg,
        "{what}: nothing outside the ring"
    );
    assert_eq!(frame.pixel(x - 4, y), ring, "{what}: ring pixel 1");
    assert_eq!(frame.pixel(x - 3, y), ring, "{what}: ring pixel 2");
    assert_eq!(frame.pixel(x - 2, y), bg, "{what}: gap pixel 1");
    assert_eq!(frame.pixel(x - 1, y), bg, "{what}: gap pixel 2");
}

#[test]
fn keyboard_focus_draws_a_2px_ring_with_2px_offset_and_clicks_do_not() {
    // (tab presses to reach it, control left edge, vertical middle, what)
    let controls = [
        (1, 96, 28, "IconButton"),
        (8, 219, 108, "Tag"),
        (10, 104, 148, "Toggle"),
        (13, 104, 188, "Checkbox"),
        (16, 104, 228, "Radio"),
        (18, 92, 270, "Swatch"),
    ];
    for (dark, ring, bg) in [
        (false, px::FOCUS_LIGHT, px::APP_LIGHT),
        (true, px::FOCUS_DARK, px::APP_DARK),
    ] {
        for (presses, x, y, what) in controls {
            let h = gallery(dark);
            assert_eq!(
                h.render().pixel(x - 4, y),
                bg,
                "{what}: no ring before focus"
            );
            tab(&h, presses);
            assert_ring(
                &h.render(),
                ring,
                bg,
                x,
                y,
                &format!("{what} (dark={dark})"),
            );
        }
    }

    // A pointer click never shows the ring (the panel must not look focused on click).
    let h = gallery(false);
    let _ = h.render();
    move_to(&h, 108.0, 28.0);
    press(&h, 108.0, 28.0);
    release(&h, 108.0, 28.0);
    assert_eq!(settle(&h).pixel(92, 28), px::APP_LIGHT);
}

#[test]
fn swatches_use_step_600_in_light_and_step_400_in_dark_with_enough_contrast() {
    for (dark, expected, surface) in [
        (false, px::HUES_LIGHT, px::SURFACE_LIGHT),
        (true, px::HUES_DARK, px::SURFACE_DARK),
    ] {
        let frame = gallery(dark).render();
        for (i, hue) in expected.iter().enumerate() {
            // 28px swatches at a 32px pitch from x=92; sample 6px below the top of the 20px chip.
            let x = 92 + 14 + 32 * u32::try_from(i).expect("small index");
            assert_eq!(frame.pixel(x, 256 + 6), *hue, "hue {i} (dark={dark})");
            let ratio = contrast(*hue, surface);
            assert!(
                ratio >= 4.2,
                "hue {i} (dark={dark}) is {ratio:.2}:1 on the surface"
            );
        }
    }
}

#[test]
fn selected_swatch_has_a_2px_ring_in_primary_text_colour_and_a_check() {
    for (dark, ink, bg) in [
        (false, px::INK_LIGHT, px::APP_LIGHT),
        (true, px::INK_DARK, px::APP_DARK),
    ] {
        let frame = gallery(dark).render();
        // Teal (x 124..152, y 256..284) is selected: ring 0..2, gap 2..4, chip from 4.
        assert_eq!(frame.pixel(124, 270), ink, "ring 1 (dark={dark})");
        assert_eq!(frame.pixel(125, 270), ink, "ring 2 (dark={dark})");
        assert_eq!(frame.pixel(126, 270), bg, "gap (dark={dark})");
        // Blue is not selected: no ring.
        assert_eq!(
            frame.pixel(92, 270),
            bg,
            "unselected swatch has no ring (dark={dark})"
        );
        // The check sits in the chip's middle, so the middle differs from the plain chip colour.
        let chip = if dark {
            px::HUES_DARK[1]
        } else {
            px::HUES_LIGHT[1]
        };
        assert_ne!(
            frame.pixel(138, 270),
            chip,
            "check mark visible (dark={dark})"
        );
    }
}

#[test]
fn toggle_on_is_accent_and_off_is_not() {
    for (dark, app) in [(false, px::APP_LIGHT), (true, px::APP_DARK)] {
        let frame = gallery(dark).render();
        // Track 36x20 centred in the 24px row: y 138..158. Sample the empty end of each track.
        assert_eq!(frame.pixel(160, 148), px::ACCENT, "on track (dark={dark})");
        assert_eq!(
            frame.pixel(136, 148),
            app,
            "off track interior (dark={dark})"
        );
    }
}

#[test]
fn menu_current_row_is_visibly_highlighted_in_both_themes() {
    for (dark, expected) in [(false, px::HOVER_LIGHT), (true, px::BORDER_DEFAULT_DARK)] {
        let h = Harness::new(MenuGallery::new, 260, 200);
        h.component.set_dark(dark);
        let frame = h.render();
        // Row 1 (Edit, y 48..76) is current; row 0 (Copy, y 20..48) is not. Probe the left padding.
        let current = frame.pixel(24, 62);
        let other = frame.pixel(24, 34);
        assert_ne!(current, other, "highlight must show (dark={dark})");
        assert_eq!(current, expected, "highlight colour (dark={dark})");
    }
}
