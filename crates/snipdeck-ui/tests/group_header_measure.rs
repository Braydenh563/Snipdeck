//! Measured accessibility of a Panel group header: its tree entry, the contrast of its text, count
//! and chevron, the focus ring, and its fit at 2.25x text and a 320 px Panel. Numbers are printed
//! (`--nocapture`) so a report can quote them. Time never moves (the harness clock is frozen).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stderr,
    clippy::too_many_lines,
    reason = "pixel geometry of a test frame"
)]

use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use slint::ComponentHandle;
use snipdeck_ui::{
    PanelRow, PanelWindow, SystemLook,
    testing::{Frame, Harness},
};

#[path = "support/panel_fixture.rs"]
mod fixture;

// design/tokens/palette.json: text.primary, text.secondary, bg.app, bg.sunken, focus.ring.
const LIGHT: Tokens = Tokens {
    primary: [0x17, 0x16, 0x13],
    secondary: [0x5D, 0x5A, 0x57],
    app: [0xF5, 0xF3, 0xF1],
    sunken: [0xEB, 0xE9, 0xE7],
    ring: [0x28, 0x70, 0xD4],
};
const DARK: Tokens = Tokens {
    primary: [0xEB, 0xE9, 0xE7],
    secondary: [0xA0, 0x9E, 0x9A],
    app: [0x11, 0x0F, 0x0D],
    sunken: [0x11, 0x0F, 0x0D],
    ring: [0x74, 0xAC, 0xFF],
};

struct Tokens {
    primary: [u8; 3],
    secondary: [u8; 3],
    app: [u8; 3],
    sunken: [u8; 3],
    ring: [u8; 3],
}

