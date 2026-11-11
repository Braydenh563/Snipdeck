//! Final audit 1, accessible tree: every interactive element of the audited windows has a name,
//! is at least 24 px in both directions, and does not run past the window's right edge, at 1x and
//! 2.25x text size. Time is mocked, never the real clock.

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementQuery, ElementRoot};
use slint::ComponentHandle;
use snipdeck_ui::{HelpWindow, ItemEditor, SettingsPage, SystemLook, TemplateFillWindow};

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/fill_fixture.rs"]
mod fill;

const ROLES: [AccessibleRole; 8] = [
    AccessibleRole::Button,
    AccessibleRole::Checkbox,
    AccessibleRole::Switch,
    AccessibleRole::RadioButton,
    AccessibleRole::Tab,
    AccessibleRole::Combobox,
    AccessibleRole::TextInput,
    AccessibleRole::Spinbox,
];

/// Problems found in the interactive elements under `root`, for a window `width` px wide.
fn problems(root: &impl ElementRoot, width: f32, scene: &str, checked: &mut usize) -> Vec<String> {
    let mut out = Vec::new();
    for role in ROLES {
        for e in ElementQuery::from_root(root)
            .match_descendants()
            .match_accessible_role(role)
            .find_all()
        {
            let (p, s) = (e.absolute_position(), e.size());
            let name = e
                .accessible_label()
                .map(|l| l.to_string())
                .unwrap_or_default();
            if s.width == 0.0 || s.height == 0.0 {
                continue; // not shown
            }
            *checked += 1;
            if name.trim().is_empty() {
                out.push(format!(
                    "{scene}: {role:?} at ({:.0},{:.0}) has no name",
                    p.x, p.y
                ));
            }
            if s.width < 24.0 || s.height < 24.0 {
                out.push(format!(
                    "{scene}: {role:?} {name:?} is {:.0}x{:.0}",
                    s.width, s.height
                ));
            }
            if p.x + s.width > width + 0.5 {
                out.push(format!(
                    "{scene}: {role:?} {name:?} ends at {:.0} of {width}",
                    p.x + s.width
                ));
            }
        }
    }
    out
}

#[test]
fn interactive_elements_are_named_big_enough_and_inside_the_window() {
    on_ui_thread(|| {
        let mut all = Vec::new();
        let mut checked = 0;
        for (s, w) in [(1.0f32, 420.0f32), (2.25, 608.0)] {
            let ui = HelpWindow::new().unwrap();
            ui.global::<SystemLook<'_>>().set_text_scale(s);
            editor::help(&ui);
            editor::shortcuts(&ui);
            editor::tips(&ui);
            ui.window().set_size(slint::LogicalSize::new(w, 600.0));
            ui.show().unwrap();
            for tab in 0..4 {
                ui.set_tab(tab);
                slint::platform::update_timers_and_animations();
                all.extend(problems(
                    &ui,
                    w,
                    &format!("help tab {tab} at {s}x"),
                    &mut checked,
                ));
            }
            let ui = ItemEditor::new().unwrap();
            ui.global::<SystemLook<'_>>().set_text_scale(s);
            editor::text_item(&ui);
            ui.set_kind_index(3);
            ui.set_prompted_available(false);
            ui.set_formatted(true);
            ui.window().set_size(slint::LogicalSize::new(w, 900.0));
            ui.show().unwrap();
            slint::platform::update_timers_and_animations();
            all.extend(problems(
                &ui,
                w,
                &format!("item editor template at {s}x"),
                &mut checked,
            ));
            let ui = TemplateFillWindow::new().unwrap();
            ui.global::<SystemLook<'_>>().set_text_scale(s);
            fill::populate(&ui);
            ui.window()
                .set_size(slint::LogicalSize::new(w.max(320.0), 1400.0));
            ui.show().unwrap();
            slint::platform::update_timers_and_animations();
            all.extend(problems(
                &ui,
                w.max(320.0),
                &format!("fill at {s}x"),
                &mut checked,
            ));
            let ui = SettingsPage::new().unwrap();
            ui.global::<SystemLook<'_>>().set_text_scale(s);
            editor::settings(&ui);
            let sw = if s > 1.5 { 765.0 } else { w };
            ui.window().set_size(slint::LogicalSize::new(sw, 1400.0));
            ui.show().unwrap();
            slint::platform::update_timers_and_animations();
            all.extend(problems(
                &ui,
                sw,
                &format!("settings at {s}x"),
                &mut checked,
            ));
        }
        assert!(checked > 100, "only {checked} elements were checked");
        assert_eq!(all, Vec::<String>::new());
    });
}
