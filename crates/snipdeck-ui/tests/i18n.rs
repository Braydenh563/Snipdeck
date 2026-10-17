//! Bundled translations switch at run time and untranslated strings fall back to the source text.

use i_slint_backend_testing::{ElementHandle, init_integration_test_with_mock_time};
use slint::ComponentHandle;
use snipdeck_ui::I18nProbe;

fn shown(ui: &I18nProbe, text: &str) -> bool {
    ElementHandle::find_by_accessible_label(ui, text).count() > 0
}

#[test]
fn the_selected_translation_applies_and_english_is_the_fallback() {
    init_integration_test_with_mock_time();
    let ui = I18nProbe::new().expect("probe");
    ui.show().expect("show");
    assert!(shown(&ui, "Paste"));
    slint::select_bundled_translation("de").expect("de is bundled");
    assert!(shown(&ui, "Einfügen"));
    assert!(!shown(&ui, "Paste"));
    slint::select_bundled_translation("").expect("back to the source language");
    assert!(shown(&ui, "Paste"));
}
