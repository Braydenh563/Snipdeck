//! Binds a [`Controller`] to the Slint `PanelWindow`: copies a [`View`] into the window's
//! properties and models, and wires every callback of the window to a controller method.
//!
//! The window never holds state of its own beyond what the user is typing and where the
//! selection is, and the controller never touches the window: this file is the only place
//! that knows both.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use snipdeck_core::{ItemKind, UseMode};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::{Rgb, SystemLook},
    tray::TrayEvent,
};
use snipdeck_ui as ui;

use crate::{
    controller::{
        AppMenuEntry, Controller, Effect, HelpTopic, ItemMenuEntry, Library, ToastPhase, View,
    },
    dock::DockEvent,
    frame::{Edge, Monitor, Rect},
    host::{ClipboardText, HotkeyProblem},
    item_editor::Origin,
    palette::list_color,
    panel::{Footer, Row, Tab},
    settings::HotkeyAction,
    wording,
};

type ProblemsFn = dyn Fn(&[HotkeyProblem]);
type FrameFn = dyn Fn(&Monitor, Rect);
type MonitorFn = dyn Fn(&Monitor);

thread_local! {
    /// Every bound window's redraw, for changes that show in more than one window: the Panel
    /// and the Main window read the same library and share one toast. Weak: a window that is
    /// gone drops out.
    static REDRAW: RefCell<Vec<Weak<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
}

/// Adds a window's redraw to the set [`redraw_all`] runs.
pub(crate) fn register_redraw(redraw: &Rc<dyn Fn()>) {
    REDRAW.with(|all| {
        let mut all = all.borrow_mut();
        all.retain(|w| w.strong_count() > 0);
        all.push(Rc::downgrade(redraw));
    });
}

/// Redraws every bound window from its controller (the toast timer, a change made in the
/// other window). Call without a controller borrow held.
pub(crate) fn redraw_all() {
    let all: Vec<Rc<dyn Fn()>> =
        REDRAW.with(|all| all.borrow().iter().filter_map(Weak::upgrade).collect());
    for redraw in all {
        redraw();
    }
}

/// Runs the timer chain of toast `seq`: each tick advances the toast and redraws the windows.
pub(crate) fn arm_toast<S, C, K, T>(
    controller: Weak<RefCell<Controller<S, C, K, T>>>,
    seq: u64,
    delay: std::time::Duration,
) where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    slint::Timer::single_shot(delay, move || {
        let Some(strong) = controller.upgrade() else {
            return;
        };
        let next = strong.borrow_mut().toast_tick(seq);
        redraw_all();
        if let Some(next) = next {
            arm_toast(controller, seq, next);
        }
    });
}

/// How long after a collapse the handle's frame is asked for once more (see `Core::apply`).
const DOCK_REPLACE_AFTER: std::time::Duration = std::time::Duration::from_millis(50);

/// Moves and sizes the window to `frame`.
fn place(window: &ui::PanelWindow, frame: Rect) {
    let w = window.window();
    w.set_position(slint::PhysicalPosition::new(frame.x, frame.y));
    w.set_size(slint::PhysicalSize::new(frame.w, frame.h));
}

/// What the host keeps after binding: the entry points for events that do not come from the
/// window (hotkeys, a second launch, the Target changing, the tray icon, Windows' colour mode). Cheap to clone, not `Send`: hand
/// events over with `slint::invoke_from_event_loop`.
#[derive(Clone)]
pub struct Binding {
    hotkey: Rc<dyn Fn(HotkeyAction)>,
    show: Rc<dyn Fn()>,
    target_changed: Rc<dyn Fn()>,
    hotkey_problems: Rc<ProblemsFn>,
    own_window_active: Rc<dyn Fn(bool)>,
    tray: Rc<dyn Fn(TrayEvent)>,
    system_look: Rc<dyn Fn(SystemLook)>,
    frame_changed: Rc<FrameFn>,
    frame_moved: Rc<FrameFn>,
    restore_frame: Rc<MonitorFn>,
}

impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binding").finish_non_exhaustive()
    }
}

impl Binding {
    /// A global hotkey fired.
    pub fn hotkey(&self, action: HotkeyAction) {
        (self.hotkey)(action);
    }

    /// A second launch asked for the app: shows the Panel, or opens the Main window if the
    /// Panel is already up.
    pub fn show(&self) {
        (self.show)();
    }

