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
use slint::{ComponentHandle, Global};
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

/// One surface at one size: opens it, renders it settled, and reports what sticks out.
fn fit<C: ComponentHandle>(
    problems: &mut Vec<String>,
    what: &str,
    lang: &str,
    scale: f32,
    make: fn() -> Result<C, slint::PlatformError>,
    size: (u32, u32),
    prepare: impl FnOnce(&C),
) {
    let h = Harness::new(make, size.0, size.1);
    h.component.global::<SystemLook<'_>>().set_text_scale(scale);
    prepare(&h.component);
    let _ = h.render_settled();
    for item in outside(&h, size.0 as f32) {
        problems.push(format!(
            "[{}] {what} at {scale}x, {}px wide: {item}",
            if lang.is_empty() { "en" } else { lang },
            size.0
        ));
    }
}

#[test]
fn nothing_runs_past_the_window_edge_in_any_language() {
    let _english = EnglishAgain;
    let mut problems: Vec<String> = Vec::new();
    for lang in LANGUAGES {
        // Slint registers its catalogues when the first component is made.
        let _ = Harness::new(PanelWindow::new, 100, 100);
        slint::select_bundled_translation(lang).unwrap();
        for scale in SCALES {
            fit(
                &mut problems,
                "Panel",
                lang,
                scale,
                PanelWindow::new,
                (260, 560),
                |_| {},
            );
            fit(
                &mut problems,
                "Settings",
                lang,
                scale,
                SettingsPage::new,
                (420, 900),
                |_| {},
            );
            fit(
                &mut problems,
                "Main window",
                lang,
                scale,
                MainWindow::new,
                (480, 640),
                |_| {},
            );
            fit(
                &mut problems,
                "Help",
                lang,
                scale,
                HelpWindow::new,
                (420, 360),
                |_| {},
            );
            fit(
                &mut problems,
                "Welcome",
                lang,
                scale,
                OnboardingWindow::new,
                (560, 540),
                |_| {},
            );
        }
    }
    assert_eq!(problems, Vec::<String>::new());
}
