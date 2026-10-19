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
    ExportOptions, KeySource, SensitivePolicy,
    backup::{self, BackupOutcome},
    export_to_path,
};
use snipdeck_store::Store;

use super::{Controller, Toast, ToastKind, ToastMessage};
use crate::{host::ClipboardText, licence_store, starter, toast::LicenceProblem};

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
    /// Whether Settings shows the Pro licence.
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
        let copied = snipdeck_platform::payload::Payload::new(vec![
            snipdeck_platform::payload::Data::Text(text.to_owned()),
        ])
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

    /// Checks a pasted licence key. A good one turns Pro on at once and is saved in `data_dir`
    /// (`None` in the demo, which keeps nothing) so the next start is Pro too; a bad one changes
    /// nothing and raises a toast that says what is wrong with it.
    ///
    /// # Errors
    ///
    /// Why the key was not accepted. [`LicenceProblem::NotSaved`] means Pro is on for this run only.
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
        // Pro lifts the Free caps the windows show.
        let _ = self.refresh_panel();
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
        let now = self.clock.now();
        let key = snipdeck_portable::Library::data_key(&self.store);
        let made = match backup::run_if_due(
            &self.store,
            data_dir,
            &now.to_zoned(TimeZone::system()),
            usize::try_from(self.settings.backups_keep).unwrap_or(backup::DEFAULT_KEEP),
            key,
        ) {
            Ok(BackupOutcome::Created { .. }) => true,
            Ok(BackupOutcome::NotDue { existing }) => {
                export_to_path(&self.store, now, &self.backup_options(), &existing).is_ok()
            }
            Err(_) => false,
        };
        self.show_toast(if made {
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::BackupMade,
            }
        } else {
            danger(ToastMessage::BackupFailed)
        });
        made
    }

    /// Restores `backup` (a file `backup::list_backups` found) into the library: what is missing
    /// comes back, nothing is removed. A Free library stays within its board limit.
    pub fn restore_backup(&mut self, backup_file: &Path) -> bool {
        let max_boards = (!self.entitlements.allows(Feature::UnlimitedBoards))
            .then_some(Entitlements::FREE_BOARD_LIMIT);
        let restored = backup::restore(
            &mut self.store,
            backup_file,
            self.clock.now(),
            KeySource::Library,
            max_boards,
        );
        let worked = restored.is_ok();
        match restored {
            Ok(report) => {
                let _ = self.refresh_panel();
                self.show_toast(Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::BackupRestored {
                        items: report.items_added,
                    },
                });
            }
            Err(snipdeck_portable::Error::Core(snipdeck_core::Error::ProRequired(_))) => {
                self.show_toast(danger(ToastMessage::BoardLimit));
            }
            Err(_) => self.show_toast(danger(ToastMessage::RestoreFailed)),
        }
        worked
    }

    /// Exports the whole library as one `.snipdeck` file in `folder`, under a name that does not
    /// overwrite an earlier export. Sensitive items go in encrypted, or are left out when the
    /// library has no key.
    pub fn export_library(&mut self, folder: &Path) -> Option<PathBuf> {
        let now = self.clock.now();
        let path = export_path(folder, now.to_zoned(TimeZone::system()).date());
        let exported = fs::create_dir_all(folder).is_ok()
            && export_to_path(&self.store, now, &self.backup_options(), &path).is_ok();
        if exported {
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
            self.show_toast(danger(ToastMessage::ExportFailed));
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
        // Nothing of the old library may stay open: a revealed item, an editor, an Undo.
        self.revealed = None;
        self.editor = None;
        self.undo = None;
        let _ = self.refresh_panel();
        self.show_toast(if erased {
            Toast {
                kind: ToastKind::Success,
                message: ToastMessage::LibraryErased,
            }
        } else {
            danger(ToastMessage::EraseFailed)
        });
        erased
    }
}
