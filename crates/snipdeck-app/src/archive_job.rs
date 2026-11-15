//! Writing archives off the UI thread: the daily backup, "Back up now", "Export library" and the
//! salvage copy of "Rebuild from backup". Compressing and encrypting a large library takes long
//! enough to freeze the windows, so the work is a [`ArchiveJob`] (no controller, no main store:
//! it opens its own read-only view of the database, [`Store::open_reader`], and reads one
//! consistent moment of it), and the controller applies the outcome on the UI thread
//! (`Controller::finish_archive`).
//!
//! Slint-free except for the hand-back. [`Offload`] says where `run` happens, as for
//! [`crate::master_job`]: [`Inline`] on the calling thread (tests), [`Threads`] on a worker.
//! A library with no folder (the in-memory demo) never builds a job: the controller writes from
//! the store it holds, in place.

use std::{
    cell::RefCell,
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    rc::Rc,
};

use jiff::Zoned;
use snipdeck_core::Timestamp;
use snipdeck_portable::{
    ExportOptions, Library, SensitivePolicy,
    backup::{self, BackupOutcome},
    export_to_path,
};
use snipdeck_store::{Store, crypto::DataKey};

use crate::{failure::Cause, maintenance::JobError};

/// What a job writes.
#[derive(Debug, Clone)]
pub(crate) enum Work {
    /// Today's backup in `data_dir`, pruned to `keep`. `replace` writes a fresh copy even when
    /// today's exists ("Back up now").
    Backup {
        data_dir: PathBuf,
        now: Zoned,
        keep: usize,
        replace: bool,
    },
    /// The whole library as one `.snipdeck` file at `path` (its folder is made first).
    Export { path: PathBuf, now: Timestamp },
    /// The salvage copy for a rebuild, at `path`.
    Salvage { path: PathBuf, now: Timestamp },
}

/// Which step a job is, so the controller knows what to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveOp {
    /// The scheduled daily backup.
    DailyBackup,
    /// "Back up now".
    BackupNow,
    /// "Export library".
    Export,
    /// The salvage copy of "Rebuild from backup".
    Salvage,
}

/// What a finished job made.
#[derive(Debug)]
pub enum Made {
    /// A backup: what [`backup::run_if_due`] did.
    Backup(BackupOutcome),
    /// A file at the job's path.
    File,
}

/// The slow half of one archive step. `Send`.
#[derive(Debug)]
pub struct ArchiveJob {
    pub(crate) op: ArchiveOp,
    pub(crate) work: Work,
    /// The data folder the reader opens; `None` for a library with no folder.
    pub(crate) folder: Option<PathBuf>,
    pub(crate) key: Option<DataKey>,
}

/// What a job came to. `Send`.
#[derive(Debug)]
pub struct ArchiveDone {
    pub(crate) op: ArchiveOp,
    pub(crate) made: Result<Made, JobError>,
}

impl ArchiveDone {
    /// The step this is the outcome of.
    #[must_use]
    pub const fn op(&self) -> ArchiveOp {
        self.op
    }

    /// A step that could not run at all (no thread, or the work panicked).
    #[must_use]
    pub fn failed(op: ArchiveOp) -> Self {
        Self {
            op,
            made: Err(JobError::other("the archive worker stopped")),
        }
    }
}

fn failure(e: &snipdeck_portable::Error) -> JobError {
    JobError {
        cause: Cause::of_portable(e),
        detail: e.to_string(),
    }
}

impl ArchiveJob {
    /// Does the work on its own read-only view of the library: one snapshot, so what is written
    /// is the library as it was when the job started.
    #[must_use]
    pub fn run(mut self) -> ArchiveDone {
        let op = self.op;
        let key = self.key.take();
        let made = match &self.folder {
            Some(folder) => Store::open_reader(folder, key)
                .and_then(|reader| reader.begin_snapshot().map(|()| reader))
                .map_err(|e| JobError {
                    cause: Cause::of_store(&e),
                    detail: e.to_string(),
                })
                .and_then(|reader| self.run_on(&reader)),
            None => Err(JobError::other("no data folder to read")),
        };
        ArchiveDone { op, made }
    }

