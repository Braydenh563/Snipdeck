//! Where the app keeps things, and loading the settings file.

use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// The settings file inside the data folder.
pub const SETTINGS_FILE: &str = "settings.json";
/// Where a settings file that could not be read as it stands is set aside.
pub const BAD_FILE: &str = "settings.json.bad";

/// The data folder: `%LOCALAPPDATA%\Snipdeck` on Windows (the database must not roam),
/// `$XDG_DATA_HOME/snipdeck` or `~/.local/share/snipdeck` elsewhere. `env` reads an
/// environment variable; `windows` says which rule applies.
#[must_use]
pub fn data_dir(env: &dyn Fn(&str) -> Option<String>, windows: bool) -> Option<PathBuf> {
    let non_empty = |name: &str| env(name).filter(|v| !v.trim().is_empty());
    if windows {
        return non_empty("LOCALAPPDATA").map(|base| Path::new(&base).join("Snipdeck"));
    }
    non_empty("XDG_DATA_HOME")
        .map(|base| Path::new(&base).join("snipdeck"))
        .or_else(|| non_empty("HOME").map(|home| Path::new(&home).join(".local/share/snipdeck")))
}

/// Reads the settings from `folder`. A missing or unreadable file gives the defaults, and a
/// damaged one keeps whatever parses ([`Settings::from_json`] never fails).
///
/// A file that does not parse as it stands (not JSON, cut short, a field of the wrong type) is
/// moved to `settings.json.bad` first, so the next save does not overwrite the only copy of what
/// the user had; a previous `.bad` is replaced. Out-of-range values that parse are not damage:
/// they are clamped and the file is left alone.
#[must_use]
pub fn load_settings(folder: &Path) -> Settings {
    let path = folder.join(SETTINGS_FILE);
    let Ok(bytes) = std::fs::read(&path) else {
        return Settings::default();
    };
    let text = String::from_utf8_lossy(&bytes);
    if serde_json::from_str::<Settings>(&text).is_err() {
        // Best effort: if the file cannot be moved it is simply overwritten by the next save.
        let _ = std::fs::rename(&path, folder.join(BAD_FILE));
    }
    Settings::from_json(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn windows_uses_local_app_data() {
        let vars = [("LOCALAPPDATA", "C:\\Users\\a\\AppData\\Local")];
        let dir = data_dir(&env(&vars), true).expect("dir");
        assert!(dir.ends_with("Snipdeck"), "{dir:?}");
        assert!(dir.starts_with("C:\\Users\\a\\AppData\\Local"), "{dir:?}");
        assert_eq!(data_dir(&env(&[]), true), None);
    }

    #[test]
    fn elsewhere_prefers_xdg_then_home() {
        let both = [("XDG_DATA_HOME", "/x"), ("HOME", "/home/a")];
        assert_eq!(
            data_dir(&env(&both), false),
            Some(PathBuf::from("/x/snipdeck"))
        );
        let home = [("HOME", "/home/a"), ("XDG_DATA_HOME", "  ")];
        assert_eq!(
            data_dir(&env(&home), false),
            Some(PathBuf::from("/home/a/.local/share/snipdeck"))
        );
        assert_eq!(data_dir(&env(&[]), false), None);
    }

    #[test]
    fn settings_load_tolerantly() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().to_path_buf();
        assert_eq!(load_settings(&dir), Settings::default(), "no file");
        std::fs::write(dir.join(SETTINGS_FILE), "{ not json").expect("write");
        assert_eq!(load_settings(&dir), Settings::default(), "garbage");
        std::fs::write(
            dir.join(SETTINGS_FILE),
            r#"{"panel_width": 400, "theme": 7}"#,
        )
        .expect("write");
        let settings = load_settings(&dir);
        assert_eq!(settings.panel_width, 400, "the good field is kept");
        assert_eq!(
            settings.theme,
            crate::settings::Theme::System,
            "the bad one is not"
        );
    }

    #[test]
    fn a_damaged_file_is_set_aside_as_bad_and_what_parses_is_kept() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        let cut = r#"{"panel_width": 400, "no_paste_apps": ["keepass.exe", "bank"#;
        std::fs::write(dir.join(SETTINGS_FILE), cut).expect("write");
        let settings = load_settings(dir);
        assert_eq!(
            settings.no_paste_apps,
            ["keepass.exe"],
            "the safety list survives"
        );
        assert!(!dir.join(SETTINGS_FILE).exists(), "moved, not copied");
        assert_eq!(
            std::fs::read_to_string(dir.join(BAD_FILE)).expect("bad"),
            cut
        );

        // A second damaged file replaces the first .bad.
        std::fs::write(dir.join(SETTINGS_FILE), "{ nope").expect("write");
        let _ = load_settings(dir);
        assert_eq!(
            std::fs::read_to_string(dir.join(BAD_FILE)).expect("bad"),
            "{ nope"
        );
    }

    #[test]
    fn a_wrong_type_is_damage_but_an_out_of_range_number_is_not() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"theme": 7}"#).expect("write");
        let _ = load_settings(dir);
        assert!(dir.join(BAD_FILE).exists());

        std::fs::remove_file(dir.join(BAD_FILE)).expect("remove");
        std::fs::write(dir.join(SETTINGS_FILE), r#"{"panel_width": 9999}"#).expect("write");
        assert_eq!(load_settings(dir).panel_width, 480, "clamped");
        assert!(dir.join(SETTINGS_FILE).exists());
        assert!(!dir.join(BAD_FILE).exists());
    }

    #[test]
    fn a_good_or_missing_file_makes_no_bad_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        let _ = load_settings(dir);
        std::fs::write(dir.join(SETTINGS_FILE), Settings::default().to_json()).expect("write");
        let _ = load_settings(dir);
        assert!(!dir.join(BAD_FILE).exists());
    }
}
