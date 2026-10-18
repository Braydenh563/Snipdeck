//! The real `SettingsPage` bound to a `Controller` over the demo library, the platform fakes, a
//! fake machine (`SettingsHost`) and a frozen clock, driven through the page's callbacks and
//! Slint's testing backend. Time is mocked, never the real clock. One process for this file (the
//! testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]

use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::{ComponentHandle, Model};
use snipdeck_app::{
    controller::{Controller, Services},
    demo,
    host::{FakeHotkeys, FakeTargets, FixedClock, HotkeyProblem},
    licence_store,
    main_binding::MainUi,
    paths::load_settings,
    settings::{HotkeyAction, Hotkeys, Settings, Theme},
    settings_binding::{self, SettingsBinding, SettingsHost, SettingsUi, request_open},
    settings_store::{FileSink, SettingsSaver},
    wording,
};
use snipdeck_core::{
    Entitlements, Tier, Timestamp,
    licence::{
        IS_DEVELOPMENT_KEY,
        signing::{SigningKey, pro_payload, sign_token},
    },
};
use snipdeck_platform::{
    testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    tray::TrayEvent,
};
use snipdeck_store::{
    BoardRepo, ItemRepo, Store,
    crypto::{KeyRing, MemoryKeyProvider},
};
use snipdeck_ui::{SettingsPage, ToastKind};

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;
type Bound = SettingsBinding<FakeClipboard, FakeKeyboard, FakeTiming>;

fn at(s: &str) -> Timestamp {
    s.parse().expect("timestamp")
}

fn now() -> Timestamp {
    at("2026-10-17T10:30:00Z")
}

/// A machine that records what it was asked and can refuse.
#[derive(Default)]
struct FakeHost {
    hotkeys: RefCell<FakeHotkeys>,
    autostart: Cell<Option<bool>>,
    autostart_fails: Cell<bool>,
    opened: RefCell<Vec<PathBuf>>,
    documents: RefCell<Option<PathBuf>>,
}

impl SettingsHost for FakeHost {
    fn apply_hotkeys(&self, hotkeys: &Hotkeys) -> Vec<HotkeyProblem> {
        settings_binding::apply_hotkeys(&mut *self.hotkeys.borrow_mut(), hotkeys)
    }

    fn set_autostart(&self, on: bool) -> Result<(), String> {
        if self.autostart_fails.get() {
            return Err("the registry said no".into());
        }
        self.autostart.set(Some(on));
        Ok(())
    }

    fn open_folder(&self, folder: &Path) -> Result<(), String> {
        self.opened.borrow_mut().push(folder.to_owned());
        Ok(())
    }

    fn documents_folder(&self) -> Option<PathBuf> {
        self.documents.borrow().clone()
    }
}

struct Rig {
    ui: SettingsPage,
    ctl: Rc<RefCell<Ctl>>,
    #[allow(dead_code, reason = "the binding lives as long as the rig")]
    binding: Bound,
    clock: FixedClock,
    /// Milliseconds the clocks have been moved on.
    elapsed: Cell<u64>,
    host: Rc<FakeHost>,
    /// The data folder (settings, licence, backups).
    dir: tempfile::TempDir,
}

fn controller_on(store: Store, tier: Tier, clock: &FixedClock) -> Rc<RefCell<Ctl>> {
    Rc::new(RefCell::new(
        Controller::new(
            store,
            Services {
                clipboard: FakeClipboard::default(),
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            },
            Box::new(FakeTargets::new(Some(demo::target()))),
            Box::new(clock.clone()),
            Settings::default(),
            Entitlements::new(tier),
        )
        .expect("controller"),
    ))
}

fn rig_on(store: Store, tier: Tier, dir: tempfile::TempDir, with_data_dir: bool) -> Rig {
    let clock = FixedClock::new(now());
    let ctl = controller_on(store, tier, &clock);
    ctl.borrow_mut().set_settings_saver(SettingsSaver::new(
        Box::new(FileSink::new(dir.path())),
        Settings::default(),
    ));
    let host = Rc::new(FakeHost::default());
    *host.documents.borrow_mut() = Some(dir.path().join("Documents"));
    let ui = SettingsPage::new().expect("settings page");
    // Tall, so the whole page is on screen.
    ui.window().set_size(slint::LogicalSize::new(640.0, 2400.0));
    let data_dir = with_data_dir.then(|| dir.path().to_owned());
    let host_dyn: Rc<dyn SettingsHost> = Rc::clone(&host) as Rc<dyn SettingsHost>;
    let binding = Controller::bind_settings(&ctl, &ui, data_dir, host_dyn);
    ui.show().expect("show");
    slint::platform::update_timers_and_animations();
    Rig {
        ui,
        ctl,
        binding,
        clock,
        elapsed: Cell::new(0),
        host,
        dir,
    }
}

