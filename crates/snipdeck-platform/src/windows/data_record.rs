//! `HKCU\Software\Snipdeck`, value `DataDir`: the data folder Setup's "erase my data" must delete.
//!
//! The decision (record a custom folder, clear a stale record) is [`crate::data_record::plan`]; this
//! file only reads and writes the value. Failures are returned and the caller ignores them: a missing
//! record costs nothing but a folder left behind at uninstall.
//!
//! **Unverified on Windows.** Check with `reg query HKCU\Software\Snipdeck` after starting with
//! `--data-dir` (see `tools/windows-test/`).

use std::ffi::c_void;
use std::io;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

use crate::data_record::{RecordAction, plan};

const KEY: PCWSTR = w!(r"Software\Snipdeck");
const VALUE: PCWSTR = w!("DataDir");

fn os_error(code: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(i32::try_from(code.0).unwrap_or(i32::MAX))
}

fn check(code: WIN32_ERROR) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(os_error(code))
    }
}

/// The recorded folder; `None` when there is none or it cannot be read.
fn read() -> Option<String> {
    let mut size = 0_u32;
    // SAFETY: static NUL-terminated names; only the size is requested (null data pointer).
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&raw mut size),
        )
    };
    if code != ERROR_SUCCESS || size < 2 {
        return None;
    }
    let mut buf = vec![0_u16; (size as usize).div_ceil(2) + 1];
    let mut size = u32::try_from(buf.len() * 2).ok()?;
    // SAFETY: `buf` is writable for `size` bytes; `RRF_RT_REG_SZ` limits the type to a string.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            Some(&raw mut size),
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

fn write(folder: &str) -> io::Result<()> {
    let bytes: Vec<u8> = folder
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut hkey = HKEY::default();
    // SAFETY: static NUL-terminated key name; `hkey` is a valid out-pointer; the handle is closed
    // below on every path.
    let code = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            KEY,
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &raw mut hkey,
            None,
        )
    };
    check(code)?;
    // SAFETY: `hkey` is open with `KEY_SET_VALUE`; `bytes` is a NUL-terminated UTF-16 `REG_SZ`.
    let code = unsafe { RegSetValueExW(hkey, VALUE, None, REG_SZ, Some(&bytes)) };
    // SAFETY: `hkey` came from the successful create above and is closed exactly once.
    let _ = unsafe { RegCloseKey(hkey) };
    check(code)
}

fn clear() -> io::Result<()> {
    let mut hkey = HKEY::default();
    // SAFETY: static NUL-terminated key name; `hkey` is a valid out-pointer.
    let code = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, KEY, None, KEY_SET_VALUE, &raw mut hkey) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(code)?;
    // SAFETY: `hkey` is open with `KEY_SET_VALUE`; the name is NUL-terminated.
    let code = unsafe { RegDeleteValueW(hkey, VALUE) };
    // SAFETY: closed exactly once.
    let _ = unsafe { RegCloseKey(hkey) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(code)
}

/// Brings the record in line with the folder in use (see [`crate::data_record::plan`]).
///
/// # Errors
///
/// The registry's OS error when writing or clearing the value fails.
pub fn sync(folder: &Path, default: Option<&Path>) -> io::Result<()> {
    match plan(read().as_deref(), folder, default) {
        RecordAction::Nothing => Ok(()),
        RecordAction::Write(s) => write(&s),
        RecordAction::Clear => clear(),
    }
}
