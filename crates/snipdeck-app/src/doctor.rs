//! The Troubleshoot checks of the Help window: is the data folder writable, does the database
//! pass its integrity check, are there recent backups, is the encryption key there, are the
//! shortcuts set, is the tray icon up, did the app crash lately.
//!
//! [`run`] is pure: it asks a [`DoctorHost`] for facts and turns them into [`Check`]s, so tests
//! run every check passing and failing against a fake. The app's host is
//! [`crate::help_binding::ControllerDoctor`]. The words are literal English, like the toasts
//! (see [`crate::wording`]); the window's own labels are `@tr`.
//!
//! [`report`] renders the diagnostic report the user can copy: versions, system, setting flags and
//! the check results. It never holds item content, and a path inside the data folder is replaced
//! by `<data folder>` (nothing outside it is ever asked for).

use std::fmt::Write as _;

use jiff::civil::Date;

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
}

impl Fix {
    /// The button's number across the Slint boundary (0 is "no fix").
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::BackUpNow => 1,
            Self::OpenSettings => 2,
            Self::OpenLogsFolder => 3,
        }
    }

    /// The inverse of [`Fix::code`].
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::BackUpNow),
            2 => Some(Self::OpenSettings),
            3 => Some(Self::OpenLogsFolder),
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
    /// One sentence on what was found.
    pub detail: String,
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
    fn shortcuts_unset(&self) -> Vec<String>;
    /// How many global shortcuts there are in all.
    fn shortcuts_total(&self) -> usize;
    /// Whether the tray icon is in the notification area.
    fn tray_present(&self) -> bool;
    /// The file name of the newest crash report, if any.
    fn newest_crash(&self) -> Option<String>;
}

fn check(
    label: &'static str,
    status: Status,
    detail: impl Into<String>,
    fix: Option<Fix>,
) -> Check {
    Check {
        label,
        status,
        detail: detail.into(),
        fix,
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
            format!("Snipdeck cannot write to its data folder: {e}"),
            None,
        );
    }
    match host.free_bytes() {
        Some(free) if free < LOW_SPACE_BYTES => check(
            label,
            Status::Warn,
            format!(
                "Writable, but only {} MB are free on its drive.",
                free / (1024 * 1024)
            ),
            None,
        ),
        _ => check(label, Status::Ok, "Writable, with room to spare.", None),
    }
}

fn database(host: &dyn DoctorHost) -> Check {
    let label = LABELS[1];
    match host.integrity() {
        Err(e) => check(
            label,
            Status::Fail,
            format!("The integrity check could not run: {e}"),
            None,
        ),
        Ok(problems) if problems.is_empty() => check(
            label,
            Status::Ok,
            "The integrity check found no problems.",
            None,
        ),
        Ok(problems) => check(
            label,
            Status::Fail,
            format!(
                "The integrity check found {} problem(s); the first: {}",
                problems.len(),
                problems[0]
            ),
            None,
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
                format!("The backups cannot be read: {e}"),
                Some(Fix::BackUpNow),
            );
        }
    };
    let Some(newest) = dates.iter().max().copied() else {
        return check(
            label,
            Status::Warn,
            "No backup has been made yet.",
            Some(Fix::BackUpNow),
        );
    };
    let age = host
        .today()
        .since(newest)
        .map_or(0, |span| i64::from(span.get_days()));
    if age > STALE_BACKUP_DAYS {
        check(
            label,
            Status::Warn,
            format!(
                "The newest backup is from {newest}, {age} days ago ({} kept).",
                dates.len()
            ),
            Some(Fix::BackUpNow),
        )
    } else {
        check(
            label,
            Status::Ok,
            format!("Newest backup {newest}, {} kept.", dates.len()),
            None,
        )
    }
}

