//! The names of colour labels, list icons and spoken keys, and the words of a new library, come
//! from the UI crate's `NameTexts` and `StarterTexts` globals, so they follow the selected
//! language. One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    commands::KeyWords,
    starter::StarterWords,
    wording::{item_label_name, key_words, list_icon_name, starter_words},
};
use snipdeck_core::{ItemLabel, ListIcon};
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

#[test]
fn in_english_the_globals_give_the_english_words() {
    on_ui_thread(|| {
        let window = PanelWindow::new().expect("window");
        // The tests of this file share the process, and the language is process-wide.
        slint::select_bundled_translation("").expect("untranslated");
        assert_eq!(starter_words(&window), StarterWords::english());
        assert_eq!(key_words(&window), KeyWords::english());
        let labels: Vec<String> = ItemLabel::ALL
            .iter()
            .map(|l| item_label_name(&window, *l))
            .collect();
        assert_eq!(
            labels,
            [
                "Blue", "Teal", "Green", "Amber", "Orange", "Red", "Violet", "Slate"
            ]
        );
        let icons: Vec<String> = ListIcon::ALL
            .iter()
            .map(|i| list_icon_name(&window, *i))
            .collect();
        assert_eq!(icons.len(), 24);
        assert_eq!(
            &icons[..6],
            ["Folder", "Star", "Heart", "Briefcase", "House", "Person"]
        );
        assert_eq!(icons[23], "Rocket");
    });
}

#[test]
fn every_label_and_icon_has_its_own_name_in_every_bundled_language() {
    on_ui_thread(|| {
        let window = PanelWindow::new().expect("window");
        for tag in ["de", "es", "fr", "ja", "pt-BR", "zh-Hans"] {
            snipdeck_app::language::apply(tag).expect("bundled language");
            let mut names: Vec<String> = ItemLabel::ALL
                .iter()
                .map(|l| item_label_name(&window, *l))
                .collect();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), ItemLabel::ALL.len(), "{tag}: label names");
            let mut icons: Vec<String> = ListIcon::ALL
                .iter()
                .map(|i| list_icon_name(&window, *i))
                .collect();
            assert!(icons.iter().all(|n| !n.is_empty()), "{tag}: icon names");
            icons.sort();
            icons.dedup();
            assert_eq!(icons.len(), ListIcon::ALL.len(), "{tag}: icon names differ");
        }
        slint::select_bundled_translation("").expect("untranslated");
    });
}

#[test]
fn a_new_library_is_worded_in_the_selected_language() {
    on_ui_thread(|| {
        let window = PanelWindow::new().expect("window");
        snipdeck_app::language::apply("de").expect("bundled language");
        let german = starter_words(&window);
        let english = StarterWords::english();
        assert_eq!(german.board, "Meine Textbausteine");
        assert_ne!(german.getting_started, english.getting_started);
        assert_ne!(german.new_list, english.new_list);
        assert_ne!(german.new_board, english.new_board);
        for (german, english) in german.cards.iter().zip(&english.cards) {
            assert_ne!(german.0, english.0);
            assert_ne!(german.1, english.1);
        }
        // The template card keeps its placeholders, whatever the language.
        assert!(german.cards[2].1.contains("{date}") && german.cards[2].1.contains("{time}"));
        assert_ne!(key_words(&window), KeyWords::english());
        slint::select_bundled_translation("").expect("untranslated");
    });
}
