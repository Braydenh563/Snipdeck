//! Binds the Main window and its list and board editors to the [`Controller`]: copies a
//! [`MainScreen`] into the window's properties and models, and wires every callback to a
//! `main_*` method. The same split as [`binding`](crate::binding) has for the Panel: the
//! controller never touches a window, and this file is the only place that knows both.
//!
//! The Panel and the Main window read one library and share one toast, so after any gesture
//! every bound window is redrawn from the controller (see `binding::redraw_all`).
//!
//! Every control in the window is wired. "New item" and Edit open the item editor, which has
//! its own window ([`editor_binding`](crate::editor_binding)); the Recent section lists the inbox's entries.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{
    CloseRequestResponse, ComponentHandle, Global, Model, SharedString,
    winit_030::{WinitWindowAccessor, winit},
};
use snipdeck_core::{ItemKind, ItemLabel, ListAccent, ListIcon, Sort, SortKey};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};
use snipdeck_ui as ui;

use crate::{
    accent::Accent,
    binding::{
        Loggable, Payload, Shown, arm_toast, index, item_kind_of, model, redraw_all,
        register_redraw, to_i32,
    },
    controller::{
        BoardMenuEntry, Controller, Effect, HelpTopic, Library, MainAction, MainScreen, ToastPhase,
    },
    host::ClipboardText,
    item_editor::Origin,
    main_window::{EditorKind, EditorView, FilterTerm, Key, Section, SelectMode, ViewMode},
    native_window::{
        fit_logical, fit_when_settled, place_of, place_over_owner, restore_place, size_before_show,
    },
    palette::{list_color, list_icon_tint},
    popup_window::set_look_globals,
    settings::Theme,
    wording,
};

thread_local! {
    /// How many regular (activated, in the taskbar) windows are being shown right now.
    static REGULAR: Cell<usize> = const { Cell::new(0) };
}

/// Held while a Main window, Settings page or editor is *created*. The host makes its windows
/// un-activated and out of the taskbar for the Panel's sake (ADR-0003); a window made under this
/// guard is an ordinary one, which [`regular_window_being_shown`] tells the window-attributes
/// hook. Slint runs that hook inside `X::new()` and bakes the result into the window, so the
/// guard has to cover the constructor, not the later `show()`: use [`RegularWindow::create`].
#[derive(Debug)]
pub struct RegularWindow(());

impl RegularWindow {
    /// Runs `make` (a window constructor such as `ui::MainWindow::new`) as a regular window.
    pub fn create<T>(make: impl FnOnce() -> T) -> T {
        REGULAR.with(|n| n.set(n.get() + 1));
        let _guard = Self(());
        make()
    }
}

/// Makes a regular window with `make` (see [`RegularWindow::create`]) and starts following its
/// focus, so the Panel can step aside for it (`Controller::set_own_window_active`).
///
/// # Errors
///
/// What `make` returns.
pub fn create_window<T: ComponentHandle>(
    make: impl FnOnce() -> Result<T, slint::PlatformError>,
) -> Result<T, slint::PlatformError> {
    let window = RegularWindow::create(make)?;
    let name = std::any::type_name::<T>();
    crate::native_window::track_focus(window.window(), name.rsplit("::").next().unwrap_or(name));
    Ok(window)
}

impl Drop for RegularWindow {
    fn drop(&mut self) {
        REGULAR.with(|n| n.set(n.get().saturating_sub(1)));
    }
}

/// Whether the window being created now is a regular one: Main, Settings or an editor (see
/// [`RegularWindow`]).
#[must_use]
pub fn regular_window_being_shown() -> bool {
    REGULAR.with(|n| n.get() > 0)
}

/// The width an editor window opens at (logical pixels).
const EDITOR_WIDTH: f32 = 460.0;

/// Sizes an editor window to its content: the height `wanted` reports once the window has its
/// width. Slint applies `preferred-height` only when a window is first shown, and the editors
/// are reused, so the host sizes them every time they open.
fn size_to_content(window: &slint::Window, wanted: impl Fn() -> f32) {
    // The form's height depends on its width (a notice wraps), so give it the width first.
    window.set_size(slint::LogicalSize::new(EDITOR_WIDTH, 300.0));
    // The window is hidden and was filled just now: its `if` rows (the last-board notice) are only
    // created by a layout pass, so run one before reading the height.
    ui::layout::force_layout(window);
    let (w, h) = fit_logical((EDITOR_WIDTH, wanted()));
    window.set_size(slint::LogicalSize::new(w, h));
}

/// The list and board editor windows, made on first use and reused (hidden between uses).
#[derive(Default)]
struct Editors {
    list: Option<ui::ListEditor>,
    board: Option<ui::BoardEditor>,
    /// The editor whose fields the windows hold.
    seq: u64,
}

struct MainCore<S, C: Clipboard, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    window: slint::Weak<ui::MainWindow>,
    /// What the window shows now, so a redraw sets only what changed.
    shown: RefCell<Option<MainScreen>>,
    editors: RefCell<Editors>,
    /// The Tag manager window, made the first time it is asked for.
    tag_manager: RefCell<Option<ui::TagManager>>,
    /// Set once the window has been on screen: from then on a hidden Main is not drawn.
    been_shown: std::cell::Cell<bool>,
    /// Redraws this window (registered with `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
    /// Redraws the window when only the toast changed (registered with
    /// [`register_toast_redraw`](crate::binding::register_toast_redraw)).
    toast_redraw: Rc<dyn Fn()>,
}

/// A bound Main window. Dropping it does not unbind the window; the window's callbacks keep
/// the binding alive.
pub struct MainBinding<S, C: Clipboard, K, T> {
    core: Rc<MainCore<S, C, K, T>>,
}

impl<S, C: Clipboard, K, T> std::fmt::Debug for MainBinding<S, C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainBinding").finish_non_exhaustive()
    }
}

impl<S, C, K, T> MainBinding<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// The list editor window, if one has been opened.
    #[must_use]
    pub fn list_editor(&self) -> Option<ui::ListEditor> {
        self.core.editors.borrow().list.as_ref().map(clone_handle)
    }

    /// The board editor window, if one has been opened.
    #[must_use]
    pub fn board_editor(&self) -> Option<ui::BoardEditor> {
        self.core.editors.borrow().board.as_ref().map(clone_handle)
    }

    /// The Tag manager window, if one has been opened.
    #[must_use]
    pub fn tag_manager(&self) -> Option<ui::TagManager> {
        self.core.tag_manager.borrow().as_ref().map(clone_handle)
    }

    /// Redraws the window from the controller.
    pub fn redraw(&self) {
        self.core.sync();
    }
}

fn clone_handle<C: ComponentHandle>(c: &C) -> C {
    c.clone_strong()
}

