//! A tooltip draws above everything in its window and is never clipped by a container or a narrow parent (DESIGN.md §4
//! Tooltip): the window's `TipLayer` is its last child, so a later sibling (the heading, the toolbar) cannot paint over
//! it, the collapsed sidebar cannot cut it off, and a tooltip near an edge is shifted to stay inside the window.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::expect_used,
    reason = "pixel geometry of a test frame: logical sizes are small and positive"
)]

use std::time::Duration;

use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementQuery};
use slint::{ComponentHandle, LogicalPosition, platform::WindowEvent};
use snipdeck_ui::{
    MainWindow,
    testing::{Frame, Harness},
};

#[path = "support/main_fixture.rs"]
mod fixture;

const INK_LIGHT: [u8; 3] = [0x17, 0x16, 0x13];

fn window(size: (u32, u32), collapsed: bool) -> Harness<MainWindow> {
    let h = Harness::new(MainWindow::new, size.0, size.1);
    h.component.set_dark(false);
    h.component.set_sidebar_collapsed(collapsed);
    let _ = fixture::populate(&h.component);
    let _ = h.render_settled();
    h
}

fn button(h: &Harness<MainWindow>, label: &str) -> ElementHandle {
    ElementHandle::find_by_accessible_label(&h.component, label)
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("no button labelled {label:?}"))
}

/// Hovers the button until its tooltip is up and returns the tooltip's box (x, y, w, h) and the frame.
fn hover_tip(h: &Harness<MainWindow>, label: &str) -> ((f32, f32, f32, f32), Frame) {
    let control = button(h, label);
    let (at, size) = (control.absolute_position(), control.size());
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0),
        });
    h.advance(Duration::from_millis(600));
    // A frame creates the tooltip and starts its 90ms fade; the next one, after the fade, is checked.
    let _ = h.render_settled();
    h.advance(Duration::from_millis(200));
    let frame = h.render_settled();
    let tips = ElementQuery::from_root(&h.component)
        .match_inherits("Tooltip")
        .find_all();
    let tip = tips.last().expect("a tooltip is on screen");
    let (pos, dims) = (tip.absolute_position(), tip.size());
    ((pos.x, pos.y, dims.width, dims.height), frame)
}

/// Every pixel of the tooltip's left and right padding columns (clear of the glyphs, which start 8px in) is the ink
/// fill: the whole width is on screen and nothing paints over it.
fn assert_whole(f: &Frame, (x, y, w, hgt): (f32, f32, f32, f32), what: &str) {
    assert!(
        w > 40.0 && hgt > 16.0,
        "{what}: a tooltip box, got {w}x{hgt}"
    );
    for col in [x as u32 + 2, (x + w) as u32 - 3] {
        for row in (y as u32 + 3)..((y + hgt) as u32 - 3) {
            assert_eq!(
                f.pixel(col, row),
                INK_LIGHT,
                "{what}: ink at ({col}, {row}), box {x},{y} {w}x{hgt}"
            );
        }
    }
}

#[test]
fn the_collapse_tooltip_is_whole_over_the_heading_and_toolbar_below_it() {
    let h = window((1040, 640), false);
    let (b, f) = hover_tip(&h, "Collapse sidebar (Ctrl+B)");
    assert_whole(&f, b, "expanded sidebar");
}

#[test]
fn the_expand_tooltip_on_a_collapsed_sidebar_is_not_cut_off() {
    let h = window((1040, 640), true);
    assert!(h.component.get_rail(), "the sidebar is a rail");
    let btn = button(&h, "Expand sidebar (Ctrl+B)");
    let (b, f) = hover_tip(&h, "Expand sidebar (Ctrl+B)");
    assert!(
        b.2 > btn.size().width * 3.0,
        "the tooltip ({}px) is much wider than the rail button",
        b.2
    );
    assert_whole(&f, b, "collapsed sidebar");
}

#[test]
fn a_tooltip_near_the_right_edge_stays_inside_the_window() {
    let size = (720, 480);
    let h = window(size, false);
    h.component.set_panel_hotkey("Ctrl+Alt+Space".into());
    let (b, f) = hover_tip(&h, "Show Panel (Ctrl+Alt+Space)");
    assert!(b.0 >= 8.0, "starts at x {}", b.0);
    assert!(b.0 + b.2 <= size.0 as f32 - 8.0, "ends at x {}", b.0 + b.2);
    assert_whole(&f, b, "right edge");
}

#[test]
fn a_tooltip_fades_in_over_90ms() {
    let h = window((1040, 640), false);
    let control = button(&h, "Compact view");
    let (at, size) = (control.absolute_position(), control.size());
    h.component
        .window()
        .dispatch_event(WindowEvent::PointerMoved {
            position: LogicalPosition::new(at.x + size.width / 2.0, at.y + size.height / 2.0),
        });
    // The rest ends: the tooltip is created, closed, and starts to fade in.
    h.advance(Duration::from_millis(500));
    let first = h.render_settled();
    let tips = ElementQuery::from_root(&h.component)
        .match_inherits("Tooltip")
        .find_all();
    let tip = tips.last().expect("a tooltip is on screen");
    let (pos, dims) = (tip.absolute_position(), tip.size());
    let (col, row) = (pos.x as u32 + 2, (pos.y + dims.height / 2.0) as u32);
    assert_ne!(
        first.pixel(col, row),
        INK_LIGHT,
        "not yet opaque when it appears"
    );
    h.advance(Duration::from_millis(200));
    let later = h.render_settled();
    assert_eq!(later.pixel(col, row), INK_LIGHT, "opaque after the fade");
}
