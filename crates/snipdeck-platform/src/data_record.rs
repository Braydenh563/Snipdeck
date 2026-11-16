//! The data folders Setup's "erase my data" may clean: recorded in the registry when they are not
//! the default, and only ever erased file by file.
//!
//! A library kept elsewhere with `--data-dir` would stay behind at uninstall, so the app records
//! each such folder in a list under `HKCU\Software\Snipdeck` (multi-string value `DataDirs`) and
//! writes [`MARKER_FILE`] into every data folder it uses. Setup erases a recorded folder only when
//! the marker is there and the path passes [`safe_to_erase`]; it deletes just the names in
//! [`OWN_ENTRIES`] and the marker, then removes the folder only if that left it empty. Nothing is
//! ever deleted recursively at the folder itself, so `--data-dir D:\Documents` loses nothing but
//! Snipdeck's own files. Everything that decides is here and runs on every OS; the registry calls
//! are in `windows::data_record` and the uninstaller code is in `packaging/inno/snipdeck.iss`
//! (`scripts/installer_check.py` keeps the tables below equal to the ones in that file).

use std::io;
use std::path::Path;

/// The key under `HKCU` (shared with `packaging/registry-leftovers.txt`).
pub const KEY: &str = r"Software\Snipdeck";
/// The multi-string value under [`KEY`]: every folder used with `--data-dir`.
pub const VALUE: &str = "DataDirs";
/// How many folders the record keeps (the oldest fall off).
pub const MAX_FOLDERS: usize = 16;

/// The file the app writes into every data folder; Setup erases nothing without it.
pub const MARKER_FILE: &str = "snipdeck-data-folder.txt";
/// The names Setup deletes inside a recorded folder (files, or folders with everything below).
pub const OWN_ENTRIES: &[&str] = &[
    "snipdeck.db",
    "snipdeck.db-wal",
    "snipdeck.db-shm",
    "snipdeck.lock",
    "blobs",
    "settings.json",
    "settings.json.bad",
    "maintenance.json",
    "drafts",
    "backups",
    "logs",
];
/// Folder names Setup never touches, wherever they are (compared without case).
pub const PROTECTED_NAMES: &[&str] = &[
    "documents",
    "desktop",
    "downloads",
    "pictures",
    "music",
    "videos",
    "users",
    "appdata",
    "local",
    "roaming",
    "windows",
    "system32",
    "program files",
    "program files (x86)",
    "programdata",
    "onedrive",
];

/// What to do with the registry record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordAction {
    /// Already right.
    Nothing,
    /// Store this whole list.
    Write(Vec<String>),
}

/// Adds the folder in use to the recorded list when it is new and not the default.
///
/// `default` is `%LOCALAPPDATA%\Snipdeck` (`None` when the variable is unset). A folder that is
/// relative, empty or not Unicode is never recorded: Setup would not know what it meant. Old
/// entries stay: a library used earlier with `--data-dir` is still on disk.
#[must_use]
pub fn plan(recorded: &[String], folder: &Path, default: Option<&Path>) -> RecordAction {
    if default.is_some_and(|d| same_folder(d, folder)) {
        return RecordAction::Nothing;
    }
    let Some(s) = folder
        .to_str()
        .filter(|s| !s.contains('\0') && is_absolute(s))
    else {
        return RecordAction::Nothing;
    };
    if recorded.iter().any(|r| same_folder(Path::new(r), folder)) {
        return RecordAction::Nothing;
    }
    let mut list: Vec<String> = recorded.iter().filter(|r| !r.is_empty()).cloned().collect();
    list.push(s.to_owned());
    let extra = list.len().saturating_sub(MAX_FOLDERS);
    list.drain(..extra);
    RecordAction::Write(list)
}

/// A `REG_MULTI_SZ` payload: each entry NUL-terminated, then one more NUL.
#[must_use]
pub fn encode_multi(list: &[String]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for item in list.iter().filter(|s| !s.is_empty()) {
        out.extend(item.encode_utf16());
        out.push(0);
    }
    if out.is_empty() {
        out.push(0);
    }
    out.push(0);
    out
}

