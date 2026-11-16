//! Choosing a file for an Image or File item.
//!
//! [`FilePicker`] is what the item editor asks; Windows answers with the standard Open dialog
//! (`windows::file_dialog::WinFilePicker`: `GetOpenFileNameW` for files, `IFileOpenDialog` in
//! folder mode for folders; both **unverified on Windows**) and tests with
//! [`FakeFilePicker`]. Off Windows the app has no picker and the editor only offers its path
//! field. The pure parts of the Win32 call (the filter string, reading the chosen path back, the
//! option flags) live here so they are tested everywhere.
//!
//! The dialogs do not block the caller: the answer comes to a [`Done`] callback, which the
//! Windows picker calls from the thread that ran the dialog. The app hands it a callback that
//! hops back to the UI thread, so the UI's timers, hotkeys and repainting keep running while a
//! dialog is up.

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
    /// A Beeftext JSON export (`.json`).
    Beeftext,
    /// A CSV export (`.csv`, or `.txt`).
    Csv,
    /// An Espanso match file (`.yml` or `.yaml`).
    Yaml,
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
            Self::Beeftext => &[("Beeftext exports", "*.json"), ALL],
            Self::Csv => &[("CSV exports", "*.csv;*.txt"), ALL],
            Self::Yaml => &[("Espanso match files", "*.yml;*.yaml"), ALL],
            Self::Folder => &[],
        }
    }
}

/// What kind of file the Save dialog writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKind {
    /// An SVG drawing (`.svg`).
    Svg,
    /// A PNG picture (`.png`).
    Png,
}

impl SaveKind {
    /// The extension without the dot, which the dialog adds to a name typed without one.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
        }
    }

    /// The `(label, pattern)` choices of the dialog's file-type list: the kind's own, then
    /// "All files".
    #[cfg(any(windows, test))]
    pub(crate) const fn filters(self) -> &'static [(&'static str, &'static str)] {
        const ALL: (&str, &str) = ("All files", "*.*");
        match self {
            Self::Svg => &[("SVG drawings", "*.svg"), ALL],
            Self::Png => &[("PNG pictures", "*.png"), ALL],
        }
    }
}

/// What a dialog answers: the path chosen, `Ok(None)` when the user cancelled, or why the dialog
/// could not be shown (the call name and code, never a path).
pub type Answer = Result<Option<PathBuf>, Error>;

/// Where a dialog's [`Answer`] goes. Called exactly once, on whichever thread ran the dialog (the
/// calling thread itself for a fake).
pub type Done = Box<dyn FnOnce(Answer) + Send>;

/// Lets the user choose one existing file, or the name of a file to write.
pub trait FilePicker {
    /// Shows the dialog without blocking the caller and gives the [`Answer`] to `done`. `owner`
    /// is the native handle of the window the dialog belongs to (it is disabled meanwhile), if
    /// known.
    fn pick(&self, kind: PickKind, owner: Option<isize>, done: Done);

    /// Shows the Save dialog with `suggested` (a file name, with or without the extension) filled
    /// in, without blocking the caller, and gives the [`Answer`] to `done`. The dialog asks
    /// before replacing a file. It writes nothing itself: the caller writes the file at the path
    /// answered.
    fn save(&self, kind: SaveKind, suggested: &str, owner: Option<isize>, done: Done);
}

/// Which dialog the option flags are for.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogUse {
    /// `IFileOpenDialog` picking a folder.
    PickFolder,
    /// `IFileSaveDialog`.
    SaveFile,
}

// `FILEOPENDIALOGOPTIONS` bits; the Windows backend asserts they equal the `windows` crate's.
/// `FOS_OVERWRITEPROMPT`.
#[cfg(any(windows, test))]
pub(crate) const FOS_OVERWRITEPROMPT: u32 = 0x2;
/// `FOS_NOCHANGEDIR`.
#[cfg(any(windows, test))]
pub(crate) const FOS_NOCHANGEDIR: u32 = 0x8;
/// `FOS_PICKFOLDERS`.
#[cfg(any(windows, test))]
pub(crate) const FOS_PICKFOLDERS: u32 = 0x20;
/// `FOS_FORCEFILESYSTEM`.
#[cfg(any(windows, test))]
pub(crate) const FOS_FORCEFILESYSTEM: u32 = 0x40;
/// `FOS_PATHMUSTEXIST`.
#[cfg(any(windows, test))]
pub(crate) const FOS_PATHMUSTEXIST: u32 = 0x800;
/// `FOS_DONTADDTORECENT` (the same bit as `OFN_DONTADDTORECENT`).
#[cfg(any(windows, test))]
pub(crate) const FOS_DONTADDTORECENT: u32 = 0x0200_0000;

