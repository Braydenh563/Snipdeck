//! O39: a window whose pixels Windows threw away (Alt+Tab back, restore) must be redrawn whole.
//!
//! The Windows backend (softbuffer, GDI) copies only the damaged part of a frame and presents
//! nothing when nothing is dirty. These tests draw the way it does (one kept buffer, damage only)
//! and check that flipping `repaint-flip` damages the whole window, and that every top-level
//! window paints an opaque theme colour as its background.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use slint::ComponentHandle;
use snipdeck_ui::{MainWindow, PanelWindow, testing::Harness};

/// Draws until nothing is dirty, as the backend does between events.
fn settle<C: ComponentHandle>(h: &Harness<C>) {
    for _ in 0..16 {
        slint::platform::update_timers_and_animations();
        if h.damaged_area().is_none() {
            return;
        }
    }
    panic!("the window never stopped repainting");
}

#[test]
fn an_idle_main_window_presents_nothing_and_the_flip_repaints_all_of_it() {
    let (w, h) = (1040_u32, 640_u32);
    let main = Harness::new(MainWindow::new, w, h);
    settle(&main);
    assert_eq!(main.damaged_area(), None, "idle: no frame is presented");
    main.component
        .set_repaint_flip(!main.component.get_repaint_flip());
    assert_eq!(main.damaged_area(), Some(u64::from(w) * u64::from(h)));
    settle(&main);
    main.component
        .set_repaint_flip(!main.component.get_repaint_flip());
    assert_eq!(
        main.damaged_area(),
        Some(u64::from(w) * u64::from(h)),
        "flipping back repaints all again"
    );
}

#[test]
fn an_idle_panel_presents_nothing_and_the_flip_repaints_all_of_it() {
    let (w, h) = (320_u32, 480_u32);
    let panel = Harness::new(PanelWindow::new, w, h);
    settle(&panel);
    assert_eq!(panel.damaged_area(), None, "idle: no frame is presented");
    panel
        .component
        .set_repaint_flip(!panel.component.get_repaint_flip());
    assert_eq!(panel.damaged_area(), Some(u64::from(w) * u64::from(h)));
    settle(&panel);
    panel
        .component
        .set_repaint_flip(!panel.component.get_repaint_flip());
    assert_eq!(
        panel.damaged_area(),
        Some(u64::from(w) * u64::from(h)),
        "flipping back repaints all again"
    );
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

/// The nudge must change no pixel: both of its colours are fully transparent (Slint reads
/// `#RRGGBBAA`; `#00ffffff` was opaque cyan and covered the content after Alt+Tab).
#[test]
fn the_flip_changes_no_pixel() {
    let main = Harness::new(MainWindow::new, 1040, 640);
    let before = main.render_settled();
    main.component
        .set_repaint_flip(!main.component.get_repaint_flip());
    let after = main.render_settled();
    for (x, y) in [(10, 10), (300, 300), (700, 400), (1000, 600)] {
        assert_eq!(
            before.pixel(x, y),
            after.pixel(x, y),
            "pixel ({x}, {y}) changed when the repaint flip was set"
        );
    }
}