    /// The foreground window changed.
    pub fn target_changed(&self) {
        (self.target_changed)();
    }

    /// Some hotkeys could not be registered: tell the user.
    pub fn hotkey_problems(&self, problems: &[HotkeyProblem]) {
        (self.hotkey_problems)(problems);
    }

    /// A regular Snipdeck window gained or lost the focus: while one has it the Panel is not
    /// topmost (see `Controller::set_own_window_active`).
    pub fn own_window_active(&self, active: bool) {
        (self.own_window_active)(active);
    }

    /// Draws every bound window again from the controller. For a change made on the
    /// controller from outside a gesture (a toast set by the maintenance run) with no
    /// borrow of it held.
    pub fn redraw(&self) {
        redraw_all();
    }

    /// The user clicked the tray icon or chose an entry of its menu.
    pub fn tray(&self, event: TrayEvent) {
        (self.tray)(event);
    }

    /// Windows' light / dark or high-contrast state is `look` (read at start-up, and again on
    /// every change notification; an unchanged look redraws nothing).
    pub fn system_look(&self, look: SystemLook) {
        (self.system_look)(look);
    }

    /// The Panel's frame is now `frame` on `monitor` (physical pixels; after a resize or a
    /// move). It is clamped and remembered for that monitor, and the window is put back inside
    /// the limits if it got out of them ([`Effect::PlaceWindow`]).
    pub fn frame_changed(&self, monitor: &Monitor, frame: Rect) {
        (self.frame_changed)(monitor, frame);
    }

    /// The Panel was moved to `frame` on `monitor`: remembered, never pulled back inside the
    /// work area (a Panel may straddle two monitors while it is dragged across).
    pub fn frame_moved(&self, monitor: &Monitor, frame: Rect) {
        (self.frame_moved)(monitor, frame);
    }

    /// The Panel is shown on `monitor`: put it where the user left it there, if they did.
    pub fn restore_frame(&self, monitor: &Monitor) {
        (self.restore_frame)(monitor);
    }
}

struct Core<S, C, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    window: slint::Weak<ui::PanelWindow>,
    on_effect: Box<dyn Fn(&Effect)>,
    /// What the window shows now, so a redraw sets only what changed (re-setting the rows
    /// model would reset the list's scroll position on every toast tick).
    shown: RefCell<Option<View>>,
    /// Redraws this window (registered with [`register_redraw`]).
    redraw: Rc<dyn Fn()>,
    /// A timer is waiting to write changed settings.
    settings_timer: Cell<bool>,
    /// A timer is waiting to clear a copied sensitive item off the clipboard.
    clipboard_timer: Cell<bool>,
    /// Sends the dock its ticks while a hover or leave delay runs.
    dock_timer: slint::Timer,
}

