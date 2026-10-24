//! The OS theme and the settings writes.

use super::*;
use crate::toast::ToastMessage;

// ---- The OS theme ------------------------------------------------------------------------

fn os(dark: bool) -> SystemLook {
    SystemLook {
        dark,
        high_contrast: None,
        accent: None,
        reduce_motion: false,
    }
}

#[test]
fn the_system_theme_follows_windows_and_light_and_dark_ignore_it() {
    let mut w = world();
    assert!(!w.ctl.view().dark, "light until told otherwise");
    w.ctl.set_system_look(os(true));
    assert!(w.ctl.view().dark, "theme = system follows Windows");
    w.ctl.set_system_look(os(false));
    assert!(!w.ctl.view().dark);

    w.ctl.set_theme(Theme::Light);
    w.ctl.set_system_look(os(true));
    assert!(!w.ctl.view().dark, "light stays light");
    w.ctl.set_theme(Theme::Dark);
    w.ctl.set_system_look(os(false));
    assert!(w.ctl.view().dark, "dark stays dark");
}

#[test]
fn motion_is_reduced_by_the_setting_or_by_windows() {
    let mut w = world();
    assert!(!w.ctl.view().reduced_motion);
    w.ctl.update_settings(|s| s.reduce_motion = true);
    assert!(w.ctl.view().reduced_motion, "the setting alone");
    w.ctl.update_settings(|s| s.reduce_motion = false);
    w.ctl.set_system_look(SystemLook {
        reduce_motion: true,
        ..os(false)
    });
    assert!(w.ctl.view().reduced_motion, "Windows alone");
    assert!(w.ctl.settings_look().reduce_motion);
    w.ctl.set_system_look(os(false));
    assert!(!w.ctl.view().reduced_motion, "and it ends");
}

#[test]
fn high_contrast_wins_over_every_theme_setting() {
    let hc = HighContrast::default();
    for theme in [Theme::System, Theme::Light, Theme::Dark] {
        let mut w = world_with(
            None,
            Settings {
                theme,
                ..Settings::default()
            },
        );
        assert_eq!(w.ctl.view().high_contrast, None);
        w.ctl.set_system_look(SystemLook {
            dark: false,
            high_contrast: Some(hc),
            accent: None,
            reduce_motion: false,
        });
        assert_eq!(w.ctl.view().high_contrast, Some(hc), "{theme:?}");
        w.ctl.set_system_look(os(false));
        assert_eq!(w.ctl.view().high_contrast, None, "{theme:?}: and it ends");
    }
}

#[test]
fn a_fake_source_drives_the_look_through_a_change() {
    use snipdeck_platform::{system_look::SystemLookSource, testing::FakeSystemLook};
    let source = FakeSystemLook::default();
    let mut w = world();
    w.ctl.set_system_look(source.read());
    assert!(!w.ctl.view().dark);
    source.set(os(true));
    w.ctl.set_system_look(source.read());
    assert!(w.ctl.view().dark);
}

// ---- Settings are saved ------------------------------------------------------------------

pub(super) fn saving_world(dir: &std::path::Path) -> World {
    let mut w = world();
    let saver = SettingsSaver::new(
        Box::new(crate::settings_store::FileSink::new(dir)),
        Settings::default(),
    );
    w.ctl.set_settings_saver(saver);
    w
}

#[test]
fn the_main_window_place_is_remembered_in_the_settings() {
    let mut w = world();
    let place = crate::settings::WindowPlace {
        x: 40,
        y: 60,
        w: 1000,
        h: 640,
        maximized: false,
    };
    w.ctl.remember_main_window(place);
    assert_eq!(w.ctl.settings().main_window, Some(place));
}

#[test]
fn a_setting_is_written_after_the_quiet_time_by_the_injected_clock() {
    let temp = tempfile::tempdir().expect("dir");
    let file = temp.path().join(crate::paths::SETTINGS_FILE);
    let mut w = saving_world(temp.path());
    assert_eq!(w.ctl.settings_wait(), None, "nothing changed yet");

    w.ctl.set_theme(Theme::Dark);
    assert!(w.ctl.view().dark, "applied at once");
    assert_eq!(w.ctl.settings_wait(), Some(Duration::from_millis(500)));
    w.ctl.save_settings_if_due();
    assert!(!file.exists(), "not before the quiet time");

    w.clock.set(at("2026-10-17T10:30:00.4Z"));
    w.ctl.save_settings_if_due();
    assert!(!file.exists());
    assert_eq!(w.ctl.settings_wait(), Some(Duration::from_millis(100)));

    w.clock.set(at("2026-10-17T10:30:00.5Z"));
    w.ctl.save_settings_if_due();
    assert_eq!(crate::paths::load_settings(temp.path()).theme, Theme::Dark);
    assert_eq!(w.ctl.settings_wait(), None);
}

