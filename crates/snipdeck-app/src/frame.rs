//! Where the Panel sits and how large it may be (DESIGN.md §5 Panel): width 260 to 480
//! logical pixels, height up to the monitor's work area. Pure arithmetic with no window in it,
//! so the rules run under test; the host reads the real frame and monitor (winit, on Windows)
//! and applies what [`clamp`] returns.
//!
//! Everything is in physical pixels in desktop coordinates, as the OS reports it. The limits
//! are logical, so [`Monitor::scale`] turns them into pixels.

use std::ops::Range;

use crate::settings::{DockEdge, PANEL_WIDTH, PanelGeometry};

/// The shortest Panel, in logical pixels (the Slint window's `min-height`).
pub const MIN_HEIGHT: u32 = 240;
/// How thick the collapsed handle is, in logical pixels (DESIGN.md §5 Panel: "a 6px tab").
pub const HANDLE_THICKNESS: u32 = 6;

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// The monitor the Panel is on.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// A stable name for the monitor: [`Monitor::id_of`].
    pub id: String,
    /// The part of it a window may use (without the taskbar, where the host can tell).
    pub work_area: Rect,
    /// The edge the taskbar sits on (the side the work area is inset from), `None` when it
    /// is hidden or the host cannot tell. The Panel never docks to it.
    pub taskbar: Option<DockEdge>,
    /// Physical pixels per logical pixel (1.5 at 150 % text scale).
    pub scale: f32,
}

impl Monitor {
    /// The id for a monitor called `name` that is `w` by `h` pixels: the name alone is not
    /// unique (two identical displays) or stable (a resolution change moves everything).
    #[must_use]
    pub fn id_of(name: &str, w: u32, h: u32) -> String {
        format!("{name} {w}x{h}")
    }
}

/// Which edge or corner of the Panel the user pulled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Left edge.
    Left,
    /// Right edge.
    Right,
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
    /// Top-left corner.
    TopLeft,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-right corner.
    BottomRight,
}

impl Edge {
    /// A name for the log.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
        }
    }
}

impl Rect {
    /// The rectangle a [`PanelGeometry`] describes.
    #[must_use]
    pub fn of(g: &PanelGeometry) -> Self {
        Self {
            x: g.x,
            y: g.y,
            w: g.w,
            h: g.h,
        }
    }

    /// The geometry to remember for `monitor`, docked to `edge` or floating.
    #[must_use]
    pub fn geometry(self, monitor: &Monitor, edge: Option<DockEdge>) -> PanelGeometry {
        PanelGeometry {
            monitor_id: monitor.id.clone(),
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
            edge,
            pre_dock: None,
        }
    }

    /// The span this rectangle covers along `edge`: its height for a left or right edge, its
    /// width for a top or bottom one. The handle keeps it, so it opens where it was.
    #[must_use]
    pub fn along(self, edge: DockEdge) -> Range<i32> {
        let (start, len) = if edge.vertical() {
            (self.y, self.h)
        } else {
            (self.x, self.w)
        };
        let end = i64::from(start) + i64::from(len);
        start..i32::try_from(end).unwrap_or(i32::MAX)
    }
}

fn scaled(logical: u32, scale: f32) -> u32 {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a small positive pixel count, rounded"
    )]
    let pixels = (f64::from(logical) * f64::from(scale)).round() as u32;
    pixels
}

/// Puts `frame` where the Panel may be on `monitor`: width within 260 to 480 logical pixels,
/// height from the minimum to the work area's, and the whole frame inside the work area (a
/// frame that cannot fit is cut to the work area).
#[must_use]
pub fn clamp(frame: Rect, monitor: &Monitor) -> Rect {
    let area = monitor.work_area;
    let min_w = scaled(*PANEL_WIDTH.start(), monitor.scale);
    let max_w = scaled(*PANEL_WIDTH.end(), monitor.scale);
    let min_h = scaled(MIN_HEIGHT, monitor.scale);
    let w = frame.w.clamp(min_w, max_w).min(area.w);
    let h = frame.h.max(min_h).min(area.h);
    Rect {
        x: inside(frame.x, w, area.x, area.w),
        y: inside(frame.y, h, area.y, area.h),
        w,
        h,
    }
}

/// Puts any window's frame inside `area` (a monitor's work area): a frame larger than the
/// area is cut to it, one hanging over an edge is moved in. The Main window, Settings and the
/// editors open at their preferred size, which does not fit a small or scaled screen.
#[must_use]
pub fn fit(frame: Rect, area: Rect) -> Rect {
    let w = frame.w.min(area.w);
    let h = frame.h.min(area.h);
    Rect {
        x: inside(frame.x, w, area.x, area.w),
        y: inside(frame.y, h, area.y, area.h),
        w,
        h,
    }
}

/// The collapsed handle of a Panel docked to `edge` of `work_area`: a strip
/// [`HANDLE_THICKNESS`] logical pixels thick, flush with the edge, spanning `along` (the
/// Panel's former height for a left or right edge, its former width for a top or bottom one).
/// The strip never leaves the work area: `along` is cut to it, and an `along` that misses it
/// entirely (or is empty) spans the whole edge, so there is always a handle to find.
#[must_use]
pub fn handle_rect(edge: DockEdge, work_area: Rect, along: Range<i32>, scale: f32) -> Rect {
    let across_len = if edge.vertical() {
        work_area.w
    } else {
        work_area.h
    };
    let thickness = scaled(HANDLE_THICKNESS, scale).clamp(1, across_len.max(1));
    let (low, len) = if edge.vertical() {
        (work_area.y, work_area.h)
    } else {
        (work_area.x, work_area.w)
    };
    let (start, span) = cut(along, low, len);
    match edge {
        DockEdge::Left => Rect {
            x: work_area.x,
            y: start,
            w: thickness,
            h: span,
        },
        DockEdge::Right => Rect {
            x: far_side(work_area.x, work_area.w, thickness),
            y: start,
            w: thickness,
            h: span,
        },
        DockEdge::Top => Rect {
            x: start,
            y: work_area.y,
            w: span,
            h: thickness,
        },
        DockEdge::Bottom => Rect {
            x: start,
            y: far_side(work_area.y, work_area.h, thickness),
            w: span,
            h: thickness,
        },
    }
}

