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

/// Every locale calls an item by the glossary's word for it, never by one the glossary says to
/// avoid (scripts/glossary.json is the one source of the nouns).
#[test]
fn every_locale_uses_the_glossary_item_noun() {
    let glossary: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/glossary.json"
        ))
        .expect("glossary"),
    )
    .expect("glossary is JSON");
    let item = glossary["terms"]
        .as_array()
        .expect("terms")
        .iter()
        .find(|t| t["id"] == "item")
        .expect("an item term");
    let avoid = item["avoid"].as_object().expect("avoid words");
    assert!(avoid.len() >= 5, "every locale with a wrong noun is listed");
    for (locale, words) in avoid {
        let path = format!(
            "{}/lang/{locale}/LC_MESSAGES/snipdeck-ui.po",
            env!("CARGO_MANIFEST_DIR")
        );
        let po = std::fs::read_to_string(&path).expect("catalogue");
        let stems = item["stems"][locale.as_str()].as_array().expect("stems");
        assert!(
            stems.iter().any(|w| po.contains(w.as_str().expect("stem"))),
            "{locale} uses the glossary noun somewhere"
        );
        // A string is about items when its English names one (the glossary's pattern, plus the
        // older "snippet" source words); only those must use the item noun.
        let about_items = |msgid: &str| {
            msgid
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .any(|t| matches!(t, "item" | "items" | "snippet" | "snippets"))
        };
        let mut msgid = String::new();
        let mut in_msgstr = false;
        for (n, line) in po.lines().enumerate() {
            if line.starts_with("msgid") {
                msgid = line.to_owned();
                in_msgstr = false;
            } else if line.starts_with("msgstr") {
                in_msgstr = true;
            } else if line.starts_with('"') && !in_msgstr {
                msgid.push_str(line);
            } else if !line.starts_with('"') {
                in_msgstr = false;
            }
            if !in_msgstr || !about_items(&msgid) {
                continue;
            }
            for w in words.as_array().expect("words") {
                let w = w.as_str().expect("word");
                // Latin words match whole words only; CJK text has no spaces, so it matches anywhere.
                let hit = if w.chars().all(|c| u32::from(c) < 0x3000) {
                    line.split(|c: char| !c.is_alphanumeric()).any(|t| t == w)
                } else {
                    line.contains(w)
                };
                assert!(!hit, "{locale}:{}: {msgid} says {w}: {line}", n + 1);
            }
        }
    }
}
