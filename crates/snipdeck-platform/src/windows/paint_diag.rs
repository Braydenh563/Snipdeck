//! `SNIPDECK_DIAG=render`: what Windows tells the app's windows about painting, for a log the
//! owner sends back (render glitches, `docs/reviews/render-glitch-analysis.md`).
//!
//! [`install`] puts two hooks on the calling (UI) thread, so every window of that thread is seen,
//! also the ones made later, without a subclass per window:
//!
//! - `WH_GETMESSAGE` sees `WM_PAINT` as it is taken off the queue, before winit handles it.
//!   [`GetUpdateRect`] then says whether Windows itself invalidated part of the window (an area
//!   uncovered, dragged back on screen, a restore) or the paint is the app's own redraw request
//!   (`RedrawWindow(RDW_INTERNALPAINT)`, no update region). Only the first kind is logged; the
//!   window is then remembered for [`take_os_invalidated`].
//! - `WH_CALLWNDPROC` sees the sent messages: `WM_ERASEBKGND` (Windows fills the update region
//!   with the class brush because no frame validated it), moves and sizes, the modal move loop,
//!   DPI and display changes, activation and showing.
//!
//! A `WM_PAINT` with an update region followed by a `WM_ERASEBKGND` for the same window means
//! the renderer presented nothing for that region.
//!
//! Off unless asked for; the log never carries window titles or content. **Unverified on
//! Windows.**

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::Write as _;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWINDOWATTRIBUTE,
    DwmGetWindowAttribute, DwmIsCompositionEnabled,
};
use windows::Win32::Graphics::Gdi::GetUpdateRect;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CWPSTRUCT, CallNextHookEx, GWL_EXSTYLE, GWL_STYLE, GetClientRect, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowRect, HC_ACTION, HHOOK, IsWindowVisible, MSG, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SetWindowsHookExW, WH_CALLWNDPROC,
    WH_GETMESSAGE, WINDOWPOS, WM_ACTIVATE, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_DWMCOMPOSITIONCHANGED, WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_PAINT,
    WM_SHOWWINDOW, WM_SIZE, WM_WINDOWPOSCHANGED,
};

/// Where the log lines go (the app's `trace` writer).
pub type LogFn = fn(&str);

thread_local! {
    static LOG: std::cell::Cell<Option<LogFn>> = const { std::cell::Cell::new(None) };
    /// Windows whose last `WM_PAINT` carried an update region Windows made.
    static OS_INVALIDATED: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    static HOOKS: std::cell::Cell<(isize, isize)> = const { std::cell::Cell::new((0, 0)) };
}

fn log(line: &str) {
    if let Some(f) = LOG.with(std::cell::Cell::get) {
        f(line);
    }
}

/// Installs the two hooks on this thread (the UI thread, before or after windows exist).
/// Calling it again only changes where the lines go. Returns `false` when Windows refused a hook.
pub fn install(log_fn: LogFn) -> bool {
    LOG.with(|l| l.set(Some(log_fn)));
    if HOOKS.with(std::cell::Cell::get) != (0, 0) {
        return true;
    }
    // SAFETY: thread hooks (no module handle, this thread's id) with procedures that match
    // `HOOKPROC` and live for the whole process. They are never removed: the process ends first.
    let (get, call) = unsafe {
        let thread = GetCurrentThreadId();
        (
            SetWindowsHookExW(WH_GETMESSAGE, Some(get_message_hook), None, thread),
            SetWindowsHookExW(WH_CALLWNDPROC, Some(call_wnd_proc_hook), None, thread),
        )
    };
    let (get, call) = (
        get.map(|h| h.0 as isize).unwrap_or(0),
        call.map(|h| h.0 as isize).unwrap_or(0),
    );
    HOOKS.with(|h| h.set((get, call)));
    log(&format!(
        "diag render: hooks installed get_message={} call_wnd_proc={}",
        get != 0,
        call != 0
    ));
    get != 0 && call != 0
}

/// Whether Windows invalidated part of `hwnd` since the last call (and forgets it). Only filled
/// while [`install`] is in force.
pub fn take_os_invalidated(hwnd: isize) -> bool {
    OS_INVALIDATED.with(|s| s.borrow_mut().remove(&hwnd))
}

fn rect_text(r: &RECT) -> String {
    format!(
        "({},{} {}x{})",
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top
    )
}

/// The virtual screen (all monitors) as `(x, y, width, height)` in physical pixels.
#[must_use]
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    // SAFETY: plain metric queries.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

fn dwm_u32(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE) -> String {
    let mut value: u32 = 0;
    // SAFETY: `value` is a valid 4-byte out buffer for these DWORD attributes.
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            (&raw mut value).cast(),
            u32::try_from(std::mem::size_of::<u32>()).unwrap_or(4),
        )
    };
    match result {
        Ok(()) => value.to_string(),
        Err(e) => format!("err {:#010x}", e.code().0),
    }
}

