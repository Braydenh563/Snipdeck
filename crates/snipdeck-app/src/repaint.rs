//! When a window must be drawn again from scratch (O39).
//!
//! Windows can throw away a window's pixels without telling the app: Alt+Tab back, restore from
//! minimise, a display or DPI change, unlock. Slint's software renderer then has nothing dirty
//! and presents no frame, so the client area stays blank (see `RepaintNudge` in the UI crate).
//! The window-event hooks translate each winit event to a [`Signal`] and ask [`Watch::on_signal`]
//! whether to nudge the window. The decision is pure so it is tested on every platform.

/// What happened to a window, reduced to what matters for repainting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// The window gained (`true`) or lost (`false`) the focus (Alt+Tab, `WM_ACTIVATE`).
    Focused(bool),
    /// The window became hidden by others or minimised (`true`) or visible again (`false`).
    Occluded(bool),
    /// The inner size changed; a minimised window reports 0 x 0.
    Resized {
        /// Physical width.
        width: u32,
        /// Physical height.
        height: u32,
    },
    /// The display scale changed (DPI change, moved to another display).
    ScaleFactorChanged,
    /// The system light or dark theme changed.
    ThemeChanged,
    /// Anything else: moves, pointer, keys, close.
    Other,
}

/// Remembers whether the window was minimised, so a restore is told from an ordinary resize.
#[derive(Clone, Copy, Debug, Default)]
pub struct Watch {
    minimised: bool,
}

impl Watch {
    /// A watch for a window that is not minimised.
    #[must_use]
    pub const fn new() -> Self {
        Self { minimised: false }
    }

    /// Whether the window needs a full redraw after `signal`.
    ///
    /// Yes for: gaining the focus, becoming visible again, leaving the minimised state (a size
    /// from 0 x 0 to a real one), and a scale or theme change. No for losing the focus, being
    /// hidden, going minimised, and every ordinary resize (Slint redraws those itself).
    pub fn on_signal(&mut self, signal: Signal) -> bool {
        match signal {
            Signal::Focused(gained) => gained,
            Signal::Occluded(hidden) => !hidden,
            Signal::Resized { width, height } => {
                let empty = width == 0 || height == 0;
                let restored = self.minimised && !empty;
                self.minimised = empty;
                restored
            }
            Signal::ScaleFactorChanged | Signal::ThemeChanged => true,
            Signal::Other => false,
        }
    }
}

/// The signal for a winit window event.
#[must_use]
pub fn signal_of(event: &slint::winit_030::winit::event::WindowEvent) -> Signal {
    use slint::winit_030::winit::event::WindowEvent;
    match event {
        WindowEvent::Focused(gained) => Signal::Focused(*gained),
        WindowEvent::Occluded(hidden) => Signal::Occluded(*hidden),
        WindowEvent::Resized(size) => Signal::Resized {
            width: size.width,
            height: size.height,
        },
        WindowEvent::ScaleFactorChanged { .. } => Signal::ScaleFactorChanged,
        WindowEvent::ThemeChanged(_) => Signal::ThemeChanged,
        _ => Signal::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn size(width: u32, height: u32) -> Signal {
        Signal::Resized { width, height }
    }

    #[test]
    fn gaining_the_focus_redraws_and_losing_it_does_not() {
        let mut w = Watch::new();
        assert!(w.on_signal(Signal::Focused(true)));
        assert!(!w.on_signal(Signal::Focused(false)));
    }

    #[test]
    fn becoming_visible_again_redraws_and_being_hidden_does_not() {
        let mut w = Watch::new();
        assert!(w.on_signal(Signal::Occluded(false)));
        assert!(!w.on_signal(Signal::Occluded(true)));
    }

    #[test]
    fn restoring_from_minimise_redraws_once() {
        let mut w = Watch::new();
        assert!(!w.on_signal(size(0, 0)), "going minimised");
        assert!(w.on_signal(size(1040, 640)), "the restore");
        assert!(!w.on_signal(size(1050, 640)), "an ordinary resize after it");
    }

    #[test]
    fn an_ordinary_resize_does_not_redraw() {
        let mut w = Watch::new();
        assert!(!w.on_signal(size(800, 600)));
        assert!(!w.on_signal(size(801, 600)));
    }

    #[test]
    fn a_width_or_height_of_zero_counts_as_minimised() {
        let mut w = Watch::new();
        assert!(!w.on_signal(size(0, 600)));
        assert!(w.on_signal(size(800, 600)));
        assert!(!w.on_signal(size(800, 0)));
        assert!(w.on_signal(size(800, 600)));
    }

    #[test]
    fn a_scale_or_theme_change_redraws() {
        let mut w = Watch::new();
        assert!(w.on_signal(Signal::ScaleFactorChanged));
        assert!(w.on_signal(Signal::ThemeChanged));
    }

    #[test]
    fn other_events_do_not_redraw() {
        let mut w = Watch::new();
        assert!(!w.on_signal(Signal::Other));
    }

    #[test]
    fn winit_events_map_to_signals() {
        use slint::winit_030::winit::{dpi::PhysicalSize, event::WindowEvent};
        assert_eq!(
            signal_of(&WindowEvent::Focused(true)),
            Signal::Focused(true)
        );
        assert_eq!(
            signal_of(&WindowEvent::Occluded(true)),
            Signal::Occluded(true)
        );
        assert_eq!(
            signal_of(&WindowEvent::Resized(PhysicalSize::new(3, 4))),
            size(3, 4)
        );
        assert_eq!(signal_of(&WindowEvent::CloseRequested), Signal::Other);
    }
}
