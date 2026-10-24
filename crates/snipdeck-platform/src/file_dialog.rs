//! Choosing a file for an Image or File item.
//!
//! [`FilePicker`] is what the item editor asks; Windows answers with the standard Open dialog
//! (`windows::file_dialog::WinFilePicker`: `GetOpenFileNameW` for files, `IFileOpenDialog` in
//! folder mode for folders; both **unverified on Windows**) and tests with
//! [`FakeFilePicker`]. Off Windows the app has no picker and the editor only offers its path
//! field. The pure parts of the Win32 call (the filter string, reading the chosen path back)
//! live here so they are tested everywhere.

use std::path::PathBuf;

use crate::error::Error;

/// What kind of file the dialog should offer first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    /// PNG pictures (the Image item's kind), with "All files" as the other choice.
    Image,
    /// Any file.
    Any,
    /// A `.snipdeck` file (an export or a backup).
    Snipdeck,
    /// A Ditto database (`.db`).
    Ditto,
    /// A `CopyQ` JSON export (`.json`).
    CopyQ,
    /// A folder, chosen with the folder picker rather than by picking a file inside it.
    Folder,
}

impl PickKind {
    /// The `(label, pattern)` choices of the dialog's file-type list, the kind's own first and
    /// "All files" last (empty for a folder, which has no file types).
    #[cfg(any(windows, test))]
    pub(crate) const fn filters(self) -> &'static [(&'static str, &'static str)] {
        const ALL: (&str, &str) = ("All files", "*.*");
        match self {
            Self::Image => &[("PNG pictures", "*.png"), ALL],
            Self::Any => &[ALL],
            Self::Snipdeck => &[("Snipdeck files", "*.snipdeck"), ALL],
            Self::Ditto => &[("Ditto databases", "*.db"), ALL],
            Self::CopyQ => &[("CopyQ exports", "*.json"), ALL],
            Self::Folder => &[],
        }
    }
}

/// Lets the user choose one existing file.
pub trait FilePicker {
    /// Shows the dialog and waits. `owner` is the native handle of the window the dialog
    /// belongs to (it is disabled meanwhile), if known.
    ///
    /// Returns `Ok(None)` when the user cancelled.
    ///
    /// # Errors
    ///
    /// The dialog could not be shown. It carries the call name and code, never a path.
    fn pick(&self, kind: PickKind, owner: Option<isize>) -> Result<Option<PathBuf>, Error>;
}

/// The Win32 filter string: `(label, pattern)` pairs, each part and the whole ended by a NUL.
#[cfg(any(windows, test))]
pub(crate) fn filter_utf16(pairs: &[(&str, &str)]) -> Vec<u16> {
    let mut out = Vec::new();
    for (label, pattern) in pairs {
        out.extend(label.encode_utf16());
        out.push(0);
        out.extend(pattern.encode_utf16());
        out.push(0);
    }
    out.push(0);
    out
}

/// The path in a dialog's file buffer: everything before the first NUL. `None` when empty.
#[cfg(any(windows, test))]
pub(crate) fn path_of_buffer(buffer: &[u16]) -> Option<PathBuf> {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    if end == 0 {
        return None;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_wide(&buffer[..end])))
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..end])))
    }
}

/// A picker that answers from a script and remembers what it was asked (for tests and the
/// dev build). Clones share the script.
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Clone, Default)]
pub struct FakeFilePicker {
    state: std::sync::Arc<std::sync::Mutex<FakeState>>,
}

#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Default)]
struct FakeState {
    answers: std::collections::VecDeque<Result<Option<PathBuf>, Error>>,
    asked: Vec<(PickKind, Option<isize>)>,
}

#[cfg(any(test, feature = "testing"))]
impl FakeFilePicker {
    /// A picker whose next answer is `answer` (later asks get "cancelled" once it runs out).
    #[must_use]
    pub fn answering(answer: Result<Option<PathBuf>, Error>) -> Self {
        let picker = Self::default();
        picker.push(answer);
        picker
    }

    /// Adds an answer to the end of the script.
    pub fn push(&self, answer: Result<Option<PathBuf>, Error>) {
        if let Ok(mut state) = self.state.lock() {
            state.answers.push_back(answer);
        }
    }

    /// What the picker was asked so far, oldest first.
    #[must_use]
    pub fn asked(&self) -> Vec<(PickKind, Option<isize>)> {
        self.state
            .lock()
            .map(|s| s.asked.clone())
            .unwrap_or_default()
    }
}

#[cfg(any(test, feature = "testing"))]
impl FilePicker for FakeFilePicker {
    fn pick(&self, kind: PickKind, owner: Option<isize>) -> Result<Option<PathBuf>, Error> {
        let Ok(mut state) = self.state.lock() else {
            return Err(Error::Unavailable("the fake file picker is poisoned"));
        };
        state.asked.push((kind, owner));
        state.answers.pop_front().unwrap_or(Ok(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_offers_its_own_extension_first_and_all_files_last() {
        let patterns = |k: PickKind| -> Vec<&'static str> {
            k.filters().iter().map(|(_, pattern)| *pattern).collect()
        };
        assert_eq!(patterns(PickKind::Image), ["*.png", "*.*"]);
        assert_eq!(patterns(PickKind::Any), ["*.*"]);
        assert_eq!(patterns(PickKind::Snipdeck), ["*.snipdeck", "*.*"]);
        assert_eq!(patterns(PickKind::Ditto), ["*.db", "*.*"]);
        assert_eq!(patterns(PickKind::CopyQ), ["*.json", "*.*"]);
        assert_eq!(PickKind::Folder.filters(), []);
    }

    #[test]
    fn the_filter_is_pairs_of_nul_ended_strings_and_one_more_nul() {
        let f = filter_utf16(&[("PNG", "*.png"), ("All", "*.*")]);
        let text: Vec<String> = f.split(|&c| c == 0).map(String::from_utf16_lossy).collect();
        assert_eq!(text, ["PNG", "*.png", "All", "*.*", "", ""]);
        assert_eq!(f.last(), Some(&0));
        assert_eq!(f[f.len() - 2], 0, "ends with a double NUL");
    }

    #[test]
    fn the_chosen_path_is_what_comes_before_the_first_nul() {
        let mut buffer: Vec<u16> = "C:\\a b\\c.png".encode_utf16().collect();
        buffer.extend([0, 0x41, 0x42]);
        assert_eq!(
            path_of_buffer(&buffer),
            Some(PathBuf::from("C:\\a b\\c.png"))
        );
        assert_eq!(path_of_buffer(&[0, 0x41]), None);
        assert_eq!(path_of_buffer(&[]), None);
        let no_nul: Vec<u16> = "x".encode_utf16().collect();
        assert_eq!(path_of_buffer(&no_nul), Some(PathBuf::from("x")));
    }

    #[test]
    fn the_fake_answers_in_order_then_cancels_and_remembers_the_asks() {
        let picker = FakeFilePicker::answering(Ok(Some(PathBuf::from("a.png"))));
        picker.push(Err(Error::Unavailable("no dialog")));
        let clone = picker.clone();
        assert_eq!(
            picker.pick(PickKind::Image, Some(7)),
            Ok(Some(PathBuf::from("a.png")))
        );
        assert_eq!(
            clone.pick(PickKind::Any, None),
            Err(Error::Unavailable("no dialog"))
        );
        assert_eq!(picker.pick(PickKind::Any, None), Ok(None));
        assert_eq!(
            picker.asked(),
            [
                (PickKind::Image, Some(7)),
                (PickKind::Any, None),
                (PickKind::Any, None)
            ]
        );
    }
}