impl<S, C, K, T> Core<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// The core for `window`, registered for redraws (see [`register_redraw`]).
    fn new(
        controller: &Rc<RefCell<Controller<S, C, K, T>>>,
        window: &ui::PanelWindow,
        on_effect: Box<dyn Fn(&Effect)>,
    ) -> Rc<Self> {
        let core = Rc::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            Self {
                controller: Rc::clone(controller),
                window: window.as_weak(),
                on_effect,
                shown: RefCell::new(None),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
                settings_timer: Cell::new(false),
                clipboard_timer: Cell::new(false),
                dock_timer: slint::Timer::default(),
            }
        });
        register_redraw(&core.redraw);
        core
    }

    /// Runs `f` on the controller, then redraws.
    fn act<R>(self: &Rc<Self>, f: impl FnOnce(&mut Controller<S, C, K, T>) -> R) -> R {
        let result = f(&mut self.controller.borrow_mut());
        redraw_all();
        result
    }

    /// Runs `f` and applies the effects it returns.
    fn act_effects(self: &Rc<Self>, f: impl FnOnce(&mut Controller<S, C, K, T>) -> Vec<Effect>) {
        let effects = self.act(f);
        self.apply(&effects);
    }

    fn apply(&self, effects: &[Effect]) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        for effect in effects {
            crate::trace!("effect {effect:?}");
            match effect {
                Effect::ShowPanel => {
                    // A failure to show leaves the Panel hidden; the next hotkey tries again.
                    let shown = window.show();
                    crate::trace!(
                        "show panel: {shown:?}, visible now {}",
                        window.window().is_visible()
                    );
                    // A hidden Panel comes back where it was in the stack, behind the app in
                    // use; and one that is "shown" already (a second Open Panel) may be buried.
                    crate::native_window::raise(&window);
                }
                Effect::HidePanel => {
                    let _ = window.hide();
                }
                // The item editor is its own window; the controller opens it and every bound
                // window (the editor's included) is redrawn from it.
                Effect::NewItem => {
                    self.controller.borrow_mut().open_new_item(Origin::Panel);
                    redraw_all();
                }
                Effect::EditItem(id) => {
                    self.controller.borrow_mut().open_item_editor(*id);
                    redraw_all();
                }
                Effect::PlaceWindow(frame) => {
                    place(&window, *frame);
                    if window.get_dock_collapsed() {
                        // The window's minimum size is lifted by the redraw that follows, so
                        // a size asked for now may still be held up to the old minimum: ask
                        // again once it is.
                        let (weak, frame) = (self.window.clone(), *frame);
                        slint::Timer::single_shot(DOCK_REPLACE_AFTER, move || {
                            if let Some(window) = weak.upgrade()
                                && window.get_dock_collapsed()
                            {
                                place(&window, frame);
                            }
                        });
                    }
                }
                _ => {}
            }
            (self.on_effect)(effect);
        }
    }

    /// Copies the controller's view into the window and keeps the toast timer running.
    fn sync(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let view = self.controller.borrow().view();
        render(&window, self.shown.borrow().as_ref(), &view);
        // Placements decided by another window (Settings docking the Panel, the dock being
        // switched off) are carried out here, where the Panel's window is.
        let pending = self.controller.borrow_mut().take_pending();
        if !pending.is_empty() {
            self.apply(&pending);
        }
        self.arm_dock();
        if let Some(toast) = &view.toast
            && self.controller.borrow_mut().claim_toast_timer(toast.seq)
        {
            arm_toast(
                Rc::downgrade(&self.controller),
                toast.seq,
                crate::controller::TOAST_ENTER,
            );
        }
        *self.shown.borrow_mut() = Some(view);
        self.arm_settings();
        self.arm_clipboard_clear();
    }

    /// Runs the dock's tick timer while a hover or leave delay is running, and only then.
    fn arm_dock(self: &Rc<Self>) {
        let needed = self.controller.borrow().dock_needs_tick();
        if needed == self.dock_timer.running() {
            return;
        }
        if needed {
            let core = Rc::downgrade(self);
            self.dock_timer
                .start(slint::TimerMode::Repeated, crate::dock::TICK, move || {
                    if let Some(core) = core.upgrade() {
                        core.act_effects(|ctl| ctl.dock_event(DockEvent::Tick));
                    }
                });
        } else {
            self.dock_timer.stop();
        }
    }

    /// Keeps one timer running while a copied sensitive item waits to come off the clipboard;
    /// the controller's clock decides whether it is due.
    fn arm_clipboard_clear(self: &Rc<Self>) {
        if self.clipboard_timer.get() {
            return;
        }
        let Some(wait) = self.controller.borrow().clipboard_clear_wait() else {
            return;
        };
        self.clipboard_timer.set(true);
        let core = Rc::downgrade(self);
        slint::Timer::single_shot(wait, move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            core.clipboard_timer.set(false);
            core.controller.borrow_mut().clear_clipboard_if_due();
            core.sync();
        });
    }

    /// Keeps one timer running while changed settings wait to be written: when it fires the
    /// controller writes them if they are due (its clock decides) and the timer is re-armed
    /// for whatever is still waiting, such as a retry after a failed write.
    fn arm_settings(self: &Rc<Self>) {
        if self.settings_timer.get() {
            return;
        }
        let Some(wait) = self.controller.borrow().settings_wait() else {
            return;
        };
        self.settings_timer.set(true);
        let core = Rc::downgrade(self);
        slint::Timer::single_shot(wait, move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            core.settings_timer.set(false);
            core.controller.borrow_mut().save_settings_if_due();
            core.sync();
        });
    }
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Draws the controller in `window` and routes the window's gestures to it.
    ///
    /// `on_effect` is called with every [`Effect`] after the window has acted on it (a
    /// `ShowPanel` has shown the window, a `HidePanel` has hidden it); the host handles the
    /// rest (`OpenPicker`, `StartCapture`, `PasteNext`, `DragWindow`).
    ///
    #[allow(
        clippy::too_many_lines,
        reason = "one flat list of the window's callbacks, each a single line"
    )]
    pub fn bind(
        this: &Rc<RefCell<Self>>,
        window: &ui::PanelWindow,
        on_effect: impl Fn(&Effect) + 'static,
    ) -> Binding {
        let core = Core::new(this, window, Box::new(on_effect));
        core.sync();
        {
            // Where the other windows open over the Panel, while it is shown.
            let weak = window.as_weak();
            crate::native_window::register_surface(crate::native_window::Surface::Panel, move || {
                weak.upgrade()
                    .and_then(|w| crate::native_window::shown_frame(w.window()))
            });
        }

        // Wires `ui.on_<name>(|args| ...)` to a controller call, redrawing afterwards.
        macro_rules! wire {
            ($on:ident, |$ctl:ident $(, $arg:ident)*| $body:expr) => {{
                let core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback {}{}", stringify!($on), Shown(&[$(&$arg as &dyn Loggable),*]));
                    crate::native_window::note_opener(crate::native_window::Surface::Panel);
                    core.act(|$ctl| {
                        let _ = $body;
                    });
                });
            }};
        }
        macro_rules! wire_effects {
            ($on:ident, |$ctl:ident $(, $arg:ident)*| $body:expr) => {{
                let core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback {}{}", stringify!($on), Shown(&[$(&$arg as &dyn Loggable),*]));
                    crate::native_window::note_opener(crate::native_window::Surface::Panel);
                    core.act_effects(|$ctl| $body);
                });
            }};
        }

        wire!(on_query_changed, |ctl, query| ctl.set_query(query.as_str()));
        wire_effects!(on_row_activated, |ctl, i| {
            let used = ctl.activate_row(index(i), UseMode::Paste);
            ctl.dock_after_use(&used)
        });
        wire!(on_row_copied, |ctl, i| ctl
            .activate_row(index(i), UseMode::Copy));
        wire_effects!(on_quick_use, |ctl, n| {
            let used = ctl.quick_use(u8::try_from(n).unwrap_or(0));
            ctl.dock_after_use(&used)
        });
        wire!(on_context_menu_requested, |ctl, i, _x, _y| ctl
            .open_item_menu(index(i), false));
        wire!(on_pinned_context_menu_requested, |ctl, i, _x, _y| ctl
            .open_item_menu(index(i), true));
        wire_effects!(on_item_menu_activated, |ctl, i| ctl
            .choose_item_menu(index(i)));
        wire!(on_item_menu_dismissed, |ctl| ctl.close_item_menu());
        wire!(on_list_menu_requested, |ctl, i| ctl
            .open_list_menu(index(i)));
        wire_effects!(on_list_menu_activated, |ctl, i| ctl
            .choose_list_menu(index(i)));
        wire!(on_list_menu_dismissed, |ctl| ctl.close_list_menu());
        wire!(on_list_edit_requested, |ctl, i| ctl.edit_list_tab(index(i)));
        wire!(on_reveal_requested, |ctl, i, on| ctl
            .reveal_row(index(i), on));
        wire!(on_selection_moved, |ctl, delta| ctl
            .move_selection(isize::try_from(delta).unwrap_or(0)));
        wire!(on_tab_selected, |ctl, i| ctl.select_tab(index(i)));
        wire_effects!(on_pinned_used, |ctl, i| {
            let used = ctl.activate_pinned(index(i), UseMode::Paste);
            ctl.dock_after_use(&used)
        });
        wire!(on_pinned_copied, |ctl, i| ctl
            .activate_pinned(index(i), UseMode::Copy));
        wire!(on_board_menu_requested, |ctl| ctl.open_board_menu());
        wire!(on_board_menu_activated, |ctl, i| ctl.choose_board(index(i)));
        wire!(on_board_menu_dismissed, |ctl| ctl.close_board_menu());
        wire!(on_pin_toggled, |ctl, on| ctl.set_on_top(on));
        wire!(on_add_list_requested, |ctl| ctl.add_list());
        wire!(on_add_from_clipboard, |ctl| ctl.quick_capture());
        wire!(on_app_menu_requested, |ctl| ctl.open_app_menu());
        wire_effects!(on_app_menu_activated, |ctl, i| ctl
            .choose_app_menu(index(i)));
        wire_effects!(on_help_requested, |ctl| ctl.open_help(HelpTopic::Guide));
        wire!(on_app_menu_dismissed, |ctl| ctl.close_app_menu());
        wire_effects!(on_edit_requested, |ctl, i| ctl.edit_row(index(i)));
        wire_effects!(on_rename_requested, |ctl, i| ctl.rename_row(index(i)));
        wire!(on_delete_requested, |ctl, i| ctl.delete_row(index(i)));
        wire!(on_toast_undo, |ctl| ctl.toast_action());
        wire_effects!(on_resize_requested, |ctl, edge| ctl.resize(edge_of(edge)));
        wire_effects!(on_handle_hovered, |ctl, on| ctl
            .dock_event(DockEvent::HandleHover(on)));
        wire_effects!(on_handle_clicked, |ctl| ctl
            .dock_event(DockEvent::HandleClick));
        wire_effects!(on_panel_hovered, |ctl, on| ctl
            .dock_event(DockEvent::PanelHover(on)));
        wire_effects!(on_close_requested, |ctl| ctl.close());
        // An OS close (Alt+F4, WM_CLOSE) goes the same way as the close button: the controller
        // decides between hiding to the tray and quitting, and keeps `visible` in step.
        {
            let core = Rc::clone(&core);
            window.window().on_close_requested(move || {
                core.act_effects(Controller::close);
                slint::CloseRequestResponse::KeepWindowShown
            });
        }
        wire_effects!(on_drag_requested, |ctl| ctl.drag());

        Binding {
            hotkey: {
                let core = Rc::clone(&core);
                Rc::new(move |action| {
                    crate::trace!("hotkey {action:?}");
                    core.act_effects(|ctl| ctl.on_hotkey(action));
                })
            },
            show: {
                let core = Rc::clone(&core);
                Rc::new(move || {
                    crate::trace!("show requested");
                    core.act_effects(Controller::second_launch);
                })
            },
            target_changed: {
                let core = Rc::clone(&core);
                Rc::new(move || {
                    // The tracker can call this from inside a controller call; skip then.
                    if core.controller.try_borrow_mut().is_ok() {
                        crate::trace!("target changed");
                        core.act_effects(Controller::on_foreground_changed);
                    } else {
                        crate::trace!("target changed during a controller call: skipped");
                    }
                })
            },
            hotkey_problems: {
                let core = Rc::clone(&core);
                Rc::new(move |problems: &[HotkeyProblem]| {
                    core.act(|ctl| ctl.report_hotkey_problems(problems));
                })
            },
            own_window_active: {
                let core = Rc::clone(&core);
                Rc::new(move |active: bool| {
                    crate::trace!("own window active {active}");
                    core.act(|ctl| ctl.set_own_window_active(active));
                })
            },
            tray: {
                let core = Rc::clone(&core);
                Rc::new(move |event| {
                    crate::trace!("tray {event:?}");
                    core.act_effects(|ctl| ctl.on_tray(event));
                })
            },
            system_look: {
                let core = Rc::clone(&core);
                Rc::new(move |look| core.act(|ctl| ctl.set_system_look(look)))
            },
            frame_changed: {
                let core = Rc::clone(&core);
                Rc::new(move |monitor: &Monitor, frame| {
                    // Placing the window can report its new frame from inside a controller
                    // call; that frame is already clamped, so skip it.
                    if core.controller.try_borrow_mut().is_ok() {
                        core.act_effects(|ctl| ctl.frame_changed(monitor, frame));
                    }
                })
            },
            frame_moved: {
                let core = Rc::clone(&core);
                Rc::new(move |monitor: &Monitor, frame| {
                    if core.controller.try_borrow_mut().is_ok() {
                        core.act_effects(|ctl| ctl.frame_moved(monitor, frame));
                    }
                })
            },
            restore_frame: {
                let core = Rc::clone(&core);
                Rc::new(move |monitor: &Monitor| {
                    crate::trace!("restore frame");
                    core.act_effects(|ctl| ctl.restore_frame(monitor));
                })
            },
        }
    }
}

