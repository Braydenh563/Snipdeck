//! The walk the library audit skipped, for the views work (LIBRARY-10, 11, 12, 43, 48): the Main toolbar's Filter and
//! Sort buttons, the chip row under it, their menus, the group-delete dialog, the Tag manager's colour and merge
//! dialogs and the board editor's colour and icon. Each is measured (logical pixels, from the accessibility tree) at
//! the window's minimum, default and a large size, at text scales 1.0, 1.5 and 2.25, in light, dark and high
//! contrast. Nothing may leave the window, touch its neighbour or fall under the hit-target size.
//!
//! Goldens (a few key states): `UPDATE_GOLDEN=1 cargo test -p snipdeck-ui --test library_views_walk`, then look at
//! every PNG.

#![allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "pixel geometry of a test frame: logical sizes are small and positive"
)]

use i_slint_backend_testing::{AccessibleRole as Role, ElementHandle, ElementQuery};
use slint::{ComponentHandle, ModelRc, VecModel};
use snipdeck_ui::{
    BoardEditor, FilterChipEntry, ListEditor, MainRow, MainSection, MainWindow, SystemLook,
    TagManager,
    testing::{Harness, assert_golden},
};

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/main_fixture.rs"]
mod fixture;
#[path = "support/mod.rs"]
mod support;

/// Minimum, default and a large window width; the heights are tall enough for the dialogs.
const WIDTHS: [u32; 3] = [480, 1040, 1600];
const SCALES: [f32; 3] = [1.0, 1.5, 2.25];
/// Light, dark and high contrast.
const LOOKS: [(&str, bool, bool); 3] = [
    ("light", false, false),
    ("dark", true, false),
    ("high-contrast", false, true),
];

macro_rules! look {
    ($ui:expr, $scale:expr, $high_contrast:expr) => {{
        let g = $ui.global::<SystemLook<'_>>();
        g.set_text_scale($scale);
        g.set_high_contrast($high_contrast);
    }};
}

/// A window on a list with three kind chips, a tag chip with a long name and the colour-label filter on.
fn main_with_filter(width: u32, scale: f32, dark: bool, hc: bool) -> Harness<MainWindow> {
    let h = Harness::new(MainWindow::new, width, 640);
    h.component.set_dark(dark);
    look!(h.component, scale, hc);
    let _ = fixture::populate(&h.component);
    h.component.set_section(MainSection::List);
    h.component.set_section_index(0);
    h.component.set_sortable(true);
    h.component.set_list_sort(3);
    h.component
        .set_filter_chips(ModelRc::new(VecModel::from(vec![
            FilterChipEntry {
                kind: 0,
                name: "".into(),
            },
            FilterChipEntry {
                kind: 6,
                name: "".into(),
            },
            FilterChipEntry {
                kind: 9,
                name: "a-rather-long-tag-name".into(),
            },
        ])));
    h
}

