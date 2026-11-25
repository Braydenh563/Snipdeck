//! O39: a window whose pixels Windows threw away (a resize, a show, Alt+Tab back, restore) must
//! be redrawn whole.
//!
//! The Windows backend (softbuffer, GDI) copies only the damaged part of a frame and presents
//! nothing when nothing is dirty. These tests draw the way it does (one kept buffer, damage only)
//! and check that `damage::whole_window` damages the whole window, that `damage::mark_whole`
//! asks for no frame of its own, and that every top-level window paints an opaque theme colour
//! as its background.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use slint::ComponentHandle;
use snipdeck_ui::{HelpWindow, MainWindow, PanelWindow, TagManager, damage, testing::Harness};

/// Draws until nothing is dirty, as the backend does between events, and until a frame asked
/// for redraws nothing (the first frame after the window settles is drawn whole).
fn settle<C: ComponentHandle>(h: &Harness<C>) {
    for _ in 0..16 {
        slint::platform::update_timers_and_animations();
        if h.damaged_area().is_none() {
            h.component.window().request_redraw();
            if h.damaged_area() == Some(0) {
                return;
            }
        }
    }
    panic!("the window never stopped repainting");
}

/// An idle window presents nothing; `whole_window` repaints all of it, every time.
fn whole_window_repaints_all<C: ComponentHandle>(h: &Harness<C>, w: u32, h_px: u32) {
    settle(h);
    assert_eq!(h.damaged_area(), None, "idle: no frame is presented");
    damage::whole_window(h.component.window());
    assert_eq!(h.damaged_area(), Some(u64::from(w) * u64::from(h_px)));
    settle(h);
    damage::whole_window(h.component.window());
    assert_eq!(
        h.damaged_area(),
        Some(u64::from(w) * u64::from(h_px)),
        "again: all of it"
    );
}

#[test]
fn whole_window_damage_repaints_all_of_the_main_window() {
    let main = Harness::new(MainWindow::new, 1040, 640);
    whole_window_repaints_all(&main, 1040, 640);
}

#[test]
fn whole_window_damage_repaints_all_of_the_panel() {
    let panel = Harness::new(PanelWindow::new, 320, 480);
    whole_window_repaints_all(&panel, 320, 480);
}

#[test]
fn whole_window_damage_repaints_all_of_a_child_window() {
    let tags = Harness::new(TagManager::new, 420, 520);
    whole_window_repaints_all(&tags, 420, 520);
    let help = Harness::new(HelpWindow::new, 420, 520);
    whole_window_repaints_all(&help, 420, 520);
}

/// `mark_whole` is for the frame being drawn (winit's `RedrawRequested`): it asks for no frame
/// itself, and the frame that is drawn is drawn whole.
#[test]
fn mark_whole_asks_for_no_frame_and_the_next_frame_is_drawn_whole() {
    let (w, h) = (420_u32, 520_u32);
    let tags = Harness::new(TagManager::new, w, h);
    settle(&tags);
    let before = damage::marked_whole();
    damage::mark_whole(tags.component.window());
    assert_eq!(damage::marked_whole(), before + 1, "counted");
    assert_eq!(tags.damaged_area(), None, "no frame asked for");
    tags.component.window().request_redraw();
    assert_eq!(tags.damaged_area(), Some(u64::from(w) * u64::from(h)));
    assert_eq!(tags.damaged_area(), None, "and no frame after it");
}

/// A missed frame must show a theme colour, never the desktop: every top-level window sets an
/// opaque `background` on its root, from the theme (`Theme.` ...), never `transparent` or an alpha.
#[test]
fn every_top_level_window_has_an_opaque_theme_background() {
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut files = Vec::new();
    collect(&ui, &mut files);
    assert!(files.len() > 5, "found no .slint files");
    let mut windows = 0;
    for file in files {
        let text = std::fs::read_to_string(&file).expect("reading a .slint file");
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("inherits Window") {
                continue;
            }
            windows += 1;
            // The root's own properties are indented four spaces; the component ends at `}`.
            let background = lines[i + 1..]
                .iter()
                .take_while(|l| !l.starts_with('}'))
                .find(|l| l.starts_with("    background:"));
            let name = line.trim();
            let background = background
                .unwrap_or_else(|| panic!("{}: `{name}` has no root background", file.display()));
            assert!(
                background.contains("Theme.bg-"),
                "{}: `{name}` background must be a Theme.bg-* colour: {background}",
                file.display()
            );
            assert!(
                !background.contains("transparent") && !background.contains("with-alpha"),
                "{}: `{name}` background must be opaque: {background}",
                file.display()
            );
        }
    }
    assert!(windows >= 10, "found only {windows} windows");
}

fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("reading the ui folder") {
        let path = entry.expect("reading an entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "slint") {
            out.push(path);
        }
    }
}

/// The whole-window damage changes no pixel: it redraws what is there.
#[test]
fn whole_window_damage_changes_no_pixel() {
    let main = Harness::new(MainWindow::new, 1040, 640);
    let before = main.render_settled();
    damage::whole_window(main.component.window());
    let after = main.render_settled();
    for (x, y) in [(10, 10), (300, 300), (700, 400), (1000, 600)] {
        assert_eq!(
            before.pixel(x, y),
            after.pixel(x, y),
            "pixel ({x}, {y}) changed when the window was damaged whole"
        );
    }
}