fn rig() -> Rig {
    rig_on(
        demo::store(now()).expect("demo"),
        Tier::Free,
        tempfile::tempdir().expect("dir"),
        true,
    )
}

fn element<C: ComponentHandle>(ui: &C, label: &str, role: AccessibleRole) -> ElementHandle {
    ElementHandle::find_by_accessible_label(ui, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} labelled {label:?}"))
}

impl Rig {
    fn settings_file(&self) -> PathBuf {
        self.dir.path().join("settings.json")
    }

    /// Moves both clocks on by `ms`.
    fn advance(&self, ms: u64) {
        self.elapsed.set(self.elapsed.get() + ms);
        let to = Timestamp::from_millisecond(
            now().as_millisecond() + i64::try_from(self.elapsed.get()).unwrap(),
        )
        .expect("timestamp");
        self.clock.set(to);
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(ms));
    }
}

// ---- Every setting is written after the quiet time -----------------------------------------

#[test]
fn each_toggle_writes_settings_json_after_the_quiet_time() {
    on_ui_thread(|| {
        type Case = (&'static str, fn(&Rig), fn(&Settings) -> bool);
        let cases: [Case; 9] = [
            (
                "theme",
                |r| r.ui.invoke_theme_chosen(2),
                |s| s.theme == Theme::Dark,
            ),
            (
                "language",
                |r| r.ui.invoke_language_chosen(3),
                |s| s.language == "de",
            ),
            (
                "start with Windows",
                |r| r.ui.invoke_autostart_toggled(true),
                |s| s.autostart,
            ),
            (
                "close to tray",
                |r| r.ui.invoke_close_to_tray_toggled(false),
                |s| !s.close_to_tray,
            ),
            (
                "site icons",
                |r| r.ui.invoke_site_icons_toggled(true),
                |s| s.site_icons,
            ),
            (
                "panel width",
                |r| r.ui.invoke_panel_width_chosen(400),
                |s| s.panel_width == 400,
            ),
            (
                "a program added",
                |r| r.ui.invoke_no_paste_app_added("KeePass.EXE".into()),
                |s| s.no_paste_apps == ["keepass.exe"],
            ),
            (
                "a shortcut recorded",
                |r| r.ui.invoke_hotkey_recorded(2, "Ctrl+Alt+K".into()),
                |s| s.hotkeys.capture == "Ctrl+Alt+K",
            ),
            (
                "a shortcut cleared",
                |r| r.ui.invoke_hotkey_cleared(3),
                |s| s.hotkeys.paste_next.is_empty(),
            ),
        ];
        for (name, act, check) in cases {
            let r = rig();
            assert!(!r.settings_file().exists(), "{name}: nothing yet");
            act(&r);
            assert!(
                !r.settings_file().exists(),
                "{name}: not before the quiet time"
            );
            r.advance(400);
            assert!(
                !r.settings_file().exists(),
                "{name}: still inside the quiet time"
            );
            r.advance(100);
            assert!(r.settings_file().exists(), "{name}: written at 500 ms");
            assert!(
                check(&load_settings(r.dir.path())),
                "{name}: {:?}",
                load_settings(r.dir.path())
            );
        }
    });
}

#[test]
fn removing_a_program_is_written_too_and_a_repeat_is_refused_in_words() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_no_paste_app_added("keepass.exe".into());
        r.ui.invoke_no_paste_app_added("Putty.exe".into());
        assert_eq!(r.ui.get_no_paste_apps().row_count(), 2);
        r.ui.invoke_no_paste_app_added("KEEPASS.exe".into());
        assert_eq!(
            r.ui.get_no_paste_error(),
            "keepass.exe is already in the list."
        );
        assert_eq!(r.ui.get_no_paste_apps().row_count(), 2, "not added twice");
        r.ui.invoke_no_paste_app_removed(0);
        assert_eq!(
            r.ui.get_no_paste_error(),
            "",
            "the message goes with the next change"
        );
        r.advance(500);
        assert_eq!(load_settings(r.dir.path()).no_paste_apps, ["putty.exe"]);
    });
}

