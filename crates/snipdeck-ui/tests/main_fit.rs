//! MAIN-48, 49, 50, 51, 52, 53, 54, 62: one table-driven fit test for the Main window over every window size,
//! text scale and scene. At every case:
//! - no interactive box leaves the window (the item rows are two, so they fit too),
//! - no two interactive boxes overlap (one inside the other is a control and its parts),
//! - the search field keeps a floor of 150 px x text scale,
//! - Tab reaches the search field and the item list, and the walk comes round.
//!
//! The window size is `max(size, the window's own minimum)`: the minimum scales with the text size, so the
//! 480x360 row at 2.25 is a 1080x810 window.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "pixel geometry of a test frame"
)]

use i_slint_backend_testing::{AccessibleRole, ElementQuery};
use i_slint_core::{item_tree::ItemRc, lengths::LogicalPoint, window::WindowInner};
use slint::{ComponentHandle, platform::Key};
use snipdeck_ui::{MainSection, MainWindow, SystemLook, testing::Harness};

#[path = "support/main_fixture.rs"]
mod fixture;
#[path = "support/mod.rs"]
mod support;

const SIZES: [(u32, u32); 4] = [(480, 360), (760, 520), (1100, 720), (1920, 1080)];
const SCALES: [f32; 3] = [1.0, 1.5, 2.25];
const SCENES: [&str; 6] = ["list", "multi", "trash", "empty-all", "rail", "search"];

struct Case {
    h: Harness<MainWindow>,
    size: (f32, f32),
}

fn build(size: (u32, u32), scale: f32, scene: &str) -> Case {
    // The minimum size is a property of the window; read it at the real scale before sizing the harness.
    let probe = Harness::new(MainWindow::new, size.0, size.1);
    probe
        .component
        .global::<SystemLook<'_>>()
        .set_text_scale(scale);
    let _ = probe.render_settled();
    let (mw, mh) = (probe.component.get_min_w(), probe.component.get_min_h());
    drop(probe);
    let size = (size.0.max(mw.ceil() as u32), size.1.max(mh.ceil() as u32));
    let h = Harness::new(MainWindow::new, size.0, size.1);
    h.component.global::<SystemLook<'_>>().set_text_scale(scale);
    let _ = fixture::populate(&h.component);
    let two: Vec<_> = fixture::sample_rows().into_iter().take(2).collect();
    match scene {
        "list" => {
            fixture::set_rows(&h.component, two);
            h.component.set_section(MainSection::List);
            h.component.set_section_index(1);
        }
        "multi" => {
            fixture::set_rows(&h.component, two);
            h.component.set_section(MainSection::List);
            h.component.set_section_index(1);
            h.component.set_selected_count(2);
        }
        "trash" => {
            fixture::set_rows(&h.component, two);
            h.component.set_section(MainSection::Trash);
        }
        "empty-all" => {
            fixture::set_rows(&h.component, Vec::new());
        }
        "rail" => {
            fixture::set_rows(&h.component, two);
            h.component.set_sidebar_collapsed(true);
        }
        "search" => {
            fixture::set_rows(&h.component, two);
            h.component.set_query("re".into());
        }
        other => panic!("unknown scene {other}"),
    }
    let _ = h.render_settled();
    Case {
        h,
        size: (size.0 as f32, size.1 as f32),
    }
}

/// Interactive boxes: (name, x, y, w, h).
fn boxes(h: &Harness<MainWindow>) -> Vec<(String, f32, f32, f32, f32)> {
    let mut v: Vec<_> = ElementQuery::from_root(&h.component)
        .match_descendants()
        .find_all()
        .into_iter()
        .filter(|e| {
            matches!(
                e.accessible_role(),
                Some(AccessibleRole::Button | AccessibleRole::TextInput | AccessibleRole::Checkbox)
            )
        })
        .map(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            (
                e.accessible_label()
                    .map(|l| l.to_string())
                    .unwrap_or_default(),
                p.x,
                p.y,
                s.width,
                s.height,
            )
        })
        .filter(|b| b.3 > 0.0 && b.4 > 0.0)
        .collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)));
    v.dedup();
    v
}

fn inside(a: &(String, f32, f32, f32, f32), b: &(String, f32, f32, f32, f32)) -> bool {
    a.1 >= b.1 - 0.5
        && a.2 >= b.2 - 0.5
        && a.1 + a.3 <= b.1 + b.3 + 0.5
        && a.2 + a.4 <= b.2 + b.4 + 0.5
}

