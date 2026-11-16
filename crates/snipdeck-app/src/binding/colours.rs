//! The Colours tab in the Panel window: copies [`ColourView`] into the `ColourState` global, keeps the little
//! state the controller does not (the remembered hue, the input line while it is being typed in, the surface
//! picture) and routes the tab's gestures to the controller. Docs: `docs/reviews/colour-spec.md` §3.

use std::{cell::RefCell, rc::Rc};

use slint::{Global, Model, ModelRc, SharedString, VecModel};
use snipdeck_colour::{
    Format, InputModel, PickerStyle, Rgba8, Scheme, apca, composite, contrast_over, from_hsv,
    marks, nearest_name, nearest_passing, parse, ratio_text, render_square, render_wheel, scheme,
};
use snipdeck_core::{ItemId, ListId, UseMode};
use snipdeck_ui as ui;

use super::Core;
use crate::{
    colours_panel as panel,
    controller::{ColourView, Controller, Library, SaveList, SaveTarget},
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

    /// What the HSL or CMYK row last held, while it still makes `c` in `model`.
    fn memo(&self, c: Rgba8, model: InputModel) -> Option<panel::Memo> {
        match &self.typed {
            Some((m, was, values)) if *m == model && (was.r, was.g, was.b) == (c.r, c.g, c.b) => {
                Some(values.clone())
            }
            _ => None,
        }
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
    typed: Option<(InputModel, Rgba8, panel::Memo)>,
    backdrop_black: bool,
}

#[derive(Default)]
struct Inner {
    /// The HSL or CMYK values as last typed or stepped, next to the colour they made.
    typed: Option<(InputModel, Rgba8, panel::Memo)>,
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
    /// A translucent colour is judged over black instead of white.
    backdrop_black: bool,
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

/// The Name row: the keyword when it is exactly the colour, else the explicit "Use royalblue
/// (#4169e1)": a keyword that is another colour is never pasted.
fn name_row_text(t: &ui::ColourTexts<'_>, c: Rgba8) -> SharedString {
    let n = nearest_name(c);
    if n.exact && c.a == 255 {
        t.invoke_exact(n.name.into())
    } else {
        t.invoke_use_name(n.name.into(), n.rgb.to_hex().into())
    }
}

/// A chip: its caption is always the short hex (a 60 px cell elides a long OKLCH text to the same
/// prefix); the default format's full text is in the label the tooltip and screen reader read.
fn swatch(t: &ui::ColourTexts<'_>, c: Rgba8, format: Format) -> ui::ColourSwatch {
    let full = snipdeck_colour::format(c, format);
    let label = t.invoke_swatch_label(full.as_str().into(), name_text(t, c).into());
    ui::ColourSwatch {
        color: color(c),
        label,
        hex: c.to_hex().into(),
        translucent: c.a < 255,
    }
}

/// One contrast line: `fg` on `bg`, a translucent colour judged over `backdrop`. Carries the APCA
/// lightness contrast, which backdrop was used, and the nearest colour that passes AA.
fn contrast_row(
    t: &ui::ColourTexts<'_>,
    name: SharedString,
    fg: Rgba8,
    bg: Rgba8,
    backdrop: Rgba8,
) -> ui::ColourContrastRow {
    let ratio = contrast_over(fg, bg, backdrop);
    let m = marks(ratio);
    let text = ratio_text(ratio);
    let flat_bg = composite(bg, Rgba8 { a: 255, ..backdrop });
    let flat_fg = composite(fg, flat_bg);
    let over = if fg.a < 255 || bg.a < 255 {
        if backdrop.r == 0 {
            t.invoke_over_black()
        } else {
            t.invoke_over_white()
        }
    } else {
        SharedString::new()
    };
    let fix_text = if m.aa {
        SharedString::new()
    } else {
        nearest_passing(flat_fg, flat_bg, 4.5)
            .map(|c| t.invoke_fix_text(c.to_hex().into()))
            .unwrap_or_default()
    };
    ui::ColourContrastRow {
        apca: t.invoke_apca_text(format!("{:.0}", apca(flat_fg, flat_bg).abs()).into()),
        over,
        fix_text,
        speak: t.invoke_contrast_speak(
            name.clone(),
            text.as_str().into(),
            m.aa,
            m.aa_large,
            m.aaa,
            m.aaa_large,
            m.ui,
        ),
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
        lists: &[SaveList],
        eyedropper: (bool, bool),
    ) {
        let state = ui::ColourState::get(window);
        let t = ui::ColourTexts::get(window);
        let mut s = self.inner.borrow_mut();
        let c = view.current;
        let target = window.get_target_name().to_string();
        let pastes = !target.is_empty();
        let names: Vec<String> = lists
            .iter()
            .map(|l| {
                if l.several_boards {
                    t.invoke_list_in_board(l.board.as_str().into(), l.name.as_str().into())
                        .to_string()
                } else {
                    l.name.clone()
                }
            })
            .collect();
        s.state(c);
        // The ids behind the Save to… entries follow the lists even where no name changed (a list
        // deleted and another made with the same name).
        s.lists = lists.iter().map(|l| l.id).collect();
        let key = key_of(view, &s, pastes, eyedropper, &names, &target);
        if s.drawn.as_ref() == Some(&key) && !s.fresh {
            // Nothing to draw: a typed literal that is the colour already must not leave its flag.
            s.typing_input = false;
            s.typing_other = false;
            return;
        }
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

        s.groups = chips(&state, &t, view, s.backdrop_black);

        picker(&state, &mut s, view);
        state.set_use_hint(if pastes {
            t.invoke_use_hint_paste(target.as_str().into())
        } else {
            t.invoke_use_hint_copy()
        });
        state.set_target(target.as_str().into());
        state.set_has_previous(view.previous.is_some());
        if let Some(p) = view.previous {
            state.set_previous(color(p));
            state.set_previous_label(t.invoke_previous_colour(p.to_hex().into()));
        }
        state.set_has_saved_list(view.has_saved_list);
        state.set_eyedropper_available(eyedropper.0);
        state.set_eyedropper_running(eyedropper.1);
        // "Save to…": every list of every board, then "New list…".
        let mut targets: Vec<ui::MenuEntry> = names
            .iter()
            .map(|n| ui::MenuEntry {
                text: n.as_str().into(),
                ..Default::default()
            })
            .collect();
        targets.push(ui::MenuEntry {
            text: t.invoke_new_list_entry(),
            ..Default::default()
        });
        state.set_save_targets(model(targets));
        state.set_extras(model(
            snipdeck_colour::ExtraFormat::ALL
                .iter()
                .map(|f| ui::MenuEntry {
                    text: snipdeck_colour::extra_text(view.current, *f).into(),
                    ..Default::default()
                })
                .collect(),
        ));
        state.set_copy_as(model(
            Format::ALL
                .iter()
                .map(|f| ui::MenuEntry {
                    text: if *f == Format::Name {
                        t.invoke_format_name()
                    } else {
                        f.label().into()
                    },
                    ..Default::default()
                })
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
    backdrop_black: bool,
) -> [Vec<Rgba8>; 3] {
    let c = view.current;
    let recents = view.recents.clone();
    let scheme_colours = panel::scheme_layout(view.scheme, scheme(c, view.scheme));
    state.set_recents(model(
        recents.iter().map(|c| swatch(t, *c, view.format)).collect(),
    ));
    state.set_saved(model(
        view.saved
            .iter()
            .map(|c| swatch(t, *c, view.format))
            .collect(),
    ));
    state.set_scheme_no_hue(view.scheme != Scheme::TintsShades && !snipdeck_colour::has_hue(c));
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
                    name_row_text(t, c).into()
                } else {
                    snipdeck_colour::format(c, *f).into()
                },
                is_default: *f == view.format,
                can_be_default: f.can_be_default(),
            })
            .collect(),
    ));

    // Contrast. A translucent colour is laid over the backdrop (white, or black on request) and the
    // line says which.
    let backdrop = if backdrop_black {
        Rgba8::rgb(0, 0, 0)
    } else {
        Rgba8::rgb(255, 255, 255)
    };
    state.set_contrast(model(vec![
        contrast_row(t, t.invoke_white(), c, Rgba8::rgb(255, 255, 255), backdrop),
        contrast_row(t, t.invoke_black(), c, Rgba8::rgb(0, 0, 0), backdrop),
    ]));
    state.set_other(color(view.other));
    state.set_other_row(contrast_row(t, t.invoke_other(), c, view.other, backdrop));
    state.set_needs_backdrop(snipdeck_colour::needs_backdrop(c, view.other));
    state.set_backdrop_black(backdrop_black);
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
    state.set_sat(sat);
    state.set_bri(bri);
    state.set_alpha(f32::from(c.a) / 255.0 * 100.0);
    let shown: Vec<ui::ColourChannel> = panel::channels(
        c,
        view.input_model,
        s.state(c),
        s.memo(c, view.input_model).as_deref(),
    )
    .into_iter()
    .map(|ch| ui::ColourChannel {
        label: ch.label.into(),
        text: ch.text.into(),
    })
    .collect();
    set_channels(state, shown);
}

