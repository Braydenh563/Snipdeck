//! The suggestion hint at the caret (v1.1 slice 6): a small window that says which item the
//! trigger being typed stands for and that Tab inserts it.
//!
//! The typing worker decides when a hint stands and calls [`SuggestHint::show_hint`] and
//! [`SuggestHint::hide_hint`]; this module only draws and places it. The window is a non-activating
//! popup like the toast, so focus stays in the app being typed into. With no caret there is no
//! hint: it never follows the mouse.

use std::rc::Rc;

use slint::ComponentHandle;
use snipdeck_platform::{route::ScreenRect, system_look::HighContrast};
use snipdeck_ui as ui;

use crate::{
    accent::Accent,
    frame::Rect,
    placement::{self, PopupHost},
    popup_window::{Shell, set_look_globals},
};

/// The look the hint is drawn in: what the controller's view carries for every window.
#[derive(Debug, Clone, Default)]
pub struct HintLook {
    /// Dark theme.
    pub dark: bool,
    /// The system's high-contrast colours, when on.
    pub high_contrast: Option<HighContrast>,
    /// The custom accent, when set.
    pub accent: Option<Accent>,
    /// Reduced motion.
    pub reduced_motion: bool,
}

/// The hint window, made the first time a hint is shown. The host keeps one.
pub struct SuggestHint {
    host: Rc<dyn PopupHost>,
    shell: Rc<Shell<ui::SuggestHintWindow>>,
}

impl std::fmt::Debug for SuggestHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuggestHint").finish_non_exhaustive()
    }
}

impl SuggestHint {
    /// A hint that shows through `host`.
    #[must_use]
    pub fn new(host: Rc<dyn PopupHost>) -> Self {
        let shell = Shell::new(Rc::clone(&host), false);
        Self { host, shell }
    }

    /// Shows the hint for `title` under `caret`. Returns whether it is showing: with no `caret`
    /// (or no monitor to place it on) nothing is shown and a hint that was up is hidden.
    pub fn show_hint(&self, title: &str, caret: Option<ScreenRect>, look: &HintLook) -> bool {
        let Ok((window, _)) = self.shell.ensure(ui::SuggestHintWindow::new) else {
            crate::trace!("cannot make the suggestion hint window");
            return false;
        };
        window.set_dark(look.dark);
        crate::text_scale::apply(&window.global::<ui::SystemLook<'_>>());
        set_look_globals(
            &window.global::<ui::SystemLook<'_>>(),
            look.high_contrast.as_ref(),
            look.accent.as_ref(),
            look.reduced_motion,
        );
        window.set_item_title(title.into());
        let size = (window.get_content_width(), window.get_content_height());
        let Some(frame) = placement::place_hint(&*self.host, caret, size) else {
            self.shell.hide();
            return false;
        };
        if self.shell.is_open() {
            self.shell.place(&window, Some(frame), size);
        } else if let Err(e) = self.shell.show(&window, Some(frame), size) {
            crate::trace!("cannot show the suggestion hint: {e}");
            return false;
        }
        true
    }

    /// Hides the hint.
    pub fn hide_hint(&self) {
        self.shell.hide();
    }

    /// Whether the hint is on screen.
    #[must_use]
    pub fn is_showing(&self) -> bool {
        self.shell.is_open()
    }

    /// The window, once a hint has been shown.
    #[must_use]
    pub fn window(&self) -> Option<ui::SuggestHintWindow> {
        self.shell.window()
    }

    /// Where the window was last put, physical pixels.
    #[must_use]
    pub fn placed(&self) -> Option<Rect> {
        self.shell.frame()
    }
}