/// The expanded Panel of `size` (width, height) docked to `edge` of `work_area`, flush with
/// it. On a left or right edge it takes the work area's full height (DESIGN.md: "full height
/// when docked"); on a top or bottom edge it keeps its height and sits in the middle of the
/// edge. The width is held between 260 and 480 logical pixels like any Panel (at `scale`).
#[must_use]
pub fn docked_rect(edge: DockEdge, work_area: Rect, size: (u32, u32), scale: f32) -> Rect {
    let min_w = scaled(*PANEL_WIDTH.start(), scale);
    let max_w = scaled(*PANEL_WIDTH.end(), scale);
    let w = size.0.clamp(min_w, max_w).min(work_area.w);
    if edge.vertical() {
        Rect {
            x: if edge == DockEdge::Left {
                work_area.x
            } else {
                far_side(work_area.x, work_area.w, w)
            },
            y: work_area.y,
            w,
            h: work_area.h,
        }
    } else {
        let h = size.1.max(scaled(MIN_HEIGHT, scale)).min(work_area.h);
        Rect {
            x: inside(
                work_area.x.saturating_add_unsigned((work_area.w - w) / 2),
                w,
                work_area.x,
                work_area.w,
            ),
            y: if edge == DockEdge::Top {
                work_area.y
            } else {
                far_side(work_area.y, work_area.h, h)
            },
            w,
            h,
        }
    }
}

/// The size (width, height) a Panel brings to a dock on `edge`, from the `size` it has now (or
/// has stored), the `pre_dock` size it had before it was docked and a `default_height`
/// (logical pixels).
///
/// A Panel docked left or right is as tall as the work area, so its frame is no guide to the
/// height of a top or bottom dock: taken as it is, that dock would cover the whole screen with
/// its bottom out of reach. A full-height size on a top or bottom edge therefore falls back to
/// the size before docking, or to the default height when that was full height too. A left or
/// right dock ignores the height anyway.
#[must_use]
pub fn dock_size(
    edge: DockEdge,
    size: (u32, u32),
    pre_dock: Option<(u32, u32)>,
    work_area: Rect,
    scale: f32,
    default_height: u32,
) -> (u32, u32) {
    // Within a pixel or two of the work area's height: a side dock's frame (or rounding of one).
    let full = |h: u32| h.saturating_add(2) >= work_area.h;
    if edge.vertical() || !full(size.1) {
        return size;
    }
    let height = pre_dock
        .map(|(_, h)| h)
        .filter(|h| !full(*h))
        .unwrap_or_else(|| scaled(default_height, scale).min(work_area.h / 2));
    (size.0, height)
}

/// The work area last read for each monitor: the system sometimes fails to answer, and the
/// whole monitor is the wrong stand-in (a Panel docked to its bottom edge then ends under the
/// taskbar, out of reach).
#[derive(Debug, Default)]
pub struct WorkAreaMemory {
    known: Vec<(String, Rect)>,
}

impl WorkAreaMemory {
    /// The work area of monitor `id`: the `read` one when it is usable (it is remembered), else
    /// the last good one for that monitor, else `fallback` (what the host can offer when it can
    /// never read one; `None` means "no placement now").
    pub fn resolve(
        &mut self,
        id: &str,
        read: Option<Rect>,
        fallback: Option<Rect>,
    ) -> Option<Rect> {
        if let Some(area) = read.filter(|r| r.w > 0 && r.h > 0) {
            match self.known.iter_mut().find(|(known, _)| known == id) {
                Some((_, last)) => *last = area,
                None => self.known.push((id.to_owned(), area)),
            }
            return Some(area);
        }
        self.known
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, area)| *area)
            .or(fallback)
    }
}

/// `frame` lies against `edge` of `work_area`, to within two pixels (rounding at a fractional
/// scale): a docked Panel that still is.
#[must_use]
pub fn is_flush(edge: DockEdge, work_area: Rect, frame: Rect) -> bool {
    let near = |a: i64, b: i64| (a - b).abs() <= 2;
    let (x, y) = (i64::from(frame.x), i64::from(frame.y));
    let (right, bottom) = (x + i64::from(frame.w), y + i64::from(frame.h));
    let (ax, ay) = (i64::from(work_area.x), i64::from(work_area.y));
    let (aright, abottom) = (ax + i64::from(work_area.w), ay + i64::from(work_area.h));
    match edge {
        DockEdge::Left => near(x, ax),
        DockEdge::Right => near(right, aright),
        DockEdge::Top => near(y, ay),
        DockEdge::Bottom => near(bottom, abottom),
    }
}

/// The edge of `bounds` (the whole monitor) the taskbar takes: the side `work_area` is inset
/// from, the largest inset when app bars take several. `None` when the two are equal (an
/// auto-hidden or missing taskbar) or when `work_area` is not inside `bounds`.
#[must_use]
pub fn taskbar_edge(bounds: Rect, work_area: Rect) -> Option<DockEdge> {
    let far = |start: i32, len: u32| i64::from(start) + i64::from(len);
    let insets = [
        (i64::from(work_area.x) - i64::from(bounds.x), DockEdge::Left),
        (
            far(bounds.x, bounds.w) - far(work_area.x, work_area.w),
            DockEdge::Right,
        ),
        (i64::from(work_area.y) - i64::from(bounds.y), DockEdge::Top),
        (
            far(bounds.y, bounds.h) - far(work_area.y, work_area.h),
            DockEdge::Bottom,
        ),
    ];
    if insets.iter().any(|(inset, _)| *inset < 0) {
        return None;
    }
    insets
        .into_iter()
        .filter(|(inset, _)| *inset > 0)
        .max_by_key(|(inset, _)| *inset)
        .map(|(_, edge)| edge)
}

