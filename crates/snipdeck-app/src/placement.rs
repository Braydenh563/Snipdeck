//! Where the Picker and the fill form open (SPEC 5.2): on the side of the Target's text caret the
//! user chose (below by default, one line clear of it), flipped to the opposite side when it would
//! leave the work area, at the mouse when the Target has no caret, and always inside the
//! monitor's work area. [`place`] is plain arithmetic on
//! rectangles in physical pixels, so the rules run under test; [`place_popup`] adds the
//! machine ([`PopupHost`]) and the logical-to-physical scale.

use std::cell::{Cell, RefCell};

use snipdeck_platform::route::ScreenRect;

use crate::frame::{Monitor, Rect};
use crate::settings::PickerSide;

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

/// The frame of a `size` window (physical pixels) opened at `anchor`, inside `area`, on the
/// `side` of the anchor the user chose, `gap` pixels away.
///
/// Below or Above (and always for a mouse anchor, which has no line to keep clear): the window
/// sits at least one line of the caret (its height, never less than `gap`) away, so it covers
/// neither the caret's line nor the line after (below) or before (above) it. It goes on the
/// chosen side if it fits there, else on the other one; if it fits neither way it is pushed up
/// against the edge of the area.
///
/// Right or Left: the window starts at the caret's right edge, or ends at its left edge, with
/// its top at the caret's top. It flips to the other side when the chosen one would leave the
/// area, and when neither fits it takes the Below placement. A window larger than the area is
/// cut to it.
#[must_use]
pub fn place(size: (u32, u32), anchor: Anchor, area: Rect, gap: u32, pick: PickerSide) -> Rect {
    let w = size.0.min(area.w);
    let h = size.1.min(area.h);
    let gap = i64::from(gap);
    let (area_left, area_top) = (i64::from(area.x), i64::from(area.y));
    let (area_right, area_bottom) = (area_left + i64::from(area.w), area_top + i64::from(area.h));
    let (wide, high) = (i64::from(w), i64::from(h));
    let (left, right, top, bottom) = match anchor {
        Anchor::Caret(r) => (
            i64::from(r.x),
            i64::from(r.x) + i64::from(r.w),
            i64::from(r.y),
            i64::from(r.y) + i64::from(r.h),
        ),
        Anchor::Point(x, y) => (i64::from(x), i64::from(x), i64::from(y), i64::from(y)),
    };
    if matches!(anchor, Anchor::Caret(_)) && matches!(pick, PickerSide::Right | PickerSide::Left) {
        let after = right + gap;
        let before = left - gap - wide;
        let fits_after = after + wide <= area_right;
        let fits_before = before >= area_left;
        let (first, second) = if pick == PickerSide::Right {
            ((after, fits_after), (before, fits_before))
        } else {
            ((before, fits_before), (after, fits_after))
        };
        let x = [first, second]
            .into_iter()
            .find_map(|(x, fits)| fits.then_some(x));
        if let Some(x) = x {
            return Rect {
                x: inside(x, w, area_left, area.w),
                y: inside(top, h, area_top, area.h),
                w,
                h,
            };
        }
    }
    // One line of the caret between it and the window, so neither the typed line nor its
    // neighbour is covered.
    let clear = gap.max(bottom - top);
    let below = bottom + clear;
    let above = top - clear - high;
    let fits_below = below + high <= area_bottom;
    let fits_above = above >= area_top;
    let y = match (pick, fits_below, fits_above) {
        (PickerSide::Above, _, true) | (_, false, true) => above,
        _ => below,
    };
    Rect {
        x: inside(left, w, area_left, area.w),
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
    pick: PickerSide,
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
    let frame = place_at(host, anchor, logical, pick)?;
    Some((frame, anchor))
}

/// [`place_popup`] for an anchor already known.
#[must_use]
pub fn place_at(
    host: &dyn PopupHost,
    anchor: Anchor,
    logical: (f32, f32),
    pick: PickerSide,
) -> Option<Rect> {
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
        pick,
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
            taskbar: None,
            scale,
        }
    }

    #[test]
    fn it_opens_below_the_caret() {
        assert_eq!(
            place(SIZE, caret(400, 200), AREA, 4, PickerSide::Below),
            rect(400, 240, 560, 300)
        );
    }

    #[test]
    fn it_flips_above_the_caret_when_below_would_leave_the_work_area() {
        // The caret is 20 px high at y 900: below would end at 1240 > 1040. Above keeps a line
        // (20 px) clear of the caret too: 900 - 20 - 300.
        assert_eq!(
            place(SIZE, caret(400, 900), AREA, 4, PickerSide::Below),
            rect(400, 580, 560, 300)
        );
    }

    #[test]
    fn it_stays_below_when_it_just_fits() {
        // 700 + 20 (the caret) + 20 (one line clear) = 740 .. 1040 exactly.
        assert_eq!(
            place(SIZE, caret(10, 700), AREA, 4, PickerSide::Below).y,
            740
        );
        // One pixel more and it flips.
        assert_eq!(
            place(SIZE, caret(10, 701), AREA, 4, PickerSide::Below).y,
            701 - 20 - 300
        );
    }

    #[test]
    fn with_no_caret_it_opens_at_the_mouse() {
        assert_eq!(
            place(SIZE, Anchor::Point(700, 500), AREA, 4, PickerSide::Below),
            rect(700, 504, 560, 300)
        );
        assert_eq!(
            place(SIZE, Anchor::Point(700, 900), AREA, 4, PickerSide::Below),
            rect(700, 596, 560, 300),
            "the mouse flips too"
        );
    }

    #[test]
    fn it_is_clamped_into_the_work_area() {
        // Too far right: shifted left so the right edge is the area's.
        assert_eq!(
            place(SIZE, caret(1900, 100), AREA, 4, PickerSide::Below).x,
            1360
        );
        // Off the left edge of the area.
        assert_eq!(
            place(SIZE, caret(-50, 100), AREA, 4, PickerSide::Below).x,
            0
        );
        // Caret above the area: pushed down to the top.
        assert_eq!(
            place(SIZE, Anchor::Point(100, -200), AREA, 4, PickerSide::Below).y,
            0
        );
    }

    #[test]
    fn when_it_fits_neither_side_it_is_pushed_against_the_edge() {
        let area = rect(0, 0, 1920, 400);
        // 300 high in a 400 high area, the caret in the middle: neither 224+300 nor above fits.
        let frame = place(SIZE, caret(10, 190), area, 4, PickerSide::Below);
        assert_eq!(frame.y, 100, "pushed up against the bottom edge");
    }

    #[test]
    fn a_window_larger_than_the_area_is_cut_to_it() {
        let area = rect(100, 50, 400, 200);
        assert_eq!(
            place((900, 900), caret(0, 0), area, 4, PickerSide::Below),
            area
        );
    }

    #[test]
    fn a_second_monitor_with_a_negative_origin_is_respected() {
        let left = rect(-1920, 0, 1920, 1040);
        assert_eq!(
            place(SIZE, caret(-100, 200), left, 4, PickerSide::Below),
            rect(-560, 240, 560, 300)
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
        // 560 x 300 logical is 840 x 450 physical; the 4 px gap is 6, so the 30 px caret line
        // (one line clear) decides.
        let (frame, anchor) =
            place_popup(&host, Some(c), (560.0, 300.0), PickerSide::Below).expect("placed");
        assert_eq!(frame, rect(400, 260, 840, 450));
        assert_eq!(anchor, Anchor::Caret(rect(400, 200, 3, 30)));
        // Flipping uses the physical height: 450 + 30 above y 700.
        let low = ScreenRect { y: 700, ..c };
        let (frame, _) =
            place_popup(&host, Some(low), (560.0, 300.0), PickerSide::Below).expect("placed");
        assert_eq!(frame.y, 700 - 30 - 450);
    }

    #[test]
    fn with_no_caret_and_no_mouse_nothing_is_placed() {
        let host = FakePopups::one(monitor(1.0));
        assert_eq!(
            place_popup(&host, None, (560.0, 300.0), PickerSide::Below),
            None
        );
        host.cursor.set(Some((50, 60)));
        let (frame, anchor) =
            place_popup(&host, None, (560.0, 300.0), PickerSide::Below).expect("placed");
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
                (1.0, 1.0),
                PickerSide::Below
            ),
            None
        );
    }

    #[test]
    fn a_nonsense_scale_counts_as_one() {
        for scale in [0.0, -1.0, f32::NAN] {
            let host = FakePopups::one(monitor(scale));
            host.cursor.set(Some((0, 0)));
            let (frame, _) =
                place_popup(&host, None, (100.0, 50.0), PickerSide::Below).expect("placed");
            assert_eq!((frame.w, frame.h), (100, 50));
        }
    }

    // ---- The chosen side ---------------------------------------------------------------------

    fn at_side(anchor: Anchor, area: Rect, pick: PickerSide) -> Rect {
        place(SIZE, anchor, area, 4, pick)
    }

    #[test]
    fn below_and_above_keep_one_line_clear_of_the_caret() {
        // The caret line is y 300..320. The next line (320..340) and the one before (280..300)
        // stay uncovered.
        let below = at_side(caret(400, 300), AREA, PickerSide::Below);
        assert_eq!(below.y, 340, "starts after the next line");
        let above = at_side(caret(400, 600), AREA, PickerSide::Above);
        assert_eq!(above.y + 300, 600 - 20, "ends before the previous line");
        // A caret too small for the gap still gets the gap.
        let thin = Anchor::Caret(rect(400, 300, 2, 2));
        assert_eq!(at_side(thin, AREA, PickerSide::Below).y, 302 + 4);
    }

    #[test]
    fn above_opens_above_and_flips_below_at_the_top_edge() {
        assert_eq!(
            at_side(caret(400, 600), AREA, PickerSide::Above),
            rect(400, 280, 560, 300)
        );
        // 100 - 20 - 300 < 0: flips below, one line clear.
        assert_eq!(
            at_side(caret(400, 100), AREA, PickerSide::Above),
            rect(400, 140, 560, 300)
        );
    }

    #[test]
    fn right_starts_after_the_caret_and_lines_up_with_its_top() {
        assert_eq!(
            at_side(caret(400, 200), AREA, PickerSide::Right),
            rect(406, 200, 560, 300),
            "400 + 2 (caret width) + 4"
        );
        // Low on the screen it stays beside the caret and is pushed up into the area.
        assert_eq!(
            at_side(caret(400, 900), AREA, PickerSide::Right),
            rect(406, 740, 560, 300)
        );
    }

    #[test]
    fn left_ends_before_the_caret() {
        assert_eq!(
            at_side(caret(800, 200), AREA, PickerSide::Left),
            rect(236, 200, 560, 300),
            "800 - 4 - 560"
        );
    }

    #[test]
    fn right_flips_left_at_the_right_edge_and_left_flips_right_at_the_left_edge() {
        // 1500 + 2 + 4 + 560 = 2066 > 1920.
        assert_eq!(
            at_side(caret(1500, 200), AREA, PickerSide::Right),
            rect(936, 200, 560, 300),
            "1500 - 4 - 560"
        );
        // 400 - 4 - 560 < 0.
        assert_eq!(
            at_side(caret(400, 200), AREA, PickerSide::Left),
            rect(406, 200, 560, 300)
        );
    }

    #[test]
    fn when_neither_horizontal_side_fits_it_goes_below_inside_the_area() {
        let narrow = rect(0, 0, 700, 1040);
        assert_eq!(
            at_side(caret(300, 200), narrow, PickerSide::Right),
            rect(140, 240, 560, 300),
            "below the caret, pushed left into the area"
        );
        assert_eq!(
            at_side(caret(300, 200), narrow, PickerSide::Left),
            rect(140, 240, 560, 300)
        );
    }

    #[test]
    fn the_sides_work_on_a_monitor_left_of_the_primary_one() {
        let left = rect(-1920, 0, 1920, 1040);
        // Right of the caret would end at 466 > 0 (the area's right edge): flips left.
        assert_eq!(
            at_side(caret(-100, 200), left, PickerSide::Right),
            rect(-664, 200, 560, 300)
        );
        assert_eq!(
            at_side(caret(-1000, 200), left, PickerSide::Right),
            rect(-994, 200, 560, 300)
        );
        // Left of a caret near the monitor's left edge flips right.
        assert_eq!(
            at_side(caret(-1800, 200), left, PickerSide::Left),
            rect(-1794, 200, 560, 300)
        );
    }

    #[test]
    fn a_mouse_anchor_keeps_below_and_above_whatever_the_side() {
        for pick in [PickerSide::Right, PickerSide::Left, PickerSide::Below] {
            assert_eq!(
                at_side(Anchor::Point(700, 500), AREA, pick),
                rect(700, 504, 560, 300),
                "{pick:?}"
            );
        }
        assert_eq!(
            at_side(Anchor::Point(700, 500), AREA, PickerSide::Above),
            rect(700, 196, 560, 300),
            "500 - 4 - 300"
        );
    }

    #[test]
    fn the_sides_scale_with_the_monitor_at_150_percent() {
        let host = FakePopups::one(monitor(1.5));
        let c = ScreenRect {
            x: 400,
            y: 200,
            w: 3,
            h: 30,
        };
        // 840 x 450 physical, gap 6.
        let right = place_popup(&host, Some(c), (560.0, 300.0), PickerSide::Right).expect("placed");
        assert_eq!(right.0, rect(409, 200, 840, 450), "400 + 3 + 6");
        let left_of = ScreenRect { x: 1000, ..c };
        let left =
            place_popup(&host, Some(left_of), (560.0, 300.0), PickerSide::Left).expect("placed");
        assert_eq!(left.0, rect(154, 200, 840, 450), "1000 - 6 - 840");
        let above = ScreenRect { y: 700, ..c };
        let above =
            place_popup(&host, Some(above), (560.0, 300.0), PickerSide::Above).expect("placed");
        assert_eq!(above.0.y, 700 - 30 - 450);
    }

    #[test]
    fn no_side_covers_the_caret_when_there_is_room() {
        for pick in [
            PickerSide::Below,
            PickerSide::Above,
            PickerSide::Right,
            PickerSide::Left,
        ] {
            for (x, y) in [(10, 10), (900, 500), (1800, 300), (700, 1000), (1900, 1000)] {
                let c = rect(x, y, 2, 20);
                let f = place(SIZE, Anchor::Caret(c), AREA, 4, pick);
                let overlaps = |a: Rect, b: Rect| {
                    i64::from(a.x) < i64::from(b.x) + i64::from(b.w)
                        && i64::from(b.x) < i64::from(a.x) + i64::from(a.w)
                        && i64::from(a.y) < i64::from(b.y) + i64::from(b.h)
                        && i64::from(b.y) < i64::from(a.y) + i64::from(a.h)
                };
                assert!(
                    !overlaps(f, c),
                    "{pick:?} at ({x}, {y}) covers the caret: {f:?}"
                );
                assert!(
                    f.x >= 0
                        && f.y >= 0
                        && i64::from(f.x) + i64::from(f.w) <= 1920
                        && i64::from(f.y) + i64::from(f.h) <= 1040,
                    "{pick:?} at ({x}, {y}) leaves the area: {f:?}"
                );
            }
        }
    }
}
