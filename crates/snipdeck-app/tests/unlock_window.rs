//! The real `UnlockWindow` (the master password prompt) bound to a `Controller` over a library
//! on disk, driven by keys and Slint's accessibility actions. Time is mocked, never the real
//! clock. One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, platform::Key};
use snipdeck_app::{
    controller::{Controller, Effect, Services},
    demo,
    host::{FakeTargets, FixedClock},
    settings::Settings,
    unlock_binding::UnlockBinding,
};
use snipdeck_core::{Entitlements, Tier, Timestamp};
use snipdeck_platform::testing::{FakeClipboard, FakeKeyboard, FakeTiming};
use snipdeck_store::{
    Store,
    crypto::{KdfParams, MemoryKeyProvider},
};
use snipdeck_ui::UnlockWindow;

#[path = "support/mod.rs"]
mod support;
use support::{Deferred, key, on_ui_thread, press, type_text};

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

fn password() -> &'static str {
    support::pw("main")
}

struct Rig {
    ui: UnlockWindow,
    ctl: Rc<RefCell<Ctl>>,
    binding: UnlockBinding<FakeClipboard, FakeKeyboard, FakeTiming>,
    _dir: tempfile::TempDir,
}

/// A library with a master password that is locked, the prompt asked for and shown.
fn rig() -> Rig {
    let dir = tempfile::tempdir().expect("dir");
    let now: Timestamp = "2026-10-17T10:30:00Z".parse().expect("timestamp");
    let mut store = Store::open(dir.path()).expect("store");
    demo::seed(&mut store, now).expect("seed");
    let mut ctl = Controller::new(
        store,
        Services {
            clipboard: FakeClipboard::default(),
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        },
        Box::new(FakeTargets::new(Some(demo::target()))),
        Box::new(FixedClock::new(now)),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    ctl.set_key_provider(Box::new(MemoryKeyProvider::new().expect("provider")));
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("params"));
    ctl.set_master_password(password(), password())
        .expect("set");
    ctl.lock_now();
    ctl.request_unlock();
    assert!(ctl.library_locked() && ctl.unlock_prompt_open());
    assert_eq!(ctl.take_pending(), vec![Effect::OpenUnlock]);
    let ctl = Rc::new(RefCell::new(ctl));
    let ui = UnlockWindow::new().expect("unlock window");
    let binding = Controller::bind_unlock(&ctl, &ui);
    binding.prepare();
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    Rig {
        ui,
        ctl,
        binding,
        _dir: dir,
    }
}

