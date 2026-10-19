//! Binds the controller's template fill form ([`View::fill`](crate::controller::View)) to the
//! Slint `TemplateFillWindow`. Like the Picker's window it is made the first time it is needed,
//! shown without being activated, and placed at the Target's caret; a form opened from the
//! Picker opens where the Picker was.

use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

use slint::{ComponentHandle, Model};
use snipdeck_core::UseMode;
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};
use snipdeck_ui as ui;

use crate::{
    binding::{index, model, redraw_all, register_redraw},
    controller::{Controller, FillView, Library, View},
    fill::{Field, FieldKind},
    frame::Rect,
    host::ClipboardText,
    panel::Footer,
    picker_binding::AnchorMemory,
    placement::{self, PopupHost},
    popup_window::{Shell, set_high_contrast},
    toast::{Toast, ToastKind, ToastMessage},
};

/// The form's width in logical pixels (the window's `preferred-width`).
pub const FILL_WIDTH: f32 = 440.0;

#[derive(Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a snapshot of independent switches, not a state"
)]
struct Shown {
    dark: bool,
    high_contrast: Option<HighContrast>,
    reduced_motion: bool,
    /// The form on screen (`seq`) and its preview.
    seq: u64,
    preview: String,
    masked: bool,
    copies: bool,
}

struct FillCore<S, C, K, T> {
    controller: Rc<RefCell<Controller<S, C, K, T>>>,
    host: Rc<dyn PopupHost>,
    anchor: AnchorMemory,
    shell: Rc<Shell<ui::TemplateFillWindow>>,
    shown: RefCell<Option<Shown>>,
    size: Cell<(f32, f32)>,
    redraw: Rc<dyn Fn()>,
}

impl<S, C, K, T> FillCore<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    fn act<R>(&self, f: impl FnOnce(&mut Controller<S, C, K, T>) -> R) -> R {
        let result = f(&mut self.controller.borrow_mut());
        redraw_all();
        result
    }

    fn sync(self: &Rc<Self>) {
        let (view, copies) = {
            let ctl = self.controller.borrow();
            let copies = ctl.fill().is_some_and(|f| f.mode() == UseMode::Copy);
            (ctl.view(), copies)
        };
        if let Some(fill) = &view.fill {
            self.draw(&view, fill, copies);
        } else {
            self.shell.hide();
            *self.shown.borrow_mut() = None;
        }
    }

    fn draw(self: &Rc<Self>, view: &View, fill: &FillView, copies: bool) {
        let made = self.shell.ensure(ui::TemplateFillWindow::new);
        let (window, created) = match made {
            Ok(made) => made,
            Err(e) => {
                crate::trace!("cannot make the fill window: {e}");
                self.give_up();
                return;
            }
        };
        if created {
            self.wire(&window);
        }
        // A footer that copies (no paste Target) makes the button say "Copy".
        let copies = copies || view.footer == Footer::Copies;
        let new_form = self
            .shown
            .borrow()
            .as_ref()
            .is_none_or(|s| s.seq != fill.seq);
        render(&window, self.shown.borrow().as_ref(), view, fill, copies);
        *self.shown.borrow_mut() = Some(Shown {
            dark: view.dark,
            high_contrast: view.high_contrast,
            reduced_motion: view.reduced_motion,
            seq: fill.seq,
            preview: fill.preview.clone(),
            masked: fill.masked,
            copies,
        });
        let size = (FILL_WIDTH, window.get_content_height());
        if self.shell.is_open() {
            if size != self.size.get() {
                let frame = self
                    .anchor
                    .get()
                    .and_then(|anchor| placement::place_at(&*self.host, anchor, size));
                self.shell.place(&window, frame, size);
            }
            if new_form {
                window.invoke_focus_field(0);
            }
        } else {
            // The caret of the Target if it can still be read, else where the Picker was, else
            // the mouse.
            let caret = self.controller.borrow().target_caret();
            let placed = match (caret, self.anchor.get()) {
                (None, Some(anchor)) => {
                    placement::place_at(&*self.host, anchor, size).map(|frame| (frame, anchor))
                }
                _ => placement::place_popup(&*self.host, caret, size),
            };
            self.anchor.set(placed.map(|(_, anchor)| anchor));
            if let Err(e) = self.shell.show(&window, placed.map(|(f, _)| f), size) {
                crate::trace!("cannot show the fill form: {e}");
                self.give_up();
                return;
            }
            window.invoke_focus_field(0);
        }
        self.size.set(size);
    }

    fn give_up(self: &Rc<Self>) {
        self.act(|ctl| {
            ctl.cancel_fill();
            ctl.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::WindowNotShown,
            });
        });
    }

    /// The form was submitted with every required field filled. The window is off the screen
    /// before the paste, so the keys go into the Target and not into the form.
    fn submit(&self, values: &[String]) {
        let submission = self.controller.borrow_mut().take_fill(values);
        if let Some(submission) = submission {
            self.shell.hide();
            crate::trace!("fill form submitted");
            self.controller.borrow_mut().run_fill(submission);
        }
        redraw_all();
    }

    fn wire(self: &Rc<Self>, window: &ui::TemplateFillWindow) {
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
        wire!(on_answer_changed, |core, i, value| core
            .act(|ctl| ctl.fill_changed(index(i), value.as_str())));
        wire!(on_submitted, |core, fields| {
            let values: Vec<String> = fields.iter().map(|f| f.value.to_string()).collect();
            core.submit(&values);
        });
        wire!(on_cancelled, |core| core.act(Controller::cancel_fill));
        wire!(on_drag_requested, |core| core.shell.drag());
    }
}

