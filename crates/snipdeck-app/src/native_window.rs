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

/// Gives the Panel a taskbar button and Alt+Tab entry (`on`) or takes them away, live. Windows
/// refreshes the taskbar only when the window is hidden and shown again, which the platform call
/// does for a visible Panel without activating it.
#[cfg(windows)]
pub fn set_panel_taskbar(window: &PanelWindow, on: bool) {
    use slint::{ComponentHandle, winit_030::winit::platform::windows::WindowExtWindows};
    window
        .window()
        .with_winit_window(|w| w.set_skip_taskbar(!on));
    if let Some(hwnd) = handle(window) {
        let changed = snipdeck_platform::windows::window_style::set_panel_taskbar(hwnd, on);
        crate::trace!("panel taskbar button {on} on {hwnd:#x}: {changed}");
    }
}

/// The Panel has no taskbar button off Windows: nothing to change.
#[cfg(not(windows))]
pub fn set_panel_taskbar(_window: &PanelWindow, _on: bool) {}

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

/// After the Panel stepped down from topmost (a regular Snipdeck window took the focus), puts it
/// back just behind that window. winit's step-down raises the Panel above every non-topmost
/// window, so without this it would land on top of Main a moment after Main was activated. Runs
/// a little later, once winit has applied the step-down; does nothing if the Panel is topmost
/// again by then. Windows only, and not verified on Windows yet.
#[cfg(windows)]
pub fn settle_behind_active(window: &PanelWindow) {
    use slint::ComponentHandle;
    let weak = window.as_weak();
    slint::Timer::single_shot(STEP_DOWN_SETTLE, move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if window.get_topmost() {
            return;
        }
        if let Some(hwnd) = handle(&window) {
            let placed = snipdeck_platform::windows::window_style::place_behind_foreground(hwnd);
            crate::trace!("panel behind the active window {hwnd:#x}: {placed}");
        }
    });
}

/// Nothing to reorder off Windows.
#[cfg(not(windows))]
pub fn settle_behind_active(_window: &PanelWindow) {}

/// How long winit gets to apply the topmost step-down before the Panel is put behind the active
/// window (longer than a window message round trip, shorter than anything a user notices).
#[cfg(windows)]
const STEP_DOWN_SETTLE: std::time::Duration = std::time::Duration::from_millis(30);

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
    let work_area = work_area_of(window).unwrap_or(whole);
    Some(Monitor {
        id: Monitor::id_of(&monitor.name().unwrap_or_default(), size.width, size.height),
        work_area,
        taskbar: frame::taskbar_edge(whole, work_area),
        scale,
    })
}

/// The monitor the mouse pointer is on, for sizing a window before it exists. Windows only.
#[cfg(windows)]
fn monitor_near_cursor() -> Option<Monitor> {
    let ((x, y, w, h), (mx, my, mw, mh), dpi) =
        snipdeck_platform::windows::window_style::work_area_near_cursor()?;
    #[allow(clippy::cast_precision_loss, reason = "a DPI value is far below 2^24")]
    let scale = dpi as f32 / 96.0;
    let work_area = Rect { x, y, w, h };
    let bounds = Rect {
        x: mx,
        y: my,
        w: mw,
        h: mh,
    };
    Some(Monitor {
        id: Monitor::id_of("cursor", w, h),
        work_area,
        taskbar: frame::taskbar_edge(bounds, work_area),
        scale,
    })
}

#[cfg(not(windows))]
fn monitor_near_cursor() -> Option<Monitor> {
    None
}

/// `wanted` (logical pixels) cut to the work area of the monitor under the pointer, or as it is
/// where the host cannot tell. The size is traced under `SNIPDECK_LOG=1`.
#[must_use]
pub fn fit_logical(wanted: (f32, f32)) -> (f32, f32) {
    let Some(monitor) = monitor_near_cursor() else {
        return wanted;
    };
    let fitted = frame::open_size(wanted, &monitor);
    crate::trace!(
        "open size {wanted:?} -> {fitted:?} in work area {:?} at scale {}",
        monitor.work_area,
        monitor.scale
    );
    fitted
}

/// Gives a regular window that does not exist yet its size, before its first `show()`: `wanted`
/// cut to the screen (see [`fit_logical`]). Setting a size makes Slint keep it instead of
/// applying the preferred size, which on a small or scaled screen is taller than the screen.
/// [`fit_to_screen`] stays as the fallback once the window exists.
pub fn size_before_show(window: &slint::Window, wanted: (f32, f32)) {
    let (w, h) = fit_logical(wanted);
    window.set_size(slint::LogicalSize::new(w, h));
}

