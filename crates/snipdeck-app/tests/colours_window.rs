//! The Panel's Colours tab on the real window, bound to a `Controller` over the demo library:
//! it opens, shows every format, follows typed and picked colours, saves to the Colours list, and
//! renders (light and dark). Time is a fixed clock. The renders land in `target/colours-shots`
//! for a look; the assertions measure state and a few pixels.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test code: a failure is a failed test"
)]

use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
};

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use i_slint_core::{
    accessibility::AccessibleStringProperty, item_tree::ParentItemTraversalMode,
    window::WindowInner,
};
use slint::{
    ComponentHandle, Global, Model,
    platform::{Key, WindowEvent},
};
use snipdeck_app::{
    binding::Binding,
    controller::Controller,
    demo,
    screenshot::write_png,
    settings::{Settings, Theme},
};
use snipdeck_colour::{Format, Rgba8, contrast_ratio, format, ratio_text};
use snipdeck_core::Timestamp;
use snipdeck_ui::{
    ColourState, PanelWindow, SystemLook,
    testing::{Frame, Harness},
};

type Ctl = Controller<
    snipdeck_store::Store,
    snipdeck_platform::testing::FakeClipboard,
    snipdeck_platform::testing::FakeKeyboard,
    snipdeck_platform::testing::FakeTiming,
>;

struct Rig {
    h: Harness<PanelWindow>,
    ctl: Rc<RefCell<Ctl>>,
    binding: Binding,
}

fn rig(dark: bool, size: (u32, u32)) -> Rig {
    let now: Timestamp = "2026-10-17T10:30:00Z".parse().expect("timestamp");
    let settings = Settings {
        theme: if dark { Theme::Dark } else { Theme::Light },
        ..Settings::new_install()
    };
    let ctl = Rc::new(RefCell::new(demo::controller(settings, now).expect("demo")));
    let h = Harness::new(PanelWindow::new, size.0, size.1);
    let binding = Controller::bind(
        &ctl,
        &h.component,
        Rc::new(snipdeck_app::placement::NoPopups),
        |_| {},
    );
    h.component.invoke_open_colours();
    Rig { h, ctl, binding }
}

impl Rig {
    fn state(&self) -> ColourState<'_> {
        ColourState::get(&self.h.component)
    }
}

fn shot(name: &str, frame: &Frame) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/colours-shots");
    std::fs::create_dir_all(&dir).expect("dir");
    write_png(&dir.join(format!("{name}.png")), frame).expect("png");
}

fn has_pixel(frame: &Frame, rgb: [u8; 3]) -> bool {
    frame.rgb.as_chunks::<3>().0.contains(&rgb)
}

#[test]
fn the_tab_opens_and_shows_every_format_and_both_contrast_lines() {
    let r = rig(false, (320, 640));
    assert!(r.h.component.get_colours_open());
    let s = r.state();
    assert_eq!(s.get_formats().row_count(), Format::ALL.len());
    assert_eq!(s.get_contrast().row_count(), 2, "white and black");
    assert!(s.get_picker_open(), "the picker is open by default");
    assert!(!s.get_input_text().is_empty());
    assert!(s.get_channels().row_count() >= 1);
    shot("light-320", &r.frame());
}

#[test]
fn typing_a_colour_updates_every_format_and_the_contrast() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#ff0000".into());
    slint::platform::update_timers_and_animations();
    let red = Rgba8::rgb(255, 0, 0);
    assert_eq!(r.ctl.borrow().colour_view().current, red);
    let s = r.state();
    let hex = s.get_formats().row_data(0).expect("hex row");
    assert_eq!(hex.value.as_str(), "#ff0000");
    let hsl = s.get_formats().row_data(2).expect("hsl row");
    assert_eq!(hsl.value.as_str(), "hsl(0, 100%, 50%)");
    let white = s.get_contrast().row_data(0).expect("white row");
    assert_eq!(
        white.ratio.as_str(),
        ratio_text(contrast_ratio(red, Rgba8::rgb(255, 255, 255)))
    );
    assert!(!s.get_current_translucent());
    assert!(
        has_pixel(&r.frame(), [255, 0, 0]),
        "the swatch paints the colour"
    );
}

#[test]
fn a_half_typed_colour_changes_nothing_and_enter_reports_it() {
    let r = rig(false, (320, 640));
    let before = r.ctl.borrow().colour_view().current;
    r.state().invoke_input_edited("#ff".into());
    assert_eq!(r.ctl.borrow().colour_view().current, before);
    assert!(
        r.state().get_input_error().is_empty(),
        "no scolding while typing"
    );
    r.state().invoke_input_accepted("#ff".into());
    assert!(!r.state().get_input_error().is_empty());
    assert_eq!(r.ctl.borrow().colour_view().current, before);
}

#[test]
fn dragging_the_surface_picks_saturation_and_brightness() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#ff0000".into());
    // Top-right corner of the square: full saturation, full brightness.
    r.state().invoke_surface_moved(1.0, 0.0, true);
    slint::platform::update_timers_and_animations();
    assert_eq!(r.ctl.borrow().colour_view().current, Rgba8::rgb(255, 0, 0));
    // Bottom edge: black, whatever the hue.
    r.state().invoke_surface_moved(0.5, 1.0, true);
    assert_eq!(r.ctl.borrow().colour_view().current, Rgba8::rgb(0, 0, 0));
}

#[test]
fn typing_a_channel_clamps_and_a_bad_one_is_reported() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#102030".into());
    r.state().invoke_model_picked(1); // RGB
    r.state().invoke_channel_committed(0, "300".into());
    assert_eq!(
        r.ctl.borrow().colour_view().current,
        Rgba8::rgb(255, 0x20, 0x30),
        "255 is the most a channel takes"
    );
    assert!(r.state().get_values_error().is_empty());
    r.state().invoke_channel_committed(1, "abc".into());
    assert!(!r.state().get_values_error().is_empty());
    assert_eq!(
        r.ctl.borrow().colour_view().current,
        Rgba8::rgb(255, 0x20, 0x30),
        "a bad number changes nothing"
    );
}

#[test]
fn save_puts_the_colour_in_the_colours_list_with_its_details() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_save(-1);
    let ctl = r.ctl.borrow();
    let saved = ctl.colour_view().saved;
    assert_eq!(saved, vec![Rgba8::rgb(0x3a, 0x7b, 0xd5)]);
}

