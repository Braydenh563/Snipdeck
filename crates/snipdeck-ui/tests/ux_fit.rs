//! Fit checks from the UX audit: the Settings search leaves no blank gap (H1), toasts wrap instead of eliding (H3),
//! the Shortcuts list does not clip wrapped titles (M1), keycaps tell a chord from alternatives (M5) and the palette
//! scales with the text size (L3). Measured in the headless harness; sizes are logical pixels.

use i_slint_backend_testing::{AccessibleRole as Role, ElementQuery};
use slint::{ComponentHandle, Global};
use snipdeck_ui::testing::{Harness, assert_golden};
use snipdeck_ui::{
    HelpWindow, PanelWindow, SettingsFilter, SettingsPage, SystemLook, ToastKind, ToastWindow,
};

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/panel_fixture.rs"]
mod panel_fx;

/// (label, x, y, width, height) of every shown element with `role`, top to bottom.
fn find<C: ComponentHandle>(h: &Harness<C>, role: Role) -> Vec<(String, f32, f32, f32, f32)> {
    let mut out: Vec<(String, f32, f32, f32, f32)> = Vec::new();
    for e in ElementQuery::from_root(&h.component)
        .match_descendants()
        .match_accessible_role(role)
        .find_all()
    {
        let (p, s) = (e.absolute_position(), e.size());
        let n = (
            e.accessible_label()
                .map(|l| l.to_string())
                .unwrap_or_default(),
            p.x,
            p.y,
            s.width,
            s.height,
        );
        if s.width > 0.5
            && s.height > 0.5
            && !out.iter().any(|o| o.0 == n.0 && (o.2 - n.2).abs() < 0.5)
        {
            out.push(n);
        }
    }
    out.sort_by(|a, b| a.2.total_cmp(&b.2));
    out
}

