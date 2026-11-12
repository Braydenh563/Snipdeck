//! Binds the controller's Picker ([`View::picker`](crate::controller::View)) to the Slint
//! `PickerWindow`: the window is made the first time the Picker opens, placed at the Target's
//! caret (SPEC 5.2), shown without being activated like the Panel and taken off the screen
//! when the controller closes the Picker. All the logic is the controller's; this file only
//! copies state into the window and the window's gestures back.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{ComponentHandle, SharedString};
use snipdeck_core::UseMode;
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};
use snipdeck_ui as ui;

use crate::{
    accent::Accent,
    binding::{MASKED_PREVIEW, index, item_kind_of, model, redraw_all, register_redraw},
    controller::{Controller, HelpTopic, Library, PickerView, View},
    frame::Rect,
    host::ClipboardText,
    palette::list_color,
    panel::Footer,
    picker::{Entry, Scope},
    placement::{self, Anchor, PopupHost},
    popup_window::{Shell, set_look_globals},
    toast::{Toast, ToastKind, ToastMessage},
};

/// The Picker's width in logical pixels (the window's `preferred-width`).
pub const PICKER_WIDTH: f32 = 560.0;

/// Where the last popup opened, shared by the Picker and the fill form: a template chosen in
/// the Picker opens its form at the same place, once the Target's caret is no longer readable
/// (the Picker has the keyboard by then).
pub type AnchorMemory = Rc<Cell<Option<Anchor>>>;

/// What the window shows now, so a redraw sets only what changed (re-setting the rows would
/// reset the list's scroll).
#[derive(Clone)]
struct Shown {
    dark: bool,
    high_contrast: Option<HighContrast>,
    accent: Option<Accent>,
    reduced_motion: bool,
    picker: PickerView,
}

struct PickerCore<S, C, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    host: Rc<dyn PopupHost>,
    anchor: AnchorMemory,
    shell: Rc<Shell<ui::PickerWindow>>,
    shown: RefCell<Option<Shown>>,
    /// The logical size the window was last given.
    size: Cell<(f32, f32)>,
    redraw: Rc<dyn Fn()>,
}

