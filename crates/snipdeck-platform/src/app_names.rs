//! The display names programs give themselves.
//!
//! On Windows an executable's version resource carries a `FileDescription` ("Microsoft Word" for
//! `WINWORD.EXE`). The platform reads it the first time an app is the paste target and
//! [`remember`]s it here; the app crate's one naming function ([`lookup`]) prefers it over a name
//! guessed from the file name, so the footer, tooltips, toasts and exclusion lists all say the
//! same thing. Names are keyed by the executable's file name, case-insensitively.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use crate::win_logic::exe_file_name;

/// The longest display name kept; a longer description is not a name.
const MAX_NAME_CHARS: usize = 48;

/// What is known about one executable: its name, or that it has none to give.
static NAMES: Mutex<BTreeMap<String, Option<String>>> = Mutex::new(BTreeMap::new());

fn key(exe: &str) -> Option<String> {
    exe_file_name(exe).map(str::to_lowercase)
}

/// A description fit to show: trimmed, one line, not too long. `None` for anything else.
fn clean(description: &str) -> Option<String> {
    let name = description.trim();
    let fit = !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && !name.chars().any(char::is_control);
    fit.then(|| name.to_owned())
}

/// Records the description of `exe` (a file name or a path); one that is not fit to show is
/// remembered as "no name", so it is not asked for again.
pub fn remember(exe: &str, description: &str) {
    if let Some(key) = key(exe) {
        NAMES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, clean(description));
    }
}

/// The remembered display name of `exe`, if it gave one.
#[must_use]
pub fn lookup(exe: &str) -> Option<String> {
    let key = key(exe)?;
    NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .cloned()
        .flatten()
}

/// Whether `exe` was already asked for its description (whether or not it had one).
#[must_use]
pub fn asked(exe: &str) -> bool {
    key(exe).is_some_and(|key| {
        NAMES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&key)
    })
}

/// The version-resource paths to try for the `FileDescription` string, given the file's own
/// `(language, code page)` pairs: its own first, then US English (Unicode, then Windows-1252),
/// which most programs use. No pair is listed twice.
#[must_use]
pub fn description_paths(translations: &[(u16, u16)]) -> Vec<String> {
    let mut pairs: Vec<(u16, u16)> = translations.to_vec();
    for fallback in [(0x0409, 0x04B0), (0x0409, 0x04E4)] {
        if !pairs.contains(&fallback) {
            pairs.push(fallback);
        }
    }
    let mut seen = Vec::new();
    pairs.retain(|pair| {
        let fresh = !seen.contains(pair);
        seen.push(*pair);
        fresh
    });
    pairs
        .into_iter()
        .map(|(lang, page)| format!("\\StringFileInfo\\{lang:04x}{page:04x}\\FileDescription"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remembered_name_is_found_by_file_name_or_path_in_any_case() {
        remember(r"C:\Apps\Acme\AcmeWriter1.EXE", "  Acme Writer ");
        for exe in [
            "acmewriter1.exe",
            "AcmeWriter1.exe",
            r"D:\x\ACMEWRITER1.EXE",
        ] {
            assert_eq!(lookup(exe).as_deref(), Some("Acme Writer"), "{exe}");
        }
        assert!(asked("acmewriter1.exe"));
        assert_eq!(lookup("neverseen1.exe"), None);
        assert!(!asked("neverseen1.exe"));
    }

    #[test]
    fn the_files_own_language_is_tried_first_then_english_without_repeats() {
        assert_eq!(
            description_paths(&[(0x0407, 0x04B0), (0x0409, 0x04B0)]),
            [
                r"\StringFileInfo\040704b0\FileDescription",
                r"\StringFileInfo\040904b0\FileDescription",
                r"\StringFileInfo\040904e4\FileDescription",
            ]
        );
        assert_eq!(description_paths(&[]).len(), 2);
    }

    #[test]
    fn a_description_not_fit_to_show_is_remembered_as_no_name() {
        for (exe, description) in [
            ("blank1.exe", "   "),
            ("long1.exe", &"x".repeat(MAX_NAME_CHARS + 1)),
            ("lines1.exe", "Two\nlines"),
        ] {
            remember(exe, description);
            assert_eq!(lookup(exe), None, "{exe}");
            assert!(asked(exe), "{exe}: not asked again");
        }
        let exact = "y".repeat(MAX_NAME_CHARS);
        remember("exact1.exe", &exact);
        assert_eq!(lookup("exact1.exe"), Some(exact));
        remember("", "No file");
        assert_eq!(lookup(""), None);
    }
}
