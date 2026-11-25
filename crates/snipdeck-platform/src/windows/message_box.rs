//! A native message box, for the one moment the app has no window to speak with: a start-up
//! failure in the GUI-subsystem build, which has neither a console nor a window yet.
//!
//! **Unverified on Windows** (audit G2): the call compiles for the Windows target only.

use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    core::HSTRING,
};

/// Shows `text` under `title` with an OK button and waits for it to be dismissed. Failure to
/// show is ignored: there is nowhere left to report it.
pub fn show_error(title: &str, text: &str) {
    let (title, text) = (HSTRING::from(title), HSTRING::from(text));
    // SAFETY: both strings are NUL-terminated `HSTRING`s that outlive the call; no owner window.
    let _ = unsafe { MessageBoxW(None, &text, &title, MB_OK | MB_ICONERROR) };
}
