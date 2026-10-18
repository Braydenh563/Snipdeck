//! DPI awareness for the services that create windows before the UI toolkit does.
//!
//! The app's windows come from winit, which makes the process per-monitor aware when its event
//! loop is created. The tray's hidden window (and its popup menu, and the icon size it asks for)
//! is created earlier, in `platform::connect`; a process that is not yet aware gets 96-DPI
//! metrics and a bitmap-stretched menu at 150 % / 200 % scaling. Two defences:
//!
//! - [`enable_per_monitor_awareness`] sets the process default first thing at start-up. If the
//!   process already has an awareness (a manifest, or an earlier call) Windows refuses and the
//!   existing one stays; either way no window created afterwards is virtualised.
//! - Each hidden-window thread also puts itself in per-monitor-v2 mode ([`aware_thread`]), so a
//!   thread keeps correct metrics even if the process call was refused for an unaware default.
//!
//! **Unverified on Windows** (`tools/windows-test/tests/m3-app.json`).

use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, GetSystemMetricsForDpi,
    SetProcessDpiAwarenessContext, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{SM_CXSMICON, SM_CYSMICON};

/// Makes the process per-monitor-v2 DPI aware. Call once, before any window exists.
///
/// Returns `false` if Windows refused, which means the process already has an awareness that
/// cannot be changed (a manifest, or an earlier call): nothing to fix then.
pub fn enable_per_monitor_awareness() -> bool {
    // SAFETY: takes a documented awareness-context constant and no pointers; the call fails
    // (rather than misbehaves) once an awareness is already set for the process.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
}

/// Puts the calling thread in per-monitor-v2 mode for the windows it creates from now on.
/// Returns `false` if Windows refused (the thread then keeps the process default).
pub(super) fn aware_thread() -> bool {
    // SAFETY: takes a documented awareness-context constant and no pointers; it changes only
    // the calling thread's context and returns the previous one, or null on failure.
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    !previous.0.is_null()
}

/// The size of a small icon (the notification-area size) at the system DPI, in pixels, as
/// (width, height). Unlike `GetSystemMetrics` this does not depend on the caller's awareness.
pub(super) fn small_icon_size() -> (i32, i32) {
    // SAFETY: plain queries with no preconditions; `GetDpiForSystem` gives 96 on failure.
    unsafe {
        let dpi = GetDpiForSystem();
        (
            GetSystemMetricsForDpi(SM_CXSMICON, dpi),
            GetSystemMetricsForDpi(SM_CYSMICON, dpi),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_small_icon_is_at_least_sixteen_pixels() {
        let (cx, cy) = small_icon_size();
        assert!(cx >= 16 && cy >= 16, "{cx}x{cy}");
    }

    #[test]
    fn a_thread_can_be_made_aware_and_the_process_call_does_not_panic() {
        // The process default may already be set by the test host: only the thread call is
        // expected to succeed. Neither may crash.
        let _ = enable_per_monitor_awareness();
        std::thread::spawn(|| assert!(aware_thread()))
            .join()
            .unwrap();
    }
}
