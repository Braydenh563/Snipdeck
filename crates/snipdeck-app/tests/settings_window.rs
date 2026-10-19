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
use slint::{ComponentHandle, Global, Model, platform::Key};
use snipdeck_app::{
    controller::{Controller, Effect, MasterState, Services},
    demo,
    frame::{Monitor, Rect},
    host::{FakeHotkeys, FakeTargets, FixedClock, HotkeyProblem},
    licence_store,
    main_binding::MainUi,
    paths::load_settings,
    settings::{DockEdge, DockTrigger, HotkeyAction, Hotkeys, Settings, Theme},
    settings_binding::{self, SettingsBinding, SettingsHost, SettingsUi, request_open},
    settings_store::{FileSink, SettingsSaver},
    wording,
};
use snipdeck_core::{
    Entitlements, Feature, Tier, Timestamp,
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
use support::{key, on_ui_thread, press};

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
    /// The last "Panel taskbar button" the page asked for.
    taskbar: Cell<Option<bool>>,
    opened: RefCell<Vec<PathBuf>>,
    /// The links the page asked to open.
    urls: RefCell<Vec<String>>,
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

    fn set_panel_in_taskbar(&self, on: bool) {
        self.taskbar.set(Some(on));
    }

    fn open_folder(&self, folder: &Path) -> Result<(), String> {
        self.opened.borrow_mut().push(folder.to_owned());
        Ok(())
    }

    fn open_url(&self, url: &str) -> Result<(), String> {
        self.urls.borrow_mut().push(url.to_owned());
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
    ui.window().set_size(slint::LogicalSize::new(640.0, 4000.0));
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
        let cases: [Case; 14] = [
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
                "panel in the taskbar",
                |r| r.ui.invoke_panel_in_taskbar_toggled(true),
                |s| s.panel_in_taskbar,
            ),
            (
                "hide the panel on blur",
                |r| r.ui.invoke_panel_hide_on_blur_toggled(false),
                |s| !s.panel_hide_on_blur,
            ),
            (
                "backups to keep",
                |r| r.ui.invoke_backups_keep_chosen(12),
                |s| s.backups_keep == 12,
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
            (
                "the dock trigger",
                |r| r.ui.invoke_dock_trigger_chosen(3),
                |s| s.dock.trigger == DockTrigger::Both,
            ),
            (
                "the hover delay",
                |r| r.ui.invoke_dock_hover_delay_chosen(300),
                |s| s.dock.hover_delay_ms == 300,
            ),
            (
                "the leave delay",
                |r| r.ui.invoke_dock_leave_delay_chosen(1000),
                |s| s.dock.leave_delay_ms == 1000,
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
        r.ui.invoke_close_to_tray_toggled(true);
        assert_eq!(r.ui.get_language_index(), 6);
        assert_eq!(r.ui.get_panel_width(), 300);
        assert_eq!(
            r.ui.get_languages().row_count(),
            8,
            "Follow Windows and seven languages"
        );
    });
}

#[test]
fn the_dock_group_shows_what_is_stored_and_holds_the_delays_in_range() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(
            (
                r.ui.get_dock_edge_index(),
                r.ui.get_dock_trigger_index(),
                r.ui.get_dock_hover_delay(),
                r.ui.get_dock_leave_delay()
            ),
            (0, 0, 150, 400),
            "floating, off, 150 ms and 400 ms by default"
        );
        r.ui.invoke_dock_trigger_chosen(2);
        r.ui.invoke_dock_hover_delay_chosen(99_999);
        r.ui.invoke_dock_leave_delay_chosen(-5);
        assert_eq!(r.ui.get_dock_trigger_index(), 2);
        assert_eq!(r.ui.get_dock_hover_delay(), 2000, "the cap");
        assert_eq!(r.ui.get_dock_leave_delay(), 0, "the floor");
        assert_eq!(r.ctl.borrow().settings().dock.trigger, DockTrigger::Click);
        // The control the group is found by.
        element(&r.ui, "Dock to", AccessibleRole::Combobox);
    });
}

