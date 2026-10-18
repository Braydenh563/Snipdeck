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

/// What reading the settings file found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// There is no file yet: first run, defaults.
    Missing,
    /// The file was read (out-of-range values clamped; nothing to set aside).
    Read,
    /// The file did not parse as it stands and was moved to `settings.json.bad`; what could be
    /// salvaged is used and the next save writes a fresh file.
    SetAside,
    /// A file exists but could not be read, or could not be set aside: it is the only copy of what
    /// the user had, and the defaults in use are **not** what it holds. It must not be
    /// overwritten (see [`Loaded::may_save`]).
    Untouchable,
}

/// The settings and how reading them went.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The settings to run with.
    pub settings: Settings,
    /// What was found.
    pub outcome: LoadOutcome,
}

impl Loaded {
    /// Whether writing the settings file is safe. `false` when the file on disk holds something
    /// the app could not use and could not keep a copy of: saving would destroy it.
    #[must_use]
    pub fn may_save(&self) -> bool {
        self.outcome != LoadOutcome::Untouchable
    }
}

/// Decodes the file's bytes. Notepad adds a UTF-8 byte-order mark ("UTF-8 with BOM", and in older
/// versions by default) or saves UTF-16 with a mark ("Unicode"): both are accepted. Anything else
/// is read as UTF-8 with replacement characters.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], word: fn([u8; 2]) -> u16| {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| word([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes),
        _ => {
            let text = String::from_utf8_lossy(bytes);
            text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
        }
    }
}

/// Reads the settings from `folder`. A missing file gives the defaults, and a damaged one keeps
/// whatever parses ([`Settings::from_json`] never fails).
///
/// A file that does not parse as it stands (not JSON, cut short, a field of the wrong type) is
/// moved to `settings.json.bad` first, so the next save does not overwrite the only copy of what
/// the user had; a previous `.bad` is replaced. Out-of-range values that parse are not damage:
/// they are clamped and the file is left alone. A file that cannot be read at all, or cannot be
/// moved, is left where it is and reported as [`LoadOutcome::Untouchable`].
#[must_use]
pub fn read_settings(folder: &Path) -> Loaded {
    let path = folder.join(SETTINGS_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Loaded {
                settings: Settings::default(),
                outcome: LoadOutcome::Missing,
            };
        }
        Err(_) => {
            return Loaded {
                settings: Settings::default(),
                outcome: LoadOutcome::Untouchable,
            };
        }
    };
    let text = decode(&bytes);
    let mut outcome = LoadOutcome::Read;
    if serde_json::from_str::<Settings>(&text).is_err() {
        outcome = match std::fs::rename(&path, folder.join(BAD_FILE)) {
            Ok(()) => LoadOutcome::SetAside,
            Err(_) => LoadOutcome::Untouchable,
        };
    }
    Loaded {
        settings: Settings::from_json(&text),
        outcome,
    }
}

/// The settings from `folder`, without the outcome (for callers that never write them back).
#[must_use]
pub fn load_settings(folder: &Path) -> Settings {
    read_settings(folder).settings
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

    #[test]
    fn a_byte_order_mark_from_notepad_is_not_damage() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        let json = r#"{"panel_width": 400, "no_paste_apps": ["keepass.exe"]}"#;
        std::fs::write(dir.join(SETTINGS_FILE), format!("\u{feff}{json}")).expect("write");
        let loaded = read_settings(dir);
        assert_eq!(loaded.outcome, LoadOutcome::Read);
        assert_eq!(loaded.settings.panel_width, 400);
        assert_eq!(loaded.settings.no_paste_apps, ["keepass.exe"]);
        assert!(dir.join(SETTINGS_FILE).exists(), "left in place");
        assert!(!dir.join(BAD_FILE).exists());
    }

    #[test]
    fn utf16_files_with_a_mark_are_read_too() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        let json = r#"{"panel_width": 410}"#;
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(json.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        std::fs::write(dir.join(SETTINGS_FILE), &le).expect("write");
        assert_eq!(read_settings(dir).settings.panel_width, 410);
        let be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain(json.encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        std::fs::write(dir.join(SETTINGS_FILE), &be).expect("write");
        let loaded = read_settings(dir);
        assert_eq!(loaded.settings.panel_width, 410);
        assert_eq!(loaded.outcome, LoadOutcome::Read);
    }

    #[test]
    fn the_outcome_says_what_was_found() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        let missing = read_settings(dir);
        assert_eq!(missing.outcome, LoadOutcome::Missing);
        assert!(missing.may_save());
        std::fs::write(dir.join(SETTINGS_FILE), "{ cut").expect("write");
        let aside = read_settings(dir);
        assert_eq!(aside.outcome, LoadOutcome::SetAside);
        assert!(aside.may_save(), "the old text is safe in .bad");
    }

    #[test]
    fn a_file_that_cannot_be_read_is_never_to_be_overwritten() {
        // A directory where the file should be: reading fails with something other than "not found".
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        std::fs::create_dir(dir.join(SETTINGS_FILE)).expect("dir");
        let loaded = read_settings(dir);
        assert_eq!(loaded.outcome, LoadOutcome::Untouchable);
        assert!(!loaded.may_save());
        assert_eq!(loaded.settings, Settings::default());
    }

    #[test]
    fn a_damaged_file_that_cannot_be_set_aside_is_left_alone_and_not_to_be_overwritten() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path();
        let cut = r#"{"no_paste_apps": ["keepass.exe", "bank"#;
        std::fs::write(dir.join(SETTINGS_FILE), cut).expect("write");
        // A non-empty directory at the .bad name: the rename onto it fails.
        std::fs::create_dir(dir.join(BAD_FILE)).expect("dir");
        std::fs::write(dir.join(BAD_FILE).join("keep"), "x").expect("write");
        let loaded = read_settings(dir);
        assert_eq!(loaded.outcome, LoadOutcome::Untouchable);
        assert!(!loaded.may_save());
        assert_eq!(
            loaded.settings.no_paste_apps,
            ["keepass.exe"],
            "the salvage still protects this run"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(SETTINGS_FILE)).expect("still there"),
            cut
        );
    }
}