#[test]
fn the_dark_tab_renders_too() {
    let r = rig(true, (320, 640));
    r.state().invoke_input_edited("#ff0000".into());
    let frame = r.h.render_settled();
    shot("dark-320", &frame);
    assert!(has_pixel(&frame, [255, 0, 0]));
    let r = rig(false, (420, 700));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_style_picked(1);
    shot("wheel-420", &r.frame());
}

fn channel_texts(r: &Rig) -> Vec<String> {
    r.state()
        .get_channels()
        .iter()
        .map(|c| c.text.to_string())
        .collect()
}

/// Every format row (but the localised Name) says what the crate says for the current colour.
fn assert_rows_follow_the_colour(r: &Rig) {
    let cur = r.ctl.borrow().colour_view().current;
    let rows = r.state().get_formats();
    for (i, f) in Format::ALL
        .iter()
        .enumerate()
        .filter(|(_, f)| **f != Format::Name)
    {
        assert_eq!(
            rows.row_data(i).unwrap().value.as_str(),
            format(cur, *f),
            "the {f:?} row follows {cur:?}"
        );
    }
}

#[test]
fn brightness_to_zero_and_back_keeps_the_colour_and_the_values_row_says_so() {
    let r = rig(false, (320, 640));
    let blue = Rgba8::rgb(58, 123, 213);
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_model_picked(3); // HSB
    let start = channel_texts(&r);
    assert_eq!(start.len(), 3, "H, S and B: {start:?}");
    let bri = r.state().get_bri();
    r.state().invoke_bri_moved(0.0);
    assert_eq!(
        r.ctl.borrow().colour_view().current,
        Rgba8::rgb(0, 0, 0),
        "black"
    );
    let black = channel_texts(&r);
    assert_eq!(
        black[0], start[0],
        "the hue is still there on black: {black:?}"
    );
    assert_eq!(black[1], start[1], "and so is the saturation: {black:?}");
    assert_eq!(black[2], "0", "brightness reads 0: {black:?}");
    r.state().invoke_bri_moved(bri);
    assert_eq!(r.ctl.borrow().colour_view().current, blue);
    assert_eq!(channel_texts(&r), start, "the Values row is what it was");
    assert_rows_follow_the_colour(&r);
}

#[test]
fn a_hue_slider_value_outside_the_wheel_is_held_at_its_ends_and_nan_is_zero() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_model_picked(3); // HSB
    let hue = |r: &Rig| r.state().get_hue();
    r.state().invoke_hue_moved(-5.0);
    assert!(hue(&r) >= 0.0, "below the wheel is held at 0: {}", hue(&r));
    r.state().invoke_hue_moved(500.0);
    assert!(
        hue(&r) <= 359.999,
        "above the wheel is held at its top: {}",
        hue(&r)
    );
    r.state().invoke_hue_moved(f32::NAN);
    assert!(hue(&r) == 0.0, "NaN reads as 0: {}", hue(&r));
}

#[test]
fn a_list_replaced_by_one_of_the_same_name_is_where_save_to_puts_the_colour() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    // The first save makes "Colours"; Undo takes it away; the next save makes it again: the same
    // name on another id, so nothing the window draws changes. (Saves go through the window, which
    // is what refreshes its Save to… entries.)
    r.state().invoke_save(-1);
    r.settle();
    r.ctl.borrow_mut().main_undo();
    r.state().invoke_input_edited("#ff0000".into());
    r.state().invoke_save(-1);
    r.settle();
    let targets = r.state().get_save_targets();
    let at = (0..targets.row_count())
        .find(|i| targets.row_data(*i).unwrap().text.ends_with("Colours"))
        .expect("a Colours entry");
    r.state().invoke_input_edited("#00ff00".into());
    r.state().invoke_save(i32::try_from(at).unwrap());
    r.settle();
    assert_eq!(
        r.ctl.borrow().colour_view().saved.len(),
        2,
        "the second colour joined the new list, not the one that is gone"
    );
}

#[test]
fn a_translucent_second_colour_is_judged_over_white_and_the_row_says_so() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    let blue = Rgba8::rgb(0x3a, 0x7b, 0xd5);
    let white = ratio_text(contrast_ratio(blue, Rgba8::rgb(255, 255, 255)));
    for typed in ["#ffffff10", "transparent"] {
        r.state().invoke_other_accepted(typed.into());
        r.settle();
        let row = r.state().get_other_row();
        assert_eq!(
            row.ratio.as_str(),
            white,
            "{typed}: over white, not over black"
        );
        assert!(row.name.contains("on white"), "{typed}: {:?}", row.name);
    }
    r.state().invoke_other_accepted("#000000".into());
    r.settle();
    assert!(
        !r.state().get_other_row().name.contains("on white"),
        "an opaque colour is not flattened"
    );
}

fn number_at(r: &Rig, i: usize) -> f64 {
    r.state()
        .get_channels()
        .row_data(i)
        .unwrap()
        .text
        .parse()
        .unwrap()
}

#[test]
fn committing_an_hsl_channel_updates_the_other_rows() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_model_picked(2); // HSL
    assert_eq!(channel_texts(&r).len(), 3);
    let before = r.ctl.borrow().colour_view().current;
    r.state().invoke_channel_committed(2, "20".into());
    let after = r.ctl.borrow().colour_view().current;
    assert_ne!(after, before, "L=20 darkens the colour");
    assert!(after.b < before.b);
    assert_eq!(channel_texts(&r)[2], "20", "the typed lightness stays");
    assert_rows_follow_the_colour(&r);
    let hex = r
        .state()
        .get_formats()
        .row_data(0)
        .unwrap()
        .value
        .to_string();
    assert_eq!(hex, format(after, Format::Hex));
    // The saturation moves the other rows too.
    r.state().invoke_channel_committed(1, "10".into());
    assert_ne!(r.ctl.borrow().colour_view().current, after);
    assert_eq!(channel_texts(&r)[1], "10");
    assert_rows_follow_the_colour(&r);
}

