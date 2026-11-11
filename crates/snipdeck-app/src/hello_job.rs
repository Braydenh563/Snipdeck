//! The slow half of a Windows Hello step (v1.1 slice 8, `docs/crypto.md` "Windows Hello").
//!
//! Hello may put a prompt on screen and wait for a person, so like [`crate::master_job`] a step is
//! split in three: the controller gathers what the work needs (`Controller::begin_hello_*`, fast),
//! [`HelloJob::run`] talks to Hello (no controller, no store: any thread), and the controller
//! applies the outcome (`Controller::finish_hello`, fast, back on the UI thread).
//!
//! Slint-free apart from [`run_job`]'s worker hand-back. Nothing here logs key material or a
//! signature; a job and its outcome wipe the keys they hold when dropped.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use snipdeck_platform::hello::{HelloAuthenticator, HelloAvailability, HelloError};
use snipdeck_store::crypto::{DataKey, HelloSealError, SealedKey};

/// What a job does.
enum Work {
    /// Seals `key` under Hello (after a password unlock). `enrol`: this is the first seal after the
    /// user turned Hello on, so two signatures are compared; later seals sign once.
    Seal { key: DataKey, enrol: bool },
    /// Asks Hello whether it can be used.
    Availability,
    /// Opens `sealed` with Hello.
    Open { sealed: SealedKey },
}

/// One Hello step. `Send`: hand it to a worker and its [`HelloDone`] back.
pub struct HelloJob {
    hello: Arc<dyn HelloAuthenticator>,
    now: Duration,
    work: Work,
    generation: u64,
    /// Set by the controller when the library is locked or the step is given up on: the job then
    /// does not ask Hello and drops its key.
    cancel: Arc<AtomicBool>,
}

impl core::fmt::Debug for HelloJob {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HelloJob").finish_non_exhaustive()
    }
}

/// Why a Hello step gave nothing. Every one falls back to the password and drops the copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloFailure {
    /// The seal is older than 7 days (or the clock went back). Hello was not asked.
    Expired,
    /// The user cancelled the prompt.
    Cancelled,
    /// Hello failed, was removed, or is not repeatable on this PC.
    Failed,
    /// Hello answered, but its signature no longer opens the copy (a PIN reset made a new
    /// credential, for example): the copy goes, and the next password unlock seals a new one.
    Mismatch,
    /// Two signatures of the same challenge differ: this Hello cannot protect a key and is
    /// turned off.
    NotRepeatable,
}

fn failure(error: HelloSealError<HelloError>) -> HelloFailure {
    match error {
        HelloSealError::Expired => HelloFailure::Expired,
        HelloSealError::Sign(HelloError::Cancelled) => HelloFailure::Cancelled,
        HelloSealError::NotRepeatable => HelloFailure::NotRepeatable,
        HelloSealError::Sign(_) | HelloSealError::Failed => HelloFailure::Failed,
    }
}

/// What a finished job hands back.
pub(crate) enum Product {
    /// A new sealed copy.
    Sealed(SealedKey),
    /// Whether Hello can be used.
    Availability(HelloAvailability),
    /// The data key the copy opened.
    Opened(DataKey),
}

/// The outcome of a [`HelloJob`].
pub struct HelloDone {
    pub(crate) generation: u64,
    /// Which counter `generation` is from: the availability questions or the steps. Set by the job,
    /// so a job that panicked or could not start still says which counter to compare with.
    pub(crate) availability: bool,
    pub(crate) outcome: Result<Product, HelloFailure>,
}

impl core::fmt::Debug for HelloDone {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HelloDone")
            .field("ok", &self.outcome.is_ok())
            .finish_non_exhaustive()
    }
}

impl HelloDone {
    /// Whether this answers "can Hello be used" (it has its own generation, apart from the steps).
    #[must_use]
    pub fn is_availability(&self) -> bool {
        self.availability
    }
}

impl HelloJob {
    pub(crate) fn seal(
        hello: Arc<dyn HelloAuthenticator>,
        key: DataKey,
        enrol: bool,
        now: Duration,
        (generation, cancel): (u64, Arc<AtomicBool>),
    ) -> Self {
        Self {
            hello,
            now,
            work: Work::Seal { key, enrol },
            generation,
            cancel,
        }
    }

    pub(crate) fn availability(hello: Arc<dyn HelloAuthenticator>, generation: u64) -> Self {
        Self {
            hello,
            now: Duration::ZERO,
            work: Work::Availability,
            generation,
            cancel: Arc::default(),
        }
    }

    pub(crate) fn open(
        hello: Arc<dyn HelloAuthenticator>,
        sealed: SealedKey,
        now: Duration,
        (generation, cancel): (u64, Arc<AtomicBool>),
    ) -> Self {
        Self {
            hello,
            now,
            work: Work::Open { sealed },
            generation,
            cancel,
        }
    }