/// The form window, made the first time a form opens. The host keeps one of these.
pub struct FillUi {
    /// Keeps the binding alive.
    _core: Rc<dyn std::any::Any>,
    window: Box<dyn Fn() -> Option<ui::TemplateFillWindow>>,
    placed: Box<dyn Fn() -> Option<Rect>>,
}

impl std::fmt::Debug for FillUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FillUi").finish_non_exhaustive()
    }
}

impl FillUi {
    /// Follows `controller`: the window appears when a form opens and goes when it closes.
    /// `anchor` is shared with the Picker.
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
        let core = Rc::new_cyclic(|weak: &Weak<FillCore<S, C, K, T>>| {
            let weak = weak.clone();
            FillCore {
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

    /// The window, once a form has been opened.
    #[must_use]
    pub fn window(&self) -> Option<ui::TemplateFillWindow> {
        (self.window)()
    }

    /// Where the window was last put, in physical pixels (none until it has been shown, or if
    /// the machine could not say where).
    #[must_use]
    pub fn placed(&self) -> Option<Rect> {
        (self.placed)()
    }
}

/// One prompt as the window's field.
#[must_use]
pub fn form_field(field: &Field) -> ui::FormField {
    ui::FormField {
        label: field.label.as_str().into(),
        kind: match field.kind {
            FieldKind::Text => ui::TemplateFieldKind::Text,
            FieldKind::Choice => ui::TemplateFieldKind::Choice,
        },
        value: field.value.as_str().into(),
        options: model(field.options.iter().map(|o| o.as_str().into())),
        choice_index: crate::binding::to_i32(field.choice_index),
        required: field.required,
        placeholder: slint::SharedString::new(),
        error: slint::SharedString::new(),
    }
}

fn render(
    window: &ui::TemplateFillWindow,
    previous: Option<&Shown>,
    view: &View,
    fill: &FillView,
    copies: bool,
) {
    let differs = |same: &dyn Fn(&Shown) -> bool| previous.is_none_or(|p| !same(p));
    if differs(&|p| p.dark == view.dark) {
        window.set_dark(view.dark);
    }
    if differs(&|p| {
        p.high_contrast == view.high_contrast && p.reduced_motion == view.reduced_motion
    }) {
        set_high_contrast(
            &window.global::<ui::SystemLook<'_>>(),
            view.high_contrast.as_ref(),
            view.reduced_motion,
        );
    }
    // The fields are set once per form: afterwards the window holds what is typed.
    if differs(&|p| p.seq == fill.seq) {
        window.set_template_title(fill.title.as_str().into());
        window.set_fields(model(fill.fields.iter().map(form_field)));
    }
    if differs(&|p| p.preview == fill.preview && p.masked == fill.masked) {
        window.set_preview(if fill.masked {
            crate::binding::MASKED_PREVIEW.into()
        } else {
            fill.preview.as_str().into()
        });
    }
    if differs(&|p| p.copies == copies) {
        window.set_copies(copies);
    }
}
