//! The records `SNIPDECK_DIAG=render` takes inside window hooks, and how a line of the log reads
//! (PLATFORM-40).
//!
//! The hook procedures run on the UI thread in the middle of message handling. They only build a
//! [`PaintRecord`] (plain numbers, no formatting, no file I/O) and push it to a channel; another
//! thread turns it into a line with [`format_record`] and writes it, so the diagnostics do not
//! change the timing they measure. Portable so the wording is tested on every OS.

use std::fmt::Write as _;

/// One thing a hook saw. `hwnd` is the window handle as a number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintRecord {
    /// `WM_PAINT` with an update region Windows made: `(left, top, width, height)`.
    Paint {
        /// The window.
        hwnd: isize,
        /// The update rectangle.
        update: (i32, i32, i32, i32),
    },
    /// `WM_ERASEBKGND`; `skipped` erases were skipped before this one.
    EraseBackground {
        /// The window.
        hwnd: isize,
        /// Erases skipped so far.
        skipped: u64,
    },
    /// `WM_WINDOWPOSCHANGED`.
    PosChanged {
        /// The window.
        hwnd: isize,
        /// New x.
        x: i32,
        /// New y.
        y: i32,
        /// New width.
        cx: i32,
        /// New height.
        cy: i32,
        /// `WINDOWPOS` flags.
        flags: u32,
    },
    /// `WM_SIZE`: the size type and the client size from `lParam`.
    Size {
        /// The window.
        hwnd: isize,
        /// `SIZE_RESTORED`, `SIZE_MINIMIZED`, ...
        kind: usize,
        /// Client width.
        width: usize,
        /// Client height.
        height: usize,
    },
    /// `WM_ENTERSIZEMOVE`.
    EnterSizeMove {
        /// The window.
        hwnd: isize,
    },
    /// `WM_EXITSIZEMOVE`.
    ExitSizeMove {
        /// The window.
        hwnd: isize,
    },
    /// `WM_DPICHANGED`.
    DpiChanged {
        /// The window.
        hwnd: isize,
        /// The new DPI.
        dpi: usize,
    },
    /// `WM_DISPLAYCHANGE`.
    DisplayChange {
        /// The window.
        hwnd: isize,
    },
    /// `WM_DWMCOMPOSITIONCHANGED`.
    CompositionChanged {
        /// The window.
        hwnd: isize,
    },
    /// `WM_SHOWWINDOW`.
    ShowWindow {
        /// The window.
        hwnd: isize,
        /// `wParam`: non-zero when shown.
        show: usize,
    },
    /// `WM_ACTIVATE`.
    Activate {
        /// The window.
        hwnd: isize,
        /// The low word of `wParam`.
        state: usize,
    },
}

/// `(left, top, width, height)` as the log shows a rectangle: `(l,t WxH)`.
#[must_use]
pub fn rect_text(left: i32, top: i32, width: i32, height: i32) -> String {
    format!("({left},{top} {width}x{height})")
}