    /// Talks to Hello; may block on a prompt. Touches no controller and no store.
    #[must_use]
    pub fn run(self) -> HelloDone {
        let hello = self.hello;
        let cancel = self.cancel;
        let availability = matches!(self.work, Work::Availability);
        // Asked at each prompt: a locked library or a given-up step does not wait for a person.
        let sign = |challenge: &[u8]| {
            if cancel.load(Ordering::SeqCst) {
                Err(HelloError::Cancelled)
            } else {
                hello.sign_challenge(challenge)
            }
        };
        let outcome = match self.work {
            Work::Availability => Ok(Product::Availability(hello.availability())),
            Work::Seal { key, enrol } => if cancel.load(Ordering::SeqCst) {
                Err(HelloError::Cancelled)
            } else {
                hello.ensure_credential()
            }
            .map_err(|e| failure(HelloSealError::Sign(e)))
            .and_then(|()| {
                let sealed = if enrol {
                    SealedKey::seal(&key, self.now, sign)
                } else {
                    SealedKey::seal_once(&key, self.now, sign)
                };
                sealed.map(Product::Sealed).map_err(failure)
            }),
            Work::Open { sealed } => sealed
                .open(self.now, sign)
                .map(Product::Opened)
                .map_err(|e| match e {
                    HelloSealError::Failed => HelloFailure::Mismatch,
                    other => failure(other),
                }),
        };
        HelloDone {
            generation: self.generation,
            availability,
            outcome,
        }
    }

    /// The outcome of a job that could not run (its thread could not start, or it panicked).
    #[must_use]
    pub fn failed(&self) -> HelloDone {
        HelloDone {
            generation: self.generation,
            availability: matches!(self.work, Work::Availability),
            outcome: Err(HelloFailure::Failed),
        }
    }
}

/// What to do with a job's outcome, on the UI thread.
pub type Finish = Box<dyn FnOnce(HelloDone)>;

thread_local! {
    static THREADS: RefCell<bool> = const { RefCell::new(false) };
    static WAITING: RefCell<HashMap<u64, Finish>> = RefCell::new(HashMap::new());
    static NEXT_TICKET: RefCell<u64> = const { RefCell::new(0) };
    static HELD: RefCell<Option<Vec<(HelloJob, Finish)>>> = const { RefCell::new(None) };
}

/// For tests: from now on the jobs of this thread wait in a queue instead of running, so a test
/// can finish them in any order (and look at the windows meanwhile).
pub fn hold_jobs() {
    HELD.with(|h| *h.borrow_mut() = Some(Vec::new()));
}

/// For tests: how many jobs wait.
#[must_use]
pub fn held_jobs() -> usize {
    HELD.with(|h| h.borrow().as_ref().map_or(0, Vec::len))
}

/// For tests: runs the `index`th waiting job and applies its outcome, as the worker would. Returns
/// whether a job waited at `index`.
pub fn finish_held(index: usize) -> bool {
    let held = HELD.with(|h| {
        h.borrow_mut()
            .as_mut()
            .filter(|queue| index < queue.len())
            .map(|queue| queue.remove(index))
    });
    held.map(|(job, finish)| finish(job.run())).is_some()
}

/// For tests: jobs run before `run_job` returns again; waiting jobs are dropped.
pub fn release_jobs() {
    HELD.with(|h| *h.borrow_mut() = None);
}

/// Runs Hello jobs of this (UI) thread on worker threads instead of the calling thread. The app
/// calls this once; tests keep the default, which runs the job before `run_job` returns.
pub fn use_worker_threads() {
    THREADS.with(|t| *t.borrow_mut() = true);
}

fn deliver(ticket: u64, done: HelloDone) {
    if let Some(finish) = WAITING.with(|w| w.borrow_mut().remove(&ticket)) {
        finish(done);
    }
}

/// Runs `job`, then calls `finish` with its outcome on the UI thread.
pub fn run_job(job: HelloJob, finish: Finish) {
    let mut pending = Some((job, finish));
    HELD.with(|h| {
        if let Some(queue) = h.borrow_mut().as_mut() {
            queue.extend(pending.take());
        }
    });
    let Some((job, finish)) = pending else {
        return;
    };
    if !THREADS.with(|t| *t.borrow()) {
        finish(job.run());
        return;
    }
    let ticket = NEXT_TICKET.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        *n
    });
    WAITING.with(|w| w.borrow_mut().insert(ticket, finish));
    let failed = job.failed();
    let (generation, availability) = (job.generation, failed.availability);
    let spawned = std::thread::Builder::new()
        .name("snipdeck-hello".to_owned())
        .spawn(move || {
            let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run()))
                .unwrap_or_else(|_| HelloDone {
                    generation,
                    availability,
                    outcome: Err(HelloFailure::Failed),
                });
            let _ = slint::invoke_from_event_loop(move || deliver(ticket, done));
        });
    if spawned.is_err() {
        deliver(ticket, failed);
    }
}
