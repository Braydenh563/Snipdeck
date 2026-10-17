//! The daily local auto-backup (SPEC §5.7a): a `.snipdeck` zip in `<data folder>/backups/`,
//! once per local day, keeping the newest few.
//!
//! The functions take the clock as an argument (`now`, a zoned time in the user's time zone, so
//! "once per local day" is decided by the caller's zone) and the data folder as a path, and do
//! nothing else with the outside world, so tests need only a temporary directory.
//!
//! * **Name**: `snipdeck-backup-YYYYMMDD.snipdeck`, the local date. One file per day; if today's
//!   file exists, nothing is done.
//! * **Atomic**: written as `<name>.part` and renamed when complete, so a crash never leaves a
//!   half-written backup that looks real.
//! * **Pruning** deletes only regular files whose names match the pattern above exactly
//!   (eight digits forming a real date), plus stale `<pattern>.part` leftovers. Anything else
//!   in the folder, including folders and symbolic links with such names, is left alone, and
//!   the file just written is never pruned.
//! * **Sensitive items** are encrypted with the key given, or left out of the backup when there
//!   is none; never written in plain text. A Free-tier library has no key, so its sensitive
//!   items are not in its backups; the data folder holds them in the database itself.
//! * **Restore** is an ordinary import: [`restore`].

use std::fs;
use std::path::{Path, PathBuf};

use jiff::Zoned;
use jiff::civil::Date;
use snipdeck_core::Timestamp;
use snipdeck_store::crypto::DataKey;

use crate::error::{Error, Result};
use crate::export::{ExportOptions, ExportReport, SensitivePolicy, export_to_path};
use crate::import::{BoardLayout, ImportOptions, KeySource, MergeStrategy, import_archive_path};
use crate::plan::Library;
use crate::report::ImportReport;

/// How many backups to keep by default (SPEC §5.7a).
pub const DEFAULT_KEEP: usize = 7;

/// The folder inside the data folder that holds the backups.
const DIR: &str = "backups";
const PREFIX: &str = "snipdeck-backup-";
const EXTENSION: &str = ".snipdeck";
const PART: &str = ".part";

/// What [`run_if_due`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupOutcome {
    /// Today's backup already exists; nothing was written or deleted.
    NotDue {
        /// Today's backup.
        existing: PathBuf,
    },
    /// A backup was written.
    Created {
        /// The new file.
        path: PathBuf,
        /// What it contains.
        report: ExportReport,
        /// Older backups (and stale partial files) that were deleted.
        pruned: Vec<PathBuf>,
        /// Files that matched but could not be deleted; the backup itself succeeded.
        prune_failed: Vec<PathBuf>,
    },
}

/// A backup file found by [`list_backups`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupFile {
    /// The local date in the file name.
    pub date: Date,
    /// The file.
    pub path: PathBuf,
    /// Its size in bytes.
    pub bytes: u64,
}

/// The backups folder inside a data folder.
pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

/// The file name for a day's backup, `snipdeck-backup-YYYYMMDD.snipdeck`.
pub fn file_name(date: Date) -> String {
    format!(
        "{PREFIX}{:04}{:02}{:02}{EXTENSION}",
        date.year(),
        date.month(),
        date.day()
    )
}

/// The date in a backup file name, or `None` if `name` is not exactly such a name.
pub fn parse_file_name(name: &str) -> Option<Date> {
    let digits = name.strip_prefix(PREFIX)?.strip_suffix(EXTENSION)?;
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i16 = digits[..4].parse().ok()?;
    let month: i8 = digits[4..6].parse().ok()?;
    let day: i8 = digits[6..].parse().ok()?;
    Date::new(year, month, day).ok()
}

