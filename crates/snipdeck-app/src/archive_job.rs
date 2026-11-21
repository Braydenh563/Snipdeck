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
    BoardLayout, Decoded, ExportOptions, ImportOptions, KeySource, Known, Library, MergeStrategy,
    SensitivePolicy,
    backup::{self, BackupOutcome},
    export_to_path,
};
use snipdeck_store::{Store, crypto::DataKey};

use crate::{controller::ImportSource, failure::Cause, maintenance::JobError};

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
        /// Also copy the file written into this folder, pruned to `keep` (the Store build's
        /// `Documents\Snipdeck backups`, `store_data`).
        copy_to: Option<PathBuf>,
    },
    /// The whole library as one `.snipdeck` file at `path` (its folder is made first).
    Export { path: PathBuf, now: Timestamp },
    /// Everything a rebuild reads: the backup is read and checked (no library involved), the
    /// salvage copy of the damaged library is written at `salvage` and read back.
    Rebuild {
        backup: PathBuf,
        salvage: PathBuf,
        now: Timestamp,
    },
    /// Reads and checks an import or a restore (no library involved) and the duplicate set of
    /// the snapshot, for the UI thread to preview or apply.
    Decode {
        source: ImportSource,
        layout: BoardLayout,
        now: Timestamp,
    },
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
    /// Reading an import or a restore source.
    Decode,
}

/// What a finished job made.
#[derive(Debug)]
pub enum Made {
    /// A backup: what [`backup::run_if_due`] did.
    Backup(BackupOutcome),
    /// A file at the job's path.
    File,
    /// What a rebuild will apply: the backup and the salvage, read.
    Rebuilt(Box<Prepared>),
    /// A decoded source, or why it could not be read.
    Decoded(Box<Result<Ready, snipdeck_portable::Error>>),
}

/// A rebuild's two sources, read on the worker.
#[derive(Debug)]
pub struct Prepared {
    /// The backup, or why it cannot be read (the rebuild then touches nothing).
    pub(crate) backup: Result<Decoded<'static>, snipdeck_portable::Error>,
    /// What the damaged library could still give up (`None`: nothing could be salvaged).
    pub(crate) salvage: Option<Decoded<'static>>,
}

/// A decoded import and the duplicate set of the snapshot it was read beside.
#[derive(Debug)]
pub struct Ready {
    pub(crate) decoded: Decoded<'static>,
    pub(crate) known: Known,
}

/// Where a job reads the library from.
#[derive(Debug)]
pub(crate) enum Source {
    /// A read-only view whose snapshot began when the step did (on the UI thread, so what is
    /// written is the library as it was then, whatever is edited while the worker runs).
    Reader(Box<Store>),
    /// The view could not be made: the job fails with this.
    Failed(JobError),
    /// A library with no folder: the controller writes from its own store, in place.
    InPlace,
}

impl Source {
    /// A snapshot reader on the data folder `folder` (`None`: no folder).
    pub(crate) fn open(folder: Option<&Path>, key: Option<DataKey>) -> Self {
        let Some(folder) = folder else {
            return Self::InPlace;
        };
        match Store::open_reader(folder, key)
            .and_then(|reader| reader.begin_snapshot().map(|()| reader))
        {
            Ok(reader) => Self::Reader(Box::new(reader)),
            Err(e) => Self::Failed(JobError {
                cause: Cause::of_store(&e),
                detail: e.to_string(),
            }),
        }
    }
}