#[test]
fn stepping_hsl_hue_on_a_grey_blue_and_hsl_saturation_on_a_dark_red_both_advance() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("rgb(58, 60, 62)".into());
    r.state().invoke_model_picked(2); // HSL
    let h = number_at(&r, 0);
    r.state().invoke_channel_stepped(0, 1);
    assert!(
        (number_at(&r, 0) - (h + 1.0)).abs() < 0.5,
        "H {h} -> {}",
        number_at(&r, 0)
    );
    r.state().invoke_channel_stepped(0, 10);
    assert!((number_at(&r, 0) - (h + 11.0)).abs() < 0.5, "and again");
    assert_rows_follow_the_colour(&r);

    r.state().invoke_input_edited("rgb(30, 20, 20)".into());
    let s = number_at(&r, 1);
    r.state().invoke_channel_stepped(1, 1);
    assert!(
        (number_at(&r, 1) - (s + 1.0)).abs() < 0.5,
        "S {s} -> {}",
        number_at(&r, 1)
    );
    assert_rows_follow_the_colour(&r);
}

#[test]
fn committing_and_stepping_a_cmyk_channel_updates_the_other_rows() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("rgb(13, 20, 25)".into());
    r.state().invoke_model_picked(4); // CMYK
    assert_eq!(channel_texts(&r).len(), 4);
    let c = number_at(&r, 0);
    r.state().invoke_channel_stepped(0, 1);
    assert!(
        (number_at(&r, 0) - (c + 1.0)).abs() < 0.5,
        "C {c} -> {}",
        number_at(&r, 0)
    );
    assert_rows_follow_the_colour(&r);
    let before = r.ctl.borrow().colour_view().current;
    r.state().invoke_channel_committed(3, "0".into());
    assert_ne!(
        r.ctl.borrow().colour_view().current,
        before,
        "K=0 lightens it"
    );
    assert_eq!(channel_texts(&r)[3], "0");
    assert_rows_follow_the_colour(&r);
}

#[test]
fn a_typed_hue_on_a_grey_sticks_and_stepping_it_advances() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#808080".into());
    r.state().invoke_model_picked(3); // HSB
    r.state().invoke_channel_committed(0, "120".into());
    assert_eq!(
        r.state().get_channels().row_data(0).unwrap().text.as_str(),
        "120"
    );
    r.state().invoke_channel_stepped(0, 1);
    assert_eq!(
        r.state().get_channels().row_data(0).unwrap().text.as_str(),
        "121"
    );
    assert_eq!(
        r.ctl.borrow().colour_view().current,
        Rgba8::rgb(128, 128, 128)
    );
}

// ---- Real key events, focus and geometry --------------------------------------------------------

fn key(k: Key) -> String {
    char::from(k).to_string()
}

impl Rig {
    fn settle(&self) {
        let _ = self.frame();
    }

    /// A settled render (the window is made dirty first, so it always draws).
    fn frame(&self) -> Frame {
        self.h.component.window().request_redraw();
        self.h.render_settled()
    }

    fn press(&self, text: &str) {
        let w = self.h.component.window();
        w.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
        w.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
        self.settle();
    }

    fn chord(&self, modifiers: &[Key], text: &str) {
        let w = self.h.component.window();
        for m in modifiers {
            w.dispatch_event(WindowEvent::KeyPressed {
                text: key(*m).into(),
            });
        }
        w.dispatch_event(WindowEvent::KeyPressed { text: text.into() });
        w.dispatch_event(WindowEvent::KeyReleased { text: text.into() });
        for m in modifiers.iter().rev() {
            w.dispatch_event(WindowEvent::KeyReleased {
                text: key(*m).into(),
            });
        }
        self.settle();
    }

    /// The spoken name of whatever holds the keyboard (the nearest accessible item up the tree).
    fn focus_label(&self) -> Option<String> {
        let mut cur = WindowInner::from_pub(self.h.component.window())
            .focus_item
            .borrow()
            .upgrade();
        while let Some(i) = cur {
            if i.is_accessible() {
                return Some(
                    i.accessible_string_property(AccessibleStringProperty::Label)
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            cur = i.parent_item(ParentItemTraversalMode::FindAllParents);
        }
        None
    }

    /// Tabs forward until `label` holds the keyboard.
    fn tab_to(&self, label: &str) {
        for _ in 0..60 {
            if self.focus_label().as_deref() == Some(label) {
                return;
            }
            self.press(&key(Key::Tab));
        }
        panic!(
            "Tab never reached {label:?}, last at {:?}",
            self.focus_label()
        );
    }

    fn rows(&self, role: AccessibleRole) -> Vec<ElementHandle> {
        ElementQuery::from_root(&self.h.component)
            .match_predicate(move |e| e.accessible_role() == Some(role))
            .find_all()
    }

    fn selected_label(&self, list: &str) -> Option<String> {
        let list = if list == "Formats" {
            self.state().get_use_hint().to_string()
        } else {
            list.to_string()
        };
        self.rows(AccessibleRole::ListItem)
            .into_iter()
            .find(|e| {
                e.accessible_description().as_deref() == Some(list.as_str())
                    && e.accessible_item_selected() == Some(true)
            })
            .and_then(|e| e.accessible_label().map(|l| l.to_string()))
    }
}

#[test]
fn tab_walks_the_tab_once_in_reading_order_with_one_stop_per_row_group() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_format_used(0); // a recent, so the grid shows
    r.settle();
    let mut seen = Vec::new();
    for _ in 0..40 {
        r.press(&key(Key::Tab));
        seen.push(r.focus_label().unwrap_or_default());
    }
    let first_repeat = seen
        .iter()
        .skip(1)
        .position(|l| *l == seen[0])
        .map_or(seen.len(), |p| p + 1);
    let cycle = &seen[..first_repeat];
    let count = |name: &str| cycle.iter().filter(|l| l.as_str() == name).count();
    assert_eq!(
        count("Formats"),
        1,
        "the seven rows are one stop: {cycle:?}"
    );
    assert_eq!(count("Recently used"), 1, "{cycle:?}");
    assert_eq!(count("Picker"), 1, "the disclosure is a stop: {cycle:?}");
    let at = |name: &str| cycle.iter().position(|l| l.as_str() == name).unwrap();
    assert!(at("Colour") < at("Picker"));
    assert!(at("Picker") < at("Saturation and brightness"));
    assert!(at("Saturation and brightness") < at("Hue"));
    assert!(at("Hue") < at("Recently used"));
    assert!(at("Recently used") < at("Formats"));
}

#[test]
fn the_picker_disclosure_takes_enter_and_space() {
    let r = rig(false, (320, 640));
    r.tab_to("Picker");
    assert!(r.state().get_picker_open());
    r.press(&key(Key::Return));
    assert!(!r.state().get_picker_open(), "Enter closes it");
    r.press(" ");
    assert!(r.state().get_picker_open(), "Space opens it again");
}

#[test]
fn format_rows_are_one_stop_with_a_roving_selection_and_keys() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.tab_to("Formats");
    let rows = r.state().get_formats();
    let label = |i: usize| {
        let f = rows.row_data(i).unwrap();
        format!("{}, {}", f.label, f.value)
    };
    assert_eq!(
        r.selected_label("Formats"),
        Some(label(0)),
        "row 0 on focus"
    );
    r.press(&key(Key::DownArrow));
    r.press(&key(Key::DownArrow));
    assert_eq!(r.selected_label("Formats"), Some(label(2)));
    r.press(&key(Key::End));
    assert_eq!(
        r.selected_label("Formats"),
        Some(label(rows.row_count() - 1))
    );
    r.press(&key(Key::Home));
    assert_eq!(r.selected_label("Formats"), Some(label(0)));
    r.press(&key(Key::UpArrow));
    assert_eq!(
        r.selected_label("Formats"),
        Some(label(0)),
        "stops at the top"
    );
    assert_eq!(
        r.ctl.borrow().colour_view().recents.len(),
        0,
        "no recents left"
    );
    r.press(&key(Key::Return));
    assert_eq!(
        r.ctl.borrow().colour_view().recents,
        vec![Rgba8::rgb(0x3a, 0x7b, 0xd5)],
        "Enter uses the row"
    );
}

