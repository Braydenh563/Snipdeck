//! The template placeholder help in the item editor is a grid: every chip starts in one shared column, so the
//! Supporter badge and the help text start at the same x on every row, however long each token is. The badge and the
//! chip are both 24 px and share a centre line; at a narrow window and a large text size the help wraps instead of
//! leaving the window.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use slint::{ComponentHandle, Global};
use snipdeck_ui::{ItemEditor, SystemLook};

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

#[path = "support/editor_fixture.rs"]
mod fixture;

fn settle() {
    slint::platform::update_timers_and_animations();
}

/// A shown template editor; `free` hides the prompted placeholders' availability (the Supporter badges show).
fn editor(width: f32, scale: f32, free: bool) -> ItemEditor {
    let ui = ItemEditor::new().expect("item editor");
    fixture::text_item(&ui);
    ui.set_kind_index(3);
    ui.set_prompted_available(!free);
    SystemLook::get(&ui).set_text_scale(scale);
    ui.window().set_size(slint::LogicalSize::new(width, 3000.0));
    ui.show().expect("show");
    settle();
    ui
}

struct Rows {
    /// x and width of each row's help text.
    help: Vec<(f32, f32)>,
    /// x of each chip.
    chips: Vec<f32>,
    /// (x, y, height) of each badge.
    badges: Vec<(f32, f32, f32)>,
    /// (y, height) of each chip.
    chip_boxes: Vec<(f32, f32)>,
}

fn rows(ui: &ItemEditor) -> Rows {
    // One element per row: the query reports each instance once per repeated copy, so drop duplicates.
    let mut help: Vec<(f32, f32, f32)> = ElementQuery::from_root(ui)
        .match_descendants()
        .match_type_name("PlaceholderHelpText")
        .find_all()
        .into_iter()
        .map(|e| {
            (
                e.absolute_position().x,
                e.absolute_position().y,
                e.size().width,
            )
        })
        .collect();
    help.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)));
    help.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5);
    let help = help.into_iter().map(|(x, _, w)| (x, w)).collect();
    let mut chip_els: Vec<ElementHandle> = ElementQuery::from_root(ui)
        .match_descendants()
        .match_accessible_role(AccessibleRole::Button)
        .find_all()
        .into_iter()
        .filter(|e| {
            e.accessible_label()
                .is_some_and(|l| l.starts_with("Insert {"))
        })
        .collect();
    chip_els.sort_by(|a, b| a.absolute_position().y.total_cmp(&b.absolute_position().y));
    chip_els.dedup_by(|a, b| (a.absolute_position().y - b.absolute_position().y).abs() < 0.5);
    let mut badges: Vec<(f32, f32, f32)> = ElementHandle::find_by_accessible_label(ui, "Supporter")
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Text))
        .map(|e| {
            (
                e.absolute_position().x,
                e.absolute_position().y,
                e.size().height,
            )
        })
        .collect();
    badges.sort_by(|a, b| a.1.total_cmp(&b.1));
    badges.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5);
    Rows {
        help,
        chips: chip_els.iter().map(|e| e.absolute_position().x).collect(),
        chip_boxes: chip_els
            .iter()
            .map(|e| (e.absolute_position().y, e.size().height))
            .collect(),
        badges,
    }
}

fn all_equal(xs: impl IntoIterator<Item = f32>) -> bool {
    let xs: Vec<f32> = xs.into_iter().collect();
    xs.windows(2).all(|w| (w[0] - w[1]).abs() < 0.5)
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
    fn the_help_text_starts_at_one_x_on_every_row() {
        for (free, width) in [(false, 560.0), (true, 560.0), (true, 420.0)] {
            let r = rows(&editor(width, 1.0, free));
            eprintln!("free={free} width={width} help x {:?} chips x {:?} badges x {:?}",
                r.help.iter().map(|h| h.0).collect::<Vec<_>>(), r.chips,
                r.badges.iter().map(|b| b.0).collect::<Vec<_>>());
            assert_eq!(r.help.len(), 11, "one help text per placeholder");
            assert!(all_equal(r.help.iter().map(|h| h.0)), "free={free} width={width}: help x {:?}", r.help);
        }
    }

    fn the_chips_share_one_left_edge_and_the_badges_one_column() {
        let r = rows(&editor(560.0, 1.0, true));
        assert!(all_equal(r.chips.iter().copied()), "chip x {:?}", r.chips);
        assert_eq!(r.badges.len(), 5, "one badge per prompted placeholder");
        assert!(all_equal(r.badges.iter().map(|b| b.0)), "badge x {:?}", r.badges);
    }

    fn the_badge_and_its_chip_are_24px_and_share_a_centre() {
        let r = rows(&editor(560.0, 1.0, true));
        for (i, badge) in r.badges.iter().enumerate() {
            let (chip_y, chip_h) = r.chip_boxes[6 + i];
            assert!((badge.2 - 24.0).abs() < 0.5 && (chip_h - 24.0).abs() < 0.5, "badge {} chip {chip_h}", badge.2);
            assert!(((badge.1 + badge.2 / 2.0) - (chip_y + chip_h / 2.0)).abs() < 0.5, "centres {badge:?} {chip_y} {chip_h}");
        }
    }

    fn a_narrow_window_with_large_text_keeps_the_help_inside_the_window() {
        let r = rows(&editor(420.0, 2.25, true));
        assert!(all_equal(r.help.iter().map(|h| h.0)), "help x {:?}", r.help);
        for (x, w) in &r.help {
            assert!(*w >= 40.0 && x + w <= 420.0 + 0.5, "help at {x} width {w} leaves the 420 px window");
        }
    }
}
