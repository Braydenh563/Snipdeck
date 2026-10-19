//! Shows the toast while the Panel is hidden. The Panel and the Main window draw the toast
//! inside themselves; but a paste from the Picker, the capture hotkey and "paste next" all end
//! with the Panel out of sight, and their "Pasted into Outlook" or "Couldn't reach the
//! clipboard" would never be seen. This follows the controller's toast and shows it in a small
//! window at the bottom of the screen, only while nothing else of Snipdeck shows toasts.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{ComponentHandle, SharedString};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};
use snipdeck_ui as ui;

use crate::{
    binding::{redraw_all, register_redraw},
    controller::{Controller, Library, ToastPhase, ToastView, View},
    frame::Rect,
    host::ClipboardText,
    placement::{self, PopupHost},
    popup_window::{Shell, set_high_contrast},
    toast::ToastKind,
    wording,
};

/// Whether another window of Snipdeck is showing toasts (the Main window or Settings is open).
pub type OthersShowing = Rc<dyn Fn() -> bool>;

#[derive(Clone)]
struct Shown {
    dark: bool,
    high_contrast: Option<HighContrast>,
    toast: ToastView,
}

struct ToastCore<S, C, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    host: Rc<dyn PopupHost>,
    others: OthersShowing,
    shell: Rc<Shell<ui::ToastWindow>>,
    shown: RefCell<Option<Shown>>,
    size: Cell<(f32, f32)>,
    redraw: Rc<dyn Fn()>,
}

impl<S, C, K, T> ToastCore<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    fn sync(self: &Rc<Self>) {
        let (view, panel_visible) = {
            let ctl = self.controller.borrow();
            (ctl.view(), ctl.visible())
        };
        let wanted = view
            .toast
            .as_ref()
            .filter(|_| !panel_visible && !(self.others)());
        let Some(toast) = wanted else {
            self.shell.hide();
            *self.shown.borrow_mut() = None;
            return;
        };
        let Ok((window, created)) = self.shell.ensure(ui::ToastWindow::new) else {
            // There is nowhere else to say so: the failure goes to the diagnostic log.
            crate::trace!("cannot make the toast window");
            return;
        };
        if created {
            let weak: Weak<Self> = Rc::downgrade(self);
            window.on_undo(move || {
                if let Some(core) = weak.upgrade() {
                    core.controller.borrow_mut().main_undo();
                    redraw_all();
                }
            });
        }
        render(&window, self.shown.borrow().as_ref(), &view, toast);
        *self.shown.borrow_mut() = Some(Shown {
            dark: view.dark,
            high_contrast: view.high_contrast,
            toast: toast.clone(),
        });
        let size = (window.get_content_width(), window.get_content_height());
        if !self.shell.is_open() || size != self.size.get() {
            let frame = placement::place_toast(&*self.host, size);
            if self.shell.is_open() {
                self.shell.place(&window, frame, size);
            } else if let Err(e) = self.shell.show(&window, frame, size) {
                crate::trace!("cannot show the toast window: {e}");
                return;
            }
            self.size.set(size);
        }
    }
}

/// The toast window, made the first time a toast has to be shown with the Panel hidden. The
/// host keeps one of these.
pub struct ToastUi {
    /// Keeps the binding alive.
    _core: Rc<dyn std::any::Any>,
    window: Box<dyn Fn() -> Option<ui::ToastWindow>>,
    placed: Box<dyn Fn() -> Option<Rect>>,
}

impl std::fmt::Debug for ToastUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToastUi").finish_non_exhaustive()
    }
}

impl ToastUi {
    /// Follows `controller`'s toast. `others` says whether the Main window or Settings is
    /// showing, and so drawing the toast itself.
    pub fn new<S, C, K, T>(
        controller: &Rc<RefCell<Controller<S, C, K, T>>>,
        host: Rc<dyn PopupHost>,
        others: OthersShowing,
    ) -> Self
    where
        S: Library + 'static,
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        let shell = Shell::new(Rc::clone(&host), false);
        let core = Rc::new_cyclic(|weak: &Weak<ToastCore<S, C, K, T>>| {
            let weak = weak.clone();
            ToastCore {
                controller: Rc::clone(controller),
                host,
                others,
                shell: Rc::clone(&shell),
                shown: RefCell::new(None),
                size: Cell::new((0.0, 0.0)),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
            }
        });
        register_redraw(&core.redraw);
        Self {
            _core: core,
            window: Box::new({
                let shell = Rc::clone(&shell);
                move || shell.window()
            }),
            placed: Box::new(move || shell.frame()),
        }
    }

    /// The window, once a toast has been shown with the Panel hidden.
    #[must_use]
    pub fn window(&self) -> Option<ui::ToastWindow> {
        (self.window)()
    }

    /// Where the window was last put, in physical pixels (none until it has been shown, or if
    /// the machine could not say where).
    #[must_use]
    pub fn placed(&self) -> Option<Rect> {
        (self.placed)()
    }
}

fn render(window: &ui::ToastWindow, previous: Option<&Shown>, view: &View, toast: &ToastView) {
    let differs = |same: &dyn Fn(&Shown) -> bool| previous.is_none_or(|p| !same(p));
    if differs(&|p| p.dark == view.dark) {
        window.set_dark(view.dark);
    }
    if differs(&|p| p.high_contrast == view.high_contrast) {
        set_high_contrast(
            &window.global::<ui::SystemLook<'_>>(),
            view.high_contrast.as_ref(),
        );
    }
    if differs(&|p| p.toast == *toast) {
        window.set_undo_label(if toast.undo {
            wording::UNDO_LABEL.into()
        } else {
            SharedString::new()
        });
        window.set_kind(match toast.kind {
            ToastKind::Neutral => ui::ToastKind::Neutral,
            ToastKind::Success => ui::ToastKind::Success,
            ToastKind::Danger => ui::ToastKind::Danger,
        });
        window.set_phase(match toast.phase {
            ToastPhase::Enter => ui::ToastPhase::Enter,
            ToastPhase::Hold => ui::ToastPhase::Hold,
            ToastPhase::Exit => ui::ToastPhase::Exit,
        });
        window.set_text(wording::toast_text(&toast.message).into());
    }
}
