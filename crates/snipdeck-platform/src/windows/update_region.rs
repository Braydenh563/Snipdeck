//! Whether Windows holds an invalid area of a window it made itself (render flicker,
//! `docs/reviews/render-flicker-2026-11-14.md`).
//!
//! winit answers `WM_PAINT` by sending `RedrawRequested` first and calling `DefWindowProcW`
//! (which runs `BeginPaint`, erases with the class brush and empties the region) after. Read
//! during `RedrawRequested`, the update region is still there: non-empty when Windows itself
//! invalidated part of the window (uncovered, dragged back on screen, shown again, resized),
//! empty for the app's own redraw request (`RedrawWindow(RDW_INTERNALPAINT)`). The frame drawn
//! then must cover the whole window, although nothing in Slint's scene changed.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::GetUpdateRect;

/// Whether `hwnd` has a non-empty update region. A null or stale handle answers `false`.
#[must_use]
pub fn has_update_region(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    let mut r = RECT::default();
    // SAFETY: a read-only query on a window handle; `false` leaves the region (and any pending
    // erase) as it is.
    unsafe { GetUpdateRect(HWND(hwnd as *mut _), Some(&raw mut r), false) }.as_bool()
}
