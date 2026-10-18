//! `WinFilePicker`: [`FilePicker`] on the standard Open dialog (`GetOpenFileNameW`).
//!
//! The dialog is modal to `owner` when one is given (Windows disables it while the dialog is up,
//! so the UI toolkit receives no events meanwhile), runs on the calling thread and blocks it
//! until the user chooses or cancels. It never changes the process's working directory
//! (`OFN_NOCHANGEDIR`). A path longer than [`BUFFER_UNITS`] code units is not offered: the
//! dialog itself refuses it.
//!
//! **Unverified on Windows.**

use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY,
    OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR};

use crate::error::Error;
use crate::file_dialog::{FilePicker, PickKind, filter_utf16, path_of_buffer};

/// The size of the buffer the dialog writes the chosen path into (the Win32 maximum path).
const BUFFER_UNITS: usize = 32_768;

/// The standard Open dialog.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinFilePicker;

impl FilePicker for WinFilePicker {
    fn pick(&self, kind: PickKind, owner: Option<isize>) -> Result<Option<PathBuf>, Error> {
        let filter = match kind {
            PickKind::Image => filter_utf16(&[("PNG pictures", "*.png"), ("All files", "*.*")]),
            PickKind::Any => filter_utf16(&[("All files", "*.*")]),
        };
        let mut buffer = vec![0_u16; BUFFER_UNITS];
        let mut dialog = OPENFILENAMEW {
            lStructSize: u32::try_from(std::mem::size_of::<OPENFILENAMEW>())
                .map_err(|_| Error::Unavailable("the dialog structure is too large"))?,
            hwndOwner: HWND(owner.unwrap_or(0) as *mut c_void),
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
}