#[test]
fn shift_f10_opens_the_row_menu_and_esc_closes_it_without_closing_the_panel() {
    let r = rig(false, (320, 640));
    r.tab_to("Formats");
    r.press(&key(Key::DownArrow));
    r.chord(&[Key::Shift], &key(Key::F10));
    assert_eq!(r.state().get_popup(), 2, "the row menu");
    assert!(
        r.rows(AccessibleRole::List)
            .iter()
            .any(|e| e.accessible_label().as_deref() == Some("Format")),
        "the menu is drawn"
    );
    r.press(&key(Key::Escape));
    assert_eq!(r.state().get_popup(), 0);
    assert!(r.h.component.get_colours_open(), "Esc closed only the menu");
    assert_eq!(
        r.focus_label().as_deref(),
        Some("Formats"),
        "the keyboard goes back"
    );
    r.press(&key(Key::Menu));
    assert_eq!(r.state().get_popup(), 2, "the Menu key too");
}

#[test]
fn the_swatch_menu_has_copy_as_all_formats_save_to_and_remove_and_delete_removes_a_recent() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_format_used(0);
    r.state().invoke_input_edited("#ff0000".into());
    r.state().invoke_format_used(0);
    r.settle();
    assert_eq!(r.ctl.borrow().colour_view().recents.len(), 2);
    assert_eq!(
        r.state().get_copy_as().row_count(),
        Format::ALL.len(),
        "Copy as lists every format, Name included"
    );
    r.tab_to("Recently used");
    assert_eq!(
        r.selected_label("Recently used")
            .map(|l| l.starts_with("#ff0000")),
        Some(true),
        "chip 0 is selected on focus"
    );
    r.chord(&[Key::Shift], &key(Key::F10));
    assert_eq!(r.state().get_popup(), 1);
    // Make current, Copy as…, Save to Colours, Save to…, Remove from recents.
    r.press(&key(Key::DownArrow));
    r.press(&key(Key::DownArrow));
    r.press(&key(Key::Return));
    assert_eq!(r.state().get_popup(), 4, "Copy as… opens the formats");
    r.press(&key(Key::Escape));
    assert_eq!(r.state().get_popup(), 0);
    assert_eq!(r.focus_label().as_deref(), Some("Recently used"));
    r.press(&key(Key::Delete));
    assert_eq!(
        r.ctl.borrow().colour_view().recents,
        vec![Rgba8::rgb(0x3a, 0x7b, 0xd5)],
        "Delete removed the selected recent"
    );
}

#[test]
fn ctrl_s_saves_and_ctrl_shift_s_asks_where_and_ctrl_c_copies() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.tab_to("Hue");
    r.chord(&[Key::Control], "s");
    assert_eq!(
        r.ctl.borrow().colour_view().saved,
        vec![Rgba8::rgb(0x3a, 0x7b, 0xd5)],
        "Ctrl+S saves to Colours"
    );
    r.chord(&[Key::Control, Key::Shift], "S");
    assert_eq!(r.state().get_popup(), 3, "Ctrl+Shift+S opens Save to…");
    assert!(r.state().get_save_targets().row_count() >= 2);
    r.press(&key(Key::Escape));
    assert_eq!(r.state().get_popup(), 0);
    assert_eq!(
        r.focus_label().as_deref(),
        Some("Hue"),
        "the keyboard goes back to where Ctrl+Shift+S found it"
    );
    r.state().invoke_input_edited("#ff0000".into());
    r.tab_to("Picker");
    r.chord(&[Key::Control], "c");
    assert_eq!(
        r.ctl.borrow().colour_view().recents.first(),
        Some(&Rgba8::rgb(255, 0, 0)),
        "Ctrl+C copies the colour"
    );
    r.chord(&[Key::Control], "i"); // no eyedropper in the test rig: nothing happens, nothing breaks
    assert!(r.h.component.get_colours_open());
}

#[test]
fn save_to_lists_every_list_and_ends_with_new_list_which_makes_one() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    let targets = r.state().get_save_targets();
    let n = targets.row_count();
    let last = targets.row_data(n - 1).unwrap();
    assert_eq!(last.text.as_str(), "New list…");
    let lists_before = r.ctl.borrow().colour_save_targets().len();
    assert_eq!(n, lists_before + 1);
    r.state().invoke_save(i32::try_from(n - 1).unwrap());
    r.settle();
    assert_eq!(r.ctl.borrow().colour_save_targets().len(), lists_before + 1);
    assert_eq!(r.state().get_save_targets().row_count(), n + 1);
}

#[test]
fn page_up_and_down_step_brightness_in_the_wheel_and_the_square() {
    let r = rig(false, (420, 700));
    r.state().invoke_input_edited("#3a7bd5".into());
    for style in [0, 1] {
        r.state().invoke_style_picked(style);
        r.settle();
        let name = if style == 1 {
            "Hue and saturation"
        } else {
            "Saturation and brightness"
        };
        r.tab_to(name);
        let before = r.state().get_bri();
        r.press(&key(Key::PageDown));
        let after = r.state().get_bri();
        assert!(
            (before - after - 10.0).abs() < 1.5,
            "style {style}: {before} -> {after}"
        );
        r.press(&key(Key::PageUp));
        assert!((r.state().get_bri() - before).abs() < 1.5);
    }
}