fn element(ui: &UnlockWindow, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

fn has_text(ui: &UnlockWindow, text: &str) -> bool {
    ElementHandle::find_by_accessible_label(ui, text)
        .any(|e| e.accessible_role() == Some(AccessibleRole::Text))
}

fn settle() {
    slint::platform::update_timers_and_animations();
}

#[test]
fn typing_the_password_and_pressing_enter_unlocks_with_the_focus_already_in_the_field() {
    on_ui_thread(|| {
        let r = rig();
        // No click: the field has the focus when the window opens (WCAG 2.4.3).
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        assert!(!r.ctl.borrow().unlock_prompt_open());
        assert!(!r.ui.window().is_visible(), "the window closes on success");
        assert_eq!(r.ui.get_password(), "", "nothing is kept in the window");
    });
}

#[test]
fn a_wrong_password_says_so_in_words_empties_the_field_and_allows_another_try() {
    on_ui_thread(|| {
        let r = rig();
        type_text(r.ui.window(), support::pw("wrong"));
        press(r.ui.window(), "\n");
        settle();
        assert!(r.ctl.borrow().library_locked());
        assert!(r.ui.window().is_visible(), "the window stays for a retry");
        assert!(
            has_text(&r.ui, "That password is not right. Try again."),
            "the error is text with an icon, announced as it appears"
        );
        assert_eq!(r.ui.get_password(), "", "the wrong password is not kept");
        // The focus is back in the field: type the right one and Enter again.
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        assert!(!r.ui.window().is_visible());
    });
}

#[test]
fn an_empty_password_is_told_apart_from_a_wrong_one() {
    on_ui_thread(|| {
        let r = rig();
        element(&r.ui, "Unlock", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(has_text(&r.ui, "Enter your password."));
        assert!(r.ctl.borrow().library_locked());
    });
}

#[test]
fn escape_and_cancel_leave_the_library_locked_and_close_the_prompt() {
    on_ui_thread(|| {
        let r = rig();
        type_text(r.ui.window(), "half typed");
        press(r.ui.window(), &key(Key::Escape));
        settle();
        assert!(!r.ui.window().is_visible());
        assert_eq!(r.ui.get_password(), "", "what was typed is dropped");
        assert!(r.ctl.borrow().library_locked());
        assert!(!r.ctl.borrow().unlock_prompt_open());

        let r = rig();
        element(&r.ui, "Cancel", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(!r.ui.window().is_visible());
        assert!(r.ctl.borrow().library_locked());
        assert!(!r.ctl.borrow().unlock_prompt_open());
    });
}

#[test]
fn the_field_is_named_and_the_typed_password_is_not_exposed() {
    on_ui_thread(|| {
        let r = rig();
        let field = element(&r.ui, "Master password", AccessibleRole::TextInput);
        field.set_accessible_value(password());
        assert!(
            field.accessible_value().is_none_or(|v| v.is_empty()),
            "a screen reader does not read the password back"
        );
        // The window's group is labelled for the screen reader.
        element(&r.ui, "Unlock Snipdeck", AccessibleRole::Groupbox);
    });
}

#[test]
fn the_prompt_closes_by_itself_when_something_else_unlocked_the_library() {
    on_ui_thread(|| {
        let r = rig();
        r.ctl
            .borrow_mut()
            .unlock_with_password(password())
            .expect("unlocked elsewhere");
        // Every window redraws after a change made in another; nothing here does, so ask.
        r.binding.redraw();
        settle();
        assert!(!r.ui.window().is_visible());
    });
}

#[test]
fn while_the_key_is_derived_the_window_says_so_and_takes_no_second_try() {
    on_ui_thread(|| {
        let r = rig();
        let held = Deferred::default();
        let _inline_again = held.install();
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert_eq!(held.held(), 1, "the work was handed off");
        assert!(r.ui.get_busy());
        assert!(r.ctl.borrow().library_locked(), "nothing is open yet");
        assert!(r.ui.window().is_visible());
        assert_eq!(r.ui.get_password(), "", "the field is emptied at once");

        // The status is a polite live region, and the buttons wait.
        let status = element(&r.ui, "Unlocking\u{2026}", AccessibleRole::Text);
        assert_eq!(
            status.accessible_live_region(),
            Some(i_slint_backend_testing::AccessibleLiveness::Polite)
        );
        assert_eq!(
            element(&r.ui, "Unlock", AccessibleRole::Button).accessible_enabled(),
            Some(false)
        );
        assert_eq!(
            element(&r.ui, "Cancel", AccessibleRole::Button).accessible_enabled(),
            Some(false)
        );
        // Enter and Esc do nothing meanwhile.
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        press(r.ui.window(), &key(Key::Escape));
        settle();
        assert_eq!(held.held(), 1, "one step at a time");
        assert!(r.ctl.borrow().unlock_prompt_open());

        held.release();
        settle();
        assert!(!r.ui.get_busy());
        assert!(!r.ctl.borrow().library_locked());
        assert!(!r.ui.window().is_visible(), "closed once it is open");
    });
}

#[test]
fn a_wrong_password_found_in_the_background_frees_the_window_for_another_try() {
    on_ui_thread(|| {
        let r = rig();
        let held = Deferred::default();
        let _inline_again = held.install();
        type_text(r.ui.window(), support::pw("wrong"));
        press(r.ui.window(), "\n");
        settle();
        held.release();
        settle();
        assert!(!r.ui.get_busy());
        assert!(r.ui.window().is_visible());
        assert!(has_text(&r.ui, "That password is not right. Try again."));
        assert_eq!(
            element(&r.ui, "Unlock", AccessibleRole::Button).accessible_enabled(),
            Some(true)
        );
    });
}

/// The real worker-thread offload, then the event loop stops once the outcome has been applied.
struct ThenQuit(Rc<Cell<bool>>);

impl snipdeck_app::master_job::Offload for ThenQuit {
    fn run(
        &self,
        job: snipdeck_app::master_job::MasterJob,
        finish: snipdeck_app::master_job::Finish,
    ) {
        let applied = Rc::clone(&self.0);
        snipdeck_app::master_job::Threads.run(
            job,
            Box::new(move |done| {
                finish(done);
                applied.set(true);
                slint::quit_event_loop().expect("the loop is running");
            }),
        );
    }
}

#[test]
fn a_worker_thread_hands_the_outcome_back_through_the_event_loop() {
    on_ui_thread(|| {
        let r = rig();
        let applied = Rc::new(Cell::new(false));
        snipdeck_app::master_job::set_offload(Rc::new(ThenQuit(Rc::clone(&applied))));
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        assert!(r.ui.get_busy(), "the work left the UI thread");
        // Earlier tests on this thread hid their last window outside a running loop, and Slint
        // queued a quit for each; those end a run early, so run again until the answer is in.
        for _ in 0..64 {
            slint::run_event_loop().expect("event loop");
            if applied.get() {
                break;
            }
        }
        snipdeck_app::master_job::set_offload(Rc::new(snipdeck_app::master_job::Inline));
        assert!(!r.ui.get_busy(), "the worker's answer arrived");
        assert!(!r.ctl.borrow().library_locked());
        assert!(!r.ui.window().is_visible());
    });
}
