//! Binds the welcome tour window to the [`Controller`]: draws the step the controller is on and
//! turns the window's callbacks into Back, Next and Skip (the controller's `tour_*` methods) or
//! into the same changes Settings makes (a recorded shortcut, the dock edge, the taskbar and
//! tray switches, Start with Windows). Those go through [`SettingsHost`] like Settings, so tests
//! run them against fakes, and share Settings' logic: [`with_hotkey`], `try_register` and the
//! dock-edge mapping.
//!
//! Like Settings, the window is made the first time it is asked for and then reused.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{ComponentHandle, SharedString};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::Store;
use snipdeck_ui as ui;

use crate::{
    binding::{Loggable, Shown, model, redraw_all, register_redraw, to_i32},
    controller::Controller,
    host::ClipboardText,
    main_binding::{create_window, raise, set_look},
    native_window::fit_to_screen,
    settings::{HotkeyAction, Hotkeys},
    settings_binding::{
        HotkeyRefusal, SettingsHost, arm_settings_save, dock_edge_index, dock_edge_of,
        hotkey_refusal_text, key_chips, try_register, with_hotkey, without_hotkey,
    },
    toast::ToastMessage,
    tour::TourStep,
    wording,
};

/// The text of the shortcut `action` has in `hotkeys` ("Ctrl+Alt+V"; empty when off).
fn chord_text(hotkeys: &Hotkeys, action: HotkeyAction) -> &str {
    match action {
        HotkeyAction::TogglePanel => &hotkeys.toggle_panel,
        HotkeyAction::Picker => &hotkeys.picker,
        HotkeyAction::Capture => &hotkeys.capture,
        HotkeyAction::PasteNext => &hotkeys.paste_next,
    }
}

/// One shortcut as the window shows it; compared with the last one drawn so a redraw does not
/// reset a recorder that is waiting for keys.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyRow {
    keys: Vec<String>,
    value: String,
    conflict: String,
}

struct TourCore<C, K, T> {
    controller: Rc<RefCell<Controller<Store, C, K, T>>>,
    window: slint::Weak<ui::OnboardingWindow>,
    host: Rc<dyn SettingsHost>,
    /// Why each shortcut (in [`HotkeyAction::ALL`] order) was refused; empty when it was not.
    conflicts: RefCell<Vec<String>>,
    /// The Start with Windows entry could not be changed.
    autostart_failed: Cell<bool>,
    /// The shortcuts the window shows now.
    shown_hotkeys: RefCell<Option<Vec<HotkeyRow>>>,
    /// The window has been drawn at least once.
    drawn: Cell<bool>,
    /// Redraws this window (registered with `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
    /// A timer is waiting to write changed settings.
    save_timer: Rc<Cell<bool>>,
}

/// A bound welcome tour window. Dropping it does not unbind the window; the window's callbacks
/// keep the binding alive.
pub struct OnboardingBinding<C, K, T> {
    core: Rc<TourCore<C, K, T>>,
}

impl<C, K, T> std::fmt::Debug for OnboardingBinding<C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnboardingBinding").finish_non_exhaustive()
    }
}

impl<C, K, T> OnboardingBinding<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Redraws the window from the controller.
    pub fn redraw(&self) {
        self.core.sync();
    }
}

