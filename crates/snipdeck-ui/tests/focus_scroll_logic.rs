//! Focus audit for every form that scrolls. Slint treats an item clipped out of a `Flickable` as invisible and drops
//! the focus when the next key arrives, so a form must scroll each focused control into view (`reveal`). Each test
//! shrinks the window so the form scrolls, then Tabs (and Shift+Tabs) through every stop. After each stop F1 must
//! reach the window's key handler (that is only so while some visible control holds the focus), and a text field
//! that took the focus must lie inside the window. Time is never read.
//!
//! Surfaces with a scroll area: the item editor, the list and board editors, Settings (its import preview is a
//! panel of the same page) and Help. The template fill form, the master password form and the unlock window have
//! no scroll area (their rows are laid out to fit), so there is nothing to lose focus in.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::Cell, rc::Rc};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery, ElementRoot};
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{BoardEditor, ItemEditor, ListEditor, SettingsPage};

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

#[path = "support/editor_fixture.rs"]
mod fixture;

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

fn settle() {
    slint::platform::update_timers_and_animations();
}

/// The text of every text field, in tree order.
fn field_values(root: &impl ElementRoot) -> Vec<(ElementHandle, String)> {
    ElementQuery::from_root(root)
        .match_descendants()
        .match_accessible_role(AccessibleRole::TextInput)
        .find_all()
        .into_iter()
        .map(|e| {
            let v = e
                .accessible_value()
                .map(|v| v.to_string())
                .unwrap_or_default();
            (e, v)
        })
        .collect()
}

/// What a walk saw.
struct Walk {
    /// Tab stops that did not reach the key handler (the focus was lost).
    lost: Vec<usize>,
    /// Text fields that took the focus but lay outside the window (not revealed).
    hidden: Vec<String>,
    /// Text fields (by accessible label) that took the focus.
    reached: Vec<String>,
}

/// Presses `tab` up to `stops` times; `help` counts the key handler's F1 callback.
fn walk(
    ui: &impl ComponentHandle,
    help: &Rc<Cell<u32>>,
    tab: Key,
    stops: usize,
    height: f32,
) -> Walk {
    let mut out = Walk {
        lost: vec![],
        hidden: vec![],
        reached: vec![],
    };
    for stop in 0..stops {
        if tab == Key::Backtab {
            chord(ui.window(), &[Key::Shift], &key(Key::Backtab));
        } else {
            press(ui.window(), &key(Key::Tab));
        }
        settle();
        let before = help.get();
        press(ui.window(), &key(Key::F1));
        if help.get() != before + 1 {
            out.lost.push(stop);
            // The focus is gone for good: the next Tab would start again from the top, hiding the rest.
            break;
        }
        // Type one character: the field whose text changed is the one that holds the focus.
        let snapshot = field_values(ui);
        type_text(ui.window(), "~");
        settle();
        for ((e, was), (_, now)) in snapshot.iter().zip(field_values(ui)) {
            if *was != now {
                let label = e
                    .accessible_label()
                    .map(|l| l.to_string())
                    .unwrap_or_default();
                let (p, s) = (e.absolute_position(), e.size());
                if p.y < 0.0 || p.y + s.height > height {
                    out.hidden
                        .push(format!("{label} at y {} h {} in {height}", p.y, s.height));
                }
                out.reached.push(label);
            }
        }
    }
    out
}

fn assert_kept(name: &str, w: &Walk) {
    assert!(
        w.lost.is_empty(),
        "{name}: the focus was lost at Tab stop(s) {:?}",
        w.lost
    );
    assert!(
        !w.reached.is_empty(),
        "{name}: no text field took the focus, so the walk proved nothing"
    );
    assert!(
        w.hidden.is_empty(),
        "{name}: focused field not scrolled into view: {:?}",
        w.hidden
    );
}

