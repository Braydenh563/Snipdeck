//! The Colours tab scrolls its controls into view when the keyboard reaches them (not only the chip grids and the
//! format list): the sliders, the selects and the buttons. Slint does this itself (`i-slint-core` `window.rs`,
//! `publish_focus_item` calls `try_scroll_into_visible`, so on every backend), so `colours.slint` has no `reveal`
//! wiring for those controls; this test guards that they stay reachable. Time is never read.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::ElementQuery;
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::{key, on_ui_thread, press};

#[path = "support/panel_fixture.rs"]
mod fixture;

const HEIGHT: f32 = 300.0;

fn settle() {
    slint::platform::update_timers_and_animations();
}

/// Tabs from the top of the Colours tab until a control labelled `label` is in the tree (clipped controls are not),
/// and returns its y and height. `None` when it never shows.
fn tab_until_shown(ui: &PanelWindow, label: &'static str) -> Option<(f32, f32)> {
    let find = |ui: &PanelWindow| {
        ElementQuery::from_root(ui)
            .match_descendants()
            .match_predicate(move |e| e.accessible_label().as_deref() == Some(label))
            .find_first()
    };
    assert!(
        find(ui).is_none(),
        "{label} must start below the fold for this test to prove anything"
    );
    for _ in 0..80 {
        press(ui.window(), &key(Key::Tab));
        settle();
        if let Some(e) = find(ui) {
            return Some((e.absolute_position().y, e.size().height));
        }
    }
    None
}

fn colours_panel() -> PanelWindow {
    let ui = PanelWindow::new().expect("panel");
    fixture::populate(&ui);
    ui.set_colours_available(true);
    ui.show().expect("show");
    ui.window().set_size(slint::LogicalSize::new(360.0, HEIGHT));
    settle();
    ui.invoke_open_colours();
    settle();
    assert!(ui.get_colours_open());
    ui
}

#[test]
fn tabbing_to_a_control_below_the_fold_scrolls_it_into_view() {
    on_ui_thread(|| {
        // A slider, a select and a button, none of them a chip grid or the format list.
        for label in ["Alpha", "Scheme", "Save to\u{2026}"] {
            let ui = colours_panel();
            let (y, h) = tab_until_shown(&ui, label)
                .unwrap_or_else(|| panic!("tabbing never scrolled {label} into view"));
            assert!(
                y >= 0.0 && y + h <= HEIGHT,
                "{label} is inside the window: y {y} h {h} in {HEIGHT}"
            );
        }
    });
}
