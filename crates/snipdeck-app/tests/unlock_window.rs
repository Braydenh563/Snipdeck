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
use snipdeck_platform::{
    hello::{HelloAvailability, HelloError},
    testing::{FakeClipboard, FakeHello, FakeKeyboard, FakeTiming},
};
use snipdeck_store::{
    Store,
    crypto::{KdfParams, MemoryKeyProvider},
};
use snipdeck_ui::UnlockWindow;

#[path = "support/mod.rs"]
mod support;
use support::{Deferred, MasterSteps as _, key, on_ui_thread, press, type_text};

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
    rig_with(None)
}

/// Like [`rig`], with Windows Hello on and a sealed copy when `hello` is given.
fn rig_with(hello: Option<&std::sync::Arc<FakeHello>>) -> Rig {
    rig_with_clock(hello).0
}

/// Like [`rig_with`], and the clock the controller reads (to move time on).
fn rig_with_clock(hello: Option<&std::sync::Arc<FakeHello>>) -> (Rig, FixedClock) {
    let dir = tempfile::tempdir().expect("dir");
    let now: Timestamp = "2026-10-17T10:30:00Z".parse().expect("timestamp");
    let clock = FixedClock::new(now);
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
        Box::new(clock.clone()),
        Settings::default(),
        Entitlements::new(Tier::Pro),
    )
    .expect("controller");
    ctl.set_key_provider(Box::new(MemoryKeyProvider::new().expect("provider")));
    ctl.set_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).expect("params"));
    ctl.set_master_password(password(), password())
        .expect("set");
    if let Some(hello) = hello {
        ctl.set_hello(hello.clone());
        let job = ctl.begin_hello_refresh().expect("a Hello");
        ctl.finish_hello(job.run()).expect("answered");
        let job = ctl.set_hello_unlock(true).expect("sealing step");
        ctl.finish_hello(job.run()).expect("sealed");
    }
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
    (
        Rig {
            ui,
            ctl,
            binding,
            _dir: dir,
        },
        clock,
    )
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

fn hello_button(ui: &UnlockWindow) -> Option<ElementHandle> {
    ElementHandle::find_by_accessible_label(ui, "Unlock with Windows Hello")
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
}

#[test]
fn hello_button_shows_only_when_ready() {
    on_ui_thread(|| {
        // No Hello on this PC, or not turned on: only the password.
        let r = rig();
        assert!(hello_button(&r.ui).is_none());

        // Ready with a sealed copy: the button is there, and the password field stays.
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        assert!(hello_button(&r.ui).is_some());
        element(&r.ui, "Master password", AccessibleRole::TextInput);

        // Hello no longer ready (no PIN): the option disappears.
        hello.set_availability(HelloAvailability::NotSetUp);
        let job = r.ctl.borrow_mut().begin_hello_refresh().expect("job");
        r.ctl
            .borrow_mut()
            .finish_hello(job.run())
            .expect("answered");
        r.binding.prepare();
        settle();
        assert!(hello_button(&r.ui).is_none());
    });
}

#[test]
fn the_hello_button_unlocks_and_a_cancelled_hello_leaves_the_password() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        settle();
        assert!(!r.ctl.borrow().library_locked());
        assert!(!r.ui.window().is_visible(), "the window closes on success");

        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        hello.set_sign(Err(HelloError::Cancelled));
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        settle();
        assert!(r.ctl.borrow().library_locked());
        assert!(r.ui.window().is_visible(), "the prompt stays");
        assert!(has_text(
            &r.ui,
            "Windows Hello was cancelled. Use your master password."
        ));
        assert!(
            hello_button(&r.ui).is_some(),
            "a cancel keeps the button, so there is a retry"
        );
        hello.set_sign(Ok(()));
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        settle();
        assert!(!r.ctl.borrow().library_locked(), "the retry unlocked");

        // A Hello that failed for real: the copy is gone and so is the button.
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        hello.set_sign(Err(HelloError::Removed));
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        settle();
        assert!(has_text(
            &r.ui,
            "Windows Hello did not unlock Snipdeck. Use your master password."
        ));
        assert!(hello_button(&r.ui).is_none(), "the copy is gone");
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked(), "the password still works");
    });
}

/// Shows the unlock prompt again after a lock.
fn reopen(r: &Rig) {
    {
        let mut ctl = r.ctl.borrow_mut();
        ctl.request_unlock();
        let _ = ctl.take_pending();
    }
    r.binding.prepare();
    r.ui.show().expect("show");
    settle();
}

