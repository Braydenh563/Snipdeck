//! The Windows backend. Each submodule owns one concern; see its header for the contract.
//!
//! | Module | Provides |
//! |---|---|
//! | [`clipboard`] | `WinClipboard`: [`crate::pipeline::Clipboard`] with delayed rendering |
//! | [`clipboard_history`] | Text entries of the Win+V clipboard history (`WinRT`) |
//! | [`keyboard`] | `WinKeyboard`: [`crate::pipeline::Keyboard`] via `SendInput` |
//! | [`target`] | Foreground tracking, UIPI check, caret / UI Automation focus probe |
//! | [`window_style`] | Non-activating panel style and its `WM_STYLECHANGING` subclass |
//! | [`screen`] | Mouse position and the work area and scale of the monitor under a point |
//! | [`hotkey`] | Global hotkeys on a dedicated thread |
//! | [`autostart`] | Start with Windows (`HKCU\...\Run`) |
//! | [`single_instance`] | One running instance; a second launch signals the first |
//! | [`console`] | Attach to the parent's console so `--help` and `--check-db` can print |
//! | [`dpi`] | Per-monitor DPI awareness for windows created before the UI toolkit's |
//! | [`dpapi`] | `CryptProtectData` wrappers for the sensitive-items key |
//! | [`tray`] | `WinTray`: [`crate::tray::Tray`] via `Shell_NotifyIconW` (v4), menu, `TaskbarCreated` |
//! | [`system_look`] | Light / dark and high contrast, and a watcher for changes |
//! | `hidden_window` | The hidden top-level window and thread that tray and system look share |
//!
//! None of this has run on Windows yet. Behaviour is checked by the wizard steps in
//! `tools/windows-test/tests/` (M2 section).

pub mod autostart;
pub mod clipboard;
pub mod clipboard_history;
pub mod console;
pub mod dpapi;
pub mod dpi;
pub mod file_dialog;
mod hidden_window;
pub mod hotkey;
pub mod keyboard;
pub mod screen;
pub mod single_instance;
pub mod system_look;
pub mod target;
pub mod tray;
pub mod window_style;
