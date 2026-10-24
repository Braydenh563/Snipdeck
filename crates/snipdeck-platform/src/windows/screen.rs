//! Where things are on the screen: the mouse, and the monitor under a point.
//!
//! The Picker and the fill form open at the Target's caret (or, with no caret, at the mouse)
//! and must stay inside the part of the monitor the taskbar leaves free. winit does not give
//! that work area, so it is read here.
//!
//! **Unverified on Windows** (`tools/windows-test/`).

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::DataExchange::{
    GetClipboardSequenceNumber, IsClipboardFormatAvailable,
};
use windows::Win32::System::Ole::{CF_BITMAP, CF_DIB, CF_DIBV5};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::route::ScreenRect;

/// What is known of one monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorFacts {
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
    let mut info = MONITORINFO {
        cbSize: u32::try_from(size_of::<MONITORINFO>()).ok()?,
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires, and outlives the call.
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (96_u32, 96_u32);
    // SAFETY: both out-parameters are valid and outlive the call; a failure leaves 96 DPI.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a DPI value is a few hundred at most"
    )]
    let scale = dpi_x.max(1) as f32 / 96.0;
    Some(MonitorFacts {
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
