//! The Troubleshoot checks of the Help window: is the data folder writable, does the database
//! pass its integrity check, are there recent backups, is the encryption key there, are the
//! shortcuts set, is the tray icon up, did the app crash lately.
//!
//! [`run`] is pure: it asks a [`DoctorHost`] for facts and turns them into [`Check`]s, so tests
//! run every check passing and failing against a fake. The app's host is
//! `help_binding::WorkerDoctor`. A check carries its facts ([`Detail`]); the window
//! words them through `DoctorTexts` in the language shown, and only the report is English.
//!
//! [`report`] renders the diagnostic report the user can copy: versions, system, setting flags and
//! the check results. It never holds item content, and a path inside the data folder is replaced
//! by `<data folder>` (nothing outside it is ever asked for).

use std::fmt::Write as _;

use jiff::civil::Date;

use crate::{settings::HotkeyAction, settings_binding::action_name};

/// Less free space than this on the data folder's drive is a warning.
pub const LOW_SPACE_BYTES: u64 = 200 * 1024 * 1024;
/// A newest backup older than this many days is a warning.
pub const STALE_BACKUP_DAYS: i64 = 3;

/// The labels of the checks, in the order they are shown.
pub const LABELS: [&str; 7] = [
    "Data folder",
    "Database",
    "Backups",
    "Encryption key",
    "Shortcuts",
    "Tray icon",
    "Crash reports",
];

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Fine.
    Ok,
    /// Works, but worth a look.
    Warn,
    /// Broken.
    Fail,
}

impl Status {
    /// The word the report and the screen reader use.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "Warning",
            Self::Fail => "Problem",
        }
    }
}

/// What a button next to a check does about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    /// Writes today's backup.
    BackUpNow,
    /// Opens the Settings page.
    OpenSettings,
    /// Opens the folder with the crash reports.
    OpenLogsFolder,
    /// Sets the damaged database aside and restores the newest backup (after a confirmation).
    RebuildFromBackup,
    /// Opens the data folder.
    OpenDataFolder,
}

impl Fix {
    /// The button's number across the Slint boundary (0 is "no fix").
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::BackUpNow => 1,
            Self::OpenSettings => 2,
            Self::OpenLogsFolder => 3,
            Self::RebuildFromBackup => 4,
            Self::OpenDataFolder => 5,
        }
    }

    /// The inverse of [`Fix::code`].
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::BackUpNow),
            2 => Some(Self::OpenSettings),
            3 => Some(Self::OpenLogsFolder),
            4 => Some(Self::RebuildFromBackup),
            5 => Some(Self::OpenDataFolder),
            _ => None,
        }
    }
}

/// One line of the Troubleshoot list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What was checked (one of [`LABELS`]).
    pub label: &'static str,
    /// How it came out.
    pub status: Status,
    /// What was found. [`Detail`]'s `Display` is the English sentence (for the report); the
    /// window words it in the language shown.
    pub detail: Detail,
    /// What can be done about it, if anything.
    pub fix: Option<Fix>,
}

/// The facts the checks need. Each call may touch the disk; [`run`] calls each once.
pub trait DoctorHost {
    /// Today's local date.
    fn today(&self) -> Date;
    /// Whether a file can be written in the data folder.
    ///
    /// # Errors
    ///
    /// Why not.
    fn data_dir_writable(&self) -> Result<(), String>;
    /// Free bytes on the data folder's drive, if known.
    fn free_bytes(&self) -> Option<u64>;
    /// The integrity check's findings, one line per problem (empty when healthy).
    ///
    /// # Errors
    ///
    /// The check could not run.
    fn integrity(&self) -> Result<Vec<String>, String>;
    /// The dates of the backups that exist.
    ///
    /// # Errors
    ///
    /// The backups folder could not be read.
    fn backup_dates(&self) -> Result<Vec<Date>, String>;
    /// How many items are marked sensitive.
    ///
    /// # Errors
    ///
    /// The library could not be read.
    fn sensitive_items(&self) -> Result<usize, String>;
    /// Whether the library has an encryption key ring.
    ///
    /// # Errors
    ///
    /// The library could not be read.
    fn key_ring_exists(&self) -> Result<bool, String>;
    /// The names of the global shortcuts that are not set.
    fn shortcuts_unset(&self) -> Vec<HotkeyAction>;
    /// How many global shortcuts there are in all.
    fn shortcuts_total(&self) -> usize;
    /// Whether the tray icon is in the notification area.
    fn tray_present(&self) -> bool;
    /// The file name of the newest crash report, if any.
    fn newest_crash(&self) -> Option<String>;
}

