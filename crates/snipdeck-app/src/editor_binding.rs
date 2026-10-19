//! Binds the item editor window to the [`Controller`]: copies an [`ItemEditorView`] into the
//! `ItemEditor` window and wires its callbacks to the `item_editor_*` methods. The same split as
//! the Panel's [`binding`](crate::binding) and the Main window's
//! [`main_binding`](crate::main_binding): the controller never touches a window.
//!
//! The window is made the first time an editor opens and reused, hidden between uses. It is
//! drawn from the controller by the shared redraw (see `binding::redraw_all`), so the Panel's
//! quick edit, the Main window's Edit and New, the Picker's Ctrl+E and Ctrl+N all open it the same
//! way: by asking the controller. While it is open a timer asks the controller for a draft every
//! [`crate::drafts::AUTOSAVE_EVERY`].
//!
//! The typed text stays in the window until Save, the draft tick or Discard asks for it; this
//! file reads it through `read_input` and never logs it.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{
    CloseRequestResponse, ComponentHandle, SharedPixelBuffer, SharedString,
    winit_030::WinitWindowAccessor,
};
use snipdeck_platform::{
    file_dialog::PickKind,
    pipeline::{Clipboard, Keyboard, Timing},
};
use snipdeck_ui as ui;

use crate::{
    binding::{index, model, redraw_all, register_redraw, to_i32},
    controller::{Controller, Library},
    drafts::AUTOSAVE_EVERY,
    host::ClipboardText,
    item_editor::{EditorInput, EditorProblem, ItemEditorView, kind_at, kind_index},
    main_binding::{create_window, set_look},
    native_window, wording,
};

struct EditorCore<S, C, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    window: RefCell<Option<ui::ItemEditor>>,
    /// The editor whose fields the window holds.
    seq: Cell<u64>,
    /// The last "bring to front" the window acted on.
    raise: Cell<u64>,
    /// The tag chips the window shows now: its model is only replaced when they change, so a
    /// keystroke does not reset an open tag list.
    tags_shown: RefCell<Vec<(String, bool)>>,
    /// Asks for a draft while an editor is open.
    autosave: slint::Timer,
    /// Redraws this window (registered with `binding::register_redraw`).
    redraw: Rc<dyn Fn()>,
}

/// A bound item editor. Dropping it does not unbind the window; the window's callbacks keep the
/// binding alive.
pub struct EditorBinding<S, C, K, T> {
    core: Rc<EditorCore<S, C, K, T>>,
}

impl<S, C, K, T> std::fmt::Debug for EditorBinding<S, C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorBinding").finish_non_exhaustive()
    }
}

impl<S, C, K, T> EditorBinding<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// The editor window, if one has been opened.
    #[must_use]
    pub fn window(&self) -> Option<ui::ItemEditor> {
        self.core
            .window
            .borrow()
            .as_ref()
            .map(ComponentHandle::clone_strong)
    }

    /// Redraws the window from the controller.
    pub fn redraw(&self) {
        self.core.sync();
    }
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Shows the item editor whenever the controller has one open, and routes the window's
    /// gestures to it. The window itself is made on the first use.
    pub fn bind_item_editor(this: &Rc<RefCell<Self>>) -> EditorBinding<S, C, K, T> {
        let core = Rc::new_cyclic(|weak: &Weak<EditorCore<S, C, K, T>>| {
            let weak = weak.clone();
            EditorCore {
                controller: Rc::clone(this),
                window: RefCell::new(None),
                seq: Cell::new(0),
                raise: Cell::new(0),
                tags_shown: RefCell::new(Vec::new()),
                autosave: slint::Timer::default(),
                redraw: Rc::new(move || {
                    if let Some(core) = weak.upgrade() {
                        core.sync();
                    }
                }),
            }
        });
        register_redraw(&core.redraw);
        EditorBinding { core }
    }
}