/// The edge of `work_area` nearest to the centre of `frame`, never the `taskbar` edge: where
/// "Dock" puts a floating Panel. A Panel in the lower middle over a bottom taskbar goes to
/// the left or the right, whichever is nearer (the left on a tie).
#[must_use]
pub fn nearest_dock_edge(frame: Rect, work_area: Rect, taskbar: Option<DockEdge>) -> DockEdge {
    // Twice the distances, so that the centre needs no division.
    let twice = |start: i32, len: u32| i64::from(start) * 2 + i64::from(len);
    let (cx, cy) = (twice(frame.x, frame.w), twice(frame.y, frame.h));
    let (left, top) = (cx - twice(work_area.x, 0), cy - twice(work_area.y, 0));
    let right = twice(work_area.x, 0) + 2 * i64::from(work_area.w) - cx;
    let bottom = twice(work_area.y, 0) + 2 * i64::from(work_area.h) - cy;
    let edges = [
        (left, DockEdge::Left),
        (right, DockEdge::Right),
        (top, DockEdge::Top),
        (bottom, DockEdge::Bottom),
    ];
    let nearest = |keep: &dyn Fn(DockEdge) -> bool| {
        edges
            .into_iter()
            .filter(|(_, edge)| keep(*edge))
            .min_by_key(|(distance, _)| *distance)
            .map_or(DockEdge::Right, |(_, edge)| edge)
    };
    let edge = nearest(&|_| true);
    if Some(edge) != taskbar {
        return edge;
    }
    // Nearest the taskbar: the nearer of the two sides next to it, never the far opposite edge.
    let horizontal = matches!(edge, DockEdge::Left | DockEdge::Right);
    nearest(&|e| matches!(e, DockEdge::Left | DockEdge::Right) != horizontal)
}

/// How close (logical pixels) a dragged Panel's side must come to an edge of the work area
/// for the drop to dock it there.
pub const DOCK_SNAP: u32 = 16;

/// The edge a drag would dock the Panel to when it is released now: the edge of `work_area`
/// that the same side of `frame` lies within [`DOCK_SNAP`] of (at `scale`), the nearer one in
/// a corner, never the `taskbar` edge. `None` away from every edge.
#[must_use]
pub fn dock_preview(
    frame: Rect,
    work_area: Rect,
    taskbar: Option<DockEdge>,
    scale: f32,
) -> Option<DockEdge> {
    let far = |start: i32, len: u32| i64::from(start) + i64::from(len);
    let snap = i64::from(scaled(DOCK_SNAP, scale));
    [
        (i64::from(frame.x) - i64::from(work_area.x), DockEdge::Left),
        (
            far(work_area.x, work_area.w) - far(frame.x, frame.w),
            DockEdge::Right,
        ),
        (i64::from(frame.y) - i64::from(work_area.y), DockEdge::Top),
        (
            far(work_area.y, work_area.h) - far(frame.y, frame.h),
            DockEdge::Bottom,
        ),
    ]
    .into_iter()
    .map(|(gap, edge)| (gap.abs(), edge))
    .filter(|(gap, edge)| *gap <= snap && Some(*edge) != taskbar)
    .min_by_key(|(gap, _)| *gap)
    .map(|(_, edge)| edge)
}

/// `span` cut to `[low, low + len)`; the whole of it when nothing of `span` is inside.
fn cut(span: Range<i32>, low: i32, len: u32) -> (i32, u32) {
    let high = i64::from(low) + i64::from(len);
    let start = i64::from(span.start).max(i64::from(low));
    let end = i64::from(span.end).min(high);
    if end <= start {
        return (low, len);
    }
    (
        i32::try_from(start).unwrap_or(low),
        u32::try_from(end - start).unwrap_or(len),
    )
}

/// The start of a strip `len` long that ends where `[low, low + span)` does.
fn far_side(low: i32, span: u32, len: u32) -> i32 {
    let start = i64::from(low) + i64::from(span) - i64::from(len);
    i32::try_from(start).unwrap_or(low)
}

/// The title bar and frame a regular window adds around its content, in logical pixels, with
/// some air so the window does not touch the taskbar.
pub const WINDOW_CHROME: f32 = 56.0;

/// The logical size a regular window is opened at: `wanted`, cut so that it (with its title bar)
/// fits the work area of `monitor`. A window taller than the screen opens half off it and its
/// lower part is never painted.
#[must_use]
pub fn open_size(wanted: (f32, f32), monitor: &Monitor) -> (f32, f32) {
    #[allow(
        clippy::cast_precision_loss,
        reason = "screen sizes are far below 2^24 pixels"
    )]
    let room = |physical: u32| (physical as f32 / monitor.scale - WINDOW_CHROME).max(0.0);
    let (w, h) = (
        wanted.0.min(room(monitor.work_area.w)),
        wanted.1.min(room(monitor.work_area.h)),
    );
    (w.max(1.0), h.max(1.0))
}

/// The air between a secondary window and the one it opens beside, in logical pixels.
pub const WINDOW_GAP: f32 = 8.0;

/// [`WINDOW_GAP`] in physical pixels on a display at `scale` (1.5 for 150 %), so the gap looks
/// the same on every screen.
#[must_use]
pub fn gap_px(scale: f32) -> u32 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a gap of a few pixels is positive and far below u32::MAX"
    )]
    let px = (WINDOW_GAP * scale.max(1.0)).round() as u32;
    px
}

/// How many pixels of `a` and `b` overlap.
fn overlap(a: Rect, b: Rect) -> u64 {
    let span = |a0: i32, al: u32, b0: i32, bl: u32| {
        let lo = i64::from(a0).max(i64::from(b0));
        let hi = (i64::from(a0) + i64::from(al)).min(i64::from(b0) + i64::from(bl));
        u64::try_from((hi - lo).max(0)).unwrap_or(0)
    };
    span(a.x, a.w, b.x, b.w) * span(a.y, a.h, b.y, b.h)
}