fn check(label: &'static str, status: Status, detail: Detail, fix: Option<Fix>) -> Check {
    Check {
        label,
        status,
        detail,
        fix,
    }
}

/// What one check found: a kind and its facts, so the window can word it in any language
/// (`DoctorTexts` in Slint) and the report keeps one English sentence (`Display`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// The data folder cannot be written (why).
    DataNotWritable(String),
    /// Writable, but few megabytes are free.
    LowSpace(u64),
    /// Writable with room to spare.
    Writable,
    /// The integrity check could not run (why).
    IntegrityFailed(String),
    /// No problem found.
    IntegrityOk,
    /// This many problems, the first one worded.
    IntegrityProblems(usize, String),
    /// The backups folder cannot be read (why).
    BackupsUnreadable(String),
    /// No backup yet.
    NoBackup,
    /// The newest backup is old: its date, its age in days, how many are kept.
    BackupStale(String, i64, usize),
    /// The newest backup's date and how many are kept.
    BackupFresh(String, usize),
    /// The items cannot be read (why).
    ItemsUnreadable(String),
    /// No item is sensitive.
    NoSensitive,
    /// This many sensitive items, the key is there.
    KeyPresent(usize),
    /// This many sensitive items are plain text (no key ring yet).
    PlainSensitive(usize),
    /// The key ring cannot be checked (why).
    KeyUnchecked(String),
    /// Every global shortcut is set.
    ShortcutsAllSet,
    /// None is set.
    ShortcutsNoneSet,
    /// These are not set.
    ShortcutsUnset(Vec<HotkeyAction>),
    /// The tray icon is there.
    TrayPresent,
    /// The tray icon is missing.
    TrayMissing,
    /// No crash report.
    NoCrash,
    /// The newest crash report's file name.
    NewestCrash(String),
    /// Not asked (the command line has no tray).
    NotChecked,
}

impl Detail {
    /// The number of the kind across the Slint boundary (`DoctorTexts.detail`).
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::DataNotWritable(_) => 0,
            Self::LowSpace(_) => 1,
            Self::Writable => 2,
            Self::IntegrityFailed(_) => 3,
            Self::IntegrityOk => 4,
            Self::IntegrityProblems(..) => 5,
            Self::BackupsUnreadable(_) => 6,
            Self::NoBackup => 7,
            Self::BackupStale(..) => 8,
            Self::BackupFresh(..) => 9,
            Self::ItemsUnreadable(_) => 10,
            Self::NoSensitive => 11,
            Self::KeyPresent(_) => 12,
            Self::PlainSensitive(_) => 13,
            Self::KeyUnchecked(_) => 14,
            Self::ShortcutsAllSet => 15,
            Self::ShortcutsNoneSet => 16,
            Self::ShortcutsUnset(_) => 17,
            Self::TrayPresent => 18,
            Self::TrayMissing => 19,
            Self::NoCrash => 20,
            Self::NewestCrash(_) => 21,
            Self::NotChecked => 22,
        }
    }

    /// The facts as up to three strings (`a`, `b`, `c` of `DoctorTexts.detail`); `names` words
    /// the unset shortcuts (the window's language, or English for the report).
    #[must_use]
    pub fn args(&self, names: impl Fn(HotkeyAction) -> String) -> [String; 3] {
        let none = String::new;
        match self {
            Self::DataNotWritable(a)
            | Self::IntegrityFailed(a)
            | Self::BackupsUnreadable(a)
            | Self::ItemsUnreadable(a)
            | Self::KeyUnchecked(a)
            | Self::NewestCrash(a) => [a.clone(), none(), none()],
            Self::LowSpace(n) => [n.to_string(), none(), none()],
            Self::KeyPresent(n) | Self::PlainSensitive(n) => [n.to_string(), none(), none()],
            Self::IntegrityProblems(n, first) => [n.to_string(), first.clone(), none()],
            Self::BackupStale(date, age, kept) => [date.clone(), age.to_string(), kept.to_string()],
            Self::BackupFresh(date, kept) => [date.clone(), kept.to_string(), none()],
            Self::ShortcutsUnset(unset) => [
                unset
                    .iter()
                    .map(|a| names(*a))
                    .collect::<Vec<_>>()
                    .join(", "),
                none(),
                none(),
            ],
            _ => [none(), none(), none()],
        }
    }
}