#[test]
fn the_theme_applies_at_once_and_the_page_shows_what_is_stored() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_dark());
        r.ui.invoke_theme_chosen(2);
        assert!(r.ui.get_dark());
        assert_eq!(r.ui.get_theme_index(), 2);
        r.ctl.borrow_mut().update_settings(|s| {
            s.language = "ja".into();
            s.panel_width = 300;
        });
        // Any gesture redraws; the controller's settings are what the page shows.
        r.ui.invoke_site_icons_toggled(true);
        assert_eq!(r.ui.get_language_index(), 6);
        assert_eq!(r.ui.get_panel_width(), 300);
        assert_eq!(
            r.ui.get_languages().row_count(),
            8,
            "Follow Windows and seven languages"
        );
    });
}

// ---- Shortcuts ------------------------------------------------------------------------------

#[test]
fn a_shortcut_that_clashes_is_refused_and_says_which_action_has_it() {
    on_ui_thread(|| {
        let r = rig();
        let picker = r.ctl.borrow().settings().hotkeys.picker.clone();
        r.ui.invoke_hotkey_recorded(2, picker.as_str().into());
        let rows = r.ui.get_hotkeys();
        assert!(
            rows.row_data(2)
                .unwrap()
                .conflict
                .contains("Open the picker"),
            "{:?}",
            rows.row_data(2).unwrap().conflict
        );
        assert_eq!(
            r.ctl.borrow().settings().hotkeys.capture,
            "Ctrl+Shift+2",
            "unchanged"
        );
        assert_eq!(r.ctl.borrow().settings_wait(), None, "nothing to write");
        // A good one afterwards clears the message.
        r.ui.invoke_hotkey_recorded(2, "Ctrl+Alt+K".into());
        assert_eq!(r.ui.get_hotkeys().row_data(2).unwrap().conflict, "");
    });
}

#[test]
fn a_shortcut_another_program_owns_is_refused_and_the_old_ones_stay_registered() {
    on_ui_thread(|| {
        let r = rig();
        r.host.hotkeys.borrow_mut().taken.push("Ctrl+Alt+K".into());
        // The shortcuts registered at start.
        let before =
            settings_binding::apply_hotkeys(&mut *r.host.hotkeys.borrow_mut(), &Hotkeys::default());
        assert!(before.is_empty());
        r.ui.invoke_hotkey_recorded(2, "Ctrl+Alt+K".into());
        assert_eq!(
            r.ui.get_hotkeys().row_data(2).unwrap().conflict,
            "Another program already uses Ctrl+Alt+K."
        );
        assert_eq!(r.ctl.borrow().settings().hotkeys.capture, "Ctrl+Shift+2");
        let registered = r.host.hotkeys.borrow().registered.len();
        assert_eq!(registered, 4, "the capture shortcut is still registered");

        // A free one is registered and stored.
        r.ui.invoke_hotkey_recorded(2, "Ctrl+Alt+J".into());
        assert_eq!(r.ctl.borrow().settings().hotkeys.capture, "Ctrl+Alt+J");
        assert!(
            r.host
                .hotkeys
                .borrow()
                .registered
                .iter()
                .any(
                    |(id, chord)| *id == snipdeck_app::host::id_of(HotkeyAction::Capture)
                        && chord.to_string() == "Ctrl+Alt+J"
                )
        );
        // Clearing removes the registration.
        r.ui.invoke_hotkey_cleared(2);
        assert_eq!(r.host.hotkeys.borrow().registered.len(), 3);
    });
}

#[test]
fn while_the_hotkeys_are_paused_nothing_is_registered_but_the_choice_is_kept() {
    on_ui_thread(|| {
        let r = rig();
        let _ = r.ctl.borrow_mut().on_tray(TrayEvent::HotkeysPaused(true));
        r.ui.invoke_hotkey_recorded(2, "Ctrl+Alt+K".into());
        assert!(r.host.hotkeys.borrow().registered.is_empty());
        assert_eq!(r.ctl.borrow().settings().hotkeys.capture, "Ctrl+Alt+K");
    });
}

