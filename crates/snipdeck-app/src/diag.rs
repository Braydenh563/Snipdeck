//! `SNIPDECK_DIAG`: opt-in diagnostics for what only Windows shows (render glitches), for the
//! log `tools/windows-test/diag-render.ps1` collects. Off by default; any value switches the
//! `SNIPDECK_LOG` lines on too, so one file holds both.
//!
//! - `render`: every window's paint, erase, move and size messages, its styles, DPI and DWM
//!   attributes on first sight, and which redraws follow an area Windows invalidated
//!   (`snipdeck_platform::windows::paint_diag`).
//! - `fix` (implies `render`): additionally, a redraw that follows such an area damages the whole
//!   window ([`snipdeck_ui::damage::whole_window`]), the candidate fix for hypothesis H1 of
//!   `docs/reviews/render-glitch-analysis.md`, so one run with and one without tells it apart.
//!
//! Lines never carry window titles or item content.

use std::sync::OnceLock;

use slint::winit_030::winit::event::WindowEvent;

/// What `SNIPDECK_DIAG` asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Diag {
    /// Log what Windows says about painting each window.
    pub render: bool,
    /// Redraw a window whole after Windows invalidated part of it.
    pub fix: bool,
}

impl Diag {
    /// Whether anything is on.
    #[must_use]
    pub const fn any(self) -> bool {
        self.render || self.fix
    }
}

/// Reads `value` (the `SNIPDECK_DIAG` variable): a comma or space separated list of `render`,
/// `fix` and `all`; case and unknown words are ignored. `fix` implies `render`.
#[must_use]
pub fn parse(value: Option<&str>) -> Diag {
    let mut diag = Diag::default();
    for word in value
        .unwrap_or_default()
        .split([',', ' ', ';'])
        .map(|w| w.trim().to_ascii_lowercase())
    {
        match word.as_str() {
            "render" | "1" | "on" => diag.render = true,
            "fix" | "all" => {
                diag.render = true;
                diag.fix = true;
            }
            _ => {}
        }
    }
    diag
}

static DIAG: OnceLock<Diag> = OnceLock::new();

/// What this run asked for, read once.
#[must_use]
pub fn get() -> Diag {
    *DIAG.get_or_init(|| parse(std::env::var("SNIPDECK_DIAG").ok().as_deref()))
}

/// Whether the rectangle `(x, y, w, h)` reaches outside `screen` (the same shape): part of the
/// window is off every monitor, so Windows has no pixels for that part.
#[must_use]
pub fn reaches_off_screen(rect: (i32, i32, u32, u32), screen: (i32, i32, i32, i32)) -> bool {
    let (x, y) = (i64::from(rect.0), i64::from(rect.1));
    let (right, bottom) = (x + i64::from(rect.2), y + i64::from(rect.3));
    let (sx, sy) = (i64::from(screen.0), i64::from(screen.1));
    let (sright, sbottom) = (sx + i64::from(screen.2), sy + i64::from(screen.3));
    x < sx || y < sy || right > sright || bottom > sbottom
}

#[cfg(windows)]
fn log_line(line: &str) {
    crate::trace::write(format_args!("{line}"));
}

/// Starts the diagnostics asked for: call once on the UI thread, after the backend is selected.
pub fn start() {
    let diag = get();
    if !diag.any() {
        return;
    }
    crate::trace!(
        "diag: SNIPDECK_DIAG render={} fix={}; backend winit, renderer software (softbuffer, GDI \
         BitBlt of damaged rectangles only); every winit window is created transparent \
         (DWM blur-behind), so undrawn pixels show through",
        diag.render,
        diag.fix
    );
    #[cfg(windows)]
    {
        let ok = snipdeck_platform::windows::paint_diag::install(log_line);
        crate::trace!("diag: paint hooks installed={ok}");
    }
}

