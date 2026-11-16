//! UX-6: `PageUp` / `PageDown` and type-ahead in the floating lists (Menu, Chooser) and `PageUp` / `PageDown` in the command
//! palette. Time is mocked: the type-ahead word ends after a pause of `ListNav.pause`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::mock_elapsed_time;
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{BusyButtonGallery, ChooserNavGallery, MenuNavGallery, PaletteNavGallery};

#[path = "support/mod.rs"]
mod support;
use support::{key, on_ui_thread, press};

fn settle() {
    slint::platform::update_timers_and_animations();
}

fn pause() {
    mock_elapsed_time(std::time::Duration::from_millis(1200));
    settle();
}

fn type_str(ui: &impl ComponentHandle, text: &str) {
    for c in text.chars() {
        press(ui.window(), &c.to_string());
    }
    settle();
}

macro_rules! ui_tests {
    ($(fn $name:ident() $body:block)*) => {
        $(
            #[test]
            fn $name() {
                on_ui_thread(|| $body);
            }
        )*
    };
}

ui_tests! {
    fn menu_pages_down_and_up_by_a_page_and_stops_at_the_ends() {
        let ui = MenuNavGallery::new().expect("gallery");
        ui.show().expect("show");
        settle();
        assert_eq!(ui.get_current(), 0, "starts on the first row");
        press(ui.window(), &key(Key::PageDown));
        settle();
        let page = ui.get_current();
        assert!(page >= 3, "a page is several rows, not one: {page}");
        press(ui.window(), &key(Key::PageUp));
        settle();
        assert_eq!(ui.get_current(), 0, "PageUp returns by the same distance");
        press(ui.window(), &key(Key::PageUp));
        settle();
        assert_eq!(ui.get_current(), 0, "stops at the top, no wrap");
        for _ in 0..20 {
            press(ui.window(), &key(Key::PageDown));
        }
        settle();
        assert_eq!(ui.get_current(), 26, "stops on the last row (Tangerine), no wrap");
    }

    fn menu_type_ahead_jumps_to_the_row_that_starts_with_the_letters_and_skips_disabled_rows() {
        let ui = MenuNavGallery::new().expect("gallery");
        ui.show().expect("show");
        settle();
        type_str(&ui, "c");
        assert_eq!(ui.get_current(), 4, "c lands on Cherry");
        type_str(&ui, "h");
        assert_eq!(ui.get_current(), 4, "ch stays on Cherry");
        type_str(&ui, "e");
        type_str(&ui, "s");
        assert_eq!(ui.get_current(), 5, "ches lands on Chestnut");
        pause();
        type_str(&ui, "d");
        assert_eq!(ui.get_current(), 5, "Damson is disabled: nothing to jump to");
        pause();
        type_str(&ui, "p");
        type_str(&ui, "e");
        assert_eq!(ui.get_current(), 20, "after a pause a new word starts: pe lands on Peach");
    }

    fn menu_repeating_a_letter_walks_the_rows_that_start_with_it_and_wraps() {
        let ui = MenuNavGallery::new().expect("gallery");
        ui.show().expect("show");
        settle();
        let mut seen = Vec::new();
        for _ in 0..5 {
            type_str(&ui, "p");
            seen.push(ui.get_current());
            pause();
        }
        assert_eq!(seen, [19, 20, 21, 22, 19], "Papaya, Peach, Pear, Plum then back round");
    }

    fn chooser_pages_and_types_ahead() {
        let ui = ChooserNavGallery::new().expect("gallery");
        ui.show().expect("show");
        settle();
        press(ui.window(), &key(Key::PageDown));
        settle();
        assert_eq!(ui.get_current(), 7, "a page is the 8 rows in view less one");
        press(ui.window(), &key(Key::PageUp));
        settle();
        assert_eq!(ui.get_current(), 0);
        type_str(&ui, "n");
        assert_eq!(ui.get_current(), 16, "n lands on Nectarine");
        pause();
        type_str(&ui, "l");
        assert_eq!(ui.get_current(), 13, "l lands on Lemon");
    }

    fn the_palette_pages_and_ctrl_home_end_reach_the_ends_without_stealing_the_field_keys() {
        use support::chord;
        let ui = PaletteNavGallery::new().expect("gallery");
        ui.show().expect("show");
        settle();
        press(ui.window(), &key(Key::PageDown));
        settle();
        let page = ui.get_current();
        assert!(page >= 3, "a page is several rows: {page}");
        press(ui.window(), &key(Key::PageUp));
        settle();
        assert_eq!(ui.get_current(), 0);
        chord(ui.window(), &[Key::Control], &key(Key::End));
        settle();
        assert_eq!(ui.get_current(), 29, "Ctrl+End is the last command");
        chord(ui.window(), &[Key::Control], &key(Key::Home));
        settle();
        assert_eq!(ui.get_current(), 0, "Ctrl+Home is the first");
        // A plain letter is text for the field, not type-ahead: the highlight stays.
        press(ui.window(), "t");
        settle();
        assert_eq!(ui.get_current(), 0, "typing filters (the host's job), it does not jump");
    }

    // UX-25: a busy button keeps the keyboard focus and ignores the second click.
    fn a_busy_button_keeps_focus_and_ignores_a_second_press() {
        let ui = BusyButtonGallery::new().expect("gallery");
        ui.show().expect("show");
        settle();
        assert!(ui.get_has_focus(), "focused at the start");
        press(ui.window(), "\n");
        assert_eq!(ui.get_clicks(), 1);
        ui.set_busy(true);
        settle();
        assert!(ui.get_has_focus(), "the job started: the focus stays");
        press(ui.window(), "\n");
        press(ui.window(), " ");
        assert_eq!(ui.get_clicks(), 1, "no second job while one runs");
        ui.set_busy(false);
        settle();
        press(ui.window(), "\n");
        assert_eq!(ui.get_clicks(), 2, "the button answers again when the job ends");
    }
}