impl<S, C, K, T> PickerCore<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// Runs `f` on the controller, then redraws every window.
    fn act<R>(&self, f: impl FnOnce(&mut Controller<S, C, K, T>) -> R) -> R {
        let result = f(&mut self.controller.borrow_mut());
        redraw_all();
        result
    }

    /// Copies the controller's Picker into the window, opening or closing it as needed.
    fn sync(self: &Rc<Self>) {
        let view = self.controller.borrow().view();
        if let Some(picker) = &view.picker {
            self.draw(&view, picker);
        } else {
            self.shell.hide();
            // A template chosen in the Picker opens its form where the Picker was.
            if view.fill.is_none() {
                self.anchor.set(None);
            }
            *self.shown.borrow_mut() = None;
        }
    }

    fn draw(self: &Rc<Self>, view: &View, picker: &PickerView) {
        let made = self.shell.ensure(ui::PickerWindow::new);
        let (window, created) = match made {
            Ok(made) => made,
            Err(e) => {
                crate::trace!("cannot make the Picker window: {e}");
                self.give_up();
                return;
            }
        };
        if created {
            self.wire(&window);
        }
        let can_open = self.controller.borrow().settings().show_colours_tab;
        render(
            &window,
            self.shown.borrow().as_ref(),
            view,
            picker,
            can_open,
        );
        *self.shown.borrow_mut() = Some(Shown {
            dark: view.dark,
            high_contrast: view.high_contrast,
            accent: view.accent,
            reduced_motion: view.reduced_motion,
            picker: picker.clone(),
        });
        let size = (PICKER_WIDTH, window.get_content_height());
        let pick = self.controller.borrow().settings().picker_side;
        if self.shell.is_open() {
            // The list grew or shrank: keep the window where it is relative to the caret.
            if size != self.size.get() {
                let frame = self
                    .anchor
                    .get()
                    .and_then(|anchor| placement::place_at(&*self.host, anchor, size, pick));
                self.shell.place(&window, frame, size);
            }
        } else {
            // The Target still has the keyboard: read its caret now.
            let caret = self.controller.borrow().target_caret();
            let placed = placement::place_popup(&*self.host, caret, size, pick);
            self.anchor.set(placed.map(|(_, anchor)| anchor));
            if let Err(e) = self.shell.show(&window, placed.map(|(f, _)| f), size) {
                crate::trace!("cannot show the Picker: {e}");
                self.give_up();
                return;
            }
            window.invoke_focus_search();
        }
        self.size.set(size);
    }

    /// The window could not be made or shown: close the Picker and say so.
    fn give_up(self: &Rc<Self>) {
        self.act(|ctl| {
            ctl.close_picker();
            ctl.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::WindowNotShown,
            });
        });
    }

    /// Enter or a click on row `row`: paste or copy it. The window is off the screen before
    /// the paste, so the keys go into the Target and not into the Picker.
    fn use_row(&self, row: usize, mode: UseMode) {
        let item = self.controller.borrow_mut().picker_take(row);
        self.finish_use(item, mode);
    }

    /// Alt+digit (or a digit with an empty search): use the Nth row.
    fn use_number(&self, n: i32) {
        let item = u8::try_from(n)
            .ok()
            .and_then(|n| self.controller.borrow_mut().picker_take_number(n));
        self.finish_use(item, UseMode::Paste);
    }

    /// A colour row's "Paste as" chose format `format`: the colour is written that way, pasted
    /// into the Target or (Shift) only copied. As for `use_row`, the window is off the screen
    /// before the paste.
    fn use_colour_row(&self, row: usize, format: usize, copy: bool) {
        let item = self.controller.borrow_mut().picker_take(row);
        if let Some(item) = item {
            self.shell.hide();
            let mode = if copy { UseMode::Copy } else { UseMode::Paste };
            crate::trace!("picker uses a colour ({mode:?})");
            self.controller
                .borrow_mut()
                .use_colour_item_as(item, format, mode);
        }
        redraw_all();
    }

    fn finish_use(&self, item: Option<snipdeck_core::ItemId>, mode: UseMode) {
        if let Some(item) = item {
            self.shell.hide();
            crate::trace!("picker uses an item ({mode:?})");
            self.controller.borrow_mut().use_item(item, mode);
        }
        redraw_all();
    }

    fn wire(self: &Rc<Self>, window: &ui::PickerWindow) {
        macro_rules! wire {
            ($on:ident, |$core:ident $(, $arg:ident)*| $body:expr) => {{
                let weak: Weak<Self> = Rc::downgrade(self);
                window.$on(move |$($arg),*| {
                    let Some($core) = weak.upgrade() else {
                        return;
                    };
                    $body;
                });
            }};
        }
        wire!(on_query_changed, |core, query| core
            .act(|ctl| ctl.picker_query(query.as_str())));
        crate::wire_suggestions!(window, self.controller, picker_suggestions);
        wire!(on_scope_changed, |core, scope| core
            .act(|ctl| ctl.picker_scope(scope_of(scope))));
        wire!(on_row_activated, |core, i| core
            .use_row(index(i), UseMode::Paste));
        wire!(on_row_copied, |core, i| core
            .use_row(index(i), UseMode::Copy));
        wire!(on_quick_use, |core, n| core.use_number(n));
        wire!(on_selection_moved, |core, delta| core.act(
            |ctl| ctl.picker_move(isize::try_from(delta).unwrap_or(0))
        ));
        wire!(on_edit_requested, |core, i| core
            .act(|ctl| ctl.picker_edit(index(i))));
        wire!(on_close_requested, |core| core
            .act(Controller::close_picker));
        wire!(on_dismissed, |core| core.act(Controller::dismiss_picker));
        wire!(on_help_requested, |core| core
            .act(|ctl| ctl.main_open_help(HelpTopic::Guide)));
        wire!(on_colour_chosen, |core, row, format, copy| core
            .use_colour_row(index(row), index(format), copy));
        wire!(on_colour_open_requested, |core, row| {
            core.act(|ctl| ctl.picker_open_in_colours(index(row)));
        });
        {
            // "Paste as" of a colour row lists its formats; the window asks just before it opens
            // the chooser, and the window grows to fit it once they are in place.
            let weak = Rc::downgrade(self);
            let handle = window.as_weak();
            window.on_colour_choices_requested(move |row| {
                let (Some(core), Some(window)) = (weak.upgrade(), handle.upgrade()) else {
                    return;
                };
                let choices = core.controller.borrow().picker_colour_choices(index(row));
                let name = window.global::<ui::ColourTexts<'_>>().invoke_format_name();
                window.set_colour_choices(model(choices.iter().map(|(f, value)| {
                    ui::ChoiceEntry {
                        label: crate::colour_words::choice_text(*f, &name, value).into(),
                        count: -1,
                        ..Default::default()
                    }
                })));
                redraw_all();
            });
        }
    }
}

