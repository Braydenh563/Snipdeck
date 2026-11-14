//! The window hooks draw a frame whole after Windows may have drawn over the window (render
//! flicker, 2026-11-14; `docs/reviews/render-flicker-2026-11-14.md`).
//!
//! The hook body ([`repaint::follow`]) is driven with real winit events on a software-rendered
//! window that draws the way the Windows backend does (one kept buffer, damage only). A source
//! check then makes sure every window's hook calls it and every popup window gets one.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use slint::{
    ComponentHandle,
    winit_030::winit::{dpi::PhysicalSize, event::WindowEvent},
};
use snipdeck_app::repaint::{self, Watch};
use snipdeck_ui::{TagManager, testing::Harness};

const W: u32 = 420;
const H: u32 = 520;

/// A window that is drawn and idle: a frame asked for now redraws nothing (the first frame
/// after the window settles is drawn whole, so it is drawn here, not in a test).
fn settled() -> Harness<TagManager> {
    let tags = Harness::new(TagManager::new, W, H);
    for _ in 0..16 {
        slint::platform::update_timers_and_animations();
        if tags.damaged_area().is_none() {
            tags.component.window().request_redraw();
            if tags.damaged_area() == Some(0) {
                return tags;
            }
        }
    }
    panic!("the window never stopped repainting");
}

#[test]
fn the_redraw_after_a_resize_is_drawn_whole_and_no_extra_frame_follows() {
    let tags = settled();
    let window = tags.component.window();
    let mut watch = Watch::new();
    repaint::follow(
        &mut watch,
        window,
        &WindowEvent::Resized(PhysicalSize::new(W, H)),
    );
    // Slint draws on the `RedrawRequested` that answers the request; the hook runs first.
    repaint::follow(&mut watch, window, &WindowEvent::RedrawRequested);
    assert_eq!(
        tags.damaged_area(),
        Some(u64::from(W) * u64::from(H)),
        "drawn whole"
    );
    assert_eq!(tags.damaged_area(), None, "and no extra, empty frame");
}

#[test]
fn the_redraw_after_the_focus_comes_back_is_drawn_whole() {
    let tags = settled();
    let window = tags.component.window();
    let mut watch = Watch::new();
    repaint::follow(&mut watch, window, &WindowEvent::Focused(true));
    repaint::follow(&mut watch, window, &WindowEvent::RedrawRequested);
    assert_eq!(tags.damaged_area(), Some(u64::from(W) * u64::from(H)));
}

#[test]
fn an_ordinary_redraw_stays_partial() {
    let tags = settled();
    let window = tags.component.window();
    let mut watch = Watch::new();
    repaint::follow(&mut watch, window, &WindowEvent::Focused(false));
    assert_eq!(
        tags.damaged_area(),
        None,
        "losing the focus asks for nothing"
    );
    window.request_redraw();
    repaint::follow(&mut watch, window, &WindowEvent::RedrawRequested);
    assert_eq!(
        tags.damaged_area(),
        Some(0),
        "nothing changed: nothing redrawn"
    );
}

fn source(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A window has one hook. Every hook the app installs (the Panel, the Main window, every regular
/// window) feeds its events to `repaint::follow`, and every popup window (the Picker, a toast,
/// the suggestion hint) gets `repaint::watch` when it is made.
#[test]
fn every_window_hook_follows_the_repaint_watch() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hooks = 0;
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("reading src") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("repaint.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("reading a source file");
            let installed = text.matches(".on_winit_window_event(").count();
            hooks += installed;
            assert_eq!(
                installed,
                text.matches("repaint::follow(").count(),
                "{}: every window hook calls repaint::follow",
                path.display()
            );
        }
    }
    assert!(hooks >= 3, "found only {hooks} window hooks");
    let popup = source("popup_window.rs");
    let ensure = &popup[popup.find("pub(crate) fn ensure(").expect("Shell::ensure")..];
    let ensure = &ensure[..ensure.find("\n    }\n").expect("the end of ensure")];
    assert!(
        ensure.contains("crate::repaint::watch(window.window());"),
        "a popup window gets the repaint watch when it is made"
    );
}
