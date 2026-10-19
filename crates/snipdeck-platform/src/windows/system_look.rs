//! Light / dark and high contrast, read from Windows and watched for changes.
//!
//! - **Dark mode:** `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`, DWORD
//!   `AppsUseLightTheme` (0 = apps dark; missing = light, as on Windows before 1809). Read with
//!   `RegGetValueW`, no UWP `UISettings` involved.
//! - **High contrast:** `SystemParametersInfoW(SPI_GETHIGHCONTRAST)`; when on, the five system
//!   colours the UI uses come from `GetSysColor`.
//! - **Changes:** [`SystemLookWatcher`] owns a hidden top-level window (broadcasts do not reach
//!   message-only windows, see `hidden_window.rs`) and calls `on_change` on
//!   its thread when a message that can change the look arrives (`WM_SETTINGCHANGE` with
//!   `ImmersiveColorSet` or `SPI_SETHIGHCONTRAST`, `WM_THEMECHANGED`, `WM_SYSCOLORCHANGE`; the
//!   filter is the portable [`message_may_change_look`]). The callback only says "read again":
//!   call [`WinSystemLook::read`] from the UI thread.
//!
//! **Unverified on Windows** (`tools/windows-test/tests/m3-app.json`).

use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    GetSysColor, SYS_COLOR_INDEX,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    WM_SETTINGCHANGE,
};
use windows::core::{PCWSTR, w};

use super::hidden_window::{HiddenWindow, WindowHandler};
use crate::error::Error;
use crate::system_look::{
    HighContrast, Rgb, SystemLook, SystemLookSource, message_may_change_look,
};

/// Reads the live system look. Stateless, so it is `Send + Sync` and cheap to copy.
#[derive(Debug, Clone, Copy, Default)]
pub struct WinSystemLook;

impl SystemLookSource for WinSystemLook {
    fn read(&self) -> SystemLook {
        read()
    }
}

/// The look Windows has right now. A value that cannot be read counts as "not set": light, no
/// high contrast.
#[must_use]
pub fn read() -> SystemLook {
    SystemLook {
        dark: apps_use_light_theme() == Some(false),
        high_contrast: high_contrast(),
        accent: accent_color(),
    }
}

/// `AppsUseLightTheme`: `Some(true)` light, `Some(false)` dark, `None` if the value is missing.
fn apps_use_light_theme() -> Option<bool> {
    let mut data: u32 = 0;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    // SAFETY: the key and value names are static NUL-terminated wide strings; `data` and `size`
    // are valid out-pointers, `size` holding the byte size of `data`; `RRF_RT_REG_DWORD` makes
    // the call fail rather than write anything but a DWORD.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast::<c_void>()),
            Some(&raw mut size),
        )
    };
    (code == ERROR_SUCCESS).then_some(data != 0)
}

/// The user's accent colour: `HKCU\Software\Microsoft\Windows\DWM`, DWORD `ColorizationColor`
/// (0xAARRGGBB). `None` when the value is missing.
fn accent_color() -> Option<Rgb> {
    let mut data: u32 = 0;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4);
    // SAFETY: as in `apps_use_light_theme`: static NUL-terminated names, valid out-pointers,
    // `RRF_RT_REG_DWORD` limits what is written to a DWORD.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\DWM"),
            w!("ColorizationColor"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast::<c_void>()),
            Some(&raw mut size),
        )
    };
    let [_, r, g, b] = data.to_be_bytes();
    (code == ERROR_SUCCESS).then_some(Rgb { r, g, b })
}

fn sys_color(index: SYS_COLOR_INDEX) -> Rgb {
    // SAFETY: `GetSysColor` has no preconditions; an invalid index would give 0, and these are
    // valid `COLOR_*` indices.
    Rgb::from_colorref(unsafe { GetSysColor(index) })
}

