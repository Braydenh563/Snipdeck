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
//!
//! The brush must not land on a frame already presented, though. winit presents the frame of a
//! `WM_PAINT` first and calls `DefWindowProcW` after it, and its `BeginPaint` sends the erase
//! Windows owes: the brush then covered the new frame until the next one, a dark blink while the
//! Panel was dragged. [`guard_presented_frames`] gives a window a subclass that skips that erase
//! ([`EraseGate`]); the app reports each frame with [`frame_presented`].

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HGDIOBJ};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GCLP_HBRBACKGROUND, SetClassLongPtrW, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT,
};

use crate::win_logic::EraseGate;

/// Subclass id; unique among the subclasses this crate installs on a window.
const SUBCLASS_ID: usize = 0x5344_4542; // "SDEB"

thread_local! {
    static GATE: RefCell<EraseGate> = RefCell::new(EraseGate::default());
    static SKIPPED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

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

/// Gives the window `hwnd` the subclass that keeps a presented frame from being erased. Call it
/// on the thread that owns the window; again is harmless. Returns `false` for a null handle or
/// when the subclass could not be installed. The subclass removes itself on `WM_NCDESTROY`.
pub fn guard_presented_frames(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    // SAFETY: `hwnd` is a window of this thread (caller contract); `erase_subclass_proc` matches
    // `SUBCLASSPROC` and stays valid for the life of the process.
    unsafe {
        SetWindowSubclass(
            HWND(hwnd as *mut _),
            Some(erase_subclass_proc),
            SUBCLASS_ID,
            0,
        )
    }
    .as_bool()
}

/// A frame of `hwnd` is drawn and presented now (`RedrawRequested`, which Slint always answers
/// with a frame).
pub fn frame_presented(hwnd: isize) {
    GATE.with(|g| g.borrow_mut().frame_presented(hwnd));
}

/// How many erases the subclass has skipped on this thread so far (`SNIPDECK_DIAG=render`).
#[must_use]
pub fn skipped_erases() -> u64 {
    SKIPPED.with(std::cell::Cell::get)
}

unsafe extern "system" fn erase_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let id = hwnd.0 as isize;
    match msg {
        WM_PAINT => {
            GATE.with(|g| g.borrow_mut().paint_begin(id));
            // SAFETY: forwards the message unchanged to the next procedure in the chain (winit's).
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            GATE.with(|g| g.borrow_mut().paint_end(id));
            return result;
        }
        // Non-zero: "erased". The frame just presented covers the window; the brush would not.
        WM_ERASEBKGND if GATE.with(|g| g.borrow().keep_frame(id)) => {
            SKIPPED.with(|n| n.set(n.get() + 1));
            return LRESULT(1);
        }
        WM_NCDESTROY => {
            // SAFETY: removes this very subclass from the window being destroyed; the chain is
            // still intact during this message.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(erase_subclass_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    // SAFETY: forwards the message unchanged to the next procedure in the subclass chain.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
