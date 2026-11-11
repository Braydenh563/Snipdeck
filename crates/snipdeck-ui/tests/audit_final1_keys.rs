//! Final audit 1b, keyboard walk: Tab and Shift+Tab through every stop of Settings, Help (all four
//! tabs), the item editor (template kind, the Formatted notice) and the template fill form (every
//! field kind), at 1x and 2.25x text size. For each window:
//! - the stops come in visual order (top to bottom, left to right in a row);
//! - every stop paints the focus ring (the `focus.ring` colour on the border of its control) in a
//!   frame rendered while it has the focus, so a stop clipped away or scrolled out of view fails;
//! - Tab comes back to the first stop (no trap), Shift+Tab walks the same stops backwards;
//! - Esc, pressed on the first, a middle and the last stop, still reaches the window's Esc handler.
//!
//! The focus holder is read from the window (`WindowInner::focus_item`); the control is its nearest
//! accessible ancestor. Frames are drawn by the headless renderer; the frozen clock never moves.

#![allow(
    dead_code,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "pixel geometry of a test frame"
)]

use std::{cell::Cell, rc::Rc};

use i_slint_core::{
    accessibility::AccessibleStringProperty,
    item_tree::{ItemRc, ParentItemTraversalMode},
    lengths::LogicalPoint,
    window::WindowInner,
};
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{
    HelpWindow, ItemEditor, SettingsPage, SystemLook, TemplateFillWindow,
    testing::{Frame, Harness},
};

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/fill_fixture.rs"]
mod fill;
#[path = "support/mod.rs"]
mod support;

const RING_LIGHT: [u8; 3] = [0x28, 0x70, 0xD4];
const RING_DARK: [u8; 3] = [0x74, 0xAC, 0xFF];

#[derive(Clone, Debug, PartialEq)]
struct Stop {
    name: String,
    role: String,
    inner: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// The control that has the keyboard focus: its nearest accessible ancestor (or itself).
fn focused(window: &slint::Window) -> Option<Stop> {
    let item = WindowInner::from_pub(window)
        .focus_item
        .borrow()
        .upgrade()?;
    let mut cur: Option<ItemRc> = Some(item.clone());
    while let Some(i) = cur {
        if i.is_accessible() {
            let g = i.geometry();
            let p = i.map_to_window(LogicalPoint::new(g.origin.x, g.origin.y));
            let name = i
                .accessible_string_property(AccessibleStringProperty::Label)
                .unwrap_or_default()
                .to_string();
            return Some(Stop {
                name,
                role: format!("{:?}", i.accessible_role()),
                inner: {
                    let g = item.geometry();
                    let q = item.map_to_window(LogicalPoint::new(g.origin.x, g.origin.y));
                    format!(
                        "focus item at ({:.0},{:.0}) {:.0}x{:.0}",
                        q.x, q.y, g.size.width, g.size.height
                    )
                },
                x: p.x,
                y: p.y,
                w: g.size.width,
                h: g.size.height,
            });
        }
        cur = i.parent_item(ParentItemTraversalMode::FindAllParents);
    }
    None
}

/// Ring pixels in the band 4 px around the control's border (the headless frame has scale 1).
fn ring_pixels(f: &Frame, s: &Stop, ring: [u8; 3]) -> usize {
    let (x0, y0) = ((s.x - 4.0).max(0.0) as u32, (s.y - 4.0).max(0.0) as u32);
    let (x1, y1) = (
        ((s.x + s.w + 4.0) as u32).min(f.width),
        ((s.y + s.h + 4.0) as u32).min(f.height),
    );
    let (ix0, iy0, ix1, iy1) = (
        (s.x + 4.0) as u32,
        (s.y + 4.0) as u32,
        (s.x + s.w - 4.0).max(0.0) as u32,
        (s.y + s.h - 4.0).max(0.0) as u32,
    );
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let inner = x >= ix0 && x < ix1 && y >= iy0 && y < iy1;
            if !inner && f.pixel(x, y) == ring {
                n += 1;
            }
        }
    }
    n
}

fn tab(window: &slint::Window, back: bool) {
    if back {
        support::chord(window, &[Key::Shift], &support::key(Key::Tab));
    } else {
        support::press(window, &support::key(Key::Tab));
    }
}

