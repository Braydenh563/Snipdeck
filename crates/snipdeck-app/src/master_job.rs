//! The slow half of a master-password step (SPEC 5.7). Argon2id takes a noticeable moment by
//! design, so a step is split in three: the controller checks and gathers what the work needs
//! (`Controller::begin_*`, fast, on the UI thread), [`MasterJob::run`] does the key derivation
//! (no controller, no store: it may run on any thread), and the controller applies the outcome
//! (`Controller::finish_master`, fast, back on the UI thread).
//!
//! Slint-free. A job owns copies of the passwords it was given, wiped when it is dropped; nothing
//! here logs or stores one. [`Offload`] says where `run` happens: [`Inline`] runs it on the
//! calling thread (the tests, and anything that does not mind waiting), [`Threads`] on a worker
//! with the outcome handed back to the Slint event loop.

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use snipdeck_store::crypto::{
    CryptoError, DataKey, KdfParams, KeyProvider, KeyRecord, KeyRing, Zeroizing,
};

/// Which step a job is, so [`Controller::finish_master`](crate::controller::Controller::finish_master)
/// knows what to apply and the windows know what to say while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterOp {
    /// Opening the library with the typed password.
    Unlock,
    /// Setting a first password. `exists` is whether the library already had a key record (a
    /// sensitive item was saved before), which is then re-wrapped instead of made.
    Set {
        /// A key record exists.
        exists: bool,
    },
    /// Replacing the password.
    Change,
    /// Taking the password off.
    Remove,
}

/// What a job derives.
enum Work {
    Unlock {
        record: KeyRecord,
        password: Zeroizing<String>,
    },
    Rekey {
        record: KeyRecord,
        current: Option<Zeroizing<String>>,
        new: Option<Zeroizing<String>>,
    },
    Create {
        password: Zeroizing<String>,
    },
}

/// The slow half of one step. `Send`: hand it to a worker thread and its [`MasterDone`] back to
/// the controller.
pub struct MasterJob {
    op: MasterOp,
    work: Work,
    provider: Arc<dyn KeyProvider>,
    kdf: Option<KdfParams>,
}

impl core::fmt::Debug for MasterJob {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MasterJob")
            .field("op", &self.op)
            .finish_non_exhaustive()
    }
}

fn own(password: &str) -> Zeroizing<String> {
    Zeroizing::new(password.to_owned())
}

impl MasterJob {
    pub(crate) fn unlock(
        record: KeyRecord,
        password: &str,
        provider: Arc<dyn KeyProvider>,
        kdf: Option<KdfParams>,
    ) -> Self {
        Self {
            op: MasterOp::Unlock,
            work: Work::Unlock {
                record,
                password: own(password),
            },
            provider,
            kdf,
        }
    }

    /// Sets, changes or removes: re-wraps `record` (`current` opens it, `new` seals it).
    pub(crate) fn rekey(
        op: MasterOp,
        record: KeyRecord,
        current: Option<&str>,
        new: Option<&str>,
        provider: Arc<dyn KeyProvider>,
        kdf: Option<KdfParams>,
    ) -> Self {
        Self {
            op,
            work: Work::Rekey {
                record,
                current: current.map(own),
                new: new.map(own),
            },
            provider,
            kdf,
        }
    }

    /// Makes the key and its record for a first password on a library without a key.
    pub(crate) fn create(
        password: &str,
        provider: Arc<dyn KeyProvider>,
        kdf: Option<KdfParams>,
    ) -> Self {
        Self {
            op: MasterOp::Set { exists: false },
            work: Work::Create {
                password: own(password),
            },
            provider,
            kdf,
        }
    }

    /// The step this job is.
    #[must_use]
    pub const fn op(&self) -> MasterOp {
        self.op
    }