/// Sets a regular window's height to `wanted_h` logical pixels, cut to the work area, keeping its
/// width. For a window whose content grows or shrinks (the item editor's More options).
pub fn set_height_fitted(window: &slint::Window, wanted_h: f32) {
    let scale = window.scale_factor();
    let width = window.size().width as f32 / scale;
    let (_, h) = fit_logical((width, wanted_h));
    window.set_size(slint::LogicalSize::new(width, h));
}

/// Keeps a shown regular window (Main, Settings, an editor) inside its monitor's work area:
/// one that opened taller or wider than the screen, or under the taskbar, is cut to the area
/// and moved onto it. Call it right after `show()`; nothing happens without a native window.
pub fn fit_to_screen(window: &slint::Window) {
    let _ = window.with_winit_window(|w| {
        // A maximised window reaches past the work area by its frame; Windows keeps it there.
        if w.is_maximized() {
            return;
        }
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

// ---- Opening a secondary window near the one it was opened from --------------------------------

/// A window the others open over: the Main window or the Panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// The Main window.
    Main,
    /// The Panel.
    Panel,
}

/// Where a surface is now, if it is shown: its outer frame, and the monitor it is on (none
/// where the host cannot tell).
type SurfaceFn = dyn Fn() -> Option<(Rect, Option<Monitor>)>;

thread_local! {
    static SURFACES: std::cell::RefCell<Vec<(Surface, Box<SurfaceFn>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static OPENER: std::cell::Cell<Option<Surface>> = const { std::cell::Cell::new(None) };
}

/// The frame and monitor of `window` while it is shown (none when hidden or without a native
/// window).
#[must_use]
pub fn shown_frame(window: &slint::Window) -> Option<(Rect, Option<Monitor>)> {
    if !window.is_visible() {
        return None;
    }
    let (at, size) = (window.position(), window.size());
    let monitor = window.with_winit_window(monitor_of).flatten();
    Some((
        Rect {
            x: at.x,
            y: at.y,
            w: size.width,
            h: size.height,
        },
        monitor,
    ))
}

/// Tells [`place_over_owner`] where `surface` is: `frame` reports it while it is shown.
pub fn register_surface(
    surface: Surface,
    frame: impl Fn() -> Option<(Rect, Option<Monitor>)> + 'static,
) {
    SURFACES.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|(s, _)| *s != surface);
        all.push((surface, Box::new(frame)));
    });
}

/// The surface the user just acted on: a secondary window opens over it.
pub fn note_opener(surface: Surface) {
    OPENER.with(|o| o.set(Some(surface)));
}

/// The surface [`note_opener`] was last told about.
#[must_use]
pub fn last_opener() -> Option<Surface> {
    OPENER.with(std::cell::Cell::get)
}

/// The shown surface to open over: the one acted on last, else the Main window, else the Panel.
fn owner() -> Option<(Rect, Option<Monitor>)> {
    let first = OPENER.with(std::cell::Cell::get);
    let order = [first, Some(Surface::Main), Some(Surface::Panel)];
    SURFACES.with(|all| {
        let all = all.borrow();
        order.into_iter().flatten().find_map(|wanted| {
            all.iter()
                .find(|(s, _)| *s == wanted)
                .and_then(|(_, f)| f())
        })
    })
}

/// Positions (and, if need be, shrinks) a secondary window that is about to be shown: beside
/// the window it was opened from (the Main window or the Panel; see [`frame::place_beside`]),
/// else centred on the monitor under the pointer, and always inside that monitor's work area. Call it right before
/// `show()`, after the size is set: a window that is already up is left where it is. Without a
/// native window (tests, other platforms) nothing happens and [`fit_to_screen`] stays the
/// fallback after the show.
pub fn place_over_owner(window: &slint::Window) {
    if window.is_visible() {
        return;
    }
    let (owner, monitor) = match owner() {
        Some((rect, Some(monitor))) => (rect, monitor),
        Some((rect, None)) => {
            let Some(monitor) = monitor_near_cursor() else {
                return;
            };
            (rect, monitor)
        }
        None => {
            let Some(monitor) = monitor_near_cursor() else {
                return;
            };
            (monitor.work_area, monitor)
        }
    };
    let logical = window.size().to_logical(window.scale_factor());
    let (w, h) = frame::open_size((logical.width, logical.height), &monitor);
    let physical = |logical: f32| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a window size in pixels is positive and far below u32::MAX"
        )]
        let px = (logical * monitor.scale).round() as u32;
        px
    };
    let inner = (physical(w).max(1), physical(h).max(1));
    // The title bar and frame are outside the size the window is given.
    let outer = (
        inner.0,
        inner.1.saturating_add(physical(frame::WINDOW_CHROME)),
    );
    let at = frame::place_beside(
        owner,
        outer,
        monitor.work_area,
        frame::gap_px(monitor.scale),
    );
    crate::trace!(
        "place window {inner:?} over {owner:?} -> {at:?} in {:?}",
        monitor.work_area
    );
    window.set_size(slint::LogicalSize::new(w, h));
    window.set_position(slint::PhysicalPosition::new(at.x, at.y));
}