/// A callback argument as the diagnostic log shows it: numbers and flags as they are, text only
/// by its length (it is what the user typed).
pub(crate) trait Loggable {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}

impl Loggable for i32 {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl Loggable for f32 {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl Loggable for bool {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl Loggable for ui::ResizeEdge {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", edge_of(*self).name())
    }
}

impl Loggable for SharedString {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{} chars>", self.chars().count())
    }
}

/// A callback's arguments for the log: ` (3, true)`, or nothing without arguments.
pub(crate) struct Shown<'a>(pub(crate) &'a [&'a dyn Loggable]);

impl std::fmt::Display for Shown<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            return Ok(());
        }
        f.write_str(" (")?;
        for (i, arg) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            arg.log(f)?;
        }
        f.write_str(")")
    }
}

/// The grip the window reported, as the controller names it.
fn edge_of(edge: ui::ResizeEdge) -> Edge {
    match edge {
        ui::ResizeEdge::Left => Edge::Left,
        ui::ResizeEdge::Right => Edge::Right,
        ui::ResizeEdge::Top => Edge::Top,
        ui::ResizeEdge::Bottom => Edge::Bottom,
        ui::ResizeEdge::TopLeft => Edge::TopLeft,
        ui::ResizeEdge::TopRight => Edge::TopRight,
        ui::ResizeEdge::BottomLeft => Edge::BottomLeft,
        ui::ResizeEdge::BottomRight => Edge::BottomRight,
    }
}