#[test]
fn start_with_windows_that_cannot_be_changed_says_so_and_keeps_the_old_value() {
    on_ui_thread(|| {
        let r = rig();
        r.host.autostart_fails.set(true);
        r.ui.set_autostart(true);
        r.ui.invoke_autostart_toggled(true);
        assert!(!r.ctl.borrow().settings().autostart);
        assert!(!r.ui.get_autostart(), "the switch went back");
        assert_eq!(
            r.ui.get_toast_text(),
            "Couldn't change the Windows start-up entry."
        );
        assert_eq!(r.ui.get_toast_kind(), ToastKind::Danger);
        r.host.autostart_fails.set(false);
        r.ui.invoke_autostart_toggled(true);
        assert_eq!(r.host.autostart.get(), Some(true));
        assert!(r.ctl.borrow().settings().autostart);
    });
}

// ---- Licence --------------------------------------------------------------------------------

fn token(seed: &[u8; 32], payload: &str) -> String {
    sign_token(&SigningKey::from_bytes(seed), payload)
}

const DEV_SEED: &[u8; 32] = b"snipdeck development licence key";

#[test]
fn a_bad_licence_key_is_rejected_with_a_toast_that_says_what_is_wrong() {
    on_ui_thread(|| {
        use snipdeck_app::toast::LicenceProblem::{
            BadSignature, Empty, NewerFormat, NotAKey, WrongTier,
        };
        let order = pro_payload("TEST-1", 1_760_000_000, None).unwrap();
        let team = r#"{"v":1,"tier":"team","issued":1760000000,"order":"TEST-2"}"#;
        let mut cases = vec![
            ("nothing", String::new(), Empty),
            ("not a key", "please let me in".to_owned(), NotAKey),
            ("two parts", "SNIP1.abc".to_owned(), NotAKey),
            (
                "a newer format",
                "SNIP2.eyJ2IjoyfQ.AAAA".to_owned(),
                NewerFormat,
            ),
            (
                "someone else's signature",
                token(&[7; 32], &order),
                BadSignature,
            ),
        ];
        if IS_DEVELOPMENT_KEY {
            cases.push(("a key for another tier", token(DEV_SEED, team), WrongTier));
        }
        let mut texts = Vec::new();
        let mut problems = Vec::new();
        for (name, key, problem) in cases {
            let r = rig();
            r.ui.invoke_licence_submitted(key.as_str().into());
            let expected = wording::licence_problem_text(problem);
            assert_eq!(r.ui.get_toast_text(), expected, "{name}: the toast");
            assert_eq!(r.ui.get_toast_kind(), ToastKind::Danger, "{name}");
            assert_eq!(
                r.ui.get_licence_error(),
                expected,
                "{name}: under the field"
            );
            assert!(!r.ui.get_pro(), "{name}");
            assert_eq!(r.ctl.borrow().tier(), Tier::Free, "{name}");
            assert!(
                !r.dir.path().join(licence_store::FILE).exists(),
                "{name}: a refused key is not saved"
            );
            texts.push(expected);
            problems.push(problem);
        }
        let mut unique = texts.clone();
        unique.sort_unstable();
        unique.dedup();
        problems.dedup_by_key(|p| format!("{p:?}"));
        problems.sort_by_key(|p| format!("{p:?}"));
        problems.dedup();
        assert_eq!(
            unique.len(),
            problems.len(),
            "every reason has its own words: {texts:?}"
        );
    });
}

#[test]
fn a_good_licence_key_turns_pro_on_and_is_remembered() {
    if !IS_DEVELOPMENT_KEY {
        return;
    }
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_pro());
        let key = token(
            DEV_SEED,
            &pro_payload("TEST-1", 1_760_000_000, None).unwrap(),
        );
        // A pasted key often carries spaces and a line break.
        r.ui.invoke_licence_submitted(format!("  {key}\n").into());
        assert!(r.ui.get_pro());
        assert_eq!(r.ui.get_licence_error(), "");
        assert_eq!(r.ui.get_toast_text(), "Snipdeck Pro is on. Thank you.");
        assert_eq!(r.ui.get_toast_kind(), ToastKind::Success);
        assert_eq!(r.ctl.borrow().tier(), Tier::Pro);
        assert_eq!(
            licence_store::read(r.dir.path()).as_deref(),
            Some(key.as_str())
        );
        assert_eq!(
            licence_store::entitlements(r.dir.path(), now()).tier(),
            Tier::Pro,
            "the next start is Pro"
        );
    });
}

