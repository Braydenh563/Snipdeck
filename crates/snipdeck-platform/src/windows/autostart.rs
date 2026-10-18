//! "Start with Windows": the `Snipdeck` value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//!
//! The value is a `REG_SZ` holding the quoted executable path and the arguments (see
//! [`crate::command_line::run_value`]). Per-user, so it needs no elevation. Windows runs it at
//! logon; the user can also switch it off in Settings > Apps > Startup, which does not delete the
//! value (it records the choice under `StartupApproved\Run`), so [`is_enabled`] means "the value is
//! registered", not "will run at the next logon".
//!
//! **Unverified on Windows.** Covered by the M2 wizard step in `tools/windows-test/`.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{PCWSTR, w};

use crate::command_line::{CommandLineError, run_value};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE_NAME: PCWSTR = w!("Snipdeck");

/// An open registry key, closed on drop.
struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: `self.0` was returned by a successful `RegOpenKeyExW` / `RegCreateKeyExW` and is
        // closed exactly once, here. A failure to close cannot be handled meaningfully.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

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

fn invalid(error: CommandLineError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error)
}

/// Opens the `Run` key; `Ok(None)` if it does not exist.
fn open_run_key(
    access: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> io::Result<Option<Key>> {
    let mut hkey = HKEY::default();
    // SAFETY: `RUN_KEY` is a NUL-terminated static string, `hkey` is a valid out-pointer; on
    // success ownership of the handle moves into `Key`, which closes it.
    let code = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, access, &raw mut hkey) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check(code)?;
    Ok(Some(Key(hkey)))
}

/// Registers (`enabled`) or removes the "Start with Windows" entry.
///
/// Enabling writes `"<exe>" <args>` (see [`crate::command_line::run_value`]; `args` may be empty)
/// and replaces any previous value. Disabling an entry that does not exist succeeds.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidInput`] if the command line cannot be stored (empty or non-Unicode
/// path, a `"` or NUL, over 260 characters); otherwise the registry's OS error.
pub fn set_enabled(exe: &Path, args: &str, enabled: bool) -> io::Result<()> {
    if enabled {
        enable(exe, args)
    } else {
        disable()
    }
}

fn enable(exe: &Path, args: &str) -> io::Result<()> {
    let exe = exe.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the executable path is not Unicode",
        )
    })?;
    let command = run_value(exe, args).map_err(invalid)?;
    // REG_SZ: UTF-16 little endian including the terminating NUL (on Windows, always LE).
    let bytes: Vec<u8> = command
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();

    let mut hkey = HKEY::default();
    // SAFETY: `RUN_KEY` is a NUL-terminated static string; the class and security attributes are
    // omitted; `hkey` is a valid out-pointer and so is the (omitted) disposition. On success the
    // handle is owned by `Key` below.
    let code = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
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
    let key = Key(hkey);
    // SAFETY: `key.0` is an open key with `KEY_SET_VALUE`; `VALUE_NAME` is NUL-terminated; `bytes`
    // is a NUL-terminated UTF-16 string as `REG_SZ` requires and lives across the call.
    let code = unsafe { RegSetValueExW(key.0, VALUE_NAME, None, REG_SZ, Some(&bytes)) };
    check(code)
}

fn disable() -> io::Result<()> {
    let Some(key) = open_run_key(KEY_SET_VALUE)? else {
        return Ok(());
    };
    // SAFETY: `key.0` is an open key with `KEY_SET_VALUE`; `VALUE_NAME` is NUL-terminated.
    let code = unsafe { RegDeleteValueW(key.0, VALUE_NAME) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check(code)
}

/// Whether the `Snipdeck` value is registered under the `Run` key. Any failure reads as `false`.
#[must_use]
pub fn is_enabled() -> bool {
    let Ok(Some(key)) = open_run_key(KEY_QUERY_VALUE) else {
        return false;
    };
    let mut size = 0_u32;
    // SAFETY: `key.0` is an open key with `KEY_QUERY_VALUE`; `VALUE_NAME` is NUL-terminated; only
    // the size out-parameter is requested and it is a valid pointer to a `u32`.
    let code =
        unsafe { RegQueryValueExW(key.0, VALUE_NAME, None, None, None, Some(&raw mut size)) };
    code == ERROR_SUCCESS
}
