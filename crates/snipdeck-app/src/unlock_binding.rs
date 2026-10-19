//! Binds the unlock window (the master password prompt, SPEC 5.7) to the [`Controller`].
//!
//! The controller decides when the prompt is wanted ([`Controller::request_unlock`], which
//! queues [`Effect::OpenUnlock`](crate::controller::Effect::OpenUnlock)); the host opens the
//! window through [`UnlockUi::open`]. The window reports the typed password once, through
//! `submitted`: it goes straight to [`Controller::begin_unlock_with_password`] and the field is
//! emptied whatever the answer, so the password is not kept in the window between tries. The key
//! derivation then runs where [`crate::master_job::offload`] says, and the window shows it is
//! waiting until [`Controller::finish_master`] has the outcome.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use slint::ComponentHandle;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::Store;
use snipdeck_ui as ui;

use crate::{
    binding::{redraw_all, register_redraw},
    controller::{Controller, MasterProblem},
    host::ClipboardText,
    main_binding::{create_window, raise, set_look},
    native_window::fit_to_screen,
};

/// The Slint word for `problem`.
#[must_use]
pub fn slint_problem(problem: MasterProblem) -> ui::MasterProblem {
    match problem {
        MasterProblem::CurrentEmpty => ui::MasterProblem::CurrentEmpty,
        MasterProblem::WrongPassword => ui::MasterProblem::WrongPassword,
        MasterProblem::TooShort => ui::MasterProblem::TooShort,
        MasterProblem::Mismatch => ui::MasterProblem::Mismatch,
        MasterProblem::SupporterOnly => ui::MasterProblem::SupporterOnly,
        MasterProblem::Unavailable => ui::MasterProblem::Unavailable,
        MasterProblem::Failed => ui::MasterProblem::Failed,
    }
}

struct UnlockCore<C, K, T> {
    controller: Rc<RefCell<Controller<Store, C, K, T>>>,
    window: slint::Weak<ui::UnlockWindow>,
    /// Closes the window if the controller no longer wants the prompt (registered with
    /// `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
}

/// A bound unlock window. Dropping it does not unbind the window; its callbacks keep the binding
/// alive.
pub struct UnlockBinding<C, K, T> {
    core: Rc<UnlockCore<C, K, T>>,
}

impl<C, K, T> std::fmt::Debug for UnlockBinding<C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlockBinding").finish_non_exhaustive()
    }
}

impl<C, K, T> UnlockBinding<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Closes the window if the controller no longer wants the prompt (the library was opened
    /// another way, or the password removed). Every window's redraw does this too.
    pub fn redraw(&self) {
        self.core.close_if_not_wanted();
    }

    /// Gets the window ready to be shown: an empty field and no error, the look of the app, and
    /// the keyboard on the password field.
    pub fn prepare(&self) {
        let Some(window) = self.core.window.upgrade() else {
            return;
        };
        let controller = self.core.controller.borrow();
        let look = controller.settings_look();
        window.set_dark(look.dark);
        set_look(
            &window,
            look.high_contrast.as_ref(),
            controller.accent().as_ref(),
            look.reduce_motion,
        );
        drop(controller);
        window.set_password(slint::SharedString::new());
        window.set_problem(ui::MasterProblem::None);
        window.set_focus_request(window.get_focus_request() + 1);
    }
}

impl<C, K, T> Controller<Store, C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Wires the unlock window to `this`.
    pub fn bind_unlock(
        this: &Rc<RefCell<Self>>,
        window: &ui::UnlockWindow,
    ) -> UnlockBinding<C, K, T> {
        let core = Rc::new_cyclic(|weak: &Weak<UnlockCore<C, K, T>>| {
            let weak = weak.clone();
            UnlockCore {
                controller: Rc::clone(this),
                window: window.as_weak(),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.close_if_not_wanted();
                    }
                }),
            }
        });
        register_redraw(&core.redraw);
        {
            let core = Rc::clone(&core);
            window.on_submitted(move |password| core.submitted(&password));
        }
        {
            let core = Rc::clone(&core);
            window.on_cancelled(move || core.cancelled());
        }
        {
            // The title bar's close button is a cancel, so the controller knows the prompt is gone.
            let core = Rc::clone(&core);
            window.window().on_close_requested(move || {
                core.controller.borrow_mut().cancel_unlock();
                slint::CloseRequestResponse::HideWindow
            });
        }
        UnlockBinding { core }
    }
}