#[test]
fn the_demo_is_pro_and_keeps_nothing_on_disk() {
    on_ui_thread(|| {
        let r = rig_on(
            demo::store(now()).unwrap(),
            Tier::Pro,
            tempfile::tempdir().unwrap(),
            false,
        );
        assert!(r.ui.get_pro());
        assert!(!r.ui.get_has_data_folder());
        r.ui.invoke_backup_now_requested();
        r.ui.invoke_export_requested();
        r.ui.invoke_open_data_folder_requested();
        r.ui.invoke_erase_confirmed("DELETE".into(), true);
        assert!(r.host.opened.borrow().is_empty());
        assert!(!r.dir.path().join("backups").exists());
        assert!(!r.dir.path().join("Documents").exists());
        assert_eq!(
            r.ctl.borrow().store().boards().unwrap().len(),
            2,
            "the demo is untouched"
        );
    });
}

// ---- Backups --------------------------------------------------------------------------------

fn titles(store: &Store, sensitive: bool) -> Vec<String> {
    let mut titles: Vec<String> = store
        .all_items()
        .unwrap()
        .into_iter()
        .filter(|i| i.sensitive == sensitive)
        .map(|i| i.title)
        .collect();
    titles.sort();
    titles
}

fn board_names(store: &Store) -> Vec<String> {
    let mut names: Vec<String> = store
        .boards()
        .unwrap()
        .iter()
        .map(|b| b.name.as_str().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn restoring_a_backup_round_trips_the_demo_library() {
    on_ui_thread(|| {
        // Back up the demo library.
        let a = rig_on(
            demo::store(now()).unwrap(),
            Tier::Pro,
            tempfile::tempdir().unwrap(),
            true,
        );
        assert_eq!(a.ui.get_backups().row_count(), 0);
        a.ui.invoke_backup_now_requested();
        assert_eq!(a.ui.get_toast_text(), "Backed up");
        assert_eq!(
            a.ui.get_backups().row_count(),
            1,
            "the list shows the new backup"
        );
        let row = a.ui.get_backups().row_data(0).unwrap();
        assert!(
            row.size.ends_with("KB") || row.size.ends_with(" B"),
            "{:?}",
            row.size
        );
        let boards = board_names(a.ctl.borrow().store());
        let plain = titles(a.ctl.borrow().store(), false);
        assert!(!plain.is_empty());
        assert!(boards.len() >= 2);

        // Restore it into an empty library, by the Restore button of that backup.
        let dir = a.dir;
        let b = rig_on(Store::open_in_memory().unwrap(), Tier::Pro, dir, true);
        assert_eq!(
            b.ui.get_backups().row_count(),
            1,
            "the page found the backup on disk"
        );
        assert!(b.ctl.borrow().store().boards().unwrap().is_empty());
        b.ui.invoke_backup_restore_requested(0);
        assert_eq!(
            b.ui.get_toast_text(),
            format!("Restored {} items from the backup", plain.len())
        );
        assert_eq!(b.ui.get_toast_kind(), ToastKind::Success);
        assert_eq!(board_names(b.ctl.borrow().store()), boards);
        assert_eq!(titles(b.ctl.borrow().store(), false), plain);
        assert!(
            titles(b.ctl.borrow().store(), true).is_empty(),
            "sensitive items are not in a backup made without a key"
        );

        // Restoring over an intact library adds no item twice.
        b.ui.invoke_backup_restore_requested(0);
        assert_eq!(
            b.ui.get_toast_text(),
            "Nothing to restore: your library already has everything in that backup."
        );
        assert_eq!(titles(b.ctl.borrow().store(), false), plain);
    });
}

#[test]
fn back_up_now_replaces_todays_backup_instead_of_refusing() {
    on_ui_thread(|| {
        let r = rig_on(
            demo::store(now()).unwrap(),
            Tier::Pro,
            tempfile::tempdir().unwrap(),
            true,
        );
        r.ui.invoke_backup_now_requested();
        let files = snipdeck_portable::backup::list_backups(r.dir.path()).unwrap();
        assert_eq!(files.len(), 1);
        // Today's file is stale (stand-in: three bytes); asking again writes a real one over it.
        std::fs::write(&files[0].path, b"old").unwrap();
        r.ui.invoke_backup_now_requested();
        assert_eq!(r.ui.get_toast_text(), "Backed up");
        let after = snipdeck_portable::backup::list_backups(r.dir.path()).unwrap();
        assert_eq!(after.len(), 1, "still one file for today");
        assert!(after[0].bytes > 100, "{} bytes", after[0].bytes);
        assert_eq!(r.ui.get_backups().row_count(), 1);
    });
}

#[test]
fn export_saves_a_new_file_in_documents_each_time_and_opens_the_folder() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_export_requested();
        let documents = r.dir.path().join("Documents");
        let files = |dir: &Path| {
            let mut names: Vec<String> = std::fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        };
        let first = files(&documents);
        assert_eq!(first.len(), 1);
        assert!(first[0].starts_with("Snipdeck-library-") && first[0].ends_with(".snipdeck"));
        assert_eq!(
            r.ui.get_toast_text(),
            format!("Saved {} in your Documents folder", first[0])
        );
        assert_eq!(*r.host.opened.borrow(), std::slice::from_ref(&documents));
        r.ui.invoke_export_requested();
        assert_eq!(
            files(&documents).len(),
            2,
            "never overwrites an earlier export"
        );
        r.ui.invoke_open_data_folder_requested();
        assert_eq!(r.host.opened.borrow().last().unwrap(), r.dir.path());
    });
}

