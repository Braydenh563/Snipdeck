//! Where the app keeps things, and loading the settings file.

use std::path::{Path, PathBuf};

use crate::settings::Settings;

/// The settings file inside the data folder.
pub const SETTINGS_FILE: &str = "settings.json";

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
#[must_use]
pub fn load_settings(folder: &Path) -> Settings {
    std::fs::read_to_string(folder.join(SETTINGS_FILE))
        .map(|text| Settings::from_json(&text))
        .unwrap_or_default()
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
}
