//! Windows Hello unlock, controller half (v1.1 slice 8, `docs/crypto.md` "Windows Hello").
//!
//! Hello protects an in-memory sealed copy of the data key ([`SealedKey`]). Nothing is written to
//! disk; a restart forgets the copy, so the password is asked after a restart and every 7 days.
//! The master password is always the fallback: a cancelled, failed or removed Hello, or an
//! expired copy, drops the copy and leaves the password prompt as it was. Key material is never
//! logged.

use std::{
    sync::{Arc, atomic::AtomicBool, atomic::Ordering},
    time::Duration,
};

use snipdeck_core::Timestamp;
use snipdeck_platform::{
    hello::{HelloAuthenticator, HelloAvailability},
    pipeline::{Clipboard, Keyboard, Timing},
};
use snipdeck_store::crypto::{KeyRecord, SealedKey};

use crate::toast::ToastMessage;

use super::{Controller, Library};
use crate::{
    hello_job::{HelloDone, HelloFailure, HelloJob, Product},
    host::ClipboardText,
};

/// What the controller knows about Hello. No secret is shown by `Debug`.
#[derive(Default)]
pub(super) struct HelloState {
    /// The user just turned Hello on: the next seal also checks the signature repeats.
    enrol: bool,
    /// Tells a running job to stop (the library was locked, or the step was given up on).
    cancel: Arc<AtomicBool>,
    hello: Option<Arc<dyn HelloAuthenticator>>,
    availability: Option<HelloAvailability>,
    sealed: Option<SealedKey>,
    /// The key record the copy was sealed for (it holds no secret). A copy only opens the library
    /// while the stored record is still this one: a new key (erased, forgotten) never opens with
    /// an old copy.
    bound: Option<KeyRecord>,
    /// When the password last unlocked the library (the 7 days count from here).
    password_at: Option<Timestamp>,
    busy: bool,
    generation: u64,
    /// Counts the availability questions: an older answer never replaces a newer one.
    availability_generation: u64,
    /// The next seal also checks the signature repeats (Hello changed under a sealed copy).
    recheck: bool,
    /// Seals that failed in a row after password unlocks (a cancel does not count).
    seal_failures: u8,
    /// The step under way.
    step: Step,
    /// What the unlock window should say about Hello, if anything.
    note: Note,
}

/// A line about Hello for the unlock window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Note {
    #[default]
    None,
    /// An unlock with Hello found it changed; said once.
    Changed,
    /// Hello was turned off because it kept failing.
    TurnedOff,
}

/// What a Hello step does.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Step {
    #[default]
    Idle,
    Seal,
    Open,
}

/// Seals that may fail in a row before Hello is turned off.
const SEAL_FAILURES_ALLOWED: u8 = 2;

impl core::fmt::Debug for HelloState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HelloState")
            .field("sealed", &self.sealed.is_some())
            .field("busy", &self.busy)
            .finish_non_exhaustive()
    }
}

impl HelloState {
    /// Gives up on a step under way: its job is told to stop (it drops its key without asking
    /// Hello again) and its outcome, when it comes, is refused.
    pub(super) fn cancel_step(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.cancel = Arc::default();
        self.generation = self.generation.wrapping_add(1);
        self.busy = false;
    }

    /// Forgets the copy and gives up on a step under way.
    pub(super) fn drop_copy(&mut self) {
        self.sealed = None;
        self.bound = None;
        self.cancel_step();
    }

    fn token(&self) -> (u64, Arc<AtomicBool>) {
        (self.generation, Arc::clone(&self.cancel))
    }
}