// ---- Erase library --------------------------------------------------------------------------

/// A library on disk with the demo in it, a backup, the settings, a licence and a crash log.
fn library_on_disk(with_key: bool) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).expect("store");
    demo::seed(&mut store, now()).unwrap();
    if with_key {
        let provider = MemoryKeyProvider::new().unwrap();
        store
            .create_key_ring(&KeyRing::new(&provider), None)
            .expect("key ring");
    }
    let r = rig_on(store, Tier::Pro, dir, true);
    std::fs::write(r.dir.path().join("licence.key"), "SNIP1.kept.kept").unwrap();
    std::fs::create_dir_all(r.dir.path().join("logs")).unwrap();
    std::fs::write(r.dir.path().join("logs").join("crash-1.txt"), "crash").unwrap();
    r.ui.invoke_backup_now_requested();
    // The settings file exists after a change.
    r.ui.invoke_site_icons_toggled(true);
    r.advance(500);
    assert!(r.settings_file().exists());
    assert_eq!(r.ui.get_backups().row_count(), 1);
    r
}

fn left_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn erasing_the_library_needs_exactly_the_word_delete() {
    on_ui_thread(|| {
        let r = library_on_disk(false);
        let boards = board_names(r.ctl.borrow().store());
        let before = left_in(r.dir.path());
        for typed in [
            "", "delete", "Delete", "DELETE ", " DELETE", "DELET", "yes", "DELETE\n",
        ] {
            r.ui.invoke_erase_confirmed(typed.into(), true);
            assert_eq!(
                board_names(r.ctl.borrow().store()),
                boards,
                "{typed:?} erased nothing"
            );
            assert_eq!(left_in(r.dir.path()), before, "{typed:?} deleted no file");
        }
        assert!(!r.ui.get_toast_text().contains("erased"));

        r.ui.invoke_erase_confirmed("DELETE".into(), true);
        assert_eq!(r.ui.get_toast_text(), "Your library was erased.");
        assert_eq!(r.ui.get_toast_kind(), ToastKind::Success);
        // A new empty library with the starter board is running.
        assert_eq!(board_names(r.ctl.borrow().store()), ["My snippets"]);
        assert!(r.ctl.borrow().store().all_items().unwrap().is_empty());
        // The backups went; the settings, the licence and the logs stayed.
        assert_eq!(r.ui.get_backups().row_count(), 0);
        assert!(!r.dir.path().join("backups").exists());
        assert!(r.settings_file().exists());
        assert!(load_settings(r.dir.path()).site_icons, "settings stay");
        assert_eq!(
            licence_store::read(r.dir.path()).as_deref(),
            Some("SNIP1.kept.kept")
        );
        assert!(r.dir.path().join("logs").join("crash-1.txt").exists());
        // And a restart finds a library that opens.
        r.ui.hide().unwrap();
        drop(r.ui);
        drop(r.binding);
        drop(r.ctl);
        let reopened = Store::open(r.dir.path()).expect("the data folder opens again");
        assert_eq!(board_names(&reopened), ["My snippets"]);
    });
}