#[test]
fn the_contrast_line_speaks_all_five_marks() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    let speak = r
        .state()
        .get_contrast()
        .row_data(0)
        .unwrap()
        .speak
        .to_string();
    for part in [
        "AA text",
        "AA large text",
        "AAA text",
        "AAA large text",
        "Interface parts",
    ] {
        assert!(speak.contains(part), "{speak:?} lacks {part:?}");
    }
}

#[test]
fn errors_are_a_live_region_with_the_text_and_no_glyph() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_accepted("#ff".into());
    r.settle();
    let error = r.state().get_input_error().to_string();
    assert!(!error.is_empty() && !error.contains('\u{26a0}'));
    let live = ElementQuery::from_root(&r.h.component)
        .match_predicate(|e| {
            e.accessible_live_region()
                .is_some_and(|l| l != i_slint_backend_testing::AccessibleLiveness::Off)
        })
        .find_all();
    assert!(
        live.iter()
            .any(|e| e.accessible_label().as_deref() == Some(error.as_str())),
        "the error is announced"
    );
}

#[test]
fn hit_areas_are_at_least_24px() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_format_used(0);
    r.settle();
    for e in r.rows(AccessibleRole::Slider) {
        let h = e.size().height;
        assert!(h >= 24.0, "slider {:?} is {h}px tall", e.accessible_label());
    }
    for e in r.rows(AccessibleRole::ListItem) {
        let size = e.size();
        let hint = r.state().get_use_hint();
        if e.accessible_description()
            .is_some_and(|d| d == hint.as_str() || d == "Recently used" || d == "Saved colours")
        {
            assert!(
                size.height >= 24.0 && size.width >= 24.0,
                "{:?} is {}x{}",
                e.accessible_label(),
                size.width,
                size.height
            );
        }
    }
}

#[test]
fn the_values_fields_stay_inside_the_tab_at_260_and_320_for_cmyk_with_alpha() {
    for width in [260_u32, 320, 480] {
        let r = rig(false, (width, 700));
        r.state().invoke_input_edited("#3a7bd5aa".into());
        r.state().invoke_model_picked(4); // CMYK
        r.settle();
        let fields = ElementQuery::from_root(&r.h.component)
            .match_type_name("ChannelField")
            .find_all();
        assert!(fields.len() >= 5, "C, M, Y, K and alpha: {}", fields.len());
        for f in &fields {
            let (p, s) = (f.absolute_position(), f.size());
            assert!(
                p.x + s.width <= width as f32 - 11.0 && p.x >= 0.0,
                "a field at x {} width {} overflows {width}",
                p.x,
                s.width
            );
        }
        shot(&format!("cmyk-alpha-{width}"), &r.frame());
    }
}

#[test]
fn the_contrast_marks_wrap_at_260_and_the_line_grows_to_hold_them() {
    let height_at = |width: u32| {
        let r = rig(false, (width, 1600));
        r.state().invoke_input_edited("#3a7bd5".into());
        r.settle();
        let line = r
            .rows(AccessibleRole::ListItem)
            .into_iter()
            .find(|e| {
                e.accessible_label()
                    .is_some_and(|l| l.starts_with("On white"))
            })
            .unwrap_or_else(|| {
                panic!(
                    "the white line in {:?}",
                    r.rows(AccessibleRole::ListItem)
                        .iter()
                        .map(ElementHandle::accessible_label)
                        .collect::<Vec<_>>()
                )
            });
        shot(&format!("contrast-{width}"), &r.frame());
        line.size().height
    };
    let (wide, narrow) = (height_at(480), height_at(260));
    assert!(
        narrow >= wide + 16.0,
        "260px wraps the marks onto more lines: {narrow} vs {wide}"
    );
}

#[test]
fn a_printable_key_with_no_field_focused_starts_the_input_line() {
    let r = rig(false, (320, 640));
    r.tab_to("Picker");
    r.press("g");
    r.press("r");
    assert_eq!(r.state().get_input_text().as_str(), "gr");
}

#[test]
fn shots_at_both_widths_with_recents_and_saved() {
    for (w, dark) in [(260_u32, false), (320, false), (320, true)] {
        let r = rig(dark, (w, 760));
        for c in ["#3a7bd5", "#ff0000", "#10b981"] {
            r.state().invoke_input_edited(c.into());
            r.state().invoke_format_used(0);
            r.state().invoke_save(-1);
        }
        r.settle();
        r.tab_to("Formats");
        shot(
            &format!("full-{w}{}", if dark { "-dark" } else { "" }),
            &r.frame(),
        );
    }
}

// ---- Real keys in the Values fields, the channel letters and the geometry ------------------------

impl Rig {
    fn set_text_scale(&self, scale: f32) {
        // The binding re-applies the app's scale on every redraw, so set it where it comes from.
        snipdeck_app::text_scale::configure(snipdeck_app::settings::TextSize::System);
        snipdeck_app::text_scale::set_system(scale);
        snipdeck_app::text_scale::apply(&SystemLook::get(&self.h.component));
        self.settle();
        assert!(
            (SystemLook::get(&self.h.component).get_text_scale() - scale).abs() < 1e-3,
            "the window took the text scale"
        );
    }

    fn by_label(&self, role: AccessibleRole, label: &str) -> ElementHandle {
        self.rows(role)
            .into_iter()
            .find(|e| e.accessible_label().as_deref() == Some(label))
            .unwrap_or_else(|| panic!("no {role:?} named {label:?}"))
    }

    /// The footer line under the tab.
    fn footer(&self) -> String {
        ElementQuery::from_root(&self.h.component)
            .match_id("ColoursView::route")
            .find_first()
            .expect("the footer line")
            .accessible_label()
            .unwrap_or_default()
            .to_string()
    }

    fn pointer_to(&self, x: f32, y: f32) {
        self.h
            .component
            .window()
            .dispatch_event(WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x, y),
            });
        self.settle();
    }

    /// Calls `f` each time the Panel asks to close.
    fn count_closes(&self) -> Rc<Cell<u32>> {
        let n = Rc::new(Cell::new(0));
        let c = Rc::clone(&n);
        self.h
            .component
            .on_close_requested(move || c.set(c.get() + 1));
        n
    }
}