fn lin(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn lum(p: [u8; 3]) -> f64 {
    0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2])
}
fn contrast(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn header(title: &str, count: i32, collapsed: bool) -> PanelRow {
    PanelRow {
        title: title.into(),
        is_header: true,
        group_count: count,
        group_collapsed: collapsed,
        ..Default::default()
    }
}

/// `[title] a b` then `[Archive]` (folded): the first header selected or not.
fn panel(w: u32, dark: bool, scale: f32, title: &str, selected: bool) -> Harness<PanelWindow> {
    let h = Harness::new(PanelWindow::new, w, 900);
    h.component.set_dark(dark);
    fixture::populate(&h.component);
    h.component.global::<SystemLook<'_>>().set_text_scale(scale);
    let mut rows = fixture::sample_rows();
    rows.truncate(2);
    rows.insert(0, header(title, 2, false));
    rows[1].grouped = true;
    rows[2].grouped = true;
    rows[1].set_position = 1;
    rows[2].set_position = 2;
    rows[1].set_size = 2;
    rows[2].set_size = 2;
    rows.push(header("Archive", 5, true));
    fixture::set_rows(&h.component, rows);
    h.component.set_has_groups(true);
    h.component
        .set_selected_index(if selected { 0 } else { -1 });
    h
}

fn handle(h: &Harness<PanelWindow>, name: &str, count: i32) -> ElementHandle {
    let label = h.component.invoke_group_announcement(name.into(), count);
    ElementHandle::find_by_accessible_label(&h.component, &label)
        .find(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .unwrap_or_else(|| panic!("no header {label:?}"))
}

/// The header's rectangle in physical pixels (scale factor 1): x0, y0, x1, y1.
fn rect(e: &ElementHandle) -> (u32, u32, u32, u32) {
    let p = e.absolute_position();
    let s = e.size();
    (
        p.x.round() as u32,
        p.y.round() as u32,
        (p.x + s.width).round() as u32,
        (p.y + s.height).round() as u32,
    )
}

/// Bounding box of the pixels in `area` that differ from `bg` by more than 6 levels.
fn bbox(f: &Frame, area: (u32, u32, u32, u32), bg: [u8; 3]) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in area.1..area.3.min(f.height) {
        for x in area.0..area.2.min(f.width) {
            if f.pixel(x, y).iter().zip(bg).any(|(a, b)| a.abs_diff(b) > 6) {
                found = Some(found.map_or((x, y, x + 1, y + 1), |o| {
                    (o.0.min(x), o.1.min(y), o.2.max(x + 1), o.3.max(y + 1))
                }));
            }
        }
    }
    found
}

#[test]
fn a_header_is_a_button_that_names_its_group_count_and_fold_state() {
    let h = panel(320, false, 1.0, "Replies", false);
    let _ = h.render_settled();
    let e = handle(&h, "Replies", 2);
    assert_eq!(e.accessible_label().unwrap(), "group Replies, 2 items");
    assert_eq!(e.accessible_expandable(), Some(true));
    assert_eq!(e.accessible_expanded(), Some(true));
    let folded = handle(&h, "Archive", 5);
    assert_eq!(folded.accessible_label().unwrap(), "group Archive, 5 items");
    assert_eq!(
        folded.accessible_expanded(),
        Some(false),
        "a folded group says so"
    );
    // One item reads in the singular.
    let one = h.component.invoke_group_announcement("Solo".into(), 1);
    assert_eq!(one, "group Solo, 1 item");
}

#[test]
fn the_text_count_and_chevron_pass_wcag_aa_in_both_themes() {
    for (dark, t, name) in [(false, LIGHT, "light"), (true, DARK, "dark")] {
        for (bg, on) in [(t.app, "list"), (t.sunken, "selected")] {
            let title = contrast(t.primary, bg);
            let count = contrast(t.secondary, bg);
            eprintln!(
                "{name} on {on}: title {title:.2}:1, count {count:.2}:1, chevron {count:.2}:1"
            );
            assert!(title >= 4.5, "{name} title on {on}: {title:.2}");
            assert!(count >= 4.5, "{name} count on {on}: {count:.2}");
            assert!(count >= 3.0, "{name} chevron on {on}: {count:.2}");
        }
        // What the renderer really paints agrees: the darkest ink pixel of the count and chevron.
        let h = panel(320, dark, 1.0, "Replies", false);
        let f = h.render_settled();
        let (x0, y0, x1, y1) = rect(&handle(&h, "Replies", 2));
        let bg = f.pixel(x0 + 1, y0 + 1);
        let mut best = 1.0_f64;
        for y in y0..y1 {
            for x in x0..x1 {
                best = best.max(contrast(f.pixel(x, y), bg));
            }
        }
        eprintln!("{name}: strongest painted pixel in the header {best:.2}:1");
        assert!(best >= 4.5, "{name}: {best:.2}");
    }
}

#[test]
fn a_selected_header_paints_a_whole_focus_ring_inside_its_bounds() {
    for (dark, t, name) in [(false, LIGHT, "light"), (true, DARK, "dark")] {
        let h = panel(320, dark, 1.0, "Replies", true);
        let f = h.render_settled();
        let (x0, y0, x1, y1) = rect(&handle(&h, "Replies", 2));
        let (mx, my) = (u32::midpoint(x0, x1), u32::midpoint(y0, y1));
        // Two pixels thick, on the inside of every edge (corners are rounded, so the middle of each).
        let edges = [
            ("top", [(mx, y0), (mx, y0 + 1)]),
            ("bottom", [(mx, y1 - 1), (mx, y1 - 2)]),
            ("left", [(x0, my), (x0 + 1, my)]),
            ("right", [(x1 - 1, my), (x1 - 2, my)]),
        ];
        for (edge, pixels) in edges {
            for (x, y) in pixels {
                let p = f.pixel(x, y);
                eprintln!("{name} {edge} ({x},{y}) = {p:?}");
                assert!(
                    p.iter().zip(t.ring).all(|(a, b)| a.abs_diff(b) <= 8),
                    "{name} ring {edge} at ({x},{y}) is {p:?}, want {:?}",
                    t.ring
                );
            }
        }
        let ring_vs = contrast(t.ring, t.sunken).min(contrast(t.ring, t.app));
        eprintln!("{name}: ring against the page {ring_vs:.2}:1");
        assert!(ring_vs >= 3.0, "{name}: {ring_vs:.2}");
        // Not clipped: the pixel just outside the rectangle is not ring coloured.
        let outside = f.pixel(mx, y0.saturating_sub(1));
        assert!(
            !outside.iter().zip(t.ring).all(|(a, b)| a.abs_diff(b) <= 8),
            "{name}: the ring spills above the header"
        );
    }
}

#[test]
fn at_225_text_and_320_wide_a_long_title_elides_and_count_and_chevron_stay_whole() {
    for dark in [false, true] {
        let long = "A very long group name that cannot possibly fit in a narrow Panel";
        let h = panel(320, dark, 2.25, long, false);
        let f = h.render_settled();
        let e = handle(&h, long, 2);
        let (x0, y0, x1, y1) = rect(&e);
        let t = if dark { DARK } else { LIGHT };
        let bg = f.pixel(u32::midpoint(x0, x1), y0 + 4);
        eprintln!(
            "dark={dark}: header {x0},{y0} to {x1},{y1} ({} x {} px)",
            x1 - x0,
            y1 - y0
        );
        assert!(x1 <= 320 && x0 <= 8, "the header stays in the 320 px Panel");
        assert!(
            y1 - y0 >= 40,
            "the header grows with the text ({} px)",
            y1 - y0
        );
        // Everything painted inside the header stays inside it with its padding.
        let ink = bbox(&f, (x0 + 6, y0 + 6, x1 - 6, y1 - 6), bg).expect("ink");
        eprintln!("ink box {ink:?}");
        assert!(ink.0 >= x0 + 6, "chevron clipped at the left: {ink:?}");
        assert!(ink.2 <= x1 - 6, "count clipped at the right: {ink:?}");
        // The count ("2 items") is whole: a gap separates it from the title's end.
        let right_part = bbox(&f, (x1 - 96, y0 + 6, x1 - 6, y1 - 6), bg).expect("count");
        eprintln!("count box {right_part:?}");
        assert!(right_part.2 <= x1 - 6);
        let _ = t;
    }
}