/// Writes today's backup unless it exists, then prunes old ones.
///
/// "Today" is the date of `now` in its own time zone. `keep` is how many backups to retain,
/// counting the new one (at least 1; [`DEFAULT_KEEP`] is the product default). `key` encrypts
/// sensitive items; without it they are left out.
///
/// # Errors
///
/// [`Error::Io`]/[`Error::File`] if the folder or file cannot be written, and any export error.
/// A failed backup leaves no partial file. Pruning problems are not errors; see
/// [`BackupOutcome::Created::prune_failed`](BackupOutcome).
pub fn run_if_due<L: Library>(
    lib: &L,
    data_dir: &Path,
    now: &Zoned,
    keep: usize,
    key: Option<&DataKey>,
) -> Result<BackupOutcome> {
    let dir = backups_dir(data_dir);
    let name = file_name(now.date());
    let path = dir.join(&name);
    if path.exists() {
        return Ok(BackupOutcome::NotDue { existing: path });
    }
    fs::create_dir_all(&dir).map_err(|source| Error::File {
        path: dir.clone(),
        source,
    })?;

    let options = ExportOptions {
        sensitive: key.map_or(SensitivePolicy::Skip, SensitivePolicy::Encrypt),
        ..ExportOptions::default()
    };
    let report = export_to_path(lib, now.timestamp(), &options, &path)?;

    let (pruned, prune_failed) = prune(&dir, &name, keep.max(1));
    Ok(BackupOutcome::Created {
        path,
        report,
        pruned,
        prune_failed,
    })
}

/// The backups in `data_dir`, newest first. A missing folder is an empty list.
///
/// # Errors
///
/// [`Error::File`] if the folder exists but cannot be read.
pub fn list_backups(data_dir: &Path) -> Result<Vec<BackupFile>> {
    let dir = backups_dir(data_dir);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(Error::File { path: dir, source }),
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        if let Some(file) = as_backup(&entry) {
            found.push(file);
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse(f.date));
    Ok(found)
}

/// Restores a backup: an import that recreates the backup's boards under their own names, with
/// new ids, skipping items whose content is already in the library (so restoring over an intact
/// library adds nothing). Pass a `key` to restore encrypted sensitive items;
/// [`KeySource::Library`] uses the library's own unlocked key.
///
/// # Errors
///
/// As [`import_archive_path`].
pub fn restore<L: Library>(
    lib: &mut L,
    backup: &Path,
    now: Timestamp,
    key: KeySource<'_>,
) -> Result<ImportReport> {
    let options = ImportOptions {
        layout: BoardLayout::KeepBoards,
        merge: MergeStrategy::SkipDuplicates,
        key,
        ..ImportOptions::default()
    };
    import_archive_path(lib, backup, now, &options)
}

/// A directory entry as a backup file, if (and only if) it is a regular file with an exact
/// backup name. `DirEntry::file_type` does not follow symbolic links, so a link is never one.
fn as_backup(entry: &fs::DirEntry) -> Option<BackupFile> {
    let name = entry.file_name().into_string().ok()?;
    let date = parse_file_name(&name)?;
    let meta = entry
        .metadata()
        .ok()
        .filter(|m| entry.file_type().is_ok_and(|t| t.is_file()) && m.is_file())?;
    Some(BackupFile {
        date,
        path: entry.path(),
        bytes: meta.len(),
    })
}

/// Whether `name` is `<backup name>.part`.
fn is_stale_part(name: &str) -> bool {
    name.strip_suffix(PART)
        .is_some_and(|base| parse_file_name(base).is_some())
}

/// Deletes backups beyond the newest `keep` (the file just written, `current`, counts and is
/// never deleted) and stale partial files. Returns what was deleted and what could not be.
fn prune(dir: &Path, current: &str, keep: usize) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut others = Vec::new();
    let mut stale = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name == current {
            continue;
        }
        if let Some(file) = as_backup(&entry) {
            others.push(file);
        } else if is_stale_part(&name) && entry.file_type().is_ok_and(|t| t.is_file()) {
            stale.push(entry.path());
        }
    }
    others.sort_by_key(|f| std::cmp::Reverse(f.date));
    let doomed = others
        .into_iter()
        .skip(keep.saturating_sub(1))
        .map(|f| f.path)
        .chain(stale);

    let (mut pruned, mut failed) = (Vec::new(), Vec::new());
    for path in doomed {
        match fs::remove_file(&path) {
            Ok(()) => pruned.push(path),
            Err(_) => failed.push(path),
        }
    }
    (pruned, failed)
}