#[test]
fn choosing_an_edge_docks_the_panel_there_and_floating_lets_it_go() {
    on_ui_thread(|| {
        let r = rig();
        let monitor = Monitor {
            id: Monitor::id_of("DISPLAY1", 1920, 1080),
            work_area: Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            },
            scale: 1.0,
        };
        let floating = Rect {
            x: 600,
            y: 100,
            w: 320,
            h: 480,
        };
        r.ctl.borrow_mut().frame_changed(&monitor, floating);

        r.ui.invoke_dock_edge_chosen(2);
        assert_eq!(r.ui.get_dock_edge_index(), 2, "the page follows");
        assert_eq!(r.ctl.borrow().dock_edge(), Some(DockEdge::Right));
        let docked = Rect {
            x: 1600,
            y: 0,
            w: 320,
            h: 1040,
        };
        assert_eq!(
            r.ctl.borrow_mut().take_pending(),
            [Effect::PlaceWindow(docked)],
            "queued for the Panel's window"
        );

        r.ui.invoke_dock_edge_chosen(0);
        assert_eq!(r.ui.get_dock_edge_index(), 0);
        assert_eq!(r.ctl.borrow().dock_edge(), None);
        r.advance(500);
        let stored = load_settings(r.dir.path()).panel_geometry;
        assert_eq!(
            stored[0].edge, None,
            "the floating Panel is stored without an edge"
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
        assert_eq!(before.len(), 0);
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
        assert_eq!(r.host.hotkeys.borrow().registered.len(), 0);
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

#[test]
fn the_window_behaviour_toggles_start_at_their_defaults_and_reach_the_host() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_panel_in_taskbar(), "off by default");
        assert!(r.ui.get_panel_hide_on_blur(), "on by default");

        r.ui.invoke_panel_in_taskbar_toggled(true);
        assert_eq!(
            r.host.taskbar.get(),
            Some(true),
            "the Panel's window is told"
        );
        assert!(r.ctl.borrow().settings().panel_in_taskbar);
        assert!(r.ui.get_panel_in_taskbar());
        r.ui.invoke_panel_in_taskbar_toggled(false);
        assert_eq!(r.host.taskbar.get(), Some(false));

        r.ui.invoke_panel_hide_on_blur_toggled(false);
        assert!(!r.ctl.borrow().settings().panel_hide_on_blur);
        assert!(!r.ui.get_panel_hide_on_blur());
    });
}

#[test]
fn backups_to_keep_is_what_the_prune_keeps() {
    on_ui_thread(|| {
        let r = rig();
        r.ui.invoke_backups_keep_chosen(2);
        assert_eq!(r.ctl.borrow().settings().backups_keep, 2);
        assert_eq!(r.ui.get_backups_keep(), 2);
        // Four days of backups on the frozen clock, moved on a day at a time.
        for day in 0..4 {
            r.clock
                .set(at(&format!("2026-10-{:02}T10:30:00Z", 17 + day)));
            assert!(r.ctl.borrow_mut().backup_now(r.dir.path()));
        }
        let files = std::fs::read_dir(r.dir.path().join("backups"))
            .unwrap()
            .count();
        assert_eq!(files, 2, "only the newest two stay");
        // A bigger number is held between 1 and 60.
        r.ui.invoke_backups_keep_chosen(900);
        assert_eq!(r.ctl.borrow().settings().backups_keep, 60);
    });
}

#[test]
fn the_reduce_motion_switch_turns_animation_off_in_the_page_itself() {
    on_ui_thread(|| {
        let r = rig();
        let look = r.ui.global::<snipdeck_ui::SystemLook<'_>>();
        assert!(!r.ui.get_reduce_motion());
        assert!(!look.get_reduced_motion());
        r.ui.invoke_reduce_motion_toggled(true);
        assert!(r.ctl.borrow().settings().reduce_motion);
        assert!(r.ui.get_reduce_motion());
        assert!(look.get_reduced_motion());
        r.ui.invoke_reduce_motion_toggled(false);
        assert!(!look.get_reduced_motion());
    });
}

