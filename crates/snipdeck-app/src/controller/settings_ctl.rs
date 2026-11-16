//! The Settings window's half of the [`Controller`]: the backups, the export
//! and "Erase library". Slint-free like the rest of the controller: `settings_binding` calls
//! these from the page's callbacks and draws the toast each one raises.
//!
//! These work on the real [`Store`] (the backup and the erase need the folder it lives in), so
//! they are written for `Controller<Store, ..>` rather than for any [`Library`](super::Library).

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use jiff::tz::TimeZone;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_portable::{
    BoardLayout, Decoded, HistoryEntry, ImportOptions, backup, decode_archive_path,
    decode_beeftext, decode_copyq_json, decode_ditto, decode_entries, decode_espanso,
    decode_folder, decode_phraseexpress_csv, decode_textexpander_csv,
};
use snipdeck_store::{
    ItemRepo, Store,
    crypto::{DataKey, KeyRecord},
};

use super::{
    Controller, Toast, ToastKind, ToastMessage,
    archive_ctl::{ArchiveEnd, Begun, Context, ImportPurpose, ImportRun, Rebuild},
};
use crate::archive_job::{ArchiveOp, Made, Work};
use crate::maintenance::JobError;
use crate::{failure::Cause, host::ClipboardText, starter};

/// Where an import reads from. Each variant is one kind of source the Settings "Import..." row
/// offers; a new source (the Windows clipboard history, say) is one more variant, one arm in
/// `ImportSource::run` and one entry in the page's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportSource {
    /// A `.snipdeck` file (an export or a backup): its boards come back under their own names.
    Snipdeck(PathBuf),
    /// A Ditto clipboard-manager database.
    Ditto(PathBuf),
    /// A `CopyQ` JSON export.
    CopyQ(PathBuf),
    /// A folder of `.txt` and `.md` files, one item each.
    Folder(PathBuf),
    /// An Espanso configuration: its folder (or one of its `.yml` files).
    Espanso(PathBuf),
    /// A Beeftext JSON export.
    Beeftext(PathBuf),
    /// A `TextExpander` CSV export.
    TextExpander(PathBuf),
    /// A `PhraseExpress` CSV export.
    PhraseExpress(PathBuf),
    /// The text entries of the Windows clipboard history (Win+V), already read by the caller.
    /// They land on a new board called `board` (the caller words it in the UI's language).
    ClipboardHistory {
        /// The text entries, newest first.
        entries: Vec<HistoryEntry>,
        /// The name of the board they land on.
        board: String,
    },
}

impl ImportSource {
    /// The file or folder this reads; `None` for the clipboard history.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Snipdeck(p)
            | Self::Ditto(p)
            | Self::CopyQ(p)
            | Self::Folder(p)
            | Self::Espanso(p)
            | Self::Beeftext(p)
            | Self::TextExpander(p)
            | Self::PhraseExpress(p) => Some(p),
            Self::ClipboardHistory { .. } => None,
        }
    }

    /// The name of the file or folder, for the preview's heading; empty for the clipboard
    /// history, which the page names itself.
    #[must_use]
    pub fn file_name(&self) -> String {
        self.path()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Reads and checks this source, with no library: what a worker does. The result is
    /// previewed or applied by [`Decoded`]. `options.key` must not be `KeySource::Library`.
    pub(crate) fn decode(
        &self,
        now: jiff::Timestamp,
        options: &ImportOptions<'_>,
    ) -> snipdeck_portable::Result<Decoded<'static>> {
        match self {
            Self::Snipdeck(p) => decode_archive_path(p, now, options),
            Self::Ditto(p) => decode_ditto(p, now, options),
            Self::CopyQ(p) => decode_copyq_json(p, now, options),
            Self::Folder(p) => decode_folder(p, now, options),
            Self::Espanso(p) => decode_espanso(p, now, options),
            Self::Beeftext(p) => decode_beeftext(p, now, options),
            Self::TextExpander(p) => decode_textexpander_csv(p, now, options),
            Self::PhraseExpress(p) => decode_phraseexpress_csv(p, now, options),
            Self::ClipboardHistory { entries, board } => {
                decode_entries(entries, board, now, options)
            }
        }
    }
}

