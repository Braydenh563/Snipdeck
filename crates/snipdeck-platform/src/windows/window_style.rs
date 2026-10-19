//! The non-activating panel style, and the subclass that keeps it.
//!
//! **The app creates the panel window with winit `with_active(false)`** (and
//! `with_skip_taskbar(true)`): winit then shows it with `SW_SHOWNOACTIVATE`, now and on every
//! later re-show. This crate does not depend on winit or Slint, so that part lives in the app.
//! What this module adds on top, once the app has the window's `HWND`:
//!
//! - `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` without `WS_EX_APPWINDOW`: a click never activates the
//!   panel, and it has no taskbar button or Alt+Tab entry. With the "Show Snipdeck in the
//!   taskbar" setting on, `WS_EX_APPWINDOW` replaces `WS_EX_TOOLWINDOW` (still never activated by
//!   a click); [`set_panel_taskbar`] switches between the two while the panel is up.
//! - A window subclass that re-applies those bits whenever the extended style is about to change
//!   (`WM_STYLECHANGING`). winit rewrites `GWL_EXSTYLE` from its own flags every time Slint
//!   changes a window property (title, decorations, level, visibility), which would otherwise
//!   drop the bits again. The subclass also answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE`.
//!
//! Text entry is the exception to "never activated": see
//! [`TargetTracker::begin_text_entry`](super::target::TargetTracker::begin_text_entry), which
//! activates the panel explicitly with `SetForegroundWindow`.

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_NOTOPMOST, HWND_TOPMOST, MA_NOACTIVATE, STYLESTRUCT,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowLongPtrW,
    SetWindowPos, WM_MOUSEACTIVATE, WM_NCDESTROY, WM_STYLECHANGING, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};

use crate::win_logic::{self, ZStep, panel_ex_style};

const _: () = {
    assert!(win_logic::WS_EX_NOACTIVATE == WS_EX_NOACTIVATE.0);
    assert!(win_logic::WS_EX_TOOLWINDOW == WS_EX_TOOLWINDOW.0);
    assert!(win_logic::WS_EX_APPWINDOW == WS_EX_APPWINDOW.0);
};

/// Subclass id; unique among the subclasses this crate installs on a window.
const SUBCLASS_ID: usize = 0x5344_5043; // "SDPC"

/// Makes the window `hwnd` a non-activating panel and keeps it one.
///
/// Call it on the thread that owns the window (the UI thread), after the window exists and
/// before it is first shown if possible. Calling it again is harmless. The subclass removes itself
/// on `WM_NCDESTROY`, the last message the window receives; nothing is left to clean up.
///
/// `hwnd` is the raw window handle value (winit's `Win32WindowHandle::hwnd`); `in_taskbar` is
/// the "Show Snipdeck in the taskbar" setting. Returns `false` when the handle is null or the
/// subclass could not be installed; the style is then not applied either, because without the
/// guard it would not last.
pub fn install_panel_style(hwnd: isize, in_taskbar: bool) -> bool {
    if hwnd == 0 {
        return false;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    // Subclass first, so the guard is in place before the style is set.
    // SAFETY: `hwnd` is a window of this thread (caller contract); `panel_subclass_proc` matches
    // `SUBCLASSPROC` and stays valid for the life of the process.
    let installed = unsafe {
        SetWindowSubclass(
            hwnd,
            Some(panel_subclass_proc),
            SUBCLASS_ID,
            usize::from(in_taskbar),
        )
    };
    if !installed.as_bool() {
        return false;
    }
    // SAFETY: plain calls on the same window handle. A stale handle makes them fail, not crash.
    unsafe {
        let current = ex_style_bits(GetWindowLongPtrW(hwnd, GWL_EXSTYLE));
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex_style_slot(panel_ex_style(current, in_taskbar)),
        );
        // Not hidden or resized by this call; it only makes Windows re-read the frame styles.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    true
}

/// Switches the panel `hwnd` (already given [`install_panel_style`]) between "has a taskbar
/// button" and "has none", live. Windows only refreshes the taskbar when the window is hidden and
/// shown again, so a visible panel is hidden and shown without activating it. Returns `false`
/// for a null handle or when the subclass is gone.
pub fn set_panel_taskbar(hwnd: isize, in_taskbar: bool) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SW_HIDE, SW_SHOWNOACTIVATE, ShowWindow,
    };
    if hwnd == 0 {
        return false;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    // SAFETY: `hwnd` is a window of this thread (caller contract); the subclass procedure
    // matches `SUBCLASSPROC`. Setting it again replaces the reference data, which is what the
    // `WM_STYLECHANGING` branch reads.
    let installed = unsafe {
        SetWindowSubclass(
            hwnd,
            Some(panel_subclass_proc),
            SUBCLASS_ID,
            usize::from(in_taskbar),
        )
    };
    if !installed.as_bool() {
        return false;
    }
    // SAFETY: plain calls on the same window handle. A stale handle makes them fail, not crash.
    unsafe {
        let visible = IsWindowVisible(hwnd).as_bool();
        if visible {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        let current = ex_style_bits(GetWindowLongPtrW(hwnd, GWL_EXSTYLE));
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex_style_slot(panel_ex_style(current, in_taskbar)),
        );
        if visible {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
    true
}

/// Puts the window `hwnd` in front of the other windows without activating it, so the Panel
/// shown from the tray or a hotkey is not left behind the app the user was in (see
/// [`win_logic::raise_steps`]). Call it on the thread that owns the window, after showing it.
/// Returns `false` for a null handle; a refused z-order change is ignored (the window stays
/// where it is, as before this call).
pub fn raise_without_activating(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    // SAFETY: plain calls on a window handle of this thread; a stale handle makes them fail.
    unsafe {
        let was_topmost =
            ex_style_bits(GetWindowLongPtrW(hwnd, GWL_EXSTYLE)) & WS_EX_TOPMOST.0 != 0;
        for step in win_logic::raise_steps(was_topmost) {
            let after = match step {
                ZStep::Topmost => HWND_TOPMOST,
                ZStep::NotTopmost => HWND_NOTOPMOST,
            };
            let _ = SetWindowPos(
                hwnd,
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    true
}

/// The 32 style bits of a `GetWindowLongPtrW` result.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "extended styles are a 32-bit flag set stored in a pointer-sized slot"
)]
fn ex_style_bits(raw: isize) -> u32 {
    raw as u32
}

/// A style flag set as the pointer-sized value `SetWindowLongPtrW` takes.
#[expect(
    clippy::cast_possible_wrap,
    reason = "extended styles use at most 31 bits, so the value stays positive"
)]
fn ex_style_slot(bits: u32) -> isize {
    bits as isize
}

/// The style index a `WM_STYLECHANGING` message is about. `wparam` carries it sign extended
/// (`GWL_STYLE` and `GWL_EXSTYLE` are negative), so truncating to 32 bits recovers it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "see above: the index is a sign-extended 32-bit value"
)]
fn style_index(wparam: WPARAM) -> i32 {
    wparam.0 as i32
}

