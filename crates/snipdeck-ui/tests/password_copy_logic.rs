//! PRIVACY-4: a password field never copies or cuts, by key or by menu. Driven through Slint's testing
//! backend, whose clipboard is in memory: after every copy or cut attempt, a paste must bring back what was there before.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::UnlockWindow;

#[path = "support/mod.rs"]
mod support;
use support::{chord, key, on_ui_thread, press, type_text};

const SECRET: &str = "hunter2-secret";

fn attempt(name: &str, modifiers: &[Key], keystroke: &str) {
    let ui = UnlockWindow::new().expect("window");
    ui.show().expect("show");
    let window = ui.window();
    // Nothing typed yet: the field has the focus (the host bumps focus-request when it shows the window).
    ui.set_focus_request(1);
    slint::platform::update_timers_and_animations();
    // Whatever an earlier test left on the (shared, in-memory) clipboard is the baseline.
    chord(window, &[Key::Control], "v");
    let baseline = ui.get_password().to_string();
    chord(window, &[Key::Control], "a");
    press(window, &key(Key::Delete));
    type_text(window, SECRET);
    assert_eq!(
        ui.get_password(),
        SECRET,
        "{name}: typing reached the field"
    );
    chord(window, &[Key::Control], "a");
    chord(window, modifiers, keystroke);
    assert_eq!(
        ui.get_password(),
        SECRET,
        "{name}: a cut must not remove the selection"
    );
    // Drop the selection, then paste: the clipboard must hold nothing from the field.
    press(window, &key(Key::Delete));
    assert_eq!(ui.get_password(), "", "{name}: the selection was replaced");
    chord(window, &[Key::Control], "v");
    assert_eq!(
        ui.get_password(),
        baseline,
        "{name}: the password reached the clipboard"
    );
}

#[test]
fn ctrl_c_does_not_copy_a_password() {
    on_ui_thread(|| attempt("Ctrl+C", &[Key::Control], "c"));
}

#[test]
fn ctrl_x_does_not_cut_a_password() {
    on_ui_thread(|| attempt("Ctrl+X", &[Key::Control], "x"));
}

#[test]
fn ctrl_insert_does_not_copy_a_password() {
    on_ui_thread(|| attempt("Ctrl+Insert", &[Key::Control], &key(Key::Insert)));
}

#[test]
fn shift_delete_does_not_cut_a_password() {
    on_ui_thread(|| attempt("Shift+Delete", &[Key::Shift], &key(Key::Delete)));
}

// ---- PRIVACY-5: a field that holds a secret copies through the host, never the system clipboard -------------

use i_slint_backend_testing as _;
use slint::{ModelRc, SharedString, VecModel};
use snipdeck_ui::ItemEditor;
use std::{cell::RefCell, rc::Rc};

type Seen = Rc<RefCell<Vec<(String, i32, i32, bool)>>>;

/// An editor with the content `text`, `sensitive` or not, the whole text selected, and the host's hooks logging.
fn editor_with_selection(sensitive: bool) -> (ItemEditor, Seen) {
    let ui = ItemEditor::new().expect("editor");
    ui.window().set_size(slint::LogicalSize::new(640.0, 1200.0));
    ui.set_content("hunter2 and more".into());
    ui.set_sensitive(sensitive);
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    ui.invoke_place_caret(0);
    slint::platform::update_timers_and_animations();
    let seen: Seen = Rc::default();
    {
        let seen = seen.clone();
        ui.on_private_copied(move |t, a, b| seen.borrow_mut().push((t.to_string(), a, b, false)));
    }
    {
        let seen = seen.clone();
        ui.on_private_cut(move |t, a, b| {
            seen.borrow_mut().push((t.to_string(), a, b, true));
            ModelRc::new(VecModel::from(vec![SharedString::from("rest")]))
        });
    }
    chord(ui.window(), &[Key::Control], "a");
    (ui, seen)
}

#[test]
fn copy_and_cut_in_a_sensitive_editor_go_to_the_host() {
    on_ui_thread(|| {
        for (name, modifiers, keystroke, cut) in [
            ("Ctrl+C", vec![Key::Control], "c".to_string(), false),
            ("Ctrl+Insert", vec![Key::Control], key(Key::Insert), false),
            ("Ctrl+X", vec![Key::Control], "x".to_string(), true),
            ("Shift+Delete", vec![Key::Shift], key(Key::Delete), true),
        ] {
            let (ui, seen) = editor_with_selection(true);
            chord(ui.window(), &modifiers, &keystroke);
            assert_eq!(
                *seen.borrow(),
                [("hunter2 and more".to_string(), 0, 16, cut)].map(|(t, a, b, c)| (t, a, b, c)),
                "{name}"
            );
            let expect = if cut { "rest" } else { "hunter2 and more" };
            assert_eq!(ui.get_content(), expect, "{name}");
        }
    });
}

#[test]
fn an_ordinary_editor_keeps_the_fields_own_copy() {
    on_ui_thread(|| {
        let (ui, seen) = editor_with_selection(false);
        chord(ui.window(), &[Key::Control], "c");
        chord(ui.window(), &[Key::Control], "x");
        assert!(seen.borrow().is_empty());
    });
}
