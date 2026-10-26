//! `WinFilePicker`: [`FilePicker`] on the system's file dialogs.
//!
//! A file is chosen with the standard Open dialog (`GetOpenFileNameW`) filtered by the kind's
//! extensions ([`PickKind::filters`]); a folder with the Vista-style folder picker
//! (`IFileOpenDialog` with `FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM`). Both are modal to `owner`
//! when one is given (Windows disables it while the dialog is up, so the UI toolkit receives no
//! events meanwhile), run on the calling thread and block it until the user chooses or cancels.
//! Neither changes the process's working directory. A file path longer than [`BUFFER_UNITS`] code
//! units is not offered: the dialog itself refuses it.
//!
//! A file to write is named with `IFileSaveDialog` (a suggested name, the kind's extension as the
//! default, and a prompt before replacing a file).
//!
//! **Unverified on Windows**: the dialogs compile for the Windows targets but have not been run.

use std::ffi::c_void;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
    OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
    FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::core::{HRESULT, PCWSTR, PWSTR};

use crate::error::Error;
use crate::file_dialog::{FilePicker, PickKind, SaveKind, filter_utf16, path_of_buffer};

/// The size of the buffer the dialog writes the chosen path into (the Win32 maximum path).
const BUFFER_UNITS: usize = 32_768;

/// The system's file and folder dialogs.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinFilePicker;

impl FilePicker for WinFilePicker {
    fn pick(&self, kind: PickKind, owner: Option<isize>) -> Result<Option<PathBuf>, Error> {
        let owner = HWND(owner.unwrap_or(0) as *mut c_void);
        if kind == PickKind::Folder {
            pick_folder(owner)
        } else {
            pick_file(kind, owner)
        }
    }

    fn save(
        &self,
        kind: SaveKind,
        suggested: &str,
        owner: Option<isize>,
    ) -> Result<Option<PathBuf>, Error> {
        save_file(kind, suggested, HWND(owner.unwrap_or(0) as *mut c_void))
    }
}

/// `text` as a NUL-ended UTF-16 string.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The Save dialog. `HRESULT_FROM_WIN32(ERROR_CANCELLED)` from `Show` is the user cancelling.
fn save_file(kind: SaveKind, suggested: &str, owner: HWND) -> Result<Option<PathBuf>, Error> {
    let os = |op: &'static str, e: &windows::core::Error| Error::Os {
        op,
        code: e.code().0.cast_unsigned(),
    };
    // The strings the filter list points into must outlive `Show`.
    let names: Vec<(Vec<u16>, Vec<u16>)> = kind
        .filters()
        .iter()
        .map(|(label, pattern)| (wide(label), wide(pattern)))
        .collect();
    let specs: Vec<COMDLG_FILTERSPEC> = names
        .iter()
        .map(|(label, pattern)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(pattern.as_ptr()),
        })
        .collect();
    let extension = wide(kind.extension());
    let name = wide(suggested);
    // SAFETY: no pointers; see `pick_folder` for why the result is only balanced when it succeeds.
    let _com = ComGuard(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok());
    // SAFETY: creates an in-process COM object of the type asked for.
    let dialog: IFileSaveDialog =
        unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| os("CoCreateInstance(FileSaveDialog)", &e))?;
    // SAFETY: method calls on the live COM object above. `specs` points into `names`, and
    // `extension` and `name` are NUL-ended; all of them outlive the calls (the dialog copies
    // the strings). `owner` is a handle from the toolkit (or null), which Windows validates.
    unsafe {
        let options = dialog
            .GetOptions()
            .map_err(|e| os("IFileDialog::GetOptions", &e))?;
        dialog
            .SetOptions(
                options
                    | FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_OVERWRITEPROMPT
                    | FOS_NOCHANGEDIR,
            )
            .map_err(|e| os("IFileDialog::SetOptions", &e))?;
        dialog
            .SetFileTypes(&specs)
            .map_err(|e| os("IFileDialog::SetFileTypes", &e))?;
        dialog
            .SetFileTypeIndex(1)
            .map_err(|e| os("IFileDialog::SetFileTypeIndex", &e))?;
        dialog
            .SetDefaultExtension(PCWSTR(extension.as_ptr()))
            .map_err(|e| os("IFileDialog::SetDefaultExtension", &e))?;
        dialog
            .SetFileName(PCWSTR(name.as_ptr()))
            .map_err(|e| os("IFileDialog::SetFileName", &e))?;
        if let Err(e) = dialog.Show(Some(owner)) {
            if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                return Ok(None);
            }
            return Err(os("IFileDialog::Show", &e));
        }
        let item = dialog
            .GetResult()
            .map_err(|e| os("IFileDialog::GetResult", &e))?;
        let path = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| os("IShellItem::GetDisplayName", &e))?;
        let chosen = PathBuf::from(std::ffi::OsString::from_wide(path.as_wide()));
        CoTaskMemFree(Some(path.0.cast_const().cast()));
        Ok(Some(chosen))
    }
}