    /// Does the work reading from `lib`: the store the controller holds, for a library with no
    /// folder.
    pub(crate) fn run_on<L: Library>(&self, lib: &L) -> Result<Made, JobError> {
        let options = ExportOptions {
            sensitive: lib
                .data_key()
                .map_or(SensitivePolicy::Skip, SensitivePolicy::Encrypt),
            ..ExportOptions::default()
        };
        match &self.work {
            Work::Backup {
                data_dir,
                now,
                keep,
                replace,
            } => {
                let outcome = backup::run_if_due(lib, data_dir, now, *keep, lib.data_key())
                    .map_err(|e| failure(&e))?;
                match outcome {
                    // The user asked for a fresh copy: write over today's.
                    BackupOutcome::NotDue { existing } if *replace => {
                        export_to_path(lib, now.timestamp(), &options, &existing)
                            .map_err(|e| failure(&e))?;
                        Ok(Made::Backup(BackupOutcome::NotDue { existing }))
                    }
                    other => Ok(Made::Backup(other)),
                }
            }
            Work::Export { path, now } | Work::Salvage { path, now } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| JobError {
                        cause: Cause::of_io(&e),
                        detail: e.to_string(),
                    })?;
                }
                export_to_path(lib, *now, &options, path).map_err(|e| failure(&e))?;
                Ok(Made::File)
            }
        }
    }

    /// The path a salvage or export job writes, if it is one.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match &self.work {
            Work::Export { path, .. } | Work::Salvage { path, .. } => Some(path),
            Work::Backup { .. } => None,
        }
    }
}

/// What to do with a job's outcome, on the UI thread.
pub type Finish = Box<dyn FnOnce(ArchiveDone)>;

/// Where a job's slow half runs.
pub trait Offload {
    /// Runs `job`, then calls `finish` with its outcome on the UI thread.
    fn run(&self, job: ArchiveJob, finish: Finish);
}

thread_local! {
    static OFFLOAD: RefCell<Rc<dyn Offload>> = RefCell::new(Rc::new(Inline));
    static WAITING: RefCell<HashMap<u64, Finish>> = RefCell::new(HashMap::new());
    static NEXT_TICKET: RefCell<u64> = const { RefCell::new(0) };
}

/// Uses `offload` for the archive jobs of this (UI) thread. The app calls this once with
/// [`Threads`]; a test may hold the jobs to look at the windows meanwhile.
pub fn set_offload(offload: Rc<dyn Offload>) {
    OFFLOAD.with(|o| *o.borrow_mut() = offload);
}

/// Where a job on this thread runs.
#[must_use]
pub fn offload() -> Rc<dyn Offload> {
    OFFLOAD.with(|o| Rc::clone(&o.borrow()))
}

/// Runs the job on the calling thread, so the step is done when `run` returns.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inline;

impl Offload for Inline {
    fn run(&self, job: ArchiveJob, finish: Finish) {
        finish(guarded(job));
    }
}

fn deliver(ticket: u64, done: ArchiveDone) {
    if let Some(finish) = WAITING.with(|w| w.borrow_mut().remove(&ticket)) {
        finish(done);
    }
}

/// Runs the job on a worker thread and hands the outcome to the Slint event loop, which calls
/// `finish`. Must be used from the UI thread.
#[derive(Debug, Clone, Copy, Default)]
pub struct Threads;

impl Offload for Threads {
    fn run(&self, job: ArchiveJob, finish: Finish) {
        let ticket = NEXT_TICKET.with(|n| {
            let mut n = n.borrow_mut();
            *n += 1;
            *n
        });
        WAITING.with(|w| w.borrow_mut().insert(ticket, finish));
        let op = job.op;
        let spawned = std::thread::Builder::new()
            .name("snipdeck-archive".to_owned())
            .spawn(move || {
                let done = guarded(job);
                // The event loop being gone means the app is closing; nobody to tell.
                let _ = slint::invoke_from_event_loop(move || deliver(ticket, done));
            });
        if spawned.is_err() {
            deliver(ticket, ArchiveDone::failed(op));
        }
    }
}

/// Runs `job`; a panic becomes a failed outcome, so the controller is never left busy for ever.
fn guarded(job: ArchiveJob) -> ArchiveDone {
    let op = job.op;
    catch_unwind(AssertUnwindSafe(|| job.run())).unwrap_or_else(|_| ArchiveDone::failed(op))
}
