//! Where the Picker and the fill form open (SPEC 5.2): just below the Target's text caret,
//! flipped above it when it would leave the work area, at the mouse when the Target has no
//! caret, and always inside the monitor's work area. [`place`] is plain arithmetic on
//! rectangles in physical pixels, so the rules run under test; [`place_popup`] adds the
//! machine ([`PopupHost`]) and the logical-to-physical scale.

use std::cell::{Cell, RefCell};

use snipdeck_platform::route::ScreenRect;

use crate::frame::{Monitor, Rect};

/// Logical pixels between the caret (or the mouse) and the window.
pub const GAP: f32 = 4.0;

/// What a popup opens next to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The Target's text caret.
    Caret(Rect),
    /// The mouse position, for a Target with no caret.
    Point(i32, i32),
}

impl Anchor {
    /// The point the monitor is looked up by.
    #[must_use]
    pub fn point(self) -> (i32, i32) {
        match self {
            Self::Caret(r) => (r.x, r.y),
            Self::Point(x, y) => (x, y),
        }
    }
}

/// The frame of a `size` window (physical pixels) opened at `anchor`, `gap` pixels away, inside
/// `area`. Below the anchor if it fits there, else above it; if it fits neither way the window
/// is pushed up against the edge of the area. A window larger than the area is cut to it.
#[must_use]
pub fn place(size: (u32, u32), anchor: Anchor, area: Rect, gap: u32) -> Rect {
    let w = size.0.min(area.w);
    let h = size.1.min(area.h);
    let (left, top, bottom) = match anchor {
        Anchor::Caret(r) => (
            i64::from(r.x),
            i64::from(r.y),
            i64::from(r.y) + i64::from(r.h),
        ),
        Anchor::Point(x, y) => (i64::from(x), i64::from(y), i64::from(y)),
    };
    let gap = i64::from(gap);
    let area_top = i64::from(area.y);
    let area_bottom = area_top + i64::from(area.h);
    let below = bottom + gap;
    let above = top - gap - i64::from(h);
    let y = if below + i64::from(h) <= area_bottom {
        below
    } else if above >= area_top {
        above
    } else {
        below
    };
    Rect {
        x: inside(left, w, i64::from(area.x), area.w),
        y: inside(y, h, area_top, area.h),
        w,
        h,
    }
}

/// `start` moved so `[start, start + len)` lies in `[low, low + span)`.
fn inside(start: i64, len: u32, low: i64, span: u32) -> i32 {
    let high = low + i64::from(span) - i64::from(len);
    let moved = start.clamp(low, high.max(low));
    i32::try_from(moved).unwrap_or(i32::try_from(low).unwrap_or(0))
}

/// What the popups ask of the machine. The Windows implementation is in `platform`; off
/// Windows the dev build has [`NoPopups`] and the tests have [`FakePopups`].
pub trait PopupHost {
    /// A popup's native window now exists: it is never the Target, and it is a non-activating
    /// tool window like the Panel.
    fn attach(&self, window: &slint::Window);
    /// Takes the keyboard for the popup (the Picker is typed into at once). `false` if there
    /// is no window or Windows refused; the user can still click into it.
    fn activate(&self, window: &slint::Window) -> bool;
    /// Gives the keyboard back to the Target, if the Panel or a popup still holds it (the
    /// user has not moved to another app). The Panel calls it as it hides.
    fn release(&self);
    /// The mouse position, physical pixels.
    fn cursor(&self) -> Option<(i32, i32)>;
    /// The monitor under a point (the nearest one outside them all).
    fn monitor_at(&self, x: i32, y: i32) -> Option<Monitor>;
}

/// Where a popup of `logical` size (width, height) opens: next to `caret` if the Target has
/// one, else at the mouse. `None` when neither the mouse nor a monitor can be read; the window
/// is then left where the system puts it.
#[must_use]
pub fn place_popup(
    host: &dyn PopupHost,
    caret: Option<ScreenRect>,
    logical: (f32, f32),
) -> Option<(Rect, Anchor)> {
    let anchor = if let Some(c) = caret {
        Anchor::Caret(Rect {
            x: c.x,
            y: c.y,
            w: c.w,
            h: c.h,
        })
    } else {
        let (x, y) = host.cursor()?;
        Anchor::Point(x, y)
    };
    let frame = place_at(host, anchor, logical)?;
    Some((frame, anchor))
}

/// [`place_popup`] for an anchor already known.
#[must_use]
pub fn place_at(host: &dyn PopupHost, anchor: Anchor, logical: (f32, f32)) -> Option<Rect> {
    let (x, y) = anchor.point();
    let monitor = host.monitor_at(x, y)?;
    let scale = if monitor.scale.is_finite() && monitor.scale > 0.0 {
        monitor.scale
    } else {
        1.0
    };
    Some(place(
        (physical(logical.0, scale), physical(logical.1, scale)),
        anchor,
        monitor.work_area,
        physical(GAP, scale),
    ))
}

