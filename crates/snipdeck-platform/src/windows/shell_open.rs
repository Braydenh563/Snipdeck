//! Opens a file, folder or link with its default handler through `ShellExecuteW`, and says
//! whether Windows accepted it (a spawned `explorer` process says nothing about that).
//!
//! **Unverified on Windows** (audit G14): compiles for the Windows target only.

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::{PCWSTR, w},
};

/// Opens `target` with the "open" verb.
///
/// # Errors
///
/// The `ShellExecuteW` result code (32 or less means failure).
pub fn open(target: &OsStr) -> Result<(), String> {
    let wide: Vec<u16> = target.encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; the verb is a static literal; the
    // other pointers are null, which the API allows.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // Documented: a value greater than 32 is success; the rest are error codes.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!("Windows could not open it (code {code})"))
    }
}
