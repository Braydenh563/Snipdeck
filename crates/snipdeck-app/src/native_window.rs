//! The Panel's native window: its handle, and bringing it to the front when it is shown again.
//!
//! The Panel is created un-activated and never takes the focus (ADR-0003), so Windows does not
//! raise it when `show()` runs again after a hide: it comes back where it was in the stack,
//! behind the app the user has used since. [`raise`] puts it in front without activating it.
//! Off Windows (and under the headless test backend) there is no native handle and both
//! functions do nothing.

use snipdeck_ui::PanelWindow;

/// The Win32 window handle of the Panel, once its window exists (the event loop makes it when
/// the Panel is first shown; later hides and shows keep the same window).
#[cfg(windows)]
#[must_use]
pub fn handle(window: &PanelWindow) -> Option<isize> {
    use slint::ComponentHandle;
    use slint::winit_030::WinitWindowAccessor;
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    window
        .window()
        .with_winit_window(|w| match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        })
        .flatten()
}

/// The Win32 window handle of the Panel: none off Windows.
#[cfg(not(windows))]
#[must_use]
pub fn handle(_window: &PanelWindow) -> Option<isize> {
    None
}

/// Brings the shown Panel in front of the other windows without activating it.
#[cfg(windows)]
pub fn raise(window: &PanelWindow) {
    if let Some(hwnd) = handle(window) {
        let raised = snipdeck_platform::windows::window_style::raise_without_activating(hwnd);
        crate::trace!("raise panel {hwnd:#x}: {raised}");
    }
}

/// Brings the shown Panel in front of the other windows: nothing to do off Windows.
#[cfg(not(windows))]
pub fn raise(_window: &PanelWindow) {}
