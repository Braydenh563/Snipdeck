//! What kind of window the window-attributes hook is looking at, and what that means.
//!
//! The host creates every window un-activated and out of the taskbar for the Panel's sake
//! (ADR-0003). The Main window, Settings and the editors are ordinary windows instead: in the
//! taskbar, activated when shown, with the app icon. Slint runs the hook inside `X::new()`, so
//! the decision has to be made *before* the window is created; [`crate::main_binding::RegularWindow::create`]
//! does that. This module holds the decision itself, which has no windowing in it and so runs
//! under test on every platform.

/// Whether any window asks for per-pixel alpha (`with_transparent`). Never: Slint asks winit for
/// it by default, which on Windows turns on DWM blur-behind, and every pixel Windows itself
/// paints (the class-brush erase on a resize or a show, an area not yet drawn) then has alpha 0
/// and shows the desktop through the window: the render flicker of 2026-11-14. Every Snipdeck
/// window has an opaque background, so nothing needs it.
pub const TRANSPARENT: bool = false;

/// The window attributes the hook sets for one kind of window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Leave the window out of the taskbar and Alt+Tab (Windows).
    pub skip_taskbar: bool,
    /// Activate the window when it is shown.
    pub active: bool,
    /// Give the window the application icon (title bar and taskbar).
    pub window_icon: bool,
}

/// The attributes for a regular window (`true`: Main, Settings, an editor) or a floating one
/// (`false`: the Panel, the picker, a toast).
#[must_use]
pub const fn plan(regular: bool) -> Plan {
    Plan {
        skip_taskbar: !regular,
        active: regular,
        window_icon: regular,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_regular_window_is_in_the_taskbar_and_active() {
        let p = plan(true);
        assert!(!p.skip_taskbar);
        assert!(p.active);
    }

    #[test]
    fn a_floating_window_is_skipped_and_not_activated() {
        let p = plan(false);
        assert!(p.skip_taskbar);
        assert!(!p.active);
    }

    #[test]
    fn no_window_is_composed_see_through() {
        // An erase or an undrawn area must show the theme colour, never the desktop.
        const { assert!(!TRANSPARENT) };
    }

    #[test]
    fn only_regular_windows_get_the_icon() {
        assert!(plan(true).window_icon);
        assert!(!plan(false).window_icon);
    }
}
