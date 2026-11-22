//! LIBRARY-18: the month grid of a date field. The calendar arithmetic lives in Slint, so it is checked against an
//! independent day count here, for every month of 1990 to 2060 and both week starts; then the keyboard and the mouse.
//! Time is fixed by the gallery (today is 2026-10-17), never the real clock.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::MonthGridGallery;

#[path = "support/mod.rs"]
mod support;
use support::{key, on_ui_thread, press};

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn month_length(y: i64, m: i64) -> i64 {
    let next = if m == 12 {
        days(y + 1, 1, 1)
    } else {
        days(y, m + 1, 1)
    };
    next - days(y, m, 1)
}

/// 0 = Monday (1970-01-01 was a Thursday).
fn monday0(y: i64, m: i64, d: i64) -> i64 {
    (days(y, m, d) + 3).rem_euclid(7)
}

fn grid() -> MonthGridGallery {
    let ui = MonthGridGallery::new().expect("grid");
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    ui
}

fn cells(ui: &MonthGridGallery) -> Vec<i32> {
    (0..42).map(|i| ui.invoke_day_at(i)).collect()
}

#[test]
fn library_18_every_month_lays_out_as_the_calendar_says() {
    on_ui_thread(|| {
        let ui = grid();
        for sunday in [false, true] {
            ui.set_sunday(sunday);
            for y in 1990..=2060 {
                for m in 1..=12 {
                    ui.set_view_year(y);
                    ui.set_view_month(m);
                    let first = monday0(i64::from(y), i64::from(m), 1);
                    let lead = if sunday { (first + 1) % 7 } else { first };
                    assert_eq!(i64::from(ui.get_lead()), lead, "{y}-{m} sunday={sunday}");
                    let len = month_length(i64::from(y), i64::from(m));
                    assert_eq!(i64::from(ui.get_month_length()), len, "{y}-{m}");
                    let shown = cells(&ui);
                    for (i, day) in shown.iter().enumerate() {
                        let want = i64::try_from(i).unwrap() - lead + 1;
                        let want = if (1..=len).contains(&want) { want } else { 0 };
                        assert_eq!(i64::from(*day), want, "{y}-{m} cell {i} sunday={sunday}");
                    }
                }
            }
        }
    });
}

#[test]
fn library_18_the_arrows_cross_month_and_year_ends() {
    on_ui_thread(|| {
        let ui = grid();
        let w = ui.window();
        ui.set_view_year(2026);
        ui.set_view_month(12);
        ui.set_cursor_day(31);
        press(w, &key(Key::RightArrow));
        assert_eq!(
            (ui.get_view_year(), ui.get_view_month(), ui.get_cursor_day()),
            (2027, 1, 1)
        );
        press(w, &key(Key::LeftArrow));
        assert_eq!(
            (ui.get_view_year(), ui.get_view_month(), ui.get_cursor_day()),
            (2026, 12, 31)
        );
        press(w, &key(Key::DownArrow));
        assert_eq!(
            (ui.get_view_year(), ui.get_view_month(), ui.get_cursor_day()),
            (2027, 1, 7)
        );
        press(w, &key(Key::UpArrow));
        assert_eq!(
            (ui.get_view_year(), ui.get_view_month(), ui.get_cursor_day()),
            (2026, 12, 31)
        );
        // A month with fewer days keeps the cursor inside it.
        ui.set_view_month(1);
        ui.set_view_year(2027);
        ui.set_cursor_day(31);
        press(w, &key(Key::PageDown));
        assert_eq!(
            (ui.get_view_year(), ui.get_view_month(), ui.get_cursor_day()),
            (2027, 2, 28)
        );
        press(w, &key(Key::PageUp));
        press(w, &key(Key::PageUp));
        assert_eq!((ui.get_view_year(), ui.get_view_month()), (2026, 12));
        press(w, &key(Key::Home));
        assert_eq!(ui.get_cursor_day(), 1);
        press(w, &key(Key::End));
        assert_eq!(ui.get_cursor_day(), 31, "December has 31 days");
    });
}

#[test]
fn library_18_enter_and_space_pick_the_day_and_escape_closes() {
    on_ui_thread(|| {
        let ui = grid();
        let w = ui.window();
        ui.set_view_year(2026);
        ui.set_view_month(10);
        ui.set_cursor_day(7);
        press(w, &key(Key::Return));
        assert_eq!(
            (ui.get_got_year(), ui.get_got_month(), ui.get_got_day()),
            (2026, 10, 7)
        );
        press(w, &key(Key::RightArrow));
        press(w, " ");
        assert_eq!(ui.get_got_day(), 8);
        assert_eq!(ui.get_dismissed_count(), 0);
        press(w, &key(Key::Escape));
        assert_eq!(ui.get_dismissed_count(), 1);
        assert_eq!(ui.get_got_day(), 8, "Esc picks nothing");
    });
}