fn rects_overlap(a: &ElementHandle, b: &ElementHandle) -> bool {
    let (pa, sa, pb, sb) = (
        a.absolute_position(),
        a.size(),
        b.absolute_position(),
        b.size(),
    );
    pa.x < pb.x + sb.width - 0.5
        && pb.x < pa.x + sa.width - 0.5
        && pa.y < pb.y + sb.height - 0.5
        && pb.y < pa.y + sa.height - 0.5
}

#[test]
fn up_and_down_keys_in_a_values_field_step_it_and_shift_steps_ten() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_model_picked(3); // HSB
    r.settle();
    r.tab_to("H");
    let before = number_at(&r, 0);
    r.press(&key(Key::UpArrow));
    assert!(
        (number_at(&r, 0) - (before + 1.0)).abs() < 0.5,
        "Up steps the hue: {before} -> {}",
        number_at(&r, 0)
    );
    let field = r.by_label(AccessibleRole::TextInput, "H");
    assert_eq!(
        field.accessible_value().as_deref(),
        Some(r.state().get_channels().row_data(0).unwrap().text.as_str()),
        "the field shows the stepped number"
    );
    r.press(&key(Key::DownArrow));
    assert!((number_at(&r, 0) - before).abs() < 0.5);
    r.chord(&[Key::Shift], &key(Key::UpArrow));
    assert!(
        (number_at(&r, 0) - (before + 10.0)).abs() < 0.5,
        "Shift+Up steps ten"
    );
    r.chord(&[Key::Shift], &key(Key::DownArrow));
    assert!((number_at(&r, 0) - before).abs() < 0.5);
    assert_eq!(
        r.focus_label().as_deref(),
        Some("H"),
        "the keyboard stays in the field"
    );
}

#[test]
fn every_values_field_draws_its_channel_letter_and_nothing_clips_or_overlaps() {
    for (width, scale) in [(260_u32, 1.0_f32), (260, 1.5), (320, 1.5), (480, 1.0)] {
        for (model, count) in [(4, 5), (3, 4), (1, 4)] {
            let r = rig(false, (width, 1100));
            r.set_text_scale(scale);
            r.state().invoke_input_edited("#3a7bd5aa".into());
            r.state().invoke_model_picked(model);
            r.settle();
            let fields = ElementQuery::from_root(&r.h.component)
                .match_type_name("ChannelField")
                .find_all();
            let captions = ElementQuery::from_root(&r.h.component)
                .match_id("ChannelField::cap")
                .find_all();
            let at = format!("model {model} at {width}px, text x{scale}");
            assert_eq!(fields.len(), count, "{at}");
            assert_eq!(
                captions.len(),
                fields.len(),
                "a letter above each field, {at}"
            );
            let frame = r.frame();
            for f in &fields {
                let (fp, fs) = (f.absolute_position(), f.size());
                let tf = f
                    .query_descendants()
                    .match_type_name("TextField")
                    .find_first()
                    .expect("the box");
                let (tp, ts) = (tf.absolute_position(), tf.size());
                assert!(
                    tp.y + ts.height <= fp.y + fs.height + 0.5
                        && tp.x + ts.width <= fp.x + fs.width + 0.5,
                    "the box stays inside its cell, {at}"
                );
                let cap = f
                    .query_descendants()
                    .match_id("ChannelField::cap")
                    .find_first()
                    .expect("its letter");
                let (cp, cs) = (cap.absolute_position(), cap.size());
                assert!(
                    cs.width > 4.0 && cs.height > 8.0,
                    "the letter has room, {at}"
                );
                assert!(
                    cp.y >= fp.y - 0.5 && cp.y + cs.height <= tp.y + 0.5,
                    "the letter sits above the box, {at}"
                );
                // Something is drawn in the caption's rectangle.
                let (x0, y0) = (cp.x.max(0.0) as u32, cp.y.max(0.0) as u32);
                let (x1, y1) = (
                    ((cp.x + cs.width) as u32).min(frame.width),
                    ((cp.y + cs.height) as u32).min(frame.height),
                );
                let back = frame.pixel(x0, y0);
                let ink = (y0..y1)
                    .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                    .filter(|&(x, y)| frame.pixel(x, y) != back)
                    .count();
                assert!(ink >= 3, "the letter is drawn ({ink} pixels), {at}");
                assert!(
                    fp.x >= 0.0 && fp.x + fs.width <= width as f32 - 5.0,
                    "inside the tab, {at}"
                );
            }
            for (i, a) in fields.iter().enumerate() {
                for b in &fields[i + 1..] {
                    assert!(!rects_overlap(a, b), "fields overlap, {at}");
                }
            }
            if scale > 1.4 && width == 260 {
                shot(&format!("channels-{model}-{width}-x{scale}"), &frame);
            }
        }
    }
}

#[test]
fn the_picker_disclosure_and_the_style_switch_do_not_overlap_at_any_width_or_text_size() {
    for (width, scale) in [(260_u32, 1.0_f32), (260, 1.5), (320, 1.75), (480, 1.0)] {
        let r = rig(false, (width, 1100));
        r.set_text_scale(scale);
        let at = format!("{width}px, text x{scale}");
        shot(&format!("picker-row-{width}-x{scale}"), &r.frame());
        let disclosure = r.by_label(AccessibleRole::Button, "Picker");
        let switch = r.by_label(AccessibleRole::RadioGroup, "Picker style");
        assert!(
            !rects_overlap(&disclosure, &switch),
            "disclosure and switch overlap at {at}: {:?} {:?} / {:?} {:?}",
            disclosure.absolute_position(),
            disclosure.size(),
            switch.absolute_position(),
            switch.size()
        );
        let sw = switch.absolute_position().x + switch.size().width;
        assert!(
            switch.absolute_position().x >= 0.0 && sw <= width as f32,
            "the switch is inside the window at {at}"
        );
        let values_as = r.by_label(AccessibleRole::Combobox, "Values as");
        assert!(
            values_as.absolute_position().x + values_as.size().width <= width as f32,
            "Values as fits at {at}"
        );
        assert!(!rects_overlap(&values_as, &switch), "{at}");
        shot(&format!("picker-row-{width}-x{scale}"), &r.frame());
    }
}