pub(crate) fn index(i: i32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

// ---- View -> window ------------------------------------------------------------------------

/// Sets the window's properties from `view`, skipping what `previous` already shows.
#[allow(
    clippy::too_many_lines,
    reason = "one flat pass over the view, a few lines per property"
)]
pub fn render(window: &ui::PanelWindow, previous: Option<&View>, view: &View) {
    let differs = |same: &dyn Fn(&View) -> bool| previous.is_none_or(|p| !same(p));

    if differs(&|p| p.dark == view.dark) {
        window.set_dark(view.dark);
    }
    if differs(&|p| p.high_contrast == view.high_contrast) {
        let look = window.global::<ui::SystemLook<'_>>();
        look.set_high_contrast(view.high_contrast.is_some());
        if let Some(hc) = &view.high_contrast {
            let color = |c: Rgb| slint::Color::from_rgb_u8(c.r, c.g, c.b);
            look.set_hc_window(color(hc.window));
            look.set_hc_window_text(color(hc.window_text));
            look.set_hc_highlight(color(hc.highlight));
            look.set_hc_highlight_text(color(hc.highlight_text));
            look.set_hc_gray_text(color(hc.gray_text));
        }
    }
    if differs(&|p| p.board_name == view.board_name) {
        window.set_board_name(view.board_name.as_str().into());
    }
    if differs(&|p| p.tabs == view.tabs && p.dark == view.dark) {
        window.set_tabs(model(view.tabs.iter().map(|t| tab_item(t, view.dark))));
        // The dock's handle wears the colour of the list on show.
        let accent = view.tabs.iter().find(|t| t.active).and_then(|t| t.accent);
        window.set_dock_has_list_color(accent.is_some());
        window.set_dock_list_color(list_color(accent, view.dark));
    }
    if differs(&|p| p.dock_collapsed == view.dock_collapsed) {
        window.set_dock_collapsed(view.dock_collapsed);
    }
    if differs(&|p| p.dock_vertical == view.dock_vertical) {
        window.set_dock_vertical(view.dock_vertical);
    }
    if differs(&|p| p.pinned == view.pinned) {
        window.set_pinned(model(view.pinned.iter().map(|r| ui::PinnedItem {
            title: r.title.as_str().into(),
        })));
    }
    if differs(&|p| p.rows == view.rows && p.dark == view.dark) {
        window.set_rows(model(view.rows.iter().map(|r| panel_row(r, view.dark))));
    }
    let revealed = view.revealed.map_or(-1, to_i32);
    if window.get_revealed_index() != revealed {
        window.set_revealed_index(revealed);
    }
    if window.get_current_tab() != to_i32(view.current_tab) {
        window.set_current_tab(to_i32(view.current_tab));
    }
    if window.get_selected_index() != to_i32(view.selected) {
        window.set_selected_index(to_i32(view.selected));
    }
    if window.get_query().as_str() != view.query {
        window.set_query(view.query.as_str().into());
    }
    if window.get_on_top() != view.on_top {
        window.set_on_top(view.on_top);
    }
    if window.get_topmost() != view.topmost {
        window.set_topmost(view.topmost);
    }
    if differs(&|p| p.footer == view.footer) {
        let (app, title) = match &view.footer {
            Footer::PastesInto { app, title } => (app.as_str(), title.as_str()),
            Footer::Copies => ("", ""),
        };
        window.set_target_name(app.into());
        window.set_target_title(title.into());
    }
    if differs(&|p| p.board_menu == view.board_menu) {
        let entries = view.board_menu.iter().flatten().map(|name| ui::MenuEntry {
            text: name.as_str().into(),
            ..Default::default()
        });
        window.set_board_menu_entries(model(entries));
        window.set_board_menu_open(view.board_menu.is_some());
    }
    if differs(&|p| p.app_menu == view.app_menu && p.hotkeys_paused == view.hotkeys_paused)
        || window.get_app_menu_open() != view.app_menu.is_some()
    {
        render_app_menu(window, view);
    }
    if differs(&|p| p.item_menu == view.item_menu)
        || window.get_item_menu_open() != view.item_menu.is_some()
    {
        let entries = view.item_menu.iter().flatten().map(|&entry| ui::MenuEntry {
            text: wording::item_menu_text(entry).into(),
            separator: entry == ItemMenuEntry::Separator,
            destructive: entry == ItemMenuEntry::Trash,
            ..Default::default()
        });
        window.set_item_menu_entries(model(entries));
        window.set_item_menu_open(view.item_menu.is_some());
    }
    if differs(&|p| p.list_menu == view.list_menu)
        || window.get_list_menu_open() != view.list_menu.is_some()
    {
        window.set_list_menu_open(view.list_menu.is_some());
    }
    if differs(&|p| p.toast == view.toast) {
        if let Some(toast) = &view.toast {
            {
                window.set_toast_undo_label(wording::toast_button_text(toast).into());
                window.set_toast_kind(match toast.kind {
                    crate::toast::ToastKind::Neutral => ui::ToastKind::Neutral,
                    crate::toast::ToastKind::Success => ui::ToastKind::Success,
                    crate::toast::ToastKind::Danger => ui::ToastKind::Danger,
                });
                window.set_toast_phase(match toast.phase {
                    ToastPhase::Enter => ui::ToastPhase::Enter,
                    ToastPhase::Hold => ui::ToastPhase::Hold,
                    ToastPhase::Exit => ui::ToastPhase::Exit,
                });
                window.set_toast_text(wording::toast_text(&toast.message).into());
            }
        } else {
            window.set_toast_text(SharedString::new());
            window.set_toast_undo_label(SharedString::new());
            window.set_toast_phase(ui::ToastPhase::Exit);
        }
    }
}

