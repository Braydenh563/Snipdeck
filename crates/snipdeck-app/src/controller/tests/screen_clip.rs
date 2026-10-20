//! "Take screenshot" through the controller: the three ways in, the Panel stepping aside and
//! coming back, the setting, and the failure toast. The machine (overlay and clipboard) is a
//! fake the test drives, and time is the frozen clock moved by hand.

use std::{cell::RefCell, rc::Rc};

use super::*;
use crate::{
    commands::Command,
    screen_clip::{ClipboardProbe, ScreenClipHost},
};

/// What the fake machine saw and what it says.
#[derive(Default)]
struct Machine {
    launches: u32,
    launch_fails: bool,
    probe: Option<ClipboardProbe>,
}

struct FakeHost(Rc<RefCell<Machine>>);

impl ScreenClipHost for FakeHost {
    fn launch(&mut self) -> Result<(), String> {
        let mut m = self.0.borrow_mut();
        m.launches += 1;
        if m.launch_fails {
            Err("no snipping tool".into())
        } else {
            Ok(())
        }
    }

    fn probe(&mut self) -> ClipboardProbe {
        self.0.borrow().probe.unwrap_or(ClipboardProbe {
            sequence: 0,
            has_image: false,
        })
    }
}

fn with_host(w: &mut World) -> Rc<RefCell<Machine>> {
    let machine = Rc::new(RefCell::new(Machine::default()));
    w.ctl.set_screen_clip_host(Box::new(FakeHost(Rc::clone(&machine))));
    machine
}

fn clipboard(sequence: u32, has_image: bool) -> Option<ClipboardProbe> {
    Some(ClipboardProbe {
        sequence,
        has_image,
    })
}

/// Moves the controller's clock `seconds` past its start.
fn after(w: &World, seconds: i64) {
    let start = at("2026-10-17T10:30:00Z").as_second();
    w.clock
        .set(Timestamp::from_second(start + seconds).expect("a moment"));
}

fn toast(w: &World) -> Option<ToastMessage> {
    w.ctl.view().toast.map(|t| t.message)
}

#[test]
fn the_panel_menu_hides_the_panel_and_starts_the_overlay() {
    let mut w = world();
    let machine = with_host(&mut w);
    w.ctl.open_app_menu();
    let row = APP_MENU
        .iter()
        .position(|e| *e == AppMenuEntry::Screenshot)
        .expect("in the menu");
    assert_eq!(w.ctl.choose_app_menu(row), [Effect::HidePanel]);
    assert_eq!(machine.borrow().launches, 1);
    assert!(!w.ctl.visible());
    assert!(w.ctl.screen_clip_waiting());
    assert!(w.ctl.view().app_menu.is_none());
}

#[test]
fn the_tray_entry_does_the_same() {
    let mut w = world();
    let machine = with_host(&mut w);
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TakeScreenshot),
        [Effect::HidePanel]
    );
    assert_eq!(machine.borrow().launches, 1);
}

#[test]
fn the_palette_command_queues_the_same_effects_for_the_host() {
    let mut w = world();
    let machine = with_host(&mut w);
    w.ctl.run_command(Command::TakeScreenshot, 0);
    assert_eq!(w.ctl.take_pending(), [Effect::HidePanel]);
    assert_eq!(machine.borrow().launches, 1);
}

#[test]
fn a_picture_on_the_clipboard_shows_the_panel_again_and_adds_nothing() {
    let mut w = world();
    let machine = with_host(&mut w);
    machine.borrow_mut().probe = clipboard(4, false);
    let items = w.ctl.view().rows.len();
    w.ctl.on_tray(TrayEvent::TakeScreenshot);

    after(&w, 2);
    assert_eq!(w.ctl.screen_clip_tick(), Vec::new(), "nothing new yet");
    machine.borrow_mut().probe = clipboard(5, true);
    after(&w, 4);
    assert_eq!(w.ctl.screen_clip_tick(), [Effect::ShowPanel]);
    assert!(w.ctl.visible());
    assert!(!w.ctl.screen_clip_waiting());
    assert_eq!(w.ctl.view().rows.len(), items, "the snip is not saved");
    assert_eq!(w.ctl.screen_clip_tick(), Vec::new(), "once");
}

