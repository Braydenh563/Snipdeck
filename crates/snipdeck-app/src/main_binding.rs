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
    CloseRequestResponse, ComponentHandle, Global, SharedString,
    winit_030::{WinitWindowAccessor, winit},
};
use snipdeck_core::{ItemKind, ListAccent, ListIcon, ListSort};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};
use snipdeck_ui as ui;

use crate::{
    accent::Accent,
    binding::{
        Loggable, Payload, Shown, arm_toast, extension_of, index, item_kind_of, model, redraw_all,
        register_redraw, to_i32,
    },
    controller::{
        BoardMenuEntry, Controller, HelpTopic, Library, MainAction, MainScreen, ToastPhase,
    },
    host::ClipboardText,
    item_editor::Origin,
    main_window::{EditorKind, EditorView, Key, Section, SelectMode, ViewMode},
    native_window::{
        fit_logical, fit_to_screen, place_of, place_over_owner, restore_place, size_before_show,
    },
    palette::list_color,
    popup_window::set_look_globals,
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
    crate::native_window::track_focus(window.window());
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

struct MainCore<S, C, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    window: slint::Weak<ui::MainWindow>,
    /// What the window shows now, so a redraw sets only what changed.
    shown: RefCell<Option<MainScreen>>,
    editors: RefCell<Editors>,
    /// The Tag manager window, made the first time it is asked for.
    tag_manager: RefCell<Option<ui::TagManager>>,
    /// Redraws this window (registered with `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
}

/// A bound Main window. Dropping it does not unbind the window; the window's callbacks keep
/// the binding alive.
pub struct MainBinding<S, C, K, T> {
    core: Rc<MainCore<S, C, K, T>>,
}