fn encryption(host: &dyn DoctorHost) -> Check {
    let label = LABELS[3];
    let sensitive = match host.sensitive_items() {
        Ok(n) => n,
        Err(e) => {
            return check(
                label,
                Status::Warn,
                format!("The items cannot be read: {e}"),
                None,
            );
        }
    };
    if sensitive == 0 {
        return check(label, Status::Ok, "No item is marked sensitive.", None);
    }
    match host.key_ring_exists() {
        Ok(true) => check(
            label,
            Status::Ok,
            format!("{sensitive} sensitive item(s), the key is present."),
            None,
        ),
        Ok(false) => check(
            label,
            Status::Fail,
            format!("{sensitive} sensitive item(s) but no encryption key: they cannot be read."),
            None,
        ),
        Err(e) => check(
            label,
            Status::Warn,
            format!("The key cannot be checked: {e}"),
            None,
        ),
    }
}

fn shortcuts(host: &dyn DoctorHost) -> Check {
    let label = LABELS[4];
    let unset = host.shortcuts_unset();
    if unset.is_empty() {
        check(label, Status::Ok, "All global shortcuts are set.", None)
    } else if unset.len() >= host.shortcuts_total() {
        check(
            label,
            Status::Warn,
            "No global shortcut is set.",
            Some(Fix::OpenSettings),
        )
    } else {
        check(
            label,
            Status::Warn,
            format!("Not set: {}.", unset.join(", ")),
            Some(Fix::OpenSettings),
        )
    }
}

fn tray(host: &dyn DoctorHost) -> Check {
    if host.tray_present() {
        check(
            LABELS[5],
            Status::Ok,
            "The icon is in the notification area.",
            None,
        )
    } else {
        check(
            LABELS[5],
            Status::Warn,
            "No icon in the notification area; closing the panel quits Snipdeck.",
            None,
        )
    }
}

fn crashes(host: &dyn DoctorHost) -> Check {
    match host.newest_crash() {
        None => check(LABELS[6], Status::Ok, "No crash report.", None),
        Some(name) => check(
            LABELS[6],
            Status::Warn,
            format!("The newest crash report is {name}."),
            Some(Fix::OpenLogsFolder),
        ),
    }
}

/// What the report says about this machine and this build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportInfo {
    /// Snipdeck's version.
    pub version: String,
    /// The licence tier ("Free", "Pro").
    pub tier: String,
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
        "Version: {}\nLicence: {}\nSystem: {}\nDisplay scale: {}\n",
        info.version, info.tier, info.os, info.dpi
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
        unset: Vec<String>,
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
        fn shortcuts_unset(&self) -> Vec<String> {
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
        assert!(fail.detail.contains("read-only"));
        let warn = only(
            &Fake {
                free: Some(50 * 1024 * 1024),
                ..Fake::default()
            },
            "Data folder",
        );
        assert_eq!(warn.status, Status::Warn);
        assert!(warn.detail.contains("50 MB"));
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
        assert!(found.detail.contains("2 problem") && found.detail.contains("row 3 missing"));
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
        assert!(stale.detail.contains("2026-10-10") && stale.detail.contains("9 days"));
        let fresh = only(&Fake::default(), "Backups");
        assert_eq!((fresh.status, fresh.fix), (Status::Ok, None));
        assert!(fresh.detail.contains("2026-10-18") && fresh.detail.contains("2 kept"));
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
        assert_eq!(missing.status, Status::Fail);
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
                unset: vec!["Open the picker".into()],
                ..Fake::default()
            },
            "Shortcuts",
        );
        assert_eq!(
            (some.status, some.fix),
            (Status::Warn, Some(Fix::OpenSettings))
        );
        assert!(some.detail.contains("Open the picker"));
        let all = only(
            &Fake {
                unset: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                ..Fake::default()
            },
            "Shortcuts",
        );
        assert_eq!(all.detail, "No global shortcut is set.");
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
        assert!(c.detail.contains("crash-2026-10-18.txt"));
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
        for fix in [Fix::BackUpNow, Fix::OpenSettings, Fix::OpenLogsFolder] {
            assert_eq!(Fix::from_code(fix.code()), Some(fix));
        }
        assert_eq!(Fix::from_code(0), None);
    }

    #[test]
    fn the_report_lists_versions_flags_and_checks_and_hides_the_data_folder_path() {
        let info = ReportInfo {
            version: "0.1.0".into(),
            tier: "Free".into(),
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