#[test]
fn the_clipboard_clear_time_is_held_between_zero_and_ten_minutes() {
    on_ui_thread(|| {
        let r = rig();
        assert_eq!(r.ui.get_clipboard_clear_secs(), 30);
        r.ui.invoke_clipboard_clear_secs_chosen(0);
        assert_eq!(r.ctl.borrow().settings().clipboard_clear_secs, 0);
        assert_eq!(r.ui.get_clipboard_clear_secs(), 0);
        r.ui.invoke_clipboard_clear_secs_chosen(99_999);
        assert_eq!(r.ctl.borrow().settings().clipboard_clear_secs, 600);
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
        assert_eq!(
            r.ui.get_toast_text(),
            "Snipdeck Supporter is on. Thank you."
        );
        assert_eq!(r.ui.get_toast_kind(), ToastKind::Success);
        assert_eq!(r.ctl.borrow().tier(), Tier::Pro);
        assert_eq!(
            licence_store::read(r.dir.path()).as_deref(),
            Some(key.as_str())
        );
        assert_eq!(
            licence_store::entitlements(r.dir.path(), now()).tier(),
            Tier::Pro,
            "the next start is Supporter"
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
        r.ui.invoke_open_backups_folder_requested();
        r.ui.invoke_erase_confirmed("DELETE".into(), true);
        assert_eq!(r.host.opened.borrow().len(), 0);
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
        assert_eq!(b.ctl.borrow().store().boards().unwrap().len(), 0);
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
        r.ui.invoke_open_backups_folder_requested();
        assert_eq!(
            r.host.opened.borrow().last().unwrap(),
            &r.dir.path().join("backups")
        );
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
    r.ui.invoke_close_to_tray_toggled(false);
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
        assert_eq!(r.ctl.borrow().store().all_items().unwrap().len(), 0);
        // The backups went; the settings, the licence and the logs stayed.
        assert_eq!(r.ui.get_backups().row_count(), 0);
        assert!(!r.dir.path().join("backups").exists());
        assert!(r.settings_file().exists());
        assert!(!load_settings(r.dir.path()).close_to_tray, "settings stay");
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
        assert_eq!(keep.ctl.borrow().store().all_items().unwrap().len(), 0);
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
        // The page scrolls, and the accessibility queries only see what is inside the window:
        // make it tall enough for the whole confirmation (the Licence section has the plan table).
        r.ui.window()
            .set_size(slint::LogicalSize::new(640.0, 4000.0));
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

#[test]
fn get_supporter_opens_the_email_app_with_the_one_named_address() {
    on_ui_thread(|| {
        let r = rig();
        assert!(!r.ui.get_supporter_open());
        r.ui.invoke_supporter_requested();
        assert_eq!(
            *r.host.urls.borrow(),
            [format!("mailto:{}", settings_binding::SUPPORTER_EMAIL)]
        );
        // Pinned on purpose: this is the placeholder, so it fails the day the real address goes
        // into `SUPPORTER_EMAIL`. Change it together with that constant (and `site/index.html`).
        assert_eq!(
            settings_binding::supporter_mailto(),
            "mailto:SUPPORTER_EMAIL_PLACEHOLDER",
            "the placeholder stays until the owner supplies the address"
        );
    });
}

#[test]
fn the_free_is_fully_usable_line_is_readable_by_a_screen_reader() {
    on_ui_thread(|| {
        let r = rig();
        element(
            &r.ui,
            "Free is fully usable. Supporter pays for convenience and supports the app.",
            AccessibleRole::Text,
        );
    });
}

/// The whole path a person takes: the button opens the dialog, Escape closes it with the focus
/// back on the button (WCAG 2.4.3), and "Email me" hands the mailto link to the host.
#[test]
fn the_get_supporter_dialog_opens_closes_with_escape_and_emails() {
    on_ui_thread(|| {
        let r = rig();
        let button = || element(&r.ui, "Get Supporter\u{2026}", AccessibleRole::Button);
        let settle = slint::platform::update_timers_and_animations;
        assert!(!r.ui.get_supporter_open());
        assert_eq!(button().accessible_enabled(), Some(true));

        button().invoke_accessible_default_action();
        settle();
        assert!(r.ui.get_supporter_open(), "the button opens the dialog");
        assert_eq!(button().accessible_enabled(), Some(false), "and goes off");

        // The comparison table inside the dialog is laid out and sits within the card.
        // The dialog card is the group that also carries the dialog's body text.
        let card = ElementHandle::find_by_accessible_label(&r.ui, "Get Supporter")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Groupbox))
            .find(|e| {
                e.accessible_description()
                    .is_some_and(|d| d.contains("Can't afford it"))
            })
            .expect("the dialog card");
        let (cp, cs) = (card.absolute_position(), card.size());
        let table = ElementHandle::find_by_accessible_label(&r.ui, "Free and Supporter compared")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Groupbox))
            .find(|e| {
                let p = e.absolute_position();
                p.x >= cp.x && p.y >= cp.y && p.y <= cp.y + cs.height
            })
            .expect("a plan table inside the dialog card");
        let (tp, ts) = (table.absolute_position(), table.size());
        assert!(cs.width > 0.0 && cs.height > 0.0, "the card has a size");
        assert!(
            ts.width > 0.0 && ts.height > 0.0,
            "the table has a size: {ts:?}"
        );
        assert!(
            tp.x >= cp.x && tp.x + ts.width <= cp.x + cs.width,
            "the table ({tp:?} {ts:?}) is inside the card ({cp:?} {cs:?}) across"
        );
        assert!(
            tp.y >= cp.y && tp.y + ts.height <= cp.y + cs.height,
            "the table ({tp:?} {ts:?}) is inside the card ({cp:?} {cs:?}) down"
        );

        press(r.ui.window(), &key(Key::Escape));
        settle();
        assert!(!r.ui.get_supporter_open(), "Escape closes it");
        assert_eq!(button().accessible_enabled(), Some(true));
        assert!(r.host.urls.borrow().is_empty(), "closing sends nothing");

        // The focus is back on the button: Enter presses it again.
        press(r.ui.window(), "\n");
        settle();
        assert!(
            r.ui.get_supporter_open(),
            "after Escape the focus is on the button that opened the dialog"
        );

        element(&r.ui, "Email me", AccessibleRole::Button).invoke_accessible_default_action();
        settle();
        assert!(!r.ui.get_supporter_open(), "Email me closes the dialog");
        assert_eq!(
            *r.host.urls.borrow(),
            [settings_binding::supporter_mailto()],
            "and opens the mailto link, once"
        );
    });
}

