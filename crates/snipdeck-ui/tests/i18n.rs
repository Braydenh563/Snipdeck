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

/// Every locale calls an item an item: the noun the newer strings use, not the old snippet words.
#[test]
fn every_locale_uses_the_item_noun() {
    let old = [
        ("de", &["Textbaustein", "Baustein"][..]),
        ("es", &["fragmento", "Fragmento"]),
        ("fr", &["extrait", "Extrait"]),
        ("pt-BR", &["trecho", "Trecho"]),
        ("ja", &["スニペット"]),
        ("zh-Hans", &["片段"]),
    ];
    for (locale, words) in old {
        let path = format!(
            "{}/lang/{locale}/LC_MESSAGES/snipdeck-ui.po",
            env!("CARGO_MANIFEST_DIR")
        );
        let po = std::fs::read_to_string(&path).expect("catalogue");
        let mut in_msgstr = false;
        for (n, line) in po.lines().enumerate() {
            if line.starts_with("msgstr") {
                in_msgstr = true;
            } else if line.starts_with("msgid") || line.starts_with('#') || line.trim().is_empty() {
                in_msgstr = false;
            }
            if in_msgstr {
                for w in words {
                    assert!(
                        !line.contains(w),
                        "{locale}:{}: still says {w}: {line}",
                        n + 1
                    );
                }
            }
        }
    }
}
