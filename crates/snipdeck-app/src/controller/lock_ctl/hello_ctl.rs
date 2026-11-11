//! Windows Hello unlock, controller half (v1.1 slice 8, `docs/crypto.md` "Windows Hello").
//!
//! Hello protects an in-memory sealed copy of the data key ([`SealedKey`]). Nothing is written to
//! disk; a restart forgets the copy, so the password is asked after a restart and every 7 days.
//! The master password is always the fallback: a cancelled, failed or removed Hello, or an
//! expired copy, drops the copy and leaves the password prompt as it was. Key material is never
//! logged.

use std::{sync::Arc, time::Duration};

use snipdeck_core::Timestamp;
use snipdeck_platform::{
    hello::{HelloAuthenticator, HelloAvailability},
    pipeline::{Clipboard, Keyboard, Timing},
};
use snipdeck_store::crypto::SealedKey;

use super::{Controller, Library};
use crate::{
    hello_job::{HelloDone, HelloFailure, HelloJob, Product},
    host::ClipboardText,
};

/// What the controller knows about Hello. No secret is shown by `Debug`.
#[derive(Default)]
pub(super) struct HelloState {
    hello: Option<Arc<dyn HelloAuthenticator>>,
    availability: Option<HelloAvailability>,
    sealed: Option<SealedKey>,
    /// When the password last unlocked the library (the 7 days count from here).
    password_at: Option<Timestamp>,
    busy: bool,
    generation: u64,
}

impl core::fmt::Debug for HelloState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HelloState")
            .field("sealed", &self.sealed.is_some())
            .field("busy", &self.busy)
            .finish_non_exhaustive()
    }
}

impl HelloState {
    /// Forgets the copy and gives up on a step under way.
    pub(super) fn drop_copy(&mut self) {
        self.sealed = None;
        self.generation = self.generation.wrapping_add(1);
        self.busy = false;
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
    /// Uses `hello` for the Windows Hello unlock. Without one the option is never offered.
    pub fn set_hello(&mut self, hello: Arc<dyn HelloAuthenticator>) {
        self.lock.hello.hello = Some(hello);
        self.refresh_hello();
    }

    /// Asks Hello again whether it can be used (Settings opened, a PIN was set up meanwhile).
    pub fn refresh_hello(&mut self) {
        let availability = self.lock.hello.hello.as_ref().map(|h| h.availability());
        self.lock.hello.availability = availability;
        if availability != Some(HelloAvailability::Ready) {
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
        let state = &self.lock.hello;
        self.hello_enabled()
            && self.library_locked()
            && !state.busy
            && state
                .sealed
                .as_ref()
                .is_some_and(|s| !s.is_expired(since_epoch(self.clock.now())))
    }

    /// Turns the option on or off. On: the first Hello prompt seals a copy of the open key (the
    /// returned job; run it, then [`finish_hello`](Self::finish_hello)). Off: the copy is dropped.
    pub fn set_hello_unlock(&mut self, on: bool) -> Option<HelloJob> {
        if !on || !self.hello_offered() {
            self.update_settings(|s| s.hello_unlock = false);
            self.lock.hello.drop_copy();
            return None;
        }
        self.update_settings(|s| s.hello_unlock = true);
        self.begin_hello_seal()
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
        Some(HelloJob::seal(
            hello,
            key,
            since_epoch(at),
            self.lock.hello.generation,
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
        let sealed = state.sealed.take()?;
        if sealed.is_expired(now) {
            return None;
        }
        let hello = Arc::clone(state.hello.as_ref()?);
        state.busy = true;
        Some(HelloJob::open(hello, sealed, now, state.generation))
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
        if !self.hello_current(&done) {
            return Err(HelloFailure::Failed);
        }
        self.lock.hello.busy = false;
        match done.outcome {
            Ok(Product::Sealed(sealed)) => {
                self.lock.hello.sealed = Some(sealed);
                Ok(())
            }
            Ok(Product::Opened(key, sealed)) => {
                if self.store.open_with_key(key).is_err() {
                    self.lock.hello.drop_copy();
                    return Err(HelloFailure::Failed);
                }
                self.lock.hello.sealed = Some(sealed);
                self.complete_unlock();
                Ok(())
            }
            Err(failure) => {
                self.lock.hello.sealed = None;
                Err(failure)
            }
        }
    }

    /// The password just unlocked the library: the 7 days count from now, and an old copy goes.
    pub(super) fn note_password_unlock(&mut self) {
        self.lock.hello.password_at = Some(self.clock.now());
        self.lock.hello.sealed = None;
    }
}
