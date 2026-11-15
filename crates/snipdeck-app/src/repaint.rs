//! When a window must be drawn again from scratch (O39, render flicker 2026-11-14).
//!
//! Slint's software renderer presents only what changed since its last frame. Windows can throw
//! away or overwrite a window's pixels without the scene changing: Alt+Tab back, restore from
//! minimise, a display or DPI change, a resize (the class has `CS_HREDRAW | CS_VREDRAW`, so
//! Windows erases the whole client area with the class brush), a part dragged back from off
//! screen, a window shown again whose first frame was drawn before it was mapped. The frame that
//! follows then covers only what Slint thinks changed, and the rest stays as Windows left it
//! (`docs/reviews/render-flicker-2026-11-14.md`).
//!
//! The window-event hooks translate each winit event to a [`Signal`] and ask
//! [`Watch::on_signal`] what to do: nothing, ask for a frame, or damage the whole window before
//! the frame that is about to be drawn. The decision is pure so it is tested on every platform.

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
    /// A frame is about to be drawn (`RedrawRequested`). `os_update` says whether Windows holds
    /// an area of the window it invalidated itself (`GetUpdateRect`): pixels it dropped or will
    /// erase, which the frame must cover although nothing in the scene changed.
    Redraw {
        /// Windows invalidated part of the window (always `false` off Windows).
        os_update: bool,
    },
    /// Anything else: moves, pointer, keys, close.
    Other,
}

/// What the hook does for a [`Signal`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing.
    None,
    /// Ask for a frame; it will be drawn whole (the watch owes one).
    Request,
    /// Damage the whole window now: the frame being drawn covers all of it.
    DamageWhole,
}

/// Remembers whether the window was minimised (a restore is told from a resize) and whether the
/// next frame must be drawn whole.
#[derive(Clone, Copy, Debug, Default)]
pub struct Watch {
    minimised: bool,
    owed: bool,
}

impl Watch {
    /// A watch for a window that is not minimised and owes nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            minimised: false,
            owed: false,
        }
    }

    /// Whether the next frame will be drawn whole.
    #[must_use]
    pub const fn owes_whole_frame(&self) -> bool {
        self.owed
    }

    /// What to do after `signal`.
    ///
    /// A change Windows may have drawn over (the focus gained, visible again, restored, any real
    /// resize, a scale or theme change) owes a whole frame and asks for it. The damage is laid
    /// at the frame itself, not at the event: a `Resized` reaches this hook before Slint knows
    /// the new size, so damage laid then would cover the old size only. A frame is drawn whole
    /// when one is owed or when Windows invalidated part of the window; it pays what is owed.
    /// Losing the focus, being hidden, going minimised and moves change nothing.
    pub fn on_signal(&mut self, signal: Signal) -> Action {
        let owe = match signal {
            Signal::Focused(gained) => gained,
            Signal::Occluded(hidden) => !hidden,
            Signal::Resized { width, height } => {
                let empty = width == 0 || height == 0;
                self.minimised = empty;
                !empty
            }
            Signal::ScaleFactorChanged | Signal::ThemeChanged => true,
            Signal::Redraw { os_update } => {
                let whole = os_update || self.owed;
                self.owed = false;
                return if whole {
                    Action::DamageWhole
                } else {
                    Action::None
                };
            }
            Signal::Other => false,
        };
        if owe {
            self.owed = true;
            Action::Request
        } else {
            Action::None
        }
    }
}

/// The signal for a winit window event; `os_update` is asked only for a redraw.
#[must_use]
pub fn signal_of(
    event: &slint::winit_030::winit::event::WindowEvent,
    os_update: impl FnOnce() -> bool,
) -> Signal {
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
        WindowEvent::RedrawRequested => Signal::Redraw {
            os_update: os_update(),
        },
        _ => Signal::Other,
    }
}

/// Feeds one winit event of `window` to `watch` and acts on it. Every Snipdeck window's event
/// hook calls this, before Slint handles the event: on `RedrawRequested` the whole-window
/// damage is in place for the frame Slint draws next. It only marks there and asks for no frame
/// (Slint always draws on `RedrawRequested`; asking again would draw one more, empty frame).
pub fn follow(
    watch: &mut Watch,
    window: &slint::Window,
    event: &slint::winit_030::winit::event::WindowEvent,
) {
    let signal = signal_of(event, || os_update(window));
    match watch.on_signal(signal) {
        Action::None => {}
        Action::Request => window.request_redraw(),
        Action::DamageWhole => snipdeck_ui::damage::mark_whole(window),
    }
}

/// Gives `window` a hook that only [`follow`]s its events: for the popup windows (the Picker, a
/// toast, the suggestion hint), which have no hook of their own. A window has one hook, so a
/// window that has its own calls [`follow`] from it instead.
pub fn watch(window: &slint::Window) {
    use slint::winit_030::WinitWindowAccessor;
    let mut watch = Watch::new();
    window.on_winit_window_event(move |slint_window, event| {
        follow(&mut watch, slint_window, event);
        slint::winit_030::EventResult::Propagate
    });
}

