//! Where the Panel sits and how large it may be (DESIGN.md §5 Panel): width 260 to 480
//! logical pixels, height up to the monitor's work area. Pure arithmetic with no window in it,
//! so the rules run under test; the host reads the real frame and monitor (winit, on Windows)
//! and applies what [`clamp`] returns.
//!
//! Everything is in physical pixels in desktop coordinates, as the OS reports it. The limits
//! are logical, so [`Monitor::scale`] turns them into pixels.

use crate::settings::{PANEL_WIDTH, PanelGeometry};

/// The shortest Panel, in logical pixels (the Slint window's `min-height`).
pub const MIN_HEIGHT: u32 = 240;

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

    /// The geometry to remember for `monitor`.
    #[must_use]
    pub fn geometry(self, monitor: &Monitor) -> PanelGeometry {
        PanelGeometry {
            monitor_id: monitor.id.clone(),
            x: self.x,
            y: self.y,
            w: self.w,
            h: self.h,
        }
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
            scale,
        }
    }

    fn at(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
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

    #[test]
    fn a_window_taller_than_the_work_area_is_cut_and_moved_onto_it() {
        let area = at(0, 0, 1920, 1040);
        assert_eq!(fit(at(400, 100, 960, 1140), area), at(400, 0, 960, 1040));
        assert_eq!(fit(at(1700, 900, 640, 760), area), at(1280, 280, 640, 760));
        assert_eq!(fit(at(-50, -10, 3000, 3000), area), area);
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