/// What an import would do, worked out by a dry run: nothing has been written. Plain data for
/// the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPreview {
    /// Boards that would be created.
    pub boards: usize,
    /// Lists that would be created.
    pub lists: usize,
    /// Items that would be added.
    pub items: usize,
    /// Items already in the library (same content), which are skipped.
    pub duplicates: usize,
    /// Tags that would be created.
    pub tags: usize,
    /// What will not be imported or needs a look (worded by `wording::import_warning_text`).
    pub warnings: Vec<snipdeck_portable::Warning>,
    /// Warnings left out of `warnings` because there were too many.
    pub warnings_dropped: usize,
    /// The triggers the file would install (the first few), so the user sees what it types.
    pub triggers: Vec<String>,
    /// How many triggers in all.
    pub triggers_total: usize,
}

impl ImportPreview {
    /// Whether importing would change nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.boards == 0 && self.lists == 0 && self.items == 0 && self.tags == 0
    }
}

/// Why a source could not be previewed or imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportProblem {
    /// The file or folder could not be opened.
    Unreadable,
    /// It is not the kind of file the choice named, or it is damaged.
    NotThisKind(ImportKind),
    /// A `.snipdeck` file from a newer Snipdeck.
    TooNew,
    /// Windows clipboard history is turned off.
    HistoryOff,
    /// This PC cannot give out its clipboard history (an edition without it, or a policy).
    HistoryUnavailable,
    /// The clipboard history holds no text.
    HistoryEmpty,
    /// Anything else (the library could not be written, the file is over the safety limits).
    Failed,
}

/// The kind of source, without a path: what the page's choices and the problem wording name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// A `.snipdeck` file.
    Snipdeck,
    /// A Ditto database.
    Ditto,
    /// A `CopyQ` export.
    CopyQ,
    /// A folder of text files.
    Folder,
    /// An Espanso configuration folder.
    Espanso,
    /// A Beeftext export.
    Beeftext,
    /// A `TextExpander` CSV export.
    TextExpander,
    /// A `PhraseExpress` CSV export.
    PhraseExpress,
}

impl ImportSource {
    /// The source of `kind` at `path`.
    #[must_use]
    pub fn of(kind: ImportKind, path: PathBuf) -> Self {
        match kind {
            ImportKind::Snipdeck => Self::Snipdeck(path),
            ImportKind::Ditto => Self::Ditto(path),
            ImportKind::CopyQ => Self::CopyQ(path),
            ImportKind::Folder => Self::Folder(path),
            ImportKind::Espanso => Self::Espanso(path),
            ImportKind::Beeftext => Self::Beeftext(path),
            ImportKind::TextExpander => Self::TextExpander(path),
            ImportKind::PhraseExpress => Self::PhraseExpress(path),
        }
    }

    /// The kind of this source; `None` for the clipboard history, which is not a file.
    #[must_use]
    pub fn kind(&self) -> Option<ImportKind> {
        match self {
            Self::Snipdeck(_) => Some(ImportKind::Snipdeck),
            Self::Ditto(_) => Some(ImportKind::Ditto),
            Self::CopyQ(_) => Some(ImportKind::CopyQ),
            Self::Folder(_) => Some(ImportKind::Folder),
            Self::Espanso(_) => Some(ImportKind::Espanso),
            Self::Beeftext(_) => Some(ImportKind::Beeftext),
            Self::TextExpander(_) => Some(ImportKind::TextExpander),
            Self::PhraseExpress(_) => Some(ImportKind::PhraseExpress),
            Self::ClipboardHistory { .. } => None,
        }
    }
}

fn problem_of(source: &ImportSource, error: &snipdeck_portable::Error) -> ImportProblem {
    use snipdeck_portable::Error as E;
    match error {
        E::File { .. } | E::Io(_) => ImportProblem::Unreadable,
        E::FormatTooNew { .. } => ImportProblem::TooNew,
        E::NotAnArchive(_)
        | E::UnsafeEntry { .. }
        | E::MissingEntry(_)
        | E::Malformed { .. }
        | E::UnsupportedFormat(_)
        | E::CountMismatch { .. }
        | E::AssetMismatch { .. }
        | E::DuplicateId { .. }
        | E::NotDitto { .. }
        | E::Sqlite(_) => source
            .kind()
            .map_or(ImportProblem::Failed, ImportProblem::NotThisKind),
        _ => ImportProblem::Failed,
    }
}

/// The names in the data folder that are the library, which "Erase library" deletes. A list of
/// what is ours, not of what is kept: `--data-dir` may point at a folder that holds other
/// things (Documents, a synced folder, a stick), and those are not Snipdeck's to delete. The
/// settings, the logs and the lock are not in it.
fn is_library_file(name: &str) -> bool {
    const EXACT: [&str; 10] = [
        "snipdeck.db",
        "snipdeck.db-wal",
        "snipdeck.db-shm",
        "snipdeck.db-journal",
        "snipdeck.salvage.snipdeck",
        "snipdeck.salvage.snipdeck.part",
        "blobs",
        "backups",
        "drafts",
        "maintenance.json",
    ];
    EXACT.contains(&name)
        || name.starts_with("maintenance.json.")
        || (name.starts_with("snipdeck.db.pre-v") && name.ends_with(".bak"))
        || name.starts_with("snipdeck.damaged-")
}

