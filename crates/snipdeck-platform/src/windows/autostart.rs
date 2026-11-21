//! "Start with Windows": the `Snipdeck` value under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//!
//! The value is a `REG_SZ` holding the quoted executable path and the arguments (see
//! [`crate::command_line::run_value`]). Per-user, so it needs no elevation. Windows runs it at
//! logon; the user can also switch it off in Task Manager or Settings > Apps > Startup, which does
//! not delete the value (it records the choice under `Explorer\StartupApproved\Run`), so
//! [`status`] reads both and says [`AutostartStatus::SwitchedOffInWindows`] for that case. At
//! start-up [`rewrite_if_stale`] points a value left by a moved or older install at this
//! executable. The decisions are in [`crate::command_line`].
//!
//! **Unverified on Windows.** Covered by the M2 wizard step in `tools/windows-test/`.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows::core::{PCWSTR, w};

use crate::command_line::{
    AutostartStatus, CommandLineError, autostart_status, run_value, stale_run_value,
};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const APPROVED_KEY: PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
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
    open_key(RUN_KEY, access)
}

/// Opens `path` under `HKEY_CURRENT_USER`; `Ok(None)` if it does not exist.
fn open_key(
    path: PCWSTR,
    access: windows::Win32::System::Registry::REG_SAM_FLAGS,
) -> io::Result<Option<Key>> {
    let mut hkey = HKEY::default();
    // SAFETY: `path` is a NUL-terminated static string, `hkey` is a valid out-pointer; on
    // success ownership of the handle moves into `Key`, which closes it.
    let code = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path, None, access, &raw mut hkey) };
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
    write_run_value(&command)?;
    // Switching it on here is a deliberate choice: drop Windows' "switched off" record for the
    // value, or the entry would stay disabled (Windows treats a value without a record as on).
    // A failure leaves the record, and `status` then says so.
    let _ = clear_approval();
    Ok(())
}

/// Writes `command` as the `Run` value.
fn write_run_value(command: &str) -> io::Result<()> {
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

/// Removes the `StartupApproved\Run` record of our value; absent is fine.
fn clear_approval() -> io::Result<()> {
    let Some(key) = open_key(APPROVED_KEY, KEY_SET_VALUE)? else {
        return Ok(());
    };
    // SAFETY: `key.0` is an open key with `KEY_SET_VALUE`; `VALUE_NAME` is NUL-terminated.
    let code = unsafe { RegDeleteValueW(key.0, VALUE_NAME) };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
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

/// The raw bytes and type of our value under `path`; `None` if the key or value is missing or
/// unreadable.
fn read_value(path: PCWSTR) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
    let key = open_key(path, KEY_QUERY_VALUE).ok()??;
    let mut size = 0_u32;
    let mut kind = REG_VALUE_TYPE::default();
    // SAFETY: `key.0` is an open key with `KEY_QUERY_VALUE`; `VALUE_NAME` is NUL-terminated;
    // only the type and size out-parameters are requested and both are valid pointers.
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            VALUE_NAME,
            None,
            Some(&raw mut kind),
            None,
            Some(&raw mut size),
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    let mut data = vec![0_u8; usize::try_from(size).ok()?];
    let mut got = size;
    // SAFETY: as above; `data` is `size` bytes long and `got` says so, and both outlive the call.
    let code = unsafe {
        RegQueryValueExW(
            key.0,
            VALUE_NAME,
            None,
            Some(&raw mut kind),
            Some(data.as_mut_ptr()),
            Some(&raw mut got),
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    data.truncate(usize::try_from(got).ok()?);
    Some((kind, data))
}

/// The command stored in the `Run` value, if there is one.
fn stored_command() -> Option<String> {
    let (kind, data) = read_value(RUN_KEY)?;
    if kind != REG_SZ {
        return None;
    }
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// What "Start with Windows" is in the registry: off, on, or registered but switched off in
/// Windows (Task Manager, Settings > Apps > Startup). Any failure to read reads as off.
#[must_use]
pub fn status() -> AutostartStatus {
    let approved = read_value(APPROVED_KEY).map(|(_, data)| data);
    autostart_status(stored_command().as_deref(), approved.as_deref())
}

/// At start-up with the setting on: if the stored command is not the one this executable would
/// register (a portable folder moved, an old install), writes the current one. An absent value
/// stays absent. Returns whether it rewrote. **Unverified on Windows.**
///
/// # Errors
///
/// As [`set_enabled`].
pub fn rewrite_if_stale(exe: &Path, args: &str, setting_on: bool) -> io::Result<bool> {
    let exe_text = exe.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the executable path is not Unicode",
        )
    })?;
    let expected = run_value(exe_text, args).map_err(invalid)?;
    if !stale_run_value(setting_on, stored_command().as_deref(), &expected) {
        return Ok(false);
    }
    // Keep Windows' own "switched off" record: only `set_enabled` (the user's choice) clears it.
    write_run_value(&expected)?;
    Ok(true)
}
