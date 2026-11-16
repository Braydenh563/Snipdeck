//! The controller's half of the archive jobs ([`crate::archive_job`]): "Back up now", the daily
//! backup, "Export library" and the salvage copy of "Rebuild from backup". `begin_*` checks and
//! gathers what the job needs (fast, on the UI thread) and hands back the job; the host runs it
//! on a worker and calls [`Controller::finish_archive`] with the outcome, which raises the toast
//! and applies it (fast, on the UI thread). One archive runs at a time: a second request while
//! one is going is refused.
//!
//! A library with no folder (the in-memory demo) has nothing for a worker to open, so the job is
//! run in place on the store the controller holds and the step is finished at once
//! ([`Begun::Finished`]).

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

use jiff::{Zoned, tz::TimeZone};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_portable::backup::{self, BackupOutcome};
use snipdeck_store::Store;

use super::{
    Controller, Toast, ToastKind, ToastMessage,
    settings_ctl::{Carry, danger, export_path},
};
use crate::{
    archive_job::{self, ArchiveDone, ArchiveJob, ArchiveOp, Made, Source, Work},
    failure::Cause,
    host::ClipboardText,
    maintenance::{JobError, deferred_while_locked},
};

/// What a finished archive step came to, for the caller that started it.
#[derive(Debug)]
pub enum ArchiveEnd {
    /// The daily backup: what it did, or why it could not.
    Daily(Result<BackupOutcome, JobError>),
    /// "Back up now": whether it worked.
    BackupNow(bool),
    /// "Export library": the file written.
    Exported(Option<PathBuf>),
    /// "Rebuild from backup": whether it worked.
    Rebuilt(bool),
    /// An import or restore preview: what it would do, or why the source cannot be read.
    ImportPreview(Result<super::ImportPreview, super::ImportProblem>),
    /// An import or a restore: whether it worked.
    Imported(bool),
    /// The step needs another pass on a worker (a rebuild whose library changed while it was
    /// being copied): run this job, then finish it the same way.
    Again(ArchiveJob),
    /// An outcome for a step that is not running (a stale one): nothing to do.
    Ignored,
}

/// What `begin_*` came to.
#[derive(Debug)]
pub enum Begun {
    /// Nothing started (another archive is running, or the library is locked: the toast says
    /// so).
    Refused,
    /// Done in place, with nothing for a worker to do.
    Finished(ArchiveEnd),
    /// Run this on a worker and give its outcome to [`Controller::finish_archive`].
    Job(ArchiveJob),
}

/// What a running rebuild needs when its salvage copy is written.
pub(super) struct Rebuild {
    pub(super) data_dir: PathBuf,
    pub(super) backup_file: PathBuf,
    pub(super) salvage: PathBuf,
    pub(super) carry: Option<Carry>,
    /// The library's change counter when the worker's snapshot was taken: if it has moved by the
    /// time the swap runs, the salvage lacks what was written since.
    pub(super) counter: u64,
    /// How many times the copy was cut again because the library changed meanwhile.
    pub(super) attempt: u8,
}

/// What the controller keeps about the running step beyond the job itself.
pub(super) enum Context {
    None,
    /// The target of an export, for the toast and the caller.
    Export(PathBuf),
    Rebuild(Box<Rebuild>),
    /// An import, a restore or a preview of one.
    Import(Box<ImportRun>),
}

/// What an import step is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ImportPurpose {
    /// Count what would come in; write nothing.
    Preview,
    /// "Import...": write it on one new board.
    Apply,
    /// "Restore": write the backup's own boards.
    Restore,
}

/// What a running import step needs when its source has been read.
pub(super) struct ImportRun {
    pub(super) source: super::ImportSource,
    pub(super) purpose: ImportPurpose,
    /// The library's change counter when the snapshot was taken (see [`Rebuild::counter`]).
    pub(super) counter: u64,
}

/// The archive step running now, if any.
#[derive(Default)]
pub(super) struct ArchiveState {
    running: Option<(ArchiveOp, Context)>,
}

impl ArchiveState {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }
}