/// The kebab menu's rows and whether it is open. "Pause hotkeys" carries a check mark while
/// the hotkeys are paused.
fn render_app_menu(window: &ui::PanelWindow, view: &View) {
    let check = window.global::<ui::Icons<'_>>().get_check();
    let entries = view.app_menu.iter().flatten().map(|&entry| ui::MenuEntry {
        text: if entry == AppMenuEntry::Dock && view.docked {
            wording::UNDOCK_TEXT.into()
        } else {
            wording::app_menu_text(entry).into()
        },
        icon: if entry == AppMenuEntry::PauseHotkeys && view.hotkeys_paused {
            check.clone()
        } else {
            slint::Image::default()
        },
        separator: entry == AppMenuEntry::Separator,
        ..Default::default()
    });
    window.set_app_menu_entries(model(entries));
    window.set_app_menu_open(view.app_menu.is_some());
}

pub(crate) fn model<T: Clone + 'static>(items: impl Iterator<Item = T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items.collect::<Vec<_>>()))
}

pub(crate) fn to_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn tab_item(tab: &Tab, dark: bool) -> ui::TabItem {
    ui::TabItem {
        text: tab.name.as_str().into(),
        accent: list_color(tab.accent, dark),
    }
}

/// The text shown instead of a sensitive item's preview.
pub const MASKED_PREVIEW: &str = "••••••••";