impl<S, C, K, T> MainCore<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Runs `f` on the controller, then redraws every window.
    fn act<R>(self: &Rc<Self>, f: impl FnOnce(&mut Controller<S, C, K, T>) -> R) -> R {
        let mut ctl = self.controller.borrow_mut();
        // The editors for a new list or board start from words in the language shown now.
        if let Some(window) = self.window.upgrade() {
            ctl.set_starter_words(&crate::wording::starter_words(&window));
            let words = window.global::<ui::CommandTexts<'_>>();
            ctl.set_copy_title(&words.invoke_copy_title("{title}".into()));
        }
        let result = f(&mut ctl);
        drop(ctl);
        redraw_all();
        result
    }

    /// Fills the command palette with the commands that match `query`.
    /// Help > Recent messages: the last toasts, one a line, newest first, in the language shown.
    fn fill_recent_messages(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let lines: Vec<String> = self
            .controller
            .borrow()
            .recent_toasts()
            .iter()
            .map(|message| wording::toast_text(&window, message))
            .collect();
        window.set_recent_messages_text(lines.join("\n\n").into());
    }

    fn fill_palette(&self, query: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let ctl = self.controller.borrow();
        crate::command_binding::fill(
            &window,
            &ctl.settings().hotkeys,
            self.shown.borrow().as_ref(),
            crate::command_binding::Offers {
                symbols: ctl.settings().show_symbols_tab,
                colours: ctl.settings().show_colours_tab,
                eyedropper: ctl.eyedropper_state().0,
                saved_colours: ctl.has_saved_list(),
                panel_shown: ctl.visible(),
                keep_open: ctl.keeps_open(),
                can_undo: ctl.history_view().undo.is_some(),
                can_redo: ctl.history_view().redo.is_some(),
                lock_set: ctl.master_password_set(),
                panel_item: ctl.palette_item_pinned(),
            },
            &ctl.palette_places(),
            query,
        );
    }

    /// The palette ran command `id`: Search moves the window's own focus, Tour opens the tour
    /// window, every other command is the controller's.
    fn run_command(self: &Rc<Self>, id: i32) {
        // A board, list or tag row: the places are read again in the order the rows were made.
        if let Some(i) = crate::commands::place_index(id) {
            let place = self.controller.borrow().palette_places().into_iter().nth(i);
            if let Some(place) = place {
                self.act(|ctl| ctl.main_go(&place));
            }
            return;
        }
        let Some(command) = crate::command_binding::command_of(id) else {
            return;
        };
        match command {
            crate::commands::Command::Search => {
                if let Some(window) = self.window.upgrade() {
                    window.invoke_focus_search();
                }
                return;
            }
            // The tour has its own window: the same opener as Help's "Show the welcome tour again".
            crate::commands::Command::Tour => {
                crate::onboarding_binding::request_open();
                return;
            }
            // The Trash opens first, then the same confirmation as its button asks.
            crate::commands::Command::EmptyTrash => {
                self.act(|ctl| ctl.run_command(command, 0));
                if let Some(window) = self.window.upgrade() {
                    window.invoke_ask_empty_trash();
                }
                return;
            }
            // The nesting chooser is the window's own too, and opens over the same header.
            crate::commands::Command::NestGroup => {
                let cursor = self.shown.borrow().as_ref().map_or(0, |s| s.cursor);
                if let Some(window) = self.window.upgrade() {
                    window.invoke_open_group_nest(to_i32(cursor));
                }
                return;
            }
            // The colour dialog is the window's own: it opens over the header under the cursor.
            crate::commands::Command::ColourGroup => {
                let cursor = self.shown.borrow().as_ref().map_or(0, |s| s.cursor);
                if let Some(window) = self.window.upgrade() {
                    window.invoke_open_group_colour(to_i32(cursor));
                }
                return;
            }
            _ => {}
        }
        let cursor = self.shown.borrow().as_ref().map_or(0, |s| s.cursor);
        self.act(|ctl| ctl.run_command(command, cursor));
    }

    /// A toast tick: draws only if the toast differs from the one the window shows.
    fn toast_changed(self: &Rc<Self>) {
        let now = self.controller.borrow().toast_view();
        let same = self
            .shown
            .borrow()
            .as_ref()
            .is_some_and(|screen| screen.toast == now);
        if !same {
            self.sync();
        }
    }

    /// Copies the controller's screen into the window and the editors, and keeps the toast
    /// timer running.
    fn sync(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        // Nothing is drawn for a window nobody can see: the show path draws once it is visible.
        if !self.anything_visible(&window) {
            return;
        }
        let screen = self.controller.borrow_mut().main_screen();
        render(&window, self.shown.borrow().as_ref(), &screen);
        window.set_trash_days(crate::binding::limit(
            self.controller.borrow().settings().trash_days,
        ));
        if let Some(toast) = &screen.toast
            && self.controller.borrow_mut().claim_toast_timer(
                toast.seq,
                wording::toast_text(&window, &toast.message).chars().count(),
            )
        {
            arm_toast(
                Rc::downgrade(&self.controller),
                toast.seq,
                crate::controller::TOAST_ENTER,
            );
        }
        self.sync_editors(&screen);
        self.sync_tag_manager(&screen, false);
        crate::binding::arm_paste(&self.controller);
        *self.shown.borrow_mut() = Some(screen);
    }

    /// Whether Main, its Tag manager or one of its editors is on screen.
    fn anything_visible(&self, window: &ui::MainWindow) -> bool {
        let editors = self.editors.borrow();
        if window.window().is_visible() {
            self.been_shown.set(true);
            return true;
        }
        !self.been_shown.get()
            || self
                .tag_manager
                .borrow()
                .as_ref()
                .is_some_and(|w| w.window().is_visible())
            || editors
                .list
                .as_ref()
                .is_some_and(|w| w.window().is_visible())
            || editors
                .board
                .as_ref()
                .is_some_and(|w| w.window().is_visible())
    }

    /// Copies the tags into the Tag manager if it exists, so a delete, an Undo or a rename made
    /// elsewhere shows there too. Without `force` an unchanged list is left alone (setting a
    /// model again would reset the list's scroll).
    fn sync_tag_manager(&self, screen: &MainScreen, force: bool) {
        let Some(window) = self.tag_manager.borrow().as_ref().map(clone_handle) else {
            return;
        };
        let changed = force
            || self
                .shown
                .borrow()
                .as_ref()
                .is_none_or(|p| p.tags != screen.tags || p.dark != screen.dark);
        if !changed {
            return;
        }
        window.set_tags(model(screen.tags.iter().map(|t| ui::ChoiceEntry {
            label: t.name.as_str().into(),
            count: to_i32(t.count),
            color: slint::Color::default(),
            icon: slint::Image::default(),
            checked: false,
        })));
        window.set_colours(model(
            screen
                .tags
                .iter()
                .map(|t| crate::binding::label_index(t.colour)),
        ));
        window.set_depths(model(screen.tags.iter().map(|t| to_i32(t.depth))));
        window.set_paths(model(screen.tags.iter().map(|t| t.path.as_str().into())));
        window.set_own_counts(model(screen.tags.iter().map(|t| to_i32(t.own_count))));
        window.set_dark(screen.dark);
        apply_look(&window, screen);
    }

    /// The Tag manager window, made on first use (`None` if the windowing system cannot).
    fn tag_manager_window(self: &Rc<Self>) -> Option<ui::TagManager> {
        if let Some(w) = self.tag_manager.borrow().as_ref() {
            return Some(w.clone_strong());
        }
        let w = create_window(ui::TagManager::new).ok()?;
        let (core, weak) = (Rc::clone(self), w.as_weak());
        w.on_rename_requested(move |i, name| {
            crate::trace!(
                "callback tags rename_requested ({i}, <{} chars>)",
                name.chars().count()
            );
            let result = core.act(|ctl| ctl.main_rename_tag(index(i), name.as_str()));
            if let Some(w) = weak.upgrade() {
                match result {
                    Ok(()) => {
                        w.set_rename_error(SharedString::new());
                        w.invoke_finish_rename();
                    }
                    // A name that is taken offers the merge into that tag: the dialog is the
                    // message, so no error line shows beside it.
                    Err(crate::controller::TagRenameError::Exists) => {
                        w.set_rename_error(SharedString::new());
                        w.set_merge_into(name.trim().into());
                    }
                    Err(error) => {
                        w.set_rename_error(wording::tag_rename_error_text(&w, &error).into());
                    }
                }
            }
        });
        let (core, weak) = (Rc::clone(self), w.as_weak());
        w.on_create_requested(move |name| {
            crate::trace!(
                "callback tags create_requested (<{} chars>)",
                name.chars().count()
            );
            let result = core.act(|ctl| ctl.main_new_tag(name.as_str()));
            if let Some(w) = weak.upgrade() {
                match result {
                    Ok(()) => {
                        w.set_new_error(SharedString::new());
                        w.invoke_finish_create();
                    }
                    Err(error) => {
                        w.set_new_error(wording::tag_rename_error_text(&w, &error).into());
                    }
                }
            }
        });
        let core = Rc::clone(self);
        w.on_colour_chosen(move |i, accent| {
            crate::trace!("callback tags colour_chosen ({i}, {accent})");
            core.act(|ctl| ctl.main_set_tag_colour(index(i), crate::binding::label_of(accent)));
        });
        let (core, weak) = (Rc::clone(self), w.as_weak());
        w.on_merge_confirmed(move |i, into| {
            crate::trace!("callback tags merge_confirmed ({i})");
            let result = core.act(|ctl| ctl.main_merge_tag(index(i), into.as_str()));
            if let Some(w) = weak.upgrade() {
                if result.is_ok() {
                    w.set_rename_error(SharedString::new());
                    w.invoke_finish_rename();
                } else {
                    w.set_merge_into(SharedString::new());
                }
            }
        });
        let (core, weak) = (Rc::clone(self), w.as_weak());
        w.on_nest_requested(move |i| {
            crate::trace!("callback tags nest_requested ({i})");
            let screen = core.controller.borrow_mut().main_screen();
            let places = nest_places(&screen.tags, index(i));
            if let Some(w) = weak.upgrade() {
                w.set_nest_labels(model(places.iter().map(|p| p.0.as_str().into())));
                w.set_nest_targets(model(places.iter().map(|p| p.1)));
                w.set_nest_index(i);
            }
        });
        let core = Rc::clone(self);
        w.on_nest_chosen(move |i, under| {
            crate::trace!("callback tags nest_chosen ({i}, {under})");
            core.act(|ctl| ctl.main_nest_tag(index(i), usize::try_from(under).ok()));
        });
        let core = Rc::clone(self);
        w.on_delete_confirmed(move |i| {
            crate::trace!("callback tags delete_confirmed ({i})");
            core.act(|ctl| ctl.main_delete_tag(index(i)));
        });
        let weak = w.as_weak();
        w.on_close_requested(move || {
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        });
        // Closes with the window it was opened from.
        let weak = w.as_weak();
        crate::ownership::register_closer(crate::ownership::Win::TagManager, move || {
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
        });
        *self.tag_manager.borrow_mut() = Some(w.clone_strong());
        Some(w)
    }

    /// Shows the Tag manager with the current tags, in front of the Main window.
    fn open_tag_manager(self: &Rc<Self>) {
        let Some(window) = self.tag_manager_window() else {
            crate::trace!("cannot create the tag manager");
            return;
        };
        let screen = self.controller.borrow_mut().main_screen();
        self.sync_tag_manager(&screen, true);
        window.set_rename_error(SharedString::new());
        window.set_new_error(SharedString::new());
        window.invoke_finish_rename();
        window.invoke_finish_create();
        place_over_owner(window.window());
        let _ = window.show();
        crate::ownership::opened(crate::ownership::Win::TagManager);
        fit_when_settled(&window);
        crate::native_window::activate_regular(&window);
    }

    /// Shows, updates or hides the list or board editor to match `screen.editor`.
    fn sync_editors(self: &Rc<Self>, screen: &MainScreen) {
        let trash_days = crate::binding::limit(self.controller.borrow().settings().trash_days);
        let Some(view) = &screen.editor else {
            let editors = self.editors.borrow();
            if let Some(w) = &editors.list {
                let _ = w.hide();
            }
            if let Some(w) = &editors.board {
                let _ = w.hide();
            }
            return;
        };
        let fresh = self.editors.borrow().seq != view.seq;
        match view.kind {
            EditorKind::List => {
                let Some(window) = self.list_editor_window() else {
                    return;
                };
                if let Some(w) = self.editors.borrow().board.as_ref() {
                    let _ = w.hide();
                }
                apply_look(&window, screen);
                window.set_dark(screen.dark);
                window.set_trash_days(trash_days);
                window.set_name_error(error_text(&window, view));
                if fresh {
                    fill_list_editor(&window, view);
                    size_to_content(window.window(), || window.get_wanted_height());
                    place_over_owner(window.window());
                    let _ = window.show();
                    crate::ownership::opened(crate::ownership::Win::ListEditor);
                    fit_when_settled(&window);
                    // Shown again after a close, the window would stay behind the Main
                    // window (it was created un-activated): bring it up.
                    crate::native_window::activate_regular(&window);
                    window.invoke_focus_name();
                }
            }
            EditorKind::Board => {
                let Some(window) = self.board_editor_window() else {
                    return;
                };
                if let Some(w) = self.editors.borrow().list.as_ref() {
                    let _ = w.hide();
                }
                apply_look(&window, screen);
                window.set_dark(screen.dark);
                window.set_trash_days(trash_days);
                window.set_name_error(error_text(&window, view));
                if fresh {
                    fill_board_editor(&window, view);
                    size_to_content(window.window(), || window.get_wanted_height());
                    place_over_owner(window.window());
                    let _ = window.show();
                    crate::ownership::opened(crate::ownership::Win::BoardEditor);
                    fit_when_settled(&window);
                    // Shown again after a close, the window would stay behind the Main
                    // window (it was created un-activated): bring it up.
                    crate::native_window::activate_regular(&window);
                    window.invoke_focus_name();
                }
            }
        }
        self.editors.borrow_mut().seq = view.seq;
    }

    /// The list editor window, made on first use (`None` if the windowing system cannot).
    fn list_editor_window(self: &Rc<Self>) -> Option<ui::ListEditor> {
        if let Some(w) = self.editors.borrow().list.as_ref() {
            return Some(w.clone_strong());
        }
        let w = create_window(ui::ListEditor::new).ok()?;
        let core = Rc::clone(self);
        let weak = w.as_weak();
        w.on_save_requested(move || {
            if let Some(w) = weak.upgrade() {
                let (name, accent, icon, sort, board) = (
                    w.get_name(),
                    accent_of(w.get_accent()),
                    icon_of(w.get_icon_index()),
                    sort_of(w.get_sort_index(), false),
                    index(w.get_board_index()),
                );
                core.act(|ctl| {
                    ctl.main_editor_set_board(board);
                    ctl.main_editor_save(name.as_str(), accent, icon, sort);
                });
            }
        });
        let help = Rc::clone(self);
        w.on_help_requested(move || help.act(|ctl| ctl.main_open_help(HelpTopic::Guide)));
        self.wire_editor_common(
            |f| w.on_cancel_requested(f),
            |f| w.on_delete_confirmed(f),
            |f| w.on_edited(f),
        );
        let core = Rc::clone(self);
        w.window().on_close_requested(move || {
            core.act(Controller::main_editor_close);
            CloseRequestResponse::HideWindow
        });
        let (core, weak) = (Rc::clone(self), w.as_weak());
        crate::ownership::register_closer(crate::ownership::Win::ListEditor, move || {
            if let Some(w) = weak.upgrade() {
                let (name, accent, icon, sort, board) = (
                    w.get_name(),
                    accent_of(w.get_accent()),
                    icon_of(w.get_icon_index()),
                    sort_of(w.get_sort_index(), false),
                    index(w.get_board_index()),
                );
                core.act(|ctl| ctl.main_editor_park(name.as_str(), accent, icon, sort, board));
            }
        });
        self.editors.borrow_mut().list = Some(w.clone_strong());
        Some(w)
    }

    /// The board editor window, made on first use (`None` if the windowing system cannot).
    fn board_editor_window(self: &Rc<Self>) -> Option<ui::BoardEditor> {
        if let Some(w) = self.editors.borrow().board.as_ref() {
            return Some(w.clone_strong());
        }
        let w = create_window(ui::BoardEditor::new).ok()?;
        let core = Rc::clone(self);
        let weak = w.as_weak();
        w.on_save_requested(move || {
            if let Some(w) = weak.upgrade() {
                let (name, accent, icon) = (
                    w.get_name(),
                    accent_of(w.get_accent()),
                    icon_of(w.get_icon_index()),
                );
                core.act(|ctl| ctl.main_editor_save(name.as_str(), accent, icon, Sort::MANUAL));
            }
        });
        let help = Rc::clone(self);
        w.on_help_requested(move || help.act(|ctl| ctl.main_open_help(HelpTopic::Guide)));
        self.wire_editor_common(
            |f| w.on_cancel_requested(f),
            |f| w.on_delete_confirmed(f),
            |f| w.on_edited(f),
        );
        let core = Rc::clone(self);
        w.window().on_close_requested(move || {
            core.act(Controller::main_editor_close);
            CloseRequestResponse::HideWindow
        });
        let (core, weak) = (Rc::clone(self), w.as_weak());
        crate::ownership::register_closer(crate::ownership::Win::BoardEditor, move || {
            if let Some(w) = weak.upgrade() {
                let (name, accent, icon) = (
                    w.get_name(),
                    accent_of(w.get_accent()),
                    icon_of(w.get_icon_index()),
                );
                core.act(|ctl| {
                    ctl.main_editor_park(name.as_str(), accent, icon, Sort::MANUAL, 0);
                });
            }
        });
        self.editors.borrow_mut().board = Some(w.clone_strong());
        Some(w)
    }

    /// The callbacks both editors share: cancel, a confirmed delete, and an edit.
    fn wire_editor_common(
        self: &Rc<Self>,
        cancel: impl FnOnce(Box<dyn Fn()>),
        delete: impl FnOnce(Box<dyn Fn()>),
        edited: impl FnOnce(Box<dyn Fn()>),
    ) {
        let core = Rc::clone(self);
        cancel(Box::new(move || {
            crate::trace!("callback editor cancel-requested");
            core.act(Controller::main_editor_close);
        }));
        let core = Rc::clone(self);
        delete(Box::new(move || {
            crate::trace!("callback editor delete-confirmed");
            core.act(Controller::main_editor_delete);
        }));
        let core = Rc::clone(self);
        edited(Box::new(move || {
            // Typing only clears a refusal; nothing else on screen changes.
            core.controller.borrow_mut().main_editor_edited();
            core.sync();
        }));
    }
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Draws the controller's Main screen in `window` and routes the window's gestures (and
    /// those of the list and board editors it opens) to it.
    ///
    #[allow(
        clippy::too_many_lines,
        reason = "one flat list of the window's callbacks, each a single line"
    )]
    pub fn bind_main(this: &Rc<RefCell<Self>>, window: &ui::MainWindow) -> MainBinding<S, C, K, T> {
        let core = Rc::new_cyclic(|weak: &Weak<MainCore<S, C, K, T>>| {
            let weak = weak.clone();
            let toast_weak = weak.clone();
            MainCore {
                controller: Rc::clone(this),
                window: window.as_weak(),
                shown: RefCell::new(None),
                editors: RefCell::new(Editors::default()),
                tag_manager: RefCell::new(None),
                been_shown: std::cell::Cell::new(false),
                toast_redraw: Rc::new(move || {
                    if let Some(core) = toast_weak.upgrade() {
                        core.toast_changed();
                    }
                }),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
            }
        });
        register_redraw(&core.redraw);
        crate::binding::register_toast_redraw(&core.redraw, &core.toast_redraw);
        // The OS draws the title bar and its caption buttons until the client-area chrome of
        // ADR-0008 exists, so the toolbar reserves nothing for them.
        window.global::<ui::SystemLook<'_>>().set_caption_inset(0.0);
        core.sync();

        macro_rules! wire {
            ($on:ident, |$ctl:ident $(, $arg:ident)*| $body:expr) => {{
                let core = Rc::clone(&core);
                window.$on(move |$($arg),*| {
                    crate::trace!("callback main {}{}", stringify!($on), Shown(&[$(&$arg as &dyn Loggable),*]));
                    crate::native_window::note_opener(crate::native_window::Surface::Main);
                    core.act(|$ctl| {
                        let _ = $body;
                    });
                });
            }};
        }

        wire!(on_query_changed, |ctl, q| ctl.main_set_query(q.as_str()));
        crate::wire_suggestions!(window, core.controller, main_suggestions);
        {
            let core2 = Rc::clone(&core);
            window.on_section_selected(move |kind, i| {
                crate::trace!("callback main section_selected ({kind:?}, {i})");
                let section = core2
                    .shown
                    .borrow()
                    .as_ref()
                    .and_then(|screen| section_of(screen, kind, index(i)));
                if let Some(section) = section {
                    core2.act(|ctl| ctl.main_open(section));
                }
            });
        }
        wire!(on_label_filter_requested, |ctl, i| ctl
            .main_set_label_filter(crate::binding::label_of(i)));
        wire!(on_view_changed, |ctl, v| ctl.main_set_view(view_of(v)));
        wire!(on_theme_chosen, |ctl, i| ctl.choose_theme(theme_of(i)));
        wire!(on_sort_chosen, |ctl, i, reversed| ctl
            .main_set_sort(sort_of(i, reversed)));
        wire!(on_sidebar_width_changed, |ctl, w| ctl
            .set_main_sidebar_width(u32::try_from(w).unwrap_or(0)));
        wire!(on_sidebar_collapsed_changed, |ctl, c| ctl
            .set_main_sidebar_collapsed(c));
        wire!(on_detail_open_changed, |ctl, o| ctl.set_main_detail_open(o));
        {
            // Rows on screen that want a thumbnail: a few are decoded per tick, then the window draws.
            let wanted = Rc::new(RefCell::new(Vec::<usize>::new()));
            let core2 = Rc::clone(&core);
            window.on_thumb_wanted(move |i| {
                let at = index(i);
                let queued = {
                    let mut queue = wanted.borrow_mut();
                    if queue.contains(&at) {
                        false
                    } else {
                        queue.push(at);
                        queue.len() == 1
                    }
                };
                if queued {
                    decode_thumbs_later(Rc::clone(&core2), Rc::clone(&wanted));
                }
            });
        }
        wire!(on_select_requested, |ctl, i, mode| ctl
            .main_select(index(i), select_mode(mode)));
        wire!(on_select_all_requested, |ctl| ctl.main_select_all());
        wire!(on_clear_selection_requested, |ctl| ctl
            .main_clear_selection());
        wire!(on_recent_copied, |ctl, id| ctl.recent_copy(i64::from(id)));
        wire!(on_recent_deleted, |ctl, id| ctl
            .recent_delete(i64::from(id)));
        wire!(on_recent_saved, |ctl, id, target| ctl
            .recent_promote_to(i64::from(id), index(target)));
        wire!(on_recent_saved_sensitive, |ctl, id, target| ctl
            .recent_promote_sensitive_to(i64::from(id), index(target)));
        wire!(on_recent_cleared, |ctl| ctl.recent_clear());
        wire!(on_recent_unlock_requested, |ctl| ctl.request_unlock());
        wire!(on_item_action, |ctl, action, i| ctl
            .main_action(action_of(action), index(i)));
        wire!(on_move_chosen, |ctl, i| ctl.main_move_to(index(i)));
        wire!(on_paste_as_chosen, |ctl, i| ctl.main_paste_as(index(i)));
        wire!(on_colour_copied, |ctl, row, format| ctl
            .main_colour_copy(index(row), index(format)));
        wire!(on_colour_chosen, |ctl, format, copy| ctl
            .main_colour_paste_as(index(format), copy));
        wire!(on_colour_open_requested, |ctl, row| ctl
            .main_open_in_colours(index(row)));
        {
            // A colour row's "Paste as…" lists its formats; the window asks just before it opens
            // the chooser, so they are in place by then.
            let core = Rc::clone(&core);
            let weak = window.as_weak();
            window.on_colour_choices_requested(move |row| {
                let Some(window) = weak.upgrade() else {
                    return;
                };
                let choices = core.controller.borrow().main_colour_choices(index(row));
                let name = window.global::<ui::ColourTexts<'_>>().invoke_format_name();
                window.set_colour_choices(model(choices.iter().map(|(f, value)| {
                    ui::ChoiceEntry {
                        label: crate::colour_words::choice_text(*f, &name, value).into(),
                        count: -1,
                        ..Default::default()
                    }
                })));
            });
        }
        {
            let core2 = Rc::clone(&core);
            window.on_filter_toggled(move |code| {
                crate::trace!("callback main filter_toggled ({code})");
                if let Some(term) = filter_term_of(code) {
                    core2.act(|ctl| ctl.main_toggle_filter(term));
                }
            });
            let core3 = Rc::clone(&core);
            window.on_filter_chip_removed(move |i| {
                crate::trace!("callback main filter_chip_removed ({i})");
                let term = core3
                    .shown
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.filter_chips.get(index(i)).map(|c| c.term));
                if let Some(term) = term {
                    core3.act(|ctl| ctl.main_toggle_filter(term));
                }
            });
        }
        wire!(on_filter_tag_toggled, |ctl, i| ctl
            .main_toggle_filter_tag(index(i)));
        wire!(on_filter_match_all_changed, |ctl, all| ctl
            .main_set_filter_match_all(all));
        wire!(on_filter_cleared, |ctl| ctl.main_clear_filter());
        wire!(on_tag_toggled, |ctl, i, on| ctl
            .main_toggle_tag(index(i), on));
        wire!(on_reveal_requested, |ctl, i, on| ctl
            .main_reveal(index(i), on));
        wire!(on_add_list_requested, |ctl| ctl.main_new_list());
        wire!(on_list_edit_requested, |ctl, i| ctl
            .main_edit_list(index(i)));
        wire!(on_list_export_requested, |ctl, i| ctl.main_export_list(
            index(i),
            &crate::paths::documents_folder().unwrap_or_else(std::env::temp_dir)
        ));
        wire!(on_list_delete_confirmed, |ctl, i| ctl
            .main_delete_list(index(i)));
        wire!(on_tag_delete_confirmed, |ctl, i| ctl
            .main_delete_tag(index(i)));
        wire!(on_empty_trash_confirmed, |ctl| ctl.main_empty_trash());
        wire!(on_board_menu_requested, |ctl| ctl.main_board_menu_open());
        wire!(on_board_menu_activated, |ctl, i| ctl
            .main_board_menu_activate(index(i)));
        wire!(on_board_menu_dismissed, |ctl| ctl.main_board_menu_close());
        wire!(on_group_delete_confirmed, |ctl| ctl
            .main_group_delete_confirm());
        wire!(on_group_delete_dismissed, |ctl| ctl
            .main_group_delete_cancel());
        wire!(on_board_delete_confirmed, |ctl| ctl
            .main_board_delete_confirm());
        wire!(on_board_delete_dismissed, |ctl| ctl
            .main_board_delete_cancel());
        wire!(on_toast_undo, |ctl| ctl.toast_action());
        wire!(on_toast_dismissed, |ctl| ctl.toast_dismiss());
        wire!(on_toast_edit, |ctl| {
            for effect in ctl.toast_edit() {
                if let Effect::EditItem(id) = effect {
                    ctl.open_item_editor(id);
                }
            }
        });
        {
            let core = Rc::clone(&core);
            window.on_paste_create_requested(move || {
                crate::trace!("callback main on_paste_create_requested");
                crate::native_window::note_opener(crate::native_window::Surface::Main);
                let started = core.act(|ctl| ctl.paste_begin(crate::item_editor::Origin::Main));
                if let Some(run) = started {
                    read_run(&Rc::downgrade(&core.controller), run);
                }
            });
        }
        wire!(on_copy_selection_requested, |ctl, i| ctl
            .main_copy_selection(index(i)));
        wire!(on_undo_requested, |ctl| ctl.undo());
        wire!(on_redo_requested, |ctl| ctl.redo());
        {
            let core = Rc::clone(&core);
            window.on_toast_hover_changed(move |on| core.controller.borrow_mut().toast_hover(on));
        }
        {
            // Ctrl+W is the X button: the same decision between hiding and quitting.
            let weak = window.as_weak();
            window.on_close_requested(move || {
                if let Some(window) = weak.upgrade() {
                    window
                        .window()
                        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
                }
            });
        }
        window.on_settings_requested(|| {
            // Settings opens over the Main window the user is in.
            crate::native_window::note_opener(crate::native_window::Surface::Main);
            crate::settings_binding::request_open();
        });
        wire!(on_panel_requested, |ctl| ctl.main_toggle_panel());
        wire!(on_help_requested, |ctl, page| ctl.main_open_help(
            match page {
                ui::HelpPage::Troubleshoot => HelpTopic::Troubleshoot,
                ui::HelpPage::Guide => HelpTopic::Guide,
                ui::HelpPage::About => HelpTopic::About,
                ui::HelpPage::Shortcuts => HelpTopic::Shortcuts,
            }
        ));
        {
            let core = Rc::clone(&core);
            window.on_recent_messages_requested(move || core.fill_recent_messages());
        }
        {
            let core = Rc::clone(&core);
            window.on_palette_opened(move || {
                core.controller.borrow_mut().palette_opened();
                core.fill_palette("");
            });
        }
        {
            let core = Rc::clone(&core);
            window.on_palette_query_changed(move |query| core.fill_palette(query.as_str()));
        }
        {
            let core = Rc::clone(&core);
            window.on_command_requested(move |id| {
                crate::trace!("callback main command_requested ({id})");
                // A command that opens a window (Settings, an editor, the Picker) opens over Main.
                crate::native_window::note_opener(crate::native_window::Surface::Main);
                core.run_command(id);
            });
        }
        wire!(on_group_requested, |ctl, i| ctl.main_group_marked(index(i)));
        wire!(on_group_toggled, |ctl, i| ctl
            .main_group_collapse(index(i), None));
        wire!(on_group_collapsed_set, |ctl, i, c| ctl
            .main_group_collapse(index(i), Some(c)));
        wire!(on_group_renamed, |ctl, i, name| ctl
            .main_group_rename(index(i), name.as_str()));
        wire!(on_group_colour_chosen, |ctl, i, accent| ctl
            .main_group_colour(index(i), accent_of(accent)));
        wire!(on_group_rename_requested, |ctl, i| ctl
            .main_group_rename_start(index(i)));
        wire!(on_group_rename_ended, |ctl| ctl.main_group_rename_ended());
        wire!(on_group_move_requested, |ctl, i| ctl
            .main_group_move_start(index(i)));
        wire!(on_group_ungroup_requested, |ctl, i| ctl
            .main_group_ungroup(index(i)));
        wire!(on_group_nest_requested, |ctl, i| ctl
            .main_group_nest_start(index(i)));
        wire!(on_group_nest_chosen, |ctl, i| ctl
            .main_group_nest_chosen(index(i)));
        wire!(on_group_nest_dismissed, |ctl| ctl.main_group_nest_cancel());
        wire!(on_group_delete_requested, |ctl, i| ctl
            .main_group_delete(index(i)));
        wire!(on_item_reorder_requested, |ctl, d| ctl
            .main_reorder_items(isize::try_from(d).unwrap_or(0)));
        wire!(on_list_reorder_requested, |ctl, i, d| ctl
            .main_reorder_list(index(i), isize::try_from(d).unwrap_or(0)));
        wire!(on_row_dropped, |ctl, from, onto| ctl
            .main_drop_row(index(from), index(onto)));
        wire!(on_row_dropped_above, |ctl, from, onto| ctl
            .main_drop_row_above(index(from), index(onto)));
        wire!(on_row_dropped_on_list, |ctl, row, list| ctl
            .main_drop_on_list(index(row), index(list)));
        wire!(on_row_dropped_on_trash, |ctl, row| ctl
            .main_drop_on_trash(index(row)));
        wire!(on_row_dropped_on_tag, |ctl, row, tag| ctl
            .main_drop_on_tag(index(row), index(tag)));
        wire!(on_list_dropped, |ctl, from, onto| ctl
            .main_drop_list(index(from), index(onto)));
        {
            let core = Rc::clone(&core);
            window.on_row_payload(move |row| {
                core.shown
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.rows.get(index(row)))
                    .and_then(|r| row_payload(r.key))
                    .map_or_else(slint::DataTransfer::default, Payload::into_transfer)
            });
        }
        {
            let core = Rc::clone(&core);
            window.on_list_payload(move |list| {
                core.shown
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.lists.get(index(list)))
                    .map_or_else(slint::DataTransfer::default, |l| {
                        Payload::List(l.id).into_transfer()
                    })
            });
        }
        {
            let core = Rc::clone(&core);
            window.on_payload_row(move |data| {
                let Some(key) = Payload::of(&data).and_then(payload_key) else {
                    return -1;
                };
                core.shown.borrow().as_ref().map_or(-1, |s| {
                    s.rows.iter().position(|r| r.key == key).map_or(-1, to_i32)
                })
            });
        }
        {
            let core = Rc::clone(&core);
            window.on_payload_list(move |data| match Payload::of(&data) {
                Some(Payload::List(id)) => core.shown.borrow().as_ref().map_or(-1, |s| {
                    s.lists.iter().position(|l| l.id == id).map_or(-1, to_i32)
                }),
                _ => -1,
            });
        }
        wire_title_bar(window);
        wire!(on_new_item_requested, |ctl| ctl.open_new_item(Origin::Main));
        wire!(on_quick_add_requested, |ctl, text, kind, sensitive| ctl
            .quick_add(
                text.as_str(),
                Some(crate::binding::kind_of_index(kind)),
                sensitive,
                Origin::Main
            ));
        wire!(on_quick_add_full, |ctl, text| ctl
            .open_new_item_with(Origin::Main, text.as_str()));
        window.on_detect_kind(|text| crate::binding::detect_kind_index(text.as_str()));
        wire!(on_add_from_clipboard, |ctl| ctl.quick_capture());
        // Imports live in Settings (its Import row), so the empty deck's "Import…" opens it over Main.
        window.on_import_requested(|| {
            crate::native_window::note_opener(crate::native_window::Surface::Main);
            crate::settings_binding::request_open();
        });
        {
            // The X hides the window, or quits when nothing could bring it back (the setting
            // "Keep running in the tray" is off, or there is no tray icon and the Panel is down).
            let core = Rc::clone(&core);
            window.window().on_close_requested(move || {
                let quit = core.controller.borrow().main_window_closed();
                crate::trace!("main window close requested: quit {quit}");
                if quit {
                    let _ = slint::quit_event_loop();
                } else {
                    // Closing Main closes what was opened from it (O53): its own menus and
                    // dialogs, then the windows it opened. The Panel's stay.
                    if let Some(window) = core.window.upgrade() {
                        window.invoke_dismiss_overlays();
                    }
                    core.controller.borrow_mut().main_board_menu_close();
                    crate::ownership::owner_closed(crate::ownership::Win::Main);
                    redraw_all();
                }
                CloseRequestResponse::HideWindow
            });
        }
        {
            let core = Rc::clone(&core);
            window.on_manage_tags_requested(move || {
                crate::trace!("callback main manage_tags_requested");
                core.open_tag_manager();
            });
        }

        MainBinding { core }
    }
}

