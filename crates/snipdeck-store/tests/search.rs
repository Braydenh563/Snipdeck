//! The full-text index: for each word of a parsed query, the live items whose title or content
//! holds it (trigram FTS5 over folded text; `LIKE` for one or two CJK characters).
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

/// The titles of the items [`common::found`] returns, sorted.
fn titles_found(s: &Store, q: &str) -> Vec<String> {
    let mut titles: Vec<String> = found(s, q)
        .into_iter()
        .map(|id| s.item(id).unwrap().title)
        .collect();
    titles.sort();
    titles
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
    assert_eq!(titles_found(&s, "invoice"), ["Invoice template"]);
    assert_eq!(
        titles_found(&s, "pineapple"),
        ["Address"],
        "content beyond the preview is searched"
    );
}

#[test]
fn matching_is_case_and_accent_insensitive() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Café menu", "Crème brûlée");
    assert_eq!(titles_found(&s, "CAFE"), ["Café menu"]);
    assert_eq!(titles_found(&s, "creme brulee"), ["Café menu"]);
}

#[test]
fn words_match_anywhere_in_a_word_and_all_words_are_required() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Quarterly report", "revenue figures");
    item(&mut s, l.id, "Email signature", "hiring");
    assert_eq!(titles_found(&s, "quart"), ["Quarterly report"]);
    assert_eq!(titles_found(&s, "ature"), ["Email signature"], "mid-word");
    assert_eq!(titles_found(&s, "terly port"), ["Quarterly report"]);
    assert_eq!(titles_found(&s, "quarterly zebra").len(), 0);
    assert_eq!(titles_found(&s, "\"revenue fig\""), ["Quarterly report"]);
    assert!(
        titles_found(&s, "\"figures revenue\"").is_empty(),
        "a phrase keeps its order"
    );
}

#[test]
fn text_in_scripts_without_spaces_is_found_inside_sentences() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "ja", "東京都の天気は晴れです");
    item(&mut s, l.id, "zh", "我们明天在北京见面");
    item(&mut s, l.id, "ko", "안녕하세요 반갑습니다");
    item(&mut s, l.id, "th", "สวัสดีครับยินดีต้อนรับ");
    assert_eq!(titles_found(&s, "天気"), ["ja"], "two characters");
    assert_eq!(titles_found(&s, "晴"), ["ja"], "one character");
    assert_eq!(titles_found(&s, "北京"), ["zh"]);
    assert_eq!(titles_found(&s, "반갑"), ["ko"]);
    assert_eq!(titles_found(&s, "ยินดี"), ["th"]);
    assert_eq!(
        titles_found(&s, "都の天"),
        ["ja"],
        "three characters use the index"
    );
}

#[test]
fn symbols_and_width_variants_are_found() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "Lang", "notes on C++ and C# 50% off_sale");
    item(&mut s, l.id, "Wide", "ＡＢＣＤ ﾃｽﾄ");
    assert_eq!(titles_found(&s, "c++"), ["Lang"]);
    assert_eq!(titles_found(&s, "50%"), ["Lang"]);
    assert_eq!(titles_found(&s, "f_s"), ["Lang"], "LIKE wildcards are text");
    assert_eq!(titles_found(&s, "abcd"), ["Wide"]);
    assert_eq!(titles_found(&s, "テスト"), ["Wide"]);
}

#[test]
fn short_latin_words_are_left_to_the_in_memory_fields() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "x", "ab cd");
    let query = snipdeck_core::search::Query::parse("ab");
    assert!(s.search(&query).unwrap().term(0).is_none());
}

#[test]
fn trashed_items_are_not_found_until_restored() {
    let (mut s, l) = setup();
    let it = item(&mut s, l.id, "Temp", "ephemeral");
    s.trash(Trashable::Item(it.id), at(1)).unwrap();
    assert_eq!(titles_found(&s, "ephemeral").len(), 0);
    s.restore(Trashable::Item(it.id)).unwrap();
    assert_eq!(titles_found(&s, "ephemeral"), ["Temp"]);
}

#[test]
fn sensitive_items_match_on_their_title_only() {
    let (mut s, l) = setup();
    let mut it = Item::new(l.id, "Bank login".to_owned(), ItemKind::Text, at(0));
    it.sensitive = true;
    s.insert_item(&it, &text_formats("hunter2 correct horse"), Placement::Last)
        .unwrap();
    assert_eq!(titles_found(&s, "bank"), ["Bank login"]);
    assert_eq!(titles_found(&s, "hunter2").len(), 0);
    assert_eq!(titles_found(&s, "horse").len(), 0);
}

#[test]
fn flipping_the_sensitive_flag_reindexes_the_content() {
    let (mut s, l) = setup();
    let mut it = item(&mut s, l.id, "Token", "abc123token");
    assert_eq!(titles_found(&s, "abc123token").len(), 1);
    it.sensitive = true;
    it.preview.clear();
    s.update_item(&it, None).unwrap();
    assert!(
        titles_found(&s, "abc123token").is_empty(),
        "no content left in the index"
    );
    it.sensitive = false;
    it.preview = Item::preview_of("abc123token", false);
    s.update_item(&it, None).unwrap();
    assert_eq!(
        titles_found(&s, "abc123token").len(),
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
    assert!(titles_found(&s, "before").is_empty() && titles_found(&s, "old").is_empty());
    assert_eq!(titles_found(&s, "after"), ["After"]);
    assert_eq!(titles_found(&s, "words"), ["After"]);
    // A title-only edit keeps the body searchable.
    it.title = "Renamed".to_owned();
    s.update_item(&it, None).unwrap();
    assert_eq!(titles_found(&s, "words"), ["Renamed"]);
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
    assert_eq!(titles_found(&s, "needleinhaystack"), ["Long"]);
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
    assert_eq!(titles_found(&s, "logo"), ["Logo"]);
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
        let parsed = snipdeck_core::search::Query::parse(query);
        let result = s.search(&parsed);
        assert!(result.is_ok(), "{query:?} -> {result:?}");
    }
    let long = snipdeck_core::search::Query::parse(&"word ".repeat(5000));
    assert!(s.search(&long).is_ok());
    assert_eq!(
        titles_found(&s, "title:foo").len(),
        1,
        "operators are searched as words, not run"
    );
    assert_eq!(titles_found(&s, "\"quoted\"").len(), 1);
}

#[test]
fn an_empty_query_finds_nothing() {
    let (mut s, l) = setup();
    item(&mut s, l.id, "x", "y");
    assert_eq!(found(&s, "").len(), 0);
    assert_eq!(found(&s, "   ").len(), 0);
    assert_eq!(found(&s, "!!!").len(), 0);
}
