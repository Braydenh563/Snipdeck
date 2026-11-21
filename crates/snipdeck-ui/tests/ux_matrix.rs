//! UX-37, 38, 45: one table over windows x text sizes (1x, 2.25x) x looks (light, dark, high
//! contrast, and light with reduced motion). In each cell every interactive element is named, at
//! least 24 px each way and inside the window, and every Tab press changes what is drawn (a
//! visible focus), for as long as it takes to come round to the first stop. Windows are opened at
//! their own minimum size where they state one. The clock is frozen by the harness.
//!
//! Not covered: the screen reader's own announcements (Narrator is Windows-only, see
//! `tools/windows-test/`), and the Panel, Main window and toast, which `audit_final1.rs` and
//! `panel_logic.rs` walk with their own fixtures.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::{AccessibleRole, ElementQuery};
use slint::{ComponentHandle, LogicalSize, platform::Key};
use snipdeck_ui::{
    BoardEditor, HelpWindow, ItemEditor, ListEditor, OnboardingWindow, SettingsPage, SystemLook,
    TagManager,
    testing::{Frame, Harness},
};

#[path = "support/editor_fixture.rs"]
mod editor;
#[path = "support/mod.rs"]
mod support;

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

#[derive(Clone, Copy, Debug)]
enum Look {
    Light,
    Dark,
    HighContrast,
    ReducedMotion,
}

const LOOKS: [Look; 4] = [
    Look::Light,
    Look::Dark,
    Look::HighContrast,
    Look::ReducedMotion,
];
const SCALES: [f32; 2] = [1.0, 2.25];

fn problems<C: ComponentHandle>(h: &Harness<C>, width: f32, scene: &str) -> (usize, Vec<String>) {
    let mut out = Vec::new();
    let mut checked = 0;
    for role in ROLES {
        for e in ElementQuery::from_root(&h.component)
            .match_descendants()
            .match_accessible_role(role)
            .find_all()
        {
            let (p, s) = (e.absolute_position(), e.size());
            if s.width < 0.5 || s.height < 0.5 {
                continue;
            }
            checked += 1;
            let name = e
                .accessible_label()
                .map(|l| l.to_string())
                .unwrap_or_default();
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
    (checked, out)
}

fn hash(frame: &Frame) -> u64 {
    frame.rgb.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn snap<C: ComponentHandle>(h: &Harness<C>) -> Option<Frame> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| h.render_settled())).ok()
}

/// Presses Tab up to `limit` times; every press must change the picture. Returns problems.
fn tab_walk<C: ComponentHandle>(h: &Harness<C>, scene: &str, limit: usize) -> Vec<String> {
    let w = h.component.window();
    let mut out = Vec::new();
    let mut last = hash(&h.render_settled());
    let mut first_after_tab: Option<u64> = None;
    for press in 1..=limit {
        support::press(w, &support::key(Key::Tab));
        let Some(frame) = snap(h) else {
            out.push(format!("{scene}: Tab {press} changed nothing on screen"));
            continue;
        };
        let now = hash(&frame);
        if now == last {
            out.push(format!("{scene}: Tab {press} drew the same picture"));
        }
        if first_after_tab == Some(now) {
            return out; // came round to the first stop
        }
        first_after_tab.get_or_insert(now);
        last = now;
    }
    out.push(format!("{scene}: no way round after {limit} Tab presses"));
    out
}

fn apply_look<C: ComponentHandle>(c: &C, scale: f32, look: Look)
where
    for<'a> SystemLook<'a>: slint::Global<'a, C>,
{
    let sys = c.global::<SystemLook<'_>>();
    sys.set_text_scale(scale);
    sys.set_high_contrast(matches!(look, Look::HighContrast));
    sys.set_reduced_motion(matches!(look, Look::ReducedMotion));
}

/// One row of the table: how to make a window, fill it and read its own minimum size.
fn run<C: ComponentHandle>(
    name: &str,
    create: fn() -> Result<C, slint::PlatformError>,
    prepare: fn(&C, bool),
    min: fn(&C) -> (f32, f32),
    all: &mut Vec<String>,
    checked: &mut usize,
) where
    for<'a> SystemLook<'a>: slint::Global<'a, C>,
{
    for scale in SCALES {
        for look in LOOKS {
            let h = Harness::new(create, 900, 900);
            let c = &h.component;
            prepare(c, matches!(look, Look::Dark));
            apply_look(c, scale, look);
            let (mw, mh) = min(c);
            let (w, hh) = (mw.max(320.0), mh.max(240.0));
            c.window().set_size(LogicalSize::new(w, hh.max(700.0)));
            let scene = format!("{name} at {scale}x, {look:?}");
            let (n, found) = problems(&h, w, &scene);
            *checked += n;
            all.extend(found);
            all.extend(tab_walk(&h, &scene, 80));
        }
    }
}

#[test]
fn every_window_is_named_sized_and_keyboard_visible_in_every_look() {
    let mut all = Vec::new();
    let mut checked = 0;
    run(
        "tag manager",
        TagManager::new,
        |c, d| {
            editor::tag_manager(c);
            c.set_dark(d);
        },
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    run(
        "list editor",
        ListEditor::new,
        |c, d| {
            editor::list_editor_existing(c);
            // The app supplies the icon names; without them every icon cell is unnamed.
            c.set_icon_names(editor::strings(&["Briefcase"; 24]));
            c.set_dark(d);
        },
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    run(
        "board editor",
        BoardEditor::new,
        |c, d| c.set_dark(d),
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    run(
        "item editor",
        ItemEditor::new,
        |c, d| {
            editor::text_item(c);
            c.set_dark(d);
        },
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    run(
        "help",
        HelpWindow::new,
        |c, d| {
            editor::help(c);
            editor::shortcuts(c);
            editor::tips(c);
            c.set_dark(d);
        },
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    run(
        "settings",
        SettingsPage::new,
        |c, d| {
            editor::settings(c);
            c.set_dark(d);
        },
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    run(
        "onboarding",
        OnboardingWindow::new,
        |c, d| {
            editor::onboarding(c, 4);
            c.set_dark(d);
        },
        |c| (c.get_min_window_width(), c.get_min_window_height()),
        &mut all,
        &mut checked,
    );
    assert!(checked > 150, "only {checked} elements were checked");
    assert_eq!(all.len(), 0, "{all:#?}");
}
