//! Sample Main window data shared by `tests/main_window.rs` (goldens) and `tests/main_window_logic.rs`
//! (behaviour); `#[path]`-included, so each test binary compiles its own copy.
#![allow(dead_code, reason = "each test binary uses a subset of the fixtures")]

use std::rc::Rc;

use slint::{Color, ModelRc, VecModel};
use snipdeck_ui::{ChoiceEntry, ItemKind, MainRow, MainWindow, MenuEntry};

/// Step-600 list hues (light theme) from `design/tokens/palette.json`.
pub const TEAL_600: Color = Color::from_rgb_u8(0x04, 0x84, 0x84);
pub const AMBER_600: Color = Color::from_rgb_u8(0x9A, 0x67, 0x04);
pub const VIOLET_600: Color = Color::from_rgb_u8(0x7C, 0x5B, 0xC5);

pub fn row(kind: ItemKind, title: &str, preview: &str) -> MainRow {
    MainRow {
        kind,
        title: title.into(),
        preview: preview.into(),
        ..Default::default()
    }
}

/// Eight rows covering every kind.
pub fn sample_rows() -> Vec<MainRow> {
    vec![
        row(
            ItemKind::Text,
            "Thanks, received",
            "Thanks for sending this over. I'll review it today and get back to you.",
        ),
        row(
            ItemKind::Template,
            "Invoice reminder",
            "A gentle reminder that invoice {Number} is due on {Date}.",
        ),
        row(
            ItemKind::Code,
            "Retry helper",
            "async fn retry<T>(f: impl Fn() -> T, n: usize) -> T",
        ),
        MainRow {
            pinned: true,
            ..row(
                ItemKind::Link,
                "Design spec",
                "figma.com/file/snipdeck-index-deck",
            )
        },
        row(
            ItemKind::Image,
            "Screenshot 14:02",
            "1280 x 720 PNG, copied from the browser",
        ),
        MainRow {
            extension: "PDF".into(),
            ..row(ItemKind::File, "Q3 report", "report-q3-final.pdf")
        },
        MainRow {
            masked_preview: "•••• •••• 7f3a".into(),
            ..row(
                ItemKind::Sensitive,
                "Server password",
                "hunter2-correct-horse",
            )
        },
        row(
            ItemKind::Text,
            "Out of office",
            "I'm out until Monday with limited access to email. For anything urgent, contact the team inbox.",
        ),
    ]
}

/// `n` plain rows titled "Snippet 01", "Snippet 02", ...
pub fn numbered_rows(n: usize) -> Vec<MainRow> {
    (1..=n)
        .map(|i| {
            row(
                ItemKind::Text,
                &format!("Snippet {i:02}"),
                &format!("Body of snippet number {i}, long enough to need a second line in a comfortable card."),
            )
        })
        .collect()
}

/// Long titles, a long unbroken word, short text and a mixed-view list label. Latin only: goldens must not
/// contain glyphs outside the bundled IBM Plex fonts.
pub fn awkward_rows() -> Vec<MainRow> {
    vec![
        row(
            ItemKind::Code,
            "A very long snippet title that keeps going well past the width of the column",
            "SELECT id, name FROM customers WHERE created_at > now() - interval '7 days'",
        ),
        row(
            ItemKind::Text,
            "Please send the invoice",
            "Thank you for your help so far. Could you send last month's invoice when you have a moment? Payment reminder attached.",
        ),
        MainRow {
            list_name: "Work and projects".into(),
            list_color: VIOLET_600,
            ..row(
                ItemKind::Text,
                "Meeting follow-up",
                "Following up on today's call.",
            )
        },
        row(
            ItemKind::Text,
            "Supercalifragilisticexpialidocious_Supercalifragilisticexpialidocious_Snippet",
            "https://example.com/a/very/long/unbroken/path/that/never/finds/a/place/to/wrap",
        ),
        row(ItemKind::Text, "Short", "Short body"),
    ]
}

/// Japanese and Chinese rows. Never rendered into a golden; logic tests measure them.
pub fn cjk_rows() -> Vec<MainRow> {
    vec![
        row(
            ItemKind::Text,
            "請求書のお願い",
            "お世話になっております。先月分の請求書をお送りいただけますでしょうか。支付发票提醒",
        ),
        row(ItemKind::Text, "短い", "短い本文"),
    ]
}

pub fn lists() -> Vec<ChoiceEntry> {
    [
        ("Email replies", 24, TEAL_600),
        ("Addresses", 6, AMBER_600),
        ("Code", 12, VIOLET_600),
    ]
    .map(|(label, count, color)| ChoiceEntry {
        label: label.into(),
        count,
        color,
        icon: slint::Image::default(),
        checked: false,
    })
    .to_vec()
}

pub fn tags() -> Vec<ChoiceEntry> {
    [("billing", 9), ("meeting", 4), ("sql", 3)]
        .map(|(label, count)| ChoiceEntry {
            label: label.into(),
            count,
            color: Color::default(),
            icon: slint::Image::default(),
            checked: false,
        })
        .to_vec()
}

pub fn board_menu() -> Vec<MenuEntry> {
    ["Work", "Personal"]
        .map(|text| MenuEntry {
            text: text.into(),
            ..Default::default()
        })
        .to_vec()
}

pub fn set_rows(ui: &MainWindow, rows: Vec<MainRow>) -> Rc<VecModel<MainRow>> {
    let model = Rc::new(VecModel::from(rows));
    ui.set_rows(ModelRc::from(model.clone()));
    model
}

/// The usual populated window: board "Work", three lists, three tags, eight rows, a frozen "all items" view.
pub fn populate(ui: &MainWindow) -> Rc<VecModel<MainRow>> {
    ui.set_board_name("Work".into());
    ui.set_lists(ModelRc::new(VecModel::from(lists())));
    ui.set_tags(ModelRc::new(VecModel::from(tags())));
    ui.set_board_menu_entries(ModelRc::new(VecModel::from(board_menu())));
    ui.set_count_all(42);
    ui.set_count_pinned(3);
    ui.set_count_recent(0);
    ui.set_count_trash(2);
    set_rows(ui, sample_rows())
}