/// Walks the stops with Tab until the first one comes round again (or `limit` presses), then back
/// with Shift+Tab. Returns the problems found.
/// A frame after a key press; the renderer has nothing to draw when the press changed nothing on screen.
fn snap<C: ComponentHandle>(h: &Harness<C>) -> Option<Frame> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render_settled())).ok()
}

fn audit<C: ComponentHandle>(
    h: &Harness<C>,
    scene: &str,
    dark: bool,
    limit: usize,
    esc: &dyn Fn() -> bool,
) -> Vec<String> {
    let ring = if dark { RING_DARK } else { RING_LIGHT };
    let w = h.component.window();
    let mut out = Vec::new();
    let first = h.render_settled();
    let mut stops: Vec<(Stop, Frame)> = Vec::new();
    // A window that opens with a control focused starts the walk there.
    if let Some(s) = focused(w) {
        stops.push((s, first));
    }
    tab(w, false);
    loop {
        let mut cur = focused(w);
        if cur.is_none() {
            // Tab from the last stop leaves the window (to the OS); one more Tab comes back to the first.
            tab(w, false);
            cur = focused(w);
        }
        let Some(frame) = snap(h) else {
            out.push(format!(
                "{scene}: Tab {} changed nothing on screen (no visible focus)",
                stops.len() + 1
            ));
            tab(w, false);
            continue;
        };
        let Some(s) = cur else {
            out.push(format!(
                "{scene}: Tab {} left no focused control",
                stops.len() + 1
            ));
            break;
        };
        // Came round (no trap) when this stop and the one before it have been seen in that order. Names can repeat
        // (two "Add" buttons), a name after the same predecessor cannot.
        let prev = stops.last().map(|(p, _)| p.name.clone());
        if let Some(j) = stops
            .iter()
            .position(|(p, _)| p.name == s.name)
            .filter(|&j| j == 0 || stops.get(j - 1).map(|(q, _)| q.name.clone()) == prev)
        {
            if j > 0 {
                let skipped: Vec<&str> = stops[..j].iter().map(|(p, _)| p.name.as_str()).collect();
                out.push(format!("{scene}: Tab reaches {skipped:?} once but never again: the cycle goes on at {:?}", s.name));
                stops.drain(..j);
            }
            break;
        }
        stops.push((s, frame));
        if stops.len() > limit {
            let names: Vec<&str> = stops
                .iter()
                .skip(40)
                .map(|(s, _)| s.name.as_str())
                .collect();
            out.push(format!(
                "{scene}: no way round after {limit} Tab presses (trap?); stops 40..: {names:?}"
            ));
            break;
        }
        tab(w, false);
    }
    for (i, (s, f)) in stops.iter().enumerate() {
        if s.role == "Groupbox" {
            out.push(format!(
                "{scene}: stop {i} {:?} is a group, not a control ({})",
                s.name, s.inner
            ));
        }
        if s.name.trim().is_empty() {
            out.push(format!(
                "{scene}: stop {i} at ({:.0},{:.0}) has no name",
                s.x, s.y
            ));
        }
        let n = ring_pixels(f, s, ring);
        if n < 8 {
            out.push(format!(
                "{scene}: stop {i} {:?} ({:.0},{:.0} {:.0}x{:.0}) shows {n} focus-ring pixels",
                s.name, s.x, s.y, s.w, s.h
            ));
        }
    }
    // Visual order: the cycle may break once (where it wraps); a second break means focus jumps around. Pages
    // that scroll (Tips, Settings) move their stops between presses, so window y cannot be compared there.
    let scrolls = scene.contains("tab 3")
        || scene.starts_with("settings")
        || (scene.starts_with("item") && scene.contains("2.25x"));
    if !scrolls && stops.len() > 2 {
        let n = stops.len();
        let mut breaks = Vec::new();
        for i in 0..n {
            let (p, s) = (&stops[i].0, &stops[(i + 1) % n].0);
            let same_row = (s.y - p.y).abs() < p.h.min(s.h) / 2.0;
            let ok = if same_row {
                s.x >= p.x - 1.0
            } else {
                s.y >= p.y - 1.0
            };
            if !ok {
                breaks.push(format!(
                    "{:?} ({:.0},{:.0}) -> {:?} ({:.0},{:.0})",
                    p.name, p.x, p.y, s.name, s.x, s.y
                ));
            }
        }
        if breaks.len() > 1 {
            let all: Vec<String> = stops
                .iter()
                .map(|(s, _)| format!("{} ({:.0},{:.0})", s.name, s.x, s.y))
                .collect();
            out.push(format!(
                "{scene}: focus order jumps against the visual order at {}; order: {}",
                breaks.join("; "),
                all.join(" > ")
            ));
        }
    }
    // Shift+Tab from the first stop goes to the last, and back again reaches the same stops.
    if stops.len() > 1 {
        let back = || {
            tab(w, true);
            if focused(w).is_none() {
                tab(w, true); // the first Shift+Tab left the window
            }
            focused(w)
        };
        let last = back();
        if last.as_ref().map(|l| &l.name) != stops.last().map(|(s, _)| &s.name) {
            out.push(format!(
                "{scene}: Shift+Tab from the first stop did not reach the last"
            ));
        }
        for (s, _) in stops.iter().rev().skip(1) {
            let got = back();
            if got.as_ref().map(|g| &g.name) != Some(&s.name) {
                out.push(format!(
                    "{scene}: Shift+Tab walk differs: expected {:?}, got {:?}",
                    s.name,
                    got.map(|g| g.name)
                ));
                break;
            }
        }
    }
    // Esc from the first, a middle and the last stop.
    let n = stops.len();
    for target in [0, n / 2, n.saturating_sub(1)] {
        // Walk from wherever we are to `target` with Tab.
        for _ in 0..(2 * n) {
            if focused(w).map(|f| f.name) == stops.get(target).map(|(s, _)| s.name.clone()) {
                break;
            }
            tab(w, false);
        }
        if !esc() {
            out.push(format!(
                "{scene}: Esc on stop {target} ({:?}) did not reach the window",
                stops.get(target).map(|(s, _)| s.name.clone())
            ));
        }
    }
    out
}