/// One [`Row`] as the window's `PanelRow`.
#[must_use]
pub fn panel_row(row: &Row, dark: bool) -> ui::PanelRow {
    let (list_name, list_color) = row.list.as_ref().map_or_else(
        || (SharedString::new(), slint::Color::default()),
        |label| (label.name.as_str().into(), list_color(label.accent, dark)),
    );
    ui::PanelRow {
        kind: item_kind(row),
        title: row.title.as_str().into(),
        // Never rely on the store keeping a sensitive preview empty.
        preview: if row.sensitive {
            SharedString::new()
        } else {
            row.preview.as_str().into()
        },
        masked_preview: if row.sensitive {
            MASKED_PREVIEW.into()
        } else {
            SharedString::new()
        },
        extension: if row.kind == ItemKind::File {
            extension_of(&row.preview).into()
        } else {
            SharedString::new()
        },
        list_name,
        list_color,
        image: slint::Image::default(),
    }
}

fn item_kind(row: &Row) -> ui::ItemKind {
    item_kind_of(row.kind, row.sensitive)
}

/// The window's kind for an item: a sensitive item is its own kind whatever it holds.
pub(crate) fn item_kind_of(kind: ItemKind, sensitive: bool) -> ui::ItemKind {
    if sensitive {
        return ui::ItemKind::Sensitive;
    }
    match kind {
        ItemKind::Text => ui::ItemKind::Text,
        ItemKind::Template => ui::ItemKind::Template,
        ItemKind::Code => ui::ItemKind::Code,
        ItemKind::Link => ui::ItemKind::Link,
        ItemKind::Image => ui::ItemKind::Image,
        ItemKind::File => ui::ItemKind::File,
    }
}