/// Where a secondary window of outer `size` (physical pixels) opens beside `parent`, the window
/// it was opened from, on `work_area` (the monitor without its taskbar), `gap` pixels away
/// (see [`gap_px`]). The sides are tried in order: right of the parent, left, below, above; a
/// side fits when the whole window is on the work area there. The window lines up with the
/// parent's top (or left edge) and is moved along the side as far as the work area needs.
///
/// When no side fits, the window is cut to the work area and put on the side where it covers
/// the least of the parent. When it cannot avoid covering the parent (a maximised parent, say),
/// it is centred over it instead. `parent` may be on another monitor, or half off screen: the
/// result is always on `work_area`. Pass the work area itself as the parent to centre a window
/// on the screen.
#[must_use]
pub fn place_beside(parent: Rect, size: (u32, u32), work_area: Rect, gap: u32) -> Rect {
    let (w, h) = (size.0.min(work_area.w), size.1.min(work_area.h));
    let (gap, wide, high) = (i64::from(gap), i64::from(w), i64::from(h));
    let (px, py) = (i64::from(parent.x), i64::from(parent.y));
    let (right, below) = (
        px + i64::from(parent.w) + gap,
        py + i64::from(parent.h) + gap,
    );
    // Right, left, below, above: the wanted corner, before the work area is taken into account.
    let wanted = [
        (right, py),
        (px - gap - wide, py),
        (px, below),
        (px, py - gap - high),
    ];
    let clamp = |x: i64, y: i64| {
        let x = i32::try_from(x).unwrap_or(work_area.x);
        let y = i32::try_from(y).unwrap_or(work_area.y);
        Rect {
            x: inside(x, w, work_area.x, work_area.w),
            y: inside(y, h, work_area.y, work_area.h),
            w,
            h,
        }
    };
    let placed: Vec<Rect> = wanted.iter().map(|&(x, y)| clamp(x, y)).collect();
    // The side axis is the one the window was put past the parent on: it must not have moved.
    let fits = |i: usize| {
        let (at, (x, y)) = (placed[i], wanted[i]);
        if i < 2 {
            i64::from(at.x) == x
        } else {
            i64::from(at.y) == y
        }
    };
    if let Some(i) = (0..4).find(|&i| fits(i)) {
        return placed[i];
    }
    let (best, least) = placed
        .iter()
        .map(|&at| (at, overlap(at, parent)))
        .reduce(|best, next| if next.1 < best.1 { next } else { best })
        .unwrap_or((placed[0], 0));
    let unavoidable = u64::from(w) * u64::from(h);
    let parent_area = u64::from(parent.w) * u64::from(parent.h);
    if least < unavoidable.min(parent_area) {
        return best;
    }
    let half = |start: i32, span: u32, len: u32| {
        (i64::from(start) + (i64::from(span) - i64::from(len)) / 2)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX))
    };
    clamp(half(parent.x, parent.w, w), half(parent.y, parent.h, h))
}

