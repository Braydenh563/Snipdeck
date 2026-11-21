//! The DWM attributes of Snipdeck's windows: title bar theme and corner preference.
//!
//! The decisions (which attribute number on which Windows build) are in
//! [`crate::dwm_look`]; this file reads the build with `RtlGetVersion` and calls
//! `DwmSetWindowAttribute`. A failure is harmless: the window keeps what Windows draws.
//!
//! **Unverified on Windows** (PLATFORM-19, PLATFORM-20).

use std::ffi::c_void;
use std::sync::OnceLock;

use windows::Wdk::System::SystemServices::RtlGetVersion;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows::Win32::System::SystemInformation::OSVERSIONINFOW;

use crate::dwm_look::{self, DwmSetting};

/// The Windows build number (`10.0.<build>`), or 0 when Windows would not say. Read once;
/// `RtlGetVersion` is not subject to the compatibility shims `GetVersionEx` is.
#[must_use]
pub fn os_build() -> u32 {
    static BUILD: OnceLock<u32> = OnceLock::new();
    *BUILD.get_or_init(|| {
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: u32::try_from(std::mem::size_of::<OSVERSIONINFOW>()).unwrap_or(0),
            ..Default::default()
        };
        // SAFETY: `info` is a valid, correctly sized OSVERSIONINFOW for the call to fill.
        let status = unsafe { RtlGetVersion(&raw mut info) };
        if status.0 == 0 { info.dwBuildNumber } else { 0 }
    })
}

fn apply(hwnd: isize, setting: DwmSetting) -> bool {
    if hwnd == 0 {
        return false;
    }
    let value: u32 = setting.value;
    // SAFETY: `value` is a live 4-byte buffer for these DWORD attributes; a stale or foreign
    // handle makes the call fail, not crash.
    unsafe {
        DwmSetWindowAttribute(
            HWND(hwnd as *mut c_void),
            DWMWINDOWATTRIBUTE(i32::try_from(setting.attribute).unwrap_or(0)),
            (&raw const value).cast(),
            u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4),
        )
    }
    .is_ok()
}

/// Makes the title bar of `hwnd` dark (`dark`) or light, whatever the Windows app mode is.
/// False when the build has no such attribute or Windows refused.
pub fn set_dark_title_bar(hwnd: isize, dark: bool) -> bool {
    dwm_look::dark_title_bar(os_build(), dark).is_some_and(|setting| apply(hwnd, setting))
}

/// Asks for round corners on `hwnd` explicitly (Windows 11 only; nothing is set on Windows 10).
/// False when not applicable or Windows refused.
pub fn prefer_round_corners(hwnd: isize) -> bool {
    dwm_look::round_corners(os_build()).is_some_and(|setting| apply(hwnd, setting))
}

#[cfg(test)]
mod tests {
    use windows::Win32::Graphics::Dwm::{
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    use crate::dwm_look;

    /// The numbers in `dwm_look` are the Windows constants.
    #[test]
    fn constants_match_the_windows_crate() {
        assert_eq!(
            dwm_look::ATTRIBUTE_DARK_TITLE,
            u32::try_from(DWMWA_USE_IMMERSIVE_DARK_MODE.0).unwrap()
        );
        assert_eq!(
            dwm_look::ATTRIBUTE_CORNER_PREFERENCE,
            u32::try_from(DWMWA_WINDOW_CORNER_PREFERENCE.0).unwrap()
        );
        assert_eq!(
            dwm_look::CORNER_ROUND,
            u32::try_from(DWMWCP_ROUND.0).unwrap()
        );
    }
}