impl<C, K, T> TourCore<C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Runs `f` on the controller, then redraws every window.
    fn act<R>(self: &Rc<Self>, f: impl FnOnce(&mut Controller<Store, C, K, T>) -> R) -> R {
        let result = f(&mut self.controller.borrow_mut());
        redraw_all();
        result
    }

    /// Copies the controller's state into the window.
    fn sync(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        // A closed window is drawn again when it opens.
        if self.drawn.get() && !window.window().is_visible() {
            arm_settings_save(&self.controller, &self.save_timer);
            return;
        }
        self.drawn.set(true);
        let controller = self.controller.borrow();
        let settings = controller.settings();
        let look = controller.settings_look();
        window.set_dark(look.dark);
        set_look(
            &window,
            look.high_contrast.as_ref(),
            controller.accent().as_ref(),
            look.reduce_motion,
        );
        let step = controller.tour_step().unwrap_or_default();
        window.set_step(to_i32(step.index()));
        window.set_step_count(to_i32(TourStep::ALL.len()));
        let rows: Vec<HotkeyRow> = {
            let conflicts = self.conflicts.borrow();
            HotkeyAction::ALL
                .into_iter()
                .enumerate()
                .map(|(i, action)| {
                    let value = chord_text(&settings.hotkeys, action);
                    HotkeyRow {
                        keys: key_chips(value),
                        value: value.to_owned(),
                        conflict: conflicts.get(i).cloned().unwrap_or_default(),
                    }
                })
                .collect()
        };
        if self.shown_hotkeys.borrow().as_ref() != Some(&rows) {
            window.set_hotkeys(model(rows.iter().map(|h| ui::HotkeySetting {
                keys: model(h.keys.iter().map(|k| SharedString::from(k.as_str()))),
                value: h.value.as_str().into(),
                conflict: h.conflict.as_str().into(),
            })));
            *self.shown_hotkeys.borrow_mut() = Some(rows);
        }
        window.set_autostart(settings.autostart);
        window.set_close_to_tray(settings.close_to_tray);
        window.set_recent_history(settings.recent_history);
        window.set_recent_max_entries(crate::recent::max_entries_count());
        window.set_recent_max_days(crate::recent::max_age_days());
        window.set_panel_in_taskbar(settings.panel_in_taskbar);
        window.set_panel_hide_on_blur(settings.panel_hide_on_blur);
        window.set_dock_edge_index(dock_edge_index(controller.dock_edge()));
        window.set_notice(if self.autostart_failed.get() {
            wording::toast_text(&window, &ToastMessage::AutostartFailed).into()
        } else {
            SharedString::new()
        });
        drop(controller);
        arm_settings_save(&self.controller, &self.save_timer);
    }

    // ---- Shortcuts (the Settings logic) ----------------------------------------------------

    /// Shows why `combo` was refused for the action at `index`, in the window's language.
    fn refuse_hotkey(&self, index: usize, refusal: HotkeyRefusal, combo: &str) {
        if let Some(window) = self.window.upgrade() {
            self.set_conflict(index, hotkey_refusal_text(&window, refusal, combo));
        }
    }

    fn set_conflict(&self, index: usize, text: String) {
        if let Some(slot) = self.conflicts.borrow_mut().get_mut(index) {
            *slot = text;
        }
    }

    fn hotkey_recorded(self: &Rc<Self>, index: usize, combo: &str) {
        let Some(action) = HotkeyAction::ALL.get(index).copied() else {
            return;
        };
        let current = self.controller.borrow().settings().hotkeys.clone();
        let next = match with_hotkey(&current, action, combo) {
            Ok(next) => next,
            Err(refusal) => {
                self.refuse_hotkey(index, refusal, combo);
                redraw_all();
                return;
            }
        };
        let paused = self.controller.borrow().hotkeys_paused();
        if let Some(failure) = try_register(self.host.as_ref(), paused, action, &current, &next) {
            self.refuse_hotkey(index, HotkeyRefusal::Refused(failure), combo);
            redraw_all();
            return;
        }
        self.set_conflict(index, String::new());
        self.act(|ctl| ctl.update_settings(|s| s.hotkeys = next));
    }

    fn hotkey_cleared(self: &Rc<Self>, index: usize) {
        let Some(action) = HotkeyAction::ALL.get(index).copied() else {
            return;
        };
        let current = self.controller.borrow().settings().hotkeys.clone();
        let next = without_hotkey(&current, action);
        let paused = self.controller.borrow().hotkeys_paused();
        // Taking a shortcut away cannot clash; a service that is down is not the user's problem here.
        let _ = try_register(self.host.as_ref(), paused, action, &current, &next);
        self.set_conflict(index, String::new());
        self.act(|ctl| ctl.update_settings(|s| s.hotkeys = next));
    }

    // ---- Start with Windows ----------------------------------------------------------------

    fn autostart_toggled(self: &Rc<Self>, on: bool) {
        match self.host.set_autostart(on) {
            Ok(()) => {
                self.autostart_failed.set(false);
                self.act(|ctl| ctl.update_settings(|s| s.autostart = on));
            }
            Err(error) => {
                crate::trace!("autostart {on}: {error}");
                self.autostart_failed.set(true);
                // The redraw puts the switch the user flipped back to the stored value.
                redraw_all();
            }
        }
    }

    // ---- Back, Next, Skip ------------------------------------------------------------------

    /// Hides the window when the tour has ended (finished or skipped).
    fn hide_if_over(&self) {
        if self.controller.borrow().tour_step().is_none()
            && let Some(window) = self.window.upgrade()
        {
            let _ = window.hide();
        }
    }

    fn next(self: &Rc<Self>) {
        self.act(Controller::tour_next);
        self.hide_if_over();
    }

    fn skip(self: &Rc<Self>) {
        self.act(Controller::tour_skip);
        self.hide_if_over();
    }
}