#[test]
fn the_contrast_marks_grow_with_the_text_size() {
    let height_at = |scale: f32| {
        let r = rig(false, (480, 1600));
        r.state().invoke_input_edited("#3a7bd5".into());
        r.set_text_scale(scale);
        let line = r
            .rows(AccessibleRole::ListItem)
            .into_iter()
            .find(|e| {
                e.accessible_label()
                    .is_some_and(|l| l.starts_with("On white"))
            })
            .expect("the white line");
        line.size().height
    };
    let (normal, large) = (height_at(1.0), height_at(1.75));
    assert!(
        large >= normal + 10.0,
        "a mark is taller at 175%: {large} vs {normal}"
    );
}

// ---- Focus after the last recent goes, dropdowns, undo, footer, accessible range ----------------

#[test]
fn deleting_the_last_recent_keeps_the_keyboard_alive_and_esc_still_closes_the_panel() {
    let r = rig(false, (320, 640));
    let closes = r.count_closes();
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_format_used(0);
    r.settle();
    assert_eq!(r.state().get_recents().row_count(), 1);
    let removed = r
        .state()
        .get_recents()
        .row_data(0)
        .unwrap()
        .label
        .to_string();
    r.tab_to("Recently used");
    assert!(
        r.footer().starts_with(&removed),
        "the footer names the chip: {}",
        r.footer()
    );
    r.press(&key(Key::Delete));
    assert_eq!(r.state().get_recents().row_count(), 0);
    let now = r.focus_label();
    assert!(
        matches!(now.as_deref(), Some("Formats" | "Saved colours")),
        "the keyboard moved to the next stop, not nowhere: {now:?}"
    );
    assert_ne!(r.footer(), removed, "the footer forgot the removed colour");
    r.press(&key(Key::Escape));
    assert_eq!(closes.get(), 1, "Esc still reaches the Panel");
}

#[test]
fn removing_the_last_recent_from_its_menu_keeps_the_keyboard_alive_too() {
    let r = rig(false, (320, 640));
    let closes = r.count_closes();
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_format_used(0);
    r.settle();
    r.tab_to("Recently used");
    r.chord(&[Key::Shift], &key(Key::F10));
    assert_eq!(r.state().get_popup(), 1);
    for _ in 0..5 {
        r.press(&key(Key::DownArrow));
    }
    r.press(&key(Key::Return)); // Remove from recents
    assert_eq!(r.state().get_popup(), 0);
    assert_eq!(r.state().get_recents().row_count(), 0);
    let now = r.focus_label();
    assert!(
        matches!(now.as_deref(), Some("Formats" | "Saved colours")),
        "the keyboard is on a live stop: {now:?}"
    );
    r.press(&key(Key::Escape));
    assert_eq!(closes.get(), 1);
}

#[test]
fn save_to_from_its_button_gives_the_keyboard_back_to_the_button() {
    let r = rig(false, (320, 640));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.tab_to("Save to…");
    r.press(&key(Key::Return));
    assert_eq!(r.state().get_popup(), 3, "the button opens Save to…");
    r.press(&key(Key::Escape));
    assert_eq!(r.state().get_popup(), 0);
    assert_eq!(r.focus_label().as_deref(), Some("Save to…"));
    // Choosing a list gives it back too.
    r.press(&key(Key::Return));
    r.press(&key(Key::DownArrow));
    r.press(&key(Key::Return));
    assert_eq!(r.state().get_popup(), 0);
    assert_eq!(r.focus_label().as_deref(), Some("Save to…"));
    // Ctrl+Shift+S from the Formats list returns to the Formats list.
    r.tab_to("Formats");
    r.chord(&[Key::Control, Key::Shift], "S");
    assert_eq!(r.state().get_popup(), 3);
    r.press(&key(Key::Escape));
    assert_eq!(r.focus_label().as_deref(), Some("Formats"));
}

#[test]
fn esc_with_a_drop_down_open_closes_only_the_list() {
    for label in ["Values as", "Scheme"] {
        let r = rig(false, (320, 640));
        let closes = r.count_closes();
        r.tab_to(label);
        r.press(&key(Key::DownArrow));
        assert!(r.state().get_dropdown_open(), "{label} is open");
        r.press(&key(Key::Escape));
        assert!(!r.state().get_dropdown_open(), "{label} closed");
        assert_eq!(
            closes.get(),
            0,
            "the Panel stays for the Esc that closed {label}"
        );
        assert!(r.h.component.get_colours_open());
        r.press(&key(Key::Escape));
        assert_eq!(closes.get(), 1, "the next Esc closes the Panel ({label})");
    }
}

#[test]
fn ctrl_z_and_ctrl_y_in_a_field_never_run_the_librarys_undo_or_redo() {
    let r = rig(false, (320, 640));
    let (undos, redos) = (Rc::new(Cell::new(0_u32)), Rc::new(Cell::new(0_u32)));
    {
        let u = Rc::clone(&undos);
        r.h.component.on_undo_requested(move || u.set(u.get() + 1));
        let d = Rc::clone(&redos);
        r.h.component.on_redo_requested(move || d.set(d.get() + 1));
    }
    r.state().invoke_input_edited("#3a7bd5".into());
    r.state().invoke_model_picked(1); // RGB
    r.settle();
    for field in ["Colour", "R"] {
        r.tab_to(field);
        assert!(r.state().get_text_focused(), "{field} is a text field");
        r.chord(&[Key::Control], "z");
        r.chord(&[Key::Control, Key::Shift], "Z");
        r.chord(&[Key::Control], "y");
        assert_eq!(
            (undos.get(), redos.get()),
            (0, 0),
            "typing in {field} is not the library's history"
        );
    }
    // Off the fields, the shortcuts are the library's.
    r.tab_to("Picker");
    assert!(!r.state().get_text_focused());
    r.chord(&[Key::Control], "z");
    assert_eq!(undos.get(), 1);
    r.chord(&[Key::Control], "y");
    assert_eq!(redos.get(), 1);
}

#[test]
fn hovering_a_format_row_shows_its_text_in_the_footer() {
    let r = rig(false, (320, 1500));
    r.state().invoke_input_edited("#3a7bd5".into());
    r.settle();
    let hint = r.state().get_use_hint().to_string();
    assert_eq!(r.footer(), hint, "nothing hovered: where a click goes");
    let hint_for_rows = hint.clone();
    let row = r
        .rows(AccessibleRole::ListItem)
        .into_iter()
        .find(|e| {
            e.accessible_description().as_deref() == Some(hint_for_rows.as_str())
                && e.accessible_label().is_some_and(|l| l.starts_with("RGB,"))
        })
        .expect("the RGB row");
    let (p, sz) = (row.absolute_position(), row.size());
    r.pointer_to(p.x + 60.0, p.y + sz.height / 2.0);
    let rgb = r.state().get_formats().row_data(1).unwrap();
    assert_eq!(r.footer(), format!("{} {}", rgb.label, rgb.value));
    r.pointer_to(p.x + sz.width - 14.0, p.y + sz.height / 2.0);
    assert_eq!(r.footer(), "Copy", "the copy glyph says what it does");
    r.pointer_to(2.0, 2.0);
    assert_eq!(
        r.footer(),
        hint,
        "and the hint is back when the pointer leaves"
    );
}