/// The Picker window, made the first time the Picker opens. The host keeps one of these.
pub struct PickerUi {
    /// Keeps the binding alive.
    _core: Rc<dyn std::any::Any>,
    window: Box<dyn Fn() -> Option<ui::PickerWindow>>,
    placed: Box<dyn Fn() -> Option<Rect>>,
}

impl std::fmt::Debug for PickerUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickerUi").finish_non_exhaustive()
    }
}

impl PickerUi {
    /// Follows `controller`: the window appears when the Picker opens (the hotkey) and goes
    /// when it closes. `anchor` is shared with the fill form.
    pub fn new<S, C, K, T>(
        controller: &Rc<RefCell<Controller<S, C, K, T>>>,
        host: Rc<dyn PopupHost>,
        anchor: AnchorMemory,
    ) -> Self
    where
        S: Library + 'static,
        C: Clipboard + ClipboardText + 'static,
        K: Keyboard + 'static,
        T: Timing + 'static,
    {
        let shell = Shell::new(Rc::clone(&host), true);
        let core = Rc::new_cyclic(|weak: &Weak<PickerCore<S, C, K, T>>| {
            let weak = weak.clone();
            PickerCore {
                controller: Rc::clone(controller),
                host,
                anchor,
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

    /// The window, once the Picker has been opened.
    #[must_use]
    pub fn window(&self) -> Option<ui::PickerWindow> {
        (self.window)()
    }

    /// Where the window was last put, in physical pixels (none until it has been shown, or if
    /// the machine could not say where).
    #[must_use]
    pub fn placed(&self) -> Option<Rect> {
        (self.placed)()
    }
}

fn scope_of(scope: ui::PickerScope) -> Scope {
    match scope {
        ui::PickerScope::All => Scope::All,
        ui::PickerScope::Board => Scope::Board,
        ui::PickerScope::Pinned => Scope::Pinned,
    }
}

const fn scope_in(scope: Scope) -> ui::PickerScope {
    match scope {
        Scope::All => ui::PickerScope::All,
        Scope::Board => ui::PickerScope::Board,
        Scope::Pinned => ui::PickerScope::Pinned,
    }
}

/// Where a result lives, as the row's list label: `Board · List` (U+00B7 with spaces), or the
/// list alone when the board has no name.
#[must_use]
pub fn picker_place(board: &str, list: &str) -> String {
    if board.is_empty() {
        list.to_owned()
    } else {
        format!("{board} \u{b7} {list}")
    }
}

/// One result as the window's row. A sensitive item shows only the mask, never its preview.
#[must_use]
pub fn picker_row(entry: &Entry, dark: bool) -> ui::PickerRow {
    let marks = entry.marks.fields();
    let (swatch, has_swatch) =
        crate::colour_words::swatch_of_kind(entry.kind, &entry.preview, entry.sensitive);
    ui::PickerRow {
        kind: item_kind_of(entry.kind, entry.sensitive),
        title: entry.title.as_str().into(),
        mark_before: marks.mark_before.as_str().into(),
        mark_text: marks.mark_text.as_str().into(),
        // The windowed first line replaces the preview only for a marked row; a sensitive row has none.
        preview: if entry.sensitive {
            SharedString::new()
        } else {
            marks
                .preview_line
                .as_deref()
                .unwrap_or(entry.preview.as_str())
                .into()
        },
        preview_before: marks.preview_before.as_str().into(),
        preview_match: marks.preview_match.as_str().into(),
        tag_hit: marks.tag_hit.as_str().into(),
        masked_preview: if entry.sensitive {
            MASKED_PREVIEW.into()
        } else {
            SharedString::new()
        },
        // The place is built here, not in the window (shared-look spec §2).
        board_name: SharedString::new(),
        list_name: picker_place(&entry.board, &entry.list).into(),
        list_color: list_color(entry.accent, dark),
        image: slint::Image::default(),
        swatch,
        has_swatch,
    }
}

/// Sets the window's properties from `picker`, skipping what `previous` already shows.
fn render(
    window: &ui::PickerWindow,
    previous: Option<&Shown>,
    view: &View,
    picker: &PickerView,
    can_open: bool,
) {
    let differs = |same: &dyn Fn(&Shown) -> bool| previous.is_none_or(|p| !same(p));
    if differs(&|p| p.dark == view.dark) {
        window.set_dark(view.dark);
    }
    // Not behind `differs`: the text size is not part of the view.
    crate::text_scale::apply(&window.global::<ui::SystemLook<'_>>());
    if differs(&|p| {
        (p.high_contrast, p.accent, p.reduced_motion)
            == (view.high_contrast, view.accent, view.reduced_motion)
    }) {
        set_look_globals(
            &window.global::<ui::SystemLook<'_>>(),
            view.high_contrast.as_ref(),
            view.accent.as_ref(),
            view.reduced_motion,
        );
    }
    if differs(&|p| p.picker.rows == picker.rows && p.dark == view.dark) {
        window.set_rows(model(picker.rows.iter().map(|e| picker_row(e, view.dark))));
    }
    if differs(&|p| p.picker.footer == picker.footer) {
        let (app, title) = match &picker.footer {
            Footer::PastesInto { app, title } => (app.as_str(), title.as_str()),
            Footer::Copies => ("", ""),
        };
        window.set_target_name(app.into());
        window.set_target_title(title.into());
    }
    window.set_colour_can_open(can_open);
    if window.get_query().as_str() != picker.query {
        window.set_query(picker.query.as_str().into());
    }
    if window.get_scope() != scope_in(picker.scope) {
        window.set_scope(scope_in(picker.scope));
    }
    let selected = crate::binding::to_i32(picker.selected);
    if window.get_selected_index() != selected {
        window.set_selected_index(selected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_mark::Marks;

    fn entry(sensitive: bool) -> Entry {
        Entry {
            id: snipdeck_core::ItemId::generate(),
            kind: snipdeck_core::ItemKind::Text,
            sensitive,
            title: "Login".into(),
            preview: "hunter2-7f3a".into(),
            board: "Work".into(),
            list: "Logins".into(),
            accent: None,
            marks: Marks::default(),
        }
    }

    #[test]
    fn picker_mask_has_no_tail() {
        let row = picker_row(&entry(true), false);
        assert_eq!(row.masked_preview.as_str(), "\u{2022}".repeat(12));
        assert!(!row.masked_preview.contains("7f3a"));
        assert_eq!(row.preview.as_str(), "");
    }

    #[test]
    fn picker_row_carries_the_marks() {
        let mut marked = entry(false);
        marked.marks = crate::search_mark::marks("Login", "hunter2-7f3a", false, None, "hun");
        let row = picker_row(&marked, false);
        assert_eq!(row.mark_text.as_str(), "");
        assert_eq!(row.preview_match.as_str(), "hun");
        assert_eq!(row.preview.as_str(), "hunter2-7f3a");
        marked.marks = crate::search_mark::marks("Login", "x", false, None, "log");
        let row = picker_row(&marked, false);
        assert_eq!(row.mark_text.as_str(), "Log");
        marked.marks = crate::search_mark::marks("Login", "x", false, Some("work"), "work");
        assert_eq!(picker_row(&marked, false).tag_hit.as_str(), "work");
    }

    #[test]
    fn picker_place_uses_middle_dot() {
        let row = picker_row(&entry(false), false);
        assert_eq!(row.list_name.as_str(), "Work \u{b7} Logins");
        assert_eq!(row.board_name.as_str(), "");
        assert_eq!(picker_place("", "Logins"), "Logins");
    }
}