/// Tells the controller where the Main window is whenever it is moved or resized (it keeps the
/// place for the next start). Armed after the first placement, so the restore itself is not
/// taken for the user's choice.
fn watch_place<S, C, K, T>(
    window: &ui::MainWindow,
    controller: &Rc<RefCell<Controller<S, C, K, T>>>,
) where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    use slint::winit_030::{EventResult, winit::event::WindowEvent};
    let controller = Rc::downgrade(controller);
    // O39: Windows can drop or erase the window's pixels; the next frame is drawn whole then.
    let mut watch = crate::repaint::Watch::new();
    let batch = crate::drop_batch::DropBatch::default();
    let for_drops = controller.clone();
    window
        .window()
        .on_winit_window_event(move |slint_window, event| {
            crate::native_window::note_window_event("MainWindow", slint_window, event);
            // Files dragged in from Explorer (text and pictures need an OLE drop target: later).
            match event {
                WindowEvent::HoveredFile(_) | WindowEvent::HoveredFileCancelled => {
                    if let Some(c) = for_drops.upgrade()
                        && let Ok(mut c) = c.try_borrow_mut()
                    {
                        if matches!(event, WindowEvent::HoveredFile(_)) {
                            c.drop_hover(crate::item_editor::Origin::Main, None);
                        } else {
                            c.drop_end();
                        }
                    }
                    crate::binding::redraw_all();
                }
                WindowEvent::DroppedFile(path) => {
                    // The drag is over: the hint goes now, the items follow once the files of
                    // the drop have all arrived.
                    if let Some(c) = for_drops.upgrade()
                        && let Ok(mut c) = c.try_borrow_mut()
                    {
                        c.drop_end();
                    }
                    let controller = for_drops.clone();
                    batch.push(path.to_string_lossy().into_owned(), move |paths| {
                        let Some(c) = controller.upgrade() else {
                            return true;
                        };
                        let Ok(mut c) = c.try_borrow_mut() else {
                            return false;
                        };
                        let started = c.drop_begin(paths, crate::item_editor::Origin::Main, None);
                        drop(c);
                        crate::binding::redraw_all();
                        if let Some(run) = started {
                            read_run(&controller, run);
                        }
                        true
                    });
                }
                _ => {}
            }
            crate::repaint::follow(&mut watch, slint_window, event);
            if matches!(event, WindowEvent::Moved(_) | WindowEvent::Resized(_))
                && let Some(controller) = controller.upgrade()
                && let Ok(mut controller) = controller.try_borrow_mut()
            {
                let previous = controller.settings().main_window;
                let place = slint_window
                    .with_winit_window(|w| place_of(w, previous))
                    .flatten();
                if let Some(place) = place {
                    controller.remember_main_window(place);
                }
            }
            EventResult::Propagate
        });
}

