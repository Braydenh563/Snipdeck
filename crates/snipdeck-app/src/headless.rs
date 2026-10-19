//! The command-line checks `--check-db` and `--troubleshoot`: the library's integrity check and the
//! Help window's Troubleshoot list, printed as text without starting the app.
//!
//! Both open the data folder like the app does, so they refuse while Snipdeck is running (the
//! folder is locked). The words are literal English, like [`crate::doctor`].

use std::path::{Path, PathBuf};

use jiff::civil::Date;
use snipdeck_core::Tier;
use snipdeck_portable::backup;
use snipdeck_store::{ItemRepo, Store};

use crate::{
    crash,
    doctor::{self, DoctorHost, ReportInfo, Status},
    paths,
    settings::{HotkeyAction, Settings},
    settings_binding::action_name,
};

/// What a command-line check found: the text to print and the process exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// The text for standard output.
    pub text: String,
    /// 0 healthy, 1 problems found, 2 the check could not run.
    pub code: u8,
}

/// The code for a check that could not run.
const COULD_NOT_RUN: u8 = 2;

fn could_not_run(folder: &Path, why: &str) -> Outcome {
    Outcome {
        text: format!("{}: {why}", folder.display()),
        code: COULD_NOT_RUN,
    }
}

/// Opens the library in `folder`; a folder with none gets an empty one, which is healthy.
fn open(folder: &Path) -> Result<Store, Outcome> {
    Store::open(folder).map_err(|e| could_not_run(folder, &e.to_string()))
}

/// `--check-db`: the store's integrity check on the library in `folder`.
#[must_use]
pub fn check_db(folder: &Path) -> Outcome {
    let store = match open(folder) {
        Ok(store) => store,
        Err(outcome) => return outcome,
    };
    match store.check_integrity() {
        Err(e) => could_not_run(folder, &format!("the integrity check could not run: {e}")),
        Ok(problems) if problems.is_empty() => Outcome {
            text: "ok: the integrity check found no problems".to_owned(),
            code: 0,
        },
        Ok(problems) => Outcome {
            text: format!(
                "{} problem(s) found:\n{}",
                problems.len(),
                problems
                    .iter()
                    .map(|p| format!("  {p}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            code: 1,
        },
    }
}

/// What `--troubleshoot` cannot know from a stopped app.
const NOT_CHECKED: &str = "Not checked from the command line.";

/// The facts of [`DoctorHost`] for a library that is open but not running.
struct FolderDoctor<'a> {
    store: &'a Store,
    folder: &'a Path,
    settings: &'a Settings,
    today: Date,
}

impl DoctorHost for FolderDoctor<'_> {
    fn today(&self) -> Date {
        self.today
    }

    fn data_dir_writable(&self) -> Result<(), String> {
        let probe = self.folder.join(".snipdeck-write-test");
        std::fs::write(&probe, b"").map_err(|e| e.to_string())?;
        std::fs::remove_file(&probe).map_err(|e| e.to_string())
    }

    fn free_bytes(&self) -> Option<u64> {
        None
    }

    fn integrity(&self) -> Result<Vec<String>, String> {
        self.store.check_integrity().map_err(|e| e.to_string())
    }

    fn backup_dates(&self) -> Result<Vec<Date>, String> {
        backup::list_backups(self.folder)
            .map(|files| files.into_iter().map(|f| f.date).collect())
            .map_err(|e| e.to_string())
    }

    fn sensitive_items(&self) -> Result<usize, String> {
        self.store
            .all_items()
            .map(|items| items.iter().filter(|i| i.sensitive).count())
            .map_err(|e| e.to_string())
    }

    fn key_ring_exists(&self) -> Result<bool, String> {
        self.store.key_ring_exists().map_err(|e| e.to_string())
    }

    fn shortcuts_unset(&self) -> Vec<String> {
        let hotkeys = &self.settings.hotkeys;
        HotkeyAction::ALL
            .into_iter()
            .filter(|action| {
                let chord = match action {
                    HotkeyAction::TogglePanel => &hotkeys.toggle_panel,
                    HotkeyAction::Picker => &hotkeys.picker,
                    HotkeyAction::Capture => &hotkeys.capture,
                    HotkeyAction::PasteNext => &hotkeys.paste_next,
                };
                chord.is_empty()
            })
            .map(|action| action_name(action).to_owned())
            .collect()
    }

    fn shortcuts_total(&self) -> usize {
        HotkeyAction::ALL.len()
    }

    fn tray_present(&self) -> bool {
        false
    }

    fn newest_crash(&self) -> Option<String> {
        crash::newest(self.folder)
    }
}

/// `--troubleshoot`: the Troubleshoot checks and the diagnostic report for the library in
/// `folder`, as the Help window's "Copy report" would give. `today` and `os` are passed in.
#[must_use]
pub fn troubleshoot(folder: &Path, today: Date, os: &str) -> Outcome {
    let store = match open(folder) {
        Ok(store) => store,
        Err(outcome) => return outcome,
    };
    let settings = paths::read_settings(folder).settings;
    let host = FolderDoctor {
        store: &store,
        folder,
        settings: &settings,
        today,
    };
    let mut checks = doctor::run(&host);
    // The tray belongs to a running app: say so, rather than warning about an icon that cannot be.
    for c in checks.iter_mut().filter(|c| c.label == doctor::LABELS[5]) {
        c.status = Status::Ok;
        NOT_CHECKED.clone_into(&mut c.detail);
    }
    let info = ReportInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        tier: tier_word(crate::licence_store::entitlements(folder, jiff::Timestamp::now()).tier()),
        os: os.to_owned(),
        dpi: "n/a".to_owned(),
        flags: crate::help_binding::report_flags(&settings),
        data_dir: folder.display().to_string(),
    };
    Outcome {
        text: doctor::report(&info, &checks),
        code: match doctor::worst(&checks) {
            Status::Ok => 0,
            Status::Warn | Status::Fail => 1,
        },
    }
}