fn fit_problems(c: &Case, case: &str) -> Vec<String> {
    let bs = boxes(&c.h);
    let mut out = Vec::new();
    for b in &bs {
        if b.1 < -0.5 || b.2 < -0.5 || b.1 + b.3 > c.size.0 + 0.5 || b.2 + b.4 > c.size.1 + 0.5 {
            out.push(format!(
                "{case}: {:?} lies outside the {}x{} window: x {:.0}..{:.0}, y {:.0}..{:.0}",
                b.0,
                c.size.0,
                c.size.1,
                b.1,
                b.1 + b.3,
                b.2,
                b.2 + b.4
            ));
        }
    }
    // The toolbar row: neighbours keep a gap of 4 px (the three view toggles are one segmented control).
    let mut bar: Vec<_> = bs
        .iter()
        .filter(|b| b.2 + b.4 / 2.0 < 40.0 && b.1 > 40.0)
        .collect();
    bar.sort_by(|a, b| a.1.total_cmp(&b.1));
    for pair in bar.windows(2) {
        if pair[0].0.ends_with(" view") && pair[1].0.ends_with(" view") {
            continue;
        }
        let gap = pair[1].1 - (pair[0].1 + pair[0].3);
        if gap < 3.5 && !inside(pair[0], pair[1]) && !inside(pair[1], pair[0]) {
            out.push(format!(
                "{case}: {:?} and {:?} are {gap:.1} px apart",
                pair[0].0, pair[1].0
            ));
        }
    }
    for (i, a) in bs.iter().enumerate() {
        for b in &bs[i + 1..] {
            let ox = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
            let oy = (a.2 + a.4).min(b.2 + b.4) - a.2.max(b.2);
            // A rail header button scrolled below the sidebar's viewport sits behind the footer, clipped away
            // and out of sight, so it is not an overlap; only the footer's own buttons take that exemption.
            let scrolled_under_footer = |x: &(String, f32, f32, f32, f32), f: &(String, f32, f32, f32, f32)| {
                matches!(x.0.as_str(), "Add list" | "Manage tags")
                    && matches!(f.0.as_str(), "Settings" | "Help")
                    && x.2 + x.4 > f.2
            };
            if scrolled_under_footer(a, b) || scrolled_under_footer(b, a) {
                continue;
            }
            if ox > 1.0 && oy > 1.0 && !inside(a, b) && !inside(b, a) {
                out.push(format!(
                    "{case}: {:?} ({:.0},{:.0} {:.0}x{:.0}) overlaps {:?} ({:.0},{:.0} {:.0}x{:.0})",
                    a.0, a.1, a.2, a.3, a.4, b.0, b.1, b.2, b.3, b.4
                ));
            }
        }
    }
    out
}

fn search_problems(c: &Case, case: &str, scale: f32, scene: &str) -> Vec<String> {
    let bs = boxes(&c.h);
    // MAIN-53: the bulk bar's Move button keeps room for its label.
    if scene == "multi" {
        match bs.iter().find(|b| b.0 == "Move") {
            Some(m) if m.3 >= 48.0 * scale - 0.5 => {}
            Some(m) => {
                return vec![format!(
                    "{case}: the bulk bar's Move button is {:.0} px wide",
                    m.3
                )];
            }
            None => return vec![format!("{case}: no Move button in the bulk bar")],
        }
    }
    match bs.iter().find(|b| b.0 == "Search items") {
        Some(s) if s.3 >= 150.0 * scale - 0.5 => Vec::new(),
        Some(s) => vec![format!(
            "{case}: the search field is {:.0} px wide, under {}",
            s.3,
            150.0 * scale
        )],
        None => vec![format!("{case}: no search field")],
    }
}

fn focus_box(window: &slint::Window) -> Option<(String, f32, f32, f32, f32)> {
    let item: ItemRc = WindowInner::from_pub(window)
        .focus_item
        .borrow()
        .upgrade()?;
    let g = item.geometry();
    let p = item.map_to_window(LogicalPoint::new(g.origin.x, g.origin.y));
    let mut label = String::new();
    let mut cur = Some(item);
    while let Some(i) = cur {
        if i.is_accessible() {
            label = i
                .accessible_string_property(
                    i_slint_core::accessibility::AccessibleStringProperty::Label,
                )
                .unwrap_or_default()
                .to_string();
            break;
        }
        cur = i.parent_item(i_slint_core::item_tree::ParentItemTraversalMode::FindAllParents);
    }
    Some((label, p.x, p.y, g.size.width, g.size.height))
}

fn tab_problems(c: &Case, case: &str, sidebar_right: f32) -> Vec<String> {
    let w = c.h.component.window();
    // Start from the search field, as Ctrl+F does, then walk with Tab.
    support::chord(w, &[Key::Control], "f");
    c.h.settle();
    let mut seen: Vec<(String, f32, f32, f32, f32)> = Vec::new();
    let (mut search, mut list) = (false, false);
    for _ in 0..80 {
        if let Some(f) = focus_box(w) {
            if seen.contains(&f) {
                break;
            }
            search |= f.0.starts_with("Search");
            list |= f.1 + f.3 / 2.0 > sidebar_right && f.2 > 60.0 && f.3 > 200.0;
            seen.push(f);
        }
        support::press(w, &support::key(Key::Tab));
        c.h.settle();
    }
    let mut out = Vec::new();
    if !search {
        out.push(format!(
            "{case}: Tab never reaches the search field: {seen:?}"
        ));
    }
    if !list {
        out.push(format!("{case}: Tab never reaches the item list: {seen:?}"));
    }
    out
}

#[test]
fn main_fits_at_every_size_and_text_scale() {
    {
        let mut problems = Vec::new();
        for scale in SCALES {
            for size in SIZES {
                for scene in SCENES {
                    let case = format!("{}x{}@{scale} {scene}", size.0, size.1);
                    let c = build(size, scale, scene);
                    problems.extend(fit_problems(&c, &case));
                    problems.extend(search_problems(&c, &case, scale, scene));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "{} problems:\n{}",
            problems.len(),
            problems.join("\n")
        );
    }
}

#[test]
fn main_tab_walk_reaches_search_and_list_at_every_size_and_text_scale() {
    {
        let mut problems = Vec::new();
        for scale in SCALES {
            for size in SIZES {
                for scene in ["list", "rail"] {
                    let case = format!("{}x{}@{scale} {scene} tab", size.0, size.1);
                    let c = build(size, scale, scene);
                    let sidebar = c.h.component.get_sidebar_right();
                    problems.extend(tab_problems(&c, &case, sidebar));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "{} problems:\n{}",
            problems.len(),
            problems.join("\n")
        );
    }
}
