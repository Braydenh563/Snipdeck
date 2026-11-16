//! PRIVACY-4: a password field never copies or cuts, by key or by menu. Driven through Slint's testing
//! backend, whose clipboard is in memory: after every copy or cut attempt, a paste must bring back nothing.

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
        "",
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