impl<S, C, K, T> EditorCore<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Runs `f` on the controller, then redraws every window.
    fn act<R>(self: &Rc<Self>, f: impl FnOnce(&mut Controller<S, C, K, T>) -> R) -> R {
        let result = f(&mut self.controller.borrow_mut());
        redraw_all();
        result
    }

    /// Shows, updates or hides the window to match the controller.
    fn sync(self: &Rc<Self>) {
        let view = self.controller.borrow().item_editor_view();
        let Some(view) = view else {
            self.autosave.stop();
            if let Some(window) = self.window.borrow().as_ref() {
                let _ = window.hide();
            }
            return;
        };
        let Some(window) = self.window_for_use() else {
            return;
        };
        set_look(&window, view.high_contrast.as_ref(), view.reduced_motion);
        window.set_dark(view.dark);
        let fresh = view.seq != self.seq.get();
        {
            let mut shown = self.tags_shown.borrow_mut();
            if fresh || *shown != view.tags {
                set_tags(&window, &view.tags);
                shown.clone_from(&view.tags);
            }
        }
        apply_state(&window, &view);
        if !fresh {
            if view.raise != self.raise.get() {
                self.raise.set(view.raise);
                bring_to_front(&window);
            }
            return;
        }
        self.seq.set(view.seq);
        self.raise.set(view.raise);
        fill(&window, &view);
        crate::native_window::place_over_owner(window.window());
        let _ = window.show();
        crate::native_window::fit_to_screen(window.window());
        window.invoke_focus_title();
        if view.restored {
            window.invoke_mark_dirty();
        }
        self.start_autosave();
    }

    fn start_autosave(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.autosave
            .start(slint::TimerMode::Repeated, AUTOSAVE_EVERY, move || {
                let Some(core) = weak.upgrade() else {
                    return;
                };
                let Some(window) = core
                    .window
                    .borrow()
                    .as_ref()
                    .map(ComponentHandle::clone_strong)
                else {
                    return;
                };
                let input = read_input(&window);
                // Skipped when the controller is busy: the next tick tries again.
                let changed = core
                    .controller
                    .try_borrow_mut()
                    .is_ok_and(|mut ctl| ctl.item_editor_autosave(&input));
                if changed {
                    redraw_all();
                }
            });
    }

    /// The window, made on first use (`None` if the windowing system cannot).
    fn window_for_use(self: &Rc<Self>) -> Option<ui::ItemEditor> {
        if let Some(w) = self.window.borrow().as_ref() {
            return Some(w.clone_strong());
        }
        let w = create_window(ui::ItemEditor::new).ok()?;
        // The window's preferred size (editors.slint), cut to the screen before the first show.
        crate::native_window::size_before_show(w.window(), (560.0, 700.0));
        self.wire(&w);
        *self.window.borrow_mut() = Some(w.clone_strong());
        Some(w)
    }

    fn wire(self: &Rc<Self>, w: &ui::ItemEditor) {
        let core = Rc::downgrade(self);
        let weak = w.as_weak();
        w.on_save_requested(move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            crate::trace!("callback editor save-requested");
            if let Some(w) = weak.upgrade() {
                let input = read_input(&w);
                core.act(|ctl| ctl.item_editor_save(&input));
            }
        });
        let core = Rc::downgrade(self);
        w.on_cancel_requested(move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            crate::trace!("callback editor cancel-requested");
            core.act(Controller::item_editor_cancel);
        });
        let core = Rc::downgrade(self);
        w.on_delete_requested(move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            crate::trace!("callback editor delete-requested");
            core.act(Controller::item_editor_delete);
        });
        let core = Rc::downgrade(self);
        w.on_tag_toggled(move |i| {
            let Some(core) = core.upgrade() else {
                return;
            };
            core.act(|ctl| ctl.item_editor_tag_toggled(index(i)));
        });
        let core = Rc::downgrade(self);
        w.on_tag_added(move |name| {
            let Some(core) = core.upgrade() else {
                return;
            };
            core.act(|ctl| ctl.item_editor_tag_added(name.as_str()));
        });
        let core = Rc::downgrade(self);
        w.on_edited(move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            // Typing only clears a refusal; nothing else on screen changes.
            core.controller.borrow_mut().item_editor_edited();
            core.sync();
        });
        let core = Rc::downgrade(self);
        let weak = w.as_weak();
        w.on_browse_requested(move || {
            let Some(core) = core.upgrade() else {
                return;
            };
            crate::trace!("callback editor browse-requested");
            if let Some(w) = weak.upgrade() {
                core.browse(&w);
            }
        });
        // The window's own close button: leaves like Cancel, asking first when something changed.
        let core = Rc::downgrade(self);
        let weak = w.as_weak();
        w.window().on_close_requested(move || {
            let Some(core) = core.upgrade() else {
                return CloseRequestResponse::HideWindow;
            };
            if let Some(w) = weak.upgrade()
                && w.get_dirty()
            {
                w.set_discard_open(true);
                return CloseRequestResponse::KeepWindowShown;
            }
            core.act(Controller::item_editor_cancel);
            CloseRequestResponse::HideWindow
        });
    }

    /// Browse: the file dialog runs with no borrow of the controller held (it has its own
    /// message loop), then its answer goes into the path field.
    fn browse(self: &Rc<Self>, w: &ui::ItemEditor) {
        let Some(picker) = self.controller.borrow().file_picker() else {
            return;
        };
        let kind = if kind_at(index(w.get_kind_index())) == snipdeck_core::ItemKind::Image {
            PickKind::Image
        } else {
            PickKind::Any
        };
        let answer = picker.pick(kind, native_window::handle_of(w.window()));
        let path = self.controller.borrow_mut().item_editor_picked(answer);
        if let Some(path) = path {
            w.set_source_path(path.into());
            w.invoke_mark_dirty();
        }
        redraw_all();
    }
}