fn item_editor(kind: i32, height: f32) -> (ItemEditor, Rc<Cell<u32>>) {
    let ui = ItemEditor::new().expect("item editor");
    fixture::text_item(&ui);
    ui.set_kind_index(kind);
    ui.set_more_open(true);
    ui.window().set_size(slint::LogicalSize::new(560.0, height));
    ui.show().expect("show");
    settle();
    ui.invoke_focus_title();
    settle();
    let help = Rc::new(Cell::new(0));
    let h = help.clone();
    ui.on_help_requested(move || h.set(h.get() + 1));
    (ui, help)
}

ui_tests! {
    fn the_item_editor_keeps_the_focus_through_every_tab_stop_while_it_scrolls() {
        for (kind, name) in [(0, "text"), (1, "code"), (2, "link"), (3, "template")] {
            let (ui, help) = item_editor(kind, 330.0);
            assert_kept(name, &walk(&ui, &help, Key::Tab, 40, 330.0));
        }
    }

    fn the_item_editor_keeps_the_focus_going_backwards_too() {
        for (kind, name) in [(0, "text"), (3, "template")] {
            let (ui, help) = item_editor(kind, 330.0);
            assert_kept(name, &walk(&ui, &help, Key::Backtab, 80, 330.0));
        }
    }

    fn the_item_editor_keeps_the_focus_after_the_kind_is_picked_with_the_pointer() {
        // A clipped chip is not in the accessibility tree: pick in a tall window, then shrink it so the taller
        // Template form scrolls, and walk.
        let (ui, help) = item_editor(0, 900.0);
        let chip = ElementHandle::find_by_accessible_label(&ui, "Template")
            .find(|e| e.accessible_role() == Some(AccessibleRole::RadioButton))
            .expect("template chip");
        chip.mock_single_click(slint::platform::PointerEventButton::Left);
        settle();
        assert_eq!(ui.get_kind_index(), 3);
        ui.window().set_size(slint::LogicalSize::new(560.0, 330.0));
        settle();
        assert_kept("template after the pick", &walk(&ui, &help, Key::Tab, 40, 330.0));
    }

    fn the_list_editor_keeps_the_focus_through_every_tab_stop_while_it_scrolls() {
        let ui = ListEditor::new().expect("list editor");
        fixture::list_editor_existing(&ui);
        ui.window().set_size(slint::LogicalSize::new(480.0, 200.0));
        ui.show().expect("show");
        settle();
        let help = Rc::new(Cell::new(0));
        let h = help.clone();
        ui.on_help_requested(move || h.set(h.get() + 1));
        // The name field has the focus when the window opens; the walk then visits every other stop (the colours,
        // the sort order, the buttons), none of which is a text field.
        type_text(ui.window(), "~");
        settle();
        assert_eq!(ui.get_name(), "~Email replies", "the name field has the focus at the start");
        let w = walk(&ui, &help, Key::Tab, 30, 200.0);
        assert!(w.lost.is_empty(), "list editor: the focus was lost at Tab stop(s) {:?}", w.lost);
        assert!(w.hidden.is_empty(), "list editor: focused field not scrolled into view: {:?}", w.hidden);
    }

    fn the_board_editor_keeps_the_focus_through_every_tab_stop_while_it_scrolls() {
        let ui = BoardEditor::new().expect("board editor");
        ui.set_name("Personal".into());
        ui.set_can_delete(true);
        ui.window().set_size(slint::LogicalSize::new(480.0, 200.0));
        ui.show().expect("show");
        settle();
        let help = Rc::new(Cell::new(0));
        let h = help.clone();
        ui.on_help_requested(move || h.set(h.get() + 1));
        press(ui.window(), &key(Key::Tab));
        assert_kept("board editor", &walk(&ui, &help, Key::Tab, 30, 200.0));
    }

    fn settings_keeps_the_focus_through_every_tab_stop_while_it_scrolls() {
        let ui = SettingsPage::new().expect("settings");
        fixture::settings(&ui);
        ui.window().set_size(slint::LogicalSize::new(640.0, 360.0));
        ui.show().expect("show");
        settle();
        let help = Rc::new(Cell::new(0));
        let h = help.clone();
        ui.on_help_requested(move || h.set(h.get() + 1));
        press(ui.window(), &key(Key::Tab));
        assert_kept("settings", &walk(&ui, &help, Key::Tab, 120, 360.0));
    }
}