/// Deletes the library from `folder`: the files and folders [`is_library_file`] names. Anything
/// else in the folder stays. Goes on past a file it cannot delete and reports the first failure
/// at the end.
fn wipe_library(folder: &Path) -> io::Result<()> {
    let mut first_error = None;
    for entry in fs::read_dir(folder)?.flatten() {
        if !entry.file_name().to_str().is_some_and(is_library_file) {
            continue;
        }
        let path = entry.path();
        let removed = if entry.file_type().is_ok_and(|t| t.is_dir()) {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if let Err(e) = removed {
            first_error.get_or_insert(e);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod erase_tests {
    use super::*;

    #[test]
    fn erase_deletes_the_library_and_leaves_what_is_not_ours() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        for ours in [
            "snipdeck.db",
            "snipdeck.db-wal",
            "snipdeck.db.pre-v8.bak",
            "maintenance.json",
            "maintenance.json.123-1.tmp",
        ] {
            fs::write(d.join(ours), b"x").unwrap();
        }
        for ours in [
            "blobs",
            "backups",
            "drafts",
            "snipdeck.damaged-20261107-1030",
        ] {
            fs::create_dir_all(d.join(ours)).unwrap();
            fs::write(d.join(ours).join("inner"), b"x").unwrap();
        }
        // Not ours: a user's file and folder, the settings, the logs, the lock.
        fs::write(d.join("report.docx"), b"mine").unwrap();
        fs::create_dir_all(d.join("Photos")).unwrap();
        fs::write(d.join("Photos").join("a.jpg"), b"mine").unwrap();
        fs::write(d.join("snipdeck.db-notes.txt"), b"mine").unwrap();
        fs::write(d.join("settings.json"), b"{}").unwrap();
        fs::create_dir_all(d.join("logs")).unwrap();
        fs::write(d.join("snipdeck.lock"), b"").unwrap();

        wipe_library(d).unwrap();

        let mut left: Vec<String> = fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "Photos",
                "logs",
                "report.docx",
                "settings.json",
                "snipdeck.db-notes.txt",
                "snipdeck.lock"
            ]
        );
        assert!(d.join("Photos").join("a.jpg").exists());
    }
}

/// A name for an export made on `date` that no file in `folder` has yet:
/// `Snipdeck-library-20261018.snipdeck`, then `...-2.snipdeck` and so on.
pub(super) fn export_path(folder: &Path, date: jiff::civil::Date) -> PathBuf {
    let stem = format!(
        "Snipdeck-library-{:04}{:02}{:02}",
        date.year(),
        date.month(),
        date.day()
    );
    let first = folder.join(format!("{stem}.snipdeck"));
    if !first.exists() {
        return first;
    }
    (2..10_000)
        .map(|n| folder.join(format!("{stem}-{n}.snipdeck")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

pub(super) fn danger(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Danger,
        message,
    }
}

impl<C, K, T> Controller<Store, C, K, T>
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Today's local date by the controller's clock, for the Help window's checks.
    #[must_use]
    pub fn today(&self) -> jiff::civil::Date {
        self.clock.now().to_zoned(TimeZone::system()).date()
    }

    /// Keeps backups in `folder` (blank: the default folder inside the data folder).
    pub fn set_backup_folder(&mut self, folder: &str) {
        let folder = folder.trim().to_owned();
        self.update_settings(|s| s.backup_folder = folder);
    }

    /// Sets "Backups to keep" and applies it now: copies outside the new rule are deleted in the
    /// backups folder at once, not at the next backup (DATA-17). Returns what was deleted.
    pub fn set_backups_keep(
        &mut self,
        keep: u32,
        data_dir: &std::path::Path,
    ) -> snipdeck_portable::backup::Retained {
        self.update_settings(|s| s.backups_keep = keep);
        let keep = usize::try_from(self.settings.backups_keep).unwrap_or(backup::DEFAULT_KEEP);
        backup::apply_retention(&self.settings.backup_folder(data_dir), keep, self.today())
    }

    /// Whether a tray icon is in the notification area now, for the Help window's checks.
    #[must_use]
    pub fn tray_icon_present(&self) -> bool {
        self.session.tray_available
    }

    /// Puts `text` on the clipboard for the user to keep (the diagnostic report) and says so in a
    /// toast. Returns whether it worked.
    pub fn copy_text(&mut self, text: &str) -> bool {
        let copied =
            snipdeck_platform::payload::Payload::new(vec![snipdeck_platform::payload::Data::Text(
                text.to_owned(),
            )])
            .is_some_and(|payload| self.services.clipboard.write_persistent(&payload).is_ok());
        self.show_toast(if copied {
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Copied,
            }
        } else {
            danger(ToastMessage::ClipboardBusy)
        });
        copied
    }

    /// The look in force (dark or light, high contrast), which the Settings window follows.
    #[must_use]
    pub fn settings_look(&self) -> snipdeck_platform::system_look::SystemLook {
        self.look()
    }

    /// The toast on screen, for the Settings window.
    #[must_use]
    pub fn settings_toast(&self) -> Option<super::ToastView> {
        self.toast_view()
    }

    /// Restores `backup` (a file `backup::list_backups` found) into the library: what is missing
    /// comes back, nothing is removed. The file is read on a worker (`Begun::Job`); its outcome
    /// is [`ArchiveEnd::Imported`].
    pub fn begin_restore_backup(&mut self, backup_file: &Path) -> Begun {
        let source = ImportSource::Snipdeck(backup_file.to_path_buf());
        self.begin_import(source, BoardLayout::KeepBoards, ImportPurpose::Restore)
    }

    /// "Rebuild from backup…", first half: checks what must hold before anything is touched and
    /// starts the salvage copy of what the damaged database can still give up (a temporary
    /// archive, written off the UI thread). `finish_rebuild` then sets the damaged
    /// library aside into the folder `snipdeck.damaged-<stamp>/` (database, log and blobs), opens
    /// a fresh one, restores `backup_file` into it and merges the salvage. Nothing is deleted. Any
    /// failure puts the original back and changes nothing.
    ///
    /// An encrypted library keeps its key: the fresh database gets the same key record and is
    /// opened with the key already in memory, so sensitive items survive the rebuild.
    pub fn begin_rebuild_from_backup(&mut self, data_dir: &Path, backup_file: &Path) -> Begun {
        if self.refuse_archive() {
            return Begun::Refused;
        }
        let failed = |ctl: &mut Self| {
            ctl.show_toast(danger(ToastMessage::RestoreFailed(Cause::Other)));
            Begun::Finished(ArchiveEnd::Rebuilt(false))
        };
        // A key ring that cannot be read is a corrupt one: refuse rather than lose the key.
        let Ok(record) = self.store.key_record() else {
            return failed(self);
        };
        let carry = match record {
            None => None,
            Some(record) => {
                // One copy of the key for the fresh database, one to reopen the original with.
                let (Some(for_fresh), Some(for_original)) = (
                    self.store.duplicate_data_key(),
                    self.store.duplicate_data_key(),
                ) else {
                    return failed(self);
                };
                Some(Carry {
                    record,
                    for_fresh,
                    for_original,
                })
            }
        };
        let salvage = data_dir.join("snipdeck.salvage.snipdeck");
        // The worker reads the backup before anything is touched; unreadable, the rebuild ends
        // in `finish_rebuild` without a swap.
        self.begin_rebuild_pass(Rebuild {
            data_dir: data_dir.to_path_buf(),
            backup_file: backup_file.to_path_buf(),
            salvage,
            carry,
            counter: 0,
            attempt: 0,
        })
    }

    /// Hands one pass of a rebuild (read the backup, cut the salvage copy and read it back) to a
    /// worker.
    fn begin_rebuild_pass(&mut self, mut rebuild: Rebuild) -> Begun {
        let job = self.archive_job(
            ArchiveOp::Salvage,
            Work::Rebuild {
                backup: rebuild.backup_file.clone(),
                salvage: rebuild.salvage.clone(),
                now: self.clock.now(),
            },
        );
        rebuild.counter = self.store.change_counter();
        self.begin_running(job, Context::Rebuild(Box::new(rebuild)))
    }

    /// "Rebuild from backup…", second half, on the UI thread once the worker has read the backup
    /// and cut the salvage copy (or failed to cut it: a rebuild goes on without it). Whether it
    /// worked, or [`ArchiveEnd::Again`] when the library changed meanwhile.
    pub(super) fn finish_rebuild(
        &mut self,
        rebuild: Rebuild,
        made: Result<Made, JobError>,
    ) -> ArchiveEnd {
        let Rebuild {
            data_dir,
            backup_file,
            salvage,
            carry,
            counter,
            attempt,
        } = rebuild;
        let data_dir = data_dir.as_path();
        let failed = |ctl: &mut Self| {
            let _ = fs::remove_file(&salvage);
            ctl.show_toast(danger(ToastMessage::RestoreFailed(Cause::Other)));
            ArchiveEnd::Rebuilt(false)
        };
        let prepared = match made {
            Ok(Made::Rebuilt(prepared)) => *prepared,
            Ok(_) => return failed(self),
            Err(e) => {
                crate::trace!("rebuild worker failed: {e}");
                return failed(self);
            }
        };
        // The backup must be readable before anything is touched.
        let backup = match prepared.backup {
            Ok(backup) => backup,
            Err(e) => {
                crate::trace!("the backup cannot be read: {e}");
                return failed(self);
            }
        };
        // A password step is refused while a rebuild runs; if the key record moved anyway
        // (another process, a bug), applying the old one would undo it: stop, change nothing.
        let record_now = self.store.key_record().ok();
        let unchanged = match (&carry, &record_now) {
            (None, Some(None)) => true,
            (Some(carry), Some(Some(now))) => carry.record == *now,
            _ => false,
        };
        if !unchanged {
            return failed(self);
        }
        // The library stays editable while the worker copies it. If it changed after the
        // worker's snapshot, the copy lacks what was written since: cut it again on a worker
        // (a few tries; a library that keeps changing under the copy is not rebuilt).
        if prepared.salvage.is_some() && self.store.change_counter() != counter {
            if attempt >= MAX_REBUILD_PASSES {
                return failed(self);
            }
            let again = Rebuild {
                data_dir: data_dir.to_path_buf(),
                backup_file,
                salvage: salvage.clone(),
                carry,
                counter,
                attempt: attempt + 1,
            };
            return match self.begin_rebuild_pass(again) {
                Begun::Job(job) => ArchiveEnd::Again(job),
                Begun::Finished(end) => end,
                Begun::Refused => failed(self),
            };
        }
        let salvaged = prepared.salvage.is_some();
        let sensitive_before = self.sensitive_count();
        let outcome = self.swap_in_rebuilt(data_dir, backup, prepared.salvage, carry);
        let _ = fs::remove_file(&salvage);
        ArchiveEnd::Rebuilt(self.rebuild_end(outcome, sensitive_before, salvaged))
    }

    /// The toast and the cleanup for what the swap came to. Whether it worked.
    fn rebuild_end(
        &mut self,
        outcome: Result<(usize, usize, String), Swap>,
        sensitive_before: usize,
        salvaged: bool,
    ) -> bool {
        let failed = |ctl: &mut Self| {
            ctl.show_toast(danger(ToastMessage::RestoreFailed(Cause::Other)));
            false
        };
        match outcome {
            Ok((restored, recovered, kept)) => {
                self.history = crate::history::History::new();
                // The new store counts its changes from zero: nothing read from the old one may
                // be served again (search, the panel and the Recent list).
                self.index_cache.reset();
                let missing_sensitive = sensitive_before.saturating_sub(self.sensitive_count());
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::DatabaseRebuilt {
                        restored,
                        recovered,
                        kept,
                        unreadable: !salvaged,
                        missing_sensitive,
                    },
                });
                self.refresh_or_notice("a rebuild");
                true
            }
            Err(Swap::RolledBack) => failed(self),
            Err(Swap::Unfinished) => {
                // The rebuilt library keeps running: reset what the old store served, but do
                // not report plain success. The next start finishes the cleanup.
                self.history = crate::history::History::new();
                self.index_cache.reset();
                self.refresh_or_notice("a rebuild");
                self.show_toast(danger(ToastMessage::RebuildUnfinished));
                false
            }
            Err(Swap::Stuck) => {
                // The running library is an empty one in memory and the files on disk are
                // untouched: the one safe thing to ask for is a restart.
                self.show_toast(danger(ToastMessage::RebuildNeedsRestart));
                false
            }
        }
    }

    /// How many sensitive items the open library holds (0 when it cannot be read).
    fn sensitive_count(&self) -> usize {
        self.store
            .all_items()
            .map_or(0, |items| items.iter().filter(|i| i.sensitive).count())
    }

    /// The swap itself: `Ok((restored, recovered, kept folder name))` once the fresh database
    /// holds the backup. `Err(Swap::RolledBack)` with the original library back in place and
    /// open when any step failed; `Err(Swap::Stuck)` when even reopening the original failed.
    ///
    /// The damaged library (database, log, blobs) moves into the folder `snipdeck.damaged-<stamp>/`
    /// under the folder lock, which is held from the close to the fresh open
    /// (`snipdeck_store::set_aside`); the marker it writes lets the next start undo a crash.
    fn swap_in_rebuilt(
        &mut self,
        data_dir: &Path,
        backup: Decoded<'static>,
        salvage: Option<Decoded<'static>>,
        carry: Option<Carry>,
    ) -> Result<(usize, usize, String), Swap> {
        let stamp = self
            .clock
            .now()
            .to_zoned(TimeZone::system())
            .strftime("%Y%m%d-%H%M");
        let kept = (0..100)
            .map(|n| {
                if n == 0 {
                    format!("snipdeck.damaged-{stamp}")
                } else {
                    format!("snipdeck.damaged-{stamp}-{n}")
                }
            })
            .find(|stem| !data_dir.join(stem).exists())
            .ok_or(Swap::RolledBack)?;
        let (key_record, fresh_key, original_key) = match carry {
            Some(Carry {
                record,
                for_fresh,
                for_original,
            }) => (Some(record), Some(for_fresh), Some(for_original)),
            None => (None, None, None),
        };
        // Close the store, keeping its folder lock, by putting a throwaway one in its place.
        let placeholder = Store::open_in_memory().map_err(|_| Swap::RolledBack)?;
        let closed = std::mem::replace(&mut self.store, placeholder).close();
        let Some(lock) = closed.lock else {
            return Err(Swap::RolledBack);
        };
        if snipdeck_store::set_aside(data_dir, &lock, &kept).is_err() {
            // `set_aside` has undone its own moves; if it could not, the marker is still there
            // and `reopen_original` refuses to continue.
            return Err(self.reopen_original(data_dir, lock, original_key));
        }
        // Every failure from here on hands the folder lock back for `reopen_original`.
        let mut fresh = match Store::open_locked(data_dir, lock) {
            Ok(fresh) => fresh,
            Err(failed) => return Err(self.reopen_original(data_dir, failed.lock, original_key)),
        };
        // The key goes in before anything is imported, so the backup and the salvage keep
        // their sensitive items (KeySource::Library).
        if let (Some(record), Some(key)) = (&key_record, fresh_key)
            && (fresh.restore_key_record(record).is_err() || fresh.open_with_key(key).is_err())
        {
            return Err(self.roll_back(data_dir, fresh, original_key));
        }
        self.store = fresh;
        // Applying what the worker read is the one short transaction on this thread.
        let Ok(report) = backup.apply(&mut self.store, None) else {
            let fresh = std::mem::replace(
                &mut self.store,
                Store::open_in_memory().map_err(|_| Swap::Stuck)?,
            );
            return Err(self.roll_back(data_dir, fresh, original_key));
        };
        let recovered = salvage
            .and_then(|decoded| decoded.apply(&mut self.store, None).ok())
            .map_or(0, |r| r.items_added);
        // The commit point: the marker goes. The lock is the running store's own. If the marker
        // cannot be removed the rebuilt library keeps running (the marker carries a fingerprint
        // that stops a later start from undoing edits), but success is not reported.
        let finished = self
            .store
            .folder_lock()
            .map(|lock| snipdeck_store::finish_rebuild(data_dir, lock));
        if !matches!(finished, Some(Ok(()))) {
            return Err(Swap::Unfinished);
        }
        Ok((report.items_added, recovered, kept))
    }

    /// Closes `fresh`, keeping the folder lock, and undoes the set-aside (see `reopen_original`).
    fn roll_back(&mut self, data_dir: &Path, fresh: Store, key: Option<DataKey>) -> Swap {
        match fresh.close().lock {
            Some(lock) => self.reopen_original(data_dir, lock, key),
            None => Swap::Stuck,
        }
    }

    /// Puts the set-aside library back (`snipdeck_store::undo_set_aside`) and opens it again
    /// with its key if it had one, under the lock the caller still holds.
    /// `Swap::RolledBack` when it is back; `Swap::Stuck` when it would not undo or open, in
    /// which case the running library stays the empty in-memory one and the marker makes the
    /// next start retry. (Covered by the store tests and by review: a fault cannot be injected
    /// here.)
    fn reopen_original(
        &mut self,
        data_dir: &Path,
        lock: snipdeck_store::FolderLock,
        key: Option<DataKey>,
    ) -> Swap {
        if snipdeck_store::undo_set_aside(data_dir, &lock).is_err() {
            return Swap::Stuck;
        }
        let reopened = Store::open_locked(data_dir, lock)
            .ok()
            .and_then(|mut original| {
                if let Some(key) = key {
                    original.open_with_key(key).ok()?;
                }
                Some(original)
            });
        match reopened {
            Some(original) => {
                self.store = original;
                Swap::RolledBack
            }
            None => Swap::Stuck,
        }
    }

    /// The slow half of every Settings import starts here: reading and checking the source,
    /// with no library, on a worker (`Begun::Job`; the snapshot's duplicate set comes with it).
    /// Duplicates are skipped; a `.snipdeck` lands as `layout` says (one new board for an
    /// import, its own boards for a restore); encrypted items open with the library's key.
    fn begin_import(
        &mut self,
        source: ImportSource,
        layout: BoardLayout,
        purpose: ImportPurpose,
    ) -> Begun {
        // A `.snipdeck` file's sensitive items open with the library's key; locked, they would be
        // dropped.
        let needs_key = purpose == ImportPurpose::Restore
            || (purpose == ImportPurpose::Apply && matches!(source, ImportSource::Snipdeck(_)));
        if self.refuse_while_archiving() || (needs_key && self.copy_blocked_by_lock()) {
            return Begun::Refused;
        }
        let work = Work::Decode {
            source: source.clone(),
            layout,
            now: self.clock.now(),
        };
        let job = self.archive_job(ArchiveOp::Decode, work);
        let run = ImportRun {
            source,
            purpose,
            counter: self.store.change_counter(),
        };
        self.begin_running(job, Context::Import(Box::new(run)))
    }

    /// Works out what importing `source` would do, writing nothing: the preview the page shows
    /// before the user agrees. The outcome is [`ArchiveEnd::ImportPreview`].
    pub fn begin_import_preview(&mut self, source: &ImportSource) -> Begun {
        self.begin_import(
            source.clone(),
            BoardLayout::SingleBoard,
            ImportPurpose::Preview,
        )
    }

    /// The preview of restoring a backup: like [`Self::begin_import_preview`], but the backup's
    /// boards come back under their own names.
    pub fn begin_restore_preview(&mut self, backup_file: &Path) -> Begun {
        self.begin_import(
            ImportSource::Snipdeck(backup_file.to_path_buf()),
            BoardLayout::KeepBoards,
            ImportPurpose::Preview,
        )
    }

    /// Whether the next import installs the triggers in its file (the preview lists them).
    pub fn set_install_triggers(&mut self, install: bool) {
        self.archive.install_triggers = install;
    }

    /// Imports `source` for real, as previewed. The outcome is [`ArchiveEnd::Imported`].
    pub fn begin_apply_import(&mut self, source: &ImportSource) -> Begun {
        self.begin_import(
            source.clone(),
            BoardLayout::SingleBoard,
            ImportPurpose::Apply,
        )
    }

    /// The UI-thread half of an import step: previews or applies the decoded source against the
    /// library as it is now. The snapshot's duplicate set is used only if nothing was edited
    /// since it was taken; otherwise it is read again.
    pub(super) fn finish_import(
        &mut self,
        run: &ImportRun,
        made: Result<Made, JobError>,
    ) -> ArchiveEnd {
        // `None` in the error: the worker itself failed (no portable error to name).
        let decoded = match made {
            Ok(Made::Decoded(decoded)) => (*decoded).map_err(Some),
            Ok(_) => Err(None),
            Err(e) => {
                crate::trace!("import worker failed: {e}");
                Err(None)
            }
        };
        if run.purpose != ImportPurpose::Preview && self.copy_blocked_by_lock() {
            // The library was locked while the source was being read: nothing is written.
            return ArchiveEnd::Imported(false);
        }
        let fresh = run.counter == self.store.change_counter();
        let ready = decoded.map(|ready| {
            let known = fresh.then_some(ready.known);
            (ready.decoded, known)
        });
        match run.purpose {
            ImportPurpose::Preview => {
                let preview = ready
                    .and_then(|(decoded, known)| {
                        decoded.preview(&self.store, known.as_ref()).map_err(Some)
                    })
                    .map(|report| ImportPreview {
                        boards: report.boards_added,
                        lists: report.lists_added,
                        items: report.items_added,
                        duplicates: report.duplicates_skipped,
                        tags: report.tags_added,
                        warnings: report.warnings,
                        warnings_dropped: report.warnings_dropped,
                        triggers: report.triggers,
                        triggers_total: report.triggers_total,
                    })
                    .map_err(|e| e.map_or(ImportProblem::Failed, |e| problem_of(&run.source, &e)));
                ArchiveEnd::ImportPreview(preview)
            }
            ImportPurpose::Apply | ImportPurpose::Restore => {
                let restoring = run.purpose == ImportPurpose::Restore;
                let install = restoring || self.archive.install_triggers;
                let applied = ready.and_then(|(mut decoded, known)| {
                    if !install {
                        decoded.leave_triggers();
                    }
                    decoded.apply(&mut self.store, known.as_ref()).map_err(Some)
                });
                let worked = applied.is_ok();
                match applied {
                    Ok(report) => {
                        self.show_toast(Toast {
                            kind: ToastKind::Success,
                            message: if restoring {
                                ToastMessage::BackupRestored {
                                    items: report.items_added,
                                }
                            } else {
                                ToastMessage::Imported {
                                    items: report.items_added,
                                }
                            },
                        });
                        // After the toast: a failed refresh says the list is stale, which
                        // matters more.
                        self.refresh_or_notice(if restoring { "a restore" } else { "an import" });
                    }
                    Err(e) => {
                        let cause = e.as_ref().map_or(Cause::Other, Cause::of_portable);
                        self.show_toast(danger(if restoring {
                            ToastMessage::RestoreFailed(cause)
                        } else {
                            ToastMessage::ImportFailed(cause)
                        }));
                    }
                }
                ArchiveEnd::Imported(worked)
            }
        }
    }

    /// "Erase library": closes the library, deletes it from `data_dir` (everything but the
    /// settings and the logs) and opens a fresh empty one there. With
    /// `forget_key` false the library's key record is carried over to the new library, so
    /// encrypted exports made earlier can still be opened. Returns whether everything went.
    ///
    /// The caller has already asked the user to type DELETE; nothing here asks again.
    pub fn erase_library(&mut self, data_dir: &Path, forget_key: bool) -> bool {
        // The library's files are being read by a worker: wait for it to end.
        if self.archive_busy() {
            self.show_toast(danger(ToastMessage::EraseFailed));
            return false;
        }
        let record = if forget_key {
            None
        } else if let Ok(record) = self.store.key_record() {
            record
        } else {
            self.show_toast(danger(ToastMessage::EraseFailed));
            return false;
        };
        let Ok(placeholder) = Store::open_in_memory() else {
            self.show_toast(danger(ToastMessage::EraseFailed));
            return false;
        };
        // Dropping the old store closes the database and gives up the folder's lock, which
        // Windows needs before it lets the files go.
        drop(std::mem::replace(&mut self.store, placeholder));
        let mut erased = wipe_library(data_dir).is_ok();
        match Store::open(data_dir) {
            Ok(mut fresh) => {
                if let Some(record) = &record
                    && fresh.restore_key_record(record).is_err()
                {
                    erased = false;
                }
                let _ = starter::ensure_board(&mut fresh, self.clock.now());
                self.store = fresh;
            }
            // The running library stays the empty one in memory until Snipdeck restarts.
            Err(_) => erased = false,
        }
        // Nothing of the old library may stay open: a revealed item, an editor, an Undo, the item
        // editor (and its draft), the paste queue and what was cached from the old store.
        self.revealed = None;
        self.editor = None;
        self.undo = None;
        self.item_editor_cancel();
        self.queue.reset();
        self.index_cache.reset();
        self.clear_history();
        // The key record may have gone (forgotten) or stayed (with its password). A Hello copy
        // was sealed for the old key: it goes either way, and Hello is switched off.
        self.forget_hello();
        self.refresh_master();
        self.show_toast(if erased {
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::LibraryErased,
            }
        } else {
            danger(ToastMessage::EraseFailed)
        });
        if erased {
            self.refresh_or_notice("erasing the library");
        } else {
            self.refresh_logged("erasing the library");
        }
        erased
    }
}

/// The key material an encrypted library carries over to its rebuilt database.
pub(super) struct Carry {
    pub(super) record: KeyRecord,
    pub(super) for_fresh: DataKey,
    pub(super) for_original: DataKey,
}

/// Why a rebuild did not complete.
/// How many times a rebuild cuts its salvage copy again because the library changed under it.
const MAX_REBUILD_PASSES: u8 = 3;

enum Swap {
    /// The original database is back and open.
    RolledBack,
    /// The original would not reopen either: only a restart can recover.
    Stuck,
    /// The rebuilt library is open and holds the backup, but the marker could not be removed.
    Unfinished,
}