impl std::fmt::Display for Detail {
    /// The English sentence, as the report prints it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DataNotWritable(e) => write!(f, "Snipdeck cannot write to its data folder: {e}"),
            Self::LowSpace(mb) => {
                write!(f, "Writable, but only {mb} MB are free on its drive.")
            }
            Self::Writable => f.write_str("Writable, with room to spare."),
            Self::IntegrityFailed(e) => write!(f, "The integrity check could not run: {e}"),
            Self::IntegrityOk => f.write_str("The integrity check found no problems."),
            Self::IntegrityProblems(n, first) => write!(
                f,
                "The integrity check found {n} problem(s); the first: {first}"
            ),
            Self::BackupsUnreadable(e) => write!(f, "The backups cannot be read: {e}"),
            Self::NoBackup => f.write_str("No backup has been made yet."),
            Self::BackupStale(date, age, kept) => write!(
                f,
                "The newest backup is from {date}, {age} days ago ({kept} kept)."
            ),
            Self::BackupFresh(date, kept) => write!(f, "Newest backup {date}, {kept} kept."),
            Self::ItemsUnreadable(e) => write!(f, "The items cannot be read: {e}"),
            Self::NoSensitive => f.write_str("No item is marked sensitive."),
            Self::KeyPresent(n) => write!(f, "{n} sensitive item(s), the key is present."),
            Self::PlainSensitive(n) => write!(
                f,
                "{n} sensitive item(s) are stored as plain text (no encryption key yet): \
                 masked and readable, but not encrypted."
            ),
            Self::KeyUnchecked(e) => write!(f, "The key cannot be checked: {e}"),
            Self::ShortcutsAllSet => f.write_str("All global shortcuts are set."),
            Self::ShortcutsNoneSet => f.write_str("No global shortcut is set."),
            Self::ShortcutsUnset(_) => {
                write!(
                    f,
                    "Not set: {}.",
                    self.args(|a| action_name(a).to_owned())[0]
                )
            }
            Self::TrayPresent => f.write_str("The icon is in the notification area."),
            Self::TrayMissing => {
                f.write_str("No icon in the notification area; closing the panel quits Snipdeck.")
            }
            Self::NoCrash => f.write_str("No crash report."),
            Self::NewestCrash(name) => write!(f, "The newest crash report is {name}."),
            Self::NotChecked => f.write_str("Not checked from the command line."),
        }
    }
}

/// Runs every check, in the order of [`LABELS`].
#[must_use]
pub fn run(host: &dyn DoctorHost) -> Vec<Check> {
    vec![
        data_folder(host),
        database(host),
        backups(host),
        encryption(host),
        shortcuts(host),
        tray(host),
        crashes(host),
    ]
}

/// The worst status among `checks` (`Ok` for none).
#[must_use]
pub fn worst(checks: &[Check]) -> Status {
    checks
        .iter()
        .fold(Status::Ok, |worst, c| match (worst, c.status) {
            (Status::Fail, _) | (_, Status::Fail) => Status::Fail,
            (Status::Warn, _) | (_, Status::Warn) => Status::Warn,
            _ => Status::Ok,
        })
}