fn pick_file(kind: PickKind, owner: HWND) -> Result<Option<PathBuf>, Error> {
    let filter = filter_utf16(kind.filters());
    let mut buffer = vec![0_u16; BUFFER_UNITS];
    let mut dialog = OPENFILENAMEW {
        lStructSize: u32::try_from(std::mem::size_of::<OPENFILENAMEW>())
            .map_err(|_| Error::Unavailable("the dialog structure is too large"))?,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        nFilterIndex: 1,
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: u32::try_from(buffer.len())
            .map_err(|_| Error::Unavailable("the dialog buffer is too large"))?,
        Flags: OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_HIDEREADONLY
            | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    // SAFETY: `dialog` is fully initialised: its size is right, `lpstrFilter` points into
    // `filter` (double-NUL ended) and `lpstrFile` into `buffer` (`nMaxFile` units), and both
    // vectors outlive the call. The call only writes into `buffer`. `hwndOwner` is a handle
    // the caller got from the toolkit (or null); Windows validates it.
    let chosen = unsafe { GetOpenFileNameW(&raw mut dialog) };
    if chosen.as_bool() {
        return Ok(path_of_buffer(&buffer));
    }
    // SAFETY: no pointers; reads the calling thread's last dialog error.
    let code = unsafe { CommDlgExtendedError() };
    if code.0 == 0 {
        // The user cancelled.
        return Ok(None);
    }
    Err(Error::Os {
        op: "GetOpenFileNameW",
        code: code.0,
    })
}

/// Balances a successful `CoInitializeEx` on this thread when dropped.
struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` in `pick_folder` on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// The folder picker. `HRESULT_FROM_WIN32(ERROR_CANCELLED)` from `Show` is the user cancelling.
fn pick_folder(owner: HWND) -> Result<Option<PathBuf>, Error> {
    let os = |op: &'static str, e: &windows::core::Error| Error::Os {
        op,
        code: e.code().0.cast_unsigned(),
    };
    // SAFETY: no pointers. The toolkit has usually initialised COM on this thread already, in
    // which case this fails harmlessly (a different mode) or just counts; only a success is
    // balanced by the guard.
    let _com = ComGuard(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok());
    // SAFETY: creates an in-process COM object of the type asked for.
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| os("CoCreateInstance(FileOpenDialog)", &e))?;
    // SAFETY: plain method calls on the live COM object above; `owner` is a handle from the
    // toolkit (or null), which Windows validates.
    unsafe {
        let options = dialog
            .GetOptions()
            .map_err(|e| os("IFileDialog::GetOptions", &e))?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
            .map_err(|e| os("IFileDialog::SetOptions", &e))?;
        if let Err(e) = dialog.Show(Some(owner)) {
            if e.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                return Ok(None);
            }
            return Err(os("IFileDialog::Show", &e));
        }
        let item = dialog
            .GetResult()
            .map_err(|e| os("IFileDialog::GetResult", &e))?;
        let name = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| os("IShellItem::GetDisplayName", &e))?;
        let path = PathBuf::from(std::ffi::OsString::from_wide(name.as_wide()));
        CoTaskMemFree(Some(name.0.cast_const().cast()));
        Ok(Some(path))
    }
}