fn tier_word(tier: Tier) -> String {
    if matches!(tier, Tier::Pro) {
        "Pro"
    } else {
        "Free"
    }
    .to_owned()
}

/// The default data folder, or `dir` when given.
#[must_use]
pub fn folder_or_default(dir: Option<&PathBuf>) -> Option<PathBuf> {
    match dir {
        Some(dir) => Some(dir.clone()),
        None => paths::data_dir(&|name| std::env::var(name).ok(), cfg!(windows)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: Date = Date::constant(2026, 10, 19);

    fn library() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("dir");
        drop(Store::open(dir.path()).expect("store"));
        dir
    }

    #[test]
    fn a_healthy_library_passes_check_db() {
        let dir = library();
        let out = check_db(dir.path());
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(out.text.starts_with("ok"));
    }

    #[test]
    fn a_fresh_empty_folder_is_healthy() {
        let dir = tempfile::tempdir().expect("dir");
        let out = check_db(dir.path());
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(dir.path().join("snipdeck.db").exists());
    }

    #[test]
    fn a_running_app_is_named_not_a_panic() {
        let dir = library();
        let _held = Store::open(dir.path()).expect("held");
        let out = check_db(dir.path());
        assert_eq!(out.code, COULD_NOT_RUN);
        assert!(out.text.contains("already using"), "{}", out.text);
    }

    #[test]
    fn troubleshoot_prints_the_report_with_the_folder_hidden() {
        let dir = library();
        let out = troubleshoot(dir.path(), TODAY, "TestOS 1");
        assert!(
            out.text.starts_with("Snipdeck diagnostic report"),
            "{}",
            out.text
        );
        assert!(out.text.contains("System: TestOS 1"));
        assert!(out.text.contains("[OK] Database"));
        assert!(out.text.contains("[OK] Tray icon: Not checked"));
        assert!(!out.text.contains(&dir.path().display().to_string()));
        // No backup yet: a warning, so exit 1.
        assert_eq!(out.code, 1);
    }
}