/// The comparison table is held to `entitlements.rs`: its Supporter-only rows are exactly the
/// `Feature`s, except floating lists (in the enum, not part of v1, so not on the website either).
#[test]
fn the_plan_table_lists_exactly_the_supporter_features() {
    on_ui_thread(|| {
        let r = rig();
        let rows: Vec<_> = snipdeck_ui::PlanTexts::get(&r.ui)
            .get_rows()
            .iter()
            .collect();
        let mut listed: Vec<String> = rows
            .iter()
            .filter(|row| !row.feature.is_empty())
            .map(|row| row.feature.to_string())
            .collect();
        let mut expected: Vec<String> = Feature::ALL
            .iter()
            .filter(|f| **f != Feature::FloatingLists)
            .map(|f| f.key().to_owned())
            .collect();
        listed.sort();
        expected.sort();
        assert_eq!(listed, expected);
        for row in &rows {
            assert!(row.supporter, "{}: Supporter has everything", row.name);
            assert_eq!(
                row.free,
                row.feature.is_empty() || row.feature == Feature::UnlimitedBoards.key(),
                "{}: Free lacks exactly the Supporter features (boards are limited, not missing)",
                row.name
            );
            assert!(!row.name.is_empty() && !row.description.is_empty());
        }
        let boards = rows
            .iter()
            .find(|row| row.feature == Feature::UnlimitedBoards.key())
            .expect("boards row");
        assert_eq!(
            boards.free_note,
            format!("up to {}", Entitlements::FREE_BOARD_LIMIT)
        );
    });
}

#[test]
fn the_plan_table_says_each_answer_in_words_not_colour() {
    on_ui_thread(|| {
        let r = rig();
        element(
            &r.ui,
            "Free and Supporter compared",
            AccessibleRole::Groupbox,
        );
        // A row says both answers in its description.
        let row = element(&r.ui, "Paste stack", AccessibleRole::Groupbox);
        let said = row
            .accessible_description()
            .expect("description")
            .to_string();
        assert!(said.contains("Free: Not included."), "{said}");
        assert!(said.contains("Supporter: Included."), "{said}");
        let boards = element(&r.ui, "Boards", AccessibleRole::Groupbox);
        let said = boards
            .accessible_description()
            .expect("description")
            .to_string();
        assert!(said.contains("Free: Included, up to 5."), "{said}");
        assert!(said.contains("Supporter: Included, unlimited."), "{said}");
    });
}

// ---- Master password (SPEC 5.7, V1-PLAN C3) ----------------------------------------------------

const PASSWORD: &str = "tulip harbour 42";

/// A page over the demo library with a machine that can protect a key, on `tier`, with a cheap
/// Argon2id so each set or unlock takes milliseconds.
fn master_rig(tier: Tier) -> Rig {
    let r = rig_on(
        demo::store(now()).expect("demo"),
        tier,
        tempfile::tempdir().expect("dir"),
        true,
    );
    {
        let mut ctl = r.ctl.borrow_mut();
        ctl.set_key_provider(Box::new(MemoryKeyProvider::new().expect("provider")));
        ctl.set_kdf_params(
            snipdeck_store::crypto::KdfParams::insecure_for_tests(64, 1, 1).expect("params"),
        );
    }
    r.binding.redraw();
    slint::platform::update_timers_and_animations();
    r
}