// ---- Which Snipdeck window has the focus -----------------------------------------------------------

/// The regular windows that have the focus. Focus passes from one window to the next as a loss
/// then a gain (or the other way), so the change is reported only once it has settled
/// ([`ActiveWindows::take_change`]).
#[derive(Debug)]
pub struct ActiveWindows<Id> {
    focused: Vec<Id>,
    reported: bool,
}

impl<Id: PartialEq> ActiveWindows<Id> {
    /// No window active.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused: Vec::new(),
            reported: false,
        }
    }

    /// `id` gained (`true`) or lost the focus.
    pub fn set(&mut self, id: Id, focused: bool) {
        self.focused.retain(|f| *f != id);
        if focused {
            self.focused.push(id);
        }
    }

    /// The new answer to "does a Snipdeck window have the focus", when it differs from the one
    /// last handed out.
    pub fn take_change(&mut self) -> Option<bool> {
        let now = !self.focused.is_empty();
        (now != self.reported).then(|| {
            self.reported = now;
            now
        })
    }
}

impl<Id: PartialEq> Default for ActiveWindows<Id> {
    fn default() -> Self {
        Self::new()
    }
}

/// How long a focus change waits for the next window's gain before it is reported.
const FOCUS_SETTLE: std::time::Duration = std::time::Duration::from_millis(60);

type ActiveFn = dyn Fn(bool);

thread_local! {
    static FOCUSED: std::cell::RefCell<ActiveWindows<winit::window::WindowId>> =
        const { std::cell::RefCell::new(ActiveWindows::new()) };
    static ON_ACTIVE: std::cell::RefCell<Option<std::rc::Rc<ActiveFn>>> =
        const { std::cell::RefCell::new(None) };
}

/// Who hears when a regular Snipdeck window gains the focus (`true`) or the focus leaves them
/// all (`false`): the controller, which then drops the Panel from topmost or restores it.
pub fn on_own_window_active(report: impl Fn(bool) + 'static) {
    ON_ACTIVE.with(|f| *f.borrow_mut() = Some(std::rc::Rc::new(report)));
}

/// Feeds a winit window event of a regular window to the focus tracking. Every regular window
/// does this; the Main window, which handles its own events, calls it from there.
pub fn note_window_event(window: &slint::Window, event: &winit::event::WindowEvent) {
    if matches!(event, winit::event::WindowEvent::Resized(_)) {
        paint_before_first_frame(window);
    }
    let winit::event::WindowEvent::Focused(focused) = event else {
        return;
    };
    let Some(id) = window.with_winit_window(slint::winit_030::winit::window::Window::id) else {
        return;
    };
    FOCUSED.with(|set| set.borrow_mut().set(id, *focused));
    slint::Timer::single_shot(FOCUS_SETTLE, || {
        let change = FOCUSED.with(|set| set.borrow_mut().take_change());
        let report = ON_ACTIVE.with(|f| f.borrow().clone());
        if let (Some(active), Some(report)) = (change, report) {
            report(active);
        }
    });
}

/// Starts following the focus of a regular window.
pub fn track_focus(window: &slint::Window) {
    use slint::winit_030::EventResult;
    window.on_winit_window_event(|slint_window, event| {
        note_window_event(slint_window, event);
        EventResult::Propagate
    });
}

