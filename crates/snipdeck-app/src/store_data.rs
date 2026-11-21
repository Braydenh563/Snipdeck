//! The Store build's data rule, as the app keeps it (`docs/packaging.md`, "Where the Store build
//! keeps your data"; PLATFORM-33 leftover).
//!
//! Windows keeps a packaged app's `AppData` writes in the package's own folder and deletes that
//! folder when the app is uninstalled. So in the Store build:
//!
//! - the first time Snipdeck starts with a window, a toast says that uninstalling removes the
//!   library and where to back it up ([`reminder_due`]);
//! - every backup that was written (the daily one and "Back up now") is also copied into
//!   `Documents\Snipdeck backups` ([`copy_folder`], [`copy_out`]), outside the package, so a copy
//!   survives an uninstall. A failed backup copies nothing, and the copies are pruned to the same
//!   number as the backups, so the folder does not grow for ever. The copy is silent: the
//!   backup's own toast is the only one.
//!
//! Neither applies to the Setup or portable install, whose data is in a normal folder.
//! Portable decisions, tested on every OS; **unverified on Windows** for a real Store build.

use std::{
    io,
    path::{Path, PathBuf},
};

use jiff::civil::Date;
use snipdeck_portable::backup;

/// The folder under Documents that holds the copies of a Store build's backups.
pub const BACKUP_FOLDER_NAME: &str = "Snipdeck backups";

/// Where backups are also copied: `Documents\Snipdeck backups` in a packaged build, none
/// elsewhere (or when the machine has no Documents folder).
#[must_use]
pub fn copy_folder(packaged: bool, documents: Option<&Path>) -> Option<PathBuf> {
    packaged
        .then_some(documents)
        .flatten()
        .map(|documents| documents.join(BACKUP_FOLDER_NAME))
}

/// The file operations [`copy_out`] needs, so a test can use a fake folder.
pub trait CopyFs {
    /// Creates `dir` and its parents.
    ///
    /// # Errors
    ///
    /// The operating system's.
    fn create_dir_all(&self, dir: &Path) -> io::Result<()>;
    /// Copies the file `from` to `to`, replacing `to`.
    ///
    /// # Errors
    ///
    /// The operating system's.
    fn copy(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Moves `from` over `to`.
    ///
    /// # Errors
    ///
    /// The operating system's.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// The names of the files directly in `dir` (none when it does not exist).
    ///
    /// # Errors
    ///
    /// The operating system's.
    fn file_names(&self, dir: &Path) -> io::Result<Vec<String>>;
    /// Deletes a file.
    ///
    /// # Errors
    ///
    /// The operating system's.
    fn remove(&self, file: &Path) -> io::Result<()>;
}

/// The real file system.
pub struct RealFs;

impl CopyFs for RealFs {
    fn create_dir_all(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::copy(from, to).map(|_| ())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    fn file_names(&self, dir: &Path) -> io::Result<Vec<String>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect())
    }

    fn remove(&self, file: &Path) -> io::Result<()> {
        std::fs::remove_file(file)
    }
}

/// What [`copy_out`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyReport {
    /// The copy that was made.
    pub copied: PathBuf,
    /// Older copies that were deleted.
    pub pruned: usize,
    /// Older copies that should have gone and could not be deleted.
    pub prune_failed: usize,
}

/// Why a backup was not copied out.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// The file is not named like one of Snipdeck's backups (daily or manual).
    #[error("{0} is not a backup file")]
    NotABackup(PathBuf),
    /// The folder or the copy could not be written.
    #[error("could not copy the backup: {0}")]
    Io(#[from] io::Error),
}

/// The day and the (hour, minute) of a manual backup.
type When = (Date, backup::TimeOfDay);

