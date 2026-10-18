//! The tray menu words come from the bundled translations (a native menu cannot use `@tr`).

use i_slint_backend_testing::init_integration_test_with_mock_time;
use slint::Global;
use snipdeck_ui::{I18nProbe, TrayTexts};

fn words(ui: &I18nProbe) -> [String; 4] {
    let texts = TrayTexts::get(ui);
    [
        texts.get_open_panel().into(),
        texts.get_open_snipdeck().into(),
        texts.get_pause_hotkeys().into(),
        texts.get_quit().into(),
    ]
}

#[test]
fn the_tray_words_follow_the_selected_translation() {
    init_integration_test_with_mock_time();
    let ui = I18nProbe::new().expect("probe");
    assert_eq!(
        words(&ui),
        ["Open Panel", "Open Snipdeck", "Pause hotkeys", "Quit"]
    );
    slint::select_bundled_translation("de").expect("de is bundled");
    assert_eq!(
        words(&ui),
        [
            "Panel öffnen",
            "Snipdeck öffnen",
            "Tastenkürzel pausieren",
            "Beenden"
        ]
    );
    slint::select_bundled_translation("").expect("back to the source language");
    assert_eq!(words(&ui)[3], "Quit");
}