impl<C, K, T> Controller<Store, C, K, T>
where
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Draws the tour in `window` and routes its gestures to the controller.
    pub fn bind_onboarding(
        this: &Rc<RefCell<Self>>,
        window: &ui::OnboardingWindow,
        host: Rc<dyn SettingsHost>,
    ) -> OnboardingBinding<C, K, T> {
        let core = Rc::new_cyclic(|weak: &Weak<TourCore<C, K, T>>| {
            let weak = weak.clone();
            TourCore {
                controller: Rc::clone(this),
                window: window.as_weak(),
                host,
                conflicts: RefCell::new(vec![String::new(); HotkeyAction::ALL.len()]),
                autostart_failed: Cell::new(false),
                shown_hotkeys: RefCell::new(None),
                drawn: Cell::new(false),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
                save_timer: Rc::default(),
            }
        });
        register_redraw(&core.redraw);
        core.sync();

        macro_rules! wire {
            ($on:ident, |$core:ident $(, $arg:ident)*| $body:expr) => {{
                let $core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback onboarding {}{}", stringify!($on), Shown(&[$(&$arg as &dyn Loggable),*]));
                    $body;
                });
            }};
        }
        wire!(on_hotkey_recorded, |core, i, combo| core
            .hotkey_recorded(crate::binding::index(i), &combo));
        wire!(on_hotkey_cleared, |core, i| core
            .hotkey_cleared(crate::binding::index(i)));
        wire!(on_dock_edge_chosen, |core, i| core
            .act(|ctl| ctl.dock_to(dock_edge_of(i))));
        wire!(on_panel_in_taskbar_toggled, |core, on| {
            core.host.set_panel_in_taskbar(on);
            core.act(|ctl| ctl.update_settings(|s| s.panel_in_taskbar = on));
        });
        wire!(on_panel_hide_on_blur_toggled, |core, on| core
            .act(|ctl| ctl.update_settings(|s| s.panel_hide_on_blur = on)));
        wire!(on_autostart_toggled, |core, on| core.autostart_toggled(on));
        wire!(on_recent_history_toggled, |core, on| core
            .act(|ctl| ctl.update_settings(|s| s.recent_history = on)));
        wire!(on_close_to_tray_toggled, |core, on| core
            .act(|ctl| ctl.update_settings(|s| s.close_to_tray = on)));
        wire!(on_back_requested, |core| core.act(Controller::tour_back));
        wire!(on_next_requested, |core| core.next());
        wire!(on_skip_requested, |core| core.skip());
        {
            // The title bar's close button is a Skip too.
            let core = Rc::clone(&core);
            window.window().on_close_requested(move || {
                core.skip();
                slint::CloseRequestResponse::HideWindow
            });
        }
        OnboardingBinding { core }
    }
}

// ---- Opening the window on demand --------------------------------------------------------------

thread_local! {
    /// Opens the welcome tour; set by [`OnboardingUi::new`]. Help's "Show the welcome tour
    /// again" reaches the window through [`request_open`].
    static OPENER: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Opens the welcome tour at its first step (creating the window the first time), if the host
/// made one with [`OnboardingUi::new`]. UI thread only.
pub fn request_open() {
    let opener = OPENER.with(|o| o.borrow().clone());
    if let Some(opener) = opener {
        opener();
    }
}

/// Creates the welcome tour window the first time it is asked for, binds it, and shows and
/// raises it every time. Not generic: the host keeps one of these.
pub struct OnboardingUi {
    open: Box<dyn Fn() -> Result<(), slint::PlatformError>>,
    window: Box<dyn Fn() -> Option<ui::OnboardingWindow>>,
}

impl std::fmt::Debug for OnboardingUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnboardingUi").finish_non_exhaustive()
    }
}

impl OnboardingUi {
    /// A welcome tour for `controller`, not created yet. `host` does what its steps ask of the
    /// machine (the same host as Settings).
    pub fn new<C, K, T>(
        controller: &Rc<RefCell<Controller<Store, C, K, T>>>,
        host: Rc<dyn SettingsHost>,
    ) -> Self
    where
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        type Slot<C, K, T> =
            Rc<RefCell<Option<(ui::OnboardingWindow, OnboardingBinding<C, K, T>)>>>;
        let slot: Slot<C, K, T> = Rc::default();
        let open = {
            let slot = Rc::clone(&slot);
            let controller = Rc::clone(controller);
            Box::new(move || {
                // The tour always starts at the first step.
                controller.borrow_mut().show_welcome_tour();
                let mut slot = slot.borrow_mut();
                if slot.is_none() {
                    let window = create_window(ui::OnboardingWindow::new)?;
                    crate::native_window::size_before_show(
                        window.window(),
                        (window.get_wanted_width(), window.get_wanted_height()),
                    );
                    let binding =
                        Controller::bind_onboarding(&controller, &window, Rc::clone(&host));
                    *slot = Some((window, binding));
                }
                let Some((window, binding)) = slot.as_ref() else {
                    return Ok(());
                };
                crate::native_window::place_over_owner(window.window());
                window.show()?;
                fit_to_screen(window.window());
                // The window was hidden, so it skipped the redraws; draw it now that it is up.
                binding.redraw();
                raise(window.window());
                Ok(())
            })
        };
        let window = {
            let slot = Rc::clone(&slot);
            Box::new(move || {
                slot.borrow()
                    .as_ref()
                    .map(|(window, _)| window.clone_strong())
            })
        };
        let open: Rc<dyn Fn() -> Result<(), slint::PlatformError>> = Rc::new(open);
        OPENER.with(|o| {
            let open = Rc::clone(&open);
            *o.borrow_mut() = Some(Rc::new(move || {
                if let Err(e) = open() {
                    crate::trace!("cannot open the welcome tour: {e}");
                }
            }));
        });
        Self {
            open: Box::new(move || open()),
            window,
        }
    }

    /// Creates the window if this is the first time, then shows it at the first step and
    /// brings it to the front.
    ///
    /// # Errors
    ///
    /// The windowing system failed to create or show it.
    pub fn open(&self) -> Result<(), slint::PlatformError> {
        (self.open)()
    }

    /// The window, once it has been created.
    #[must_use]
    pub fn window(&self) -> Option<ui::OnboardingWindow> {
        (self.window)()
    }
}
