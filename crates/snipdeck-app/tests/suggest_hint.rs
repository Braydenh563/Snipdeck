//! The suggestion hint window: placed under the caret, flipped above at the bottom edge, never
//! shown without a caret, never given the keyboard, and hidden again.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test code: a failure is a failed test"
)]

use std::rc::Rc;

use snipdeck_app::{
    frame::{Monitor, Rect},
    placement::{FakePopups, place_hint},
    suggest_hint::{HintLook, SuggestHint},
};
use snipdeck_platform::route::ScreenRect;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

fn popups(scale: f32) -> Rc<FakePopups> {
    Rc::new(FakePopups::one(Monitor {
        taskbar: None,
        id: Monitor::id_of("DISPLAY1", 1920, 1080),
        work_area: Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        },
        scale,
    }))
}

const fn caret(x: i32, y: i32) -> ScreenRect {
    ScreenRect { x, y, w: 2, h: 18 }
}

#[test]
fn no_caret_no_hint() {
    on_ui_thread(|| {
        let host = popups(1.0);
        host.cursor.set(Some((900, 600)));
        let hint = SuggestHint::new(host.clone());
        assert!(!hint.show_hint("Signature", None, &HintLook::default()));
        assert!(!hint.is_showing(), "the mouse is never a fallback");
        assert_eq!(hint.placed(), None);
        // A hint that is up goes away when the caret is lost.
        assert!(hint.show_hint("Signature", Some(caret(400, 300)), &HintLook::default()));
        assert!(!hint.show_hint("Signature", None, &HintLook::default()));
        assert!(!hint.is_showing());
    });
}

#[test]
fn the_hint_sits_under_the_caret_and_flips_above_at_the_bottom() {
    on_ui_thread(|| {
        let host = popups(1.0);
        let hint = SuggestHint::new(host.clone());
        assert!(hint.show_hint("Signature", Some(caret(400, 300)), &HintLook::default()));
        let below = hint.placed().expect("placed");
        assert!(below.y >= 300 + 18, "below the caret: {below:?}");
        assert!(below.x >= 0 && below.x + i32::try_from(below.w).unwrap() <= 1920);
        assert!(hint.show_hint("Signature", Some(caret(400, 1030)), &HintLook::default()));
        let above = hint.placed().expect("placed");
        assert!(
            above.y + i32::try_from(above.h).unwrap() <= 1030,
            "flipped above the caret: {above:?}"
        );
        // The right edge of the screen pulls it back in.
        assert!(hint.show_hint("Signature", Some(caret(1915, 300)), &HintLook::default()));
        let edge = hint.placed().expect("placed");
        assert!(edge.x + i32::try_from(edge.w).unwrap() <= 1920, "{edge:?}");
    });
}

#[test]
fn the_hint_never_takes_the_keyboard_and_hides() {
    on_ui_thread(|| {
        let host = popups(1.5);
        let hint = SuggestHint::new(host.clone());
        assert!(hint.show_hint("Signature", Some(caret(100, 100)), &HintLook::default()));
        assert!(hint.is_showing());
        hint.hide_hint();
        assert!(!hint.is_showing());
        let calls = host.calls.borrow().clone();
        assert!(
            !calls.contains(&"activate"),
            "focus stays in the app: {calls:?}"
        );
        assert!(!calls.contains(&"release"), "{calls:?}");
        // Shown again, it is the same window, in the dark look.
        let look = HintLook {
            dark: true,
            ..HintLook::default()
        };
        assert!(hint.show_hint("Other", Some(caret(100, 100)), &look));
        let window = hint.window().expect("window");
        assert_eq!(window.get_item_title(), "Other");
        assert!(window.get_dark());
        // A 1.5x monitor gives a 1.5x frame.
        let frame = hint.placed().expect("placed");
        assert_eq!(frame.w, (window.get_content_width() * 1.5).ceil() as u32);
    });
}

#[test]
fn place_hint_needs_a_caret_and_a_monitor() {
    let host = popups(1.0);
    assert_eq!(place_hint(&*host, None, (100.0, 28.0)), None);
    assert!(place_hint(&*host, Some(caret(10, 10)), (100.0, 28.0)).is_some());
    let none = snipdeck_app::placement::NoPopups;
    assert_eq!(place_hint(&none, Some(caret(10, 10)), (100.0, 28.0)), None);
}
