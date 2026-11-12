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

use slint::{Global, Model};
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
    shot("light-320", &r.h.render_settled());
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
    assert!(hsl.value.contains('0'), "{}", hsl.value);
    let white = s.get_contrast().row_data(0).expect("white row");
    assert_eq!(
        white.ratio.as_str(),
        ratio_text(contrast_ratio(red, Rgba8::rgb(255, 255, 255)))
    );
    assert!(!s.get_current_translucent());
    assert!(
        has_pixel(&r.h.render_settled(), [255, 0, 0]),
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
    shot("wheel-420", &r.h.render_settled());
}