/// Puts a Main window that was just shown where the user left it: at `place`, cut and moved
/// inside the work area of the monitor it lands on (a monitor that is gone, or a smaller
/// screen, still gives a window on screen), and maximised again if it was.
pub fn restore_place(window: &slint::Window, place: &crate::settings::WindowPlace) {
    let _ = window.with_winit_window(|w| {
        w.set_outer_position(winit::dpi::PhysicalPosition::new(place.x, place.y));
        let Some(monitor) = monitor_of(w) else {
            return;
        };
        let (outer, inner) = (w.outer_size(), w.inner_size());
        let chrome = (
            outer.width.saturating_sub(inner.width),
            outer.height.saturating_sub(inner.height),
        );
        let fitted = frame::fit(
            Rect {
                x: place.x,
                y: place.y,
                w: place.w,
                h: place.h,
            },
            monitor.work_area,
        );
        let _ = w.request_inner_size(winit::dpi::PhysicalSize::new(
            fitted.w.saturating_sub(chrome.0).max(1),
            fitted.h.saturating_sub(chrome.1).max(1),
        ));
        w.set_outer_position(winit::dpi::PhysicalPosition::new(fitted.x, fitted.y));
        if place.maximized {
            w.set_maximized(true);
        }
    });
}

/// Where a shown Main window is now, for [`Controller::remember_main_window`]: `previous` (the
/// restored place) is kept while it is maximised, and `None` when it is minimised or has no
/// size yet.
///
/// [`Controller::remember_main_window`]: crate::controller::Controller::remember_main_window
#[must_use]
pub fn place_of(
    window: &winit::window::Window,
    previous: Option<crate::settings::WindowPlace>,
) -> Option<crate::settings::WindowPlace> {
    if window.is_minimized() == Some(true) {
        return None;
    }
    if window.is_maximized() {
        return previous.map(|p| crate::settings::WindowPlace {
            maximized: true,
            ..p
        });
    }
    let (at, size) = (window.outer_position().ok()?, window.outer_size());
    (size.width > 0 && size.height > 0).then_some(crate::settings::WindowPlace {
        x: at.x,
        y: at.y,
        w: size.width,
        h: size.height,
        maximized: false,
    })
}

/// Brings `window` in front without activating it: nothing off Windows.
#[cfg(not(windows))]
pub fn raise_window(_window: &slint::Window) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_gaining_the_focus_is_reported_once() {
        let mut tracker = ActiveWindows::new();
        assert_eq!(tracker.take_change(), None);
        tracker.set(1, true);
        assert_eq!(tracker.take_change(), Some(true));
        assert_eq!(tracker.take_change(), None);
        tracker.set(1, false);
        assert_eq!(tracker.take_change(), Some(false));
    }

    #[test]
    fn focus_passing_between_two_windows_is_no_change() {
        let mut tracker = ActiveWindows::new();
        tracker.set(1, true);
        assert_eq!(tracker.take_change(), Some(true));
        // Lost, then gained, before anyone asked: the loss never shows.
        tracker.set(1, false);
        tracker.set(2, true);
        assert_eq!(tracker.take_change(), None);
        tracker.set(2, false);
        assert_eq!(tracker.take_change(), Some(false));
    }
}

// ---- The colour before the first frame (O52) ---------------------------------------------------

thread_local! {
    /// The theme's window colour, kept for windows whose native window does not exist yet.
    static ERASE: std::cell::Cell<Option<(u8, u8, u8)>> = const { std::cell::Cell::new(None) };
}

/// The theme is `rgb` now (the window colour, `Theme.bg-app`): Windows paints every Snipdeck
/// window with it between showing it and its first frame, instead of white. Given to the window
/// class through `window` when its native window exists, else by the next window that reports
/// one ([`paint_before_first_frame`]).
pub fn set_erase_colour(window: &slint::Window, rgb: (u8, u8, u8)) {
    ERASE.with(|c| c.set(Some(rgb)));
    paint_before_first_frame(window);
}

/// The colour [`set_erase_colour`] keeps, if any.
#[must_use]
pub fn erase_colour() -> Option<(u8, u8, u8)> {
    ERASE.with(std::cell::Cell::get)
}

/// Gives the kept theme colour to the window class of `window`, once its native window exists.
/// All Snipdeck windows share one class, so this covers the windows made after it too.
pub fn paint_before_first_frame(window: &slint::Window) {
    #[cfg(windows)]
    if let (Some(rgb), Some(hwnd)) = (erase_colour(), handle_of(window)) {
        snipdeck_platform::windows::erase::set_class_background(hwnd, rgb);
    }
    #[cfg(not(windows))]
    let _ = window;
}