impl<C, K, T> Controller<Store, C, K, T>
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Whether an archive is being written. The windows keep their backup and export buttons off
    /// meanwhile.
    #[must_use]
    pub fn archive_busy(&self) -> bool {
        self.archive.running.is_some()
    }

    /// Whether a new archive step must not start now: one is running (the toast says so) or the
    /// library is locked (`copy_blocked_by_lock` raises its own toast).
    pub(super) fn refuse_archive(&mut self) -> bool {
        if self.archive_busy() {
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::ArchiveBusy,
            });
            return true;
        }
        self.copy_blocked_by_lock()
    }

    /// The app is quitting with an export or a rebuild's salvage still being written: removes the
    /// half-written `<name>.part` it would leave in the user's folder (best effort; a file the
    /// worker still holds open stays until it is closed). Only the file of the step this
    /// controller started is touched, never another `.part`.
    pub fn abandon_unfinished_archive(&mut self) {
        let path = match &self.archive.running {
            Some((_, Context::Export(path))) => path.clone(),
            Some((_, Context::Rebuild(rebuild))) => rebuild.salvage.clone(),
            _ => return,
        };
        let mut part = path.into_os_string();
        part.push(".part");
        let _ = std::fs::remove_file(PathBuf::from(part));
    }

    pub(super) fn archive_job(&self, op: ArchiveOp, work: Work) -> ArchiveJob {
        ArchiveJob {
            op,
            work,
            // A library that lives in memory has no database file for a worker to open.
            source: Source::open(
                self.store
                    .folder_lock()
                    .map(snipdeck_store::FolderLock::folder),
                self.store.duplicate_data_key(),
            ),
        }
    }

    /// Marks `job` as the running step; run in place when it has no folder to open.
    pub(super) fn begin_running(&mut self, job: ArchiveJob, context: Context) -> Begun {
        if !matches!(job.source, Source::InPlace) {
            // The worker's snapshot must still find the files that edits meanwhile orphan.
            self.store.hold_blob_release(true);
            self.archive.running = Some((job.op, context));
            return Begun::Job(job);
        }
        let done = ArchiveDone {
            op: job.op,
            made: job.run_on(&self.store),
        };
        self.archive.running = Some((job.op, context));
        Begun::Finished(self.finish_archive(done))
    }

    /// "Back up now": today's backup in `data_dir`, replacing the one already made today (the
    /// daily run writes only when there is none, and the user asked for a fresh copy).
    pub fn begin_backup_now(&mut self, data_dir: &Path) -> Begun {
        // A copy of a locked library would lack its sensitive items and replace today's whole one.
        if self.refuse_archive() {
            return Begun::Refused;
        }
        let job = self.archive_job(
            ArchiveOp::BackupNow,
            Work::Backup {
                data_dir: data_dir.to_path_buf(),
                now: self.clock.now().to_zoned(TimeZone::system()),
                keep: self.backup_keep(),
                replace: true,
            },
        );
        self.begin_running(job, Context::None)
    }

    /// The scheduled backup: today's, unless it exists.
    ///
    /// # Errors
    ///
    /// The library is locked or an archive is running: the next tick tries again.
    pub fn begin_daily_backup(&mut self, data_dir: &Path, now: &Zoned) -> Result<Begun, JobError> {
        if self.archive_busy() {
            return Err(JobError::busy());
        }
        // A library with a key ring that is locked (idle timeout) would export without its
        // sensitive items; such a backup would also count as today's and push the last complete
        // one out of the newest 7. Defer instead: the failure leaves `last_backup` alone, so the
        // next tick tries again, and the unlock makes it complete.
        deferred_while_locked(
            self.store.key_ring_exists().map_err(|e| JobError {
                cause: Cause::of_store(&e),
                detail: e.to_string(),
            })?,
            self.store.is_unlocked(),
        )?;
        let job = self.archive_job(
            ArchiveOp::DailyBackup,
            Work::Backup {
                data_dir: data_dir.to_path_buf(),
                now: now.clone(),
                keep: self.backup_keep(),
                replace: false,
            },
        );
        Ok(self.begin_running(job, Context::None))
    }

    fn backup_keep(&self) -> usize {
        usize::try_from(self.settings.backups_keep).unwrap_or(backup::DEFAULT_KEEP)
    }

    /// Exports the whole library as one `.snipdeck` file in `folder`, under a name that does not
    /// overwrite an earlier export. Sensitive items go in encrypted, or are left out when the
    /// library has no key.
    pub fn begin_export(&mut self, folder: &Path) -> Begun {
        if self.refuse_archive() {
            return Begun::Refused;
        }
        let now = self.clock.now();
        let path = export_path(folder, now.to_zoned(TimeZone::system()).date());
        let job = self.archive_job(
            ArchiveOp::Export,
            Work::Export {
                path: path.clone(),
                now,
            },
        );
        self.begin_running(job, Context::Export(path))
    }

    /// Applies the outcome of the step that was running: its toast and its effect on the library.
    /// A step that is not the running one (a stale outcome) is ignored.
    pub fn finish_archive(&mut self, done: ArchiveDone) -> ArchiveEnd {
        let Some((op, context)) = self.archive.running.take() else {
            return ArchiveEnd::Ignored;
        };
        if op != done.op {
            self.archive.running = Some((op, context));
            return ArchiveEnd::Ignored;
        }
        // The snapshot is closed: files orphaned meanwhile can go now.
        self.store.hold_blob_release(false);
        match (op, done.made, context) {
            (ArchiveOp::DailyBackup, made, _) => ArchiveEnd::Daily(made.map(|made| match made {
                Made::Backup(outcome) => outcome,
                Made::File | Made::Decoded(_) | Made::Rebuilt(_) => BackupOutcome::NotDue {
                    existing: PathBuf::new(),
                },
            })),
            (ArchiveOp::BackupNow, made, _) => {
                let worked = made.is_ok();
                self.show_toast(match made {
                    Ok(_) => Toast {
                        kind: ToastKind::Success,
                        message: ToastMessage::BackupMade,
                    },
                    Err(e) => {
                        // The toast gives the cause; the log names an item that is too big.
                        crate::trace!("backup failed: {e}");
                        danger(ToastMessage::BackupFailed(e.cause))
                    }
                });
                ArchiveEnd::BackupNow(worked)
            }
            (ArchiveOp::Export, made, Context::Export(path)) => match made {
                Ok(_) => {
                    let file = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.show_toast(Toast {
                        kind: ToastKind::Success,
                        message: ToastMessage::Exported { file },
                    });
                    ArchiveEnd::Exported(Some(path))
                }
                Err(e) => {
                    self.show_toast(danger(ToastMessage::ExportFailed(e.cause)));
                    ArchiveEnd::Exported(None)
                }
            },
            (ArchiveOp::Salvage, made, Context::Rebuild(rebuild)) => {
                self.finish_rebuild(*rebuild, made)
            }
            (ArchiveOp::Decode, made, Context::Import(run)) => self.finish_import(&run, made),
            // A context that does not belong to its step cannot be built.
            (_, _, _) => ArchiveEnd::Ignored,
        }
    }
}

/// Runs what a `begin_*` call came to: a job goes to the archive offload and, when it is done on
/// the UI thread, `controller` finishes it and `after` hears the end. With no job (refused, or
/// done at once) `after` runs now. Call it with no borrow of `controller` held.
pub fn spawn<C, K, T>(
    controller: &Rc<RefCell<Controller<Store, C, K, T>>>,
    begun: Begun,
    after: impl FnOnce(ArchiveEnd) + 'static,
) where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    let job = match begun {
        Begun::Refused => return after(ArchiveEnd::Ignored),
        Begun::Finished(end) => return after(end),
        Begun::Job(job) => job,
    };
    let controller_weak = Rc::downgrade(controller);
    archive_job::offload().run(
        job,
        Box::new(move |done| {
            let Some(controller) = controller_weak.upgrade() else {
                return;
            };
            let end = controller.borrow_mut().finish_archive(done);
            match end {
                // Another pass on a worker; the caller hears only the last end.
                ArchiveEnd::Again(job) => spawn(&controller, Begun::Job(job), after),
                end => after(end),
            }
            crate::binding::redraw_all();
        }),
    );
    // Every window shows the busy state from the start.
    crate::binding::redraw_all();
}