#[test]
fn the_surface_announces_a_range_and_increment_and_decrement_agree_with_the_spoken_value() {
    for (style, name) in [(0, "Saturation and brightness"), (1, "Hue and saturation")] {
        let r = rig(false, (420, 700));
        r.state().invoke_input_edited("#3a7bd5".into());
        r.state().invoke_style_picked(style);
        r.settle();
        let surface = r.by_label(AccessibleRole::Slider, name);
        assert_eq!(surface.accessible_value_minimum(), Some(0.0), "{name}");
        assert_eq!(surface.accessible_value_maximum(), Some(100.0), "{name}");
        assert_eq!(surface.accessible_value_step(), Some(1.0), "{name}");
        let said = |e: &ElementHandle| e.accessible_value().unwrap().to_string();
        let before = said(&surface);
        surface.invoke_accessible_decrement_action();
        r.settle();
        let after = said(&r.by_label(AccessibleRole::Slider, name));
        assert_ne!(before, after, "{name}: the colour changed, so the words do");
        let bri = r.state().get_bri().round();
        assert!(
            after.contains(&format!("brightness {bri} percent")),
            "{name}: {after:?} names the new brightness {bri}"
        );
        r.by_label(AccessibleRole::Slider, name)
            .invoke_accessible_increment_action();
        r.settle();
        assert_eq!(
            said(&r.by_label(AccessibleRole::Slider, name)),
            before,
            "{name}: and back"
        );
    }
}

// ---- The cards -------------------------------------------------------------------------------------

#[test]
fn a_colour_card_paints_its_colour_and_a_checker_for_a_clear_one_and_names_its_kind() {
    let r = rig(false, (320, 900));
    for text in ["#3a7bd5", "#ff000000"] {
        assert!(r.ctl.borrow_mut().quick_add(
            text,
            Some(snipdeck_core::ItemKind::Text),
            false,
            snipdeck_app::item_editor::Origin::Panel
        ));
    }
    r.h.component.invoke_close_colours();
    r.binding.redraw();
    r.settle();
    let tile = |hex: &str| {
        r.rows(AccessibleRole::Image)
            .into_iter()
            .find(|e| {
                e.accessible_label()
                    .is_some_and(|l| l.ends_with(&format!(", colour {hex}")))
            })
            .unwrap_or_else(|| panic!("a colour tile for {hex}"))
    };
    let frame = r.frame();
    let pixels = |e: &ElementHandle, from: f32| {
        let (p, s) = (e.absolute_position(), e.size());
        // Inside the rounded corners (the radius is at most 8px).
        let inset = 8.0;
        let (x0, x1) = (
            (p.x + (s.width * from).max(inset)) as u32,
            (p.x + s.width - inset) as u32,
        );
        let (y0, y1) = ((p.y + inset) as u32, (p.y + s.height - inset) as u32);
        let mut seen = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                let px = frame.pixel(x, y);
                if !seen.contains(&px) {
                    seen.push(px);
                }
            }
        }
        seen
    };
    let opaque = tile("#3a7bd5");
    assert!(
        !opaque.accessible_label().unwrap().starts_with("Colour"),
        "the kind name leads the label: {:?}",
        opaque.accessible_label()
    );
    assert_eq!(
        pixels(&opaque, 0.0),
        vec![[0x3a, 0x7b, 0xd5]],
        "an opaque colour is one flat fill"
    );
    let clear = tile("#ff000000");
    let left = pixels(&clear, 0.0);
    assert!(
        left.contains(&[255, 0, 0]),
        "the opaque colour shows in a strip: {left:?}"
    );
    let right = pixels(&clear, 0.45);
    assert!(
        right.contains(&[255, 255, 255]) && right.contains(&[200, 200, 200]),
        "a checker under the clear colour: {right:?}"
    );
}

// ---- Goldens of the whole tab (light and dark, default and 150% text) ---------------------------

/// The tab with three saved colours, drawn and compared to `crates/snipdeck-ui/tests/golden/colours-*.png`
/// (`UPDATE_GOLDEN=1` rewrites them; open them), and measured: nothing drawn leaves the window sideways.
#[test]
fn the_colours_tab_light_and_dark_at_default_and_150_percent_text() {
    for (dark, scale) in [(false, 1.0_f32), (false, 1.5), (true, 1.0), (true, 1.5)] {
        let width = 320_u32;
        let r = rig(dark, (width, 900));
        r.set_text_scale(scale);
        for c in ["#3a7bd5", "#ff0000", "#10b981"] {
            r.state().invoke_input_edited(c.into());
            r.state().invoke_format_used(0);
            r.state().invoke_save(-1);
        }
        r.state().invoke_input_edited("#3a7bd5".into());
        r.settle();
        let at = format!("{} text x{scale}", if dark { "dark" } else { "light" });
        // Below the Lists strip, which scrolls sideways on purpose.
        let strip = r.by_label(AccessibleRole::TabList, "Lists");
        let below = strip.absolute_position().y + strip.size().height - 0.5;
        let outside = ElementQuery::from_root(&r.h.component)
            .match_predicate(move |e| {
                let (p, s) = (e.absolute_position(), e.size());
                p.y >= below
                    && s.width > 0.5
                    && s.height > 0.5
                    && (p.x < -0.5 || p.x + s.width > width as f32 + 0.5)
            })
            .find_all();
        assert!(
            outside.is_empty(),
            "{} element(s) leave the window sideways, {at}: {:?}",
            outside.len(),
            outside
                .iter()
                .map(|e| (
                    e.type_name(),
                    e.accessible_label(),
                    e.absolute_position(),
                    e.size()
                ))
                .collect::<Vec<_>>()
        );
        snipdeck_ui::testing::assert_golden(
            &format!(
                "colours-{}-x{}",
                if dark { "dark" } else { "light" },
                (scale * 100.0) as u32
            ),
            &r.frame(),
        );
    }
}
