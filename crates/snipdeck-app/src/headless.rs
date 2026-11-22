//! The command-line commands `--check-db`, `--troubleshoot` and `--backup`: the library's integrity
//! check, the Help window's Troubleshoot list and a backup file, without starting the app.
//!
//! All three only read: the library is opened read-only (no folder or database is created, nothing
//! is migrated, no lock is taken) and the settings file is never set aside, so they are safe beside
//! a running Snipdeck and a mistyped `--data-dir` is an error, not a new library. The words are literal English, like [`crate::doctor`].

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use jiff::civil::Date;
use snipdeck_portable::backup;
use snipdeck_store::{ItemRepo, Store};

use crate::{
    crash,
    doctor::{self, DoctorHost, ReportInfo, Status},
    paths,
    settings::{HotkeyAction, Settings},
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

/// Opens the library in `folder` for reading only: a diagnostic changes nothing (it may run beside
/// a running Snipdeck) and a folder with no library is "could not run", not an empty healthy one.
fn open(folder: &Path) -> Result<Store, Outcome> {
    Store::open_read_only(folder).map_err(|e| could_not_run(folder, &e.to_string()))
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

/// `--backup <file>`: writes the library in `folder` to `dest` (a `.snipdeck` archive, as Settings'
/// "Back up now" makes), reading only. Sensitive items are left out: the key ring needs the
/// master password or Windows Hello, which a command line has not got.
#[must_use]
pub fn backup_to(folder: &Path, now: jiff::Timestamp, dest: &Path) -> Outcome {
    let store = match open(folder) {
        Ok(store) => store,
        Err(outcome) => return outcome,
    };
    let options = snipdeck_portable::ExportOptions::default();
    match snipdeck_portable::export_to_path(&store, now, &options, dest) {
        Ok(report) => {
            let mut text = format!("ok: wrote {}", dest.display());
            if report.sensitive_omitted > 0 {
                let n = report.sensitive_omitted;
                let _ = write!(
                    text,
                    "; {n} sensitive item{} {} left out: the command line has no key for {}",
                    if n == 1 { "" } else { "s" },
                    if n == 1 { "was" } else { "were" },
                    if n == 1 { "it" } else { "them" },
                );
            }
            Outcome { text, code: 0 }
        }
        Err(e) => could_not_run(folder, &format!("the backup could not be written: {e}")),
    }
}

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
        backup::list_backups(&self.settings.backup_folder(self.folder))
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

    fn shortcuts_unset(&self) -> Vec<HotkeyAction> {
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
    let settings = paths::peek_settings(folder);
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
        c.detail = doctor::Detail::NotChecked;
    }
    let info = ReportInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        os: os.to_owned(),
        dpi: "n/a".to_owned(),
        flags: crate::help_binding::report_flags(&settings),
        data_dir: folder.display().to_string(),
        log_tail: crate::log::tail(folder, crate::log::REPORT_LINES),
    };
    Outcome {
        text: doctor::report(&info, &checks),
        code: match doctor::worst(&checks) {
            Status::Ok => 0,
            Status::Warn | Status::Fail => 1,
        },
    }
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
    fn a_folder_with_no_library_is_not_a_healthy_library_and_is_not_created() {
        let dir = tempfile::tempdir().expect("dir");
        let typo = dir.path().join("mistyped");
        for out in [check_db(&typo), troubleshoot(&typo, TODAY, "TestOS 1")] {
            assert_eq!(out.code, COULD_NOT_RUN, "{}", out.text);
            assert!(out.text.contains("no library in"), "{}", out.text);
        }
        assert!(!typo.exists(), "a diagnostic must not create the folder");
        assert!(check_db(dir.path()).text.contains("no library in"));
        assert!(!dir.path().join("snipdeck.db").exists());
    }

    #[test]
    fn a_running_app_does_not_stop_the_check() {
        let dir = library();
        let _held = Store::open(dir.path()).expect("held");
        let out = check_db(dir.path());
        assert_eq!(out.code, 0, "{}", out.text);
    }

    #[test]
    fn a_damaged_settings_file_is_read_but_never_set_aside() {
        let dir = library();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{ not json").expect("write");
        let _ = troubleshoot(dir.path(), TODAY, "TestOS 1");
        assert_eq!(std::fs::read_to_string(&file).expect("kept"), "{ not json");
        assert!(!dir.path().join("settings.json.bad").exists());
    }

    #[test]
    fn a_backup_is_written_from_a_library_and_never_from_a_typo() {
        let dir = library();
        let out_dir = tempfile::tempdir().expect("dir");
        let dest = out_dir.path().join("mine.snipdeck");
        let now = jiff::Timestamp::from_second(1_760_000_000).expect("time");
        let out = backup_to(dir.path(), now, &dest);
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(dest.metadata().expect("written").len() > 0);

        let typo = out_dir.path().join("mistyped");
        let out = backup_to(&typo, now, &out_dir.path().join("x.snipdeck"));
        assert_eq!(out.code, COULD_NOT_RUN, "{}", out.text);
        assert!(!typo.exists());
    }

    #[test]
    fn a_backup_says_how_many_sensitive_items_it_left_out() {
        use snipdeck_core::{
            Board, Content, Format, Formats, Inline, Item, ItemKind, List, Name, Placement,
            Timestamp,
        };
        use snipdeck_store::{BoardRepo, ListRepo};

        let dir = tempfile::tempdir().expect("dir");
        let mut store = Store::open(dir.path()).expect("store");
        let at = Timestamp::from_second(1_800_000_000).expect("time");
        let board = Board::new(Name::new("Vault").expect("name"), at);
        store.insert_board(&board, Placement::Last).expect("board");
        let list = List::new(board.id, Name::new("Cards").expect("name"), at);
        store.insert_list(&list, Placement::Last).expect("list");
        for title in ["PIN", "Card"] {
            let mut item = Item::new(list.id, title.to_owned(), ItemKind::Text, at);
            item.sensitive = true;
            let text = Content::Inline(Inline::new(b"secret".to_vec()).expect("inline"));
            store
                .insert_item(
                    &item,
                    &Formats::new(vec![Format::Text(text)]).expect("formats"),
                    Placement::Last,
                )
                .expect("item");
        }
        drop(store);

        let out_dir = tempfile::tempdir().expect("dir");
        let dest = out_dir.path().join("mine.snipdeck");
        let now = jiff::Timestamp::from_second(1_760_000_000).expect("time");
        let out = backup_to(dir.path(), now, &dest);
        assert_eq!(out.code, 0, "{}", out.text);
        assert!(
            out.text.contains("2 sensitive items were left out"),
            "{}",
            out.text
        );

        // No sensitive item, nothing to say.
        let plain = library();
        let out = backup_to(plain.path(), now, &out_dir.path().join("plain.snipdeck"));
        assert!(!out.text.contains("sensitive"), "{}", out.text);
    }

    #[test]
    fn the_report_carries_the_last_warnings() {
        let dir = library();
        crate::log::append(dir.path(), "2026-10-19T10:00:00Z tray update failed", 1000)
            .expect("log");
        let out = troubleshoot(dir.path(), TODAY, "TestOS 1");
        assert!(out.text.contains("Recent warnings"), "{}", out.text);
        assert!(out.text.contains("tray update failed"));
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
        assert!(out.text.contains("[OK] Library"));
        assert!(out.text.contains("[OK] Tray icon: Not checked"));
        assert!(!out.text.contains(&dir.path().display().to_string()));
        // No backup yet: a warning, so exit 1.
        assert_eq!(out.code, 1);
    }
}