/// One line about `hwnd`: styles, layered or not, DPI, window and client rectangles, the DWM
/// attributes that decide what shows through undrawn pixels, and the virtual screen.
#[must_use]
pub fn describe_window(hwnd: isize) -> String {
    let h = HWND(hwnd as *mut _);
    let mut out = format!("hwnd={hwnd:#x}");
    // SAFETY: read-only queries on a window of this process; a stale handle makes them fail.
    unsafe {
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let _ = write!(
            out,
            " style={style:#x} exstyle={ex:#x} layered={} visible={} dpi={}",
            ex & 0x0008_0000 != 0,
            IsWindowVisible(h).as_bool(),
            GetDpiForWindow(h)
        );
        let mut r = RECT::default();
        if GetWindowRect(h, &raw mut r).is_ok() {
            let _ = write!(out, " window={}", rect_text(&r));
        }
        let mut c = RECT::default();
        if GetClientRect(h, &raw mut c).is_ok() {
            let _ = write!(out, " client={}", rect_text(&c));
        }
        let mut frame = RECT::default();
        if DwmGetWindowAttribute(
            h,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut frame).cast(),
            u32::try_from(std::mem::size_of::<RECT>()).unwrap_or(16),
        )
        .is_ok()
        {
            let _ = write!(out, " dwm_frame={}", rect_text(&frame));
        }
        let composition = DwmIsCompositionEnabled().map_or_else(
            |e| format!("err {:#010x}", e.code().0),
            |b| b.as_bool().to_string(),
        );
        let _ = write!(
            out,
            " dwm_composition={composition} backdrop_type={} dark_mode={} corners={} cloaked={}",
            dwm_u32(h, DWMWA_SYSTEMBACKDROP_TYPE),
            dwm_u32(h, DWMWA_USE_IMMERSIVE_DARK_MODE),
            dwm_u32(h, DWMWA_WINDOW_CORNER_PREFERENCE),
            dwm_u32(h, DWMWA_CLOAKED),
        );
    }
    let (x, y, w, hgt) = virtual_screen();
    let _ = write!(out, " virtual_screen=({x},{y} {w}x{hgt})");
    out
}

/// `WH_GETMESSAGE`: `WM_PAINT` as it leaves the queue.
unsafe extern "system" fn get_message_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == i32::try_from(HC_ACTION).unwrap_or(0) {
        // SAFETY: for HC_ACTION, `lparam` points to the MSG being retrieved; checked for null.
        if let Some(msg) = unsafe { (lparam.0 as *const MSG).as_ref() }
            && msg.message == WM_PAINT
        {
            let mut r = RECT::default();
            // SAFETY: read-only query; `false` leaves the region as it is.
            let has = unsafe { GetUpdateRect(msg.hwnd, Some(&raw mut r), false) }.as_bool();
            if has {
                let hwnd = msg.hwnd.0 as isize;
                OS_INVALIDATED.with(|s| s.borrow_mut().insert(hwnd));
                log(&format!(
                    "diag render: WM_PAINT hwnd={hwnd:#x} os_update={}",
                    rect_text(&r)
                ));
            }
        }
    }
    // SAFETY: passes the hook on unchanged, as every hook must.
    unsafe { CallNextHookEx(Some(HHOOK::default()), code, wparam, lparam) }
}

/// `WH_CALLWNDPROC`: the sent messages that matter for painting.
unsafe extern "system" fn call_wnd_proc_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == i32::try_from(HC_ACTION).unwrap_or(0) {
        // SAFETY: for HC_ACTION, `lparam` points to a CWPSTRUCT for this call; checked for null.
        if let Some(cwp) = unsafe { (lparam.0 as *const CWPSTRUCT).as_ref() } {
            let hwnd = cwp.hwnd.0 as isize;
            let what = match cwp.message {
                WM_ERASEBKGND => Some("WM_ERASEBKGND (class brush fills the update region)".into()),
                WM_WINDOWPOSCHANGED => {
                    // SAFETY: for this message `lParam` points to a WINDOWPOS; checked for null.
                    unsafe { (cwp.lParam.0 as *const WINDOWPOS).as_ref() }.map(|p| {
                        format!(
                            "WM_WINDOWPOSCHANGED at=({},{}) size={}x{} flags={:#x}",
                            p.x, p.y, p.cx, p.cy, p.flags.0
                        )
                    })
                }
                WM_SIZE => Some(format!(
                    "WM_SIZE type={} {}x{}",
                    cwp.wParam.0,
                    cwp.lParam.0 & 0xffff,
                    (cwp.lParam.0 >> 16) & 0xffff
                )),
                WM_ENTERSIZEMOVE => Some("WM_ENTERSIZEMOVE (modal move/size loop starts)".into()),
                WM_EXITSIZEMOVE => Some("WM_EXITSIZEMOVE".into()),
                WM_DPICHANGED => Some(format!("WM_DPICHANGED dpi={}", cwp.wParam.0 & 0xffff)),
                WM_DISPLAYCHANGE => Some("WM_DISPLAYCHANGE".into()),
                WM_DWMCOMPOSITIONCHANGED => Some("WM_DWMCOMPOSITIONCHANGED".into()),
                WM_SHOWWINDOW => Some(format!("WM_SHOWWINDOW show={}", cwp.wParam.0)),
                WM_ACTIVATE => Some(format!("WM_ACTIVATE state={}", cwp.wParam.0 & 0xffff)),
                _ => None,
            };
            if let Some(what) = what {
                log(&format!("diag render: {what} hwnd={hwnd:#x}"));
            }
        }
    }
    // SAFETY: passes the hook on unchanged, as every hook must.
    unsafe { CallNextHookEx(Some(HHOOK::default()), code, wparam, lparam) }
}
