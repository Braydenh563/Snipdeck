//! The Store build's data rule, as the app keeps it (`docs/packaging.md`, "Where the Store build
//! keeps your data"; PLATFORM-33 leftover).
//!
//! Windows keeps a packaged app's `AppData` writes in the package's own folder and deletes that
//! folder when the app is uninstalled. So in the Store build:
//!
//! - the first time Snipdeck starts with a window, a toast says that uninstalling removes the
//!   library and where to back it up ([`reminder_due`]);
//! - "Back up now" also saves a `.snipdeck` copy in `Documents\Snipdeck backups`
//!   ([`copy_folder`]), outside the package, so a copy survives an uninstall.
//!
//! Neither applies to the Setup or portable install, whose data is in a normal folder.
//! Portable decisions, tested on every OS; **unverified on Windows** for a real Store build.

use std::path::{Path, PathBuf};

/// The folder under Documents that holds the copies of a Store build's backups.
pub const BACKUP_FOLDER_NAME: &str = "Snipdeck backups";

/// Where "Back up now" also saves a copy: `Documents\Snipdeck backups` in a packaged build, none
/// elsewhere (or when the machine has no Documents folder).
#[must_use]
pub fn copy_folder(packaged: bool, documents: Option<&Path>) -> Option<PathBuf> {
    packaged
        .then_some(documents)
        .flatten()
        .map(|documents| documents.join(BACKUP_FOLDER_NAME))
}

/// Whether the one-time "uninstalling removes your library" reminder is due: a packaged build
/// that has not shown it yet.
#[must_use]
pub fn reminder_due(packaged: bool, already_shown: bool) -> bool {
    packaged && !already_shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packaged_build_copies_backups_into_documents() {
        assert_eq!(
            copy_folder(true, Some(Path::new("/home/ann/Documents"))),
            Some(Path::new("/home/ann/Documents").join("Snipdeck backups"))
        );
    }

    #[test]
    fn other_builds_and_machines_without_documents_make_no_copy() {
        assert_eq!(copy_folder(false, Some(Path::new("/d"))), None);
        assert_eq!(copy_folder(true, None), None);
    }

    #[test]
    fn the_reminder_is_for_the_packaged_build_once() {
        assert!(reminder_due(true, false));
        assert!(!reminder_due(true, true));
        assert!(!reminder_due(false, false));
    }
}
