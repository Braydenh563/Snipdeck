//! The data folder Setup must erase: recorded in the registry when it is not the default.
//!
//! Setup's "erase my data" answer deletes `%LOCALAPPDATA%\Snipdeck`. A library kept elsewhere with
//! `--data-dir` would stay behind, so the app records such a folder under
//! `HKCU\Software\Snipdeck`, value `DataDir`, and Setup erases that one too. The default folder is
//! never recorded (Setup knows it), and a stale record is cleared when the app runs on the default
//! again. Everything that decides is here and runs on every OS; the registry calls are in
//! `windows::data_record` and the uninstaller code is in `packaging/inno/snipdeck.iss`
//! (`scripts/installer_check.py` keeps the names in step).

use std::path::Path;

/// The key under `HKCU` (shared with `packaging/registry-leftovers.txt`).
pub const KEY: &str = r"Software\Snipdeck";
/// The value name under [`KEY`].
pub const VALUE: &str = "DataDir";

/// What to do with the registry record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordAction {
    /// Already right.
    Nothing,
    /// Store this folder.
    Write(String),
    /// Remove the record (the app uses the default folder again).
    Clear,
}

/// Compares the folder in use with the default and with what is recorded.
///
/// `default` is `%LOCALAPPDATA%\Snipdeck` (`None` when the variable is unset). A folder that is
/// relative, empty or not Unicode is never recorded: Setup would not know what it meant.
#[must_use]
pub fn plan(recorded: Option<&str>, folder: &Path, default: Option<&Path>) -> RecordAction {
    let recorded = recorded.filter(|r| !r.is_empty());
    let same_as_default = default.is_some_and(|d| same_folder(d, folder));
    let storable = folder
        .to_str()
        .filter(|s| !s.contains('\0') && is_absolute(s));
    match (same_as_default, storable, recorded) {
        (true, _, None) | (false, None, None) => RecordAction::Nothing,
        (true, _, Some(_)) | (false, None, Some(_)) => RecordAction::Clear,
        (false, Some(s), Some(r)) if r == s => RecordAction::Nothing,
        (false, Some(s), _) => RecordAction::Write(s.to_owned()),
    }
}

/// A drive path (`D:\x`) or a UNC path (`\\server\x`); decided on the text so the test runs everywhere.
fn is_absolute(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || s.starts_with(r"\\")
}

/// Windows compares paths without case and treats a trailing separator as nothing.
fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Whether Setup may delete the recorded folder: a real folder below a drive root, not the
/// profile itself. (The Pascal guard in the installer mirrors this rule.)
#[must_use]
pub fn safe_to_erase(folder: &str, profile: &str) -> bool {
    let f = folder.replace('/', "\\");
    let f = f.trim_end_matches('\\');
    let p = profile.replace('/', "\\");
    let p = p.trim_end_matches('\\');
    let depth = f.split('\\').filter(|part| !part.is_empty()).count();
    // `C:\x` has two parts (`C:` and `x`); a drive root has one.
    f.len() > 3 && depth >= 2 && !f.contains('\0') && !f.eq_ignore_ascii_case(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const DEFAULT: &str = r"C:\Users\a\AppData\Local\Snipdeck";

    fn plan_for(recorded: Option<&str>, folder: &str) -> RecordAction {
        plan(recorded, &PathBuf::from(folder), Some(Path::new(DEFAULT)))
    }

    #[test]
    fn the_default_folder_is_not_recorded() {
        assert_eq!(plan_for(None, DEFAULT), RecordAction::Nothing);
        assert_eq!(
            plan_for(None, r"c:\users\A\appdata\local\snipdeck\"),
            RecordAction::Nothing
        );
    }

    #[test]
    fn a_custom_folder_is_recorded_once() {
        let custom = r"D:\Data\Snipdeck";
        assert_eq!(
            plan_for(None, custom),
            RecordAction::Write(custom.to_owned())
        );
        assert_eq!(plan_for(Some(custom), custom), RecordAction::Nothing);
        assert_eq!(
            plan_for(Some(r"E:\old"), custom),
            RecordAction::Write(custom.to_owned())
        );
    }

    #[test]
    fn a_stale_record_is_cleared_on_the_default() {
        assert_eq!(plan_for(Some(r"D:\Data"), DEFAULT), RecordAction::Clear);
    }

    #[test]
    fn relative_and_empty_folders_are_never_recorded() {
        assert_eq!(plan_for(None, "data"), RecordAction::Nothing);
        assert_eq!(plan_for(None, ""), RecordAction::Nothing);
        assert_eq!(plan_for(Some(r"D:\x"), "data"), RecordAction::Clear);
    }

    #[test]
    fn an_unknown_default_still_records_a_custom_folder() {
        assert_eq!(
            plan(None, Path::new(r"D:\Data"), None),
            RecordAction::Write(r"D:\Data".to_owned())
        );
    }

    #[test]
    fn erase_refuses_roots_and_the_profile() {
        let profile = r"C:\Users\a";
        assert!(safe_to_erase(r"D:\Data\Snipdeck", profile));
        assert!(safe_to_erase(r"D:\Snipdeck\", profile));
        assert!(!safe_to_erase(r"D:\", profile));
        assert!(!safe_to_erase("D:", profile));
        assert!(!safe_to_erase("", profile));
        assert!(!safe_to_erase(r"c:\users\A\", profile));
        assert!(!safe_to_erase(r"C:\Users\a", profile));
    }
}
