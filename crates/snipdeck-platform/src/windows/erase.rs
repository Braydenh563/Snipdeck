//! The colour Windows paints a window with before its first frame (O52).
//!
//! winit registers its window class with no background brush, so between `ShowWindow` and the
//! app's first presented frame Windows shows the window blank: white. The software renderer's
//! frame drawn before the window is mapped is thrown away (Windows keeps nothing for an unmapped
//! window), so every first show, and every show after a hide, flashed white for a moment.
//!
//! [`set_class_background`] gives the window class a solid brush in the theme's window colour.
//! `DefWindowProcW` fills the window with it on `WM_ERASEBKGND`, which winit leaves to it, so the
//! window shows the theme colour until the first frame replaces it. All of the app's windows
//! share winit's one class, so a call on any of them sets it for every window made later too.

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HGDIOBJ};
use windows::Win32::UI::WindowsAndMessaging::{GCLP_HBRBACKGROUND, SetClassLongPtrW};

/// The brush the class has now (0: none of ours), and its colour as `0x00BBGGRR` plus one (0:
/// none).
static BRUSH: AtomicIsize = AtomicIsize::new(0);
static COLOUR: AtomicU32 = AtomicU32::new(0);

/// Sets the background brush of `hwnd`'s window class to the opaque colour `rgb` (red, green,
/// blue). Call it on the thread that owns the window. Returns `false` for a null handle or when
/// Windows refuses; the same colour twice does nothing the second time.
pub fn set_class_background(hwnd: isize, (r, g, b): (u8, u8, u8)) -> bool {
    if hwnd == 0 {
        return false;
    }
    let colour = u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16);
    if COLOUR.load(Ordering::Relaxed) == colour + 1 {
        return true;
    }
    // SAFETY: plain GDI and window-class calls on a handle of this thread; a stale handle makes
    // `SetClassLongPtrW` fail, and the brush is then deleted again.
    unsafe {
        let brush = CreateSolidBrush(COLORREF(colour));
        if brush.is_invalid() {
            return false;
        }
        let _ = SetClassLongPtrW(HWND(hwnd as *mut _), GCLP_HBRBACKGROUND, brush.0 as isize);
        let old = BRUSH.swap(brush.0 as isize, Ordering::Relaxed);
        if old != 0 {
            let _ = DeleteObject(HGDIOBJ(old as *mut _));
        }
    }
    COLOUR.store(colour + 1, Ordering::Relaxed);
    true
}