// `OFN_*` bits of `OPENFILENAMEW::Flags`.
/// `OFN_HIDEREADONLY`.
#[cfg(any(windows, test))]
pub(crate) const OFN_HIDEREADONLY: u32 = 0x4;
/// `OFN_NOCHANGEDIR`.
#[cfg(any(windows, test))]
pub(crate) const OFN_NOCHANGEDIR: u32 = 0x8;
/// `OFN_PATHMUSTEXIST`.
#[cfg(any(windows, test))]
pub(crate) const OFN_PATHMUSTEXIST: u32 = 0x800;
/// `OFN_FILEMUSTEXIST`.
#[cfg(any(windows, test))]
pub(crate) const OFN_FILEMUSTEXIST: u32 = 0x1000;
/// `OFN_EXPLORER`.
#[cfg(any(windows, test))]
pub(crate) const OFN_EXPLORER: u32 = 0x8_0000;
/// `OFN_DONTADDTORECENT`.
#[cfg(any(windows, test))]
pub(crate) const OFN_DONTADDTORECENT: u32 = 0x0200_0000;

/// The options for an `IFileDialog` of `usage`, on top of the `current` ones. The one place the
/// flags are built: no dialog adds the file it names (a backup, an export, an import) to Windows'
/// Recent files and jump lists.
#[cfg(any(windows, test))]
pub(crate) const fn item_dialog_options(current: u32, usage: DialogUse) -> u32 {
    let own = match usage {
        DialogUse::PickFolder => FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST,
        DialogUse::SaveFile => {
            FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_OVERWRITEPROMPT | FOS_NOCHANGEDIR
        }
    };
    current | own | FOS_DONTADDTORECENT
}

