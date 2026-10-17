//! Small Windows facts for the start-up report (`snipdeck-platform` does not expose them).

use std::ffi::c_void;
use std::mem::size_of;

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{MSG, PM_NOREMOVE, PeekMessageW};

/// `(major, minor, build)` from `RtlGetVersion` (not subject to the manifest lie of `GetVersionEx`).
pub fn windows_version() -> Option<(u32, u32, u32)> {
    let mut info = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid, correctly sized OSVERSIONINFOW.
    let status = unsafe { RtlGetVersion(&raw mut info) };
    status
        .is_ok()
        .then_some((info.dwMajorVersion, info.dwMinorVersion, info.dwBuildNumber))
}

/// Whether this process runs with an elevated (administrator) token.
pub fn is_elevated() -> Option<bool> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo handle of the current process is valid; `token` is a valid out-pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) }.ok()?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION of the stated size; `token` is a valid token handle.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast::<c_void>()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &raw mut returned,
        )
    };
    // SAFETY: `token` was opened above and is closed once.
    let _ = unsafe { CloseHandle(token) };
    result.ok()?;
    Some(elevation.TokenIsElevated != 0)
}

/// Creates this thread's message queue so `PostThreadMessageW` to it cannot fail for lack of one.
pub fn ensure_message_queue() {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid out-pointer; PM_NOREMOVE leaves the queue untouched.
    let _ = unsafe { PeekMessageW(&raw mut msg, None, 0, 0, PM_NOREMOVE) };
}