fn data_folder(host: &dyn DoctorHost) -> Check {
    let label = LABELS[0];
    if let Err(e) = host.data_dir_writable() {
        return check(
            label,
            Status::Fail,
            Detail::DataNotWritable(e),
            Some(Fix::OpenDataFolder),
        );
    }
    match host.free_bytes() {
        Some(free) if free < LOW_SPACE_BYTES => check(
            label,
            Status::Warn,
            Detail::LowSpace(free / (1024 * 1024)),
            Some(Fix::OpenDataFolder),
        ),
        _ => check(label, Status::Ok, Detail::Writable, None),
    }
}

fn database(host: &dyn DoctorHost) -> Check {
    let label = LABELS[1];
    // A damaged database can be rebuilt from the newest backup; with none, the folder is the way out.
    let fix = if host.backup_dates().is_ok_and(|dates| !dates.is_empty()) {
        Fix::RebuildFromBackup
    } else {
        Fix::OpenDataFolder
    };
    match host.integrity() {
        Err(e) => check(label, Status::Fail, Detail::IntegrityFailed(e), Some(fix)),
        Ok(problems) if problems.is_empty() => check(label, Status::Ok, Detail::IntegrityOk, None),
        Ok(problems) => check(
            label,
            Status::Fail,
            Detail::IntegrityProblems(problems.len(), problems[0].clone()),
            Some(fix),
        ),
    }
}

fn backups(host: &dyn DoctorHost) -> Check {
    let label = LABELS[2];
    let dates = match host.backup_dates() {
        Ok(dates) => dates,
        Err(e) => {
            return check(
                label,
                Status::Warn,
                Detail::BackupsUnreadable(e),
                Some(Fix::BackUpNow),
            );
        }
    };
    let Some(newest) = dates.iter().max().copied() else {
        return check(label, Status::Warn, Detail::NoBackup, Some(Fix::BackUpNow));
    };
    let age = host
        .today()
        .since(newest)
        .map_or(0, |span| i64::from(span.get_days()));
    if age > STALE_BACKUP_DAYS {
        check(
            label,
            Status::Warn,
            Detail::BackupStale(newest.to_string(), age, dates.len()),
            Some(Fix::BackUpNow),
        )
    } else {
        check(
            label,
            Status::Ok,
            Detail::BackupFresh(newest.to_string(), dates.len()),
            None,
        )
    }
}

fn encryption(host: &dyn DoctorHost) -> Check {
    let label = LABELS[3];
    let sensitive = match host.sensitive_items() {
        Ok(n) => n,
        Err(e) => return check(label, Status::Warn, Detail::ItemsUnreadable(e), None),
    };
    if sensitive == 0 {
        return check(label, Status::Ok, Detail::NoSensitive, None);
    }
    match host.key_ring_exists() {
        Ok(true) => check(label, Status::Ok, Detail::KeyPresent(sensitive), None),
        Ok(false) => check(label, Status::Warn, Detail::PlainSensitive(sensitive), None),
        Err(e) => check(label, Status::Warn, Detail::KeyUnchecked(e), None),
    }
}

fn shortcuts(host: &dyn DoctorHost) -> Check {
    let label = LABELS[4];
    let unset = host.shortcuts_unset();
    if unset.is_empty() {
        check(label, Status::Ok, Detail::ShortcutsAllSet, None)
    } else if unset.len() >= host.shortcuts_total() {
        check(
            label,
            Status::Warn,
            Detail::ShortcutsNoneSet,
            Some(Fix::OpenSettings),
        )
    } else {
        check(
            label,
            Status::Warn,
            Detail::ShortcutsUnset(unset),
            Some(Fix::OpenSettings),
        )
    }
}

fn tray(host: &dyn DoctorHost) -> Check {
    if host.tray_present() {
        check(LABELS[5], Status::Ok, Detail::TrayPresent, None)
    } else {
        check(LABELS[5], Status::Warn, Detail::TrayMissing, None)
    }
}

