//! The Colours tab in the Panel window: copies [`ColourView`] into the `ColourState` global, keeps the little
//! state the controller does not (the remembered hue, the input line while it is being typed in, the surface
//! picture) and routes the tab's gestures to the controller. Docs: `docs/reviews/colour-spec.md` §3.

use std::{cell::RefCell, rc::Rc};

use slint::{Global, ModelRc, SharedString, VecModel};
use snipdeck_colour::{
    Format, InputModel, PickerStyle, Rgba8, Scheme, contrast_ratio, from_hsv, marks, nearest_name,
    parse, ratio_text, render_square, render_wheel, scheme,
};
use snipdeck_core::{ListId, UseMode};
use snipdeck_ui as ui;

use super::Core;
use crate::{
    colours_panel as panel,
    controller::{ColourView, Controller, Library, SaveTarget},
    host::ClipboardText,
};
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

/// Pixel size of the rendered gradient square and the wheel's diameter.
const SQUARE: (u32, u32) = (160, 100);
const WHEEL: u32 = 220;

/// What the binding remembers between redraws.
#[derive(Default)]
pub struct Glue {
    inner: RefCell<Inner>,
}

impl Inner {
    /// The picker's hue, saturation and brightness for `c`: the held ones while they still make
    /// `c` (alpha aside), else read from `c`, keeping the last hue where `c` has none.
    fn state(&mut self, c: Rgba8) -> panel::Hsb {
        if let Some((held, st)) = self.held
            && (held.r, held.g, held.b) == (c.r, c.g, c.b)
        {
            self.held = Some((c, st));
            return st;
        }
        let st = panel::hsb(c, self.held.map_or(0.0, |(_, st)| st.0));
        self.held = Some((c, st));
        st
    }
}

/// Everything a redraw of the tab depends on besides the surface.
#[derive(PartialEq)]
struct Drawn {
    view: ColourView,
    input_error: String,
    other_error: String,
    pastes: bool,
    can_pick: bool,
    names: Vec<String>,
    target: String,
    values_error: String,
    held: Option<(Rgba8, panel::Hsb)>,
}

#[derive(Default)]
struct Inner {
    /// The hue the picker surfaces show, kept when the colour is a grey or black.
    held: Option<(Rgba8, panel::Hsb)>,
    /// The next redraw leaves the input line alone (the text in it made the colour).
    typing_input: bool,
    typing_other: bool,
    /// Errors under the fields, from the last Enter.
    input_error: String,
    other_error: String,
    values_error: String,
    /// The colours of the three chip groups and the lists of the chooser, as last drawn.
    groups: [Vec<Rgba8>; 3],
    lists: Vec<ListId>,
    last_colour: Option<Rgba8>,
    /// The picture on the surface is for this.
    surface: Option<(PickerStyle, i32)>,
    /// What was drawn last, so an unchanged tab is not rewritten.
    drawn: Option<Drawn>,
    /// The tab was just opened: the next redraw rewrites the input line.
    fresh: bool,
}