fn since_epoch(at: Timestamp) -> Duration {
    Duration::from_secs(u64::try_from(at.as_second()).unwrap_or(0))
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Uses `hello` for the Windows Hello unlock. Nothing is offered until
    /// [`begin_hello_refresh`](Self::begin_hello_refresh) has been run and applied: asking Hello
    /// whether it is ready can block, so it is a job like the others.
    pub fn set_hello(&mut self, hello: Arc<dyn HelloAuthenticator>) {
        self.lock.hello.hello = Some(hello);
        self.lock.hello.availability = None;
        self.lock.hello.availability_generation =
            self.lock.hello.availability_generation.wrapping_add(1);
    }

    /// Whether Hello has answered the availability question since it was attached.
    #[must_use]
    pub fn hello_availability_known(&self) -> bool {
        self.lock.hello.availability.is_some()
    }

    /// The first part of asking Hello whether it can be used (the app does it at start-up and
    /// when Settings or the unlock window opens, so a PIN added or removed meanwhile shows).
    pub fn begin_hello_refresh(&mut self) -> Option<HelloJob> {
        let hello = Arc::clone(self.lock.hello.hello.as_ref()?);
        let state = &mut self.lock.hello;
        state.availability_generation = state.availability_generation.wrapping_add(1);
        Some(HelloJob::availability(hello, state.availability_generation))
    }

    /// The plan changed (a licence was applied): a copy for a plan without the feature is dropped.
    pub fn change_plan(&mut self, entitlements: snipdeck_core::Entitlements) {
        self.entitlements = entitlements;
        if !self.master_password_allowed() {
            self.give_up_hello_step(true);
            self.lock.hello.drop_copy();
        }
    }

    /// Whether Settings may offer "Unlock with Windows Hello": a Supporter with a master password
    /// set on a PC where Hello is ready.
    #[must_use]
    pub fn hello_offered(&self) -> bool {
        self.lock.set
            && self.master_password_allowed()
            && self.lock.hello.availability == Some(HelloAvailability::Ready)
    }

    /// Hello is offered and the user turned it on.
    #[must_use]
    pub fn hello_enabled(&self) -> bool {
        self.settings.hello_unlock && self.hello_offered()
    }

    /// Whether the unlock window shows its Hello button: Hello is on, the library is locked, and a
    /// sealed copy that has not expired exists.
    #[must_use]
    pub fn hello_unlock_available(&self) -> bool {
        self.hello_button_shown() && !self.lock.hello.busy
    }

    /// Whether the unlock window shows its Hello button (it stays, disabled, while a step runs).
    #[must_use]
    pub fn hello_button_shown(&self) -> bool {
        self.hello_enabled()
            && self.library_locked()
            && self
                .lock
                .hello
                .sealed
                .as_ref()
                .is_some_and(|s| !s.is_expired(since_epoch(self.clock.now())))
    }

    /// Turns the option on or off. On: the first Hello prompt seals a copy of the open key (the
    /// returned job; run it, then [`finish_hello`](Self::finish_hello)). Off: the copy is dropped.
    pub fn set_hello_unlock(&mut self, on: bool) -> Option<HelloJob> {
        self.lock.hello.note = Note::None;
        self.lock.hello.seal_failures = 0;
        if !on || !self.hello_offered() {
            self.update_settings(|s| s.hello_unlock = false);
            self.lock.hello.drop_copy();
            self.lock.hello.enrol = false;
            self.lock.hello.recheck = false;
            return None;
        }
        self.update_settings(|s| s.hello_unlock = true);
        self.lock.hello.enrol = true;
        let job = self.begin_hello_seal();
        if job.is_none() && self.lock.hello.sealed.is_none() {
            // Nothing could start (the library is locked: there is no open key to seal): the switch
            // goes back off and says so, instead of staying on without a copy.
            self.update_settings(|s| s.hello_unlock = false);
            self.lock.hello.enrol = false;
            self.show_toast(super::neutral(ToastMessage::HelloNotTurnedOn));
        }
        job
    }

    /// Forgets everything about Hello: the copy, a step under way, and the option. Used when the
    /// key it was sealed for goes (the library was erased, the password removed or set anew).
    pub(in crate::controller) fn forget_hello(&mut self) {
        self.lock.hello.drop_copy();
        self.lock.hello.enrol = false;
        self.lock.hello.recheck = false;
        self.update_settings(|s| s.hello_unlock = false);
    }

    /// Gives up on a Hello step under way (the library was locked, the unlock window closed). A
    /// switch-on that was cut short goes back off, so the switch never says "on" without a copy.
    pub(super) fn give_up_hello_step(&mut self, show: bool) {
        let was_switching_on = self.lock.hello.busy && self.lock.hello.enrol;
        if was_switching_on {
            self.update_settings(|s| s.hello_unlock = false);
            self.lock.hello.enrol = false;
            self.lock.hello.recheck = false;
        }
        self.lock.hello.cancel_step();
        if was_switching_on && show {
            self.show_toast(super::neutral(ToastMessage::HelloNotTurnedOn));
        }
    }

    /// The unlock window closed: a Hello unlock under way is given up on (its outcome is refused
    /// and the copy stays).
    pub(super) fn give_up_hello_unlock(&mut self) {
        if self.lock.hello.busy && self.lock.hello.step == Step::Open {
            self.lock.hello.cancel_step();
        }
    }

    /// The first part of sealing the open key under Hello, if Hello is on and the library is open.
    pub fn begin_hello_seal(&mut self) -> Option<HelloJob> {
        if !self.hello_enabled() || self.lock.hello.busy || self.lock.hello.sealed.is_some() {
            return None;
        }
        let key = self.store.duplicate_data_key()?;
        let hello = Arc::clone(self.lock.hello.hello.as_ref()?);
        let at = self
            .lock
            .hello
            .password_at
            .unwrap_or_else(|| self.clock.now());
        self.lock.hello.busy = true;
        self.lock.hello.step = Step::Seal;
        Some(HelloJob::seal(
            hello,
            key,
            self.lock.hello.enrol || self.lock.hello.recheck,
            since_epoch(at),
            self.lock.hello.token(),
        ))
    }

    /// The first part of unlocking with Hello. `None`: not available (the password prompt stays).
    /// An expired copy is dropped here and Hello is not asked.
    pub fn begin_hello_unlock(&mut self) -> Option<HelloJob> {
        if !self.hello_enabled() || !self.library_locked() || self.lock.busy.is_some() {
            return None;
        }
        let now = since_epoch(self.clock.now());
        let state = &mut self.lock.hello;
        if state.busy {
            return None;
        }
        let sealed = state.sealed.clone()?;
        if sealed.is_expired(now) {
            return None;
        }
        let hello = Arc::clone(state.hello.as_ref()?);
        state.busy = true;
        state.step = Step::Open;
        Some(HelloJob::open(hello, sealed, now, state.token()))
    }

    /// Whether `done` is the outcome of a step that was given up on (the library locked, the option
    /// switched off, a newer step started) or of an older availability question: it is to be
    /// dropped, not applied and not reported as a failure.
    #[must_use]
    pub fn hello_stale(&self, done: &HelloDone) -> bool {
        if done.is_availability() {
            done.generation != self.lock.hello.availability_generation
        } else {
            !self.hello_current(done)
        }
    }

    /// Whether `done` belongs to the step under way.
    #[must_use]
    pub fn hello_current(&self, done: &HelloDone) -> bool {
        self.lock.hello.busy && done.generation == self.lock.hello.generation
    }

    /// Applies a Hello outcome. A sealed copy is kept; an opened key opens the library.
    ///
    /// # Errors
    ///
    /// [`HelloFailure`] when Hello gave nothing: the copy is dropped and the password remains.
    pub fn finish_hello(&mut self, done: HelloDone) -> Result<(), HelloFailure> {
        if done.is_availability() {
            if done.generation != self.lock.hello.availability_generation {
                return Ok(());
            }
            // A question that could not be asked (its job panicked) counts as "not usable".
            let availability = match done.outcome {
                Ok(Product::Availability(availability)) => availability,
                _ => HelloAvailability::Unsupported,
            };
            self.lock.hello.availability = Some(availability);
            if availability != HelloAvailability::Ready {
                self.give_up_hello_step(true);
                self.lock.hello.drop_copy();
            }
            return Ok(());
        }
        if !self.hello_current(&done) {
            return Err(HelloFailure::Failed);
        }
        self.lock.hello.busy = false;
        match done.outcome {
            Ok(Product::Sealed(sealed)) => {
                let bound = self.store.key_record().ok().flatten();
                let state = &mut self.lock.hello;
                state.sealed = Some(sealed);
                state.bound = bound;
                state.enrol = false;
                state.recheck = false;
                state.seal_failures = 0;
                if state.note == Note::Changed {
                    state.note = Note::None;
                }
                Ok(())
            }
            Ok(Product::Availability(_)) => Ok(()),
            Ok(Product::Opened(key)) => {
                // The copy opens only the key it was sealed for: if the stored record changed
                // since (the library was erased, a new key made), the copy is stale and the
                // password is asked.
                let current = self.store.key_record().ok().flatten();
                if current.is_none() || current != self.lock.hello.bound {
                    self.turn_hello_off();
                    return Err(HelloFailure::Failed);
                }
                if self.store.open_with_key(key).is_err() {
                    self.lock.hello.drop_copy();
                    return Err(HelloFailure::Failed);
                }
                self.complete_unlock();
                Ok(())
            }
            Err(failure) => {
                // The copy goes. The option stays unless the failure is final (below): a Hello
                // that changed (a PIN reset makes a new credential) is sealed again at the next
                // password unlock, with the repeat check, which asks it once.
                // A cancel at the unlock keeps the copy: the button offers another try.
                if !(self.lock.hello.step == Step::Open && failure == HelloFailure::Cancelled) {
                    self.lock.hello.sealed = None;
                    self.lock.hello.bound = None;
                }
                match (self.lock.hello.step, failure) {
                    (Step::Open, HelloFailure::Mismatch) => {
                        self.lock.hello.note = Note::Changed;
                        self.lock.hello.recheck = true;
                    }
                    (Step::Seal, failure) => self.seal_failed(failure),
                    _ => {}
                }
                Err(failure)
            }
        }
    }

    /// A seal failed. The user's own switch-on that did not finish goes back off (with a message
    /// unless they cancelled); a seal after a password unlock turns Hello off for a Hello that
    /// cannot repeat, or after two failures in a row. A cancel never counts.
    fn seal_failed(&mut self, failure: HelloFailure) {
        let quiet = matches!(failure, HelloFailure::Cancelled | HelloFailure::Expired);
        if self.lock.hello.enrol {
            // It was never on: the switch goes back off and the toast says so (a cancel or
            // an expiry as "not turned on", a failure with its reason).
            self.update_settings(|s| s.hello_unlock = false);
            let state = &mut self.lock.hello;
            state.enrol = false;
            state.recheck = false;
            state.sealed = None;
            state.bound = None;
            self.show_toast(super::neutral(if quiet {
                ToastMessage::HelloNotTurnedOn
            } else {
                ToastMessage::HelloEnrolFailed
            }));
            return;
        }
        if quiet {
            return;
        }
        let state = &mut self.lock.hello;
        state.seal_failures = state.seal_failures.saturating_add(1);
        if failure == HelloFailure::NotRepeatable || state.seal_failures >= SEAL_FAILURES_ALLOWED {
            self.turn_hello_off();
        }
    }

    /// Hello is turned off because it does not work here: the option goes, and a toast and the
    /// unlock window say why. The password is untouched.
    fn turn_hello_off(&mut self) {
        self.update_settings(|s| s.hello_unlock = false);
        let state = &mut self.lock.hello;
        state.enrol = false;
        state.recheck = false;
        state.seal_failures = 0;
        state.sealed = None;
        state.bound = None;
        state.note = Note::TurnedOff;
        self.show_toast(super::neutral(ToastMessage::HelloTurnedOff));
    }

    /// Hello was turned off because it kept failing (cleared when the user turns it on again).
    #[must_use]
    pub fn hello_turned_off_note(&self) -> bool {
        self.lock.hello.note == Note::TurnedOff
    }

    /// Takes the "Hello was turned off" note: true once per turn-off.
    pub fn take_hello_turned_off_note(&mut self) -> bool {
        let said = self.lock.hello.note == Note::TurnedOff;
        if said {
            self.lock.hello.note = Note::None;
        }
        said
    }

    /// An unlock with Hello found it changed and its note was not shown yet.
    #[must_use]
    pub fn hello_changed_note(&self) -> bool {
        self.lock.hello.note == Note::Changed
    }

    /// Takes the "Hello changed" note: true once per change.
    pub fn take_hello_changed_note(&mut self) -> bool {
        let said = self.lock.hello.note == Note::Changed;
        if said {
            self.lock.hello.note = Note::None;
        }
        said
    }

    /// The password just unlocked the library: the 7 days count from now, and an old copy goes.
    pub(super) fn note_password_unlock(&mut self) {
        // A Hello step under way belongs to the copy that goes now: its outcome must not meet the
        // new state (a Hello unlock would find no bound record and turn Hello off).
        self.give_up_hello_step(true);
        self.lock.hello.password_at = Some(self.clock.now());
        self.lock.hello.sealed = None;
        self.lock.hello.bound = None;
    }
}