thread_local! {
    static SEEN: std::cell::RefCell<std::collections::HashSet<isize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Feeds one winit event of the window `name` (`"panel"`, a component name) to the diagnostics.
/// Does nothing unless `SNIPDECK_DIAG` asks for `render`.
pub fn window_event(name: &str, window: &slint::Window, event: &WindowEvent) {
    let diag = get();
    if !diag.render {
        return;
    }
    let hwnd = crate::native_window::handle_of(window).unwrap_or(0);
    if hwnd != 0 && SEEN.with(|s| s.borrow_mut().insert(hwnd)) {
        #[cfg(windows)]
        crate::trace!(
            "diag window {name}: first seen {}",
            snipdeck_platform::windows::paint_diag::describe_window(hwnd)
        );
    }
    match event {
        WindowEvent::Moved(at) => {
            let size = window.size();
            crate::trace!(
                "diag window {name} hwnd={hwnd:#x}: moved to ({}, {}) size {}x{}{}",
                at.x,
                at.y,
                size.width,
                size.height,
                off_screen_note((at.x, at.y, size.width, size.height))
            );
        }
        WindowEvent::Resized(size) => {
            let at = window.position();
            crate::trace!(
                "diag window {name} hwnd={hwnd:#x}: resized to {}x{} at ({}, {}) scale {}{}",
                size.width,
                size.height,
                at.x,
                at.y,
                window.scale_factor(),
                off_screen_note((at.x, at.y, size.width, size.height))
            );
        }
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            crate::trace!("diag window {name} hwnd={hwnd:#x}: scale factor {scale_factor}");
        }
        WindowEvent::Focused(focused) => {
            crate::trace!("diag window {name} hwnd={hwnd:#x}: focused {focused}");
        }
        WindowEvent::Occluded(occluded) => {
            crate::trace!("diag window {name} hwnd={hwnd:#x}: occluded {occluded}");
        }
        WindowEvent::RedrawRequested if os_invalidated(hwnd) => {
            if diag.fix {
                snipdeck_ui::damage::whole_window(window);
                crate::trace!(
                    "diag window {name} hwnd={hwnd:#x}: redraw after Windows invalidated an \
                     area; fix on: whole window damaged"
                );
            } else {
                crate::trace!(
                    "diag window {name} hwnd={hwnd:#x}: redraw after Windows invalidated an \
                     area; fix off: only what Slint thinks changed is drawn"
                );
            }
        }
        _ => {}
    }
}

#[cfg(windows)]
fn os_invalidated(hwnd: isize) -> bool {
    hwnd != 0 && snipdeck_platform::windows::paint_diag::take_os_invalidated(hwnd)
}

#[cfg(not(windows))]
const fn os_invalidated(_hwnd: isize) -> bool {
    false
}

#[cfg(windows)]
fn off_screen_note(rect: (i32, i32, u32, u32)) -> &'static str {
    if reaches_off_screen(
        rect,
        snipdeck_platform::windows::paint_diag::virtual_screen(),
    ) {
        " (partly OFF SCREEN)"
    } else {
        ""
    }
}

#[cfg(not(windows))]
const fn off_screen_note(_rect: (i32, i32, u32, u32)) -> &'static str {
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_on_unless_asked() {
        assert_eq!(parse(None), Diag::default());
        assert_eq!(parse(Some("")), Diag::default());
        assert_eq!(parse(Some("verbose")), Diag::default());
        assert!(!parse(None).any());
    }

    #[test]
    fn render_and_fix_are_read_from_a_list_and_fix_implies_render() {
        let render = Diag {
            render: true,
            fix: false,
        };
        let both = Diag {
            render: true,
            fix: true,
        };
        assert_eq!(parse(Some("render")), render);
        assert_eq!(parse(Some(" RENDER ")), render);
        assert_eq!(parse(Some("render,fix")), both);
        assert_eq!(parse(Some("fix")), both);
        assert_eq!(parse(Some("all")), both);
        assert_eq!(parse(Some("render; other")), render);
    }

    #[test]
    fn a_window_past_any_edge_of_the_virtual_screen_reaches_off_screen() {
        let screen = (0, 0, 1920, 1080);
        assert!(!reaches_off_screen((100, 100, 400, 300), screen));
        assert!(
            !reaches_off_screen((1520, 780, 400, 300), screen),
            "touching"
        );
        assert!(reaches_off_screen((1600, 100, 400, 300), screen), "right");
        assert!(reaches_off_screen((-10, 100, 400, 300), screen), "left");
        assert!(reaches_off_screen((100, 900, 400, 300), screen), "bottom");
        // A second monitor on the left: the virtual screen starts at a negative x.
        assert!(!reaches_off_screen(
            (-1500, 100, 400, 300),
            (-1920, 0, 3840, 1080)
        ));
    }
}