/// A logical length in physical pixels, rounded up so the content is never cut.
fn physical(logical: f32, scale: f32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a window length is far below u32::MAX, and the value is clamped to zero from below"
    )]
    let pixels = (f64::from(logical.max(0.0)) * f64::from(scale)).ceil() as u32;
    pixels
}

/// Logical pixels between the toast window and the bottom of the work area.
pub const TOAST_MARGIN: f32 = 24.0;

/// The frame of a toast window of `logical` size: centred at the bottom of the work area of
/// the monitor the mouse is on (the first one when the mouse cannot be read).
#[must_use]
pub fn place_toast(host: &dyn PopupHost, logical: (f32, f32)) -> Option<Rect> {
    let (x, y) = host.cursor().unwrap_or((0, 0));
    let monitor = host.monitor_at(x, y)?;
    let scale = if monitor.scale.is_finite() && monitor.scale > 0.0 {
        monitor.scale
    } else {
        1.0
    };
    let area = monitor.work_area;
    let w = physical(logical.0, scale).min(area.w);
    let h = physical(logical.1, scale).min(area.h);
    let margin = i64::from(physical(TOAST_MARGIN, scale));
    let left = i64::from(area.x) + (i64::from(area.w) - i64::from(w)) / 2;
    let top = i64::from(area.y) + i64::from(area.h) - i64::from(h) - margin;
    Some(Rect {
        x: inside(left, w, i64::from(area.x), area.w),
        y: inside(top, h, i64::from(area.y), area.h),
        w,
        h,
    })
}

/// The dev build off Windows: no native window to style, no mouse, no monitors. The popups
/// open wherever the system puts them.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPopups;

impl PopupHost for NoPopups {
    fn attach(&self, _window: &slint::Window) {}
    fn activate(&self, _window: &slint::Window) -> bool {
        false
    }
    fn release(&self) {}
    fn cursor(&self) -> Option<(i32, i32)> {
        None
    }
    fn monitor_at(&self, _x: i32, _y: i32) -> Option<Monitor> {
        None
    }
}

/// A scripted machine for tests: it records what it was asked and answers from what it was
/// given.
#[derive(Debug, Default)]
pub struct FakePopups {
    /// The mouse.
    pub cursor: Cell<Option<(i32, i32)>>,
    /// The monitors; a point outside them all gets the first.
    pub monitors: RefCell<Vec<Monitor>>,
    /// What the popups asked of the machine, in order: `"attach"`, `"activate"` and
    /// `"release"`.
    pub calls: RefCell<Vec<&'static str>>,
}

impl FakePopups {
    /// A machine with one monitor.
    #[must_use]
    pub fn one(monitor: Monitor) -> Self {
        Self {
            monitors: RefCell::new(vec![monitor]),
            ..Self::default()
        }
    }
}

impl PopupHost for FakePopups {
    fn attach(&self, _window: &slint::Window) {
        self.calls.borrow_mut().push("attach");
    }

    fn activate(&self, _window: &slint::Window) -> bool {
        self.calls.borrow_mut().push("activate");
        true
    }

    fn release(&self) {
        self.calls.borrow_mut().push("release");
    }

    fn cursor(&self) -> Option<(i32, i32)> {
        self.cursor.get()
    }