/// The log line for `record`, taken `at_ms` milliseconds after the hooks were installed.
#[must_use]
pub fn format_record(record: &PaintRecord, at_ms: u64) -> String {
    let mut out = String::from("diag render: ");
    let hwnd = match *record {
        PaintRecord::Paint { hwnd, update } => {
            let _ = write!(
                out,
                "WM_PAINT hwnd={hwnd:#x} os_update={}",
                rect_text(update.0, update.1, update.2, update.3)
            );
            None
        }
        PaintRecord::EraseBackground { hwnd, skipped } => {
            let _ = write!(
                out,
                "WM_ERASEBKGND (class brush, unless a frame was just presented; skipped so far={skipped})"
            );
            Some(hwnd)
        }
        PaintRecord::PosChanged {
            hwnd,
            x,
            y,
            cx,
            cy,
            flags,
        } => {
            let _ = write!(
                out,
                "WM_WINDOWPOSCHANGED at=({x},{y}) size={cx}x{cy} flags={flags:#x}"
            );
            Some(hwnd)
        }
        PaintRecord::Size {
            hwnd,
            kind,
            width,
            height,
        } => {
            let _ = write!(out, "WM_SIZE type={kind} {width}x{height}");
            Some(hwnd)
        }
        PaintRecord::EnterSizeMove { hwnd } => {
            out.push_str("WM_ENTERSIZEMOVE (modal move/size loop starts)");
            Some(hwnd)
        }
        PaintRecord::ExitSizeMove { hwnd } => {
            out.push_str("WM_EXITSIZEMOVE");
            Some(hwnd)
        }
        PaintRecord::DpiChanged { hwnd, dpi } => {
            let _ = write!(out, "WM_DPICHANGED dpi={dpi}");
            Some(hwnd)
        }
        PaintRecord::DisplayChange { hwnd } => {
            out.push_str("WM_DISPLAYCHANGE");
            Some(hwnd)
        }
        PaintRecord::CompositionChanged { hwnd } => {
            out.push_str("WM_DWMCOMPOSITIONCHANGED");
            Some(hwnd)
        }
        PaintRecord::ShowWindow { hwnd, show } => {
            let _ = write!(out, "WM_SHOWWINDOW show={show}");
            Some(hwnd)
        }
        PaintRecord::Activate { hwnd, state } => {
            let _ = write!(out, "WM_ACTIVATE state={state}");
            Some(hwnd)
        }
    };
    if let Some(hwnd) = hwnd {
        let _ = write!(out, " hwnd={hwnd:#x}");
    }
    let _ = write!(out, " hook_at={at_ms}ms");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_names_the_update_rectangle() {
        let line = format_record(
            &PaintRecord::Paint {
                hwnd: 0x2a,
                update: (0, 4, 300, 200),
            },
            15,
        );
        assert_eq!(
            line,
            "diag render: WM_PAINT hwnd=0x2a os_update=(0,4 300x200) hook_at=15ms"
        );
    }

    #[test]
    fn erase_and_size_lines_keep_the_old_wording_and_add_the_hook_time() {
        let erase = format_record(
            &PaintRecord::EraseBackground {
                hwnd: 0x10,
                skipped: 3,
            },
            0,
        );
        assert_eq!(
            erase,
            "diag render: WM_ERASEBKGND (class brush, unless a frame was just presented; skipped so far=3) hwnd=0x10 hook_at=0ms"
        );
        let size = format_record(
            &PaintRecord::Size {
                hwnd: 0x10,
                kind: 0,
                width: 640,
                height: 480,
            },
            1200,
        );
        assert_eq!(
            size,
            "diag render: WM_SIZE type=0 640x480 hwnd=0x10 hook_at=1200ms"
        );
    }

    #[test]
    fn every_record_names_its_window_and_time() {
        let hwnd = 0x99;
        let all = [
            PaintRecord::Paint {
                hwnd,
                update: (0, 0, 1, 1),
            },
            PaintRecord::EraseBackground { hwnd, skipped: 0 },
            PaintRecord::PosChanged {
                hwnd,
                x: -5,
                y: 6,
                cx: 7,
                cy: 8,
                flags: 0x14,
            },
            PaintRecord::Size {
                hwnd,
                kind: 2,
                width: 1,
                height: 2,
            },
            PaintRecord::EnterSizeMove { hwnd },
            PaintRecord::ExitSizeMove { hwnd },
            PaintRecord::DpiChanged { hwnd, dpi: 144 },
            PaintRecord::DisplayChange { hwnd },
            PaintRecord::CompositionChanged { hwnd },
            PaintRecord::ShowWindow { hwnd, show: 1 },
            PaintRecord::Activate { hwnd, state: 2 },
        ];
        for record in all {
            let line = format_record(&record, 9);
            assert!(line.starts_with("diag render: WM_"), "{line}");
            assert!(line.contains("hwnd=0x99"), "{line}");
            assert!(line.ends_with("hook_at=9ms"), "{line}");
        }
    }

    #[test]
    fn a_position_change_shows_negative_coordinates_and_flags_in_hex() {
        let line = format_record(
            &PaintRecord::PosChanged {
                hwnd: 1,
                x: -5,
                y: 6,
                cx: 7,
                cy: 8,
                flags: 0x14,
            },
            0,
        );
        assert!(line.contains("at=(-5,6) size=7x8 flags=0x14"), "{line}");
    }
}
