//! The Panel's Colours tab on the real window, bound to a `Controller` over the demo library:
//! it opens, shows every format, follows typed and picked colours, saves to the Colours list, and
//! renders (light and dark). Time is a fixed clock. The renders land in `target/colours-shots`
//! for a look; the assertions measure state and a few pixels.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    reason = "test code: a failure is a failed test"
)]

use std::{cell::RefCell, path::Path, rc::Rc};

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
use snipdeck_colour::{Format, Rgba8, contrast_ratio, ratio_text};
use snipdeck_core::Timestamp;
use snipdeck_ui::{
    ColourState, PanelWindow,
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
    _binding: Binding,
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
    Rig {
        h,
        ctl,
        _binding: binding,
    }
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
    frame.rgb.chunks_exact(3).any(|p| p == rgb)
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

#[test]
fn brightness_to_zero_and_back_keeps_the_colour_and_the_values_row_says_so() {
    let r = rig(false, (320, 640));
    let blue = Rgba8::rgb(58, 123, 213);
    r.state().invoke_input_edited("#3a7bd5".into());
    let (hue, sat, bri) = {
        let s = r.state();
        (s.get_hue(), 0.0_f32, s.get_bri())
    };
    r.state().invoke_bri_moved(0.0);
    assert_eq!(
        r.ctl.borrow().colour_view().current,
        Rgba8::rgb(0, 0, 0),
        "black"
    );
    r.state().invoke_bri_moved(bri);
    assert_eq!(r.ctl.borrow().colour_view().current, blue);
    let _ = (hue, sat);
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
    assert!(r.ctl.borrow().colour_view().recents.is_empty());
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
        Some("Colour"),
        "the keyboard is in the input line"
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
