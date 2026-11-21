//! The saved language applies in the real start-up order: a component first, then the choice.
//! (This test binary runs one process, so no earlier test has registered the catalogues.)
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use i_slint_backend_testing::ElementHandle;
use slint::ComponentHandle;
use snipdeck_ui::I18nProbe;

#[path = "support/mod.rs"]
mod support;

fn shown(ui: &I18nProbe, text: &str) -> bool {
    ElementHandle::find_by_accessible_label(ui, text).count() > 0
}

#[test]
fn a_saved_language_applies_after_the_first_window_and_follow_windows_changes_nothing() {
    support::on_ui_thread(|| {
        // The old order (the choice before any component) is refused: this is what lost the
        // saved language.
        assert!(snipdeck_app::language::apply("de").is_err());

        let ui = I18nProbe::new().expect("probe");
        ui.show().expect("show");
        assert!(shown(&ui, "Paste"), "the test process follows no locale");

        snipdeck_app::language::apply("de").expect("de is bundled");
        assert!(shown(&ui, "Einfügen"));

        // "Follow Windows" must not force English over the language Slint picked.
        snipdeck_app::language::apply("").expect("empty is a no-op");
        assert!(shown(&ui, "Einfügen"));

        assert!(snipdeck_app::language::apply("xx").is_err());
        assert!(shown(&ui, "Einfügen"), "a bad tag changes nothing");
    });
}
