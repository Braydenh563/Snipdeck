//! Helpers shared by the input tests (`#[path]`-included, so each test binary compiles its own copy).
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test support: each test binary uses a subset, and a failure here is a failed test"
)]

use std::sync::{Mutex, OnceLock, mpsc};

use i_slint_core::{
    input::{InternalKeyEvent, KeyEventType},
    items::KeyEvent,
};
use slint::{
    Window,
    platform::{Key, WindowEvent},
};

/// Runs a test body on the one thread that owns Slint's testing platform (it is process-wide and bound to a
/// single thread, but the test harness gives every test a thread of its own). Mock time is shared by all tests.
/// A panic in `f` is re-raised in the calling test, so it fails that test alone.
pub fn on_ui_thread(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static UI: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
    let sender = UI.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            i_slint_backend_testing::init_integration_test_with_mock_time();
            for job in rx {
                job();
            }
        });
        Mutex::new(tx)
    });
    let (done_tx, done_rx) = mpsc::channel();
    let job: Job = Box::new(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = done_tx.send(outcome);
    });
    sender
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .send(job)
        .expect("the UI thread is alive");
    if let Err(panic) = done_rx.recv().expect("the UI thread answers") {
        std::panic::resume_unwind(panic);
    }
}

/// Sets the IME composition (pre-edit text) of the focused text input, as winit's `Ime::Preedit` does.
pub fn compose(window: &Window, preedit: &str) {
    window.dispatch_event(WindowEvent::internal(InternalKeyEvent {
        event_type: KeyEventType::UpdateComposition,
        preedit_text: preedit.into(),
        ..Default::default()
    }));
}

/// Commits `text` and ends the composition, as winit's `Ime::Commit` does.
pub fn commit(window: &Window, text: &str) {
    let mut key_event = KeyEvent::default();
    key_event.text = text.into();
    window.dispatch_event(WindowEvent::internal(InternalKeyEvent {
        event_type: KeyEventType::CommitComposition,
        key_event,
        ..Default::default()
    }));
}

/// Types text one character at a time (key press and release).
pub fn type_text(window: &Window, text: &str) {
    for c in text.chars() {
        press(window, &c.to_string());
    }
}

/// One key press and release; `key` is a character or a [`Key`] converted to a string.
pub fn press(window: &Window, key: &str) {
    window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

/// Holds the `modifiers`, presses `key`, then releases everything (a shortcut such as Ctrl+Alt+K).
pub fn chord(window: &Window, modifiers: &[Key], key: &str) {
    for m in modifiers {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: char::from(*m).to_string().into(),
        });
    }
    window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    for m in modifiers.iter().rev() {
        window.dispatch_event(WindowEvent::KeyReleased {
            text: char::from(*m).to_string().into(),
        });
    }
}

pub fn key(k: Key) -> String {
    char::from(k).to_string()
}