/// The Privacy section's row (the plan table has a row of the same name).
fn master_row(r: &Rig) -> ElementHandle {
    ElementHandle::find_by_accessible_label(&r.ui, "Master password")
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Groupbox))
        .find(|e| {
            e.accessible_description()
                .is_some_and(|d| !d.starts_with("Lock your encrypted items"))
        })
        .expect("the master password row")
}

fn click(r: &Rig, label: &str) {
    element(&r.ui, label, AccessibleRole::Button).invoke_accessible_default_action();
    slint::platform::update_timers_and_animations();
}

fn type_in(r: &Rig, label: &str, text: &str) {
    element(&r.ui, label, AccessibleRole::TextInput).set_accessible_value(text);
}

fn texts(r: &Rig, text: &str) -> usize {
    ElementHandle::find_by_accessible_label(&r.ui, text)
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Text))
        .count()
}

#[test]
fn a_free_user_sees_the_master_password_row_with_a_supporter_note_and_no_way_to_set_it() {
    on_ui_thread(|| {
        let r = master_rig(Tier::Free);
        let row = master_row(&r);
        let note = row.accessible_description().expect("a description");
        assert!(note.contains("A Supporter feature"), "{note}");
        assert!(note.contains("15 minutes"), "{note}");
        let set = element(&r.ui, "Set password\u{2026}", AccessibleRole::Button);
        assert_eq!(set.accessible_enabled(), Some(false));
        set.invoke_accessible_default_action();
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Closed);
        assert_eq!(r.ctl.borrow().master_password_state(), MasterState::NotSet);
    });
}

#[test]
fn a_machine_without_key_protection_says_so_and_offers_nothing() {
    on_ui_thread(|| {
        let r = rig_on(
            demo::store(now()).expect("demo"),
            Tier::Pro,
            tempfile::tempdir().expect("dir"),
            true,
        );
        let note = master_row(&r)
            .accessible_description()
            .expect("a description");
        assert!(note.contains("can't protect a key"), "{note}");
        assert_eq!(
            element(&r.ui, "Set password\u{2026}", AccessibleRole::Button).accessible_enabled(),
            Some(false)
        );
    });
}

#[test]
fn a_supporter_sets_a_master_password_through_the_form() {
    on_ui_thread(|| {
        let r = master_rig(Tier::Pro);
        click(&r, "Set password\u{2026}");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Set);

        // The form says what forgetting costs, in words, before asking for anything, and a screen
        // reader gets it as the group's description.
        let form = element(&r.ui, "Set master password", AccessibleRole::Groupbox);
        let warning = form.accessible_description().expect("a description");
        assert!(warning.contains("can never be opened again"), "{warning}");
        assert!(warning.contains("keeps no recovery key"), "{warning}");

        // A short one, then two that differ: each says which problem in words and nothing is set.
        type_in(&r, "New password", "short");
        type_in(&r, "Repeat the new password", "short");
        click(&r, "Set password");
        assert_eq!(texts(&r, "Use at least 8 characters."), 1);
        assert_eq!(r.ctl.borrow().master_password_state(), MasterState::NotSet);
        type_in(&r, "New password", PASSWORD);
        type_in(&r, "Repeat the new password", "tulip harbour 43");
        click(&r, "Set password");
        assert_eq!(texts(&r, "The two passwords are not the same."), 1);
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Set);

        // The strength hint is words that follow the typing.
        assert_eq!(texts(&r, "Strength: strong"), 1, "a 16 character phrase");
        type_in(&r, "New password", "short");
        assert_eq!(texts(&r, "Strength: weak"), 1);
        type_in(&r, "New password", PASSWORD);

        type_in(&r, "Repeat the new password", PASSWORD);
        click(&r, "Set password");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Closed);
        assert_eq!(r.ui.get_master_state(), 2, "the row now shows it is on");
        assert_eq!(r.ctl.borrow().master_password_state(), MasterState::Set);
        assert!(
            r.ui.get_toast_text().contains("Master password set"),
            "{}",
            r.ui.get_toast_text()
        );
        element(&r.ui, "Change\u{2026}", AccessibleRole::Button);
        element(&r.ui, "Remove\u{2026}", AccessibleRole::Button);
    });
}