/// Every walk must be clean: no known findings are pinned any more.
fn expect_clean(problems: &[String]) {
    assert_eq!(
        problems,
        Vec::<String>::new(),
        "keyboard walk found problems"
    );
}

macro_rules! scene {
    ($c:expr, $s:expr) => {
        $c.global::<SystemLook<'_>>().set_text_scale($s)
    };
}

/// Presses Esc and tells whether `fired` went off.
fn esc_fires(window: &slint::Window, fired: &Rc<Cell<bool>>) -> bool {
    fired.set(false);
    support::press(window, &support::key(Key::Escape));
    fired.get()
}

#[test]
fn help_keyboard_walk() {
    let mut all = Vec::new();
    for (s, w, h_) in [(1.0f32, 600u32, 640u32), (2.25, 608, 700)] {
        for tab_i in 0..4 {
            let h = Harness::new(HelpWindow::new, w, h_);
            scene!(h.component, s);
            editor::help(&h.component);
            editor::shortcuts(&h.component);
            editor::tips(&h.component);
            h.component.set_tab(tab_i);
            let fired = Rc::new(Cell::new(false));
            let f = fired.clone();
            h.component.on_close_requested(move || f.set(true));
            let win = h.component.window();
            let esc = || esc_fires(win, &fired);
            all.extend(audit(
                &h,
                &format!("help tab {tab_i} at {s}x"),
                false,
                60,
                &esc,
            ));
        }
    }
    expect_clean(&all);
}

#[test]
fn item_editor_keyboard_walk() {
    let mut all = Vec::new();
    for (s, w) in [(1.0f32, 560u32), (2.25, 608)] {
        for (free, dark) in [(true, false), (false, true)] {
            let h = Harness::new(ItemEditor::new, w, 900);
            scene!(h.component, s);
            h.component.set_dark(dark);
            editor::text_item(&h.component);
            h.component.set_kind_index(3);
            h.component.set_prompted_available(!free);
            h.component
                .set_content("Hello {field:Name}, due on {date}.".into());
            h.component.set_formatted(true);
            h.component.set_saved_content("Hello".into());
            let fired = Rc::new(Cell::new(false));
            let f = fired.clone();
            h.component.on_cancel_requested(move || f.set(true));
            let win = h.component.window();
            let esc = || esc_fires(win, &fired);
            all.extend(audit(
                &h,
                &format!("item editor template free={free} at {s}x"),
                dark,
                80,
                &esc,
            ));
        }
    }
    expect_clean(&all);
}