    fn monitor_at(&self, x: i32, y: i32) -> Option<Monitor> {
        let monitors = self.monitors.borrow();
        let holds = |m: &&Monitor| {
            let a = m.work_area;
            i64::from(x) >= i64::from(a.x)
                && i64::from(x) < i64::from(a.x) + i64::from(a.w)
                && i64::from(y) >= i64::from(a.y)
                && i64::from(y) < i64::from(a.y) + i64::from(a.h)
        };
        monitors.iter().find(holds).or(monitors.first()).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    /// 1920x1080 with a 40 px taskbar at the bottom.
    const AREA: Rect = rect(0, 0, 1920, 1040);
    const SIZE: (u32, u32) = (560, 300);

    fn caret(x: i32, y: i32) -> Anchor {
        Anchor::Caret(rect(x, y, 2, 20))
    }

    fn monitor(scale: f32) -> Monitor {
        Monitor {
            id: Monitor::id_of("DISPLAY1", 1920, 1080),
            work_area: AREA,
            scale,
        }
    }

    #[test]
    fn it_opens_below_the_caret() {
        assert_eq!(
            place(SIZE, caret(400, 200), AREA, 4),
            rect(400, 224, 560, 300)
        );
    }

    #[test]
    fn it_flips_above_the_caret_when_below_would_leave_the_work_area() {
        // The caret is 20 px high at y 900: below would end at 1224 > 1040.
        assert_eq!(
            place(SIZE, caret(400, 900), AREA, 4),
            rect(400, 596, 560, 300)
        );
    }

    #[test]
    fn it_stays_below_when_it_just_fits() {
        // 716 + 4 + 20 = 740 .. 1040 exactly.
        assert_eq!(place(SIZE, caret(10, 716), AREA, 4).y, 740);
        // One pixel more and it flips.
        assert_eq!(place(SIZE, caret(10, 717), AREA, 4).y, 717 - 4 - 300);
    }

    #[test]
    fn with_no_caret_it_opens_at_the_mouse() {
        assert_eq!(
            place(SIZE, Anchor::Point(700, 500), AREA, 4),
            rect(700, 504, 560, 300)
        );
        assert_eq!(
            place(SIZE, Anchor::Point(700, 900), AREA, 4),
            rect(700, 596, 560, 300),
            "the mouse flips too"
        );
    }

    #[test]
    fn it_is_clamped_into_the_work_area() {
        // Too far right: shifted left so the right edge is the area's.
        assert_eq!(place(SIZE, caret(1900, 100), AREA, 4).x, 1360);
        // Off the left edge of the area.
        assert_eq!(place(SIZE, caret(-50, 100), AREA, 4).x, 0);
        // Caret above the area: pushed down to the top.
        assert_eq!(place(SIZE, Anchor::Point(100, -200), AREA, 4).y, 0);
    }

    #[test]
    fn when_it_fits_neither_side_it_is_pushed_against_the_edge() {
        let area = rect(0, 0, 1920, 400);
        // 300 high in a 400 high area, the caret in the middle: neither 224+300 nor above fits.
        let frame = place(SIZE, caret(10, 190), area, 4);
        assert_eq!(frame.y, 100, "pushed up against the bottom edge");
    }

    #[test]
    fn a_window_larger_than_the_area_is_cut_to_it() {
        let area = rect(100, 50, 400, 200);
        assert_eq!(place((900, 900), caret(0, 0), area, 4), area);
    }

    #[test]
    fn a_second_monitor_with_a_negative_origin_is_respected() {
        let left = rect(-1920, 0, 1920, 1040);
        assert_eq!(
            place(SIZE, caret(-100, 200), left, 4),
            rect(-560, 224, 560, 300)
        );
    }

    #[test]
    fn the_logical_size_and_the_gap_scale_with_the_monitor_at_150_percent() {
        let host = FakePopups::one(monitor(1.5));
        let c = ScreenRect {
            x: 400,
            y: 200,
            w: 3,
            h: 30,
        };
        // 560 x 300 logical is 840 x 450 physical; the 4 px gap is 6.
        let (frame, anchor) = place_popup(&host, Some(c), (560.0, 300.0)).expect("placed");
        assert_eq!(frame, rect(400, 236, 840, 450));
        assert_eq!(anchor, Anchor::Caret(rect(400, 200, 3, 30)));
        // Flipping uses the physical height: 450 + 6 above y 700.
        let low = ScreenRect { y: 700, ..c };
        let (frame, _) = place_popup(&host, Some(low), (560.0, 300.0)).expect("placed");
        assert_eq!(frame.y, 700 - 6 - 450);
    }

    #[test]
    fn with_no_caret_and_no_mouse_nothing_is_placed() {
        let host = FakePopups::one(monitor(1.0));
        assert_eq!(place_popup(&host, None, (560.0, 300.0)), None);
        host.cursor.set(Some((50, 60)));
        let (frame, anchor) = place_popup(&host, None, (560.0, 300.0)).expect("placed");
        assert_eq!(anchor, Anchor::Point(50, 60));
        assert_eq!(frame, rect(50, 64, 560, 300));
    }

    #[test]
    fn the_toast_is_centred_at_the_bottom_of_the_work_area() {
        let host = FakePopups::one(monitor(1.0));
        assert_eq!(
            place_toast(&host, (400.0, 40.0)),
            Some(rect(760, 976, 400, 40)),
            "(1920 - 400) / 2 across, 24 above the 1040 line"
        );
        let host = FakePopups::one(monitor(1.5));
        assert_eq!(
            place_toast(&host, (400.0, 40.0)),
            Some(rect(660, 944, 600, 60)),
            "scaled: 600 x 60, a 36 px margin"
        );
        assert_eq!(place_toast(&NoPopups, (1.0, 1.0)), None);
    }

    #[test]
    fn with_no_monitor_nothing_is_placed() {
        assert_eq!(
            place_popup(
                &NoPopups,
                Some(ScreenRect {
                    x: 1,
                    y: 1,
                    w: 1,
                    h: 1
                }),
                (1.0, 1.0)
            ),
            None
        );
    }

    #[test]
    fn a_nonsense_scale_counts_as_one() {
        for scale in [0.0, -1.0, f32::NAN] {
            let host = FakePopups::one(monitor(scale));
            host.cursor.set(Some((0, 0)));
            let (frame, _) = place_popup(&host, None, (100.0, 50.0)).expect("placed");
            assert_eq!((frame.w, frame.h), (100, 50));
        }
    }
}