/// `report-q3.pdf` becomes `PDF`; nothing that does not look like an extension.
pub(crate) fn extension_of(name: &str) -> String {
    let first = name.lines().next().unwrap_or_default().trim();
    first
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|ext| {
            (1..=4).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .map(str::to_ascii_uppercase)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::ListLabel;
    use slint::Model;
    use snipdeck_core::{ItemId, ListAccent};

    fn row(kind: ItemKind, sensitive: bool, preview: &str) -> Row {
        Row {
            id: ItemId::generate(),
            number: 1,
            kind,
            sensitive,
            title: "Title".into(),
            preview: preview.into(),
            list: None,
            pinned: false,
        }
    }

    #[test]
    fn a_sensitive_row_is_masked_and_never_carries_a_preview_kind() {
        let mapped = panel_row(&row(ItemKind::Text, true, "hunter2"), false);
        assert_eq!(mapped.kind, ui::ItemKind::Sensitive);
        assert_eq!(mapped.masked_preview.as_str(), MASKED_PREVIEW);
        assert_eq!(mapped.preview.as_str(), "");
    }

    #[test]
    fn kinds_map_one_to_one() {
        for (core, expected) in [
            (ItemKind::Text, ui::ItemKind::Text),
            (ItemKind::Template, ui::ItemKind::Template),
            (ItemKind::Code, ui::ItemKind::Code),
            (ItemKind::Link, ui::ItemKind::Link),
            (ItemKind::Image, ui::ItemKind::Image),
            (ItemKind::File, ui::ItemKind::File),
        ] {
            assert_eq!(panel_row(&row(core, false, "x"), false).kind, expected);
        }
    }

    #[test]
    fn a_file_row_shows_its_extension() {
        assert_eq!(
            panel_row(&row(ItemKind::File, false, "report-q3.pdf"), false).extension,
            "PDF"
        );
        assert_eq!(
            panel_row(&row(ItemKind::File, false, "Makefile"), false).extension,
            ""
        );
        assert_eq!(
            panel_row(&row(ItemKind::File, false, "a.toolongext"), false).extension,
            ""
        );
        assert_eq!(
            panel_row(&row(ItemKind::Text, false, "a.pdf"), false).extension,
            ""
        );
    }

    #[test]
    fn a_mixed_view_row_carries_its_list_name_and_colour() {
        let mut r = row(ItemKind::Text, false, "x");
        r.list = Some(ListLabel {
            name: "Links".into(),
            accent: Some(ListAccent::Violet),
        });
        let mapped = panel_row(&r, false);
        assert_eq!(mapped.list_name, "Links");
        assert_eq!(
            mapped.list_color,
            list_color(Some(ListAccent::Violet), false)
        );
        assert_eq!(
            panel_row(&row(ItemKind::Text, false, "x"), false).list_name,
            ""
        );
    }

    #[test]
    fn indexes_never_wrap() {
        assert_eq!(index(3), 3);
        assert_eq!(index(-1), usize::MAX);
        assert_eq!(to_i32(5), 5);
    }

    #[test]
    fn models_keep_their_order() {
        let m = model([1, 2, 3].into_iter());
        assert_eq!(m.row_count(), 3);
        assert_eq!(m.row_data(2), Some(3));
    }
}
