//! Backups (SPEC §5.7a): `.snipdeck` zips in a backups folder, one a day, plus manual ones.
//!
//! The functions take the clock as an argument (`now`, a zoned time in the user's time zone, so
//! "once per local day" is decided by the caller's zone) and the backups *folder* as a path
//! (by default `<data folder>/backups`, [`backups_dir`]; the user may choose another), and do
//! nothing else with the outside world, so tests need only a temporary directory.
//!
//! * **Names**: the daily file is `snipdeck-backup-YYYYMMDD.snipdeck`, the local date. A manual
//!   backup is its own file, `snipdeck-backup-YYYYMMDD-HHMMSS.snipdeck`; it never replaces a
//!   daily one and does not count against the daily rotation.
//! * **Atomic**: written as `<name>.part` and renamed when complete, so a crash never leaves a
//!   half-written backup that looks real.
//! * **Read back**: every new backup is opened again and every asset is hashed before it
//!   counts; a file that cannot be read is removed and the error returned. Today's file also
//!   has to open (counts and manifest agree) to count as "already done": an empty or cut-off
//!   file with today's name is written again.
//! * **Keeping**: the newest `keep` daily files, the newest file of each of the last
//!   [`WEEKS_KEPT`] full weeks, the newest of each of the last [`MONTHS_KEPT`] full months, and
//!   the newest [`MANUAL_KEPT`] manual files. A backup that holds under half the items of the
//!   previous one (or none) prunes nothing that day ([`Shrunk`]).
//! * **Pruning** deletes only regular files whose names match the patterns above exactly
//!   (digits forming a real date), plus stale `<pattern>.part` leftovers. Anything else in the
//!   folder, including folders and symbolic links with such names, is left alone, and the file
//!   just written is never pruned.
//! * **Sensitive items** are encrypted with the key given, or left out of the backup when there
//!   is none; never written in plain text.
//! * **Restore** is an ordinary import ([`crate::import_archive_path`] with `BoardLayout::KeepBoards`).

use std::fs;
use std::path::{Path, PathBuf};

use jiff::Zoned;
use jiff::civil::Date;
use snipdeck_store::crypto::DataKey;

use crate::check::{ArchiveSummary, Depth, check_archive_path};
use crate::error::{Error, Result};
use crate::export::{ExportOptions, ExportReport, SensitivePolicy, export_to_path};
use crate::plan::Library;

/// How many daily backups to keep by default (SPEC §5.7a).
pub const DEFAULT_KEEP: usize = 7;
/// Full weeks before the current one of which the newest backup is kept.
pub const WEEKS_KEPT: usize = 4;
/// Full months before the current one of which the newest backup is kept.
pub const MONTHS_KEPT: usize = 6;
/// Manual backups kept.
pub const MANUAL_KEPT: usize = 10;

/// The default backups folder inside a data folder.
const DIR: &str = "backups";
const PREFIX: &str = "snipdeck-backup-";
const EXTENSION: &str = ".snipdeck";
const PART: &str = ".part";

/// A backup that holds far fewer items than the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shrunk {
    /// Items in the previous backup.
    pub was: usize,
    /// Items in this one.
    pub now: usize,
}

/// What [`run_if_due`] and [`run_manual`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "one value per backup run; boxing would only add a hop"
)]
pub enum BackupOutcome {
    /// Today's backup already exists and can be read; nothing was written or deleted.
    NotDue {
        /// Today's backup.
        existing: PathBuf,
    },
    /// A backup was written and read back.
    Created {
        /// The new file.
        path: PathBuf,
        /// What was written.
        report: ExportReport,
        /// What reading the file back found.
        checked: ArchiveSummary,
        /// Older backups (and stale partial files) that were deleted.
        pruned: Vec<PathBuf>,
        /// Files that matched but could not be deleted; the backup itself succeeded.
        prune_failed: Vec<PathBuf>,
        /// Set when this backup holds under half the items of the previous one; nothing was
        /// pruned then, so the older, fuller copies stay.
        suspect: Option<Shrunk>,
    },
}

/// A manual backup's local time in its file name: (hour, minute, second).
pub type TimeOfDay = (i8, i8, i8);