/// The slow half of one archive step. `Send`.
#[derive(Debug)]
pub struct ArchiveJob {
    pub(crate) op: ArchiveOp,
    pub(crate) work: Work,
    pub(crate) source: Source,
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

/// How a worker reads a source: duplicates skipped, encrypted items opened with the library's key.
fn read_options<L: Library>(lib: &L, layout: BoardLayout) -> ImportOptions<'_> {
    ImportOptions {
        merge: MergeStrategy::SkipDuplicates,
        layout,
        key: lib.data_key().map_or(KeySource::None, KeySource::Given),
        ..ImportOptions::default()
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
    /// is the library as it was when the step began.
    #[must_use]
    pub fn run(self) -> ArchiveDone {
        let op = self.op;
        let made = match &self.source {
            Source::Reader(reader) => self.run_on(reader.as_ref()),
            Source::Failed(e) => Err(e.clone()),
            Source::InPlace => Err(JobError::other("no data folder to read")),
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
            Work::Decode {
                source,
                layout,
                now,
            } => {
                let options = read_options(lib, *layout);
                let ready = source
                    .decode(*now, &options)
                    .and_then(|decoded| Known::of(lib).map(|known| Ready { decoded, known }));
                return Ok(Made::Decoded(Box::new(ready)));
            }
            Work::Rebuild {
                backup,
                salvage,
                now,
            } => {
                let read = |file: &Path| {
                    ImportSource::Snipdeck(file.to_path_buf())
                        .decode(*now, &read_options(lib, BoardLayout::KeepBoards))
                };
                let backup = read(backup);
                // An unreadable backup ends the rebuild before it starts: no copy is needed.
                let salvaged = backup.is_ok().then(|| {
                    export_to_path(lib, *now, &options, salvage)
                        .and_then(|_| read(salvage))
                        .ok()
                });
                return Ok(Made::Rebuilt(Box::new(Prepared {
                    backup,
                    salvage: salvaged.flatten(),
                })));
            }
            _ => {}
        }
        match &self.work {
            Work::Backup {
                data_dir,
                now,
                keep,
                replace,
                copy_to,
            } => {
                let outcome = backup::run_if_due(lib, data_dir, now, *keep, lib.data_key())
                    .map_err(|e| failure(&e))?;
                // The file to copy out of the package: only one that this run wrote. A failed
                // backup returned above and copies nothing.
                let written = match &outcome {
                    BackupOutcome::Created { path, .. } => Some(path.clone()),
                    BackupOutcome::NotDue { existing } if *replace => {
                        // The user asked for a fresh copy: write over today's.
                        export_to_path(lib, now.timestamp(), &options, existing)
                            .map_err(|e| failure(&e))?;
                        Some(existing.clone())
                    }
                    BackupOutcome::NotDue { .. } => None,
                };
                if let (Some(file), Some(folder)) = (written, copy_to) {
                    // The backup is made; a copy that fails is logged, not a failed backup.
                    if let Err(e) = crate::store_data::copy_out(
                        &crate::store_data::RealFs,
                        &file,
                        folder,
                        *keep,
                        now.date(),
                    ) {
                        crate::trace!("backup copy to {}: {e}", folder.display());
                    }
                }
                Ok(Made::Backup(outcome))
            }
            Work::Decode { .. } | Work::Rebuild { .. } => {
                Err(JobError::other("not an archive step"))
            }
            Work::Export { path, now } => {
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
            Work::Export { path, .. } => Some(path),
            Work::Rebuild { salvage, .. } => Some(salvage),
            Work::Backup { .. } | Work::Decode { .. } => None,
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
    guard(job.op, || job.run())
}

/// Runs `work`; a panic is logged with its message and becomes a failed outcome of `op`.
pub(crate) fn guard(op: ArchiveOp, work: impl FnOnce() -> ArchiveDone) -> ArchiveDone {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|payload| {
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".to_owned());
        crate::trace!("archive worker panicked ({op:?}): {what}");
        ArchiveDone::failed(op)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_worker_becomes_a_failed_outcome_of_its_step() {
        for done in [
            guard(ArchiveOp::Export, || panic!("boom")),
            guard(ArchiveOp::Export, || panic!("{}", String::from("boom"))),
            guard(ArchiveOp::Export, || std::panic::panic_any(7_u8)),
        ] {
            assert_eq!(done.op(), ArchiveOp::Export);
            assert!(done.made.is_err());
        }
        let fine = guard(ArchiveOp::BackupNow, || ArchiveDone {
            op: ArchiveOp::BackupNow,
            made: Ok(Made::File),
        });
        assert!(fine.made.is_ok(), "no panic, no change");
    }
}
