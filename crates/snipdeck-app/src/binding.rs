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
    controller::{Controller, Effect, Library, ToastPhase, View},
    host::{ClipboardText, HotkeyProblem},
    palette::list_color,
    panel::{Footer, Row, Tab},
    settings::HotkeyAction,
    wording,
};

type ProblemsFn = dyn Fn(&[HotkeyProblem]);

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

/// What the host keeps after binding: the entry points for events that do not come from the
/// window (hotkeys, a second launch, the Target changing, the tray icon, Windows' colour mode). Cheap to clone, not `Send`: hand
/// events over with `slint::invoke_from_event_loop`.
#[derive(Clone)]
pub struct Binding {
    hotkey: Rc<dyn Fn(HotkeyAction)>,
    show: Rc<dyn Fn()>,
    target_changed: Rc<dyn Fn()>,
    hotkey_problems: Rc<ProblemsFn>,
    tray: Rc<dyn Fn(TrayEvent)>,
    system_look: Rc<dyn Fn(SystemLook)>,
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

    /// Shows the Panel (a second launch asked for it).
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

    /// The user clicked the tray icon or chose an entry of its menu.
    pub fn tray(&self, event: TrayEvent) {
        (self.tray)(event);
    }

    /// Windows' light / dark or high-contrast state is `look` (read at start-up, and again on
    /// every change notification; an unchanged look redraws nothing).
    pub fn system_look(&self, look: SystemLook) {
        (self.system_look)(look);
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
                    let _ = window.show();
                }
                Effect::HidePanel => {
                    let _ = window.hide();
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
    /// Not yet wired, because the feature behind it is not built: the Panel's context menus.
    /// (Undo lives on the Main window's toast.)
    pub fn bind(
        this: &Rc<RefCell<Self>>,
        window: &ui::PanelWindow,
        on_effect: impl Fn(&Effect) + 'static,
    ) -> Binding {
        let core = Core::new(this, window, Box::new(on_effect));
        core.sync();

        // Wires `ui.on_<name>(|args| ...)` to a controller call, redrawing afterwards.
        macro_rules! wire {
            ($on:ident, |$ctl:ident $(, $arg:ident)*| $body:expr) => {{
                let core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback {}{}", stringify!($on), Shown(&[$(&$arg as &dyn Loggable),*]));
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
                    core.act_effects(|$ctl| $body);
                });
            }};
        }

        wire!(on_query_changed, |ctl, query| ctl.set_query(query.as_str()));
        wire!(on_row_activated, |ctl, i| ctl
            .activate_row(index(i), UseMode::Paste));
        wire!(on_row_copied, |ctl, i| ctl
            .activate_row(index(i), UseMode::Copy));
        wire!(on_quick_use, |ctl, n| ctl
            .quick_use(u8::try_from(n).unwrap_or(0)));
        wire!(on_selection_moved, |ctl, delta| ctl
            .move_selection(isize::try_from(delta).unwrap_or(0)));
        wire!(on_tab_selected, |ctl, i| ctl.select_tab(index(i)));
        wire!(on_pinned_used, |ctl, i| ctl
            .activate_pinned(index(i), UseMode::Paste));
        wire!(on_pinned_copied, |ctl, i| ctl
            .activate_pinned(index(i), UseMode::Copy));
        wire!(on_board_menu_requested, |ctl| ctl.open_board_menu());
        wire!(on_board_menu_activated, |ctl, i| ctl.choose_board(index(i)));
        wire!(on_board_menu_dismissed, |ctl| ctl.close_board_menu());
        wire!(on_pin_toggled, |ctl, on| ctl.set_on_top(on));
        wire!(on_add_list_requested, |ctl| ctl.add_list());
        wire!(on_add_from_clipboard, |ctl| ctl.quick_capture());
        wire_effects!(on_close_requested, |ctl| ctl.close());
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
                    core.act_effects(Controller::show);
                })
            },
            target_changed: {
                let core = Rc::clone(&core);
                Rc::new(move || {
                    // The tracker can call this from inside a controller call; skip then.
                    if core.controller.try_borrow_mut().is_ok() {
                        crate::trace!("target changed");
                        core.act(Controller::on_target_changed);
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

impl Loggable for bool {
    fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
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

pub(crate) fn index(i: i32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

// ---- View -> window ------------------------------------------------------------------------

/// Sets the window's properties from `view`, skipping what `previous` already shows.
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
    }
    if differs(&|p| p.pinned == view.pinned) {
        window.set_pinned(model(view.pinned.iter().map(|r| ui::PinnedItem {
            title: r.title.as_str().into(),
        })));
    }
    if differs(&|p| p.rows == view.rows && p.dark == view.dark) {
        window.set_rows(model(view.rows.iter().map(|r| panel_row(r, view.dark))));
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
    if differs(&|p| p.toast == view.toast) {
        if let Some(toast) = &view.toast {
            {
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
            window.set_toast_phase(ui::ToastPhase::Exit);
        }
    }
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
        preview: row.preview.as_str().into(),
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
        let mapped = panel_row(&row(ItemKind::Text, true, ""), false);
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
