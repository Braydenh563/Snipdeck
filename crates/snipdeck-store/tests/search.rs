//! Full-content search (FTS5) for the main window.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]

mod common;

use common::*;
use snipdeck_core::{Format, Formats, INLINE_LIMIT, Item, ItemKind, Placement};
use snipdeck_store::{BlobStore, ItemRepo, Store, TrashRepo, Trashable};

fn setup() -> (Store, snipdeck_core::List) {
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    (s, l)
}

fn found(s: &Store, q: &str) -> Vec<String> {
    s.search(q, 50)
        .unwrap()
        .into_iter()
        .map(|id| s.item(id).unwrap().title)
        .collect()
}

#[test]
fn matches_titles_and_full_content() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Invoice template", "Dear customer");
    item(
        &mut s,
        l.id,
        "Address",
        "Lorem ipsum dolor sit amet, the secret word is pineapple",
    );
    assert_eq!(found(&s, "invoice"), ["Invoice template"]);
    assert_eq!(
        found(&s, "pineapple"),
        ["Address"],
        "content beyond the preview is searched"
    );
}

#[test]
fn matching_is_case_and_accent_insensitive() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Café menu", "Crème brûlée");
    assert_eq!(found(&s, "CAFE"), ["Café menu"]);
    assert_eq!(found(&s, "creme brulee"), ["Café menu"]);
}

#[test]
fn the_last_word_matches_as_a_prefix_and_all_words_are_required() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Quarterly report", "revenue figures");
    item(&mut s, l.id, "Quarterly plan", "hiring");
    assert_eq!(found(&s, "quart").len(), 2);
    assert_eq!(found(&s, "quarterly rep"), ["Quarterly report"]);
    assert!(found(&s, "quarterly zebra").is_empty());
}

#[test]
fn title_matches_rank_above_body_matches() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Other", "mentions apple once");
    item(&mut s, l.id, "Apple", "fruit");
    assert_eq!(found(&s, "apple"), ["Apple", "Other"]);
}

#[test]
fn limit_caps_the_results() {
    let (mut s, l) = setup();
    for i in 0..10 {
        item(&mut s, l.id, &format!("note {i}"), "common");
    }
    assert_eq!(s.search("common", 3).unwrap().len(), 3);
    assert!(s.search("common", 0).unwrap().is_empty());
}

#[test]
fn trashed_items_are_not_found_until_restored() {
    let (mut s, l) = setup();
    let it = item(&mut s, l.id, "Temp", "ephemeral");
    s.trash(Trashable::Item(it.id), at(1)).unwrap();
    assert!(found(&s, "ephemeral").is_empty());
    s.restore(Trashable::Item(it.id)).unwrap();
    assert_eq!(found(&s, "ephemeral"), ["Temp"]);
}

#[test]
fn sensitive_items_match_on_their_title_only() {
    let (mut s, l) = setup();
    let mut it = Item::new(l.id, "Bank login".to_owned(), ItemKind::Text, at(0));
    it.sensitive = true;
    s.insert_item(&it, &text_formats("hunter2 correct horse"), Placement::Last)
        .unwrap();
    assert_eq!(found(&s, "bank"), ["Bank login"]);
    assert!(found(&s, "hunter2").is_empty());
    assert!(found(&s, "horse").is_empty());
}

#[test]
fn flipping_the_sensitive_flag_reindexes_the_content() {
    let (mut s, l) = setup();
    let mut it = item(&mut s, l.id, "Token", "abc123token");
    assert_eq!(found(&s, "abc123token").len(), 1);
    it.sensitive = true;
    it.preview.clear();
    s.update_item(&it, None).unwrap();
    assert!(
        found(&s, "abc123token").is_empty(),
        "no content left in the index"
    );
    it.sensitive = false;
    it.preview = Item::preview_of("abc123token", false);
    s.update_item(&it, None).unwrap();
    assert_eq!(
        found(&s, "abc123token").len(),
        1,
        "content comes back from storage"
    );
    assert_healthy(&s);
}

#[test]
fn edits_to_title_and_content_are_searchable_immediately() {
    let (mut s, l) = setup();
    let mut it = item(&mut s, l.id, "Before", "old text");
    it.title = "After".to_owned();
    s.update_item(&it, Some(&text_formats("new words")))
        .unwrap();
    assert!(found(&s, "before").is_empty() && found(&s, "old").is_empty());
    assert_eq!(found(&s, "after"), ["After"]);
    assert_eq!(found(&s, "words"), ["After"]);
    // A title-only edit keeps the body searchable.
    it.title = "Renamed".to_owned();
    s.update_item(&it, None).unwrap();
    assert_eq!(found(&s, "words"), ["Renamed"]);
}

#[test]
fn text_stored_as_a_blob_is_searchable() {
    let (mut s, l) = setup();
    let mut body = "filler ".repeat(INLINE_LIMIT / 4);
    body.push_str("needleinhaystack");
    let content = s.put(body.into_bytes()).unwrap();
    let it = Item::new(l.id, "Long".to_owned(), ItemKind::Text, at(0));
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Text(content)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    assert_eq!(found(&s, "needleinhaystack"), ["Long"]);
}

#[test]
fn items_without_text_are_found_by_title() {
    let (mut s, l) = setup();
    let it = Item::new(l.id, "Logo".to_owned(), ItemKind::Image, at(0));
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Png(text("png"))]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    assert_eq!(found(&s, "logo"), ["Logo"]);
}

#[test]
fn no_user_input_produces_a_syntax_error() {
    let (mut s, l) = setup();
    item(
        &mut s,
        l.id,
        "AND OR NOT",
        "title:foo \"quoted\" NEAR(a b) c*",
    );
    let nasty = [
        "",
        " ",
        "\"",
        "\"\"",
        "\"unterminated",
        "*",
        "**",
        "foo*",
        "-",
        "-foo",
        "+",
        "^",
        "(",
        ")",
        "()",
        "a AND",
        "OR",
        "NOT",
        "AND OR NOT",
        "NEAR(",
        "NEAR(a b)",
        "title:",
        "title:foo",
        "{title}: foo",
        "body : x",
        "a OR b",
        "'",
        "''",
        "\\",
        ";",
        "--",
        "/* */",
        "a\0b",
        "\u{1F600}",
        "日本語",
        ":",
        "::",
        "x:y:z",
        "\"a\" \"b\"",
        "a*b",
        "\n\t",
        "%",
        "_",
        "[",
        "]",
        "{",
        "}",
        "|",
        "&",
        "!",
        "?",
        "~",
        "`",
        "@",
        "#",
        "$",
    ];
    for query in nasty {
        let result = s.search(query, 10);
        assert!(result.is_ok(), "{query:?} -> {result:?}");
    }
    let long = "word ".repeat(5000);
    assert!(s.search(&long, 10).is_ok());
    assert_eq!(
        found(&s, "title:foo").len(),
        1,
        "operators are searched as words, not run"
    );
    assert_eq!(found(&s, "\"quoted\"").len(), 1);
}

#[test]
fn an_empty_query_finds_nothing() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "x", "y");
    assert!(s.search("", 10).unwrap().is_empty());
    assert!(s.search("   ", 10).unwrap().is_empty());
    assert!(s.search("!!!", 10).unwrap().is_empty());
}
