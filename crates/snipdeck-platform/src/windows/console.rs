//! Console output for a GUI-subsystem build.
//!
//! The release build is a Windows GUI program (no console window flashes up when it is started
//! from the Start menu or the tray). That also means it has no standard output unless it
//! attaches to the console of the program that started it. `--help`, `--check-db` and
//! `--troubleshoot` print to the terminal, so they call [`attach_parent_console`] first.
//!
//! **Unverified on Windows** (Fable v1 sweep A1).

use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

/// Attaches the process to its parent's console, if the parent has one, so that `println!`
/// reaches the terminal it was started from. Failure (no parent console, as when started from
/// Explorer) is ignored: there is nowhere to print then.
pub fn attach_parent_console() {
    // SAFETY: takes a documented process-id constant and no pointers; fails (rather than
    // misbehaves) when the process already has a console or the parent has none.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