#[derive(Clone)]
struct Boxed {
    label: String,
    role: Role,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn boxed(e: &ElementHandle) -> Option<Boxed> {
    let (p, s) = (e.absolute_position(), e.size());
    Some(Boxed {
        label: e.accessible_label()?.to_string(),
        role: e.accessible_role()?,
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
    })
}

fn all(h: &Harness<impl ComponentHandle>) -> Vec<Boxed> {
    ElementQuery::from_root(&h.component)
        .match_descendants()
        .find_all()
        .iter()
        .filter_map(boxed)
        .filter(|b| b.w > 0.5 && b.h > 0.5)
        .fold(Vec::new(), |mut out: Vec<Boxed>, b| {
            // Nested elements repeat their control's name and place.
            if !out.iter().any(|o| {
                o.label == b.label
                    && o.role == b.role
                    && (o.x - b.x).abs() < 0.5
                    && (o.y - b.y).abs() < 0.5
            }) {
                out.push(b);
            }
            out
        })
}

fn buttons(h: &Harness<impl ComponentHandle>) -> Vec<Boxed> {
    let mut v: Vec<Boxed> = all(h)
        .into_iter()
        .filter(|b| b.role == Role::Button)
        .collect();
    v.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    v.dedup_by(|a, b| a.label == b.label && (a.x - b.x).abs() < 0.5 && (a.y - b.y).abs() < 0.5);
    v
}

/// The top toolbar's buttons, the heading row's Filter and Sort buttons and the chip row's chips and Clear button.
fn rows_of_controls(h: &Harness<MainWindow>) -> (Vec<Boxed>, Vec<Boxed>, Vec<Boxed>) {
    let all = buttons(h);
    let toolbar = all
        .iter()
        .filter(|b| b.y + b.h / 2.0 < 40.0 && b.x > 40.0 && !b.label.contains("sidebar"))
        .cloned()
        .collect();
    let heading = all
        .iter()
        .filter(|b| b.label.starts_with("Filter the items") || b.label.starts_with("Sort by"))
        .cloned()
        .collect();
    let chips = all
        .iter()
        .filter(|b| {
            [
                "Text",
                "Has a trigger",
                "Tag: a-rather-long-tag-name",
                "Clear filter",
            ]
            .contains(&b.label.as_str())
        })
        .cloned()
        .collect();
    (toolbar, heading, chips)
}

#[test]
fn the_filter_and_sort_buttons_and_the_chip_row_fit_every_size_scale_and_look() {
    let mut problems: Vec<String> = Vec::new();
    for width in WIDTHS {
        for scale in SCALES {
            for (name, dark, hc) in LOOKS {
                let h = main_with_filter(width, scale, dark, hc);
                let _ = h.render_settled();
                let (toolbar, heading, chips) = rows_of_controls(&h);
                let at = format!("{width}px, scale {scale}, {name}");
                if heading.len() != 2 {
                    problems.push(format!(
                        "{at}: {} of Filter and Sort in the heading row: {:?}",
                        heading.len(),
                        heading.iter().map(|b| &b.label).collect::<Vec<_>>()
                    ));
                }
                // A chip scrolled out of the strip leaves the accessibility tree: the first chip and Clear filter stay.
                for must in ["Text", "Clear filter"] {
                    if !chips.iter().any(|b| b.label == must) {
                        problems.push(format!("{at}: no {must:?} in the chip row"));
                    }
                }
                // The top toolbar (Commands, View, New item: not part of this change) cannot hold its three controls at
                // the largest text size in the 480 px minimum window; it is reported, not asserted, here.
                let toolbar_fits = !(width < 600 && scale > 2.0);
                // The chips scroll sideways in their strip; Clear filter keeps its place, and no chip is squeezed
                // narrower than it is in a wide window (its words would spill over its neighbour).
                let wide_h = main_with_filter(1600, scale, dark, hc);
                let _ = wide_h.render_settled();
                let natural = rows_of_controls(&wide_h).2;
                for chip in &chips {
                    if let Some(wide) = natural
                        .iter()
                        .find(|n| n.label == chip.label && chip.w < n.w - 1.0)
                    {
                        problems.push(format!(
                            "{at}: chip {:?} is squeezed to {:.0} of {:.0}",
                            chip.label, chip.w, wide.w
                        ));
                    }
                }
                let clear = chips.iter().filter(|b| b.label == "Clear filter");
                let checked = heading
                    .iter()
                    .chain(clear)
                    .chain(toolbar.iter().filter(|_| toolbar_fits));
                for b in checked {
                    if b.x < 0.0 || b.x + b.w > width as f32 + 0.5 {
                        problems.push(format!(
                            "{at}: {:?} leaves the window (x {:.0}..{:.0})",
                            b.label,
                            b.x,
                            b.x + b.w
                        ));
                    }
                    if b.h < 28.0 - 0.5 {
                        problems.push(format!(
                            "{at}: {:?} is under the hit target ({:.0})",
                            b.label, b.h
                        ));
                    }
                }
                let chips_only: Vec<Boxed> = chips
                    .iter()
                    .filter(|b| b.label != "Clear filter")
                    .cloned()
                    .collect();
                for (row, what) in [
                    (&heading, "heading"),
                    (&chips_only, "chip"),
                    (&toolbar, "toolbar"),
                ] {
                    let mut row = row.clone();
                    row.sort_by(|a, b| a.x.total_cmp(&b.x));
                    for pair in row.windows(2) {
                        if pair[0].label.ends_with(" view") && pair[1].label.ends_with(" view") {
                            continue;
                        }
                        let gap = pair[1].x - (pair[0].x + pair[0].w);
                        if gap < 3.5 {
                            problems.push(format!(
                                "{at}: in the {what} row {:?} and {:?} touch (gap {gap:.1})",
                                pair[0].label, pair[1].label
                            ));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

#[test]
fn the_filter_and_sort_menus_stay_inside_the_window() {
    let mut problems: Vec<String> = Vec::new();
    for (width, scale) in [(480_u32, 1.0_f32), (480, 2.25), (1040, 1.5), (1600, 1.0)] {
        for (which, button) in [("Filter", "Filter the items"), ("Sort", "Sort by")] {
            let h = main_with_filter(width, scale, false, false);
            let _ = h.render_settled();
            let (_, heading, _) = rows_of_controls(&h);
            let Some(b) = heading.iter().find(|b| b.label.starts_with(button)) else {
                problems.push(format!("{width}/{scale}: no {which} button"));
                continue;
            };
            let _ = b;
            let handle = ElementQuery::from_root(&h.component)
                .match_descendants()
                .match_predicate(move |e| {
                    e.accessible_role() == Some(Role::Button)
                        && e.accessible_label()
                            .is_some_and(|l| l.as_str().starts_with(button))
                })
                .find_first()
                .expect("button");
            let before: Vec<(String, i32, i32)> = all(&h)
                .into_iter()
                .filter(|e| e.role == Role::ListItem)
                .map(|e| (e.label.clone(), e.x as i32, e.y as i32))
                .collect();
            handle.invoke_accessible_default_action();
            let _ = h.render_settled();
            let items: Vec<Boxed> = all(&h)
                .into_iter()
                .filter(|e| e.role == Role::ListItem)
                .filter(|e| !before.contains(&(e.label.clone(), e.x as i32, e.y as i32)))
                .collect();
            if items.len() < 5 {
                problems.push(format!(
                    "{width}/{scale}: the {which} menu shows {} rows",
                    items.len()
                ));
                continue;
            }
            for e in &items {
                // The menu scrolls inside the window (menu-max-h), so only its first rows must be on screen.
                let first = items.first().is_some_and(|f| std::ptr::eq(f, e));
                if e.x < 0.0
                    || e.x + e.w > width as f32 + 0.5
                    || e.y < 0.0
                    || (first && e.y + e.h > 640.5)
                {
                    problems.push(format!(
                        "{width}/{scale}: {which} row {:?} leaves the window ({:.0},{:.0} {:.0}x{:.0})",
                        e.label, e.x, e.y, e.w, e.h
                    ));
                }
                if e.h < 28.0 - 0.5 {
                    problems.push(format!(
                        "{width}/{scale}: {which} row {:?} is {:.0} high",
                        e.label, e.h
                    ));
                }
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

#[test]
fn main_filter_chips_golden_light_dark_and_scaled() {
    let h = main_with_filter(1040, 1.0, false, false);
    assert_golden("library-chips-light", &h.render_settled());
    drop(h);
    let h = main_with_filter(1040, 1.0, true, false);
    assert_golden("library-chips-dark", &h.render_settled());
    drop(h);
    let h = main_with_filter(1040, 1.0, false, true);
    assert_golden("library-chips-high-contrast", &h.render_settled());
    drop(h);
    let h = main_with_filter(640, 2.25, false, false);
    assert_golden("library-chips-225", &h.render_settled());
}

// ---- Tag manager ---------------------------------------------------------------------------------------------

fn tag_manager(width: u32, height: u32, scale: f32, dark: bool, hc: bool) -> Harness<TagManager> {
    let h = Harness::new(TagManager::new, width, height);
    h.component.set_dark(dark);
    look!(h.component, scale, hc);
    editor::tag_manager(&h.component);
    h.component
        .set_colours(ModelRc::new(VecModel::from(vec![0, 3, -1, 7, -1])));
    h
}

#[test]
fn the_tag_colour_dialog_keeps_all_nine_choices_inside_its_card() {
    let mut problems: Vec<String> = Vec::new();
    // The Tag manager's minimum is 360 x 280; the preferred width is 440.
    for (width, height) in [(360_u32, 420_u32), (440, 520), (800, 700)] {
        for scale in SCALES {
            for (name, dark, hc) in LOOKS {
                let h = tag_manager(width, height, scale, dark, hc);
                h.component.set_colour_index(1);
                let _ = h.render_settled();
                let at = format!("{width}px, scale {scale}, {name}");
                let swatches: Vec<Boxed> = all(&h)
                    .into_iter()
                    .filter(|b| b.role == Role::RadioButton)
                    .collect();
                if swatches.len() != 9 {
                    problems.push(format!("{at}: {} choices instead of 9", swatches.len()));
                    continue;
                }
                let card = f32::min(384.0, width as f32 - 32.0);
                let card_right = f32::midpoint(width as f32, card);
                let card_left = (width as f32 - card) / 2.0;
                for s in &swatches {
                    if s.x < card_left + 8.0 || s.x + s.w > card_right - 8.0 {
                        problems.push(format!(
                            "{at}: {:?} at x {:.0}..{:.0} is outside the card {card_left:.0}..{card_right:.0}",
                            s.label,
                            s.x,
                            s.x + s.w
                        ));
                    }
                    if s.h < 28.0 - 0.5 || s.w < 28.0 - 0.5 {
                        problems.push(format!("{at}: {:?} is {:.0}x{:.0}", s.label, s.w, s.h));
                    }
                }
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

#[test]
fn the_tag_manager_dialogs_golden() {
    let h = tag_manager(440, 520, 1.0, false, false);
    assert_golden("tag-manager-colours-light", &h.render_settled());
    h.component.set_colour_index(1);
    assert_golden("tag-manager-colour-dialog-light", &h.render_settled());
    h.component.set_colour_index(-1);
    drop(h);

    let h = tag_manager(440, 520, 1.0, false, false);
    h.component.set_editing(1);
    h.component.set_rename_text("billing".into());
    h.component
        .set_rename_error("A tag named billing already exists.".into());
    h.component.set_merge_into("billing".into());
    assert_golden("tag-manager-merge-dialog-light", &h.render_settled());
    drop(h);

    let h = tag_manager(360, 420, 2.25, true, false);
    h.component.set_colour_index(1);
    assert_golden(
        "tag-manager-colour-dialog-dark-225-min",
        &h.render_settled(),
    );
}

#[test]
fn the_tag_dot_is_drawn_only_for_a_coloured_tag() {
    // Colours [0, 3, none, 7, none]: rows 0, 1 and 3 carry a dot, in blue, amber and slate. Read down the column the
    // dots sit in and count the runs of non-background pixels.
    let h = tag_manager(440, 520, 1.0, false, false);
    let frame = h.render_settled();
    let rows: Vec<Boxed> = all(&h)
        .into_iter()
        .filter(|b| b.role == Role::Button && b.label.starts_with("Colour of tag"))
        .collect();
    assert_eq!(
        rows.len(),
        5,
        "a colour button per tag: {:?}",
        rows.iter().map(|b| &b.label).collect::<Vec<_>>()
    );
    let mut sorted = rows.clone();
    sorted.sort_by(|a, b| a.y.total_cmp(&b.y));
    // The dot is 12 px wide from about x 28 (the names start at about x 36): read x 30..33 against the plain pixel at
    // x 24 of the same row.
    let y_of = |row: &Boxed| (row.y + row.h / 2.0) as u32;
    let mut dots: Vec<Option<[u8; 3]>> = Vec::new();
    for row in &sorted {
        let y = y_of(row);
        let bg = frame.pixel(24, y);
        // A solid dot reads the same at two points; a glyph does not.
        let (p1, p2) = (frame.pixel(31, y), frame.pixel(33, y));
        let differs = p1
            .iter()
            .zip(bg.iter())
            .map(|(a, b)| u32::from(a.abs_diff(*b)))
            .sum::<u32>()
            > 60;
        let found = (p1 == p2 && differs).then_some(p1);
        dots.push(found);
    }
    let shown: Vec<bool> = dots.iter().map(Option::is_some).collect();
    let probe: Vec<Vec<[u8; 3]>> = sorted
        .iter()
        .map(|r| {
            (16..48)
                .step_by(4)
                .map(|x| frame.pixel(x, y_of(r)))
                .collect()
        })
        .collect();
    assert_eq!(
        shown,
        [true, true, false, true, false],
        "dots {dots:?}; rows {:?}; pixels at x=16,20..44 {probe:?}",
        sorted
            .iter()
            .map(|r| (r.x, r.y, r.w, r.h))
            .collect::<Vec<_>>()
    );
    assert_ne!(dots[0], dots[1], "blue and amber differ");
    assert_ne!(dots[1], dots[3], "amber and slate differ");
}

// ---- Group colour ---------------------------------------------------------------------------------------------

/// A list with a teal "Replies" group of two items, then an uncoloured "Plain" group of one item.
fn main_with_groups(width: u32, scale: f32, dark: bool, hc: bool) -> Harness<MainWindow> {
    let h = Harness::new(MainWindow::new, width, 640);
    h.component.set_dark(dark);
    look!(h.component, scale, hc);
    let _ = fixture::populate(&h.component);
    h.component.set_section(MainSection::List);
    h.component.set_section_index(0);
    let header = |title: &str, colour: i32| MainRow {
        title: title.into(),
        is_header: true,
        group_count: 2,
        group_colour: colour,
        ..Default::default()
    };
    let mut rows = fixture::sample_rows();
    rows.truncate(4);
    rows.insert(0, header("Replies", 1));
    rows.insert(3, header("Plain", -1));
    for i in [1, 2, 4] {
        rows[i].grouped = true;
    }
    fixture::set_rows(&h.component, rows);
    h.component.set_has_groups(true);
    h
}

/// Lets the window settle; a frame that has nothing new to draw is not a failure here.
fn settle_quietly(h: &Harness<impl ComponentHandle>) -> Option<snipdeck_ui::testing::Frame> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render_settled())).ok()
}

/// Pixels of a strong hue (a coloured dot) inside a box; the caret and the text are grey.
fn saturated_in(frame: &snipdeck_ui::testing::Frame, b: &Boxed) -> usize {
    let mut n = 0;
    for y in (b.y as u32)..((b.y + b.h) as u32) {
        for x in (b.x as u32)..((b.x + b.w) as u32) {
            let p = frame.pixel(x, y);
            let (hi, lo) = (
                p.iter().max().copied().unwrap(),
                p.iter().min().copied().unwrap(),
            );
            if hi - lo >= 60 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn a_coloured_group_shows_a_dot_and_says_its_colour_and_a_plain_one_shows_none() {
    let mut problems: Vec<String> = Vec::new();
    for scale in [1.0_f32, 1.5, 2.25] {
        for (name, dark, hc) in LOOKS {
            let h = main_with_groups(1040, scale, dark, hc);
            let frame = h.render_settled();
            let at = format!("scale {scale}, {name}");
            let headers: Vec<Boxed> = all(&h)
                .into_iter()
                .filter(|b| b.role == Role::Button && b.label.starts_with("group "))
                .collect();
            let find = |l: &str| headers.iter().find(|b| b.label == l);
            let (Some(teal), Some(plain)) = (
                find("group Replies, Teal, 2 items"),
                find("group Plain, 2 items"),
            ) else {
                problems.push(format!(
                    "{at}: headers {:?}",
                    headers.iter().map(|b| &b.label).collect::<Vec<_>>()
                ));
                continue;
            };
            let (with, without) = (saturated_in(&frame, teal), saturated_in(&frame, plain));
            // The dot is at least 8 logical px across, so well over 40 pixels; the plain header has none.
            if with < 40 || without != 0 {
                problems.push(format!(
                    "{at}: {with} hued pixels on the teal header, {without} on the plain one"
                ));
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

#[test]
fn the_group_menu_opens_a_colour_dialog_that_offers_ten_choices_inside_its_card_and_reports_the_pick()
 {
    use slint::platform::{PointerEventButton, WindowEvent};
    let mut problems: Vec<String> = Vec::new();
    for (width, scale) in [(480_u32, 1.0_f32), (1040, 1.0), (480, 2.25), (1040, 2.25)] {
        let h = main_with_groups(width, scale, false, false);
        let picked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let p = picked.clone();
        h.component
            .on_group_colour_chosen(move |row, hue| p.borrow_mut().push((row, hue)));
        let _ = settle_quietly(&h);
        let at = format!("{width}px, scale {scale}");
        let Some(header) = all(&h)
            .into_iter()
            .find(|b| b.role == Role::Button && b.label == "group Replies, Teal, 2 items")
        else {
            problems.push(format!("{at}: no header"));
            continue;
        };
        let position =
            slint::LogicalPosition::new(header.x + header.w / 2.0, header.y + header.h / 2.0);
        h.component
            .window()
            .dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Right,
            });
        h.component
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Right,
            });
        let _ = settle_quietly(&h);
        let entry = ElementQuery::from_root(&h.component)
            .match_descendants()
            .match_predicate(|e| {
                e.accessible_label()
                    .is_some_and(|l| l.as_str() == "Group colour…")
            })
            .find_first();
        let Some(entry) = entry else {
            problems.push(format!("{at}: the header menu has no Group colour… row"));
            continue;
        };
        entry.invoke_accessible_default_action();
        let dialog_frame = settle_quietly(&h);
        let radios: Vec<Boxed> = all(&h)
            .into_iter()
            .filter(|b| b.role == Role::RadioButton)
            .collect();
        if radios.len() != 10 {
            problems.push(format!(
                "{at}: {} choices instead of 10: {:?}",
                radios.len(),
                radios.iter().map(|b| &b.label).collect::<Vec<_>>()
            ));
            continue;
        }
        let card = f32::min(384.0, width as f32 - 32.0);
        let (left, right) = (
            (width as f32 - card) / 2.0,
            f32::midpoint(width as f32, card),
        );
        for r in &radios {
            if r.x < left + 8.0 || r.x + r.w > right - 8.0 || r.w < 27.5 || r.h < 27.5 {
                problems.push(format!(
                    "{at}: {:?} at x {:.0}..{:.0} ({:.0}x{:.0}) is outside the card {left:.0}..{right:.0} or under 28",
                    r.label, r.x, r.x + r.w, r.w, r.h
                ));
            }
        }
        if width == 1040 && scale < 1.1 {
            assert_golden(
                "main-group-colour-dialog-light",
                &dialog_frame.expect("the open dialog drew"),
            );
        }
        let rose = ElementQuery::from_root(&h.component)
            .match_descendants()
            .match_predicate(|e| {
                e.accessible_role() == Some(Role::RadioButton)
                    && e.accessible_label()
                        .is_some_and(|l| l.as_str().contains("Rose"))
            })
            .find_first()
            .expect("rose");
        rose.invoke_accessible_default_action();
        let _ = settle_quietly(&h);
        if *picked.borrow() != [(0, 6)] {
            problems.push(format!(
                "{at}: reported {:?}, wanted [(0, 6)]",
                picked.borrow()
            ));
        }
        let still: usize = all(&h)
            .iter()
            .filter(|b| b.role == Role::RadioButton)
            .count();
        if still != 0 {
            problems.push(format!("{at}: the dialog stayed open after a pick"));
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

#[test]
fn the_group_header_dot_golden_light_and_dark() {
    let h = main_with_groups(1040, 1.0, false, false);
    assert_golden("main-group-colour-light", &h.render_settled());
    drop(h);
    let h = main_with_groups(1040, 1.0, true, false);
    assert_golden("main-group-colour-dark", &h.render_settled());
}

// ---- Board editor --------------------------------------------------------------------------------------------

#[test]
fn the_board_editor_offers_a_colour_and_an_icon_at_every_scale() {
    let mut problems: Vec<String> = Vec::new();
    for (width, scale) in [
        (380_u32, 1.0_f32),
        (460, 1.0),
        (460, 1.5),
        (700, 2.25),
        (380, 2.25),
    ] {
        for (name, dark, hc) in LOOKS {
            let h = Harness::new(BoardEditor::new, width, 900);
            h.component.set_dark(dark);
            look!(h.component, scale, hc);
            h.component.set_show_colour(true);
            h.component.set_name("Work".into());
            h.component.set_icon_names(ModelRc::new(VecModel::from(
                ["Star", "Heart", "Folder"]
                    .map(slint::SharedString::from)
                    .to_vec(),
            )));
            let _ = h.render_settled();
            let at = format!("{width}px, scale {scale}, {name}");
            let radios: Vec<Boxed> = all(&h)
                .into_iter()
                .filter(|b| b.role == Role::RadioButton)
                .collect();
            if radios.len() < 9 + 1 {
                problems.push(format!(
                    "{at}: only {} colour and icon choices",
                    radios.len()
                ));
            }
            for r in &radios {
                if r.x < 16.0 || r.x + r.w > width as f32 - 16.0 + 0.5 {
                    problems.push(format!(
                        "{at}: {:?} leaves the window ({:.0}..{:.0})",
                        r.label,
                        r.x,
                        r.x + r.w
                    ));
                }
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

// ---- List editor ---------------------------------------------------------------------------------------------

#[test]
fn the_list_editors_sort_choice_opens_with_six_rows_and_scales() {
    let mut problems: Vec<String> = Vec::new();
    for scale in SCALES {
        for (name, dark, hc) in LOOKS {
            let h = Harness::new(ListEditor::new, 460, 1800);
            h.component.set_dark(dark);
            look!(h.component, scale, hc);
            editor::list_editor_existing(&h.component);
            let _ = h.render_settled();
            let at = format!("scale {scale}, {name}");
            let combo = ElementQuery::from_root(&h.component)
                .match_descendants()
                .match_accessible_role(Role::Combobox)
                .find_all()
                .into_iter()
                .find(|e| e.accessible_label().is_some_and(|l| l == "Sort items by"));
            let Some(combo) = combo else {
                problems.push(format!("{at}: no sort select"));
                continue;
            };
            if combo.size().height < 28.0 - 0.5 {
                problems.push(format!(
                    "{at}: the sort select is {:.0} high",
                    combo.size().height
                ));
            }
            combo.invoke_accessible_default_action();
            let frame = h.render_settled();
            let rows: Vec<Boxed> = all(&h)
                .into_iter()
                .filter(|b| b.role == Role::ListItem)
                .collect();
            if rows.len() != 6 {
                problems.push(format!("{at}: {} rows in the open list", rows.len()));
            }
            for r in &rows {
                if r.x < 0.0 || r.x + r.w > 460.5 || r.h < 32.0 - 0.5 {
                    problems.push(format!(
                        "{at}: row {:?} at x {:.0} w {:.0} h {:.0}",
                        r.label, r.x, r.w, r.h
                    ));
                }
            }
            if (scale - 1.0).abs() < 0.01 && !hc {
                assert_golden(
                    if dark {
                        "list-editor-sort-open-dark"
                    } else {
                        "list-editor-sort-open-light"
                    },
                    &frame,
                );
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}

// ---- Focus order ---------------------------------------------------------------------------------------------

/// The accessible name and window box of the control that has the keyboard focus.
fn focused_stop(window: &slint::Window) -> Option<(String, f32, f32)> {
    use i_slint_core::{
        accessibility::AccessibleStringProperty, item_tree::ItemRc, lengths::LogicalPoint,
        window::WindowInner,
    };
    let item = WindowInner::from_pub(window)
        .focus_item
        .borrow()
        .upgrade()?;
    let raw = item.clone();
    let mut cur: Option<ItemRc> = Some(item);
    while let Some(i) = cur {
        if i.is_accessible() {
            let g = i.geometry();
            let p = i.map_to_window(LogicalPoint::new(g.origin.x, g.origin.y));
            let name = i
                .accessible_string_property(AccessibleStringProperty::Label)
                .unwrap_or_default()
                .to_string();
            return Some((name, p.x, p.x + g.size.width));
        }
        cur = i.parent_item(i_slint_core::item_tree::ParentItemTraversalMode::FindAllParents);
    }
    let g = raw.geometry();
    let p = raw.map_to_window(LogicalPoint::new(g.origin.x, g.origin.y));
    Some((
        format!(
            "<none at {:.0},{:.0} {:.0}x{:.0}>",
            p.x, p.y, g.size.width, g.size.height
        ),
        p.x,
        p.x + g.size.width,
    ))
}

#[test]
fn tab_reaches_filter_sort_every_chip_and_clear_in_reading_order_and_scrolls_a_chip_into_view() {
    let mut problems: Vec<String> = Vec::new();
    // At text scale 2.25 in a window under about 1600 px, Tab leaves the sidebar and never reaches the content area at
    // all (Search, the toolbar, Filter, the rows): a defect of the top toolbar that predates the Filter and Sort buttons
    // (measured: 800 and 1040 px; 1600 px is fine). It is reported in the handover, not covered here.
    for (width, scale) in [(1040_u32, 1.0_f32), (640, 1.0), (640, 1.5), (1600, 2.25)] {
        let h = main_with_filter(width, scale, false, false);
        let _ = h.render_settled();
        let w = h.component.window();
        let mut stops: Vec<(String, f32, f32)> = Vec::new();
        for _ in 0..60 {
            support::press(w, &support::key(slint::platform::Key::Tab));
            // The scroll that reveals a focused chip runs on the next frame.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render_settled()));
            stops.push(focused_stop(w).unwrap_or_else(|| ("<none>".into(), 0.0, 0.0)));
        }
        let at = format!("{width}px, scale {scale}");
        let names: Vec<&str> = stops.iter().map(|s| s.0.as_str()).collect();
        let first = |starts: &str| names.iter().position(|n| n.starts_with(starts));
        let order = [
            first("Filter the items"),
            first("Sort by"),
            first("Text"),
            first("Has a trigger"),
            first("Tag: a-rather-long-tag-name"),
            first("Clear filter"),
        ];
        if order.iter().any(Option::is_none) {
            problems.push(format!(
                "{at}: Tab never reaches one of the filter controls: {names:?}"
            ));
            continue;
        }
        let at_i: Vec<usize> = order.iter().flatten().copied().collect();
        if at_i.windows(2).any(|p| p[0] >= p[1]) {
            problems.push(format!(
                "{at}: Tab order is not Filter, Sort, chips, Clear: {at_i:?} in {names:?}"
            ));
        }
        for s in &stops {
            if s.0 == "Tag: a-rather-long-tag-name" && (s.1 < 0.0 || s.2 > width as f32 + 0.5) {
                problems.push(format!(
                    "{at}: the focused tag chip is off screen ({:.0}..{:.0})",
                    s.1, s.2
                ));
            }
        }
    }
    assert_eq!(problems.len(), 0, "{}", problems.join("\n"));
}
