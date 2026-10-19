//! The master password's half of the [`Controller`] (SPEC 5.7): setting, changing and removing it,
//! the unlock prompt, and the idle lock. Slint-free like the rest of the controller.
//!
//! The store keeps the data key in a record wrapped by the OS and, when a password is set, sealed
//! under it as well ([`snipdeck_store::crypto::KeyRing`]). This module decides when each of those
//! store calls is made and what the user is told; it never keeps a password. Passwords arrive as
//! `&str` from the window, are handed straight to the store and are not copied, logged or put in
//! an error. (The store takes `&str`, and the window's own text is a Slint string, so the plain
//! text cannot be wiped from this side.)
//!
//! * Setting a password is a Supporter feature ([`Feature::MasterPassword`]). Changing it,
//!   removing it and unlocking are always allowed, so a lapsed or missing licence never traps a
//!   library behind a password the user cannot manage.
//! * With a password set, the library starts locked and stays locked until the user types it. The
//!   prompt opens the first time a locked sensitive item is needed ([`Effect::OpenUnlock`]), not
//!   at start-up, so a tray app that starts with Windows does not greet the user with a dialog.
//! * The library locks again after [`IDLE_LOCK`] without use, by the controller's clock.

use std::{sync::Arc, time::Duration};

use snipdeck_core::{Feature, Timestamp};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::crypto::{CryptoError, KdfParams, KeyProvider, KeyRecord, KeyRing};

use super::{Controller, Effect, Library, Toast, ToastKind, ToastMessage};
use crate::{
    host::ClipboardText,
    master_job::{MasterDone, MasterJob, MasterOp},
    master_password::{self, IDLE_LOCK, NewProblem},
};

/// Whether a master password can be used on this machine, and whether one is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterState {
    /// The machine cannot protect a key (no DPAPI), so there is no key to put a password on.
    Unavailable,
    /// The key record could not be read (the key store failed), so nobody knows whether a
    /// password is set. Nothing is offered until it can be read.
    KeyUnavailable,
    /// None set.
    NotSet,
    /// Set: the library asks for it.
    Set,
}

/// Why a master password action did not happen. The window words each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterProblem {
    /// The current (or unlock) password was left empty.
    CurrentEmpty,
    /// The current (or unlock) password is not the one set.
    WrongPassword,
    /// The new password is shorter than [`master_password::MIN_LEN`].
    TooShort,
    /// The two typed copies of the new password differ.
    Mismatch,
    /// Free plan: setting a master password is a Supporter feature.
    SupporterOnly,
    /// No key protection on this machine.
    Unavailable,
    /// The store refused, or the action does not fit the state. Nothing changed.
    Failed,
}

impl From<NewProblem> for MasterProblem {
    fn from(problem: NewProblem) -> Self {
        match problem {
            NewProblem::TooShort => Self::TooShort,
            NewProblem::Mismatch => Self::Mismatch,
        }
    }
}

/// What the controller knows about the master password. No secret is in it.
#[derive(Debug)]
pub(super) struct LockState {
    /// The stored key needs a password (read from the key record, refreshed by every change).
    set: bool,
    /// The key record could not be read at the last look, so `set` is not known.
    unreadable: bool,
    /// The last time the user used the library; the idle lock counts from here.
    active_at: Timestamp,
    /// The unlock window is open.
    prompt: bool,
    /// A step whose key derivation is running elsewhere ([`MasterJob`]); no other starts.
    busy: Option<MasterOp>,
    /// Counts the times a running step was given up on (the library locked, or the unlock
    /// window closed): an outcome stamped with an older count is dropped.
    generation: u64,
    /// Cost of Argon2id for a new password; `None` is [`KdfParams::DEFAULT`]. Tests make it cheap.
    pub(super) kdf: Option<KdfParams>,
}

impl LockState {
    pub(super) fn new(now: Timestamp) -> Self {
        Self {
            set: false,
            unreadable: false,
            active_at: now,
            prompt: false,
            busy: None,
            generation: 0,
            kdf: None,
        }
    }
}

/// The key ring on `provider` with the cost settings in force.
pub(super) fn ring_on(provider: &dyn KeyProvider, kdf: Option<KdfParams>) -> KeyRing<'_> {
    let ring = KeyRing::new(provider);
    match kdf {
        Some(kdf) => ring.with_kdf_params(kdf),
        None => ring,
    }
}

fn problem_of_crypto(error: &CryptoError) -> MasterProblem {
    match error {
        CryptoError::WrongPassword => MasterProblem::WrongPassword,
        CryptoError::EmptyPassword => MasterProblem::CurrentEmpty,
        _ => MasterProblem::Failed,
    }
}

