//! UX-19: the Main window and the Panel offer the same item menu. One definition
//! (`item_menu::SHARED`) fixes the shared rows and their order; the Panel's Rust model and the
//! Main window's `.slint` rows are both checked against it.

use super::*;
use crate::controller::item_menu::{MAIN_ONLY, PANEL_ONLY, SHARED, Shared, shared_of};

const MAIN_SLINT: &str = include_str!("../../../../snipdeck-ui/ui/main_window.slint");
const MENU_TEXTS: &str = include_str!("../../../../snipdeck-ui/ui/menu_texts.slint");
const TOAST_TEXTS: &str = include_str!("../../../../snipdeck-ui/ui/toast_texts.slint");

/// The English of `MenuTexts.<func>()` (or `ToastTexts`), read from its `.slint` source.
fn english(func: &str) -> String {
    for src in [MENU_TEXTS, TOAST_TEXTS] {
        if let Some(at) = src.find(&format!("function {func}() -> string")) {
            let rest = &src[at..];
            let start = rest.find("@tr(\"").expect("tr") + 5;
            let end = start + rest[start..].find('"').expect("quote");
            return rest[start..end].to_string();
        }
    }
    panic!("no text function {func}");
}

/// The rows of the Main window's item menu (not the Trash one), as the source of each `text:`.
fn main_rows() -> Vec<String> {
    let at = MAIN_SLINT
        .find("property <[MenuEntry]> item-entries")
        .expect("item-entries");
    let rest = &MAIN_SLINT[at..];
    let from = rest.find("] : [").expect("normal menu") + 5;
    let body = &rest[from..rest[from..].find("\n    ];").expect("end") + from];
    body.lines()
        .filter(|l| l.trim_start().starts_with('{'))
        .map(|l| {
            if l.contains("separator: true") {
                "---".to_string()
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// Whether a `.slint` row line carries the words `want` (through `MenuTexts` or the same msgid).
fn row_says(line: &str, func: &str) -> bool {
    (func == "item-use" && line.contains("root.use-label"))
        || line.contains(&format!("MenuTexts.{func}()"))
        || line.contains(&format!("@tr(\"{}\")", english(func)))
}

#[test]
fn the_panel_item_menu_has_the_shared_rows_in_the_shared_order() {
    let w = build(Opts::default());
    let id = w.ids[0];
    let kinds: Vec<Shared> = w
        .ctl
        .item_menu_entries(id)
        .into_iter()
        .filter_map(shared_of)
        .collect();
    let want: Vec<Shared> = SHARED
        .iter()
        .map(|s| s.kind)
        // Only text items can be pasted as, and only colours open in Colours.
        .filter(|k| !matches!(k, Shared::OpenInColours))
        .collect();
    assert_eq!(kinds, want);
    // Everything the Panel has that is not shared is listed, with a reason, as Panel-only.
    let panel_only = w
        .ctl
        .item_menu_entries(id)
        .into_iter()
        .filter(|e| shared_of(*e).is_none() && *e != ItemMenuEntry::Separator)
        .count();
    assert_eq!(panel_only, PANEL_ONLY.len(), "{PANEL_ONLY:?}");
}

#[test]
fn the_main_item_menu_has_the_shared_rows_in_the_shared_order_with_the_same_words() {
    let rows = main_rows();
    let mut at = 0;
    for shared in SHARED {
        let line = rows[at..]
            .iter()
            .position(|l| row_says(l, shared.words))
            .map_or_else(
                || {
                    panic!(
                        "Main lacks {:?} ({}) after row {at}: {rows:#?}",
                        shared.kind, shared.words
                    )
                },
                |p| p + at,
            );
        at = line + 1;
    }
    // Every Main row is a shared one, a separator or listed as Main-only.
    for (i, line) in rows.iter().enumerate() {
        let known = line == "---"
            || SHARED.iter().any(|s| row_says(line, s.words))
            || row_says(line, "item-unpin")
            || MAIN_ONLY.iter().any(|(words, _)| line.contains(*words));
        assert!(
            known,
            "row {i} is neither shared nor listed as Main-only: {line}"
        );
    }
}
