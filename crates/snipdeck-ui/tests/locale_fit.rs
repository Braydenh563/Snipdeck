//! On-screen fit per language (audit EXTRAS-15). Translations are often longer than English; a
//! fixed-width control or a label with no room then runs past the window's edge. For every bundled
//! language, at the normal text size and at 2.25x, this opens the surfaces at their smallest usable
//! width and checks, through the accessibility tree, that no shown element starts left of the
//! window or ends right of it. (Height is not checked: the pages scroll.)
//!
//! A test binary of its own: the interface language is process-wide, so it cannot share a process
//! with tests that read English.
#![allow(clippy::unwrap_used, reason = "test code")]

use i_slint_backend_testing::ElementQuery;
use slint::ComponentHandle;
use snipdeck_ui::testing::Harness;
use snipdeck_ui::{
    HelpWindow, MainWindow, OnboardingWindow, PanelWindow, SettingsPage, SystemLook,
};

/// The language tags that have a bundled catalogue (English is the source language, `""`).
const LANGUAGES: [&str; 7] = ["", "de", "es", "fr", "ja", "pt-BR", "zh-Hans"];
const SCALES: [f32; 2] = [1.0, 2.25];

/// Puts the source language back when a test ends, even by a panic.
struct EnglishAgain;
impl Drop for EnglishAgain {
    fn drop(&mut self) {
        let _ = slint::select_bundled_translation("");
    }
}

/// Every shown element of `component` that lies outside `0..=width` horizontally, as
/// `"role label (x..x2)"`.
fn outside<C: ComponentHandle>(h: &Harness<C>, width: f32) -> Vec<String> {
    let mut out = Vec::new();
    for e in ElementQuery::from_root(&h.component)
        .match_descendants()
        .find_all()
    {
        let (p, s) = (e.absolute_position(), e.size());
        if s.width < 0.5 || s.height < 0.5 {
            continue;
        }
        if p.x < -0.5 || p.x + s.width > width + 0.5 {
            out.push(format!(
                "{:?} {:?} ({:.0}..{:.0})",
                e.accessible_role(),
                e.accessible_label().unwrap_or_default(),
                p.x,
                p.x + s.width
            ));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Opens one surface in the current language.
type Run = Box<dyn Fn(f32) -> Vec<String>>;

/// One surface at one size: opens it, renders it settled, and returns what sticks out.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "pixel sizes of test windows are small positive numbers"
)]
fn fit<C: ComponentHandle>(
    make: fn() -> Result<C, slint::PlatformError>,
    size: (u32, u32),
    min_at: fn(f32) -> f32,
    scale: f32,
    set_scale: fn(&C, f32),
) -> Vec<String> {
    // Settings, Help and the Main window widen their smallest size with the text size (`min-window-width`).
    let width = size.0.max(min_at(scale).ceil() as u32);
    let h = Harness::new(make, width, size.1);
    set_scale(&h.component, scale);
    let _ = h.render_settled();
    outside(&h, width as f32)
}

/// English is the design: what it already lets stick out (a scrolled strip, a track) is the
/// baseline, and a language fails when it makes more elements stick out than English does.
#[test]
fn a_language_does_not_push_more_past_the_window_edge_than_english_does() {
    let _english = EnglishAgain;
    let mut problems: Vec<String> = Vec::new();
    // `(surface name, runner)`: each runner opens its surface in the current language.
    let surfaces: Vec<(&str, Run)> = vec![
        (
            "Panel",
            Box::new(|v| {
                fit(
                    PanelWindow::new,
                    (260, 560),
                    |_| 0.0,
                    v,
                    |c, v| {
                        c.global::<SystemLook<'_>>().set_text_scale(v);
                    },
                )
            }),
        ),
        (
            "Settings",
            Box::new(|v| {
                fit(
                    SettingsPage::new,
                    (420, 900),
                    |s| 340.0 * s,
                    v,
                    |c, v| {
                        c.global::<SystemLook<'_>>().set_text_scale(v);
                    },
                )
            }),
        ),
        (
            "Main window",
            Box::new(|v| {
                fit(
                    MainWindow::new,
                    (480, 640),
                    |s| 480.0 + (s - 1.0) * 552.0,
                    v,
                    |c, v| {
                        c.global::<SystemLook<'_>>().set_text_scale(v);
                    },
                )
            }),
        ),
        (
            "Help",
            Box::new(|v| {
                fit(
                    HelpWindow::new,
                    (420, 360),
                    |s| 270.0 * s,
                    v,
                    |c, v| {
                        c.global::<SystemLook<'_>>().set_text_scale(v);
                    },
                )
            }),
        ),
        (
            "Welcome",
            Box::new(|v| {
                fit(
                    OnboardingWindow::new,
                    (560, 540),
                    |_| 0.0,
                    v,
                    |c, v| {
                        c.global::<SystemLook<'_>>().set_text_scale(v);
                    },
                )
            }),
        ),
    ];
    let mut baseline: std::collections::BTreeMap<(usize, usize), usize> =
        std::collections::BTreeMap::default();
    for lang in LANGUAGES {
        // Slint registers its catalogues when the first component is made.
        let _ = Harness::new(PanelWindow::new, 100, 100);
        slint::select_bundled_translation(lang).unwrap();
        for (si, (name, run)) in surfaces.iter().enumerate() {
            for (ci, scale) in SCALES.iter().enumerate() {
                let found = run(*scale);
                if lang.is_empty() {
                    // The Main window's toolbar fits at every text size in English: its minimum width
                    // grows with the text size (`min-window-width`), so a language starts from zero.
                    if *name == "Main window" {
                        assert_eq!(found, Vec::<String>::new(), "English, {scale}x");
                    }
                    baseline.insert((si, ci), found.len());
                } else if found.len() > baseline[&(si, ci)] {
                    problems.push(format!(
                        "[{lang}] {name} at {scale}x: {} stick out, English {}: {found:?}",
                        found.len(),
                        baseline[&(si, ci)]
                    ));
                }
            }
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}