#[test]
fn a_password_unlock_seals_again_and_a_cancelled_seal_keeps_hello_on() {
    on_ui_thread(|| {
        // The password unlocks and Hello is asked once to seal: the next lock offers the button.
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        let before = hello.prompts();
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        assert_eq!(hello.prompts(), before + 1, "one prompt to seal");
        r.ctl.borrow_mut().lock_now();
        assert!(r.ctl.borrow().hello_unlock_available());

        // The prompt is cancelled at the seal: the password unlock stands, there is no copy, and
        // the option stays on, so the next password unlock asks again.
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        hello.set_sign(Err(HelloError::Cancelled));
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        assert!(
            r.ctl.borrow().settings().hello_unlock,
            "a cancel is not a no"
        );
        r.ctl.borrow_mut().lock_now();
        assert!(!r.ctl.borrow().hello_unlock_available(), "no copy");
        hello.set_sign(Ok(()));
        reopen(&r);
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        r.ctl.borrow_mut().lock_now();
        assert!(
            r.ctl.borrow().hello_unlock_available(),
            "sealed on the retry"
        );
    });
}

/// A Hello that has not answered yet when the password unlocks (the app asks at start-up).
fn rig_unanswered() -> (Rig, std::sync::Arc<FakeHello>) {
    let hello = std::sync::Arc::new(FakeHello::new());
    let r = rig_with(Some(&hello));
    r.ctl.borrow_mut().set_hello(hello.clone());
    (r, hello)
}

#[test]
fn a_late_availability_answer_seals_the_open_key_once() {
    on_ui_thread(|| {
        let (r, hello) = rig_unanswered();
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        r.ctl.borrow_mut().lock_now();
        assert!(
            !r.ctl.borrow().hello_unlock_available(),
            "nothing was sealed yet"
        );
        r.ctl
            .borrow_mut()
            .unlock_with_password(password())
            .expect("password");

        snipdeck_app::hello_job::hold_jobs();
        snipdeck_app::unlock_binding::refresh_hello(&r.ctl);
        assert_eq!(snipdeck_app::hello_job::held_jobs(), 1, "the question");
        let before = hello.prompts();
        assert!(snipdeck_app::hello_job::finish_held(0), "a job waits");
        assert_eq!(snipdeck_app::hello_job::held_jobs(), 1, "then the seal");
        assert!(snipdeck_app::hello_job::finish_held(0), "a job waits");
        snipdeck_app::hello_job::release_jobs();
        assert_eq!(hello.prompts(), before + 1);
        r.ctl.borrow_mut().lock_now();
        assert!(r.ctl.borrow().hello_unlock_available());
    });
}

#[test]
fn opening_settings_later_does_not_put_a_hello_prompt_on_screen() {
    on_ui_thread(|| {
        // Hello is on and answered, there is no copy (the seal was cancelled), the library is open.
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        hello.set_sign(Err(HelloError::Cancelled));
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        hello.set_sign(Ok(()));
        let before = hello.prompts();
        snipdeck_app::unlock_binding::refresh_hello(&r.ctl);
        assert_eq!(
            hello.prompts(),
            before,
            "a refresh only asks if Hello is ready"
        );
        assert!(r.ctl.borrow().settings().hello_unlock);
    });
}

#[test]
fn the_outcome_of_a_step_given_up_on_shows_no_hello_error() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        snipdeck_app::hello_job::hold_jobs();
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        assert_eq!(
            snipdeck_app::hello_job::held_jobs(),
            1,
            "Hello is being asked"
        );
        // The step is given up on while the prompt is up (the plan changed, the copy goes).
        r.ctl
            .borrow_mut()
            .change_plan(Entitlements::new(Tier::Free));
        assert!(snipdeck_app::hello_job::finish_held(0), "a job waits");
        snipdeck_app::hello_job::release_jobs();
        settle();
        assert!(r.ctl.borrow().library_locked());
        assert!(
            !has_text(
                &r.ui,
                "Windows Hello did not unlock Snipdeck. Use your master password."
            ),
            "a dropped step is not reported as a failure"
        );
        type_text(r.ui.window(), password());
        assert_eq!(r.ui.get_password(), password(), "the field has the focus");
    });
}

