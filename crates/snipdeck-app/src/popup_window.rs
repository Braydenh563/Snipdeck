//! What the Picker, the fill form and the toast window share: a window made the first time it
//! is needed, placed, shown without being activated like the Panel, registered with the Target
//! tracker so it is never a paste Target, and hidden again.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use slint::{ComponentHandle, PhysicalPosition, PhysicalSize};
use snipdeck_platform::system_look::{HighContrast, Rgb};
use snipdeck_ui as ui;

use crate::{accent::Accent, frame::Rect, placement::PopupHost};

/// One popup window and its state. Not generic over the controller: the bindings own that.
pub(crate) struct Shell<W: ComponentHandle + 'static> {
    host: Rc<dyn PopupHost>,
    window: RefCell<Option<W>>,
    /// The native window has been registered and styled.
    attached: Cell<bool>,
    open: Cell<bool>,
    /// The frame the window was last given (none when the machine could not say where).
    frame: Cell<Option<Rect>>,
    /// The popup takes the keyboard when it opens (the Picker and the fill form are typed
    /// into; the toast window never is).
    keyboard: bool,
}

/// How long a popup waits before asking for the keyboard again after Windows refused.
const ACTIVATE_RETRY: Duration = Duration::from_millis(80);

impl<W: ComponentHandle + 'static> Shell<W> {
    pub(crate) fn new(host: Rc<dyn PopupHost>, keyboard: bool) -> Rc<Self> {
        Rc::new(Self {
            host,
            window: RefCell::new(None),
            attached: Cell::new(false),
            open: Cell::new(false),
            frame: Cell::new(None),
            keyboard,
        })
    }

    /// Whether the window is on screen.
    pub(crate) fn is_open(&self) -> bool {
        self.open.get()
    }

    /// The frame the window was last placed at, physical pixels.
    pub(crate) fn frame(&self) -> Option<Rect> {
        self.frame.get()
    }

    /// The window, once it has been made.
    pub(crate) fn window(&self) -> Option<W> {
        self.window.borrow().as_ref().map(W::clone_strong)
    }

    /// The window, made with `make` the first time. The flag says it was made just now.
    pub(crate) fn ensure(
        &self,
        make: impl FnOnce() -> Result<W, slint::PlatformError>,
    ) -> Result<(W, bool), slint::PlatformError> {
        if let Some(window) = self.window() {
            return Ok((window, false));
        }
        let window = make()?;
        *self.window.borrow_mut() = Some(window.clone_strong());
        Ok((window, true))
    }

    /// Puts the window at `frame` (when the machine could say where) and shows it in front of
    /// the others without activating it; then, once the native window exists, registers it and
    /// (for a popup that is typed into) takes the keyboard.
    pub(crate) fn show(
        self: &Rc<Self>,
        window: &W,
        frame: Option<Rect>,
        logical: (f32, f32),
    ) -> Result<(), slint::PlatformError> {
        self.place(window, frame, logical);
        if !self.open.get() {
            window.show()?;
        }
        self.open.set(true);
        crate::native_window::raise_window(window.window());
        let this = Rc::downgrade(self);
        // The native window only exists once the event loop has run: the Panel does the same.
        slint::Timer::single_shot(Duration::ZERO, move || {
            let Some(this) = this.upgrade() else {
                return;
            };
            let Some(window) = this.window() else {
                return;
            };
            if !this.attached.replace(true) {
                this.host.attach(window.window());
            }
            if this.open.get() && this.keyboard {
                let taken = this.host.activate(window.window());
                crate::trace!("popup takes the keyboard: {taken}");
                if !taken {
                    // Windows refused (a foreground lock still running): try again shortly, so
                    // Esc reaches the popup instead of leaving it stuck on screen.
                    let this = Rc::downgrade(&this);
                    slint::Timer::single_shot(ACTIVATE_RETRY, move || {
                        if let Some(this) = this.upgrade()
                            && this.open.get()
                            && let Some(window) = this.window()
                        {
                            let taken = this.host.activate(window.window());
                            crate::trace!("popup takes the keyboard (retry): {taken}");
                        }
                    });
                }
            }
        });
        Ok(())
    }

    /// Moves and sizes the window to `frame`. With no frame (the machine could not say where)
    /// the window keeps its place and takes the `logical` size.
    pub(crate) fn place(&self, window: &W, frame: Option<Rect>, logical: (f32, f32)) {
        self.frame.set(frame);
        let w = window.window();
        if let Some(f) = frame {
            w.set_size(PhysicalSize::new(f.w, f.h));
            w.set_position(PhysicalPosition::new(f.x, f.y));
        } else {
            w.set_size(slint::LogicalSize::new(logical.0, logical.1));
        }
    }

    /// Starts the OS drag of the window (pointer down on its header). The popups have no OS
    /// caption; under the test backend there is no OS window and nothing happens.
    pub(crate) fn drag(&self) {
        use slint::winit_030::WinitWindowAccessor;
        if let Some(window) = self.window() {
            let _ = window
                .window()
                .with_winit_window(slint::winit_030::winit::window::Window::drag_window);
        }
    }

    /// Hides the window.
    pub(crate) fn hide(&self) {
        if self.open.replace(false)
            && let Some(window) = self.window()
        {
            let _ = window.hide();
        }
    }
}

/// Sets the high-contrast colours, the accent and reduced motion of a window's `SystemLook` global (a
/// global is per window).
pub(crate) fn set_look_globals(
    look: &ui::SystemLook<'_>,
    hc: Option<&HighContrast>,
    accent: Option<&Accent>,
    reduced_motion: bool,
) {
    look.set_reduced_motion(reduced_motion);
    crate::text_scale::apply(look);
    look.set_high_contrast(hc.is_some());
    if let Some(hc) = hc {
        let color = |c: Rgb| slint::Color::from_rgb_u8(c.r, c.g, c.b);
        look.set_hc_window(color(hc.window));
        look.set_hc_window_text(color(hc.window_text));
        look.set_hc_highlight(color(hc.highlight));
        look.set_hc_highlight_text(color(hc.highlight_text));
        look.set_hc_gray_text(color(hc.gray_text));
    }
    look.set_accent_custom(accent.is_some());
    if let Some(accent) = accent {
        let steps = |s: &crate::accent::Steps| {
            let color = |[r, g, b]: crate::accent::Rgb| slint::Color::from_rgb_u8(r, g, b);
            ui::AccentSteps {
                default: color(s.default),
                hover: color(s.hover),
                pressed: color(s.pressed),
                text: color(s.text),
                subtle: color(s.subtle),
                focus: color(s.focus),
            }
        };
        look.set_accent_light(steps(&accent.light));
        look.set_accent_dark(steps(&accent.dark));
    }
}
