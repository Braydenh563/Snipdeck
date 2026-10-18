//! Sample Picker data shared by `tests/picker.rs` (goldens) and `tests/picker_logic.rs` (behaviour);
//! `#[path]`-included, so each test binary compiles its own copy.
#![allow(dead_code, reason = "each test binary uses a subset of the fixtures")]

use slint::{Color, ModelRc, VecModel};
use snipdeck_ui::{ItemKind, PickerRow, PickerWindow};

/// Step-600 list hues (light theme) from `design/tokens/palette.json`.
pub const TEAL_600: Color = Color::from_rgb_u8(0x04, 0x84, 0x84);
pub const AMBER_600: Color = Color::from_rgb_u8(0x9A, 0x67, 0x04);
pub const VIOLET_600: Color = Color::from_rgb_u8(0x7C, 0x5B, 0xC5);

pub fn row(
    kind: ItemKind,
    title: &str,
    preview: &str,
    board: &str,
    list: &str,
    color: Color,
) -> PickerRow {
    PickerRow {
        kind,
        title: title.into(),
        preview: preview.into(),
        board_name: board.into(),
        list_name: list.into(),
        list_color: color,
        ..Default::default()
    }
}

/// Seven rows from two boards covering the common kinds.
pub fn sample_rows() -> Vec<PickerRow> {
    vec![
        row(
            ItemKind::Text,
            "Thanks, received",
            "Thanks for sending this over. I'll review it today.",
            "Work",
            "Email replies",
            TEAL_600,
        ),
        row(
            ItemKind::Template,
            "Invoice reminder",
            "A gentle reminder that invoice {Number} is due on {Date}.",
            "Work",
            "Email replies",
            TEAL_600,
        ),
        row(
            ItemKind::Code,
            "Retry helper",
            "async fn retry<T>(f: impl Fn() -> T, n: usize) -> T",
            "Work",
            "Code",
            VIOLET_600,
        ),
        row(
            ItemKind::Link,
            "Design spec",
            "figma.com/file/snipdeck-index-deck",
            "Work",
            "Links",
            AMBER_600,
        ),
        PickerRow {
            masked_preview: "•••• •••• 7f3a".into(),
            ..row(
                ItemKind::Sensitive,
                "Server password",
                "hunter2-correct-horse",
                "Personal",
                "Logins",
                AMBER_600,
            )
        },
        PickerRow {
            extension: "PDF".into(),
            ..row(
                ItemKind::File,
                "Q3 report",
                "report-q3-final.pdf",
                "Personal",
                "Documents",
                TEAL_600,
            )
        },
        row(
            ItemKind::Text,
            "Home address",
            "12 Example Street, Springfield",
            "Personal",
            "Addresses",
            VIOLET_600,
        ),
    ]
}

/// `n` plain rows titled "Snippet 01", "Snippet 02", ...
pub fn numbered_rows(n: usize) -> Vec<PickerRow> {
    (1..=n)
        .map(|i| {
            row(
                ItemKind::Text,
                &format!("Snippet {i:02}"),
                &format!("Body of snippet number {i}"),
                "Work",
                "Notes",
                TEAL_600,
            )
        })
        .collect()
}

/// Long English titles and breadcrumbs, a long unbroken word and short text. Latin only: goldens must not
/// contain glyphs outside the bundled IBM Plex fonts.
pub fn awkward_rows() -> Vec<PickerRow> {
    vec![
        row(
            ItemKind::Code,
            "A very long snippet title that keeps going well past the width of the picker window and then some",
            "SELECT id, name FROM customers WHERE created_at > now() - interval '7 days'",
            "Client projects and contracts",
            "Quarterly reporting queries",
            VIOLET_600,
        ),
        row(
            ItemKind::Text,
            "Pneumonoultramicroscopicsilicovolcanoconiosis_and_other_unbreakable_words_in_a_title",
            "Short body",
            "Work",
            "Notes",
            TEAL_600,
        ),
        row(
            ItemKind::Text,
            "Short",
            "Short body",
            "Personal",
            "Long list name that needs to be shortened somewhere along the row",
            AMBER_600,
        ),
        row(ItemKind::Text, "No breadcrumb", "Body", "", "", TEAL_600),
    ]
}

pub fn set_rows(ui: &PickerWindow, rows: Vec<PickerRow>) {
    ui.set_rows(ModelRc::new(VecModel::from(rows)));
}

/// The usual populated Picker, pasting into Outlook.
pub fn populate(ui: &PickerWindow) {
    set_rows(ui, sample_rows());
    ui.set_target_name("Outlook".into());
    ui.set_target_title("New message".into());
}