/// The toolbar's empty parts start the OS window drag, a double click maximises or restores.
fn wire_title_bar(window: &ui::MainWindow) {
    let weak = window.as_weak();
    window.on_drag_requested(move || {
        if let Some(w) = weak.upgrade() {
            let _ = w
                .window()
                .with_winit_window(winit::window::Window::drag_window);
        }
    });
    let weak = window.as_weak();
    window.on_title_double_clicked(move || {
        if let Some(w) = weak.upgrade() {
            let _ = w
                .window()
                .with_winit_window(|w| w.set_maximized(!w.is_maximized()));
        }
    });
}

// ---- Opening the window on demand --------------------------------------------------------------

/// Creates the Main window the first time it is asked for, binds it, and shows and raises it
/// every time. Not generic: the host keeps one of these.
pub struct MainUi {
    open: Box<dyn Fn() -> Result<(), slint::PlatformError>>,
    bind: Box<dyn Fn() -> Result<(), slint::PlatformError>>,
    list_editor: Box<dyn Fn() -> Option<ui::ListEditor>>,
    window: Box<dyn Fn() -> Option<ui::MainWindow>>,
}

impl std::fmt::Debug for MainUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MainUi").finish_non_exhaustive()
    }
}

impl MainUi {
    /// A Main window for `controller`, not created yet.
    pub fn new<S, C, K, T>(controller: &Rc<RefCell<Controller<S, C, K, T>>>) -> Self
    where
        S: Library + 'static,
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        type Slot<S, C, K, T> = Rc<RefCell<Option<(ui::MainWindow, MainBinding<S, C, K, T>)>>>;
        let slot: Slot<S, C, K, T> = Rc::default();
        // Creates the window and its binding the first time, without showing it (the Panel's
        // list editor needs the binding, not the window).
        let ensure: Rc<dyn Fn() -> Result<(), slint::PlatformError>> = {
            let slot = Rc::clone(&slot);
            let controller = Rc::clone(controller);
            Rc::new(move || {
                let mut slot = slot.borrow_mut();
                if slot.is_none() {
                    let window = create_window(ui::MainWindow::new)?;
                    size_before_show(
                        window.window(),
                        (window.get_wanted_width(), window.get_wanted_height()),
                    );
                    let binding = Controller::bind_main(&controller, &window);
                    let weak = window.as_weak();
                    crate::native_window::register_surface(
                        crate::native_window::Surface::Main,
                        move || {
                            weak.upgrade()
                                .and_then(|w| crate::native_window::shown_frame(w.window()))
                        },
                    );
                    *slot = Some((window, binding));
                }
                Ok(())
            })
        };
        let placed = Rc::new(std::cell::Cell::new(false));
        let open = {
            let slot = Rc::clone(&slot);
            let controller = Rc::clone(controller);
            let ensure = Rc::clone(&ensure);
            Box::new(move || {
                ensure()?;
                let slot = slot.borrow();
                let Some((window, binding)) = slot.as_ref() else {
                    return Ok(());
                };
                // Reading the library again: it may have changed while the window was closed.
                window.show()?;
                binding.redraw();
                if !placed.replace(true) {
                    // Where the user left it (then cut to the screen below); only once, as a
                    // hidden window keeps its place.
                    let saved = controller.borrow().settings().main_window;
                    if let Some(place) = saved {
                        restore_place(window.window(), &place);
                    }
                    watch_place(window, &controller);
                }
                fit_when_settled(window);
                crate::native_window::activate_regular(window);
                Ok(())
            })
        };
        let bind = {
            let slot = Rc::clone(&slot);
            Box::new(move || {
                ensure()?;
                if let Some((_, binding)) = slot.borrow().as_ref() {
                    binding.redraw();
                }
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
        let list_editor = {
            let slot = Rc::clone(&slot);
            Box::new(move || {
                slot.borrow()
                    .as_ref()
                    .and_then(|(_, binding)| binding.list_editor())
            })
        };
        Self {
            open,
            bind,
            list_editor,
            window,
        }
    }

    /// Creates the window if this is the first time, then shows it and brings it to the front.
    ///
    /// # Errors
    ///
    /// The windowing system failed to create or show it.
    pub fn open(&self) -> Result<(), slint::PlatformError> {
        (self.open)()
    }

    /// Creates the window and its binding if needed without showing it, and redraws, so the
    /// list or board editor window the controller asked for appears (the Panel's list menu).
    ///
    /// # Errors
    ///
    /// The windowing system failed to create the window.
    pub fn bind(&self) -> Result<(), slint::PlatformError> {
        (self.bind)()
    }

    /// The list editor window, once the Main binding has opened it.
    #[must_use]
    pub fn list_editor(&self) -> Option<ui::ListEditor> {
        (self.list_editor)()
    }

    /// The window, once it has been created.
    #[must_use]
    pub fn window(&self) -> Option<ui::MainWindow> {
        (self.window)()
    }
}

// ---- Screen -> window --------------------------------------------------------------------------

/// The colour-label filter row.
fn set_label_filter(window: &ui::MainWindow, screen: &MainScreen) {
    window.set_label_chips(model(screen.label_chips.iter().map(|chip| {
        let (label, label_name) = crate::binding::label_fields(Some(chip.label), &|l| {
            wording::item_label_name(window, l)
        });
        ui::LabelChipEntry {
            label,
            name: label_name,
            count: to_i32(chip.count),
        }
    })));
    window.set_label_filter(crate::binding::label_index(screen.label_filter));
}

/// The Filter menu's row `code` (and a chip's `kind`): 0 to 5 an item kind in [`ItemKind::ALL`]
/// order, 6 has a trigger, 7 sensitive, 8 untagged.
fn filter_term_of(code: i32) -> Option<FilterTerm> {
    let code = usize::try_from(code).ok()?;
    match code {
        0..=5 => ItemKind::ALL.get(code).copied().map(FilterTerm::Kind),
        6 => Some(FilterTerm::HasTrigger),
        7 => Some(FilterTerm::Sensitive),
        8 => Some(FilterTerm::Untagged),
        _ => None,
    }
}

/// The code of a criterion for the window: see [`filter_term_of`]; 9 is a tag.
fn filter_code(term: FilterTerm) -> i32 {
    match term {
        FilterTerm::Kind(kind) => ItemKind::ALL
            .iter()
            .position(|k| *k == kind)
            .map_or(0, to_i32),
        FilterTerm::HasTrigger => 6,
        FilterTerm::Sensitive => 7,
        FilterTerm::Untagged => 8,
        FilterTerm::Tag(_) => 9,
    }
}

/// The Filter menu's ticks and the chips of the active criteria.
fn set_filter(window: &ui::MainWindow, screen: &MainScreen) {
    let mut checked = [false; 9];
    for chip in &screen.filter_chips {
        if let Some(slot) = usize::try_from(filter_code(chip.term))
            .ok()
            .and_then(|i| checked.get_mut(i))
        {
            *slot = true;
        }
    }
    window.set_filter_checked(model(checked.into_iter()));
    window.set_filter_chips(model(screen.filter_chips.iter().map(|chip| {
        ui::FilterChipEntry {
            kind: filter_code(chip.term),
            name: chip.name.as_str().into(),
        }
    })));
    window.set_filter_match_all(screen.filter_match_all);
    window.set_filter_tag_choices(model(screen.filter_tag_choices.iter().map(|t| {
        ui::ChoiceEntry {
            label: t.name.as_str().into(),
            count: to_i32(t.count),
            color: slint::Color::default(),
            icon: slint::Image::default(),
            checked: t.checked,
        }
    })));
}

/// The Colour block under the list: its swatch and the colour in every format.
fn set_colour_block(window: &ui::MainWindow, block: Option<&crate::controller::ColourBlock>) {
    window.set_colour_block_shown(block.is_some());
    let Some(block) = block else {
        return;
    };
    let c = block.colour;
    window.set_colour_block_swatch(slint::Color::from_argb_u8(c.a, c.r, c.g, c.b));
    let name = window.global::<ui::ColourTexts<'_>>().invoke_format_name();
    window.set_colour_block_formats(model(block.formats.iter().map(|(f, value)| {
        ui::ColourFormatCell {
            label: crate::colour_words::format_word(*f, &name),
            value: value.as_str().into(),
        }
    })));
}

/// The sidebar's lists, with their icons and the colour that tints each (transparent where the list
/// colour would not read against the rows: the default icon colour then).
fn set_side_lists(window: &ui::MainWindow, screen: &MainScreen) {
    let icons = ui::ListIcons::get(window).get_all();
    window.set_lists(model(screen.lists.iter().map(|l| ui::ChoiceEntry {
        label: l.name.as_str().into(),
        count: to_i32(l.count),
        color: list_icon_tint(l.accent, screen.dark),
        icon: crate::binding::list_icon_image(&icons, l.icon),
        checked: false,
    })));
}

#[allow(clippy::too_many_lines, reason = "one flat screen-to-window diff")]
fn render(window: &ui::MainWindow, previous: Option<&MainScreen>, screen: &MainScreen) {
    let differs = |same: &dyn Fn(&MainScreen) -> bool| previous.is_none_or(|p| !same(p));

    if differs(&|p| p.dark == screen.dark) {
        window.set_dark(screen.dark);
    }
    // Not behind `differs`: the text size is not part of the screen.
    crate::text_scale::apply(&window.global::<ui::SystemLook<'_>>());
    if differs(&|p| look_key(p) == look_key(screen)) {
        apply_look(window, screen);
    }
    window.set_colour_can_open(screen.colours_tab);
    if differs(&|p| p.colour_block == screen.colour_block) {
        set_colour_block(window, screen.colour_block.as_ref());
    }
    if differs(&|p| p.panel_hotkey == screen.panel_hotkey) {
        window.set_panel_hotkey(screen.panel_hotkey.as_str().into());
    }
    if differs(&|p| p.panel_visible == screen.panel_visible) {
        window.set_panel_visible(screen.panel_visible);
    }
    if differs(&|p| p.board_name == screen.board_name) {
        window.set_board_name(screen.board_name.as_str().into());
    }
    if differs(&|p| p.board_menu == screen.board_menu) {
        let entries = screen
            .board_menu
            .iter()
            .flatten()
            .map(|entry| menu_entry(window, entry));
        window.set_board_menu_entries(model(entries));
        window.set_board_menu_open(screen.board_menu.is_some());
    }
    if differs(&|p| p.board_delete == screen.board_delete)
        || window.get_board_delete_open() != screen.board_delete.is_some()
    {
        if let Some(count) = screen.board_delete {
            window.set_board_delete_count(i32::try_from(count).unwrap_or(i32::MAX));
        }
        window.set_board_delete_open(screen.board_delete.is_some());
    }
    if differs(&|p| p.group_delete == screen.group_delete)
        || window.get_group_delete_open() != screen.group_delete.is_some()
    {
        if let Some((name, count)) = &screen.group_delete {
            window.set_group_delete_name(name.as_str().into());
            window.set_group_delete_count(to_i32(*count));
        }
        window.set_group_delete_open(screen.group_delete.is_some());
    }
    if differs(&|p| p.lists == screen.lists && p.dark == screen.dark) {
        set_side_lists(window, screen);
    }
    if differs(&|p| p.move_targets == screen.move_targets && p.dark == screen.dark) {
        let icons = ui::ListIcons::get(window).get_all();
        window.set_move_choices(model(screen.move_targets.iter().map(|l| ui::ChoiceEntry {
            label: l.label.as_str().into(),
            count: to_i32(l.count),
            color: list_icon_tint(l.accent, screen.dark),
            icon: crate::binding::list_icon_image(&icons, l.icon),
            checked: false,
        })));
    }
    if differs(&|p| p.label_chips == screen.label_chips && p.label_filter == screen.label_filter) {
        set_label_filter(window, screen);
    }
    if differs(&|p| {
        p.filter_chips == screen.filter_chips
            && p.filter_match_all == screen.filter_match_all
            && p.filter_tag_choices == screen.filter_tag_choices
    }) {
        set_filter(window, screen);
    }
    if differs(&|p| p.tags == screen.tags && p.dark == screen.dark) {
        window.set_tags(model(screen.tags.iter().map(|t| ui::ChoiceEntry {
            label: t.name.as_str().into(),
            count: to_i32(t.count),
            color: crate::palette::tag_icon_tint(t.colour, screen.dark),
            icon: slint::Image::default(),
            checked: false,
        })));
        window.set_tag_depths(model(screen.tags.iter().map(|t| to_i32(t.depth))));
    }
    if differs(&|p| p.nest_choices == screen.nest_choices) {
        let top = ui::MenuTexts::get(window).invoke_group_nest_top();
        window.set_nest_choices(model(screen.nest_choices.iter().map(|c| ui::ChoiceEntry {
            label: match &c.name {
                // A nested group's name is set in by a step per level.
                Some(name) => format!("{}{name}", "\u{2003}".repeat(c.depth)).into(),
                None => top.clone(),
            },
            count: if c.name.is_some() {
                to_i32(c.count)
            } else {
                -1
            },
            color: slint::Color::default(),
            icon: slint::Image::default(),
            checked: false,
        })));
    }
    if differs(&|p| p.tag_choices == screen.tag_choices) {
        window.set_tag_choices(model(screen.tag_choices.iter().map(|t| ui::ChoiceEntry {
            label: t.name.as_str().into(),
            count: to_i32(t.count),
            color: slint::Color::default(),
            icon: slint::Image::default(),
            checked: t.checked,
        })));
    }
    if window.get_recent_locked() != screen.recent_locked {
        window.set_recent_locked(screen.recent_locked);
    }
    let days = i32::try_from(screen.recent_days).unwrap_or(i32::MAX);
    if window.get_recent_days() != days {
        window.set_recent_days(days);
    }
    if window.get_recent_on() != screen.recent_on {
        window.set_recent_on(screen.recent_on);
    }
    if differs(&|p| p.recent_rows == screen.recent_rows) {
        window.set_recent_rows(model(screen.recent_rows.iter().map(|r| ui::RecentRow {
            id: i32::try_from(r.id).unwrap_or(i32::MAX),
            preview: r.preview.as_str().into(),
            app: r.app.as_str().into(),
            age_minutes: i32::try_from(r.age_minutes).unwrap_or(i32::MAX),
            kind: crate::binding::recent_kind(r.kind),
            image: crate::binding::recent_thumb(r.id, r.thumb.as_ref()),
        })));
    }
    if differs(&|p| p.recent_targets == screen.recent_targets && p.dark == screen.dark) {
        window.set_recent_targets(model(screen.recent_targets.iter().map(|t| {
            ui::ChoiceEntry {
                label: t.label.as_str().into(),
                count: 0,
                color: list_color(t.accent, screen.dark),
                icon: slint::Image::default(),
                checked: false,
            }
        })));
    }
    if differs(&|p| p.counts == screen.counts) {
        window.set_count_all(to_i32(screen.counts.all));
        window.set_count_pinned(to_i32(screen.counts.pinned));
        window.set_count_recent(to_i32(screen.counts.recent));
        window.set_count_trash(to_i32(screen.counts.trash));
    }
    if differs(&|p| p.rows == screen.rows && p.dark == screen.dark) {
        let current = window.get_rows();
        // The window's rows are the converted rows of the screen shown before, one for one: a row that
        // did not change is not converted again (MAIN-29).
        let converted: Vec<ui::MainRow> = (0..current.row_count())
            .filter_map(|i| current.row_data(i))
            .collect();
        let old = previous
            .filter(|p| p.dark == screen.dark && p.rows.len() == converted.len())
            .map(|p| (p.rows.as_slice(), converted.as_slice(), p.revealed));
        let rows = convert_rows(old, &screen.rows, screen.revealed, |i, r| {
            let mut row = main_row(r, screen.dark, &|l| wording::item_label_name(window, l));
            // The one sensitive row held revealed: the controller put its text in the preview.
            if screen.revealed == Some(i) {
                row.preview = r.preview.as_str().into();
            }
            row
        });
        if let Some(fresh) = crate::binding::refreshed_model(&current, rows) {
            window.set_rows(fresh);
        }
    }
    if window.get_selected_count() != to_i32(screen.selected_count) {
        window.set_selected_count(to_i32(screen.selected_count));
    }
    if differs(&|p| p.use_label == screen.use_label) {
        window.set_use_label(wording::use_label_text(window, screen.use_label).into());
        window.set_use_copies(screen.use_label == crate::panel::UseLabel::Copy);
    }
    let revealed = screen.revealed.map_or(-1, to_i32);
    if window.get_revealed_index() != revealed {
        window.set_revealed_index(revealed);
    }
    let (kind, at) = ui_section(screen.section, screen.section_index);
    if window.get_section() != kind {
        window.set_section(kind);
    }
    if window.get_section_index() != at {
        window.set_section_index(at);
    }
    let view = ui_view(screen.view_mode);
    if window.get_view() != view {
        window.set_view(view);
    }
    let theme = theme_index(screen.theme);
    if window.get_theme_index() != theme {
        window.set_theme_index(theme);
    }
    if window.get_query().as_str() != screen.query {
        window.set_query(screen.query.as_str().into());
    }
    window.set_search_capped(screen.search_capped);
    window.set_rows_reorderable(screen.reorderable);
    window.set_has_groups(screen.rows.iter().any(|r| r.header.is_some()));
    window.set_list_total(to_i32(list_total(&screen.rows)));
    let rename_row = screen.rename_row.map_or(-1, to_i32);
    if window.get_rename_row() != rename_row {
        window.set_rename_row(rename_row);
    }
    let rename_seq = to_i32(usize::try_from(screen.rename_seq).unwrap_or(0));
    if window.get_rename_seq() != rename_seq {
        window.set_rename_seq(rename_seq);
    }
    window.set_list_sort(sort_index(screen.sort));
    window.set_list_sort_reversed(screen.sort.reversed && screen.sort.key.has_direction());
    window.set_sortable(screen.sortable);
    window.set_reorder_offer(screen.reorder_offer);
    // The window runs ahead of the setting while the splitter is dragged: only a change of the setting is pushed.
    if differs(&|p| p.sidebar_width == screen.sidebar_width) {
        window.set_sidebar_width(i32::try_from(screen.sidebar_width).unwrap_or(232));
    }
    if window.get_detail_open() != screen.detail_open {
        window.set_detail_open(screen.detail_open);
    }
    if differs(&|p| p.detail == screen.detail && p.dark == screen.dark) {
        window.set_detail(detail_data(screen.detail.as_ref()));
    }
    if differs(&|p| p.sidebar_collapsed == screen.sidebar_collapsed) {
        window.set_sidebar_collapsed(screen.sidebar_collapsed);
    }
    if window.get_cursor() != to_i32(screen.cursor) {
        window.set_cursor(to_i32(screen.cursor));
    }
    if differs(&|p| p.history == screen.history) {
        window.set_can_undo(screen.history.undo.is_some());
        window.set_can_redo(screen.history.redo.is_some());
        window.set_undo_tip(wording::undo_tip(window, screen.history.undo).into());
        window.set_redo_tip(wording::redo_tip(window, screen.history.redo).into());
    }
    if differs(&|p| p.toast == screen.toast) {
        if let Some(toast) = &screen.toast {
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
            window.set_toast_undo_label(wording::toast_button_text(window, toast).into());
            window.set_toast_edit_label(if toast.edit {
                wording::toast_edit_label(window).into()
            } else {
                SharedString::new()
            });
            window.set_toast_text(wording::toast_text(window, &toast.message).into());
        } else {
            window.set_toast_text(SharedString::new());
            window.set_toast_undo_label(SharedString::new());
            window.set_toast_edit_label(SharedString::new());
            window.set_toast_phase(ui::ToastPhase::Exit);
        }
    }
}

/// The Windows high-contrast colours, the accent and whether motion is reduced.
pub(crate) fn set_look<C>(
    component: &C,
    hc: Option<&HighContrast>,
    accent: Option<&Accent>,
    reduced_motion: bool,
) where
    C: ComponentHandle,
    for<'a> ui::SystemLook<'a>: slint::Global<'a, C>,
{
    set_look_globals(
        &component.global::<ui::SystemLook<'_>>(),
        hc,
        accent,
        reduced_motion,
    );
}

/// What `apply_look` reads, to tell whether the window's look changed.
fn look_key(screen: &MainScreen) -> (Option<HighContrast>, Option<Accent>, bool) {
    (screen.high_contrast, screen.accent, screen.reduced_motion)
}

fn apply_look<C>(component: &C, screen: &MainScreen)
where
    C: ComponentHandle,
    for<'a> ui::SystemLook<'a>: slint::Global<'a, C>,
{
    set_look(
        component,
        screen.high_contrast.as_ref(),
        screen.accent.as_ref(),
        screen.reduced_motion,
    );
}

/// What a dragged row carries: an item, or a group's header (a note or a header only; nothing else drags).
fn row_payload(key: Key) -> Option<Payload> {
    match key {
        Key::Item(id) => Some(Payload::Item(id)),
        Key::Group(id) => Some(Payload::Group(id)),
        _ => None,
    }
}

/// The row a dropped payload stands for.
fn payload_key(payload: Payload) -> Option<Key> {
    match payload {
        Payload::Item(id) => Some(Key::Item(id)),
        Payload::Group(id) => Some(Key::Group(id)),
        Payload::List(_) => None,
    }
}

/// The window's rows for `rows`: a row equal to the one with the same key in `old` takes that
/// row's converted form, and only the others go through `convert`. `old` is the rows shown
/// before, their converted forms (one for one) and which of them was held revealed; a row that
/// is or was revealed is always converted.
fn convert_rows<R: Clone>(
    old: Option<(&[crate::main_window::Row], &[R], Option<usize>)>,
    rows: &[crate::main_window::Row],
    revealed: Option<usize>,
    mut convert: impl FnMut(usize, &crate::main_window::Row) -> R,
) -> Vec<R> {
    let at_key: std::collections::HashMap<crate::main_window::Key, usize> = old
        .map(|(shown, _, _)| shown.iter().enumerate().map(|(i, r)| (r.key, i)).collect())
        .unwrap_or_default();
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            if let Some((shown, out, was_revealed)) = old
                && revealed != Some(i)
                && let Some(&at) = at_key.get(&row.key)
                && was_revealed != Some(at)
                && shown.get(at) == Some(row)
                && let Some(kept) = out.get(at)
            {
                return kept.clone();
            }
            convert(i, row)
        })
        .collect()
}

/// The items a list holds: a top-level header stands for its group's count (members folded away and nested groups
/// included) and the rows shown under it are not counted twice.
fn list_total(rows: &[crate::main_window::Row]) -> usize {
    rows.iter()
        .map(|r| match &r.header {
            Some(h) if h.depth == 0 => h.count,
            Some(_) => 0,
            None => usize::from(r.depth == 0),
        })
        .sum()
}

fn main_row(
    row: &crate::main_window::Row,
    dark: bool,
    label_name: &dyn Fn(ItemLabel) -> String,
) -> ui::MainRow {
    let (list_name, color) = row.list.as_ref().map_or_else(
        || (SharedString::new(), slint::Color::default()),
        |label| {
            (
                crate::picker_binding::picker_place(&label.board, &label.name).into(),
                list_color(label.accent, dark),
            )
        },
    );
    let (label, label_name) = crate::binding::label_fields(row.label, label_name);
    let marks = row.marks.fields();
    let (swatch, has_swatch) =
        crate::colour_words::swatch_of_kind(row.kind, &row.preview, row.sensitive);
    ui::MainRow {
        kind: item_kind_of(row.kind, row.sensitive),
        title: row
            .header
            .as_ref()
            .map_or(row.title.as_str(), |h| h.name.as_str())
            .into(),
        mark_before: marks.mark_before.as_str().into(),
        mark_text: marks.mark_text.as_str().into(),
        preview: if row.sensitive {
            SharedString::new()
        } else {
            marks
                .preview_line
                .as_deref()
                .unwrap_or(row.preview.as_str())
                .into()
        },
        preview_before: marks.preview_before.as_str().into(),
        preview_match: marks.preview_match.as_str().into(),
        swatch,
        has_swatch,
        tag_hit: marks.tag_hit.as_str().into(),
        masked_preview: if row.sensitive {
            crate::binding::MASKED_PREVIEW.into()
        } else {
            SharedString::new()
        },
        list_name,
        list_color: color,
        label,
        label_name,
        image: row.thumb.as_ref().map_or_else(slint::Image::default, |t| {
            crate::binding::thumb_image(Some(t.pixels()))
        }),
        pinned: row.pinned,
        selected: row.selected,
        is_header: row.header.is_some(),
        group_count: row
            .header
            .as_ref()
            .map_or(0, |h| i32::try_from(h.count).unwrap_or(i32::MAX)),
        group_collapsed: row.header.as_ref().is_some_and(|h| h.collapsed),
        group_colour: accent_index(row.header.as_ref().and_then(|h| h.colour)),
        depth: row
            .header
            .as_ref()
            .map_or(row.depth, |h| h.depth)
            .try_into()
            .unwrap_or(i32::MAX),
    }
}

fn menu_entry(window: &ui::MainWindow, entry: &BoardMenuEntry) -> ui::MenuEntry {
    let disabled = matches!(
        entry,
        BoardMenuEntry::MoveUp { enabled: false }
            | BoardMenuEntry::MoveDown { enabled: false }
            | BoardMenuEntry::Delete { enabled: false }
    );
    ui::MenuEntry {
        text: wording::board_menu_text(window, entry).into(),
        separator: matches!(entry, BoardMenuEntry::Separator),
        destructive: matches!(entry, BoardMenuEntry::Delete { .. }),
        disabled,
        ..Default::default()
    }
}

fn error_text<C>(window: &C, view: &EditorView) -> SharedString
where
    for<'a> ui::FormTexts<'a>: slint::Global<'a, C>,
{
    view.name_error
        .as_ref()
        .map(|error| wording::name_error_text(window, error))
        .unwrap_or_default()
        .into()
}

fn fill_list_editor(window: &ui::ListEditor, view: &EditorView) {
    window.set_is_new(view.is_new);
    window.set_restored(view.restored);
    window.set_name(view.name.as_str().into());
    window.set_accent(accent_index(view.accent));
    window.set_icon_names(model(
        ListIcon::ALL
            .iter()
            .map(|i| SharedString::from(wording::list_icon_name(window, *i))),
    ));
    window.set_icon_index(icon_index(view.icon));
    window.set_sort_index(sort_index(view.sort));
    window.set_board_names(model(
        view.boards
            .iter()
            .map(|(_, name)| SharedString::from(name.as_str())),
    ));
    window.set_board_index(to_i32(view.board_index));
    window.set_show_colour(true);
    window.set_show_sort(true);
    window.set_item_count(to_i32(view.item_count));
    window.set_can_delete(view.can_delete);
    window.set_delete_hint(hint_text(window, view));
    window.set_discard_open(false);
    window.set_delete_open(false);
    window.invoke_mark_clean();
}

fn fill_board_editor(window: &ui::BoardEditor, view: &EditorView) {
    window.set_is_new(view.is_new);
    window.set_restored(view.restored);
    window.set_name(view.name.as_str().into());
    window.set_accent(accent_index(view.accent));
    window.set_icon_names(model(
        ListIcon::ALL
            .iter()
            .map(|i| SharedString::from(wording::list_icon_name(window, *i))),
    ));
    window.set_icon_index(icon_index(view.icon));
    window.set_show_colour(true);
    window.set_show_sort(false);
    window.set_item_count(to_i32(view.item_count));
    window.set_can_delete(view.can_delete);
    window.set_delete_hint(hint_text(window, view));
    window.set_discard_open(false);
    window.set_delete_open(false);
    window.invoke_mark_clean();
}

fn hint_text<C>(window: &C, view: &EditorView) -> SharedString
where
    for<'a> ui::FormTexts<'a>: slint::Global<'a, C>,
{
    view.delete_hint
        .map(|hint| wording::delete_hint_text(window, hint))
        .unwrap_or_default()
        .into()
}

// ---- Enums -------------------------------------------------------------------------------------

/// The editor's colour index: -1 for none, else the position in [`ListAccent::ALL`] (the order of
/// the window's `ListHue`).
pub(crate) fn accent_index(accent: Option<ListAccent>) -> i32 {
    accent
        .and_then(|a| ListAccent::ALL.iter().position(|x| *x == a))
        .map_or(-1, to_i32)
}

fn accent_of(index: i32) -> Option<ListAccent> {
    usize::try_from(index)
        .ok()
        .and_then(|i| ListAccent::ALL.get(i).copied())
}

/// The editor's icon index: -1 for none, else the position in [`ListIcon::ALL`] (the order of the
/// window's `ListIcons.all`).
fn icon_index(icon: Option<ListIcon>) -> i32 {
    icon.and_then(|a| ListIcon::ALL.iter().position(|x| *x == a))
        .map_or(-1, to_i32)
}

fn icon_of(index: i32) -> Option<ListIcon> {
    usize::try_from(index)
        .ok()
        .and_then(|i| ListIcon::ALL.get(i).copied())
}

/// The position of the sort's key in [`SortKey::ALL`] (manual, name, created, modified, most
/// used, recently used): the sort menu and the list editor use the same order.
pub(crate) fn sort_index(sort: Sort) -> i32 {
    SortKey::ALL
        .iter()
        .position(|k| *k == sort.key)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(0)
}

/// The sort at `index` of [`SortKey::ALL`]: the key's natural direction, or the other one.
fn sort_of(index: i32, reversed: bool) -> Sort {
    let key = usize::try_from(index)
        .ok()
        .and_then(|i| SortKey::ALL.get(i).copied())
        .unwrap_or(SortKey::Manual);
    Sort::new(key, reversed)
}

fn ui_section(section: Section, index: usize) -> (ui::MainSection, i32) {
    match section {
        Section::All => (ui::MainSection::All, 0),
        Section::Pinned => (ui::MainSection::Pinned, 0),
        Section::Recent => (ui::MainSection::Recent, 0),
        Section::Trash => (ui::MainSection::Trash, 0),
        Section::List(_) => (ui::MainSection::List, to_i32(index)),
        Section::Tag(_) => (ui::MainSection::Tag, to_i32(index)),
    }
}

/// The section the sidebar row `(kind, i)` names, from the lists and tags the window shows.
fn section_of(screen: &MainScreen, kind: ui::MainSection, i: usize) -> Option<Section> {
    match kind {
        ui::MainSection::All => Some(Section::All),
        ui::MainSection::Pinned => Some(Section::Pinned),
        ui::MainSection::Recent => Some(Section::Recent),
        ui::MainSection::Trash => Some(Section::Trash),
        ui::MainSection::List => screen.lists.get(i).map(|l| Section::List(l.id)),
        ui::MainSection::Tag => screen.tags.get(i).map(|t| Section::Tag(t.id)),
    }
}

/// The View menu's index of `theme` (the order of its rows).
fn theme_index(theme: Theme) -> i32 {
    match theme {
        Theme::System => 0,
        Theme::Light => 1,
        Theme::Dark => 2,
    }
}

/// The theme the View menu's row `index` names.
fn theme_of(index: i32) -> Theme {
    match index {
        1 => Theme::Light,
        2 => Theme::Dark,
        _ => Theme::System,
    }
}

pub(crate) fn ui_view(view: ViewMode) -> ui::MainView {
    match view {
        ViewMode::Comfortable => ui::MainView::Comfortable,
        ViewMode::Compact => ui::MainView::Compact,
        ViewMode::Tile => ui::MainView::Tile,
    }
}

fn view_of(view: ui::MainView) -> ViewMode {
    match view {
        ui::MainView::Comfortable => ViewMode::Comfortable,
        ui::MainView::Compact => ViewMode::Compact,
        ui::MainView::Tile => ViewMode::Tile,
    }
}

fn select_mode(mode: ui::SelectMode) -> SelectMode {
    match mode {
        ui::SelectMode::Replace => SelectMode::Replace,
        ui::SelectMode::Toggle => SelectMode::Toggle,
        ui::SelectMode::Extend => SelectMode::Extend,
    }
}

/// The Detail pane's data for `view` (an empty pane when there is none).
fn detail_data(view: Option<&crate::main_window::DetailView>) -> ui::DetailData {
    let Some(v) = view else {
        return ui::DetailData::default();
    };
    ui::DetailData {
        has_item: true,
        kind: item_kind_of(v.kind, v.sensitive),
        title: v.title.as_str().into(),
        text: v.text.as_str().into(),
        text_cut: v.text_cut,
        image: crate::binding::thumb_image(v.thumb.as_ref().map(crate::thumb_cache::Thumb::pixels)),
        formats: model(v.formats.iter().map(|k| match k {
            snipdeck_core::FormatKind::Text => 0,
            snipdeck_core::FormatKind::Html => 1,
            snipdeck_core::FormatKind::Rtf => 2,
            snipdeck_core::FormatKind::Png => 3,
            snipdeck_core::FormatKind::Files => 4,
        })),
        use_count: i32::try_from(v.use_count).unwrap_or(i32::MAX),
        last_used: v.last_used.as_str().into(),
        created: v.created.as_str().into(),
        updated: v.updated.as_str().into(),
        source_app: v.source_app.as_str().into(),
        list_name: v.list_name.as_str().into(),
        tags: v.tags.as_str().into(),
    }
}

/// Decodes the queued rows' thumbnails a few at a time, one tick after the window drew them.
fn decode_thumbs_later<S, C, K, T>(core: Rc<MainCore<S, C, K, T>>, wanted: Rc<RefCell<Vec<usize>>>)
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    slint::Timer::single_shot(std::time::Duration::ZERO, move || {
        let batch: Vec<usize> = {
            let mut queue = wanted.borrow_mut();
            let n = queue.len().min(crate::thumb_cache::DECODES_PER_PASS);
            queue.drain(..n).collect()
        };
        core.act(|ctl| {
            for at in batch {
                ctl.main_thumbnail(at);
            }
        });
        if !wanted.borrow().is_empty() {
            decode_thumbs_later(core, wanted);
        }
    });
}