/// What the window's fields hold now.
fn read_input(w: &ui::ItemEditor) -> EditorInput {
    EditorInput {
        kind: kind_at(index(w.get_kind_index())),
        title: w.get_item_title().to_string(),
        content: w.get_content().to_string(),
        sensitive: w.get_sensitive(),
        list_index: index(w.get_list_index()),
        source_path: w.get_source_path().to_string(),
    }
}

/// The parts of the view that follow the controller on every redraw: problems, the tag chips,
/// what the plan allows.
fn apply_state(w: &ui::ItemEditor, view: &ItemEditorView) {
    let text = |p: &Option<EditorProblem>| -> SharedString {
        p.as_ref()
            .map(wording::editor_problem_text)
            .unwrap_or_default()
            .into()
    };
    w.set_title_error(text(&view.title_problem));
    w.set_content_error(text(&view.content_problem));
    w.set_source_error(text(&view.source_problem));
    w.set_privacy_error(text(&view.privacy_problem));
    w.set_banner(text(&view.banner));
    w.set_prompted_available(view.prompted_available);
    w.set_can_browse(view.can_browse);
    w.set_restored(view.restored);
}

/// Shows the tag chips and how many are ticked.
fn set_tags(w: &ui::ItemEditor, tags: &[(String, bool)]) {
    let entries = tags.iter().map(|(label, checked)| ui::ChoiceEntry {
        label: label.as_str().into(),
        count: 0,
        color: slint::Color::default(),
        checked: *checked,
    });
    w.set_tags(model(entries));
    w.set_tags_on(to_i32(tags.iter().filter(|(_, on)| *on).count()));
}

/// Puts the starting values of a freshly opened editor into the window.
fn fill(w: &ui::ItemEditor, view: &ItemEditorView) {
    let fields = &view.fields;
    w.set_is_new(view.is_new);
    w.set_kind_index(to_i32(kind_index(fields.kind)));
    w.set_item_title(fields.title.as_str().into());
    w.set_content(fields.content.as_str().into());
    w.set_sensitive(fields.sensitive);
    w.set_source_path(fields.source_path.as_str().into());
    w.set_list_names(model(
        view.list_names
            .iter()
            .map(|n| SharedString::from(n.as_str())),
    ));
    w.set_list_index(to_i32(fields.list_index));
    w.set_content_large(view.large.is_some());
    w.set_content_size(
        view.large
            .map(wording::size_text)
            .unwrap_or_default()
            .into(),
    );
    if let Some(a) = &view.attachment {
        w.set_attachment_name(wording::attachment_name_text(a).into());
        w.set_attachment_detail(wording::attachment_detail_text(a).into());
        w.set_preview(a.preview.as_ref().map_or_else(slint::Image::default, |p| {
            slint::Image::from_rgba8(SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                &p.rgba, p.width, p.height,
            ))
        }));
    } else {
        w.set_attachment_name(SharedString::new());
        w.set_attachment_detail(SharedString::new());
        w.set_preview(slint::Image::default());
    }
    w.set_discard_open(false);
    w.set_tags_open(false);
    w.invoke_mark_clean();
}

/// Brings the shown window in front of the others and gives it the focus.
fn bring_to_front(w: &ui::ItemEditor) {
    let _ = w.show();
    let _ = w
        .window()
        .with_winit_window(slint::winit_030::winit::window::Window::focus_window);
}