#[test]
fn choosing_a_theme_applies_it_toasts_it_and_writes_it_after_the_quiet_time() {
    let temp = tempfile::tempdir().expect("dir");
    let mut w = saving_world(temp.path());
    for theme in [Theme::Dark, Theme::Light, Theme::System] {
        w.ctl.choose_theme(theme);
        assert_eq!(w.ctl.settings().theme, theme);
        assert_eq!(
            w.ctl.view().toast.map(|t| t.message),
            Some(ToastMessage::ThemeSet(theme)),
            "a toast says which theme is on"
        );
    }
    w.ctl.choose_theme(Theme::Dark);
    assert!(w.ctl.view().dark, "applied at once");
    w.clock.set(at("2026-10-17T10:30:00.5Z"));
    w.ctl.save_settings_if_due();
    assert_eq!(
        crate::paths::load_settings(temp.path()).theme,
        Theme::Dark,
        "the same setting as Settings > Appearance, saved"
    );
}

#[test]
fn several_quick_changes_make_one_write_and_flush_writes_at_once() {
    let temp = tempfile::tempdir().expect("dir");
    let mut w = saving_world(temp.path());
    w.ctl.update_settings(|s| s.panel_width = 300);
    w.clock.set(at("2026-10-17T10:30:00.2Z"));
    w.ctl.update_settings(|s| s.panel_width = 9999);
    assert_eq!(w.ctl.settings().panel_width, 480, "sanitised");
    w.ctl.update_settings(|s| s.panel_width = 480);
    // The last two were the same value: still waiting from the second change.
    assert_eq!(w.ctl.settings_wait(), Some(Duration::from_millis(500)));

    w.ctl.flush_settings().expect("flush");
    assert_eq!(crate::paths::load_settings(temp.path()).panel_width, 480);
    assert_eq!(w.ctl.settings_wait(), None);
}

#[test]
fn an_unchanged_setting_schedules_nothing() {
    let temp = tempfile::tempdir().expect("dir");
    let mut w = saving_world(temp.path());
    w.ctl.update_settings(|s| s.close_to_tray = true);
    w.ctl.set_theme(Theme::System);
    assert_eq!(w.ctl.settings_wait(), None);
    w.ctl.flush_settings().expect("flush");
    assert!(!temp.path().join(crate::paths::SETTINGS_FILE).exists());
}

#[test]
fn without_a_saver_changes_stay_in_memory() {
    let mut w = world();
    w.ctl.set_theme(Theme::Dark);
    assert_eq!(w.ctl.settings_wait(), None);
    assert!(w.ctl.flush_settings().is_ok());
    assert_eq!(w.ctl.settings().theme, Theme::Dark);
}

#[test]
fn a_failed_write_toasts_once_and_succeeds_when_the_disk_does() {
    let temp = tempfile::tempdir().expect("dir");
    // The "folder" is a file, so every write fails.
    let blocked = temp.path().join("blocked");
    std::fs::write(&blocked, "x").expect("file");
    let mut w = saving_world(&blocked);
    w.ctl.set_theme(Theme::Dark);

    w.clock.set(at("2026-10-17T10:30:01Z"));
    w.ctl.save_settings_if_due();
    let toast = w.ctl.view().toast.expect("toast");
    assert_eq!(toast.kind, ToastKind::Danger);
    assert_eq!(toast.message, ToastMessage::SettingsNotSaved);
    let first = toast.seq;
    assert_eq!(
        w.ctl.settings_wait(),
        Some(Duration::from_secs(5)),
        "kept for a retry"
    );

    w.clock.set(at("2026-10-17T10:30:07Z"));
    w.ctl.save_settings_if_due();
    assert_eq!(
        w.ctl.view().toast.expect("toast").seq,
        first,
        "no second toast for the same trouble"
    );
    assert!(w.ctl.flush_settings().is_err());
}