impl<C, K, T> UnlockCore<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    fn submitted(self: &Rc<Self>, password: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        // Cleared first and whatever the answer: the window never holds a password between tries.
        window.set_password(slint::SharedString::new());
        // Cleared before the answer is set, so a repeated error is announced again.
        window.set_problem(ui::MasterProblem::None);
        let begun = self
            .controller
            .borrow_mut()
            .begin_unlock_with_password(password);
        match begun {
            Ok(Some(job)) => {
                // The key derivation takes a moment on purpose: the window waits, on another
                // thread in the app, and `finished` carries on from here.
                window.set_busy(true);
                let core = Rc::clone(self);
                crate::master_job::offload().run(job, Box::new(move |done| core.finished(done)));
            }
            Ok(None) => Self::opened(&window),
            Err(problem) => Self::refused(&window, problem),
        }
    }

    /// The outcome of the key derivation `submitted` started.
    fn finished(self: &Rc<Self>, done: crate::master_job::MasterDone) {
        let (current, idle) = {
            let ctl = self.controller.borrow();
            (ctl.master_current(&done), ctl.master_busy().is_none())
        };
        if !current {
            // The window was closed or the library locked again while the password was being
            // checked: the answer is dropped (with its key) and the window only stops waiting.
            drop(done);
            if let Some(window) = self.window.upgrade()
                && idle
            {
                window.set_busy(false);
            }
            return;
        }
        let result = self.controller.borrow_mut().finish_master(done);
        let Some(window) = self.window.upgrade() else {
            redraw_all();
            return;
        };
        window.set_busy(false);
        match result {
            Ok(()) => Self::opened(&window),
            Err(problem) => Self::refused(&window, problem),
        }
    }

    fn opened(window: &ui::UnlockWindow) {
        let _ = window.hide();
        redraw_all();
    }

    fn refused(window: &ui::UnlockWindow, problem: MasterProblem) {
        window.set_problem(slint_problem(problem));
        window.set_focus_request(window.get_focus_request() + 1);
    }

    fn cancelled(self: &Rc<Self>) {
        self.controller.borrow_mut().cancel_unlock();
        if let Some(window) = self.window.upgrade() {
            window.set_password(slint::SharedString::new());
            let _ = window.hide();
        }
    }

    fn close_if_not_wanted(&self) {
        if let Some(window) = self.window.upgrade()
            && window.window().is_visible()
            && !self.controller.borrow().unlock_prompt_open()
        {
            window.set_password(slint::SharedString::new());
            let _ = window.hide();
        }
    }
}

thread_local! {
    /// Opens the unlock window; set by [`UnlockUi::new`].
    static OPENER: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Opens the unlock window (creating it the first time), if the host made one with
/// [`UnlockUi::new`]. UI thread only.
pub fn request_open() {
    let opener = OPENER.with(|o| o.borrow().clone());
    if let Some(opener) = opener {
        opener();
    }
}

/// Creates the unlock window the first time it is asked for, binds it, and shows and raises it
/// every time. Not generic: the host keeps one of these.
pub struct UnlockUi {
    open: Box<dyn Fn() -> Result<(), slint::PlatformError>>,
}

impl std::fmt::Debug for UnlockUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlockUi").finish_non_exhaustive()
    }
}

impl UnlockUi {
    /// An unlock window for `controller`, not created yet.
    pub fn new<C, K, T>(controller: &Rc<RefCell<Controller<Store, C, K, T>>>) -> Self
    where
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        type Slot<C, K, T> = Rc<RefCell<Option<(ui::UnlockWindow, UnlockBinding<C, K, T>)>>>;
        let slot: Slot<C, K, T> = Rc::default();
        let controller = Rc::clone(controller);
        let open: Rc<dyn Fn() -> Result<(), slint::PlatformError>> = Rc::new(move || {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                let window = create_window(ui::UnlockWindow::new)?;
                crate::native_window::size_before_show(
                    window.window(),
                    (window.get_wanted_width(), window.get_wanted_height()),
                );
                let binding = Controller::bind_unlock(&controller, &window);
                *slot = Some((window, binding));
            }
            let Some((window, binding)) = slot.as_ref() else {
                return Ok(());
            };
            binding.prepare();
            crate::native_window::place_over_owner(window.window());
            window.show()?;
            fit_to_screen(window.window());
            raise(window.window());
            Ok(())
        });
        OPENER.with(|o| {
            let open = Rc::clone(&open);
            *o.borrow_mut() = Some(Rc::new(move || {
                if let Err(e) = open() {
                    crate::trace!("cannot open the unlock window: {e}");
                }
            }));
        });
        Self {
            open: Box::new(move || open()),
        }
    }

    /// Shows the window, creating it the first time.
    ///
    /// # Errors
    ///
    /// The window could not be created or shown.
    pub fn open(&self) -> Result<(), slint::PlatformError> {
        (self.open)()
    }
}