impl<S, C, K, T> std::fmt::Debug for MainBinding<S, C, K, T> {
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
            let words = window.global::<ui::CommandTexts<'_>>();
            ctl.set_new_name_bases(&words.get_new_list_name(), &words.get_new_board_name());
        }
        let result = f(&mut ctl);
        drop(ctl);
        redraw_all();
        result
    }

    /// Fills the command palette with the commands that match `query`.
    fn fill_palette(&self, query: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let ctl = self.controller.borrow();
        crate::command_binding::fill(
            &window,
            &ctl.settings().hotkeys,
            self.shown.borrow().as_ref(),
            query,
        );
    }

    /// The palette ran command `id`: Search moves the window's own focus, Tour opens the tour
    /// window, every other command is the controller's.
    fn run_command(self: &Rc<Self>, id: i32) {
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
            _ => {}
        }
        let cursor = self.shown.borrow().as_ref().map_or(0, |s| s.cursor);
        self.act(|ctl| ctl.run_command(command, cursor));
    }

    /// Copies the controller's screen into the window and the editors, and keeps the toast
    /// timer running.
    fn sync(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let screen = self.controller.borrow_mut().main_screen();
        render(&window, self.shown.borrow().as_ref(), &screen);
        if let Some(toast) = &screen.toast
            && self.controller.borrow_mut().claim_toast_timer(toast.seq)
        {
            arm_toast(
                Rc::downgrade(&self.controller),
                toast.seq,
                crate::controller::TOAST_ENTER,
            );
        }
        self.sync_editors(&screen);
        self.sync_tag_manager(&screen, false);
        *self.shown.borrow_mut() = Some(screen);
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
                    Err(error) => {
                        w.set_rename_error(wording::tag_rename_error_text(&w, &error).into());
                    }
                }
            }
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
        window.invoke_finish_rename();
        place_over_owner(window.window());
        let _ = window.show();
        fit_to_screen(window.window());
        raise(window.window());
    }

    /// Shows, updates or hides the list or board editor to match `screen.editor`.
    fn sync_editors(self: &Rc<Self>, screen: &MainScreen) {
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
                window.set_name_error(error_text(&window, view));
                if fresh {
                    fill_list_editor(&window, view);
                    size_to_content(window.window(), || window.get_wanted_height());
                    place_over_owner(window.window());
                    let _ = window.show();
                    fit_to_screen(window.window());
                    // Shown again after a close, the window would stay behind the Main
                    // window (it was created un-activated): bring it up.
                    raise(window.window());
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
                window.set_name_error(error_text(&window, view));
                if fresh {
                    fill_board_editor(&window, view);
                    size_to_content(window.window(), || window.get_wanted_height());
                    place_over_owner(window.window());
                    let _ = window.show();
                    fit_to_screen(window.window());
                    // Shown again after a close, the window would stay behind the Main
                    // window (it was created un-activated): bring it up.
                    raise(window.window());
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
                let (name, accent, icon, sort) = (
                    w.get_name(),
                    accent_of(w.get_accent()),
                    icon_of(w.get_icon_index()),
                    sort_of(w.get_sort_index()),
                );
                core.act(|ctl| ctl.main_editor_save(name.as_str(), accent, icon, sort));
            }
        });
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
                let name = w.get_name();
                core.act(|ctl| ctl.main_editor_save(name.as_str(), None, None, ListSort::Manual));
            }
        });
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
            MainCore {
                controller: Rc::clone(this),
                window: window.as_weak(),
                shown: RefCell::new(None),
                editors: RefCell::new(Editors::default()),
                tag_manager: RefCell::new(None),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
            }
        });
        register_redraw(&core.redraw);
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
        wire!(on_recent_cleared, |ctl| ctl.recent_clear());
        wire!(on_recent_unlock_requested, |ctl| ctl.request_unlock());
        wire!(on_item_action, |ctl, action, i| ctl
            .main_action(action_of(action), index(i)));
        wire!(on_move_chosen, |ctl, i| ctl.main_move_to(index(i)));
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
        wire!(on_board_delete_confirmed, |ctl| ctl
            .main_board_delete_confirm());
        wire!(on_board_delete_dismissed, |ctl| ctl
            .main_board_delete_cancel());
        wire!(on_toast_undo, |ctl| ctl.toast_action());
        wire!(on_undo_requested, |ctl| ctl.main_undo());
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
        wire!(on_panel_requested, |ctl| ctl.main_show_panel());
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
            window.on_palette_opened(move || core.fill_palette(""));
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
        wire!(on_item_reorder_requested, |ctl, d| ctl
            .main_reorder_items(isize::try_from(d).unwrap_or(0)));
        wire!(on_list_reorder_requested, |ctl, i, d| ctl
            .main_reorder_list(index(i), isize::try_from(d).unwrap_or(0)));
        wire!(on_row_dropped, |ctl, from, onto| ctl
            .main_drop_row(index(from), index(onto)));
        wire!(on_row_dropped_on_list, |ctl, row, list| ctl
            .main_drop_on_list(index(row), index(list)));
        wire!(on_list_dropped, |ctl, from, onto| ctl
            .main_drop_list(index(from), index(onto)));
        {
            let core = Rc::clone(&core);
            window.on_row_payload(move |row| {
                core.shown
                    .borrow()
                    .as_ref()
                    .and_then(|s| s.rows.get(index(row)))
                    .and_then(|r| match r.key {
                        Key::Item(id) => Some(Payload::Item(id)),
                        _ => None,
                    })
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
            window.on_payload_row(move |data| match Payload::of(&data) {
                Some(Payload::Item(id)) => core.shown.borrow().as_ref().map_or(-1, |s| {
                    s.rows
                        .iter()
                        .position(|r| r.key == Key::Item(id))
                        .map_or(-1, to_i32)
                }),
                _ => -1,
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
        {
            // The X hides the window, or quits when nothing could bring it back (the setting
            // "Keep running in the tray" is off, or there is no tray icon and the Panel is down).
            let core = Rc::clone(&core);
            window.window().on_close_requested(move || {
                let quit = core.controller.borrow().main_window_closed();
                crate::trace!("main window close requested: quit {quit}");
                if quit {
                    let _ = slint::quit_event_loop();
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
    // O39: Windows can drop the window's pixels (Alt+Tab, restore); redraw all of it then.
    let weak = window.as_weak();
    let mut watch = crate::repaint::Watch::new();
    window
        .window()
        .on_winit_window_event(move |slint_window, event| {
            crate::native_window::note_window_event(slint_window, event);
            if watch.on_signal(crate::repaint::signal_of(event))
                && let Some(w) = weak.upgrade()
            {
                w.set_repaint_flip(!w.get_repaint_flip());
                slint_window.request_redraw();
            }
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
                binding.redraw();
                window.show()?;
                if !placed.replace(true) {
                    // Where the user left it (then cut to the screen below); only once, as a
                    // hidden window keeps its place.
                    let saved = controller.borrow().settings().main_window;
                    if let Some(place) = saved {
                        restore_place(window.window(), &place);
                    }
                    watch_place(window, &controller);
                }
                fit_to_screen(window.window());
                raise(window.window());
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

/// Un-minimises the window and asks the OS to focus it (it was created un-activated).
pub(crate) fn raise(window: &slint::Window) {
    let _ = window.with_winit_window(|w| {
        w.set_minimized(false);
        w.focus_window();
    });
}

// ---- Screen -> window --------------------------------------------------------------------------

/// The colour-label filter row.
fn set_label_filter(window: &ui::MainWindow, screen: &MainScreen) {
    window.set_label_chips(model(screen.label_chips.iter().map(|chip| {
        let (label, label_name) = crate::binding::label_fields(Some(chip.label));
        ui::LabelChipEntry {
            label,
            name: label_name,
            count: to_i32(chip.count),
        }
    })));
    window.set_label_filter(crate::binding::label_index(screen.label_filter));
}

/// The sidebar's lists, with their colour marks and icons.
fn set_side_lists(window: &ui::MainWindow, screen: &MainScreen) {
    let icons = ui::ListIcons::get(window).get_all();
    window.set_lists(model(screen.lists.iter().map(|l| ui::ChoiceEntry {
        label: l.name.as_str().into(),
        count: to_i32(l.count),
        color: list_color(l.accent, screen.dark),
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
    if differs(&|p| look_key(p) == look_key(screen)) {
        apply_look(window, screen);
    }
    if differs(&|p| p.panel_hotkey == screen.panel_hotkey) {
        window.set_panel_hotkey(screen.panel_hotkey.as_str().into());
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
    if differs(&|p| p.lists == screen.lists && p.dark == screen.dark) {
        set_side_lists(window, screen);
    }
    if differs(&|p| p.label_chips == screen.label_chips && p.label_filter == screen.label_filter) {
        set_label_filter(window, screen);
    }
    if differs(&|p| p.tags == screen.tags) {
        window.set_tags(model(screen.tags.iter().map(|t| ui::ChoiceEntry {
            label: t.name.as_str().into(),
            count: to_i32(t.count),
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
    if window.get_recent_days() != crate::recent::max_age_days() {
        window.set_recent_days(crate::recent::max_age_days());
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
        window.set_rows(model(screen.rows.iter().map(|r| main_row(r, screen.dark))));
    }
    if window.get_selected_count() != to_i32(screen.selected_count) {
        window.set_selected_count(to_i32(screen.selected_count));
    }
    if differs(&|p| p.use_label == screen.use_label) {
        window.set_use_label(wording::use_label_text(window, screen.use_label).into());
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
    if window.get_query().as_str() != screen.query {
        window.set_query(screen.query.as_str().into());
    }
    window.set_rows_reorderable(screen.reorderable);
    if window.get_cursor() != to_i32(screen.cursor) {
        window.set_cursor(to_i32(screen.cursor));
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
            window.set_toast_undo_label(wording::toast_button_text(toast).into());
            window.set_toast_text(wording::toast_text(window, &toast.message).into());
        } else {
            window.set_toast_text(SharedString::new());
            window.set_toast_undo_label(SharedString::new());
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

fn main_row(row: &crate::main_window::Row, dark: bool) -> ui::MainRow {
    let (list_name, color) = row.list.as_ref().map_or_else(
        || (SharedString::new(), slint::Color::default()),
        |label| (label.name.as_str().into(), list_color(label.accent, dark)),
    );
    let (label, label_name) = crate::binding::label_fields(row.label);
    ui::MainRow {
        kind: item_kind_of(row.kind, row.sensitive),
        title: row.title.as_str().into(),
        preview: row.preview.as_str().into(),
        masked_preview: if row.sensitive {
            crate::binding::MASKED_PREVIEW.into()
        } else {
            SharedString::new()
        },
        extension: if row.kind == ItemKind::File {
            extension_of(&row.preview).into()
        } else {
            SharedString::new()
        },
        list_name,
        list_color: color,
        label,
        label_name,
        image: slint::Image::default(),
        pinned: row.pinned,
        selected: row.selected,
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
    window.set_name(view.name.as_str().into());
    window.set_accent(accent_index(view.accent));
    window.set_icon_names(model(
        ListIcon::ALL
            .iter()
            .map(|i| SharedString::from(wording::list_icon_name(*i))),
    ));
    window.set_icon_index(icon_index(view.icon));
    window.set_sort_index(sort_index(view.sort));
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
    window.set_name(view.name.as_str().into());
    window.set_show_colour(false);
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
fn accent_index(accent: Option<ListAccent>) -> i32 {
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

/// 0 manual, 1 most used, 2 recently used.
fn sort_index(sort: ListSort) -> i32 {
    match sort {
        ListSort::Manual => 0,
        ListSort::MostUsed => 1,
        ListSort::RecentlyUsed => 2,
    }
}

fn sort_of(index: i32) -> ListSort {
    match index {
        1 => ListSort::MostUsed,
        2 => ListSort::RecentlyUsed,
        _ => ListSort::Manual,
    }
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

fn ui_view(view: ViewMode) -> ui::MainView {
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

fn action_of(action: ui::ItemAction) -> MainAction {
    match action {
        ui::ItemAction::Use => MainAction::Use,
        ui::ItemAction::Copy => MainAction::Copy,
        ui::ItemAction::Edit => MainAction::Edit,
        ui::ItemAction::Pin => MainAction::Pin,
        ui::ItemAction::Duplicate => MainAction::Duplicate,
        ui::ItemAction::Move => MainAction::Move,
        ui::ItemAction::Tag => MainAction::Tag,
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

#[cfg(test)]
mod tests {
    use super::*;

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
        for sort in [ListSort::Manual, ListSort::MostUsed, ListSort::RecentlyUsed] {
            assert_eq!(sort_of(sort_index(sort)), sort);
        }
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
}