fn action_of(action: ui::ItemAction) -> MainAction {
    match action {
        ui::ItemAction::Use => MainAction::Use,
        ui::ItemAction::UsePlain => MainAction::UsePlain,
        ui::ItemAction::Copy => MainAction::Copy,
        ui::ItemAction::OpenLink => MainAction::OpenLink,
        ui::ItemAction::Edit => MainAction::Edit,
        ui::ItemAction::Pin => MainAction::Pin,
        ui::ItemAction::Duplicate => MainAction::Duplicate,
        ui::ItemAction::Move => MainAction::Move,
        ui::ItemAction::Tag => MainAction::Tag,
        ui::ItemAction::PasteAs => MainAction::PasteAs,
        ui::ItemAction::Delete => MainAction::Delete,
        ui::ItemAction::Restore => MainAction::Restore,
        ui::ItemAction::DeleteForever => MainAction::DeleteForever,
    }
}

macro_rules! loggable_debug {
    ($($ty:ty),*) => {$(
        impl Loggable for $ty {
            fn log(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{self:?}")
            }
        }
    )*};
}
loggable_debug!(
    ui::MainSection,
    ui::MainView,
    ui::SelectMode,
    ui::ItemAction,
    ui::HelpPage
);

/// Reads the files of `run` (dropped, or pasted from Explorer) off the UI thread; the window
/// stays alive, and each result is applied when the controller is free.
fn read_run<S, C, K, T>(
    controller: &std::rc::Weak<RefCell<Controller<S, C, K, T>>>,
    run: crate::controller::DropRun,
) where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    let controller = controller.clone();
    crate::drop_read::drive(
        run,
        Rc::new(move |run, done| {
            let Some(c) = controller.upgrade() else {
                return Ok(None);
            };
            let Ok(mut c) = c.try_borrow_mut() else {
                return Err(Box::new((run, done)));
            };
            let next = c.drop_read_done(run, done);
            drop(c);
            crate::binding::redraw_all();
            Ok(next)
        }),
    );
}