/// The entries of a `REG_MULTI_SZ` payload (empty entries and the terminator are dropped).
#[must_use]
pub fn decode_multi(units: &[u16]) -> Vec<String> {
    units
        .split(|&u| u == 0)
        .filter(|part| !part.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Puts [`MARKER_FILE`] in the folder (creating it), unless it is already there.
///
/// # Errors
///
/// The I/O error from creating the folder or writing the file.
pub fn write_marker(folder: &Path) -> io::Result<()> {
    let path = folder.join(MARKER_FILE);
    if path.is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(folder)?;
    std::fs::write(
        path,
        "This folder holds a Snipdeck library. Uninstalling Snipdeck may offer to erase Snipdeck's\r\n\
         own files here; it never deletes anything else. Delete this file to keep the folder.\r\n",
    )
}

/// A drive path (`D:\x`) or a UNC path (`\\server\x`); decided on the text so the test runs everywhere.
fn is_absolute(s: &str) -> bool {
    has_drive(s) || s.starts_with(r"\\")
}

fn has_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
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

/// Whether Setup may even look at the recorded folder: a drive-letter path below the drive root,
/// not the profile, not a well-known folder, no `..`. (The Pascal guard in the installer mirrors
/// this rule; the marker and the empty check come on top of it.)
#[must_use]
pub fn safe_to_erase(folder: &str, profile: &str) -> bool {
    let f = folder.replace('/', "\\");
    let f = f.trim_end_matches('\\');
    let p = profile.replace('/', "\\");
    let p = p.trim_end_matches('\\');
    let last = f.rsplit('\\').next().unwrap_or("").to_lowercase();
    has_drive(&format!("{f}\\"))
        && f.len() > 3
        && !f.contains('\0')
        && !f.split('\\').any(|part| part == "..")
        && !f.eq_ignore_ascii_case(p)
        && !PROTECTED_NAMES.contains(&last.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const DEFAULT: &str = r"C:\Users\a\AppData\Local\Snipdeck";

    fn plan_for(recorded: &[&str], folder: &str) -> RecordAction {
        let recorded: Vec<String> = recorded.iter().map(|s| (*s).to_owned()).collect();
        plan(&recorded, &PathBuf::from(folder), Some(Path::new(DEFAULT)))
    }

    fn list(items: &[&str]) -> RecordAction {
        RecordAction::Write(items.iter().map(|s| (*s).to_owned()).collect())
    }

    #[test]
    fn the_default_folder_is_not_recorded() {
        assert_eq!(plan_for(&[], DEFAULT), RecordAction::Nothing);
        assert_eq!(
            plan_for(&[], r"c:\users\A\appdata\local\snipdeck\"),
            RecordAction::Nothing
        );
    }

    #[test]
    fn a_custom_folder_is_added_once() {
        let custom = r"D:\Data\Snipdeck";
        assert_eq!(plan_for(&[], custom), list(&[custom]));
        assert_eq!(plan_for(&[custom], custom), RecordAction::Nothing);
        assert_eq!(
            plan_for(&[custom], r"d:\data\snipdeck\"),
            RecordAction::Nothing
        );
    }

    #[test]
    fn a_second_folder_does_not_replace_the_first() {
        assert_eq!(
            plan_for(&[r"E:\old"], r"D:\Data"),
            list(&[r"E:\old", r"D:\Data"])
        );
    }

    #[test]
    fn the_default_keeps_older_records() {
        assert_eq!(plan_for(&[r"D:\Data"], DEFAULT), RecordAction::Nothing);
    }

    #[test]
    fn the_list_is_capped_and_drops_the_oldest() {
        let old: Vec<String> = (0..MAX_FOLDERS).map(|i| format!(r"D:\f{i}")).collect();
        let RecordAction::Write(new) = plan(&old, Path::new(r"D:\new"), None) else {
            panic!("expected a write");
        };
        assert_eq!(new.len(), MAX_FOLDERS);
        assert_eq!(new.first().map(String::as_str), Some(r"D:\f1"));
        assert_eq!(new.last().map(String::as_str), Some(r"D:\new"));
    }

    #[test]
    fn relative_and_empty_folders_are_never_recorded() {
        assert_eq!(plan_for(&[], "data"), RecordAction::Nothing);
        assert_eq!(plan_for(&[], ""), RecordAction::Nothing);
    }

    #[test]
    fn an_unknown_default_still_records_a_custom_folder() {
        assert_eq!(plan(&[], Path::new(r"D:\Data"), None), list(&[r"D:\Data"]));
    }

    #[test]
    fn multi_string_round_trips() {
        let l = vec![r"D:\a".to_owned(), r"E:\b c".to_owned()];
        let units = encode_multi(&l);
        assert_eq!(units[units.len() - 2..], [0, 0]);
        assert_eq!(decode_multi(&units), l);
        assert_eq!(encode_multi(&[]), [0, 0]);
        assert!(decode_multi(&[0, 0]).is_empty());
        assert!(decode_multi(&[]).is_empty());
    }

    #[test]
    fn the_marker_is_written_once_and_creates_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("new").join("lib");
        write_marker(&folder).unwrap();
        let marker = folder.join(MARKER_FILE);
        assert!(marker.is_file());
        std::fs::write(&marker, "mine").unwrap();
        write_marker(&folder).unwrap();
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "mine");
    }

    #[test]
    fn the_tables_are_plain_names() {
        for n in OWN_ENTRIES.iter().chain(&[MARKER_FILE]) {
            assert!(
                !n.is_empty() && !n.contains(['\\', '/', '|', '*', '?']),
                "{n}"
            );
        }
        for n in PROTECTED_NAMES {
            assert_eq!(*n, n.to_lowercase());
            assert!(!n.contains(['\\', '/', '|']), "{n}");
        }
        assert!(!OWN_ENTRIES.contains(&MARKER_FILE));
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

    #[test]
    fn erase_refuses_well_known_folders_and_odd_paths() {
        let profile = r"C:\Users\a";
        assert!(!safe_to_erase(r"D:\Documents", profile));
        assert!(!safe_to_erase(r"C:\Users\a\Documents", profile));
        assert!(!safe_to_erase(r"D:\x\DESKTOP\", profile));
        assert!(!safe_to_erase(r"C:\Program Files", profile));
        assert!(!safe_to_erase(r"\\server\share\Snipdeck", profile));
        assert!(!safe_to_erase(r"D:\Data\..\..", profile));
        assert!(safe_to_erase(r"D:\Documents\Snipdeck", profile));
    }
}