/// The system colours if a high-contrast theme is on.
fn high_contrast() -> Option<HighContrast> {
    let mut info = HIGHCONTRASTW {
        cbSize: u32::try_from(std::mem::size_of::<HIGHCONTRASTW>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is a valid `HIGHCONTRASTW` with `cbSize` set, as `SPI_GETHIGHCONTRAST`
    // requires; `uiparam` is its size. The scheme-name pointer is left null (we do not ask for
    // the name), which the call accepts.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            Some((&raw mut info).cast::<c_void>()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if read.is_err() || !info.dwFlags.contains(HCF_HIGHCONTRASTON) {
        return None;
    }
    Some(HighContrast {
        window: sys_color(COLOR_WINDOW),
        window_text: sys_color(COLOR_WINDOWTEXT),
        highlight: sys_color(COLOR_HIGHLIGHT),
        highlight_text: sys_color(COLOR_HIGHLIGHTTEXT),
        gray_text: sys_color(COLOR_GRAYTEXT),
    })
}

/// Watches for changes of the system look. Dropping it stops the thread.
pub struct SystemLookWatcher {
    _window: HiddenWindow,
}

impl std::fmt::Debug for SystemLookWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemLookWatcher").finish_non_exhaustive()
    }
}

impl SystemLookWatcher {
    /// Starts watching. `on_change` runs on the watcher's thread, possibly several times for
    /// one change; it should hand over to the UI thread and read the look there.
    ///
    /// # Errors
    ///
    /// [`Error::Unavailable`] if the thread cannot start; [`Error::Os`] if the window cannot be
    /// created.
    pub fn start(on_change: impl Fn() + Send + 'static) -> Result<Self, Error> {
        let window = HiddenWindow::spawn("snipdeck-look", "SnipdeckLookWindow", move || Watcher {
            on_change: Box::new(on_change),
        })?;
        Ok(Self { _window: window })
    }
}

struct Watcher {
    on_change: Box<dyn Fn() + Send>,
}

impl WindowHandler for Watcher {
    fn message(&self, _hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        // For a colour-mode change `wParam` is 0 and `lParam` is the area name. With a nonzero
        // `wParam` (an `SPI_*` code) `lParam` is not necessarily a string: do not read it.
        let area = (msg == WM_SETTINGCHANGE && wparam.0 == 0 && lparam.0 != 0)
            .then(|| {
                // SAFETY: for `WM_SETTINGCHANGE` with `wParam == 0`, a nonzero `lParam` points
                // to a NUL-terminated wide string that Windows keeps alive during the message.
                unsafe { PCWSTR(lparam.0 as *const u16).to_string() }.ok()
            })
            .flatten();
        if message_may_change_look(msg, wparam.0, area.as_deref()) {
            let _ = catch_unwind(AssertUnwindSafe(|| (self.on_change)()));
        }
        // Never "handled": `DefWindowProcW` is what answers these broadcasts.
        None
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_SETHIGHCONTRAST, WM_SYSCOLORCHANGE, WM_THEMECHANGED,
    };

    use super::*;
    use crate::system_look as portable;

    #[test]
    fn the_portable_message_codes_match_the_windows_constants() {
        assert_eq!(portable::WM_SETTINGCHANGE, WM_SETTINGCHANGE);
        assert_eq!(portable::WM_THEMECHANGED, WM_THEMECHANGED);
        assert_eq!(portable::WM_SYSCOLORCHANGE, WM_SYSCOLORCHANGE);
        assert_eq!(
            portable::SPI_SETHIGHCONTRAST,
            SPI_SETHIGHCONTRAST.0 as usize
        );
    }

    #[test]
    fn reading_the_look_does_not_fail() {
        // The value depends on the machine; this only proves the calls work and agree.
        assert_eq!(read(), WinSystemLook.read());
    }

    #[test]
    fn the_watcher_starts_and_stops() {
        let watcher = SystemLookWatcher::start(|| {}).unwrap();
        drop(watcher);
    }
}