/// Copies the backup `file` (just written, so it exists) into `folder` under the same name, then
/// keeps only the newest `keep` copies there (at least 1). `today` is the day the backup was made:
/// a copy dated after it (a clock that was once wrong) is neither ranked nor deleted, and other
/// files in the folder are left alone. A failure to prune is counted, not an error: the copy is
/// made.
///
/// # Errors
///
/// [`CopyError`] when `file` is not a backup or the copy could not be written; a failed copy
/// leaves no partial file under the backup's name.
pub fn copy_out(
    fs: &dyn CopyFs,
    file: &Path,
    folder: &Path,
    keep: usize,
    today: Date,
) -> Result<CopyReport, CopyError> {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| backup::parse_backup_name(n).is_some())
        .ok_or_else(|| CopyError::NotABackup(file.to_path_buf()))?;
    fs.create_dir_all(folder)?;
    let target = folder.join(name);
    let part = folder.join(format!("{name}.part"));
    if let Err(e) = fs
        .copy(file, &part)
        .and_then(|()| fs.rename(&part, &target))
    {
        let _ = fs.remove(&part);
        return Err(e.into());
    }

    // Daily copies keep `keep`, manual ones the newest `MANUAL_KEPT`, as in the backups folder.
    let mut dated: Vec<(Date, String)> = Vec::new();
    let mut manual: Vec<(When, String)> = Vec::new();
    for found in fs.file_names(folder)? {
        match backup::parse_backup_name(&found) {
            Some((date, None)) if date <= today => dated.push((date, found)),
            Some((date, Some(time))) if date <= today => manual.push(((date, time), found)),
            _ => {}
        }
    }
    dated.sort_by_key(|(date, _)| std::cmp::Reverse(*date));
    manual.sort_by_key(|(when, _)| std::cmp::Reverse(*when));
    let doomed = dated
        .into_iter()
        .skip(keep.max(1))
        .map(|(_, found)| found)
        .chain(
            manual
                .into_iter()
                .skip(backup::MANUAL_KEPT)
                .map(|(_, found)| found),
        );
    let (mut pruned, mut prune_failed) = (0, 0);
    for found in doomed {
        if found == name {
            continue;
        }
        match fs.remove(&folder.join(found)) {
            Ok(()) => pruned += 1,
            Err(_) => prune_failed += 1,
        }
    }
    Ok(CopyReport {
        copied: target,
        pruned,
        prune_failed,
    })
}