#[test]
fn erasing_can_keep_the_encryption_key_or_forget_it() {
    on_ui_thread(|| {
        let keep = library_on_disk(true);
        assert!(keep.ctl.borrow().store().key_ring_exists().unwrap());
        keep.ui.invoke_erase_confirmed("DELETE".into(), false);
        assert!(
            keep.ctl.borrow().store().key_ring_exists().unwrap(),
            "the key record was carried over"
        );
        assert!(keep.ctl.borrow().store().all_items().unwrap().is_empty());
        keep.ui.hide().unwrap();
        drop(keep.ui);
        drop(keep.binding);
        drop(keep.ctl);
        assert!(
            Store::open(keep.dir.path())
                .unwrap()
                .key_ring_exists()
                .unwrap()
        );

        let forget = library_on_disk(true);
        forget.ui.invoke_erase_confirmed("DELETE".into(), true);
        assert!(!forget.ctl.borrow().store().key_ring_exists().unwrap());
    });
}

#[test]
fn the_erase_button_stays_off_until_the_word_is_typed() {
    on_ui_thread(|| {
        let r = library_on_disk(false);
        assert!(!r.ui.get_erase_open());
        element(&r.ui, "Erase library\u{2026}", AccessibleRole::Button)
            .invoke_accessible_default_action();
        assert!(r.ui.get_erase_open(), "the question opens first");
        // Nothing else happened yet.
        assert_eq!(board_names(r.ctl.borrow().store()).len(), 2);

        let field = element(&r.ui, "Type DELETE to confirm", AccessibleRole::TextInput);
        let erase = element(&r.ui, "Erase everything", AccessibleRole::Button);
        assert_eq!(erase.accessible_enabled(), Some(false));
        field.set_accessible_value("delete");
        assert_eq!(
            erase.accessible_enabled(),
            Some(false),
            "lower case is not the word"
        );
        field.set_accessible_value("DELETE");
        assert_eq!(erase.accessible_enabled(), Some(true));

        // The key question is a switch that starts off.
        let forget = element(
            &r.ui,
            "Also forget the encryption key",
            AccessibleRole::Switch,
        );
        assert_eq!(forget.accessible_checked(), Some(false));

        // Cancel closes it without erasing.
        element(&r.ui, "Cancel", AccessibleRole::Button).invoke_accessible_default_action();
        assert!(!r.ui.get_erase_open());
        assert_eq!(board_names(r.ctl.borrow().store()).len(), 2);

        // Typing the word and pressing the button erases.
        element(&r.ui, "Erase library\u{2026}", AccessibleRole::Button)
            .invoke_accessible_default_action();
        element(&r.ui, "Type DELETE to confirm", AccessibleRole::TextInput)
            .set_accessible_value("DELETE");
        element(&r.ui, "Erase everything", AccessibleRole::Button)
            .invoke_accessible_default_action();
        assert_eq!(board_names(r.ctl.borrow().store()), ["My snippets"]);
        assert!(
            !r.ui.get_erase_open(),
            "the question closes after the erase"
        );
    });
}

#[test]
fn the_erase_word_on_the_page_is_the_one_the_host_checks() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_erase_word(), settings_binding::ERASE_WORD);
    });
}

// ---- Opening ---------------------------------------------------------------------------------

#[test]
fn the_main_windows_settings_button_opens_the_page_once_and_shows_it_again() {
    on_ui_thread(|| {
        let clock = FixedClock::new(now());
        let ctl = controller_on(demo::store(now()).unwrap(), Tier::Pro, &clock);
        let host: Rc<dyn SettingsHost> = Rc::new(FakeHost::default());
        let settings_ui = SettingsUi::new(&ctl, None, host);
        let main_ui = MainUi::new(&ctl);
        assert!(
            settings_ui.window().is_none(),
            "not created before it is asked for"
        );

        main_ui.open().unwrap();
        main_ui.window().unwrap().invoke_settings_requested();
        let page = settings_ui.window().expect("created");
        assert!(page.window().is_visible());
        assert_eq!(page.get_hotkeys().row_count(), 4);

        page.window().hide().unwrap();
        assert!(!page.window().is_visible());
        request_open();
        let again = settings_ui.window().unwrap();
        assert!(again.window().is_visible());
        assert_eq!(
            again.window().size(),
            page.window().size(),
            "the same window"
        );
        assert_eq!(again.get_hotkeys().row_count(), 4);
    });
}