fn color(c: Rgba8) -> slint::Color {
    slint::Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

fn name_text(t: &ui::ColourTexts<'_>, c: Rgba8) -> String {
    let n = nearest_name(c);
    if n.exact {
        t.invoke_exact(n.name.into()).to_string()
    } else {
        t.invoke_near(n.name.into()).to_string()
    }
}

fn swatch(t: &ui::ColourTexts<'_>, c: Rgba8, format: Format) -> ui::ColourSwatch {
    let hex = snipdeck_colour::format(c, format);
    ui::ColourSwatch {
        color: color(c),
        label: t.invoke_swatch_label(c.to_hex().into(), name_text(t, c).into()),
        hex: hex.into(),
        translucent: c.a < 255,
    }
}

fn contrast_row(
    t: &ui::ColourTexts<'_>,
    name: SharedString,
    fg: Rgba8,
    bg: Rgba8,
) -> ui::ColourContrastRow {
    let ratio = contrast_ratio(fg, bg);
    let m = marks(ratio);
    let text = ratio_text(ratio);
    ui::ColourContrastRow {
        speak: t.invoke_contrast_speak(name.clone(), text.as_str().into(), m.aa, m.aaa),
        name,
        bg: color(bg),
        ratio: text.into(),
        aa: m.aa,
        aa_large: m.aa_large,
        aaa: m.aaa,
        aaa_large: m.aaa_large,
        ui: m.ui,
    }
}

fn model_index(m: InputModel) -> i32 {
    match m {
        InputModel::Hex => 0,
        InputModel::Rgb => 1,
        InputModel::Hsl => 2,
        InputModel::Hsb => 3,
        InputModel::Cmyk => 4,
    }
}

fn model_of(i: i32) -> InputModel {
    match i {
        1 => InputModel::Rgb,
        2 => InputModel::Hsl,
        3 => InputModel::Hsb,
        4 => InputModel::Cmyk,
        _ => InputModel::Hex,
    }
}

fn scheme_of(i: i32) -> Scheme {
    usize::try_from(i)
        .ok()
        .and_then(|i| Scheme::ALL.get(i).copied())
        .unwrap_or(Scheme::Complementary)
}

fn image(buf: &[u8], w: u32, h: u32) -> slint::Image {
    slint::Image::from_rgba8(
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(buf, w, h),
    )
}

impl Glue {
    /// The tab was opened: the input line shows the current colour again.
    pub fn opened(&self) {
        let mut s = self.inner.borrow_mut();
        s.fresh = true;
        s.input_error.clear();
        s.other_error.clear();
        s.values_error.clear();
        s.drawn = None;
    }

    /// Copies the tab's state into the window. Cheap when nothing changed.
    pub fn sync(
        &self,
        window: &ui::PanelWindow,
        view: &ColourView,
        lists: &[(ListId, String)],
        eyedropper: (bool, bool),
    ) {
        let state = ui::ColourState::get(window);
        let t = ui::ColourTexts::get(window);
        let mut s = self.inner.borrow_mut();
        let c = view.current;
        let target = window.get_target_name().to_string();
        let pastes = !target.is_empty();
        let names: Vec<String> = lists.iter().map(|(_, n)| n.clone()).collect();
        s.state(c);
        let key = key_of(view, &s, pastes, eyedropper, &names, &target);
        if s.drawn.as_ref() == Some(&key) && !s.fresh {
            // Nothing to draw: a typed literal that is the colour already must not leave its flag.
            s.typing_input = false;
            s.typing_other = false;
            return;
        }
        s.lists = lists.iter().map(|(id, _)| *id).collect();
        let changed = s.last_colour != Some(c);
        s.last_colour = Some(c);

        state.set_current(color(c));
        state.set_current_translucent(c.a < 255);
        state.set_current_label(t.invoke_swatch_label(c.to_hex().into(), name_text(&t, c).into()));
        state.set_name_text(name_text(&t, c).into());
        if s.fresh || (changed && !s.typing_input) {
            state.set_input_text(snipdeck_colour::format(c, view.format).into());
            s.input_error.clear();
        }
        s.typing_input = false;
        if s.fresh || (changed && !s.typing_other) || state.get_other_text().is_empty() {
            state.set_other_text(view.other.to_hex().into());
        }
        s.typing_other = false;
        s.fresh = false;
        state.set_input_error(SharedString::from(s.input_error.as_str()));
        state.set_other_error(SharedString::from(s.other_error.as_str()));
        state.set_values_error(SharedString::from(s.values_error.as_str()));

        s.groups = chips(&state, &t, view);

        picker(&state, &mut s, view);
        state.set_use_hint(if pastes {
            t.invoke_use_hint_paste(target.as_str().into())
        } else {
            t.invoke_use_hint_copy()
        });
        state.set_has_saved_list(view.has_saved_list);
        state.set_eyedropper_available(eyedropper.0);
        state.set_eyedropper_running(eyedropper.1);
        state.set_list_names(model(
            names
                .iter()
                .map(|n| SharedString::from(n.as_str()))
                .collect(),
        ));
        s.drawn = Some(key);
    }
}

/// The chips, the format rows and the contrast lines. Returns the three chip groups.
fn chips(
    state: &ui::ColourState<'_>,
    t: &ui::ColourTexts<'_>,
    view: &ColourView,
) -> [Vec<Rgba8>; 3] {
    let c = view.current;
    let recents = view.recents.clone();
    let scheme_colours = scheme(c, view.scheme);
    state.set_recents(model(
        recents.iter().map(|c| swatch(t, *c, view.format)).collect(),
    ));
    state.set_saved(model(
        view.saved
            .iter()
            .map(|c| swatch(t, *c, view.format))
            .collect(),
    ));
    state.set_scheme(model(
        scheme_colours
            .iter()
            .map(|c| swatch(t, *c, view.format))
            .collect(),
    ));
    let groups = [recents, view.saved.clone(), scheme_colours];

    // Formats.
    state.set_formats(model(
        Format::ALL
            .iter()
            .map(|f| ui::ColourFormatRow {
                label: f.label().into(),
                value: if *f == Format::Name {
                    name_text(t, c).into()
                } else {
                    snipdeck_colour::format(c, *f).into()
                },
                is_default: *f == view.format,
                can_be_default: f.can_be_default(),
            })
            .collect(),
    ));

    // Contrast.
    state.set_contrast(model(vec![
        contrast_row(t, t.invoke_white(), c, Rgba8::rgb(255, 255, 255)),
        contrast_row(t, t.invoke_black(), c, Rgba8::rgb(0, 0, 0)),
    ]));
    state.set_other(color(view.other));
    state.set_other_row(contrast_row(t, t.invoke_other(), c, view.other));
    groups
}

/// The picker: the models, the surface picture, the marker and the channel fields.
fn picker(state: &ui::ColourState<'_>, s: &mut Inner, view: &ColourView) {
    let c = view.current;
    state.set_picker_open(view.picker_open);
    state.set_picker_style(i32::from(view.style == PickerStyle::Wheel));
    state.set_input_model(model_index(view.input_model));
    state.set_scheme_index(
        Scheme::ALL
            .iter()
            .position(|x| *x == view.scheme)
            .map_or(0, |i| i32::try_from(i).unwrap_or(0)),
    );
    let (hue, sat, bri) = s.state(c);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a hue or a brightness of at most 360 rounds to a small integer"
    )]
    let key = match view.style {
        PickerStyle::Square => (view.style, (hue * 2.0).round() as i32),
        PickerStyle::Wheel => (view.style, bri.round() as i32),
    };
    if s.surface != Some(key) {
        s.surface = Some(key);
        state.set_surface(match view.style {
            PickerStyle::Square => {
                image(&render_square(hue, SQUARE.0, SQUARE.1), SQUARE.0, SQUARE.1)
            }
            PickerStyle::Wheel => image(&render_wheel(bri, WHEEL), WHEEL, WHEEL),
        });
    }
    let (mx, my) = panel::marker(view.style, hue, sat, bri);
    state.set_marker_x(mx);
    state.set_marker_y(my);
    state.set_hue(hue);
    state.set_bri(bri);
    state.set_alpha(f32::from(c.a) / 255.0 * 100.0);
    state.set_channels(model(
        panel::channels(c, view.input_model, s.state(c))
            .into_iter()
            .map(|ch| ui::ColourChannel {
                label: ch.label.into(),
                text: ch.text.into(),
            })
            .collect(),
    ));
}

