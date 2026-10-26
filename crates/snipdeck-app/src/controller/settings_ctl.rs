//! The Settings window's half of the [`Controller`]: the licence key, the backups, the export
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
use snipdeck_core::{Entitlements, Feature, Tier, licence::verify_embedded};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_portable::{
    BoardLayout, ExportOptions, HistoryEntry, ImportOptions, ImportReport, KeySource,
    MergeStrategy, SensitivePolicy,
    backup::{self, BackupOutcome},
    export_to_path, import_archive_path, import_copyq_json, import_ditto, import_entries,
    import_folder,
};
use snipdeck_store::{
    ItemRepo, Store,
    crypto::{DataKey, KeyRecord},
};

use super::{Controller, Toast, ToastKind, ToastMessage};
use crate::{failure::Cause, host::ClipboardText, licence_store, starter, toast::LicenceProblem};

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
            Self::Snipdeck(p) | Self::Ditto(p) | Self::CopyQ(p) | Self::Folder(p) => Some(p),
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

    /// Runs the importer for this source over `store`. With `options.dry_run` nothing is kept.
    fn run(
        &self,
        store: &mut Store,
        now: jiff::Timestamp,
        options: &ImportOptions<'_>,
    ) -> snipdeck_portable::Result<ImportReport> {
        match self {
            Self::Snipdeck(p) => import_archive_path(store, p, now, options),
            Self::Ditto(p) => import_ditto(store, p, now, options),
            Self::CopyQ(p) => import_copyq_json(store, p, now, options),
            Self::Folder(p) => import_folder(store, p, now, options),
            Self::ClipboardHistory { entries, board } => {
                import_entries(store, entries, board, now, options)
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
    /// The Free plan's board limit would be passed, so the import is refused until Pro.
    pub blocked_by_board_limit: bool,
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

fn is_board_limit(error: &snipdeck_portable::Error) -> bool {
    matches!(
        error,
        snipdeck_portable::Error::Core(snipdeck_core::Error::ProRequired(_))
    )
}

/// Files and folders of the data folder that "Erase library" leaves: the settings (and the copies
/// of a damaged file set aside), the licence key, the crash logs and the lock the running app
/// holds. Everything else is the library.
fn kept_by_erase(name: &str) -> bool {
    name.starts_with("settings")
        || name == licence_store::FILE
        || name == "logs"
        || name == "snipdeck.lock"
}

/// Deletes everything in `folder` that is the library. Goes on past a file it cannot delete and
/// reports the first failure at the end.
fn wipe_library(folder: &Path) -> io::Result<()> {
    let mut first_error = None;
    for entry in fs::read_dir(folder)?.flatten() {
        if entry.file_name().to_str().is_some_and(kept_by_erase) {
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

/// A name for an export made on `date` that no file in `folder` has yet:
/// `Snipdeck-library-20261018.snipdeck`, then `...-2.snipdeck` and so on.
fn export_path(folder: &Path, date: jiff::civil::Date) -> PathBuf {
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

fn danger(message: ToastMessage) -> Toast {
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
    /// Whether Settings shows the Supporter licence.
    #[must_use]
    pub fn tier(&self) -> Tier {
        self.entitlements.tier()
    }

    /// Today's local date by the controller's clock, for the Help window's checks.
    #[must_use]
    pub fn today(&self) -> jiff::civil::Date {
        self.clock.now().to_zoned(TimeZone::system()).date()
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

    /// Checks a pasted licence key. A good one turns Supporter on at once and is saved in `data_dir`
    /// (`None` in the demo, which keeps nothing) so the next start is Supporter too; a bad one changes
    /// nothing and raises a toast that says what is wrong with it.
    ///
    /// # Errors
    ///
    /// Why the key was not accepted. [`LicenceProblem::NotSaved`] means Supporter is on for this run only.
    pub fn apply_licence(
        &mut self,
        token: &str,
        data_dir: Option<&Path>,
    ) -> Result<(), LicenceProblem> {
        let token = token.trim();
        let result = if token.is_empty() {
            Err(LicenceProblem::Empty)
        } else {
            verify_embedded(token, self.clock.now())
                .map_err(licence_store::problem_of)
                .and_then(|entitlements| {
                    self.entitlements = entitlements;
                    match data_dir {
                        Some(dir) => {
                            licence_store::write(dir, token).map_err(|_| LicenceProblem::NotSaved)
                        }
                        None => Ok(()),
                    }
                })
        };
        match result {
            Ok(()) => self.show_toast(Toast {
                kind: ToastKind::Success,
                message: ToastMessage::LicenceAccepted,
            }),
            Err(problem) => self.show_toast(danger(ToastMessage::LicenceRejected(problem))),
        }
        // Supporter lifts the Free caps the windows show. The real error stays on screen.
        if result.is_ok() {
            self.refresh_or_notice("a licence change");
        } else {
            self.refresh_logged("a licence change");
        }
        result
    }

    fn backup_options(&self) -> ExportOptions<'_> {
        // Encrypted with the library's key when it has one and it is unlocked; without a key the
        // export leaves sensitive items out, so a backup never holds one in plain text.
        ExportOptions {
            sensitive: snipdeck_portable::Library::data_key(&self.store)
                .map_or(SensitivePolicy::Skip, SensitivePolicy::Encrypt),
            ..ExportOptions::default()
        }
    }

    /// "Back up now": today's backup in `data_dir`, replacing the one already made today (the
    /// daily run writes only when there is none, and the user asked for a fresh copy).
    /// Returns whether it worked.
    pub fn backup_now(&mut self, data_dir: &Path) -> bool {
        // A copy of a locked library would lack its sensitive items and replace today's whole one.
        if self.copy_blocked_by_lock() {
            return false;
        }
        let now = self.clock.now();
        let key = snipdeck_portable::Library::data_key(&self.store);
        let made = match backup::run_if_due(
            &self.store,
            data_dir,
            &now.to_zoned(TimeZone::system()),
            usize::try_from(self.settings.backups_keep).unwrap_or(backup::DEFAULT_KEEP),
            key,
        ) {
            Ok(BackupOutcome::Created { .. }) => Ok(()),
            Ok(BackupOutcome::NotDue { existing }) => {
                export_to_path(&self.store, now, &self.backup_options(), &existing).map(|_| ())
            }
            Err(e) => Err(e),
        };
        let worked = made.is_ok();
        self.show_toast(match made {
            Ok(()) => Toast {
                kind: ToastKind::Success,
                message: ToastMessage::BackupMade,
            },
            Err(e) => danger(ToastMessage::BackupFailed(Cause::of_portable(&e))),
        });
        worked
    }

    /// Restores `backup` (a file `backup::list_backups` found) into the library: what is missing
    /// comes back, nothing is removed. A Free library stays within its board limit.
    pub fn restore_backup(&mut self, backup_file: &Path) -> bool {
        // Without the key the backup's sensitive items cannot be read, and they would be dropped.
        if self.copy_blocked_by_lock() {
            return false;
        }
        let source = ImportSource::Snipdeck(backup_file.to_path_buf());
        // The shared import path: the board limit is enforced inside its transaction.
        let restored = self.import_report(&source, BoardLayout::KeepBoards, false);
        let worked = restored.is_ok();
        match restored {
            Ok(report) => {
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::BackupRestored {
                        items: report.items_added,
                    },
                });
                // After the toast: a failed refresh says the list is stale, which matters more.
                self.refresh_or_notice("a restore");
            }
            Err(snipdeck_portable::Error::Core(snipdeck_core::Error::ProRequired(_))) => {
                self.show_toast(danger(ToastMessage::BoardLimit));
            }
            Err(e) => self.show_toast(danger(ToastMessage::RestoreFailed(Cause::of_portable(&e)))),
        }
        worked
    }

    /// "Rebuild from backup…": sets the damaged database aside as `snipdeck.damaged-<stamp>.db`
    /// (with its `-wal` and `-shm`), opens a fresh one, restores `backup_file` into it and then
    /// merges what the old database could still be read for. Nothing is deleted. Any failure
    /// puts the original back and changes nothing. Returns whether it worked.
    ///
    /// An encrypted library keeps its key: the fresh database gets the same key record and is
    /// opened with the key already in memory, so sensitive items survive the rebuild.
    pub fn rebuild_from_backup(&mut self, data_dir: &Path, backup_file: &Path) -> bool {
        if self.copy_blocked_by_lock() {
            return false;
        }
        let failed = |ctl: &mut Self| {
            ctl.show_toast(danger(ToastMessage::RestoreFailed(Cause::Other)));
            false
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
        // The backup must be readable before anything is touched.
        if self.preview_restore(backup_file).is_err() {
            return failed(self);
        }
        // Best effort: what the damaged database can still give up, as a temporary archive.
        let now = self.clock.now();
        let salvage = data_dir.join("snipdeck.salvage.snipdeck");
        let salvaged = export_to_path(&self.store, now, &self.backup_options(), &salvage).is_ok();
        let sensitive_before = self.sensitive_count();
        let outcome =
            self.swap_in_rebuilt(data_dir, backup_file, salvaged.then_some(&salvage), carry);
        let _ = fs::remove_file(&salvage);
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

    /// The swap itself: `Ok((restored, recovered, kept file name))` once the fresh database
    /// holds the backup. `Err(Swap::RolledBack)` with the original database back in place and
    /// open when any step failed; `Err(Swap::Stuck)` when even reopening the original failed.
    fn swap_in_rebuilt(
        &mut self,
        data_dir: &Path,
        backup_file: &Path,
        salvage: Option<&Path>,
        carry: Option<Carry>,
    ) -> Result<(usize, usize, String), Swap> {
        let stamp = self
            .clock
            .now()
            .to_zoned(TimeZone::system())
            .strftime("%Y%m%d-%H%M");
        let damaged = (0..100)
            .map(|n| {
                if n == 0 {
                    format!("snipdeck.damaged-{stamp}")
                } else {
                    format!("snipdeck.damaged-{stamp}-{n}")
                }
            })
            .find(|stem| !data_dir.join(format!("{stem}.db")).exists())
            .ok_or(Swap::RolledBack)?;
        let kept = format!("{damaged}.db");
        let (key_record, fresh_key, original_key) = match carry {
            Some(Carry {
                record,
                for_fresh,
                for_original,
            }) => (Some(record), Some(for_fresh), Some(for_original)),
            None => (None, None, None),
        };
        // Close the store (its lock and connection) by putting a throwaway one in its place.
        let placeholder = Store::open_in_memory().map_err(|_| Swap::RolledBack)?;
        drop(std::mem::replace(&mut self.store, placeholder));
        if !move_db_files(data_dir, "snipdeck.db", &kept) {
            return Err(self.reopen_original(data_dir, original_key));
        }
        let rebuilt = Store::open(data_dir).ok().and_then(|mut fresh| {
            // The key goes in before anything is imported, so the backup and the salvage keep
            // their sensitive items (KeySource::Library).
            if let (Some(record), Some(key)) = (&key_record, fresh_key) {
                fresh.restore_key_record(record).ok()?;
                fresh.open_with_key(key).ok()?;
            }
            self.store = fresh;
            self.import_report(
                &ImportSource::Snipdeck(backup_file.to_path_buf()),
                BoardLayout::KeepBoards,
                false,
            )
            .ok()
        });
        let Some(report) = rebuilt else {
            // Back out: drop the fresh database and put the original back.
            if let Ok(throwaway) = Store::open_in_memory() {
                drop(std::mem::replace(&mut self.store, throwaway));
            }
            for suffix in ["", "-wal", "-shm"] {
                let _ = fs::remove_file(data_dir.join(format!("snipdeck.db{suffix}")));
            }
            if !move_db_files(data_dir, &kept, "snipdeck.db") {
                return Err(Swap::Stuck);
            }
            return Err(self.reopen_original(data_dir, original_key));
        };
        let recovered = salvage
            .and_then(|file| {
                self.import_report(
                    &ImportSource::Snipdeck(file.to_path_buf()),
                    BoardLayout::KeepBoards,
                    false,
                )
                .ok()
            })
            .map_or(0, |r| r.items_added);
        Ok((report.items_added, recovered, kept))
    }

    /// Opens the original database again after a failed swap, with its key if it had one.
    /// `Swap::RolledBack` when it is back; `Swap::Stuck` when it would not open, in which case
    /// the running library stays the empty in-memory one until Snipdeck restarts. (Opening a
    /// database the swap itself just closed cannot be made to fail in a test without a fault
    /// injected into `Store::open`, so this path is covered by review only.)
    fn reopen_original(&mut self, data_dir: &Path, key: Option<DataKey>) -> Swap {
        // A missing file means the original could not be moved back: `Store::open` would make
        // an empty library in its place, so stay stuck rather than install that one.
        if !data_dir.join("snipdeck.db").exists() {
            return Swap::Stuck;
        }
        let reopened = Store::open(data_dir).ok().and_then(|mut original| {
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

    /// The importer options every Settings import uses: duplicates are skipped, a `.snipdeck`
    /// lands as `layout` says (one new board for an import, its own boards for a restore), encrypted items open with the library's own key, and a Free library
    /// stays within its board limit.
    fn import_report(
        &mut self,
        source: &ImportSource,
        layout: BoardLayout,
        dry_run: bool,
    ) -> snipdeck_portable::Result<ImportReport> {
        let max_boards = (!self.entitlements.allows(Feature::UnlimitedBoards))
            .then_some(Entitlements::FREE_BOARD_LIMIT);
        let options = ImportOptions {
            merge: MergeStrategy::SkipDuplicates,
            layout,
            key: KeySource::Library,
            max_boards,
            dry_run,
            ..ImportOptions::default()
        };
        source.run(&mut self.store, self.clock.now(), &options)
    }

    /// Works out what importing `source` would do, writing nothing: the preview the page shows
    /// before the user agrees. Under the Free plan's board limit the counts are still those of
    /// the file, and `blocked_by_board_limit` says the import would be refused.
    ///
    /// # Errors
    ///
    /// Why the source cannot be read.
    pub fn preview_import(
        &mut self,
        source: &ImportSource,
    ) -> Result<ImportPreview, ImportProblem> {
        self.preview_as(source, BoardLayout::SingleBoard)
    }

    /// The preview of restoring a backup: like [`Self::preview_import`], but the backup's boards
    /// come back under their own names.
    ///
    /// # Errors
    ///
    /// Why the backup cannot be read.
    pub fn preview_restore(&mut self, backup_file: &Path) -> Result<ImportPreview, ImportProblem> {
        let source = ImportSource::Snipdeck(backup_file.to_path_buf());
        self.preview_as(&source, BoardLayout::KeepBoards)
    }

    fn preview_as(
        &mut self,
        source: &ImportSource,
        layout: BoardLayout,
    ) -> Result<ImportPreview, ImportProblem> {
        let (report, blocked) = match self.import_report(source, layout, true) {
            Ok(report) => (report, false),
            Err(e) if is_board_limit(&e) => {
                // Refused for the limit: read it again without the limit for the counts.
                let options = ImportOptions {
                    merge: MergeStrategy::SkipDuplicates,
                    layout,
                    key: KeySource::Library,
                    dry_run: true,
                    ..ImportOptions::default()
                };
                let report = source
                    .run(&mut self.store, self.clock.now(), &options)
                    .map_err(|e| problem_of(source, &e))?;
                (report, true)
            }
            Err(e) => return Err(problem_of(source, &e)),
        };
        Ok(ImportPreview {
            boards: report.boards_added,
            lists: report.lists_added,
            items: report.items_added,
            duplicates: report.duplicates_skipped,
            tags: report.tags_added,
            warnings: report.warnings,
            warnings_dropped: report.warnings_dropped,
            blocked_by_board_limit: blocked,
        })
    }

    /// Imports `source` for real, as previewed, and says how many items came in. Returns
    /// whether it worked.
    pub fn apply_import(&mut self, source: &ImportSource) -> bool {
        // A `.snipdeck` file's sensitive items open with the library's key; locked, they would be
        // dropped.
        if matches!(source, ImportSource::Snipdeck(_)) && self.copy_blocked_by_lock() {
            return false;
        }
        match self.import_report(source, BoardLayout::SingleBoard, false) {
            Ok(report) => {
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::Imported {
                        items: report.items_added,
                    },
                });
                self.refresh_or_notice("an import");
                true
            }
            Err(e) if is_board_limit(&e) => {
                self.show_toast(danger(ToastMessage::BoardLimit));
                false
            }
            Err(e) => {
                self.show_toast(danger(ToastMessage::ImportFailed(Cause::of_portable(&e))));
                false
            }
        }
    }

    /// Exports the whole library as one `.snipdeck` file in `folder`, under a name that does not
    /// overwrite an earlier export. Sensitive items go in encrypted, or are left out when the
    /// library has no key.
    pub fn export_library(&mut self, folder: &Path) -> Option<PathBuf> {
        if self.copy_blocked_by_lock() {
            return None;
        }
        let now = self.clock.now();
        let path = export_path(folder, now.to_zoned(TimeZone::system()).date());
        let exported = fs::create_dir_all(folder)
            .map_err(|e| Cause::of_io(&e))
            .and_then(|()| {
                export_to_path(&self.store, now, &self.backup_options(), &path)
                    .map_err(|e| Cause::of_portable(&e))
            });
        if exported.is_ok() {
            let file = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.show_toast(Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Exported { file },
            });
            Some(path)
        } else {
            let cause = exported.err().unwrap_or(Cause::Other);
            self.show_toast(danger(ToastMessage::ExportFailed(cause)));
            None
        }
    }

    /// "Erase library": closes the library, deletes it from `data_dir` (everything but the
    /// settings, the licence key and the logs) and opens a fresh empty one there. With
    /// `forget_key` false the library's key record is carried over to the new library, so
    /// encrypted exports made earlier can still be opened. Returns whether everything went.
    ///
    /// The caller has already asked the user to type DELETE; nothing here asks again.
    pub fn erase_library(&mut self, data_dir: &Path, forget_key: bool) -> bool {
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
        // The key record may have gone (forgotten) or stayed (with its password).
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

/// Renames `from` and its `-wal` and `-shm` in `dir` to `to` (and the same suffixes). All or
/// nothing: on a failure what was moved is moved back and `false` is returned.
/// The key material an encrypted library carries over to its rebuilt database.
struct Carry {
    record: KeyRecord,
    for_fresh: DataKey,
    for_original: DataKey,
}

/// Why a rebuild did not complete.
enum Swap {
    /// The original database is back and open.
    RolledBack,
    /// The original would not reopen either: only a restart can recover.
    Stuck,
}

fn move_db_files(dir: &Path, from: &str, to: &str) -> bool {
    let mut done: Vec<&str> = Vec::new();
    for suffix in ["", "-wal", "-shm"] {
        let source = dir.join(format!("{from}{suffix}"));
        if !source.exists() {
            continue;
        }
        if fs::rename(&source, dir.join(format!("{to}{suffix}"))).is_err() {
            for undo in done {
                let _ = fs::rename(
                    dir.join(format!("{to}{undo}")),
                    dir.join(format!("{from}{undo}")),
                );
            }
            return false;
        }
        done.push(suffix);
    }
    true
}
