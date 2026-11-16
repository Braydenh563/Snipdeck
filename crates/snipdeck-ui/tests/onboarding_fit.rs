//! MAIN-55: the welcome window against the text scale. At the size it asks for, every control of every step is
//! inside the window; in a smaller window the Skip / Back / Next row is still inside (the content scrolls).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::panic,
    reason = "pixel geometry of a test frame"
)]

use i_slint_backend_testing::{AccessibleRole, ElementQuery};
use slint::ComponentHandle;
use snipdeck_ui::{OnboardingWindow, SystemLook, testing::Harness};

#[path = "support/editor_fixture.rs"]
mod editor;

fn window(size: (u32, u32), scale: f32, step: i32) -> Harness<OnboardingWindow> {
    let h = Harness::new(OnboardingWindow::new, size.0, size.1);
    h.component.global::<SystemLook<'_>>().set_text_scale(scale);
    editor::onboarding(&h.component, step);
    let _ = h.render_settled();
    h
}

fn boxes(h: &Harness<OnboardingWindow>) -> Vec<(String, f32, f32, f32, f32)> {
    ElementQuery::from_root(&h.component)
        .match_descendants()
        .find_all()
        .into_iter()
        .filter(|e| {
            matches!(
                e.accessible_role(),
                Some(AccessibleRole::Button | AccessibleRole::Checkbox | AccessibleRole::Switch)
            )
        })
        .map(|e| {
            let (p, s) = (e.absolute_position(), e.size());
            (
                e.accessible_label().map(|l| l.to_string()).unwrap_or_default(),
                p.x,
                p.y,
                s.width,
                s.height,
            )
        })
        .filter(|b| b.3 > 0.0)
        .collect()
}

fn outside(h: &Harness<OnboardingWindow>, size: (u32, u32), only: &[&str]) -> Vec<String> {
    boxes(h)
        .into_iter()
        .filter(|b| only.is_empty() || only.contains(&b.0.as_str()))
        .filter(|b| b.1 < -0.5 || b.2 < -0.5 || b.1 + b.3 > size.0 as f32 + 0.5 || b.2 + b.4 > size.1 as f32 + 0.5)
        .map(|b| format!("{:?} x {:.0}..{:.0} y {:.0}..{:.0} of {}x{}", b.0, b.1, b.1 + b.3, b.2, b.2 + b.4, size.0, size.1))
        .collect()
}

#[test]
fn the_welcome_window_fits_its_content_at_every_text_scale() {
    let mut problems = Vec::new();
    for scale in [1.0_f32, 1.5, 2.25] {
        for step in 0..4 {
            // The size the window asks for at this scale, read from a probe window of the same scale.
            let probe = window((560, 540), scale, step);
            let want = (
                probe.component.get_wanted_width().ceil() as u32,
                probe.component.get_wanted_height().ceil() as u32,
            );
            drop(probe);
            let h = window(want, scale, step);
            for p in outside(&h, want, &[]) {
                problems.push(format!("scale {scale} step {step} at the wanted size: {p}"));
            }
        }
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn the_welcome_buttons_stay_in_a_small_window_at_every_text_scale() {
    let mut problems = Vec::new();
    for scale in [1.0_f32, 1.5, 2.25] {
        for size in [(560_u32, 520_u32), (720, 620)] {
            for step in 0..4 {
                let h = window(size, scale, step);
                for p in outside(&h, size, &["Skip", "Back", "Next", "Get started"]) {
                    problems.push(format!("scale {scale} step {step}: {p}"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}