/// Where tag `at` of `tags` (in sidebar order) can go, as `(label, row)`: the top level (row -1)
/// when it is nested now, then every tag it is not already under, other than itself and the tags
/// nested under it (which follow it in the order, deeper than it).
#[must_use]
pub(crate) fn nest_places(tags: &[crate::main_window::SideTag], at: usize) -> Vec<(String, i32)> {
    let Some(own) = tags.get(at) else {
        return Vec::new();
    };
    let mut places = Vec::new();
    if own.depth > 0 {
        places.push((String::new(), -1));
    }
    let mut below_end = at + 1;
    while tags.get(below_end).is_some_and(|t| t.depth > own.depth) {
        below_end += 1;
    }
    let parent = tags[..at].iter().rposition(|t| t.depth + 1 == own.depth);
    for (i, t) in tags.iter().enumerate() {
        if i == at || (at < i && i < below_end) || Some(i) == parent {
            continue;
        }
        places.push((t.path.clone(), to_i32(i)));
    }
    places
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_offered_every_place_but_itself_its_own_tags_and_where_it_already_is() {
        let tag = |path: &str, depth: usize| crate::main_window::SideTag {
            id: snipdeck_core::TagId::generate(),
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            count: 0,
            colour: None,
            own_count: 0,
            depth,
            path: path.to_owned(),
        };
        // a, a/b, a/b/c, a/d, e
        let tags = [
            tag("a", 0),
            tag("a/b", 1),
            tag("a/b/c", 2),
            tag("a/d", 1),
            tag("e", 0),
        ];
        let labels = |at: usize| -> Vec<(String, i32)> { nest_places(&tags, at) };
        let owned = |v: &[(&str, i32)]| -> Vec<(String, i32)> {
            v.iter().map(|(l, i)| ((*l).to_owned(), *i)).collect()
        };
        // a: not itself or its three nested tags; it is at the top already.
        assert_eq!(labels(0), owned(&[("e", 4)]));
        // a/b: top level, not a (its parent), not itself or a/b/c.
        assert_eq!(labels(1), owned(&[("", -1), ("a/d", 3), ("e", 4)]));
        // e: not itself.
        assert_eq!(
            labels(4),
            owned(&[("a", 0), ("a/b", 1), ("a/b/c", 2), ("a/d", 3)])
        );
        assert_eq!(labels(9), owned(&[]));
    }

    #[test]
    fn accent_and_sort_indexes_round_trip_in_the_editors_order() {
        assert_eq!(accent_index(None), -1);
        assert_eq!(accent_of(-1), None);
        assert_eq!(accent_of(99), None);
        for (i, accent) in ListAccent::ALL.iter().enumerate() {
            assert_eq!(accent_index(Some(*accent)), to_i32(i));
            assert_eq!(accent_of(to_i32(i)), Some(*accent));
        }
        // The window's ListHue order: blue, teal, green, amber, orange, red, rose, violet, slate.
        assert_eq!(accent_index(Some(ListAccent::Blue)), 0);
        assert_eq!(accent_index(Some(ListAccent::Slate)), 8);
        for &key in SortKey::ALL {
            for reversed in [false, true] {
                let sort = Sort::new(key, reversed);
                assert_eq!(sort_of(sort_index(sort), reversed), sort);
            }
        }
        assert_eq!(
            sort_index(Sort::MOST_USED),
            4,
            "the menu lists the keys in SortKey::ALL order"
        );
    }

    #[test]
    fn a_window_created_under_create_sees_the_regular_flag() {
        assert!(!regular_window_being_shown());
        let seen = RegularWindow::create(regular_window_being_shown);
        assert!(seen);
        assert!(!regular_window_being_shown());
    }

    #[test]
    fn create_nests_and_clears_the_flag_when_it_returns_an_error() {
        let result: Result<(), ()> = RegularWindow::create(|| {
            RegularWindow::create(|| assert!(regular_window_being_shown()));
            // The inner guard is gone, the outer one still holds.
            assert!(regular_window_being_shown());
            Err(())
        });
        assert!(result.is_err());
        assert!(!regular_window_being_shown());
    }

    #[test]
    fn a_dragged_header_travels_as_a_group_payload_and_finds_its_row_again() {
        use crate::main_window::Key;
        let (item, group) = (
            snipdeck_core::ItemId::generate(),
            snipdeck_core::GroupId::generate(),
        );
        let rows = [Key::Group(group), Key::Item(item)];
        for (at, key) in rows.iter().enumerate() {
            let data = row_payload(*key).expect("a row drags").into_transfer();
            let back = Payload::of(&data).and_then(payload_key);
            assert_eq!(back, Some(*key));
            assert_eq!(rows.iter().position(|k| Some(*k) == back), Some(at));
        }
        assert_eq!(
            row_payload(Key::Board(snipdeck_core::BoardId::generate())),
            None
        );
        assert_eq!(
            payload_key(Payload::List(snipdeck_core::ListId::generate())),
            None
        );
    }

    #[test]
    fn the_list_total_counts_items_not_headers_and_folded_members() {
        use crate::main_window::{Key, Row};
        use crate::panel::GroupHeader;
        let item = |depth: usize, header: Option<GroupHeader>| Row {
            key: Key::Item(snipdeck_core::ItemId::generate()),
            kind: snipdeck_core::ItemKind::Text,
            sensitive: false,
            title: String::new(),
            preview: String::new(),
            list: None,
            pinned: false,
            label: None,
            selected: false,
            marks: crate::search_mark::marks("", "", false, None, ""),
            header,
            depth,
            image_stamp: None,
            thumb: None,
        };
        let head = |count, collapsed, depth| {
            Some(GroupHeader {
                group: snipdeck_core::GroupId::generate(),
                name: "G".into(),
                count,
                collapsed,
                colour: None,
                depth,
            })
        };
        let rows = [
            item(0, head(3, false, 0)),
            item(1, None),
            item(1, head(2, false, 1)),
            item(2, None),
            item(2, None),
            item(0, None),
            item(0, None),
            item(0, head(5, true, 0)),
        ];
        assert_eq!(list_total(&rows), 10);
        assert_eq!(list_total(&rows[5..7]), 2);
    }

    /// MAIN-29: a redraw converts the rows that changed, not all of them.
    #[test]
    fn unchanged_rows_are_not_converted_again() {
        use crate::main_window::{Key, Row};
        let plain = |title: &str| Row {
            key: Key::Item(snipdeck_core::ItemId::generate()),
            kind: snipdeck_core::ItemKind::Text,
            sensitive: false,
            title: title.into(),
            preview: String::new(),
            list: None,
            pinned: false,
            label: None,
            selected: false,
            marks: crate::search_mark::marks(title, "", false, None, ""),
            header: None,
            depth: 0,
            image_stamp: None,
            thumb: None,
        };
        let rows: Vec<Row> = ["a", "b", "c", "d"].iter().map(|t| plain(t)).collect();
        let mut converted = 0;
        let mut convert = |_: usize, r: &Row| {
            converted += 1;
            r.title.clone()
        };
        let first = convert_rows(None, &rows, None, &mut convert);
        assert_eq!(first, ["a", "b", "c", "d"]);
        assert_eq!(converted, 4, "the first draw converts every row");

        // One row is selected and a new one comes in at the top: two conversions, not five.
        let mut next = rows.clone();
        next[1].selected = true;
        next.insert(0, plain("new"));
        let mut count = 0;
        let out = convert_rows(
            Some((&rows, &first, None)),
            &next,
            None,
            |_: usize, r: &Row| {
                count += 1;
                r.title.clone()
            },
        );
        assert_eq!(out, ["new", "a", "b", "c", "d"]);
        assert_eq!(count, 2);

        // Nothing changed: nothing is converted.
        let mut count = 0;
        let same = convert_rows(
            Some((&rows, &first, None)),
            &rows,
            None,
            |_: usize, r: &Row| {
                count += 1;
                r.title.clone()
            },
        );
        assert_eq!(same, first);
        assert_eq!(count, 0);

        // The row held revealed (now or before) always converts: its text came from outside the row.
        let mut count = 0;
        convert_rows(
            Some((&rows, &first, Some(2))),
            &rows,
            Some(1),
            |_: usize, r: &Row| {
                count += 1;
                r.title.clone()
            },
        );
        assert_eq!(count, 2, "the revealed row and the one that was");
    }

    #[test]
    fn main_row_carries_the_marks_and_masks_a_secret() {
        use crate::main_window::{Key, Row};
        let mut row = Row {
            key: Key::Item(snipdeck_core::ItemId::generate()),
            kind: snipdeck_core::ItemKind::Text,
            sensitive: false,
            title: "Q3 Report".into(),
            preview: "see the report today".into(),
            list: None,
            pinned: false,
            label: None,
            selected: false,
            marks: crate::search_mark::marks(
                "Q3 Report",
                "see the report today",
                false,
                None,
                "report",
            ),
            header: None,
            depth: 0,
            image_stamp: None,
            thumb: None,
        };
        let mapped = main_row(&row, false, &|_| String::new());
        assert_eq!(mapped.mark_before.as_str(), "Q3 ");
        assert_eq!(mapped.mark_text.as_str(), "Report");
        assert_eq!(mapped.preview_before.as_str(), "see the ");
        assert_eq!(mapped.preview_match.as_str(), "report");
        row.sensitive = true;
        row.marks = crate::search_mark::marks("Q3 Report", "hunter", true, None, "report");
        let secret = main_row(&row, false, &|_| String::new());
        assert_eq!(secret.preview.as_str(), "");
        assert_eq!(secret.preview_match.as_str(), "");
        assert_eq!(
            secret.masked_preview.as_str(),
            crate::binding::MASKED_PREVIEW
        );
    }
}