/// Puts the Values row's fields in the window. The same fields (same labels, same count) are updated
/// where they stand: replacing the model would rebuild the field being typed in and drop its focus.
fn set_channels(state: &ui::ColourState<'_>, shown: Vec<ui::ColourChannel>) {
    let current = state.get_channels();
    let same_fields = current.row_count() == shown.len()
        && shown
            .iter()
            .enumerate()
            .all(|(i, ch)| current.row_data(i).is_some_and(|c| c.label == ch.label));
    if same_fields {
        for (i, ch) in shown.into_iter().enumerate() {
            if current.row_data(i).is_some_and(|c| c.text != ch.text) {
                current.set_row_data(i, ch);
            }
        }
    } else {
        state.set_channels(model(shown));
    }
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
        typed: s.typed.clone(),
        backdrop_black: s.backdrop_black,
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
            title: texts.invoke_eyedrop_title().to_string(),
            pick_label: texts.invoke_eyedrop_pick().to_string(),
            unavailable: texts.invoke_eyedrop_unavailable().to_string(),
            ..snipdeck_platform::eyedrop::EyedropTexts::default()
        });
        ctl.set_colours_list_name(&texts.invoke_colours_list());
        ctl.set_new_list_base(&texts.invoke_new_list_name());
        ctl.set_pasted_image_title(&texts.invoke_pasted_image_title());
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
    /// The item of saved chip `i` (the strip is read from the controller, newest first).
    fn saved_id(&self, i: i32) -> Option<ItemId> {
        let i = usize::try_from(i).ok()?;
        self.controller
            .borrow()
            .saved_colours()
            .get(i)
            .map(|(id, _)| *id)
    }

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
        {
            let mut s = self.colours.inner.borrow_mut();
            s.held = Some((c, st));
            s.typed = None;
        }
        self.act(|ctl| ctl.set_colour(c));
    }

    /// A drag step: the colour changes but is not written to the settings until the pointer is
    /// released (`on_settled`).
    fn set_picked_live(self: &Rc<Self>, c: Rgba8, st: panel::Hsb) {
        {
            let mut s = self.colours.inner.borrow_mut();
            s.held = Some((c, st));
            s.typed = None;
        }
        self.act(|ctl| ctl.set_colour_live(c));
    }

    /// The picker's state for the colour now.
    fn picker_state(&self) -> (Rgba8, panel::Hsb, Option<panel::Memo>) {
        let view = self.colour_view();
        let mut s = self.colours.inner.borrow_mut();
        let st = s.state(view.current);
        (view.current, st, s.memo(view.current, view.input_model))
    }

    /// The target of entry `i` of the Save to… menu: a list, "New list…" (the entry after the last
    /// list), or the home list for a negative index.
    fn save_target(&self, i: i32) -> SaveTarget {
        let Ok(i) = usize::try_from(i) else {
            return SaveTarget::Home;
        };
        self.colours
            .inner
            .borrow()
            .lists
            .get(i)
            .copied()
            .map_or(SaveTarget::NewList, SaveTarget::List)
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
                // A whole colour literal applies live, in memory only (words and bare numbers do not
                // count while typing); the error waits for Enter and the setting for a pause.
                if let Ok(c) = snipdeck_colour::parse_live(text.as_str()) {
                    core.colours.inner.borrow_mut().typing_input = true;
                    core.colours.inner.borrow_mut().input_error.clear();
                    core.act(|ctl| ctl.set_colour_live(c));
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
                if let Ok(c) = snipdeck_colour::parse_live(text.as_str()) {
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
            state.on_extra_copied(move |i| {
                core.act_effects(|ctl| {
                    let used = ctl.copy_extra_format(usize::try_from(i).unwrap_or(0));
                    ctl.after_use(&used)
                });
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swatch_copied_as(move |group, i, format| {
                if let Some(c) = core.chip(group, i)
                    && let Some(f) = usize::try_from(format)
                        .ok()
                        .and_then(|f| Format::ALL.get(f).copied())
                {
                    core.act_effects(|ctl| {
                        let used = ctl.use_colour(c, f, UseMode::Copy);
                        ctl.after_use(&used)
                    });
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_copy_current(move || {
                core.act_effects(|ctl| {
                    let (c, f) = {
                        let v = ctl.colour_view();
                        (v.current, v.format)
                    };
                    let used = ctl.use_colour(c, f, UseMode::Copy);
                    ctl.after_use(&used)
                });
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
            state.on_swatch_removed(move |group, i| match group {
                0 => {
                    if let Some(c) = core.chip(group, i) {
                        core.act(|ctl| ctl.forget_recent_colour(c));
                    }
                }
                1 => {
                    if let Some(id) = core.saved_id(i) {
                        core.act(|ctl| ctl.delete_saved_colour(id));
                    }
                }
                _ => {}
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swatch_edited(move |group, i| {
                if group == 1
                    && let Some(id) = core.saved_id(i)
                {
                    core.act_effects(|ctl| ctl.edit_saved_colour(id));
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
                    if snipdeck_colour::copy_text(c, f).is_none() {
                        ctl.use_nearest_name();
                        return Vec::new();
                    }
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
            state.on_previous_restored(move || core.act(Controller::restore_previous_colour));
        }
        {
            let core = Rc::clone(self);
            state.on_paste_colour(move || {
                core.act(Controller::paste_colour);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swap(move || core.act(Controller::swap_contrast));
        }
        {
            let core = Rc::clone(self);
            state.on_backdrop_toggled(move || {
                {
                    let mut s = core.colours.inner.borrow_mut();
                    s.backdrop_black = !s.backdrop_black;
                }
                core.act(|_| ());
            });
        }
        {
            let core = Rc::clone(self);
            state.on_contrast_fixed(move |row| {
                let (fg, other, black) = {
                    let v = core.colour_view();
                    (
                        v.current,
                        v.other,
                        core.colours.inner.borrow().backdrop_black,
                    )
                };
                let bg = match row {
                    0 => Rgba8::rgb(255, 255, 255),
                    1 => Rgba8::rgb(0, 0, 0),
                    _ => other,
                };
                let backdrop = if black {
                    Rgba8::rgb(0, 0, 0)
                } else {
                    Rgba8::rgb(255, 255, 255)
                };
                let flat_bg = composite(bg, backdrop);
                if let Some(fixed) = nearest_passing(composite(fg, flat_bg), flat_bg, 4.5) {
                    core.act(|ctl| ctl.set_colour(Rgba8 { a: 255, ..fixed }));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_swatch_other(move |group, i| {
                if let Some(c) = core.chip(group, i) {
                    core.act(|ctl| ctl.set_contrast_with(c));
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_eyedropper_other(move || {
                core.act_effects(Controller::start_eyedropper_for_other);
            });
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
            state.on_surface_moved(move |x, y, done| {
                let (current, st, _) = core.picker_state();
                let style = core.colour_view().style;
                let (c, next) = panel::pick_hsb(style, x, y, st, current.a);
                if done {
                    core.set_picked(c, next);
                } else {
                    core.set_picked_live(c, next);
                }
            });
        }
        {
            let core = Rc::clone(self);
            state.on_settled(move || core.act(Controller::settle_colour));
        }
        {
            let core = Rc::clone(self);
            state.on_surface_nudged(move |dx, dy| {
                let (current, st, _) = core.picker_state();
                let style = core.colour_view().style;
                let (c, next) = panel::nudge(style, st, dx, dy, current.a);
                core.set_picked(c, next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_surface_paged(move |delta| {
                let (current, st, _) = core.picker_state();
                let (c, next) = panel::page(st, delta, current.a);
                core.set_picked(c, next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_hue_moved(move |h| {
                let (current, st, _) = core.picker_state();
                let next = (panel::slider(h, 359.999), st.1, st.2);
                core.set_picked_live(hsb_colour(next, current.a), next);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_bri_moved(move |b| {
                let (current, st, _) = core.picker_state();
                let next = (st.0, st.1, panel::slider(b, 100.0));
                core.set_picked_live(hsb_colour(next, current.a), next);
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
                let a = (panel::slider(a, 100.0) / 100.0 * 255.0).round() as u8;
                core.act(|ctl| ctl.set_colour_live(Rgba8 { a, ..c }));
            });
        }
        // Typed values.
        {
            let core = Rc::clone(self);
            state.on_channel_committed(move |i, text| {
                let (current, st, memo) = core.picker_state();
                let model = core.colour_view().input_model;
                let result = usize::try_from(i)
                    .map_err(|_| panel::Reject::NoSuchChannel)
                    .and_then(|i| {
                        panel::commit(current, model, i, text.as_str(), st, memo.as_deref())
                    });
                core.apply_channel(result);
            });
        }
        {
            let core = Rc::clone(self);
            state.on_channel_stepped(move |i, d| {
                let (current, st, memo) = core.picker_state();
                let model = core.colour_view().input_model;
                let result = usize::try_from(i)
                    .map_err(|_| panel::Reject::NoSuchChannel)
                    .and_then(|i| panel::step(current, model, i, d, st, memo.as_deref()));
                core.apply_channel(result);
            });
        }
    }

    fn apply_channel(
        self: &Rc<Self>,
        result: Result<(Rgba8, panel::Hsb, Option<panel::Memo>), panel::Reject>,
    ) {
        match result {
            Ok((c, st, memo)) => {
                self.colours.inner.borrow_mut().values_error.clear();
                self.set_picked(c, st);
                let model = self.colour_view().input_model;
                self.colours.inner.borrow_mut().typed = memo.map(|m| (model, c, m));
                // `set_picked` drew the rows before the memo was kept: draw them again with it.
                self.act(|_| ());
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
