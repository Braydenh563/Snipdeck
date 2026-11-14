//! Unit tests of the crate's plain-Rust modules (fonts, selection). They live here rather than in
//! `src/` so that `cargo test` never builds a second, test-flavoured copy of the generated Slint
//! code (`[lib] test = false`): compiling it twice at once runs CI runners out of memory.

use snipdeck_ui::{
    SelectMode,
    fonts::{cjk_font_candidates, cjk_font_family},
    selection::Selection,
};

#[test]
fn launch_locales_map_to_the_families_of_the_design() {
    assert_eq!(cjk_font_candidates("ja")[0], "Yu Gothic UI");
    assert_eq!(cjk_font_candidates("ja-JP")[0], "Yu Gothic UI");
    assert_eq!(cjk_font_candidates("zh-Hans")[0], "Microsoft YaHei UI");
    assert_eq!(cjk_font_candidates("zh_CN")[0], "Microsoft YaHei UI");
    assert_eq!(cjk_font_candidates("zh-Hant")[0], "Microsoft JhengHei UI");
    assert_eq!(cjk_font_candidates("ko")[0], "Malgun Gothic");
    for latin in ["en", "de", "fr", "es", "pt-BR", ""] {
        assert!(cjk_font_candidates(latin).is_empty(), "{latin}");
    }
}

#[test]
fn the_first_installed_family_wins_and_none_means_no_change() {
    let only = |name: &'static str| move |f: &str| f == name;
    assert_eq!(
        cjk_font_family("ja", only("Noto Sans CJK JP")),
        "Noto Sans CJK JP"
    );
    assert_eq!(cjk_font_family("ja", |_| true), "Yu Gothic UI");
    assert_eq!(cjk_font_family("ja", |_| false), "");
    assert_eq!(cjk_font_family("de", |_| true), "");
}

fn picked(sel: &Selection) -> Vec<usize> {
    sel.indices().collect()
}

#[test]
fn replace_selects_one_row_and_sets_the_anchor() {
    let mut sel = Selection::new();
    sel.select(2, SelectMode::Replace, 5);
    sel.select(4, SelectMode::Replace, 5);
    assert_eq!(picked(&sel), [4]);
}

#[test]
fn toggle_adds_and_removes_without_touching_the_rest() {
    let mut sel = Selection::new();
    sel.select(1, SelectMode::Replace, 5);
    sel.select(3, SelectMode::Toggle, 5);
    assert_eq!(picked(&sel), [1, 3]);
    sel.select(1, SelectMode::Toggle, 5);
    assert_eq!(picked(&sel), [3]);
}

#[test]
fn extend_selects_the_range_from_the_anchor_in_both_directions() {
    let mut sel = Selection::new();
    sel.select(2, SelectMode::Replace, 8);
    sel.select(5, SelectMode::Extend, 8);
    assert_eq!(picked(&sel), [2, 3, 4, 5]);
    // The anchor stays at 2: extending back past it flips the range.
    sel.select(0, SelectMode::Extend, 8);
    assert_eq!(picked(&sel), [0, 1, 2]);
}

#[test]
fn extend_after_a_toggle_starts_from_the_toggled_row() {
    let mut sel = Selection::new();
    sel.select(0, SelectMode::Replace, 8);
    sel.select(6, SelectMode::Toggle, 8);
    sel.select(4, SelectMode::Extend, 8);
    assert_eq!(picked(&sel), [4, 5, 6]);
}

#[test]
fn out_of_range_indices_are_ignored_and_clamp_drops_vanished_rows() {
    let mut sel = Selection::new();
    sel.select(9, SelectMode::Replace, 5);
    assert_eq!(sel.len(), 0);
    sel.select_all(5);
    assert_eq!(sel.len(), 5);
    sel.clamp(3);
    assert_eq!(picked(&sel), [0, 1, 2]);
    sel.clear();
    assert_eq!(sel.len(), 0);
}