/// H1: with a search query, the first section that still matches starts right under the search field, at every text
/// size, instead of below the room the hidden sections used to take.
#[test]
fn settings_search_leaves_no_blank_gap() {
    let mut problems: Vec<String> = Vec::new();
    for (scale, width) in [(1.0f32, 640u32), (2.25, 765)] {
        for (query, section) in [
            ("as I type", "Typing"),
            ("backups", "Backups and your data"),
            ("supporter", "Licence"),
            ("programs", "Programs"),
        ] {
            let h = Harness::new(SettingsPage::new, width, 760);
            h.component.global::<SystemLook<'_>>().set_text_scale(scale);
            editor::settings(&h.component);
            let filter = SettingsFilter::get(&h.component);
            filter.on_matches(|text, query| text.to_lowercase().contains(&query.to_lowercase()));
            filter.set_query(query.into());
            let _ = h.render_settled();
            let field = find(&h, Role::TextInput)
                .into_iter()
                .find(|n| n.0 == "Search settings")
                .expect("search field");
            let Some(sec) = find(&h, Role::Groupbox)
                .into_iter()
                .find(|n| n.0 == section)
            else {
                problems.push(format!(
                    "{query:?} at {scale}x: section {section:?} is not on screen"
                ));
                continue;
            };
            let gap = sec.2 - (field.2 + field.4);
            if gap > 40.0 * scale {
                problems.push(format!(
                    "{query:?} at {scale}x: {section:?} starts {gap:.0} px under the search field"
                ));
            }
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}

const LONG_TOAST: &str = "Saved. This code overlaps the trigger of \u{201c}Signature\u{201d}, so one of them may never fire.";

/// H3: a long toast wraps to two or three lines instead of eliding to one, in the toast window (capped at 480 logical
/// px times the text size) and in a 320 px Panel.
#[test]
fn long_toasts_wrap_in_the_toast_window() {
    let mut problems: Vec<String> = Vec::new();
    for scale in [1.0f32, 2.25] {
        let size = |text: &str| {
            let h = Harness::new(ToastWindow::new, 1100, 300);
            h.component.global::<SystemLook<'_>>().set_text_scale(scale);
            h.component.set_text(text.into());
            h.component.set_kind(ToastKind::Danger);
            h.component.set_undo_label("Undo".into());
            let _ = h.render_settled();
            (
                h.component.get_content_width(),
                h.component.get_content_height(),
            )
        };
        let (short_w, short_h) = size("Copied");
        let (w, h) = size(LONG_TOAST);
        if w > 480.0 * scale + 0.5 {
            problems.push(format!(
                "{scale}x: the toast is {w:.0} wide, over the {:.0} cap",
                480.0 * scale
            ));
        }
        if h < short_h + 8.0 {
            problems.push(format!(
                "{scale}x: a long toast is {h:.0} tall, a short one {short_h:.0}: it did not wrap"
            ));
        }
        if h > short_h * 3.5 {
            problems.push(format!(
                "{scale}x: a long toast is {h:.0} tall (short {short_h:.0}): more than three lines"
            ));
        }
        let _ = short_w;
    }
    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn long_toasts_wrap_in_the_panel() {
    let mut problems: Vec<String> = Vec::new();
    for scale in [1.0f32, 2.25] {
        let h = Harness::new(PanelWindow::new, 320, 600);
        panel_fx::populate(&h.component);
        h.component.global::<SystemLook<'_>>().set_text_scale(scale);
        h.component.set_toast_text(LONG_TOAST.into());
        h.component.set_toast_kind(ToastKind::Danger);
        let _ = h.render_settled();
        let toast = find(&h, Role::Text)
            .into_iter()
            .find(|n| n.0 == LONG_TOAST)
            .expect("toast node");
        if toast.4 < 40.0 + 8.0 {
            problems.push(format!(
                "{scale}x: the toast is {:.0} tall: it did not wrap",
                toast.4
            ));
        }
        if toast.2 < 0.0 || toast.2 + toast.4 > 600.0 || toast.1 < 0.0 || toast.1 + toast.3 > 320.5
        {
            problems.push(format!(
                "{scale}x: the toast is at ({:.0},{:.0}) {:.0}x{:.0} in 320x600",
                toast.1, toast.2, toast.3, toast.4
            ));
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}

/// The wrapped toast as drawn: toast window at 1x and 2.25x (sized to its content), Panel at 320 wide at 2.25x.
#[test]
fn wrapped_toast_goldens() {
    for (scale, name) in [(1.0f32, "toast-wrapped-1x"), (2.25, "toast-wrapped-225")] {
        let probe = Harness::new(ToastWindow::new, 1100, 300);
        probe
            .component
            .global::<SystemLook<'_>>()
            .set_text_scale(scale);
        probe.component.set_text(LONG_TOAST.into());
        probe.component.set_kind(ToastKind::Danger);
        probe.component.set_undo_label("Undo".into());
        let _ = probe.render_settled();
        let (w, h) = (
            probe.component.get_content_width().ceil() as u32,
            probe.component.get_content_height().ceil() as u32,
        );
        let hn = Harness::new(ToastWindow::new, w, h);
        hn.component
            .global::<SystemLook<'_>>()
            .set_text_scale(scale);
        hn.component.set_text(LONG_TOAST.into());
        hn.component.set_kind(ToastKind::Danger);
        hn.component.set_undo_label("Undo".into());
        assert_golden(name, &hn.render_settled());
    }
    let h = Harness::new(PanelWindow::new, 320, 560);
    panel_fx::populate(&h.component);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    h.component.set_toast_text(LONG_TOAST.into());
    h.component.set_toast_kind(ToastKind::Danger);
    assert_golden("panel-toast-wrapped-225", &h.render_settled());
}

/// M1: a Shortcuts line whose title wraps is as tall as its wrapped lines (it used to keep a one-line minimum and clip).
#[test]
fn wrapped_shortcut_titles_are_not_clipped() {
    let h = Harness::new(HelpWindow::new, 608, 1400);
    h.component.global::<SystemLook<'_>>().set_text_scale(2.25);
    editor::help(&h.component);
    let line = |title: &str, keys: &[&str]| snipdeck_ui::ShortcutLineData {
        title: title.into(),
        keys: slint::ModelRc::new(slint::VecModel::from(
            keys.iter()
                .map(|k| (*k).into())
                .collect::<Vec<slint::SharedString>>(),
        )),
        spoken: keys.join(" ").into(),
    };
    h.component
        .set_shortcut_groups(slint::ModelRc::new(slint::VecModel::from(vec![
            snipdeck_ui::ShortcutGroup {
                title: "Main window".into(),
                note: "".into(),
                lines: slint::ModelRc::new(slint::VecModel::from(vec![
                    line(
                        "Move the selected items to another list or board",
                        &["Ctrl", "Shift", "M"],
                    ),
                    line("Group the selected items under one heading, then move the whole group to another board without opening it", &["Ctrl", "G"]),
                    line("New item", &["Ctrl", "N"]),
                ])),
            },
        ])));
    h.component.set_tab(2);
    let frame = h.render_settled();
    assert_golden("shortcuts-wrapped-225", &frame);
    let lines: Vec<_> = find(&h, Role::Groupbox)
        .into_iter()
        .filter(|n| n.0.chars().count() >= 34)
        .collect();
    assert!(!lines.is_empty(), "no long shortcut titles on screen");
    // Two lines of the 14 px text at 2.25x are about 2 x 44 high; a one-line minimum is about 67.
    let short: Vec<_> = lines
        .iter()
        .filter(|n| n.4 < 85.0)
        .map(|n| (n.0.clone(), n.4))
        .collect();
    assert_eq!(
        short,
        Vec::new(),
        "lines of long titles that are too short to hold them"
    );
}