/// Whether Windows holds an invalid area of `window` that it made itself (read in the middle of
/// `WM_PAINT`, before `BeginPaint` empties it).
fn os_update(window: &slint::Window) -> bool {
    #[cfg(windows)]
    {
        crate::native_window::handle_of(window)
            .is_some_and(snipdeck_platform::windows::update_region::has_update_region)
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn size(width: u32, height: u32) -> Signal {
        Signal::Resized { width, height }
    }

    const PLAIN: Signal = Signal::Redraw { os_update: false };
    const OS: Signal = Signal::Redraw { os_update: true };

    #[test]
    fn gaining_the_focus_owes_a_whole_frame_and_losing_it_does_not() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(Signal::Focused(false)), Action::None);
        assert_eq!(w.on_signal(PLAIN), Action::None);
        assert_eq!(w.on_signal(Signal::Focused(true)), Action::Request);
        assert_eq!(w.on_signal(PLAIN), Action::DamageWhole);
    }

    #[test]
    fn becoming_visible_again_owes_a_whole_frame_and_being_hidden_does_not() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(Signal::Occluded(true)), Action::None);
        assert!(!w.owes_whole_frame());
        assert_eq!(w.on_signal(Signal::Occluded(false)), Action::Request);
        assert!(w.owes_whole_frame());
    }

    #[test]
    fn going_minimised_owes_nothing_and_the_restore_owes_a_whole_frame() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(size(0, 0)), Action::None, "going minimised");
        assert_eq!(w.on_signal(size(1040, 640)), Action::Request, "the restore");
        assert_eq!(w.on_signal(PLAIN), Action::DamageWhole);
    }

    #[test]
    fn every_real_resize_owes_a_whole_frame_at_the_new_size() {
        // A dock, an undock, the dock preview and a fit all resize; Windows erases the whole
        // client area then (CS_HREDRAW | CS_VREDRAW), so the frame after must cover all of it.
        let mut w = Watch::new();
        for (width, height) in [(320, 480), (280, 1040), (6, 480), (320, 481)] {
            assert_eq!(w.on_signal(size(width, height)), Action::Request);
            assert_eq!(w.on_signal(PLAIN), Action::DamageWhole, "{width}x{height}");
        }
    }

    #[test]
    fn a_width_or_height_of_zero_counts_as_minimised() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(size(0, 600)), Action::None);
        assert_eq!(w.on_signal(size(800, 0)), Action::None);
        assert_eq!(w.on_signal(PLAIN), Action::None);
        assert_eq!(w.on_signal(size(800, 600)), Action::Request);
    }

    #[test]
    fn a_frame_after_windows_invalidated_an_area_is_drawn_whole() {
        // The Panel shown again after the item editor saved: its first frame was drawn before it
        // was mapped, and Windows then asks for the whole window with nothing dirty in Slint.
        let mut w = Watch::new();
        assert_eq!(w.on_signal(OS), Action::DamageWhole);
        assert_eq!(
            w.on_signal(OS),
            Action::DamageWhole,
            "every time it happens"
        );
    }

    #[test]
    fn an_owed_frame_is_paid_once_and_later_frames_stay_partial() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(Signal::ScaleFactorChanged), Action::Request);
        assert_eq!(w.on_signal(Signal::ThemeChanged), Action::Request);
        assert_eq!(w.on_signal(PLAIN), Action::DamageWhole);
        assert!(!w.owes_whole_frame());
        // Hover and typing redraw only what changed: full frames there would cost every frame.
        for _ in 0..3 {
            assert_eq!(w.on_signal(PLAIN), Action::None);
        }
    }

    #[test]
    fn an_os_invalidated_frame_also_pays_what_is_owed() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(size(300, 400)), Action::Request);
        assert_eq!(w.on_signal(OS), Action::DamageWhole);
        assert_eq!(w.on_signal(PLAIN), Action::None);
    }

    #[test]
    fn moves_and_other_events_owe_nothing() {
        let mut w = Watch::new();
        assert_eq!(w.on_signal(Signal::Other), Action::None);
        assert_eq!(w.on_signal(PLAIN), Action::None);
    }

    #[test]
    fn winit_events_map_to_signals() {
        use slint::winit_030::winit::{
            dpi::{PhysicalPosition, PhysicalSize},
            event::WindowEvent,
        };
        let never = || -> bool { panic!("asked only for a redraw") };
        assert_eq!(
            signal_of(&WindowEvent::Focused(true), never),
            Signal::Focused(true)
        );
        assert_eq!(
            signal_of(&WindowEvent::Occluded(true), never),
            Signal::Occluded(true)
        );
        assert_eq!(
            signal_of(&WindowEvent::Resized(PhysicalSize::new(3, 4)), never),
            size(3, 4)
        );
        assert_eq!(
            signal_of(&WindowEvent::Moved(PhysicalPosition::new(5, 6)), never),
            Signal::Other
        );
        assert_eq!(
            signal_of(&WindowEvent::CloseRequested, never),
            Signal::Other
        );
        assert_eq!(signal_of(&WindowEvent::RedrawRequested, || true), OS);
        assert_eq!(signal_of(&WindowEvent::RedrawRequested, || false), PLAIN);
    }
}