/// A backup file found by [`list_backups`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupFile {
    /// The local date in the file name.
    pub date: Date,
    /// The local time (hour, minute, second) of a manual backup; `None` for a daily one.
    pub time: Option<TimeOfDay>,
    /// Whether it was made by "Back up now".
    pub manual: bool,
    /// The file.
    pub path: PathBuf,
    /// Its size in bytes.
    pub bytes: u64,
}

/// What [`apply_retention`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Retained {
    /// Deleted files.
    pub removed: Vec<PathBuf>,
    /// Files that should go but could not be deleted.
    pub failed: Vec<PathBuf>,
}

/// The default backups folder of a data folder.
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

/// The file name of a manual backup, `snipdeck-backup-YYYYMMDD-HHMMSS.snipdeck`.
pub fn manual_file_name(now: &Zoned) -> String {
    format!(
        "{PREFIX}{:04}{:02}{:02}-{:02}{:02}{:02}{EXTENSION}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// The date in a daily backup file name, or `None` if `name` is not exactly such a name.
pub fn parse_file_name(name: &str) -> Option<Date> {
    match parse_any(name)? {
        (date, None) => Some(date),
        (_, Some(_)) => None,
    }
}

/// The date, and for a manual backup its time (hour, minute, second), in a backup file name: `None` if
/// `name` is neither a daily nor a manual backup's name.
pub fn parse_backup_name(name: &str) -> Option<(Date, Option<TimeOfDay>)> {
    parse_any(name)
}

/// The date, and for a manual backup the time, in a backup file name.
fn parse_any(name: &str) -> Option<(Date, Option<TimeOfDay>)> {
    let stem = name.strip_prefix(PREFIX)?.strip_suffix(EXTENSION)?;
    let (digits, time) = match stem.split_once('-') {
        Some((d, t)) => (d, Some(t)),
        None => (stem, None),
    };
    if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i16 = digits[..4].parse().ok()?;
    let month: i8 = digits[4..6].parse().ok()?;
    let day: i8 = digits[6..].parse().ok()?;
    let date = Date::new(year, month, day).ok()?;
    let time = match time {
        None => None,
        Some(t) => {
            if t.len() != 6 || !t.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let hour: i8 = t[..2].parse().ok()?;
            let minute: i8 = t[2..4].parse().ok()?;
            let second: i8 = t[4..].parse().ok()?;
            if hour > 23 || minute > 59 || second > 59 {
                return None;
            }
            Some((hour, minute, second))
        }
    };
    Some((date, time))
}

/// Writes today's backup unless a readable one exists, then prunes old ones.
///
/// "Today" is the date of `now` in its own time zone. `keep` is how many daily backups to
/// retain, counting the new one (at least 1; [`DEFAULT_KEEP`] is the product default). `key`
/// encrypts sensitive items; without it they are left out. An item whose attachment cannot be
/// read is left out and named in the report; everything else is written.
///
/// # Errors
///
/// [`Error::Io`]/[`Error::File`] if the folder or file cannot be written, any export error,
/// and the read-back error if the file just written cannot be read (it is removed). A failed
/// backup leaves no partial file. Pruning problems are not errors; see
/// [`BackupOutcome::Created::prune_failed`](BackupOutcome).
pub fn run_if_due<L: Library>(
    lib: &L,
    folder: &Path,
    now: &Zoned,
    keep: usize,
    key: Option<&DataKey>,
) -> Result<BackupOutcome> {
    let name = file_name(now.date());
    let path = folder.join(&name);
    if path.exists() && check_archive_path(&path, Depth::Counts).is_ok() {
        return Ok(BackupOutcome::NotDue { existing: path });
    }
    let previous = newest_summary(folder, Some(&name));
    let (report, checked) = write_checked(lib, folder, now, key, &path)?;
    let suspect = previous.and_then(|was| shrunk(was, checked.items));
    let (pruned, prune_failed) = if suspect.is_some() {
        (Vec::new(), Vec::new())
    } else {
        let kept = apply_retention_except(folder, keep.max(1), now.date(), Some(&name));
        (kept.removed, kept.failed)
    };
    Ok(BackupOutcome::Created {
        path,
        report,
        checked,
        pruned,
        prune_failed,
        suspect,
    })
}

/// Writes a manual backup, `snipdeck-backup-YYYYMMDD-HHMMSS.snipdeck`: a file of its own that
/// never replaces a daily backup and does not count against the daily rotation. Only the
/// newest [`MANUAL_KEPT`] manual backups are kept.
///
/// # Errors
///
/// As [`run_if_due`].
pub fn run_manual<L: Library>(
    lib: &L,
    folder: &Path,
    now: &Zoned,
    key: Option<&DataKey>,
) -> Result<BackupOutcome> {
    let name = manual_file_name(now);
    let path = folder.join(&name);
    let (report, checked) = write_checked(lib, folder, now, key, &path)?;
    let kept = apply_retention_except(folder, usize::MAX, now.date(), Some(&name));
    Ok(BackupOutcome::Created {
        path,
        report,
        checked,
        pruned: kept.removed,
        prune_failed: kept.failed,
        suspect: None,
    })
}

/// Exports to `path` and reads the file back; a file that does not read back is removed.
fn write_checked<L: Library>(
    lib: &L,
    folder: &Path,
    now: &Zoned,
    key: Option<&DataKey>,
    path: &Path,
) -> Result<(ExportReport, ArchiveSummary)> {
    fs::create_dir_all(folder).map_err(|source| Error::File {
        path: folder.to_owned(),
        source,
    })?;
    let options = ExportOptions {
        sensitive: key.map_or(SensitivePolicy::Skip, SensitivePolicy::Encrypt),
        every_tag: true,
        ..ExportOptions::default()
    };
    let report = export_to_path(lib, now.timestamp(), &options, path)?;
    match check_archive_path(path, Depth::Everything) {
        Ok(checked) => Ok((report, checked)),
        Err(error) => {
            let _ = fs::remove_file(path);
            Err(error)
        }
    }
}

/// Whether `now` items after `was` items is a loss big enough to keep the older copies.
fn shrunk(was: usize, now: usize) -> Option<Shrunk> {
    (was >= 4 && now * 2 < was).then_some(Shrunk { was, now })
}

/// What the newest readable backup other than `except` holds, by item count.
fn newest_summary(folder: &Path, except: Option<&str>) -> Option<usize> {
    list_backups(folder)
        .ok()?
        .into_iter()
        .filter(|f| except.is_none_or(|e| f.path.file_name().is_none_or(|n| n != e)))
        .find_map(|f| check_archive_path(&f.path, Depth::Counts).ok())
        .map(|s| s.items)
}

/// The newest readable backup, for a "last backup" line: the file and what it holds.
///
/// # Errors
///
/// [`Error::File`] if the folder cannot be read.
pub fn newest_readable(folder: &Path) -> Result<Option<(BackupFile, ArchiveSummary)>> {
    for file in list_backups(folder)? {
        if let Ok(summary) = check_archive_path(&file.path, Depth::Counts) {
            return Ok(Some((file, summary)));
        }
    }
    Ok(None)
}

/// The backups in `folder`, newest first. A missing folder is an empty list.
///
/// # Errors
///
/// [`Error::File`] if the folder exists but cannot be read.
pub fn list_backups(folder: &Path) -> Result<Vec<BackupFile>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(Error::File {
                path: folder.to_owned(),
                source,
            });
        }
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        if let Some(file) = as_backup(&entry) {
            found.push(file);
        }
    }
    found.sort_by_key(|f| std::cmp::Reverse((f.date, f.time)));
    Ok(found)
}

