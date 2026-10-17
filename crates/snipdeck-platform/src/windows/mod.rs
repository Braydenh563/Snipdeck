//! The Windows backend. Each submodule owns one concern; see its header for the contract.
//!
//! | Module | Provides |
//! |---|---|
//! | [`clipboard`] | `WinClipboard`: [`crate::pipeline::Clipboard`] with delayed rendering |
//! | [`keyboard`] | `WinKeyboard`: [`crate::pipeline::Keyboard`] via `SendInput` |
//! | [`target`] | Foreground tracking, UIPI check, caret / UI Automation focus probe |
//! | [`window_style`] | Non-activating panel style and its `WM_STYLECHANGING` subclass |
//! | [`hotkey`] | Global hotkeys on a dedicated thread |
//! | [`autostart`] | Start with Windows (`HKCU\...\Run`) |
//! | [`single_instance`] | One running instance; a second launch signals the first |
//! | [`dpapi`] | `CryptProtectData` wrappers for the sensitive-items key |
//!
//! None of this has run on Windows yet. Behaviour is checked by the wizard steps in
//! `tools/windows-test/tests/` (M2 section).

pub mod autostart;
pub mod clipboard;
pub mod dpapi;
pub mod hotkey;
pub mod keyboard;
pub mod single_instance;
pub mod target;
pub mod window_style;
