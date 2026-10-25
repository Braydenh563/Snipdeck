//! Sample Panel data shared by `tests/panel.rs` (goldens) and `tests/panel_logic.rs` (behaviour);
//! `#[path]`-included, so each test binary compiles its own copy.
#![allow(dead_code, reason = "each test binary uses a subset of the fixtures")]

use slint::{Color, ModelRc, VecModel};
use snipdeck_ui::{ItemKind, MenuEntry, PanelRow, PanelWindow, PinnedItem, TabItem};

/// Step-600 list hues (light theme) from `design/tokens/palette.json`.
pub const TEAL_600: Color = Color::from_rgb_u8(0x04, 0x84, 0x84);
pub const AMBER_600: Color = Color::from_rgb_u8(0x9A, 0x67, 0x04);
pub const VIOLET_600: Color = Color::from_rgb_u8(0x7C, 0x5B, 0xC5);

pub fn row(kind: ItemKind, title: &str, preview: &str) -> PanelRow {
    PanelRow {
        kind,
        title: title.into(),
        preview: preview.into(),
        ..Default::default()
    }
}

/// Six rows covering the common kinds, the first nine of which carry gutter numbers.
pub fn sample_rows() -> Vec<PanelRow> {
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
        row(
            ItemKind::Link,
            "Design spec",
            "figma.com/file/snipdeck-index-deck",
        ),
        PanelRow {
            masked_preview: "•••• •••• 7f3a".into(),
            ..row(
                ItemKind::Sensitive,
                "Server password",
                "hunter2-correct-horse",
            )
        },
        PanelRow {
            extension: "PDF".into(),
            ..row(ItemKind::File, "Q3 report", "report-q3-final.pdf")
        },
    ]
}

/// `n` plain rows titled "Snippet 01", "Snippet 02", ...
pub fn numbered_rows(n: usize) -> Vec<PanelRow> {
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

/// Long English titles, a long unbroken word, short text and a mixed-view list label. Latin only:
/// goldens must not contain glyphs outside the bundled IBM Plex fonts (system fallback fonts differ
/// per machine). CJK coverage is `cjk_rows`, used by logic tests that measure sizes, not pixels.
pub fn awkward_rows() -> Vec<PanelRow> {
    vec![
        row(
            ItemKind::Code,
            "A very long snippet title that keeps going well past the width of the panel",
            "SELECT id, name FROM customers WHERE created_at > now() - interval '7 days'",
        ),
        row(
            ItemKind::Text,
            "Please send the invoice",
            "Thank you for your help so far. Could you send last month's invoice when you have a moment? Payment reminder attached.",
        ),
        PanelRow {
            list_name: "Work and projects".into(),
            list_color: VIOLET_600,
            ..row(
                ItemKind::Text,
                "Meeting follow-up",
                "Following up on today's call.",
            )
        },
        row(ItemKind::Text, "Short", "Short body"),
    ]
}

/// Japanese and Chinese rows (long and short). Never rendered into a golden; logic tests measure them.
pub fn cjk_rows() -> Vec<PanelRow> {
    vec![
        row(
            ItemKind::Text,
            "請求書のお願い",
            "お世話になっております。先月分の請求書をお送りいただけますでしょうか。支付发票提醒",
        ),
        row(ItemKind::Text, "短い", "短い本文"),
        PanelRow {
            list_name: "仕事とプロジェクト".into(),
            list_color: VIOLET_600,
            ..row(
                ItemKind::Text,
                "会議のフォローアップ",
                "本日の会議の件でご連絡します。",
            )
        },
    ]
}

pub fn tabs() -> Vec<TabItem> {
    vec![
        TabItem {
            text: "Email replies".into(),
            accent: TEAL_600,
            icon: slint::Image::default(),
        },
        TabItem {
            text: "Addresses".into(),
            accent: AMBER_600,
            icon: slint::Image::default(),
        },
        TabItem {
            text: "Code".into(),
            accent: VIOLET_600,
            icon: slint::Image::default(),
        },
    ]
}

pub fn pinned() -> Vec<PinnedItem> {
    ["Thanks, received", "Home address", "Retry helper"]
        .map(|title| PinnedItem {
            title: title.into(),
            kind: ItemKind::Text,
        })
        .to_vec()
}

/// `n` pinned chips titled "Pinned 01", "Pinned 02", ...
pub fn pinned_n(n: usize) -> Vec<PinnedItem> {
    (1..=n)
        .map(|i| PinnedItem {
            title: format!("Pinned {i:02}").into(),
            kind: ItemKind::Text,
        })
        .collect()
}

/// The Panel's kebab menu as the app fills it: two actions, a separator, then the rest.
pub fn app_menu() -> Vec<MenuEntry> {
    let entry = |text: &str, shortcut: &str| MenuEntry {
        text: text.into(),
        shortcut: shortcut.into(),
        ..Default::default()
    };
    vec![
        entry("Open Snipdeck", ""),
        entry("Settings", "Ctrl+,"),
        MenuEntry {
            separator: true,
            ..Default::default()
        },
        entry("Pause hotkeys", ""),
        entry("Help", "F1"),
        entry("Quit", ""),
    ]
}

pub fn board_menu() -> Vec<MenuEntry> {
    ["Work", "Personal"]
        .map(|text| MenuEntry {
            text: text.into(),
            ..Default::default()
        })
        .to_vec()
}

pub fn set_rows(ui: &PanelWindow, rows: Vec<PanelRow>) {
    ui.set_rows(ModelRc::new(VecModel::from(rows)));
}

/// The usual populated Panel: board "Work", three lists, three pinned items, six rows, pasting into Outlook.
pub fn populate(ui: &PanelWindow) {
    ui.set_board_name("Work".into());
    ui.set_tabs(ModelRc::new(VecModel::from(tabs())));
    ui.set_pinned(ModelRc::new(VecModel::from(pinned())));
    ui.set_board_menu_entries(ModelRc::new(VecModel::from(board_menu())));
    ui.set_app_menu_entries(ModelRc::new(VecModel::from(app_menu())));
    set_rows(ui, sample_rows());
    ui.set_target_name("Outlook".into());
    ui.set_target_title("New message".into());
}