/// A directory entry as a backup file, if (and only if) it is a regular file with an exact
/// backup name. `DirEntry::file_type` does not follow symbolic links, so a link is never one.
fn as_backup(entry: &fs::DirEntry) -> Option<BackupFile> {
    let name = entry.file_name().into_string().ok()?;
    let (date, time) = parse_any(&name)?;
    let meta = entry
        .metadata()
        .ok()
        .filter(|m| entry.file_type().is_ok_and(|t| t.is_file()) && m.is_file())?;
    Some(BackupFile {
        date,
        time,
        manual: time.is_some(),
        path: entry.path(),
        bytes: meta.len(),
    })
}

/// Whether `name` is `<backup name>.part`.
fn is_stale_part(name: &str) -> bool {
    name.strip_suffix(PART)
        .is_some_and(|base| parse_any(base).is_some())
}

/// Applies the keeping rules to `folder` now (the "Backups to keep" setting changed, or a
/// backup was just made): deletes what falls outside them and stale partial files.
pub fn apply_retention(folder: &Path, keep: usize, today: Date) -> Retained {
    apply_retention_except(folder, keep, today, None)
}

/// The keeping rules; `current` (the file just written) is never deleted. A daily file dated
/// after `today` (a clock that was once wrong, or a copied file) is not ranked: it would
/// otherwise hold a slot for ever. It is not deleted either.
fn apply_retention_except(
    folder: &Path,
    keep: usize,
    today: Date,
    current: Option<&str>,
) -> Retained {
    let Ok(entries) = fs::read_dir(folder) else {
        return Retained::default();
    };
    let mut daily = Vec::new();
    let mut manual = Vec::new();
    let mut stale = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if current == Some(name.as_str()) {
            continue;
        }
        if let Some(file) = as_backup(&entry) {
            if file.manual {
                manual.push(file);
            } else if file.date <= today {
                daily.push(file);
            }
        } else if is_stale_part(&name) && entry.file_type().is_ok_and(|t| t.is_file()) {
            stale.push(entry.path());
        }
    }
    daily.sort_by_key(|f| std::cmp::Reverse(f.date));
    manual.sort_by_key(|f| std::cmp::Reverse((f.date, f.time)));

    // The file just written is one of the daily slots (when it is a daily file).
    let current_is_daily = current.is_some_and(|c| parse_file_name(c).is_some());
    let daily_slots = keep.saturating_sub(usize::from(current_is_daily));
    let mut doomed: Vec<PathBuf> = Vec::new();
    for (index, file) in daily.iter().enumerate() {
        if index < daily_slots || is_tier_pick(&daily, file, today, current) {
            continue;
        }
        doomed.push(file.path.clone());
    }
    let manual_slots =
        MANUAL_KEPT.saturating_sub(usize::from(current.is_some() && !current_is_daily));
    doomed.extend(manual.iter().skip(manual_slots).map(|f| f.path.clone()));
    doomed.extend(stale);

    let mut out = Retained::default();
    for path in doomed {
        match fs::remove_file(&path) {
            Ok(()) => out.removed.push(path),
            Err(_) => out.failed.push(path),
        }
    }
    out
}