/// The flags of the Open dialog for a file (`GetOpenFileNameW`), likewise without Recent.
#[cfg(any(windows, test))]
pub(crate) const fn open_file_name_flags() -> u32 {
    OFN_EXPLORER
        | OFN_FILEMUSTEXIST
        | OFN_PATHMUSTEXIST
        | OFN_HIDEREADONLY
        | OFN_NOCHANGEDIR
        | OFN_DONTADDTORECENT
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
    save_answers: std::collections::VecDeque<Result<Option<PathBuf>, Error>>,
    save_asked: Vec<(SaveKind, String, Option<isize>)>,
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

    /// Adds an answer to the end of the script for [`FilePicker::save`] (later saves get
    /// "cancelled" once it runs out).
    pub fn push_save(&self, answer: Result<Option<PathBuf>, Error>) {
        if let Ok(mut state) = self.state.lock() {
            state.save_answers.push_back(answer);
        }
    }

    /// What [`FilePicker::save`] was asked so far (kind, suggested name, owner), oldest first.
    #[must_use]
    pub fn asked_save(&self) -> Vec<(SaveKind, String, Option<isize>)> {
        self.state
            .lock()
            .map(|s| s.save_asked.clone())
            .unwrap_or_default()
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
    /// Answers at once, on the calling thread.
    fn pick(&self, kind: PickKind, owner: Option<isize>, done: Done) {
        let answer = match self.state.lock() {
            Ok(mut state) => {
                state.asked.push((kind, owner));
                state.answers.pop_front().unwrap_or(Ok(None))
            }
            Err(_) => Err(Error::Unavailable("the fake file picker is poisoned")),
        };
        done(answer);
    }

    /// Answers at once, on the calling thread.
    fn save(&self, kind: SaveKind, suggested: &str, owner: Option<isize>, done: Done) {
        let answer = match self.state.lock() {
            Ok(mut state) => {
                state.save_asked.push((kind, suggested.to_owned(), owner));
                state.save_answers.pop_front().unwrap_or(Ok(None))
            }
            Err(_) => Err(Error::Unavailable("the fake file picker is poisoned")),
        };
        done(answer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake answers before `pick` returns: collect the answer it gave `done`.
    fn picked(picker: &FakeFilePicker, kind: PickKind, owner: Option<isize>) -> Answer {
        let (tx, rx) = std::sync::mpsc::channel();
        picker.pick(
            kind,
            owner,
            Box::new(move |answer| {
                let _ = tx.send(answer);
            }),
        );
        rx.try_recv().expect("the fake answers at once")
    }

    fn saved(picker: &FakeFilePicker, kind: SaveKind, name: &str, owner: Option<isize>) -> Answer {
        let (tx, rx) = std::sync::mpsc::channel();
        picker.save(
            kind,
            name,
            owner,
            Box::new(move |answer| {
                let _ = tx.send(answer);
            }),
        );
        rx.try_recv().expect("the fake answers at once")
    }

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
        assert_eq!(patterns(PickKind::Beeftext), ["*.json", "*.*"]);
        assert_eq!(patterns(PickKind::Csv), ["*.csv;*.txt", "*.*"]);
        assert_eq!(patterns(PickKind::Yaml), ["*.yml;*.yaml", "*.*"]);
        assert_eq!(PickKind::Folder.filters(), []);
    }

    #[test]
    fn a_save_kind_offers_its_own_extension_first_and_all_files_last() {
        let patterns = |k: SaveKind| -> Vec<&'static str> {
            k.filters().iter().map(|(_, pattern)| *pattern).collect()
        };
        assert_eq!(patterns(SaveKind::Svg), ["*.svg", "*.*"]);
        assert_eq!(patterns(SaveKind::Png), ["*.png", "*.*"]);
        assert_eq!(
            (SaveKind::Svg.extension(), SaveKind::Png.extension()),
            ("svg", "png")
        );
    }

    #[test]
    fn the_fake_saves_in_order_then_cancels_and_remembers_the_asks() {
        let picker = FakeFilePicker::default();
        picker.push_save(Ok(Some(PathBuf::from("out.png"))));
        picker.push_save(Err(Error::Unavailable("no dialog")));
        assert_eq!(
            saved(&picker, SaveKind::Png, "trash", Some(3)),
            Ok(Some(PathBuf::from("out.png")))
        );
        assert_eq!(
            saved(&picker, SaveKind::Svg, "trash", None),
            Err(Error::Unavailable("no dialog"))
        );
        assert_eq!(saved(&picker, SaveKind::Svg, "x", None), Ok(None));
        assert_eq!(
            picker.asked_save(),
            [
                (SaveKind::Png, "trash".to_owned(), Some(3)),
                (SaveKind::Svg, "trash".to_owned(), None),
                (SaveKind::Svg, "x".to_owned(), None)
            ]
        );
        assert!(
            picker.asked().is_empty(),
            "pick and save are separate scripts"
        );
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
            picked(&picker, PickKind::Image, Some(7)),
            Ok(Some(PathBuf::from("a.png")))
        );
        assert_eq!(
            picked(&clone, PickKind::Any, None),
            Err(Error::Unavailable("no dialog"))
        );
        assert_eq!(picked(&picker, PickKind::Any, None), Ok(None));
        assert_eq!(
            picker.asked(),
            [
                (PickKind::Image, Some(7)),
                (PickKind::Any, None),
                (PickKind::Any, None)
            ]
        );
    }

    #[test]
    fn no_dialog_adds_its_file_to_recent() {
        for usage in [DialogUse::PickFolder, DialogUse::SaveFile] {
            assert_ne!(
                item_dialog_options(0, usage) & FOS_DONTADDTORECENT,
                0,
                "{usage:?}"
            );
            assert_ne!(
                item_dialog_options(0x1234, usage) & FOS_DONTADDTORECENT,
                0,
                "{usage:?} keeps what the dialog had and adds to it"
            );
        }
        assert_ne!(open_file_name_flags() & OFN_DONTADDTORECENT, 0);
        assert_eq!(FOS_DONTADDTORECENT, OFN_DONTADDTORECENT);
    }

    #[test]
    fn each_dialog_keeps_its_own_options() {
        let folder = item_dialog_options(0, DialogUse::PickFolder);
        assert_eq!(
            folder,
            FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST | FOS_DONTADDTORECENT
        );
        let save = item_dialog_options(0, DialogUse::SaveFile);
        assert_eq!(
            save,
            FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST
                | FOS_OVERWRITEPROMPT
                | FOS_NOCHANGEDIR
                | FOS_DONTADDTORECENT
        );
        assert_eq!(save & FOS_PICKFOLDERS, 0, "a save is not a folder pick");
        assert_eq!(
            open_file_name_flags(),
            OFN_EXPLORER
                | OFN_FILEMUSTEXIST
                | OFN_PATHMUSTEXIST
                | OFN_HIDEREADONLY
                | OFN_NOCHANGEDIR
                | OFN_DONTADDTORECENT
        );
    }
}