/// Whether the one-time "uninstalling removes your library" reminder is due: a packaged build
/// that has not shown it yet.
#[must_use]
pub fn reminder_due(packaged: bool, already_shown: bool) -> bool {
    packaged && !already_shown
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap};

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

    /// A folder in memory: path to contents. `fail_copy` and `stuck` break one operation.
    #[derive(Default)]
    struct FakeFs {
        files: RefCell<BTreeMap<PathBuf, String>>,
        fail_copy: bool,
        stuck: Vec<PathBuf>,
    }

    const DOCS: &str = "/docs/Snipdeck backups";
    const TODAY: &str = "/data/backups/snipdeck-backup-20261116.snipdeck";

    impl FakeFs {
        /// `names` in the Documents folder, and today's backup in the data folder.
        fn with(names: &[&str]) -> Self {
            let fs = Self::default();
            for n in names {
                fs.files
                    .borrow_mut()
                    .insert(Path::new(DOCS).join(n), "old".into());
            }
            fs.files
                .borrow_mut()
                .insert(PathBuf::from(TODAY), "new".into());
            fs
        }

        fn names(&self) -> Vec<String> {
            let mut names = self.file_names(Path::new(DOCS)).unwrap();
            names.sort();
            names
        }
    }

    impl CopyFs for FakeFs {
        fn create_dir_all(&self, _dir: &Path) -> io::Result<()> {
            Ok(())
        }

        fn copy(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.fail_copy {
                return Err(io::Error::other("disk full"));
            }
            let text = self
                .files
                .borrow()
                .get(from)
                .cloned()
                .ok_or(io::ErrorKind::NotFound)?;
            self.files.borrow_mut().insert(to.to_path_buf(), text);
            Ok(())
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            let text = self
                .files
                .borrow_mut()
                .remove(from)
                .ok_or(io::ErrorKind::NotFound)?;
            self.files.borrow_mut().insert(to.to_path_buf(), text);
            Ok(())
        }

        fn file_names(&self, dir: &Path) -> io::Result<Vec<String>> {
            Ok(self
                .files
                .borrow()
                .keys()
                .filter(|p| p.parent() == Some(dir))
                .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
                .collect())
        }

        fn remove(&self, file: &Path) -> io::Result<()> {
            if self.stuck.iter().any(|s| s == file) {
                return Err(io::Error::other("in use"));
            }
            self.files.borrow_mut().remove(file);
            Ok(())
        }
    }

    /// The fixed "today" of these tests.
    fn day() -> Date {
        Date::new(2026, 11, 16).unwrap()
    }

    #[test]
    fn a_backup_is_copied_and_the_copies_are_pruned_to_the_same_count() {
        let fs = FakeFs::with(&[
            "snipdeck-backup-20261110.snipdeck",
            "snipdeck-backup-20261111.snipdeck",
            "snipdeck-backup-20261112.snipdeck",
            "notes.txt",
        ]);
        let report = copy_out(&fs, Path::new(TODAY), Path::new(DOCS), 2, day()).unwrap();
        assert_eq!(report.pruned, 2);
        assert_eq!(
            report.copied,
            Path::new(DOCS).join("snipdeck-backup-20261116.snipdeck")
        );
        assert_eq!(
            fs.names(),
            [
                "notes.txt",
                "snipdeck-backup-20261112.snipdeck",
                "snipdeck-backup-20261116.snipdeck"
            ],
            "the two newest stay; a file that is not a backup is not touched"
        );
        assert_eq!(fs.files.borrow()[&report.copied], "new");
    }

    #[test]
    fn a_manual_backup_is_copied_and_pruned_apart_from_the_daily_ones() {
        let manual = "/data/backups/snipdeck-backup-20261116-093000.snipdeck";
        let mut names = vec!["snipdeck-backup-20261115.snipdeck".to_owned()];
        // Ten older manual copies: with the new one that is one over the newest ten.
        for day in 6..16 {
            names.push(format!("snipdeck-backup-202611{day:02}-080000.snipdeck"));
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let fs = FakeFs::with(&refs);
        fs.files
            .borrow_mut()
            .insert(PathBuf::from(manual), "manual".into());
        let report = copy_out(&fs, Path::new(manual), Path::new(DOCS), 1, day()).unwrap();
        assert_eq!(report.pruned, 1, "only the oldest manual copy goes");
        let left = fs.names();
        assert!(left.contains(&"snipdeck-backup-20261115.snipdeck".to_owned()));
        assert!(left.contains(&"snipdeck-backup-20261116-093000.snipdeck".to_owned()));
        assert!(!left.contains(&"snipdeck-backup-20261106-080000.snipdeck".to_owned()));
        assert_eq!(left.len(), 11);
    }

    #[test]
    fn a_failed_copy_leaves_nothing_and_prunes_nothing() {
        let mut fs = FakeFs::with(&["snipdeck-backup-20261101.snipdeck"]);
        fs.fail_copy = true;
        let err = copy_out(&fs, Path::new(TODAY), Path::new(DOCS), 1, day()).unwrap_err();
        assert!(matches!(err, CopyError::Io(_)));
        assert_eq!(fs.names(), ["snipdeck-backup-20261101.snipdeck"]);
    }

    #[test]
    fn a_file_that_is_not_a_backup_is_refused() {
        let fs = FakeFs::default();
        let file = Path::new("/x/export.snipdeck");
        let err = copy_out(&fs, file, Path::new(DOCS), 7, day());
        assert!(matches!(err, Err(CopyError::NotABackup(_))));
    }

    #[test]
    fn a_copy_that_cannot_be_pruned_is_counted_not_an_error() {
        let mut fs = FakeFs::with(&["snipdeck-backup-20261101.snipdeck"]);
        fs.stuck
            .push(Path::new(DOCS).join("snipdeck-backup-20261101.snipdeck"));
        let report = copy_out(&fs, Path::new(TODAY), Path::new(DOCS), 1, day()).unwrap();
        assert_eq!((report.pruned, report.prune_failed), (0, 1));
    }

    #[test]
    fn a_copy_dated_after_today_holds_no_slot_and_is_kept() {
        let fs = FakeFs::with(&[
            "snipdeck-backup-20270101.snipdeck",
            "snipdeck-backup-20261101.snipdeck",
        ]);
        let report = copy_out(&fs, Path::new(TODAY), Path::new(DOCS), 1, day()).unwrap();
        assert_eq!(report.pruned, 1);
        assert_eq!(
            fs.names(),
            [
                "snipdeck-backup-20261116.snipdeck",
                "snipdeck-backup-20270101.snipdeck"
            ]
        );
    }

    #[test]
    fn a_second_copy_the_same_day_replaces_the_first() {
        let fs = FakeFs::with(&["snipdeck-backup-20261116.snipdeck"]);
        copy_out(&fs, Path::new(TODAY), Path::new(DOCS), 7, day()).unwrap();
        assert_eq!(fs.names().len(), 1);
        let kept = Path::new(DOCS).join("snipdeck-backup-20261116.snipdeck");
        assert_eq!(fs.files.borrow()[&kept], "new");
    }
}