#[test]
fn passwords_are_hidden_from_the_accessibility_tree() {
    on_ui_thread(|| {
        let r = master_rig(Tier::Pro);
        click(&r, "Set password\u{2026}");
        type_in(&r, "New password", PASSWORD);
        let field = element(&r.ui, "New password", AccessibleRole::TextInput);
        assert_ne!(
            field.accessible_value().as_deref(),
            Some(PASSWORD),
            "a screen reader is not handed the password"
        );
        assert!(field.accessible_value().is_none_or(|v| v.is_empty()));
    });
}

#[test]
fn changing_and_removing_ask_for_the_current_password_and_say_when_it_is_wrong() {
    on_ui_thread(|| {
        let r = master_rig(Tier::Pro);
        r.ctl
            .borrow_mut()
            .set_master_password(PASSWORD, PASSWORD)
            .expect("set");
        r.binding.redraw();
        slint::platform::update_timers_and_animations();

        click(&r, "Change\u{2026}");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Change);
        let new = "marble orchard 77";
        type_in(&r, "Current password", "not the password");
        type_in(&r, "New password", new);
        type_in(&r, "Repeat the new password", new);
        click(&r, "Change password");
        assert_eq!(texts(&r, "That password is not right. Try again."), 1);
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Change);
        type_in(&r, "Current password", PASSWORD);
        click(&r, "Change password");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Closed);
        assert!(r.ui.get_toast_text().contains("changed"));

        click(&r, "Remove\u{2026}");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Remove);
        assert_eq!(
            ElementHandle::find_by_accessible_label(&r.ui, "New password").count(),
            0,
            "removing asks only for the current password"
        );
        click(&r, "Remove password");
        assert_eq!(texts(&r, "Enter your password."), 1, "empty is told apart");
        type_in(&r, "Current password", PASSWORD);
        click(&r, "Remove password");
        assert_eq!(
            texts(&r, "That password is not right. Try again."),
            1,
            "the old password is gone after the change"
        );
        type_in(&r, "Current password", new);
        click(&r, "Remove password");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Closed);
        assert_eq!(r.ui.get_master_state(), 1);
        assert_eq!(r.ctl.borrow().master_password_state(), MasterState::NotSet);
    });
}

#[test]
fn escape_closes_only_the_form_and_the_focus_goes_back_to_its_button() {
    on_ui_thread(|| {
        let r = master_rig(Tier::Pro);
        click(&r, "Set password\u{2026}");
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Set);
        // The first field has the focus: typing goes into it.
        support::type_text(r.ui.window(), "abc");
        assert_eq!(
            element(&r.ui, "New password", AccessibleRole::TextInput).accessible_value().as_deref(),
            Some(""),
            "masked: the value is not exposed, but the field took the keys"
        );
        press(r.ui.window(), &key(Key::Escape));
        slint::platform::update_timers_and_animations();
        assert_eq!(r.ui.get_master_mode(), snipdeck_ui::MasterMode::Closed);
        assert!(r.ui.window().is_visible(), "Escape did not close the page");
        // What was typed is gone with the form.
        // The focus is on the button that opened it: Enter opens the form again.
        press(r.ui.window(), "\n");
        slint::platform::update_timers_and_animations();
        assert_eq!(
            r.ui.get_master_mode(),
            snipdeck_ui::MasterMode::Set,
            "focus came back to the Set password button"
        );
        assert_eq!(
            element(&r.ui, "New password", AccessibleRole::TextInput)
                .accessible_value()
                .as_deref(),
            Some(""),
        );
    });
}

#[test]
fn a_locked_library_offers_unlock_in_the_row_and_asks_for_the_prompt() {
    on_ui_thread(|| {
        let r = master_rig(Tier::Pro);
        {
            let mut ctl = r.ctl.borrow_mut();
            ctl.set_master_password(PASSWORD, PASSWORD).expect("set");
            ctl.lock_now();
        }
        r.binding.redraw();
        slint::platform::update_timers_and_animations();
        let note = master_row(&r)
            .accessible_description()
            .expect("a description");
        assert!(note.contains("locked"), "{note}");
        click(&r, "Unlock\u{2026}");
        let mut ctl = r.ctl.borrow_mut();
        assert!(ctl.unlock_prompt_open());
        assert!(ctl.take_pending().contains(&Effect::OpenUnlock));
    });
}
