//! LIBRARY-18: screenshots of the month grid of a date field: light, dark, high contrast and 225% text. The grid
//! shows October 2026 with the 7th picked, today (the 17th) ringed and the cursor on the 7th. Fixed dates, never the
//! real clock. Create goldens with `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test month_grid`.

use slint::ComponentHandle;
use snipdeck_ui::{
    MonthGridGallery, SystemLook,
    testing::{Harness, assert_golden},
};

fn grid(width: u32, height: u32) -> Harness<MonthGridGallery> {
    let h = Harness::new(MonthGridGallery::new, width, height);
    h.component.set_picked_day(7);
    h.component.set_focused(true);
    // The cursor is on the 21st, apart from today (the 17th) and the picked day (the 7th).
    h.component.set_cursor_day(21);
    h
}

#[test]
fn month_grid_light() {
    assert_golden("month-grid-light", &grid(360, 330).render());
}

#[test]
fn month_grid_dark() {
    let h = grid(360, 330);
    h.component.set_dark(true);
    assert_golden("month-grid-dark", &h.render());
}

#[test]
fn month_grid_week_starting_on_sunday() {
    let h = grid(360, 330);
    h.component.set_sunday(true);
    assert_golden("month-grid-sunday-light", &h.render());
}

#[test]
fn month_grid_high_contrast() {
    let h = grid(360, 330);
    let look = h.component.global::<SystemLook<'_>>();
    look.set_hc_window(slint::Color::from_rgb_u8(0, 0, 0));
    look.set_hc_window_text(slint::Color::from_rgb_u8(0xFF, 0xFF, 0xFF));
    look.set_hc_highlight(slint::Color::from_rgb_u8(0x1A, 0xEB, 0xFF));
    look.set_high_contrast(true);
    assert_golden("month-grid-hc", &h.render());
}

#[test]
fn month_grid_at_225_percent_text_size() {
    let h = grid(560, 760);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    assert_golden("month-grid-scale-light", &h.render());
}