/// Whether `file` is the newest daily backup of one of the last [`WEEKS_KEPT`] full weeks or
/// [`MONTHS_KEPT`] full months before `today` (counting the file just written as present).
fn is_tier_pick(
    daily: &[BackupFile],
    file: &BackupFile,
    today: Date,
    current: Option<&str>,
) -> bool {
    let current_date = current.and_then(parse_file_name);
    let newest_in = |same: &dyn Fn(Date) -> bool| {
        daily
            .iter()
            .map(|f| f.date)
            .chain(current_date)
            .filter(|d| same(*d))
            .max()
    };
    let week_of = |d: Date| {
        let w = d.iso_week_date();
        (w.year(), w.week())
    };
    let this_week = today.iso_week_date();
    for back in 1..=WEEKS_KEPT {
        let Some(monday) = today
            .checked_sub(
                jiff::Span::new().days(i64::from(this_week.weekday().to_monday_zero_offset())),
            )
            .ok()
            .and_then(|m| {
                m.checked_sub(jiff::Span::new().weeks(i64::try_from(back).unwrap_or(i64::MAX)))
                    .ok()
            })
        else {
            continue;
        };
        let target = week_of(monday);
        if newest_in(&|d| week_of(d) == target) == Some(file.date) {
            return true;
        }
    }
    for back in 1..=MONTHS_KEPT {
        let Some(first) = today
            .first_of_month()
            .checked_sub(jiff::Span::new().months(i64::try_from(back).unwrap_or(i64::MAX)))
            .ok()
        else {
            continue;
        };
        let target = (first.year(), first.month());
        if newest_in(&|d| (d.year(), d.month()) == target) == Some(file.date) {
            return true;
        }
    }
    false
}