fn neutral(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Success,
        message,
    }
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Uses `kdf` for master passwords set from now on (tests pass a cheap one; the app keeps
    /// [`KdfParams::DEFAULT`]).
    pub fn set_kdf_params(&mut self, kdf: KdfParams) {
        self.lock.kdf = Some(kdf);
    }

    /// Reads from the key record whether a password is set. Called at start-up and after
    /// anything that can change the record.
    pub(super) fn refresh_master(&mut self) {
        let (set, unreadable) = match self.editor_env.keys.as_deref() {
            Some(provider) => {
                let ring = ring_on(provider, self.lock.kdf);
                match self.store.has_master_password(&ring) {
                    Ok(set) => (set, false),
                    Err(e) => {
                        crate::trace!("key record not read: {e}");
                        (false, true)
                    }
                }
            }
            None => (false, false),
        };
        self.lock.set = set;
        self.lock.unreadable = unreadable;
        if self.lock.unreadable {
            // Not "no password": the Settings row offers nothing, and the user is told.
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::KeyUnavailable,
            });
        }
    }

    /// Whether a master password is set, or cannot be.
    #[must_use]
    pub fn master_password_state(&self) -> MasterState {
        if self.editor_env.keys.is_none() {
            MasterState::Unavailable
        } else if self.lock.unreadable {
            MasterState::KeyUnavailable
        } else if self.lock.set {
            MasterState::Set
        } else {
            MasterState::NotSet
        }
    }

    /// Whether this plan may set a master password ([`Feature::MasterPassword`]).
    #[must_use]
    pub fn master_password_allowed(&self) -> bool {
        self.entitlements.allows(Feature::MasterPassword)
    }

    /// A master password is set and the key is not open.
    #[must_use]
    pub fn library_locked(&self) -> bool {
        self.lock.set && !self.store.is_unlocked()
    }

    /// Whether the unlock window should be open.
    #[must_use]
    pub fn unlock_prompt_open(&self) -> bool {
        self.lock.prompt
    }

    /// Marks the user as active now, which restarts the idle count.
    pub(super) fn note_activity(&mut self) {
        self.lock.active_at = self.clock.now();
    }

    /// For revealing a sensitive row: `false`, with the unlock window asked for, when the key is
    /// locked; otherwise counts it as use and says `true`.
    pub(super) fn reveal_allowed(&mut self) -> bool {
        if self.library_locked() {
            self.request_unlock();
            return false;
        }
        self.note_activity();
        true
    }

    /// For a copy of the whole library (backup, export, restore): `true` when the key exists and
    /// is not open, so the copy would leave out the sensitive items or lose them. The unlock
    /// window is asked for when a password is what is missing; the toast says why nothing was
    /// done. A copy never goes ahead on a locked library.
    pub(super) fn copy_blocked_by_lock(&mut self) -> bool {
        if !matches!(self.store.key_ring_exists(), Ok(true)) || self.store.is_unlocked() {
            return false;
        }
        if self.library_locked() {
            self.request_unlock();
            self.show_toast(neutral(ToastMessage::UnlockFirst));
        } else {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::KeyUnavailable,
            });
        }
        true
    }

    /// Asks for the unlock window if the library is locked behind a password. The host opens it
    /// ([`Effect::OpenUnlock`], through [`take_pending`](Self::take_pending)).
    pub fn request_unlock(&mut self) {
        if self.library_locked() {
            self.lock.prompt = true;
            self.pending.push(Effect::OpenUnlock);
        }
    }

    /// The user closed the unlock window without a password. The library stays locked.
    pub fn cancel_unlock(&mut self) {
        self.lock.prompt = false;
        // A password already being checked must not open the library after the user said no.
        if self.lock.busy == Some(MasterOp::Unlock) {
            self.invalidate_master();
        }
    }

    /// Opens the library with the master password typed in the unlock window. On success the
    /// window is done and the Panel shows the sensitive items again; a wrong password changes
    /// nothing and the window stays for another try. Waits for the key derivation: a window
    /// uses [`begin_unlock_with_password`](Self::begin_unlock_with_password) instead.
    ///
    /// # Errors
    ///
    /// [`MasterProblem::CurrentEmpty`], [`MasterProblem::WrongPassword`],
    /// [`MasterProblem::Unavailable`] or [`MasterProblem::Failed`].
    pub fn unlock_with_password(&mut self, password: &str) -> Result<(), MasterProblem> {
        let job = self.begin_unlock_with_password(password)?;
        self.finish_inline(job)
    }

    /// Sets a master password on a library that has none (a Supporter feature). Makes the key if
    /// there is none yet, which encrypts any sensitive item still stored as plain text. The
    /// library stays open for this session. Waits for the key derivation: a window uses
    /// [`begin_set_master_password`](Self::begin_set_master_password) instead.
    ///
    /// # Errors
    ///
    /// [`MasterProblem::SupporterOnly`] on Free, [`MasterProblem::Unavailable`] without key
    /// protection, [`MasterProblem::TooShort`], [`MasterProblem::Mismatch`], or
    /// [`MasterProblem::Failed`] (a password is already set, or the store refused).
    pub fn set_master_password(&mut self, new: &str, again: &str) -> Result<(), MasterProblem> {
        let job = self.begin_set_master_password(new, again)?;
        self.finish_inline(Some(job))
    }

    /// Changes the master password. `current` must be the one set. Waits for the key
    /// derivation: a window uses [`begin_change_master_password`](Self::begin_change_master_password)
    /// instead.
    ///
    /// # Errors
    ///
    /// [`MasterProblem::CurrentEmpty`], [`MasterProblem::WrongPassword`],
    /// [`MasterProblem::TooShort`], [`MasterProblem::Mismatch`], [`MasterProblem::Unavailable`]
    /// or [`MasterProblem::Failed`] (none is set, or the store refused). Nothing changes on error.
    pub fn change_master_password(
        &mut self,
        current: &str,
        new: &str,
        again: &str,
    ) -> Result<(), MasterProblem> {
        let job = self.begin_change_master_password(current, new, again)?;
        self.finish_inline(Some(job))
    }

    /// Removes the master password. `current` must be the one set. The key stays wrapped by the
    /// OS, as on the Free plan, and no item is re-encrypted. Waits for the key derivation: a
    /// window uses [`begin_remove_master_password`](Self::begin_remove_master_password) instead.
    ///
    /// # Errors
    ///
    /// As [`change_master_password`](Self::change_master_password), minus the new password's.
    pub fn remove_master_password(&mut self, current: &str) -> Result<(), MasterProblem> {
        let job = self.begin_remove_master_password(current)?;
        self.finish_inline(Some(job))
    }

    fn finish_inline(&mut self, job: Option<MasterJob>) -> Result<(), MasterProblem> {
        match job {
            Some(job) => self.finish_master(job.run()),
            None => Ok(()),
        }
    }

    // ---- Steps in three parts ------------------------------------------------------------------
    //
    // `begin_*` checks and gathers (fast), the job derives keys (slow, `MasterJob::run`, anywhere),
    // and `finish_master` applies the outcome (fast). Until `finish_master` or `abort_master`
    // the controller is busy and refuses another step, and nothing else about the password
    // changes.

    /// The step running elsewhere, if any. The windows show it and disable their buttons.
    #[must_use]
    pub fn master_busy(&self) -> Option<MasterOp> {
        self.lock.busy
    }

    /// Whether `done` is the outcome of the step that is running now. It is not when the library
    /// was locked, or the unlock window closed, while the key was being derived: the window then
    /// only stops waiting, and [`finish_master`](Self::finish_master) would drop it anyway.
    #[must_use]
    pub fn master_current(&self, done: &MasterDone) -> bool {
        self.lock.busy.is_some() && done.generation == self.lock.generation
    }

    /// Gives up on the running step: its outcome, when it comes, is dropped with its key.
    fn invalidate_master(&mut self) {
        self.lock.generation = self.lock.generation.wrapping_add(1);
        self.lock.busy = None;
    }

    /// Marks the step as running and stamps the job so a stale outcome can be told.
    fn start(&mut self, job: MasterJob) -> MasterJob {
        self.lock.busy = Some(job.op());
        job.stamped(self.lock.generation)
    }

    /// The first part of [`unlock_with_password`](Self::unlock_with_password). `Ok(None)`: the
    /// library is open already, so there is nothing to derive.
    ///
    /// # Errors
    ///
    /// As [`unlock_with_password`](Self::unlock_with_password), except a wrong password, which
    /// only [`finish_master`](Self::finish_master) can tell; [`MasterProblem::Failed`] while
    /// another step is running.
    pub fn begin_unlock_with_password(
        &mut self,
        password: &str,
    ) -> Result<Option<MasterJob>, MasterProblem> {
        self.check_idle()?;
        if self.store.is_unlocked() {
            self.lock.prompt = false;
            return Ok(None);
        }
        if password.is_empty() {
            return Err(MasterProblem::CurrentEmpty);
        }
        let provider = self.provider()?;
        let record = self.stored_record()?;
        let job = MasterJob::unlock(record, password, provider, self.lock.kdf);
        Ok(Some(self.start(job)))
    }

    /// The first part of [`set_master_password`](Self::set_master_password).
    ///
    /// # Errors
    ///
    /// As [`set_master_password`](Self::set_master_password); [`MasterProblem::Failed`] while
    /// another step is running.
    pub fn begin_set_master_password(
        &mut self,
        new: &str,
        again: &str,
    ) -> Result<MasterJob, MasterProblem> {
        self.check_idle()?;
        if !self.master_password_allowed() {
            return Err(MasterProblem::SupporterOnly);
        }
        let provider = self.provider()?;
        if self.lock.set || self.lock.unreadable {
            return Err(MasterProblem::Failed);
        }
        master_password::check_new(new, again).map_err(MasterProblem::from)?;
        let exists = self
            .store
            .key_ring_exists()
            .map_err(|_| MasterProblem::Failed)?;
        let job = if exists {
            // Opens the key when it is not open yet (no password is set, so this is quick), then
            // wraps it under the new password.
            let record = self.stored_record()?;
            MasterJob::rekey(
                MasterOp::Set { exists },
                record,
                None,
                Some(new),
                provider,
                self.lock.kdf,
            )
        } else {
            MasterJob::create(new, provider, self.lock.kdf)
        };
        Ok(self.start(job))
    }

    /// The first part of [`change_master_password`](Self::change_master_password).
    ///
    /// # Errors
    ///
    /// As [`change_master_password`](Self::change_master_password), except a wrong current
    /// password, which only [`finish_master`](Self::finish_master) can tell;
    /// [`MasterProblem::Failed`] while another step is running.
    pub fn begin_change_master_password(
        &mut self,
        current: &str,
        new: &str,
        again: &str,
    ) -> Result<MasterJob, MasterProblem> {
        self.check_idle()?;
        let provider = self.check_can_change(current)?;
        master_password::check_new(new, again).map_err(MasterProblem::from)?;
        let record = self.stored_record()?;
        let job = MasterJob::rekey(
            MasterOp::Change,
            record,
            Some(current),
            Some(new),
            provider,
            self.lock.kdf,
        );
        Ok(self.start(job))
    }

    /// The first part of [`remove_master_password`](Self::remove_master_password).
    ///
    /// # Errors
    ///
    /// As [`remove_master_password`](Self::remove_master_password), except a wrong current
    /// password, which only [`finish_master`](Self::finish_master) can tell;
    /// [`MasterProblem::Failed`] while another step is running.
    pub fn begin_remove_master_password(
        &mut self,
        current: &str,
    ) -> Result<MasterJob, MasterProblem> {
        self.check_idle()?;
        let provider = self.check_can_change(current)?;
        let record = self.stored_record()?;
        let job = MasterJob::rekey(
            MasterOp::Remove,
            record,
            Some(current),
            None,
            provider,
            self.lock.kdf,
        );
        Ok(self.start(job))
    }

    /// Applies the outcome of a [`MasterJob`]: stores the new record, opens the library, and tells
    /// the user. Frees the controller whatever the outcome.
    ///
    /// # Errors
    ///
    /// [`MasterProblem::WrongPassword`] for an unlock, change or removal with the wrong
    /// password; [`MasterProblem::Failed`] for anything else that went wrong. Nothing changed.
    pub fn finish_master(&mut self, done: MasterDone) -> Result<(), MasterProblem> {
        if !self.master_current(&done) {
            // Locked or cancelled meanwhile: nothing is applied and the key is wiped on drop.
            return Err(MasterProblem::Failed);
        }
        self.lock.busy = None;
        let op = done.op;
        let (key, record) = done.outcome.map_err(|e| match op {
            // A first password has no "current" to get wrong.
            MasterOp::Set { .. } => MasterProblem::Failed,
            _ => problem_of_crypto(&e),
        })?;
        match (op, record) {
            (MasterOp::Unlock, _) => {
                self.store
                    .open_with_key(key)
                    .map_err(|_| MasterProblem::Failed)?;
                self.lock.prompt = false;
                self.note_activity();
                self.show_toast(neutral(ToastMessage::Unlocked));
                let _ = self.refresh_panel();
                Ok(())
            }
            (MasterOp::Set { exists }, Some(record)) => {
                if exists {
                    self.store
                        .replace_key_record(&record)
                        .map_err(|_| MasterProblem::Failed)?;
                    // The password is set for good from here, whether or not the library
                    // opens now: a retry must ask for it, not offer to set it again.
                    self.lock.set = true;
                    self.store
                        .open_with_key(key)
                        .map_err(|_| MasterProblem::Failed)?;
                } else {
                    self.store
                        .install_key_ring(key, &record)
                        .map_err(|_| MasterProblem::Failed)?;
                    self.lock.set = true;
                }
                self.note_activity();
                self.show_toast(neutral(ToastMessage::MasterPasswordSet));
                let _ = self.refresh_panel();
                Ok(())
            }
            (MasterOp::Change, Some(record)) => {
                self.store
                    .replace_key_record(&record)
                    .map_err(|_| MasterProblem::Failed)?;
                // The password is changed for good from here. The user just proved the old one,
                // so the library opens if it was locked; if that fails the change still stands
                // and the toast says the library is locked.
                let reopened = self.open_after_change(key);
                self.note_activity();
                self.show_toast(neutral(if reopened {
                    ToastMessage::MasterPasswordChanged
                } else {
                    ToastMessage::MasterPasswordChangedNotOpen
                }));
                Ok(())
            }
            (MasterOp::Remove, Some(record)) => {
                self.store
                    .replace_key_record(&record)
                    .map_err(|_| MasterProblem::Failed)?;
                // The record is OS-only from here, so there is no password to ask for, whether
                // or not the key opens now.
                self.lock.set = false;
                self.lock.prompt = false;
                let reopened = self.open_after_change(key);
                self.note_activity();
                self.show_toast(neutral(if reopened {
                    ToastMessage::MasterPasswordRemoved
                } else {
                    ToastMessage::MasterPasswordRemovedNotOpen
                }));
                let _ = self.refresh_panel();
                Ok(())
            }
            (MasterOp::Set { .. } | MasterOp::Change | MasterOp::Remove, None) => {
                Err(MasterProblem::Failed)
            }
        }
    }

    fn check_idle(&self) -> Result<(), MasterProblem> {
        if self.lock.busy.is_some() {
            return Err(MasterProblem::Failed);
        }
        Ok(())
    }

    fn provider(&self) -> Result<Arc<dyn KeyProvider>, MasterProblem> {
        self.editor_env
            .keys
            .clone()
            .ok_or(MasterProblem::Unavailable)
    }

    fn stored_record(&self) -> Result<KeyRecord, MasterProblem> {
        match self.store.key_record() {
            Ok(Some(record)) => Ok(record),
            _ => Err(MasterProblem::Failed),
        }
    }

    /// The checks every change or removal starts with. Gives the key protection to use.
    fn check_can_change(&self, current: &str) -> Result<Arc<dyn KeyProvider>, MasterProblem> {
        let provider = self.provider()?;
        if !self.lock.set {
            return Err(MasterProblem::Failed);
        }
        if current.is_empty() {
            return Err(MasterProblem::CurrentEmpty);
        }
        Ok(provider)
    }

    /// After a change that proved the password, opens the key if it is still locked. `false` when
    /// the store would not.
    fn open_after_change(&mut self, key: snipdeck_store::crypto::DataKey) -> bool {
        if self.store.open_with_key(key).is_err() {
            return false;
        }
        self.lock.prompt = false;
        true
    }

    // ---- The idle lock -------------------------------------------------------------------------

    /// How long until the idle lock is due, while a password is set and the library is open. The
    /// host arms a timer for this and then calls [`lock_if_idle`](Self::lock_if_idle).
    #[must_use]
    pub fn idle_lock_wait(&self) -> Option<Duration> {
        if !self.lock.set || !self.store.is_unlocked() {
            return None;
        }
        let due = self.lock.active_at.checked_add(IDLE_LOCK).ok()?;
        Some(Duration::try_from(due.duration_since(self.clock.now())).unwrap_or_default())
    }

    /// Locks the library if it has been idle for [`IDLE_LOCK`]. Returns whether it locked.
    pub fn lock_if_idle(&mut self) -> bool {
        match self.idle_lock_wait() {
            Some(wait) if wait.is_zero() => {
                self.lock_now();
                true
            }
            _ => false,
        }
    }

    /// Locks the library now: the key and every decrypted payload held in memory are dropped, a
    /// revealed row is hidden again, and an editor of a sensitive item is closed. Does nothing without a master password (the OS-only key
    /// opens at start-up and has nothing to ask for).
    pub fn lock_now(&mut self) {
        if !self.lock.set {
            return;
        }
        // A change or removal still being derived must not reopen what is being locked.
        self.invalidate_master();
        self.store.lock_key_ring();
        self.revealed = None;
        self.panel_revealed = None;
        // An open editor of a sensitive item holds its text in the window: it closes, unsaved.
        if self.close_sensitive_item_editor() {
            self.show_toast(neutral(ToastMessage::LockedEditorClosed));
        }
        let _ = self.refresh_panel();
    }
}