    /// Does the key derivation. Touches no controller and no store.
    #[must_use]
    pub fn run(self) -> MasterDone {
        let ring = KeyRing::new(self.provider.as_ref());
        let ring = match self.kdf {
            Some(kdf) => ring.with_kdf_params(kdf),
            None => ring,
        };
        let outcome = match &self.work {
            Work::Unlock { record, password } => ring
                .unlock(record, Some(password.as_str()))
                .map(|key| (key, None)),
            Work::Rekey {
                record,
                current,
                new,
            } => ring
                .rekey(
                    record,
                    current.as_ref().map(|p| p.as_str()),
                    new.as_ref().map(|p| p.as_str()),
                )
                .map(|(key, record)| (key, Some(record))),
            Work::Create { password } => ring
                .create(Some(password.as_str()))
                .map(|(key, record)| (key, Some(record))),
        };
        MasterDone {
            op: self.op,
            outcome,
        }
    }
}

/// What a job produced: the opened data key and, except for an unlock, the key record to store.
pub struct MasterDone {
    pub(crate) op: MasterOp,
    pub(crate) outcome: Result<(DataKey, Option<KeyRecord>), CryptoError>,
}

impl core::fmt::Debug for MasterDone {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MasterDone")
            .field("op", &self.op)
            .field("ok", &self.outcome.is_ok())
            .finish_non_exhaustive()
    }
}

impl MasterDone {
    /// A step that could not run at all (no thread, or the derivation panicked).
    pub(crate) const fn failed(op: MasterOp) -> Self {
        Self {
            op,
            outcome: Err(CryptoError::Primitive),
        }
    }

    /// The step this is the outcome of.
    #[must_use]
    pub const fn op(&self) -> MasterOp {
        self.op
    }
}

/// What to do with a job's outcome, on the UI thread.
pub type Finish = Box<dyn FnOnce(MasterDone)>;

/// Where a job's slow half runs.
pub trait Offload {
    /// Runs `job`, then calls `finish` with its outcome on the UI thread.
    fn run(&self, job: MasterJob, finish: Finish);
}

thread_local! {
    /// Where the windows of this (UI) thread run their jobs; [`Inline`] until the host says
    /// otherwise.
    static OFFLOAD: RefCell<Rc<dyn Offload>> = RefCell::new(Rc::new(Inline));
}

/// Uses `offload` for the master password steps of the windows of this thread. The app calls
/// this once with [`Threads`]; a test may hold the jobs to look at the windows meanwhile.
pub fn set_offload(offload: Rc<dyn Offload>) {
    OFFLOAD.with(|o| *o.borrow_mut() = offload);
}

/// Where a window on this thread runs a master password job.
#[must_use]
pub fn offload() -> Rc<dyn Offload> {
    OFFLOAD.with(|o| Rc::clone(&o.borrow()))
}

/// Runs the job on the calling thread, so the step is done when `run` returns.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inline;

impl Offload for Inline {
    fn run(&self, job: MasterJob, finish: Finish) {
        finish(job.run());
    }
}

thread_local! {
    /// Outcomes waiting for the UI thread, by ticket. Only the UI thread reads this.
    static WAITING: RefCell<HashMap<u64, Finish>> = RefCell::new(HashMap::new());
    static NEXT_TICKET: RefCell<u64> = const { RefCell::new(0) };
}

fn deliver(ticket: u64, done: MasterDone) {
    if let Some(finish) = WAITING.with(|w| w.borrow_mut().remove(&ticket)) {
        finish(done);
    }
}

/// Runs the job on a worker thread and hands the outcome to the Slint event loop, which calls
/// `finish`. Must be used from the UI thread.
#[derive(Debug, Clone, Copy, Default)]
pub struct Threads;

impl Offload for Threads {
    fn run(&self, job: MasterJob, finish: Finish) {
        let ticket = NEXT_TICKET.with(|n| {
            let mut n = n.borrow_mut();
            *n += 1;
            *n
        });
        WAITING.with(|w| w.borrow_mut().insert(ticket, finish));
        let op = job.op();
        let spawned = std::thread::Builder::new()
            .name("snipdeck-master".to_owned())
            .spawn(move || {
                // A panic in the derivation must not leave the window waiting for ever.
                let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run()))
                    .unwrap_or_else(|_| MasterDone::failed(op));
                // The event loop being gone means the app is closing; there is nobody to tell.
                let _ = slint::invoke_from_event_loop(move || deliver(ticket, done));
            });
        if spawned.is_err() {
            deliver(ticket, MasterDone::failed(op));
        }
    }
}