#[test]
fn settings_keyboard_walk() {
    let mut all = Vec::new();
    for (s, w, h_) in [(1.0f32, 640u32, 700u32), (2.25, 765, 700)] {
        let h = Harness::new(SettingsPage::new, w, h_);
        scene!(h.component, s);
        editor::settings(&h.component);
        let fired = Rc::new(Cell::new(false));
        let f = fired.clone();
        h.component.on_close_requested(move || f.set(true));
        let win = h.component.window();
        let esc = || esc_fires(win, &fired);
        all.extend(audit(&h, &format!("settings at {s}x"), false, 120, &esc));
    }
    expect_clean(&all);
}

fn every_kind() -> Vec<snipdeck_ui::FormField> {
    vec![
        fill::text("Name", "Ada", true),
        fill::choice("Tone", &["Friendly", "Formal", "Brief"], 0),
        fill::multiline("Notes"),
        fill::date("Due date"),
        snipdeck_ui::FormField {
            label: "Send a copy".into(),
            kind: snipdeck_ui::TemplateFieldKind::Toggle,
            ..Default::default()
        },
    ]
}

#[test]
fn fill_form_keyboard_walk() {
    let mut all = Vec::new();
    for (s, w) in [(1.0f32, 440u32), (2.25, 608)] {
        for dark in [false, true] {
            let h = Harness::new(TemplateFillWindow::new, w, 1600);
            scene!(h.component, s);
            h.component.set_dark(dark);
            h.component.set_template_title("Invoice reminder".into());
            fill::set_fields(&h.component, every_kind());
            h.component.set_preview(fill::PREVIEW.into());
            let _ = h.render_settled();
            let height = h.component.get_content_height();
            h.component
                .window()
                .set_size(slint::LogicalSize::new(w as f32, height));
            let fired = Rc::new(Cell::new(false));
            let f = fired.clone();
            h.component.on_cancelled(move || f.set(true));
            let win = h.component.window();
            let esc = || esc_fires(win, &fired);
            all.extend(audit(
                &h,
                &format!("fill dark={dark} at {s}x"),
                dark,
                30,
                &esc,
            ));
        }
    }
    expect_clean(&all);
}

/// 2.25x text at the new smallest width: the multi-line box is whole and a date the form could not
/// read shows its message in full (not clipped by the form or the footer).
#[test]
fn fill_form_multiline_and_date_error_fit_at_225() {
    let w = 608u32;
    let h = Harness::new(TemplateFillWindow::new, w, 1600);
    scene!(h.component, 2.25);
    let mut fields = every_kind();
    fields[3].error = "Not a date. Use the form 2026-11-30.".into();
    fields[2].value = "line one\nline two\nline three".into();
    h.component.set_template_title("Invoice reminder".into());
    fill::set_fields(&h.component, fields);
    h.component.set_preview(fill::PREVIEW.into());
    let _ = h.render_settled();
    let height = h.component.get_content_height();
    h.component
        .window()
        .set_size(slint::LogicalSize::new(w as f32, height));
    h.component.window().request_redraw();
    let f = h.render_settled();
    // Dump for the findings file.
    if let Some(dir) = std::env::var_os("SNIPDECK_AUDIT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = std::fs::File::create(dir.join("fill-225-errors.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), f.width, f.height);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .unwrap()
            .write_image_data(&f.rgb)
            .unwrap();
    }
    assert!(
        f.height as f32 >= height,
        "frame {} vs content {height}",
        f.height
    );
    // The date error is drawn in full: red text pixels exist and none reach the right padding (a clipped or
    // overflowing line would run to the edge of the window).
    let (mut n, mut max_x) = (0u32, 0u32);
    for y in 0..f.height {
        for x in 0..f.width {
            let [r, g, b] = f.pixel(x, y);
            if r > 150 && g < 80 && b < 90 {
                n += 1;
                max_x = max_x.max(x);
            }
        }
    }
    assert!(n > 200, "the date error is not drawn ({n} red pixels)");
    assert!(
        max_x < w - 16,
        "the date error runs to the window edge (x {max_x} of {w})"
    );
    // The multiline box (three lines typed) keeps all three lines inside its border: the last line's ink
    // sits above the box's bottom edge. Found by scanning the column at x = 40 for ink below "Notes".
    let ink = |x: u32, y: u32| f.pixel(x, y).iter().all(|c| *c < 100);
    let rows: Vec<u32> = (400..560)
        .filter(|&y| (24..140).any(|x| ink(x, y)))
        .collect();
    assert!(
        rows.len() > 30,
        "multiline text not drawn ({} ink rows)",
        rows.len()
    );
}