#[test]
fn after_a_restart_hello_asks_nothing_at_open_and_once_after_the_first_password() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        // A restart forgets the in-memory copy; the option is still on.
        r.ctl
            .borrow_mut()
            .change_plan(Entitlements::new(Tier::Free));
        r.ctl.borrow_mut().change_plan(Entitlements::new(Tier::Pro));
        assert!(r.ctl.borrow().settings().hello_unlock);
        let before = hello.prompts();
        r.binding.prepare();
        snipdeck_app::unlock_binding::refresh_hello(&r.ctl);
        settle();
        assert!(hello_button(&r.ui).is_none(), "no copy: only the password");
        assert_eq!(hello.prompts(), before, "no Hello prompt at open");
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        assert_eq!(hello.prompts(), before + 1, "one seal after the password");
        r.ctl.borrow_mut().lock_now();
        assert!(r.ctl.borrow().hello_unlock_available());
    });
}

#[test]
fn a_changed_hello_says_so_once_and_the_password_sets_it_up_again() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        hello.replace_credential(b"another credential");
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        settle();
        assert!(r.ctl.borrow().library_locked());
        assert!(has_text(
            &r.ui,
            "Your Windows Hello PIN, face or fingerprint changed. Unlock with your password once to set Windows Hello up again."
        ));
        assert!(hello_button(&r.ui).is_none());
        type_text(r.ui.window(), password());
        press(r.ui.window(), "\n");
        settle();
        assert!(!r.ctl.borrow().library_locked());
        r.ctl.borrow_mut().lock_now();
        assert!(r.ctl.borrow().hello_unlock_available(), "sealed again");
        assert!(r.ctl.borrow().settings().hello_unlock);
    });
}

#[test]
fn a_hello_turned_off_for_failing_says_why_in_the_unlock_window() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        r.ctl
            .borrow_mut()
            .change_plan(Entitlements::new(Tier::Free));
        r.ctl.borrow_mut().change_plan(Entitlements::new(Tier::Pro));
        hello.set_sign(Err(HelloError::Removed));
        for _ in 0..2 {
            type_text(r.ui.window(), password());
            press(r.ui.window(), "\n");
            settle();
            r.ctl.borrow_mut().lock_now();
            reopen(&r);
        }
        assert!(!r.ctl.borrow().settings().hello_unlock);
        assert!(has_text(
            &r.ui,
            "Windows Hello didn't work, so it was turned off. You can turn it on again in Settings."
        ));
        assert!(hello_button(&r.ui).is_none());
    });
}

#[test]
fn at_225_percent_text_the_buttons_have_room_and_hello_has_its_own_row() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        r.ui.global::<snipdeck_ui::SystemLook<'_>>()
            .set_text_scale(2.25);
        settle();
        // The host sizes the window from what it wants (as `size_before_show` does).
        let scale = r.ui.window().scale_factor();
        r.ui.window().set_size(slint::LogicalSize::new(
            r.ui.get_wanted_width(),
            r.ui.get_wanted_height(),
        ));
        settle();
        let width =
            f32::from(u16::try_from(r.ui.window().size().width).unwrap_or(u16::MAX)) / scale;
        assert!(
            width >= 270.0 * 2.25,
            "the window grew with the text: {width}"
        );
        let hello_btn = hello_button(&r.ui).expect("hello");
        let cancel = element(&r.ui, "Cancel", AccessibleRole::Button);
        let unlock = element(&r.ui, "Unlock", AccessibleRole::Button);
        let (h, c, u) = (
            (hello_btn.absolute_position(), hello_btn.size()),
            (cancel.absolute_position(), cancel.size()),
            (unlock.absolute_position(), unlock.size()),
        );
        assert!(
            h.0.y + h.1.height <= c.0.y,
            "Hello sits on a row above Cancel and Unlock"
        );
        for (name, (pos, size)) in [("hello", h), ("cancel", c), ("unlock", u)] {
            assert!(pos.x >= 0.0, "{name} starts inside the window");
            assert!(
                pos.x + size.width <= width + 0.5,
                "{name} ends inside the window"
            );
        }
        assert!(
            h.1.width > c.1.width * 2.0,
            "Hello's long label has the whole row"
        );
        assert!(
            c.1.width >= 60.0 && u.1.width >= 60.0,
            "Cancel and Unlock are not squeezed"
        );
    });
}

