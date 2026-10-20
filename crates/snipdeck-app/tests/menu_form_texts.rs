//! The words of the menus, the use button and the editors' refusals come from the UI crate's
//! `MenuTexts` and `FormTexts` globals, so they follow the language selected for the UI. Rust
//! builds no English for them. One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    controller::{AppMenuEntry, BoardMenuEntry, ItemMenuEntry, TagRenameError},
    main_window::{DeleteHint, NameError},
    panel::UseLabel,
    settings::DockEdge,
    wording::{
        app_menu_text, board_menu_text, delete_hint_text, item_menu_label, name_error_text,
        size_text, tag_rename_error_text, use_label_text,
    },
};
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

#[test]
fn the_use_label_is_use_where_a_click_pastes_and_copy_where_it_cannot() {
    on_ui_thread(|| {
        let w = PanelWindow::new().unwrap();
        assert_eq!(use_label_text(&w, UseLabel::Paste), "Use");
        assert_eq!(use_label_text(&w, UseLabel::Copy), "Copy");
    });
}

#[test]
fn every_menu_row_has_words_except_the_separators() {
    on_ui_thread(|| {
        let w = PanelWindow::new().unwrap();
        assert_eq!(
            app_menu_text(&w, AppMenuEntry::Dock, false, None),
            "Dock to the nearest edge"
        );
        assert_eq!(
            app_menu_text(&w, AppMenuEntry::Dock, false, Some(DockEdge::Left)),
            "Dock to the left edge"
        );
        assert_eq!(
            app_menu_text(&w, AppMenuEntry::Dock, true, None),
            "Undock the panel"
        );
        assert_eq!(
            app_menu_text(&w, AppMenuEntry::Commands, false, None),
            "All commands\u{2026}"
        );
        assert_eq!(app_menu_text(&w, AppMenuEntry::Separator, false, None), "");
        assert_eq!(
            item_menu_label(&w, ItemMenuEntry::Pin { pinned: true }),
            "Unpin"
        );
        assert_eq!(item_menu_label(&w, ItemMenuEntry::Trash), "Move to Trash");
        assert_eq!(item_menu_label(&w, ItemMenuEntry::Separator), "");
        assert_eq!(
            board_menu_text(&w, &BoardMenuEntry::Delete),
            "Delete board\u{2026}"
        );
        assert_eq!(board_menu_text(&w, &BoardMenuEntry::Separator), "");
    });
}

#[test]
fn refusals_and_sizes_are_worded_by_the_ui_crate() {
    on_ui_thread(|| {
        let w = PanelWindow::new().unwrap();
        assert_eq!(name_error_text(&w, &NameError::Empty), "Give it a name.");
        assert_eq!(
            name_error_text(&w, &NameError::TooLong { max: 80 }),
            "Names can have at most 80 characters."
        );
        assert_eq!(
            tag_rename_error_text(&w, &TagRenameError::Exists),
            "A tag with that name already exists."
        );
        assert!(
            delete_hint_text(&w, DeleteHint::LastBoard).starts_with("This is your only board.")
        );
        assert_eq!(size_text(&w, 1), "1 byte");
        assert_eq!(size_text(&w, 800), "800 bytes");
        assert_eq!(size_text(&w, 12 * 1024), "12 KB");
        assert_eq!(size_text(&w, 2 * 1024 * 1024 + 450_000), "2.4 MB");
    });
}