fn key_of(
    view: &ColourView,
    s: &Inner,
    pastes: bool,
    eyedropper: (bool, bool),
    names: &[String],
    target: &str,
) -> Drawn {
    Drawn {
        view: view.clone(),
        input_error: s.input_error.clone(),
        other_error: s.other_error.clone(),
        pastes,
        can_pick: eyedropper.0 && !eyedropper.1,
        names: names.to_vec(),
        target: target.to_owned(),
        values_error: s.values_error.clone(),
        held: s.held,
    }
}

impl<S, C, K, T> Core<S, C, K, T>
where
    S: Library + 'static,
    C: Clipboard + ClipboardText + 'static,
    K: Keyboard + 'static,
    T: Timing + 'static,
{
    /// The words the controller keeps for things made later (the home list's name, the eyedropper
    /// overlay's texts), in the language the window is in now.
    pub(super) fn refresh_words(ctl: &mut Controller<S, C, K, T>, window: &ui::PanelWindow) {
        let texts = ui::ColourTexts::get(window);
        ctl.set_eyedrop_texts(snipdeck_platform::eyedrop::EyedropTexts {
            hint: texts.invoke_eyedrop_hint().to_string(),
            hdr_note: texts.invoke_eyedrop_hdr_note().to_string(),
        });
        ctl.set_colours_list_name(&texts.invoke_colours_list());
    }

    /// Draws the Colours tab (a no-op while it is not the one shown).
    pub(super) fn sync_colours(&self, window: &ui::PanelWindow) {
        if !window.get_colours_open() {
            return;
        }
        let (view, lists, eyedropper) = {
            let ctl = self.controller.borrow();
            (
                ctl.colour_view(),
                ctl.colour_save_targets(),
                ctl.eyedropper_state(),
            )
        };
        self.colours.sync(window, &view, &lists, eyedropper);
    }

    fn colour_view(&self) -> ColourView {
        self.controller.borrow().colour_view()
    }

    /// The colour of chip `i` of `group` (0 recents, 1 saved, 2 scheme).
    fn chip(&self, group: i32, i: i32) -> Option<Rgba8> {
        let s = self.colours.inner.borrow();
        s.groups
            .get(usize::try_from(group).ok()?)?
            .get(usize::try_from(i).ok()?)
            .copied()
    }

    /// Picker gestures and typed values set the colour and the hue, saturation and brightness it
    /// was made from.
    fn set_picked(self: &Rc<Self>, c: Rgba8, st: panel::Hsb) {
        self.colours.inner.borrow_mut().held = Some((c, st));
        self.act(|ctl| ctl.set_colour(c));
    }

    /// The picker's state for the colour now.
    fn picker_state(&self) -> (Rgba8, panel::Hsb) {
        let current = self.colour_view().current;
        (current, self.colours.inner.borrow_mut().state(current))
    }

    fn save_target(&self, i: i32) -> SaveTarget {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.colours.inner.borrow().lists.get(i).copied())
            .map_or(SaveTarget::Home, SaveTarget::List)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one flat list of the tab's callbacks, each a few lines"
    )]
    pub(super) fn wire_colours(self: &Rc<Self>, window: &ui::PanelWindow) {
        let state = ui::ColourState::get(window);
        self.act(|ctl| Self::refresh_words(ctl, window));

        {
            let core = Rc::clone(self);
            window.on_colours_toggled(move |open| {
                if open {
                    core.colours.opened();
                    core.act(|_| ());
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_input_edited(move |text| {
                // A whole colour literal applies live; the error waits for Enter.
                if let Ok(c) = parse(text.as_str()) {
                    core.colours.inner.borrow_mut().typing_input = true;
                    core.colours.inner.borrow_mut().input_error.clear();
                    core.act(|ctl| ctl.set_colour(c));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_input_accepted(move |text| {
                if let Ok(c) = parse(text.as_str()) {
                    core.colours.inner.borrow_mut().input_error.clear();
                    core.colours.inner.borrow_mut().fresh = true;
                    core.act(|ctl| ctl.set_colour(c));
                } else {
                    let Some(w) = core.window.upgrade() else {
                        return;
                    };
                    let sample = snipdeck_colour::format(core.colour_view().current, Format::Hex);
                    core.colours.inner.borrow_mut().input_error = ui::ColourTexts::get(&w)
                        .invoke_not_a_colour(sample.into())
                        .to_string();
                    core.act(|_| ());
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_other_edited(move |text| {
                if let Ok(c) = parse(text.as_str()) {
                    core.colours.inner.borrow_mut().typing_other = true;
                    core.colours.inner.borrow_mut().other_error.clear();
                    core.act(|ctl| ctl.set_contrast_with(c));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_other_accepted(move |text| {
                if let Ok(c) = parse(text.as_str()) {
                    core.colours.inner.borrow_mut().other_error.clear();
                    core.act(|ctl| ctl.set_contrast_with(c));
                } else {
                    let Some(w) = core.window.upgrade() else {
                        return;
                    };
                    core.colours.inner.borrow_mut().other_error = ui::ColourTexts::get(&w)
                        .invoke_not_a_colour("#ffffff".into())
                        .to_string();
                    core.act(|_| ());
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_eyedropper(move || core.act_effects(Controller::start_eyedropper));
        }
        // Chips: a click makes the colour current; Shift+click copies it.
        {
            let core = Rc::clone(self);
            state.on_swatch_used(move |group, i| {
                if let Some(c) = core.chip(group, i) {
                    core.act(|ctl| ctl.set_colour(c));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swatch_copied(move |group, i| {
                if let Some(c) = core.chip(group, i) {
                    core.act_effects(|ctl| {
                        let f = ctl.colour_view().format;
                        let used = ctl.use_colour(c, f, UseMode::Copy);
                        ctl.after_use(&used)
                    });
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swatch_saved(move |group, i, list| {
                if let Some(c) = core.chip(group, i) {
                    let to = core.save_target(list);
                    core.act(|ctl| ctl.save_colour(c, to));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swatch_removed(move |group, i| {
                if group == 0
                    && let Some(c) = core.chip(group, i)
                {
                    core.act(|ctl| ctl.forget_recent_colour(c));
                }
            });
        }
        // Format rows: a click pastes into the Target (copies when there is none), Shift copies.
        for (mode, copy) in [(UseMode::Paste, false), (UseMode::Copy, true)] {
            let core = Rc::clone(self);
            let run = move |i: i32| {
                let Some(f) = usize::try_from(i)
                    .ok()
                    .and_then(|i| Format::ALL.get(i).copied())
                else {
                    return;
                };
                core.act_effects(|ctl| {
                    let c = ctl.colour_view().current;
                    let used = ctl.use_colour(c, f, mode);
                    ctl.after_use(&used)
                });
            };
            if copy {
                state.on_format_copied(run);
            } else {
                state.on_format_used(run);
            }
        }
        {
            let core = Rc::clone(self);
            state.on_format_made_default(move |i| {
                if let Some(f) = usize::try_from(i)
                    .ok()
                    .and_then(|i| Format::ALL.get(i).copied())
                {
                    core.act(|ctl| ctl.set_colour_format(f));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_format_saved(move |_, list| {
                let to = core.save_target(list);
                core.act(|ctl| {
                    let c = ctl.colour_view().current;
                    ctl.save_colour(c, to)
                });
            });
        }
        {
            let core = Rc::clone(self);
            state.on_save(move |list| {
                let to = core.save_target(list);
                core.act(|ctl| {
                    let c = ctl.colour_view().current;
                    ctl.save_colour(c, to)
                });
            });
        }
        {
            let core = Rc::clone(self);
            state.on_open_saved(move || core.act_effects(Controller::open_saved_colours));
        }
        {
            let core = Rc::clone(self);
            state.on_swap(move || core.act(Controller::swap_contrast));
        }
        {
            let core = Rc::clone(self);
            state.on_scheme_picked(move |i| core.act(|ctl| ctl.set_colour_scheme(scheme_of(i))));
        }
        {
            let core = Rc::clone(self);
            state.on_picker_toggled(move |open| core.act(|ctl| ctl.set_colour_adjust_open(open)));
        }
        {
            let core = Rc::clone(self);
            state.on_style_picked(move |i| {
                core.colours.inner.borrow_mut().surface = None;
                core.act(|ctl| {
                    ctl.set_colour_picker_style(if i == 1 {
                        PickerStyle::Wheel
                    } else {
                        PickerStyle::Square
                    });
                });
            });
        }
        {
            let core = Rc::clone(self);
            state.on_model_picked(move |i| {
                core.colours.inner.borrow_mut().values_error.clear();
                core.act(|ctl| ctl.set_colour_input_model(model_of(i)));
            });
        }
        // The surface and the sliders.
        {
            let core = Rc::clone(self);
            state.on_surface_moved(move |x, y, _done| {
                let (current, st) = core.picker_state();
                let style = core.colour_view().style;
                let (c, next) = panel::pick_hsb(style, x, y, st, current.a);
                core.set_picked(c, next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_surface_nudged(move |dx, dy| {
                let (current, st) = core.picker_state();
                let style = core.colour_view().style;
                let (c, next) = panel::nudge(style, st, dx, dy, current.a);
                core.set_picked(c, next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_hue_moved(move |h| {
                let (current, st) = core.picker_state();
                let next = (h.min(359.999), st.1, st.2);
                core.set_picked(hsb_colour(next, current.a), next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_bri_moved(move |b| {
                let (current, st) = core.picker_state();
                let next = (st.0, st.1, b.clamp(0.0, 100.0));
                core.set_picked(hsb_colour(next, current.a), next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_alpha_moved(move |a| {
                let c = core.colour_view().current;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "clamped to 0..=100 first, so the result is 0..=255"
                )]
                let a = (a.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8;
                core.act(|ctl| ctl.set_colour(Rgba8 { a, ..c }));
            });
        }
        // Typed values.
        {
            let core = Rc::clone(self);
            state.on_channel_committed(move |i, text| {
                let (current, st) = core.picker_state();
                let model = core.colour_view().input_model;
                let result = usize::try_from(i)
                    .map_err(|_| panel::Reject::NoSuchChannel)
                    .and_then(|i| panel::commit(current, model, i, text.as_str(), st));
                core.apply_channel(result);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_channel_stepped(move |i, d| {
                let (current, st) = core.picker_state();
                let model = core.colour_view().input_model;
                let result = usize::try_from(i)
                    .map_err(|_| panel::Reject::NoSuchChannel)
                    .and_then(|i| panel::step(current, model, i, d, st));
                core.apply_channel(result);
            });
        }
    }

    fn apply_channel(self: &Rc<Self>, result: Result<(Rgba8, panel::Hsb), panel::Reject>) {
        match result {
            Ok((c, st)) => {
                self.colours.inner.borrow_mut().values_error.clear();
                self.set_picked(c, st);
            }
            Err(panel::Reject::NotANumber(label)) => {
                if let Some(w) = self.window.upgrade() {
                    let text = ui::ColourTexts::get(&w).invoke_value_error(label.into());
                    self.colours.inner.borrow_mut().values_error = text.to_string();
                }
                self.act(|_| ());
            }
            Err(panel::Reject::NoSuchChannel) => {}
        }
    }
}

fn hsb_colour((h, sat, bri): panel::Hsb, alpha: u8) -> Rgba8 {
    from_hsv(
        snipdeck_colour::Hsv {
            h: f64::from(h),
            s: f64::from(sat) / 100.0,
            v: f64::from(bri) / 100.0,
        },
        alpha,
    )
}