#[test]
fn an_older_availability_answer_is_dropped_by_the_binding() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        snipdeck_app::hello_job::hold_jobs();
        snipdeck_app::unlock_binding::refresh_hello(&r.ctl);
        snipdeck_app::unlock_binding::refresh_hello(&r.ctl);
        assert_eq!(snipdeck_app::hello_job::held_jobs(), 2);
        // The newer question answers first (ready), then the older one answers late (no PIN).
        assert!(snipdeck_app::hello_job::finish_held(1), "a job waits");
        hello.set_availability(HelloAvailability::NotSetUp);
        assert!(snipdeck_app::hello_job::finish_held(0), "a job waits");
        snipdeck_app::hello_job::release_jobs();
        assert!(
            r.ctl.borrow().hello_offered(),
            "the late, older answer must not replace the newer one"
        );
    });
}

#[test]
fn when_the_hello_button_disappears_the_focus_goes_to_the_password_field() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        // Tab moves the focus to the Hello button.
        press(r.ui.window(), "\t");
        settle();
        type_text(r.ui.window(), "x");
        assert_eq!(r.ui.get_password(), "", "Tab moved the focus off the field");
        // The copy goes while the window is open (the plan changed): the next redraw takes the
        // button away and the focus goes back to the field.
        r.ctl
            .borrow_mut()
            .change_plan(Entitlements::new(Tier::Free));
        r.binding.redraw();
        settle();
        assert!(hello_button(&r.ui).is_none(), "the button is gone");
        type_text(r.ui.window(), password());
        assert_eq!(
            r.ui.get_password(),
            password(),
            "keys reach the password field"
        );
    });
}

#[test]
fn a_redraw_never_leaves_a_hello_button_for_a_copy_that_is_gone() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        assert!(hello_button(&r.ui).is_some());
        r.ctl
            .borrow_mut()
            .change_plan(Entitlements::new(Tier::Free));
        assert!(
            hello_button(&r.ui).is_some(),
            "nothing redrew yet, so the window still shows it"
        );
        r.binding.redraw();
        settle();
        assert!(hello_button(&r.ui).is_none(), "the redraw removes it");
        // Going back to a plan with the feature does not bring the dropped copy back: the button
        // stays away until a password unlock seals a new one.
        r.ctl.borrow_mut().change_plan(Entitlements::new(Tier::Pro));
        r.binding.redraw();
        settle();
        assert!(hello_button(&r.ui).is_none(), "no copy, so no button");
    });
}

#[test]
fn the_turned_off_line_shows_once_and_not_every_time_the_window_opens() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let r = rig_with(Some(&hello));
        r.ctl
            .borrow_mut()
            .change_plan(Entitlements::new(Tier::Free));
        r.ctl.borrow_mut().change_plan(Entitlements::new(Tier::Pro));
        hello.set_sign(Err(HelloError::Removed));
        for _ in 0..2 {
            type_text(r.ui.window(), password());
            press(r.ui.window(), "\n");
            settle();
            r.ctl.borrow_mut().lock_now();
            reopen(&r);
        }
        let line = "Windows Hello didn't work, so it was turned off. You can turn it on again in Settings.";
        assert!(has_text(&r.ui, line), "said when the window opens");
        r.ctl.borrow_mut().lock_now();
        reopen(&r);
        assert!(!has_text(&r.ui, line), "not said again");
    });
}

#[test]
fn pressing_a_hello_button_whose_copy_expired_says_so_and_does_not_call_hello() {
    on_ui_thread(|| {
        let hello = std::sync::Arc::new(FakeHello::new());
        let (r, clock) = rig_with_clock(Some(&hello));
        assert!(hello_button(&r.ui).is_some());
        let prompts = hello.prompts();
        // Eight days later, with nothing redrawn: the window still shows the button.
        let later: Timestamp = "2026-10-25T10:30:00Z".parse().expect("timestamp");
        clock.set(later);
        assert!(
            hello_button(&r.ui).is_some(),
            "stale until something redraws"
        );
        hello_button(&r.ui)
            .expect("button")
            .invoke_accessible_default_action();
        settle();
        assert_eq!(hello.prompts(), prompts, "Hello was not asked");
        assert!(has_text(
            &r.ui,
            "Windows Hello did not unlock Snipdeck. Use your master password."
        ));
        assert!(hello_button(&r.ui).is_none(), "the button is gone");
        assert!(r.ctl.borrow().library_locked());
    });
}
