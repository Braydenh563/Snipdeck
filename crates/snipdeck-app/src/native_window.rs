//! Native windows: the Panel's handle and bringing it to the front, the monitor a window is
//! on, and keeping a regular window inside the screen.
//!
//! The Panel is created un-activated and never takes the focus (ADR-0003), so Windows does not
//! raise it when `show()` runs again after a hide: it comes back where it was in the stack,
//! behind the app the user has used since. [`raise`] puts it in front without activating it.
//! Off Windows (and under the headless test backend) there is no native handle and both
//! functions do nothing.

use slint::winit_030::{WinitWindowAccessor, winit};
use snipdeck_ui::PanelWindow;

use crate::frame::{self, Monitor, Rect};

/// The Win32 window handle of a winit window; none off Windows or without a handle.
#[cfg(windows)]
fn hwnd_of(window: &winit::window::Window) -> Option<isize> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// The Win32 window handle of `window`, once its native window exists.
#[cfg(windows)]
#[must_use]
pub fn handle_of(window: &slint::Window) -> Option<isize> {
    window.with_winit_window(hwnd_of).flatten()
}

/// The Win32 window handle of `window`: none off Windows.
#[cfg(not(windows))]
#[must_use]
pub fn handle_of(_window: &slint::Window) -> Option<isize> {
    None
}

/// The Win32 window handle of the Panel, once its window exists (none off Windows).
#[must_use]
pub fn handle(window: &PanelWindow) -> Option<isize> {
    use slint::ComponentHandle;
    handle_of(window.window())
}

/// Brings the shown Panel in front of the other windows without activating it.
pub fn raise(window: &PanelWindow) {
    use slint::ComponentHandle;
    raise_window(window.window());
}

/// Brings the shown window in front of the other windows without activating it.
#[cfg(windows)]
pub fn raise_window(window: &slint::Window) {
    if let Some(hwnd) = handle_of(window) {
        let raised = snipdeck_platform::windows::window_style::raise_without_activating(hwnd);
        crate::trace!("raise window {hwnd:#x}: {raised}");
    }
}


/// The work area of the monitor holding `window`: without the taskbar on Windows, none
/// elsewhere (winit reports only the whole monitor).
#[cfg(windows)]
fn work_area_of(window: &winit::window::Window) -> Option<Rect> {
    let (x, y, w, h) = snipdeck_platform::windows::window_style::work_area_of(hwnd_of(window)?)?;
    Some(Rect { x, y, w, h })
}

#[cfg(not(windows))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "same signature as the Windows version"
)]
fn work_area_of(_window: &winit::window::Window) -> Option<Rect> {
    None
}

/// The monitor `window` is on, with the area a window may use (without the taskbar on
/// Windows; the whole monitor where the host cannot tell).
#[must_use]
pub fn monitor_of(window: &winit::window::Window) -> Option<Monitor> {
    let monitor = window.current_monitor()?;
    let (size, at) = (monitor.size(), monitor.position());
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a display scale such as 1.5 fits an f32"
    )]
    let scale = monitor.scale_factor() as f32;
    let whole = Rect {
        x: at.x,
        y: at.y,
        w: size.width,
        h: size.height,
    };
    Some(Monitor {
        id: Monitor::id_of(&monitor.name().unwrap_or_default(), size.width, size.height),
        work_area: work_area_of(window).unwrap_or(whole),
        scale,
    })
}

/// Keeps a shown regular window (Main, Settings, an editor) inside its monitor's work area:
/// one that opened taller or wider than the screen, or under the taskbar, is cut to the area
/// and moved onto it. Call it right after `show()`; nothing happens without a native window.
pub fn fit_to_screen(window: &slint::Window) {
    let _ = window.with_winit_window(|w| {
        let Some(monitor) = monitor_of(w) else {
            return;
        };
        let Ok(at) = w.outer_position() else {
            return;
        };
        let (outer, inner) = (w.outer_size(), w.inner_size());
        let chrome = (
            outer.width.saturating_sub(inner.width),
            outer.height.saturating_sub(inner.height),
        );
        let now = Rect {
            x: at.x,
            y: at.y,
            w: outer.width,
            h: outer.height,
        };
        let fitted = frame::fit(now, monitor.work_area);
        if fitted == now {
            return;
        }
        crate::trace!(
            "fit window {now:?} -> {fitted:?} in {:?}",
            monitor.work_area
        );
        let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(
            fitted.w.saturating_sub(chrome.0).max(1),
            fitted.h.saturating_sub(chrome.1).max(1),
        ));
        w.set_outer_position(winit::dpi::PhysicalPosition::new(fitted.x, fitted.y));
    });
}

/// Brings `window` in front without activating it: nothing off Windows.
#[cfg(not(windows))]
pub fn raise_window(_window: &slint::Window) {}