/// Subclass procedure, called before winit's own window procedure.
///
/// Runs inside the message loop: no panics, no unwinding, no blocking.
unsafe extern "system" fn panel_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    match msg {
        // Belt and braces with WS_EX_NOACTIVATE: a click must not activate the panel.
        WM_MOUSEACTIVATE => return LRESULT(isize::try_from(MA_NOACTIVATE).unwrap_or(3)),
        WM_STYLECHANGING if style_index(wparam) == GWL_EXSTYLE.0 => {
            // SAFETY: for WM_STYLECHANGING, `lparam` points to a STYLESTRUCT that the system
            // owns for the duration of this call; it is checked for null before use.
            if let Some(style) = unsafe { (lparam.0 as *mut STYLESTRUCT).as_mut() } {
                style.styleNew = panel_ex_style(style.styleNew, data != 0);
            }
        }
        // The subclass is not removed by the system: remove it here, last message of the window.
        WM_NCDESTROY => {
            // SAFETY: removes this very subclass from the window being destroyed; the chain
            // is still intact during this message, and `DefSubclassProc` below is the
            // documented way to finish the message after removal.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(panel_subclass_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    // SAFETY: forwards the message unchanged to the next procedure in the subclass chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// The work area and DPI of the monitor the mouse pointer is on: `(x, y, width, height, dpi)` in
/// physical pixels. A window about to be created (the Main window, Settings) is sized from it
/// before it exists, because the window's own monitor is not known yet and a window taller than
/// the work area opens half off the screen. `None` if Windows cannot say.
#[must_use]
pub fn work_area_near_cursor() -> Option<(i32, i32, u32, u32, u32)> {
    use windows::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::GetCursorPos,
        },
    };
    let mut at = POINT::default();
    let mut info = MONITORINFO {
        cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).ok()?,
        ..Default::default()
    };
    let (mut dpi_x, mut dpi_y) = (0_u32, 0_u32);
    // SAFETY: `at`, `info` and the two DPI values are properly sized and outlive the calls; a
    // failure is reported through the return values, not by crashing.
    let ok = unsafe {
        GetCursorPos(&raw mut at).is_ok() && {
            let monitor = MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST);
            GetMonitorInfoW(monitor, &raw mut info).as_bool()
                && GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y)
                    .is_ok()
        }
    };
    if !ok {
        return None;
    }
    let r = info.rcWork;
    let (w, h) = (
        u32::try_from(r.right - r.left).ok()?,
        u32::try_from(r.bottom - r.top).ok()?,
    );
    (w > 0 && h > 0 && dpi_x > 0).then_some((r.left, r.top, w, h, dpi_x))
}

/// The work area (the monitor without the taskbar and other app bars) of the monitor that
/// holds the window `hwnd`, as `(x, y, width, height)` in physical pixels; `None` for a null or
/// stale handle. winit only reports the whole monitor, which is why windows ended up under the
/// taskbar.
#[must_use]
pub fn work_area_of(hwnd: isize) -> Option<(i32, i32, u32, u32)> {
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    if hwnd == 0 {
        return None;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    let mut info = MONITORINFO {
        cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>()).ok()?,
        ..Default::default()
    };
    // SAFETY: `info` is a properly sized MONITORINFO that outlives the call; a stale handle
    // only makes the call fail.
    let ok = unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        GetMonitorInfoW(monitor, &raw mut info).as_bool()
    };
    if !ok {
        return None;
    }
    let r = info.rcWork;
    let (w, h) = (
        u32::try_from(r.right - r.left).ok()?,
        u32::try_from(r.bottom - r.top).ok()?,
    );
    (w > 0 && h > 0).then_some((r.left, r.top, w, h))
}