#[test]
fn the_panel_comes_back_by_itself_after_a_minute() {
    let mut w = world();
    let machine = with_host(&mut w);
    machine.borrow_mut().probe = clipboard(4, false);
    w.ctl.on_tray(TrayEvent::TakeScreenshot);
    after(&w, 59);
    assert_eq!(w.ctl.screen_clip_tick(), Vec::new());
    after(&w, 60);
    assert_eq!(w.ctl.screen_clip_tick(), [Effect::ShowPanel]);
    assert!(w.ctl.visible());
}

#[test]
fn a_launch_that_fails_shows_the_panel_at_once_with_a_toast() {
    let mut w = world();
    let machine = with_host(&mut w);
    machine.borrow_mut().launch_fails = true;
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TakeScreenshot),
        [Effect::HidePanel, Effect::ShowPanel]
    );
    assert!(w.ctl.visible());
    assert!(!w.ctl.screen_clip_waiting());
    assert_eq!(toast(&w), Some(ToastMessage::ScreenshotFailed));
}

#[test]
fn without_a_machine_to_launch_on_it_fails_the_same_way() {
    let mut w = world();
    assert_eq!(
        w.ctl.on_tray(TrayEvent::TakeScreenshot),
        [Effect::HidePanel, Effect::ShowPanel]
    );
    assert_eq!(toast(&w), Some(ToastMessage::ScreenshotFailed));
}

#[test]
fn keeping_the_panel_visible_hides_nothing_and_blur_does_not_hide_it_either() {
    let mut w = world_with(
        Some(outlook()),
        Settings {
            screenshot_keep_panel: true,
            ..Settings::default()
        },
    );
    let machine = with_host(&mut w);
    machine.borrow_mut().probe = clipboard(4, false);
    // Past the grace after showing, with the Panel unpinned and hiding on blur on.
    w.ctl.set_on_top(false);
    after(&w, 5);
    assert_eq!(w.ctl.on_tray(TrayEvent::TakeScreenshot), Vec::new());
    assert!(w.ctl.visible());
    assert_eq!(machine.borrow().launches, 1);
    // The overlay takes the foreground: the Panel stays.
    assert_eq!(w.ctl.on_foreground_changed(), Vec::new());
    assert!(w.ctl.visible());
    machine.borrow_mut().probe = clipboard(5, true);
    after(&w, 8);
    assert_eq!(w.ctl.screen_clip_tick(), Vec::new(), "nothing was hidden");
    // Done: blur hides an unpinned Panel again.
    assert_eq!(w.ctl.on_foreground_changed(), [Effect::HidePanel]);
}

#[test]
fn a_second_ask_while_the_overlay_is_up_starts_nothing() {
    let mut w = world();
    let machine = with_host(&mut w);
    w.ctl.on_tray(TrayEvent::TakeScreenshot);
    assert_eq!(w.ctl.on_tray(TrayEvent::TakeScreenshot), Vec::new());
    assert_eq!(machine.borrow().launches, 1);
}

#[test]
fn a_hidden_panel_stays_hidden_when_it_was_not_hidden_for_the_screenshot() {
    let mut w = world();
    let machine = with_host(&mut w);
    machine.borrow_mut().probe = clipboard(4, false);
    w.ctl.hide();
    assert_eq!(w.ctl.on_tray(TrayEvent::TakeScreenshot), Vec::new());
    machine.borrow_mut().probe = clipboard(5, true);
    after(&w, 3);
    assert_eq!(w.ctl.screen_clip_tick(), Vec::new());
    assert!(!w.ctl.visible());
}