/// `start` moved so `[start, start + len)` lies in `[low, low + span)`.
fn inside(start: i32, len: u32, low: i32, span: u32) -> i32 {
    let high = i64::from(low) + i64::from(span) - i64::from(len);
    let moved = i64::from(start).clamp(i64::from(low), high.max(i64::from(low)));
    i32::try_from(moved).unwrap_or(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(scale: f32) -> Monitor {
        Monitor {
            id: Monitor::id_of("DISPLAY1", 1920, 1080),
            work_area: Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            },
            taskbar: None,
            scale,
        }
    }

    fn at(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_window_that_fits_opens_at_its_wanted_size() {
        assert_eq!(open_size((640.0, 760.0), &monitor(1.0)), (640.0, 760.0));
    }

    #[test]
    fn a_window_taller_than_the_work_area_is_cut_to_it_with_room_for_the_title_bar() {
        // 1040 px of work area at 150 %: 693 logical, minus the chrome.
        let (w, h) = open_size((640.0, 760.0), &monitor(1.5));
        assert!((w - 640.0).abs() < 0.01, "{w}");
        assert!((h - (1040.0 / 1.5 - WINDOW_CHROME)).abs() < 0.01, "{h}");
        assert!(h < 760.0);
    }

    #[test]
    fn a_tiny_work_area_still_gives_a_positive_size() {
        let mut m = monitor(2.0);
        m.work_area = at(0, 0, 40, 20);
        assert_eq!(open_size((640.0, 760.0), &m), (1.0, 1.0));
    }

    #[test]
    fn width_is_held_between_260_and_480() {
        let m = monitor(1.0);
        assert_eq!(clamp(at(100, 100, 1000, 500), &m).w, 480);
        assert_eq!(clamp(at(100, 100, 100, 500), &m).w, 260);
        assert_eq!(clamp(at(100, 100, 333, 500), &m).w, 333);
    }

    #[test]
    fn the_limits_are_logical_so_they_grow_with_the_scale() {
        let m = monitor(1.5);
        assert_eq!(clamp(at(0, 0, 2000, 600), &m).w, 720);
        assert_eq!(clamp(at(0, 0, 10, 600), &m).w, 390);
        assert_eq!(clamp(at(0, 0, 400, 10), &m).h, 360, "min height 240 x 1.5");
    }

    #[test]
    fn height_is_held_between_the_minimum_and_the_work_area() {
        let m = monitor(1.0);
        assert_eq!(clamp(at(0, 0, 320, 5000), &m).h, 1040);
        assert_eq!(clamp(at(0, 0, 320, 50), &m).h, 240);
    }

    #[test]
    fn the_frame_is_moved_inside_the_work_area() {
        let m = monitor(1.0);
        assert_eq!(clamp(at(-500, -80, 320, 480), &m), at(0, 0, 320, 480));
        assert_eq!(clamp(at(1800, 900, 320, 480), &m), at(1600, 560, 320, 480));
        assert_eq!(clamp(at(100, 100, 320, 480), &m), at(100, 100, 320, 480));
    }

    #[test]
    fn a_work_area_that_does_not_start_at_zero_is_respected() {
        let mut m = monitor(1.0);
        m.work_area = at(-1920, 0, 1920, 1040);
        assert_eq!(clamp(at(100, 10, 320, 480), &m).x, -320);
        assert_eq!(clamp(at(-2500, 10, 320, 480), &m).x, -1920);
    }

    #[test]
    fn a_work_area_smaller_than_the_minimum_wins_over_it() {
        let mut m = monitor(1.0);
        m.work_area = at(0, 0, 200, 200);
        assert_eq!(clamp(at(50, 50, 320, 480), &m), at(0, 0, 200, 200));
    }

    #[test]
    fn a_nonsense_scale_counts_as_one() {
        for scale in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(clamp(at(0, 0, 1000, 500), &monitor(scale)).w, 480);
        }
    }

    fn area() -> Rect {
        at(0, 0, 1920, 1040)
    }

    #[test]
    fn a_left_handle_is_a_six_pixel_strip_over_the_panels_former_height() {
        let h = handle_rect(DockEdge::Left, area(), 100..700, 1.0);
        assert_eq!(h, at(0, 100, 6, 600));
    }

    #[test]
    fn a_right_handle_is_flush_with_the_far_edge() {
        let h = handle_rect(DockEdge::Right, area(), 0..1040, 1.0);
        assert_eq!(h, at(1914, 0, 6, 1040));
    }

    #[test]
    fn top_and_bottom_handles_run_along_the_width() {
        assert_eq!(
            handle_rect(DockEdge::Top, area(), 800..1120, 1.0),
            at(800, 0, 320, 6)
        );
        assert_eq!(
            handle_rect(DockEdge::Bottom, area(), 800..1120, 1.0),
            at(800, 1034, 320, 6)
        );
    }

    #[test]
    fn the_handle_thickness_is_logical_so_it_grows_with_the_scale() {
        assert_eq!(handle_rect(DockEdge::Left, area(), 0..500, 1.5).w, 9);
        assert_eq!(handle_rect(DockEdge::Top, area(), 0..500, 2.0).h, 12);
    }

    #[test]
    fn a_handle_is_cut_to_the_work_area() {
        assert_eq!(
            handle_rect(DockEdge::Left, area(), -300..500, 1.0),
            at(0, 0, 6, 500)
        );
        assert_eq!(
            handle_rect(DockEdge::Left, area(), 900..5000, 1.0),
            at(0, 900, 6, 140)
        );
    }

    #[test]
    fn a_span_outside_the_work_area_gives_the_whole_edge() {
        let reversed = Range {
            start: 700,
            end: 200,
        };
        for along in [5000..6000, -900..-100, 300..300, reversed] {
            assert_eq!(
                handle_rect(DockEdge::Right, area(), along.clone(), 1.0),
                at(1914, 0, 6, 1040),
                "{along:?}"
            );
        }
    }

    #[test]
    fn a_work_area_that_does_not_start_at_zero_gets_its_own_edge() {
        let secondary = at(-1920, 40, 1920, 1000);
        assert_eq!(
            handle_rect(DockEdge::Left, secondary, 100..500, 1.0),
            at(-1920, 100, 6, 400)
        );
        assert_eq!(
            handle_rect(DockEdge::Right, secondary, 100..500, 1.0),
            at(-6, 100, 6, 400)
        );
        assert_eq!(
            handle_rect(DockEdge::Bottom, secondary, -1800..-1500, 1.0),
            at(-1800, 1034, 300, 6)
        );
    }

    #[test]
    fn every_handle_lies_inside_the_work_area() {
        let work = at(-1920, 40, 1920, 1000);
        let (wx1, wy1) = (work.x + 1920, work.y + 1000);
        for edge in DockEdge::ALL {
            for along in [-5000..5000, -2000..-1000, 0..10, -100..100, 40..41] {
                for scale in [1.0, 1.25, 2.5] {
                    let r = handle_rect(edge, work, along.clone(), scale);
                    let (rx1, ry1) = (
                        r.x + i32::try_from(r.w).unwrap(),
                        r.y + i32::try_from(r.h).unwrap(),
                    );
                    assert!(
                        r.w > 0
                            && r.h > 0
                            && r.x >= work.x
                            && r.y >= work.y
                            && rx1 <= wx1
                            && ry1 <= wy1,
                        "{edge:?} {along:?} {scale}: {r:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_work_area_thinner_than_the_handle_still_holds_it() {
        let thin = at(0, 0, 4, 400);
        assert_eq!(handle_rect(DockEdge::Left, thin, 0..400, 1.0).w, 4);
    }

    #[test]
    fn a_docked_panel_on_the_left_or_right_takes_the_full_height() {
        assert_eq!(
            docked_rect(DockEdge::Left, area(), (320, 480), 1.0),
            at(0, 0, 320, 1040)
        );
        assert_eq!(
            docked_rect(DockEdge::Right, area(), (320, 480), 1.0),
            at(1600, 0, 320, 1040)
        );
    }

    #[test]
    fn a_docked_panel_on_the_top_or_bottom_keeps_its_height_and_centres() {
        assert_eq!(
            docked_rect(DockEdge::Top, area(), (320, 480), 1.0),
            at(800, 0, 320, 480)
        );
        assert_eq!(
            docked_rect(DockEdge::Bottom, area(), (320, 480), 1.0),
            at(800, 560, 320, 480)
        );
    }

    #[test]
    fn a_docked_panel_keeps_the_panel_width_limits_and_stays_inside() {
        assert_eq!(docked_rect(DockEdge::Left, area(), (9000, 480), 1.0).w, 480);
        assert_eq!(docked_rect(DockEdge::Left, area(), (10, 480), 1.0).w, 260);
        assert_eq!(docked_rect(DockEdge::Left, area(), (320, 480), 1.5).w, 390);
        assert_eq!(docked_rect(DockEdge::Top, area(), (320, 9000), 1.0).h, 1040);
        assert_eq!(docked_rect(DockEdge::Top, area(), (320, 10), 1.0).h, 240);
        let tiny = at(10, 20, 200, 200);
        assert_eq!(
            docked_rect(DockEdge::Right, tiny, (320, 480), 1.0),
            at(10, 20, 200, 200)
        );
        assert_eq!(
            docked_rect(DockEdge::Bottom, tiny, (320, 480), 1.0),
            at(10, 20, 200, 200)
        );
    }

    /// Work areas as Windows reports them with the taskbar on each edge, on a 1920 x 1080
    /// monitor, one with a negative origin (left of the primary one) and a small one.
    fn work_areas() -> [Rect; 6] {
        [
            at(0, 0, 1920, 1040),
            at(0, 40, 1920, 1040),
            at(60, 0, 1860, 1080),
            at(0, 0, 1860, 1080),
            at(-1920, 0, 1920, 1040),
            at(-1280, -200, 1280, 664),
        ]
    }

    fn inside_area(frame: Rect, area: Rect) -> bool {
        let (x, y) = (i64::from(frame.x), i64::from(frame.y));
        x >= i64::from(area.x)
            && y >= i64::from(area.y)
            && x + i64::from(frame.w) <= i64::from(area.x) + i64::from(area.w)
            && y + i64::from(frame.h) <= i64::from(area.y) + i64::from(area.h)
    }

    #[test]
    fn a_docked_frame_never_passes_the_work_area_bottom_or_right() {
        // The taskbar inset is what makes a whole-monitor frame wrong: 1080 against 1040.
        for work in work_areas() {
            for scale in [1.0, 1.5, 2.0] {
                for size in [(320, 480), (320, 1080), (9000, 9000), (10, 10), (480, 1040)] {
                    for edge in DockEdge::ALL {
                        let open = docked_rect(edge, work, size, scale);
                        assert!(
                            inside_area(open, work),
                            "{edge:?} {size:?} at {scale} in {work:?}: {open:?}"
                        );
                        let handle = handle_rect(edge, work, open.along(edge), scale);
                        assert!(inside_area(handle, work), "{edge:?} handle {handle:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_bottom_dock_ends_on_the_taskbar_and_not_on_the_monitor_edge() {
        let work = at(0, 0, 1920, 1040);
        let docked = docked_rect(DockEdge::Bottom, work, (320, 480), 1.0);
        assert_eq!(i64::from(docked.y) + i64::from(docked.h), 1040);
        let right = docked_rect(DockEdge::Right, at(0, 0, 1860, 1080), (320, 480), 1.0);
        assert_eq!(i64::from(right.x) + i64::from(right.w), 1860);
    }

    #[test]
    fn a_top_or_bottom_dock_never_reuses_a_full_height_frame_from_a_side_dock() {
        let work = area();
        // A Panel docked left or right is as tall as the work area; its stored frame says so.
        let from_side = (320, 1040);
        for edge in [DockEdge::Top, DockEdge::Bottom] {
            // The size it had before docking comes back.
            assert_eq!(
                dock_size(edge, from_side, Some((340, 500)), work, 1.0, 480),
                (320, 500),
                "{edge:?}"
            );
            // Without one it starts at the default height, scaled.
            assert_eq!(dock_size(edge, from_side, None, work, 1.0, 480), (320, 480));
            // At 150 % the work area is 1.5 times larger in pixels, and so is the default.
            let big = at(0, 0, 2880, 1560);
            assert_eq!(
                dock_size(edge, (480, 1560), None, big, 1.5, 480),
                (480, 720)
            );
            // A pre-dock size that was itself full height is no use either.
            assert_eq!(
                dock_size(edge, from_side, Some((320, 1040)), work, 1.0, 480),
                (320, 480)
            );
            let rect = docked_rect(
                edge,
                work,
                dock_size(edge, from_side, None, work, 1.0, 480),
                1.0,
            );
            assert!(rect.h < work.h, "{edge:?}: {rect:?}");
        }
    }

    #[test]
    fn the_dock_size_leaves_everything_else_alone() {
        let work = area();
        // A side dock ignores the height anyway.
        for edge in [DockEdge::Left, DockEdge::Right] {
            assert_eq!(
                dock_size(edge, (320, 1040), Some((300, 500)), work, 1.0, 480),
                (320, 1040)
            );
        }
        // A floating Panel of ordinary height keeps it for a top or bottom dock.
        assert_eq!(
            dock_size(
                DockEdge::Bottom,
                (320, 600),
                Some((300, 500)),
                work,
                1.0,
                480
            ),
            (320, 600)
        );
        // A work area shorter than the default height still gets a frame that fits.
        let low = at(0, 0, 1000, 300);
        let (_, h) = dock_size(DockEdge::Top, (320, 300), None, low, 1.0, 480);
        assert!(h <= 300);
    }

    #[test]
    fn the_work_area_is_remembered_when_the_system_cannot_be_read() {
        let mut memory = WorkAreaMemory::default();
        let taskbar = at(0, 0, 1920, 1040);
        let whole = at(0, 0, 1920, 1080);
        // Read: used and kept.
        assert_eq!(memory.resolve("a", Some(taskbar), None), Some(taskbar));
        // The read fails: the last good one, never the whole monitor.
        assert_eq!(memory.resolve("a", None, None), Some(taskbar));
        assert_eq!(memory.resolve("a", None, Some(whole)), Some(taskbar));
        // A degenerate read counts as a failure.
        assert_eq!(
            memory.resolve("a", Some(at(0, 0, 0, 1040)), None),
            Some(taskbar)
        );
        // A newer good read replaces it; other monitors are separate.
        let moved = at(0, 40, 1920, 1040);
        assert_eq!(memory.resolve("a", Some(moved), None), Some(moved));
        assert_eq!(memory.resolve("b", None, None), None, "nothing known: skip");
        // Where the host cannot tell at all (the dev build), the fallback is used.
        assert_eq!(memory.resolve("b", None, Some(whole)), Some(whole));
    }

    #[test]
    fn a_collapsed_panel_opens_where_its_handle_is() {
        // The handle keeps the span of the docked panel, so expanding is the inverse of
        // collapsing for the span, and both sit flush on the same edge.
        for edge in DockEdge::ALL {
            let docked = docked_rect(edge, area(), (320, 480), 1.0);
            let handle = handle_rect(edge, area(), docked.along(edge), 1.0);
            assert_eq!(handle.along(edge), docked.along(edge), "{edge:?}");
            let flush = match edge {
                DockEdge::Left => handle.x == docked.x,
                DockEdge::Right => handle.x + 6 == docked.x + 320,
                DockEdge::Top => handle.y == docked.y,
                DockEdge::Bottom => handle.y + 6 == docked.y + 480,
            };
            assert!(flush, "{edge:?}: {handle:?} vs {docked:?}");
        }
    }

    #[test]
    fn a_frame_against_its_edge_is_flush_to_within_two_pixels() {
        let work = area();
        assert!(is_flush(DockEdge::Left, work, at(0, 100, 320, 500)));
        assert!(is_flush(DockEdge::Left, work, at(2, 100, 320, 500)));
        assert!(!is_flush(DockEdge::Left, work, at(3, 100, 320, 500)));
        assert!(is_flush(DockEdge::Right, work, at(1600, 0, 320, 500)));
        assert!(!is_flush(DockEdge::Right, work, at(1500, 0, 320, 500)));
        assert!(is_flush(DockEdge::Top, work, at(700, 0, 320, 500)));
        assert!(is_flush(DockEdge::Bottom, work, at(700, 540, 320, 500)));
        assert!(!is_flush(DockEdge::Bottom, work, at(700, 400, 320, 500)));
    }

    #[test]
    fn the_nearest_dock_edge_is_the_one_the_panel_centre_is_closest_to() {
        let work = area();
        assert_eq!(
            nearest_dock_edge(at(20, 300, 320, 480), work, None),
            DockEdge::Left
        );
        assert_eq!(
            nearest_dock_edge(at(1500, 300, 320, 480), work, None),
            DockEdge::Right
        );
        assert_eq!(
            nearest_dock_edge(at(800, 5, 320, 240), work, None),
            DockEdge::Top
        );
        assert_eq!(
            nearest_dock_edge(at(800, 790, 320, 240), work, None),
            DockEdge::Bottom
        );
    }

    #[test]
    fn the_taskbar_edge_is_never_the_dock_edge() {
        let work = area();
        // Lower middle over a bottom taskbar: left or right, the nearer one; the left on a tie.
        let lower_middle = at(800, 790, 320, 240);
        assert_eq!(
            nearest_dock_edge(lower_middle, work, Some(DockEdge::Bottom)),
            DockEdge::Left
        );
        assert_eq!(
            nearest_dock_edge(at(900, 790, 320, 240), work, Some(DockEdge::Bottom)),
            DockEdge::Right
        );
        // The corners: the nearest edge, and at the bottom a side, never the taskbar.
        for (corner, edge) in [
            (at(0, 0, 320, 240), DockEdge::Top),
            (at(1600, 0, 320, 240), DockEdge::Top),
            (at(0, 800, 320, 240), DockEdge::Left),
            (at(1600, 800, 320, 240), DockEdge::Right),
        ] {
            assert_eq!(
                nearest_dock_edge(corner, work, Some(DockEdge::Bottom)),
                edge,
                "{corner:?}"
            );
        }
        // The taskbar on each side: the panel against it goes to the nearest other edge.
        assert_eq!(
            nearest_dock_edge(at(0, 300, 320, 240), work, Some(DockEdge::Left)),
            DockEdge::Top
        );
        assert_eq!(
            nearest_dock_edge(at(1600, 300, 320, 240), work, Some(DockEdge::Right)),
            DockEdge::Top
        );
        assert_eq!(
            nearest_dock_edge(at(800, 0, 320, 240), work, Some(DockEdge::Top)),
            DockEdge::Left
        );
        assert_eq!(
            nearest_dock_edge(at(900, 800, 320, 240), work, Some(DockEdge::Bottom)),
            DockEdge::Right
        );
        // No taskbar (auto-hidden): every edge counts.
        assert_eq!(
            nearest_dock_edge(lower_middle, work, None),
            DockEdge::Bottom
        );
    }

    #[test]
    fn the_taskbar_edge_is_the_side_the_work_area_is_inset_from() {
        let whole = at(0, 0, 1920, 1080);
        assert_eq!(
            taskbar_edge(whole, at(0, 0, 1920, 1040)),
            Some(DockEdge::Bottom)
        );
        assert_eq!(
            taskbar_edge(whole, at(0, 40, 1920, 1040)),
            Some(DockEdge::Top)
        );
        assert_eq!(
            taskbar_edge(whole, at(60, 0, 1860, 1080)),
            Some(DockEdge::Left)
        );
        assert_eq!(
            taskbar_edge(whole, at(0, 0, 1860, 1080)),
            Some(DockEdge::Right)
        );
        // Auto-hidden or no taskbar; a second monitor at an offset.
        assert_eq!(taskbar_edge(whole, whole), None);
        assert_eq!(
            taskbar_edge(at(1920, 0, 1280, 1024), at(1920, 0, 1280, 984)),
            Some(DockEdge::Bottom)
        );
        // A work area that is not inside the monitor is not trusted.
        assert_eq!(taskbar_edge(whole, at(-10, 0, 1920, 1040)), None);
    }

    #[test]
    fn a_drag_previews_the_edge_its_side_comes_within_the_snap_of() {
        let work = area();
        assert_eq!(
            dock_preview(at(10, 300, 320, 480), work, None, 1.0),
            Some(DockEdge::Left)
        );
        assert_eq!(dock_preview(at(17, 300, 320, 480), work, None, 1.0), None);
        assert_eq!(
            dock_preview(at(1590, 300, 320, 480), work, None, 1.0),
            Some(DockEdge::Right)
        );
        assert_eq!(
            dock_preview(at(800, 12, 320, 480), work, None, 1.0),
            Some(DockEdge::Top)
        );
        assert_eq!(
            dock_preview(at(800, 550, 320, 480), work, None, 1.0),
            Some(DockEdge::Bottom)
        );
        // Never the taskbar edge: nothing near it, the side in a corner.
        assert_eq!(
            dock_preview(at(800, 550, 320, 480), work, Some(DockEdge::Bottom), 1.0),
            None
        );
        assert_eq!(
            dock_preview(at(4, 555, 320, 480), work, Some(DockEdge::Bottom), 1.0),
            Some(DockEdge::Left)
        );
        // In a corner the nearer edge wins; the snap grows with the scale.
        assert_eq!(
            dock_preview(at(8, 3, 320, 480), work, None, 1.0),
            Some(DockEdge::Top)
        );
        assert_eq!(
            dock_preview(at(30, 300, 320, 480), work, None, 2.0),
            Some(DockEdge::Left)
        );
        // A frame past the edge (half off screen) still previews it.
        assert_eq!(
            dock_preview(at(-8, 300, 320, 480), work, None, 1.0),
            Some(DockEdge::Left)
        );
    }

    #[test]
    fn a_window_taller_than_the_work_area_is_cut_and_moved_onto_it() {
        let area = at(0, 0, 1920, 1040);
        assert_eq!(fit(at(400, 100, 960, 1140), area), at(400, 0, 960, 1040));
        assert_eq!(fit(at(1700, 900, 640, 760), area), at(1280, 280, 640, 760));
        assert_eq!(fit(at(-50, -10, 3000, 3000), area), area);
    }

    fn screen() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 1920,
            h: 1040,
        }
    }

    const GAP: u32 = 8;

    #[test]
    fn a_window_opens_right_of_its_parent_lined_up_with_its_top() {
        let parent = at(400, 200, 500, 600);
        let placed = place_beside(parent, (600, 400), screen(), GAP);
        assert_eq!(placed, at(908, 200, 600, 400));
    }

    #[test]
    fn a_window_opens_left_of_a_parent_with_no_room_on_its_right() {
        let parent = at(1200, 200, 700, 600);
        let placed = place_beside(parent, (600, 400), screen(), GAP);
        assert_eq!(placed, at(592, 200, 600, 400));
    }

    #[test]
    fn a_window_opens_below_a_parent_wide_enough_to_leave_no_side_room() {
        let parent = at(100, 50, 1700, 300);
        let placed = place_beside(parent, (600, 400), screen(), GAP);
        assert_eq!(placed, at(100, 358, 600, 400));
    }

    #[test]
    fn a_window_opens_above_a_parent_low_on_a_wide_screen() {
        let parent = at(100, 700, 1700, 300);
        let placed = place_beside(parent, (600, 400), screen(), GAP);
        assert_eq!(placed, at(100, 292, 600, 400));
    }

    #[test]
    fn a_tall_window_beside_a_low_parent_is_moved_up_onto_the_work_area() {
        let parent = at(100, 800, 400, 200);
        let placed = place_beside(parent, (560, 700), screen(), GAP);
        assert_eq!(placed, at(508, 340, 560, 700));
    }

    #[test]
    fn the_gap_is_kept_between_the_windows() {
        let parent = at(100, 100, 300, 300);
        let placed = place_beside(parent, (200, 200), screen(), 12);
        assert_eq!(placed.x, 412);
    }

    #[test]
    fn with_no_side_that_fits_the_window_covers_the_least_of_the_parent() {
        let parent = at(100, 70, 1700, 900);
        let placed = place_beside(parent, (600, 400), screen(), GAP);
        assert_eq!(placed, at(1320, 70, 600, 400));
        assert!(placed.x + 600 <= 1920 && placed.y + 400 <= 1040);
        assert!(overlap(placed, parent) < 600 * 400);
    }

    #[test]
    fn a_parent_filling_the_work_area_gets_the_window_centred_over_it() {
        let placed = place_beside(screen(), (600, 400), screen(), GAP);
        assert_eq!(placed, at(660, 320, 600, 400));
    }

    #[test]
    fn a_window_larger_than_the_work_area_is_cut_to_it() {
        let placed = place_beside(screen(), (3000, 2000), screen(), GAP);
        assert_eq!(placed, screen());
    }

    #[test]
    fn a_parent_on_another_monitor_still_gives_a_window_on_this_one() {
        let second = at(1920, 0, 1280, 720);
        let parent = at(-200, 100, 600, 400);
        let placed = place_beside(parent, (500, 500), second, GAP);
        assert_eq!(placed, at(1920, 100, 500, 500));
        assert_eq!(overlap(placed, parent), 0);
        // The work area itself as the parent is the centre of the screen.
        let centred = place_beside(second, (500, 500), second, GAP);
        assert_eq!((centred.x, centred.y), (1920 + 390, 110));
    }

    #[test]
    fn a_parent_on_a_monitor_left_of_the_origin_gets_the_window_on_its_right() {
        let area = at(-1920, 0, 1920, 1040);
        let placed = place_beside(at(-1800, 100, 400, 300), (500, 400), area, GAP);
        assert_eq!(placed, at(-1392, 100, 500, 400));
    }

    #[test]
    fn the_gap_follows_the_display_scale() {
        assert_eq!(gap_px(1.0), 8);
        assert_eq!(gap_px(1.5), 12);
        assert_eq!(gap_px(2.0), 16);
        assert_eq!(gap_px(0.0), 8);
    }

    #[test]
    fn a_window_that_fits_is_left_alone() {
        let area = at(-1920, 0, 1920, 1040);
        assert_eq!(
            fit(at(-1500, 100, 640, 760), area),
            at(-1500, 100, 640, 760)
        );
    }

    #[test]
    fn a_monitor_id_names_its_size_too() {
        assert_eq!(Monitor::id_of("DISPLAY1", 2560, 1440), "DISPLAY1 2560x1440");
    }
}
