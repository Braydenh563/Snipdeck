//! Sample data for the editors and the settings page, shared by `tests/editors.rs` (goldens) and
//! `tests/editors_logic.rs` (behaviour); `#[path]`-included, so each test binary compiles its own copy.
#![allow(dead_code, reason = "each test binary uses a subset of the fixtures")]

use slint::{Color, Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use snipdeck_ui::{
    BackupRow, ChoiceEntry, HotkeySetting, ItemEditor, ListEditor, SettingsPage, TagManager,
};

pub fn strings(items: &[&str]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(
        items
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    ))
}

pub fn list_names() -> ModelRc<SharedString> {
    strings(&["Email replies", "Addresses", "Code"])
}

/// Tags with `billing` ticked.
pub fn tag_entries() -> Vec<ChoiceEntry> {
    [
        ("billing", 9, true),
        ("meeting", 4, false),
        ("sql", 3, false),
    ]
    .map(|(label, count, checked)| ChoiceEntry {
        label: label.into(),
        count,
        color: Color::default(),
        checked,
    })
    .to_vec()
}

pub fn tag_model(entries: Vec<ChoiceEntry>) -> ModelRc<ChoiceEntry> {
    ModelRc::new(VecModel::from(entries))
}

/// A 160 x 90 gradient standing in for a captured image (no glyphs, nothing system dependent).
pub fn gradient() -> Image {
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(160, 90);
    let width = buffer.width() as usize;
    for (i, px) in buffer.make_mut_slice().iter_mut().enumerate() {
        let x = u8::try_from(i % width).unwrap_or(u8::MAX); // i % 160 < 160
        let y = u8::try_from(i / width).unwrap_or(u8::MAX); // i / 160 < 90
        *px = Rgba8Pixel {
            r: x.saturating_mul(2),
            g: y.saturating_mul(2),
            b: 160,
            a: 255,
        };
    }
    Image::from_rgba8(buffer)
}

/// The editor of an existing text item: title, body, list "Email replies", tag billing.
pub fn text_item(ui: &ItemEditor) {
    ui.set_is_new(false);
    ui.set_kind_index(0);
    ui.set_item_title("Thanks, received".into());
    ui.set_content(
        "Thanks for sending this over. I'll review it today and get back to you by the end of the day."
            .into(),
    );
    ui.set_list_names(list_names());
    ui.set_list_index(0);
    ui.set_tags(tag_model(tag_entries()));
    ui.set_tags_on(1);
    ui.invoke_mark_clean();
}

pub fn hotkeys() -> Vec<HotkeySetting> {
    [
        (&["Ctrl", "Alt", "V"][..], "Ctrl+Alt+V"),
        (&["Ctrl", "Shift", "Space"][..], "Ctrl+Shift+Space"),
        (&["Ctrl", "Shift", "2"][..], "Ctrl+Shift+2"),
        (&[][..], ""),
    ]
    .map(|(keys, value)| HotkeySetting {
        keys: strings(keys),
        value: value.into(),
        conflict: SharedString::new(),
    })
    .to_vec()
}

pub fn settings(ui: &SettingsPage) {
    ui.set_hotkeys(ModelRc::new(VecModel::from(hotkeys())));
    ui.set_languages(strings(&[
        "Follow Windows",
        "English",
        "Deutsch",
        "Francais",
    ]));
    ui.set_language_index(1);
    ui.set_theme_index(0);
    ui.set_autostart(true);
    ui.set_close_to_tray(true);
    ui.set_panel_width(320);
    ui.set_no_paste_apps(strings(&["keepass.exe", "1password.exe"]));
    ui.set_pro(false);
    ui.set_has_data_folder(true);
    ui.set_backups(ModelRc::new(VecModel::from(vec![
        BackupRow {
            date: "2026-10-18".into(),
            size: "34 KB".into(),
        },
        BackupRow {
            date: "2026-10-17".into(),
            size: "33 KB".into(),
        },
    ])));
}

pub fn tag_manager(ui: &TagManager) {
    ui.set_tags(tag_model(
        [
            ("billing", 9),
            ("meeting", 4),
            ("sql", 3),
            (
                "a-rather-long-tag-name-that-needs-to-be-elided-somewhere",
                1,
            ),
            ("unused", 0),
        ]
        .map(|(label, count)| ChoiceEntry {
            label: label.into(),
            count,
            color: Color::default(),
            checked: false,
        })
        .to_vec(),
    ));
}

pub fn list_editor_existing(ui: &ListEditor) {
    ui.set_is_new(false);
    ui.set_name("Email replies".into());
    ui.set_accent(1);
    ui.set_sort_index(0);
    ui.set_item_count(24);
    ui.invoke_mark_clean();
}
