//! Where things are on the screen: the mouse, and the monitor under a point.
//!
//! The Picker and the fill form open at the Target's caret (or, with no caret, at the mouse)
//! and must stay inside the part of the monitor the taskbar leaves free. winit does not give
//! that work area, so it is read here.
//!
//! **Unverified on Windows** (`tools/windows-test/`).

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
};
use windows::Win32::System::DataExchange::{
    GetClipboardSequenceNumber, IsClipboardFormatAvailable,
};
use windows::Win32::System::Ole::{CF_BITMAP, CF_DIB, CF_DIBV5};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GUI_INMOVESIZE, GUITHREADINFO, GetCursorPos, GetGUIThreadInfo, GetSystemMetrics, SM_SWAPBUTTON,
};

use crate::route::ScreenRect;

/// What is known of one monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorFacts {
    /// The device name, as winit reports a monitor's name (`\\.\DISPLAY1`).
    pub device: String,
    /// The whole monitor.
    pub bounds: ScreenRect,
    /// The part of it that the taskbar and docked bars leave free.
    pub work_area: ScreenRect,
    /// Physical pixels per logical pixel (1.5 at 150 %).
    pub scale: f32,
}

/// The mouse position in physical pixels, or `None` if Windows will not say (a locked desktop).
#[must_use]
pub fn cursor_position() -> Option<(i32, i32)> {
    let mut at = POINT::default();
    // SAFETY: `at` is a valid POINT that outlives the call.
    unsafe { GetCursorPos(&raw mut at) }.ok()?;
    Some((at.x, at.y))
}

/// Whether the primary mouse button (the left one, or the right one for a left-handed mouse) is
/// held down right now, wherever the pointer is. The Panel's drag ends on it when the window
/// never saw the button come up.
#[must_use]
pub fn primary_button_down() -> bool {
    // SAFETY: no preconditions.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let key = if swapped { VK_RBUTTON } else { VK_LBUTTON };
    // SAFETY: no preconditions. The high bit says the physical button is down.
    let state = unsafe { GetAsyncKeyState(i32::from(key.0)) };
    state < 0
}

/// Whether the calling thread is inside the system's move or size loop (the one a window's
/// border or caption starts, which runs until the button comes up): `GUI_INMOVESIZE`. Asked on
/// the UI thread it says whether a resize of one of its windows is still under way, also
/// where the button itself cannot be read.
#[must_use]
pub fn in_size_loop() -> bool {
    let mut info = GUITHREADINFO {
        cbSize: u32::try_from(size_of::<GUITHREADINFO>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires, and outlives the
    // call; the thread id is the caller's own.
    let read = unsafe { GetGUIThreadInfo(GetCurrentThreadId(), &raw mut info) };
    read.is_ok() && info.flags.contains(GUI_INMOVESIZE)
}

/// The clipboard's change count and whether it holds a picture, for waiting on a screenshot.
/// Neither call opens the clipboard or reads what is on it, so nothing waits on another app and
/// no content is touched.
#[must_use]
pub fn clipboard_picture_state() -> (u32, bool) {
    // SAFETY: no preconditions.
    let sequence = unsafe { GetClipboardSequenceNumber() };
    let has = |format: u32| {
        // SAFETY: no preconditions.
        unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    };
    (
        sequence,
        has(u32::from(CF_DIB.0)) || has(u32::from(CF_DIBV5.0)) || has(u32::from(CF_BITMAP.0)),
    )
}

/// The monitor that contains `(x, y)`, or the nearest one when the point is outside them all.
#[must_use]
pub fn monitor_at(x: i32, y: i32) -> Option<MonitorFacts> {
    // SAFETY: plain call; `MONITOR_DEFAULTTONEAREST` never returns a null handle.
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFOEXW>()).ok()?,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires (a MONITORINFOEXW starts
    // with a MONITORINFO), and outlives the call.
    if !unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()) }.as_bool() {
        return None;
    }
    let device_len = info.szDevice.iter().position(|c| *c == 0).unwrap_or(0);
    let device = String::from_utf16_lossy(&info.szDevice[..device_len]);
    let info = info.monitorInfo;
    let (mut dpi_x, mut dpi_y) = (96_u32, 96_u32);
    // SAFETY: both out-parameters are valid and outlive the call; a failure leaves 96 DPI.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a DPI value is a few hundred at most"
    )]
    let scale = dpi_x.max(1) as f32 / 96.0;
    Some(MonitorFacts {
        device,
        bounds: rect(info.rcMonitor)?,
        work_area: rect(info.rcWork)?,
        scale,
    })
}

fn rect(r: RECT) -> Option<ScreenRect> {
    Some(ScreenRect {
        x: r.left,
        y: r.top,
        w: u32::try_from(r.right.checked_sub(r.left)?).ok()?,
        h: u32::try_from(r.bottom.checked_sub(r.top)?).ok()?,
    })
}