fn crashes(host: &dyn DoctorHost) -> Check {
    match host.newest_crash() {
        None => check(LABELS[6], Status::Ok, Detail::NoCrash, None),
        Some(name) => check(
            LABELS[6],
            Status::Warn,
            Detail::NewestCrash(name),
            Some(Fix::OpenLogsFolder),
        ),
    }
}

/// What the report says about this machine and this build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportInfo {
    /// Snipdeck's version.
    pub version: String,
    /// The operating system and its build.
    pub os: String,
    /// The display scale ("150%").
    pub dpi: String,
    /// Setting flags as `(name, value)`: booleans and numbers only, never text the user typed.
    pub flags: Vec<(String, String)>,
    /// The data folder's path, replaced by `<data folder>` wherever it appears.
    pub data_dir: String,
}

/// The diagnostic report as text.
#[must_use]
pub fn report(info: &ReportInfo, checks: &[Check]) -> String {
    let mut out = String::new();
    out.push_str("Snipdeck diagnostic report\n");
    let _ = write!(
        out,
        "Version: {}\nSystem: {}\nDisplay scale: {}\n",
        info.version, info.os, info.dpi
    );
    out.push_str("\nSettings\n");
    for (name, value) in &info.flags {
        let _ = writeln!(out, "  {name}: {value}");
    }
    out.push_str("\nChecks\n");
    for c in checks {
        let _ = writeln!(out, "  [{}] {}: {}", c.status.word(), c.label, c.detail);
    }
    if info.data_dir.is_empty() {
        out
    } else {
        out.replace(&info.data_dir, "<data folder>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host with every fact settable; `Default` is a healthy machine on 2026-10-19.
    struct Fake {
        writable: Result<(), String>,
        free: Option<u64>,
        integrity: Result<Vec<String>, String>,
        backups: Result<Vec<Date>, String>,
        sensitive: Result<usize, String>,
        key: Result<bool, String>,
        unset: Vec<HotkeyAction>,
        tray: bool,
        crash: Option<String>,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                writable: Ok(()),
                free: Some(10 * 1024 * 1024 * 1024),
                integrity: Ok(Vec::new()),
                backups: Ok(vec![
                    Date::constant(2026, 10, 18),
                    Date::constant(2026, 10, 17),
                ]),
                sensitive: Ok(0),
                key: Ok(true),
                unset: Vec::new(),
                tray: true,
                crash: None,
            }
        }
    }

    impl DoctorHost for Fake {
        fn today(&self) -> Date {
            Date::constant(2026, 10, 19)
        }
        fn data_dir_writable(&self) -> Result<(), String> {
            self.writable.clone()
        }
        fn free_bytes(&self) -> Option<u64> {
            self.free
        }
        fn integrity(&self) -> Result<Vec<String>, String> {
            self.integrity.clone()
        }
        fn backup_dates(&self) -> Result<Vec<Date>, String> {
            self.backups.clone()
        }
        fn sensitive_items(&self) -> Result<usize, String> {
            self.sensitive.clone()
        }
        fn key_ring_exists(&self) -> Result<bool, String> {
            self.key.clone()
        }
        fn shortcuts_unset(&self) -> Vec<HotkeyAction> {
            self.unset.clone()
        }
        fn shortcuts_total(&self) -> usize {
            4
        }
        fn tray_present(&self) -> bool {
            self.tray
        }
        fn newest_crash(&self) -> Option<String> {
            self.crash.clone()
        }
    }

    fn only(host: &Fake, label: &str) -> Check {
        run(host).into_iter().find(|c| c.label == label).unwrap()
    }

    #[test]
    fn a_healthy_machine_passes_every_check_in_order() {
        let checks = run(&Fake::default());
        let labels: Vec<_> = checks.iter().map(|c| c.label).collect();
        assert_eq!(labels, LABELS);
        assert!(
            checks
                .iter()
                .all(|c| c.status == Status::Ok && c.fix.is_none()),
            "{checks:?}"
        );
        assert_eq!(worst(&checks), Status::Ok);
    }

    #[test]
    fn the_data_folder_fails_when_unwritable_and_warns_when_nearly_full() {
        let fail = only(
            &Fake {
                writable: Err("read-only".into()),
                ..Fake::default()
            },
            "Data folder",
        );
        assert_eq!(fail.status, Status::Fail);
        assert!(fail.detail.to_string().contains("read-only"));
        let warn = only(
            &Fake {
                free: Some(50 * 1024 * 1024),
                ..Fake::default()
            },
            "Data folder",
        );
        assert_eq!(warn.status, Status::Warn);
        assert!(warn.detail.to_string().contains("50 MB"));
        let unknown = only(
            &Fake {
                free: None,
                ..Fake::default()
            },
            "Data folder",
        );
        assert_eq!(unknown.status, Status::Ok);
    }

    #[test]
    fn the_database_fails_on_a_finding_or_when_the_check_cannot_run() {
        let found = only(
            &Fake {
                integrity: Ok(vec!["row 3 missing".into(), "x".into()]),
                ..Fake::default()
            },
            "Database",
        );
        assert_eq!(found.status, Status::Fail);
        assert!(
            found.detail.to_string().contains("2 problem")
                && found.detail.to_string().contains("row 3 missing")
        );
        let broken = only(
            &Fake {
                integrity: Err("locked".into()),
                ..Fake::default()
            },
            "Database",
        );
        assert_eq!(broken.status, Status::Fail);
    }

    #[test]
    fn a_damaged_database_offers_a_rebuild_with_a_backup_and_the_folder_without() {
        let damaged = |backups| {
            only(
                &Fake {
                    integrity: Ok(vec!["row 3 missing".into()]),
                    backups,
                    ..Fake::default()
                },
                "Database",
            )
        };
        assert_eq!(
            damaged(Ok(vec![Date::constant(2026, 10, 18)])).fix,
            Some(Fix::RebuildFromBackup)
        );
        assert_eq!(damaged(Ok(Vec::new())).fix, Some(Fix::OpenDataFolder));
        assert_eq!(damaged(Err("x".into())).fix, Some(Fix::OpenDataFolder));
        assert_eq!(only(&Fake::default(), "Database").fix, None);
    }

    #[test]
    fn data_folder_problems_offer_the_folder() {
        let fail = only(
            &Fake {
                writable: Err("read-only".into()),
                ..Fake::default()
            },
            "Data folder",
        );
        assert_eq!(fail.fix, Some(Fix::OpenDataFolder));
        let low = only(
            &Fake {
                free: Some(50 * 1024 * 1024),
                ..Fake::default()
            },
            "Data folder",
        );
        assert_eq!(low.fix, Some(Fix::OpenDataFolder));
    }

    #[test]
    fn backups_warn_when_missing_or_stale_and_offer_a_backup() {
        let none = only(
            &Fake {
                backups: Ok(Vec::new()),
                ..Fake::default()
            },
            "Backups",
        );
        assert_eq!(
            (none.status, none.fix),
            (Status::Warn, Some(Fix::BackUpNow))
        );
        let stale = only(
            &Fake {
                backups: Ok(vec![Date::constant(2026, 10, 10)]),
                ..Fake::default()
            },
            "Backups",
        );
        assert_eq!(
            (stale.status, stale.fix),
            (Status::Warn, Some(Fix::BackUpNow))
        );
        assert!(
            stale.detail.to_string().contains("2026-10-10")
                && stale.detail.to_string().contains("9 days")
        );
        let fresh = only(&Fake::default(), "Backups");
        assert_eq!((fresh.status, fresh.fix), (Status::Ok, None));
        assert!(
            fresh.detail.to_string().contains("2026-10-18")
                && fresh.detail.to_string().contains("2 kept")
        );
        let unreadable = only(
            &Fake {
                backups: Err("denied".into()),
                ..Fake::default()
            },
            "Backups",
        );
        assert_eq!(unreadable.status, Status::Warn);
    }

    #[test]
    fn the_key_matters_only_when_sensitive_items_exist() {
        let none = only(
            &Fake {
                sensitive: Ok(0),
                key: Ok(false),
                ..Fake::default()
            },
            "Encryption key",
        );
        assert_eq!(none.status, Status::Ok);
        let missing = only(
            &Fake {
                sensitive: Ok(3),
                key: Ok(false),
                ..Fake::default()
            },
            "Encryption key",
        );
        // Not a failure: without a key the items are plain text, readable but not encrypted.
        assert_eq!(missing.status, Status::Warn);
        assert!(missing.detail.to_string().contains("plain text"));
        assert!(!missing.detail.to_string().contains("cannot be read"));
        let present = only(
            &Fake {
                sensitive: Ok(3),
                key: Ok(true),
                ..Fake::default()
            },
            "Encryption key",
        );
        assert_eq!(present.status, Status::Ok);
    }

    #[test]
    fn unset_shortcuts_warn_and_open_settings() {
        let some = only(
            &Fake {
                unset: vec![HotkeyAction::Picker],
                ..Fake::default()
            },
            "Shortcuts",
        );
        assert_eq!(
            (some.status, some.fix),
            (Status::Warn, Some(Fix::OpenSettings))
        );
        assert!(some.detail.to_string().contains("Open the Picker"));
        let all = only(
            &Fake {
                unset: HotkeyAction::ALL.to_vec(),
                ..Fake::default()
            },
            "Shortcuts",
        );
        assert_eq!(all.detail, Detail::ShortcutsNoneSet);
    }

    #[test]
    fn a_missing_tray_icon_warns() {
        assert_eq!(
            only(
                &Fake {
                    tray: false,
                    ..Fake::default()
                },
                "Tray icon"
            )
            .status,
            Status::Warn
        );
    }

    #[test]
    fn a_crash_report_warns_and_offers_the_logs_folder() {
        let c = only(
            &Fake {
                crash: Some("crash-2026-10-18.txt".into()),
                ..Fake::default()
            },
            "Crash reports",
        );
        assert_eq!((c.status, c.fix), (Status::Warn, Some(Fix::OpenLogsFolder)));
        assert!(c.detail.to_string().contains("crash-2026-10-18.txt"));
    }

    #[test]
    fn the_worst_status_wins() {
        let checks = run(&Fake {
            tray: false,
            integrity: Err("x".into()),
            ..Fake::default()
        });
        assert_eq!(worst(&checks), Status::Fail);
        assert_eq!(
            worst(&run(&Fake {
                tray: false,
                ..Fake::default()
            })),
            Status::Warn
        );
    }

    #[test]
    fn fix_codes_round_trip() {
        for fix in [
            Fix::BackUpNow,
            Fix::OpenSettings,
            Fix::OpenLogsFolder,
            Fix::RebuildFromBackup,
            Fix::OpenDataFolder,
        ] {
            assert_eq!(Fix::from_code(fix.code()), Some(fix));
        }
        assert_eq!(Fix::from_code(0), None);
    }

    #[test]
    fn the_report_lists_versions_flags_and_checks_and_hides_the_data_folder_path() {
        let info = ReportInfo {
            version: "0.1.0".into(),
            os: "Windows 11 build 26100".into(),
            dpi: "150%".into(),
            flags: vec![("close to tray".into(), "on".into())],
            data_dir: "C:\\Users\\ann\\AppData\\Snipdeck".into(),
        };
        let checks = run(&Fake {
            integrity: Err("cannot open C:\\Users\\ann\\AppData\\Snipdeck\\library.db".into()),
            ..Fake::default()
        });
        let text = report(&info, &checks);
        assert!(text.contains("Version: 0.1.0") && text.contains("Display scale: 150%"));
        assert!(text.contains("close to tray: on"));
        assert!(text.contains("[Problem] Database"));
        assert!(text.contains("<data folder>\\library.db"));
        // "ann" alone also matches the "cannot" of the error text.
        assert!(
            !text.contains("\\ann\\"),
            "the user name in the path is hidden"
        );
    }
}
