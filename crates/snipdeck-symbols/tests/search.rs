//! Logic tests for the symbols crate (search).
#![allow(clippy::unwrap_used, reason = "test helper outside a #[test] fn")]
use snipdeck_symbols::{Catalog, Kind, Lang, Rank, SymbolId, fold, search};

fn id(hit: &snipdeck_symbols::Hit) -> SymbolId {
    Catalog::get().symbol(hit.index).unwrap().id.clone()
}

fn find(q: &str, lang: Lang, kind: Option<Kind>) -> Vec<SymbolId> {
    search(Catalog::get(), q, lang, kind)
        .iter()
        .map(id)
        .collect()
}

fn emoji(seq: &str) -> SymbolId {
    SymbolId::Emoji(seq.into())
}

#[test]
fn search_cat_ranks_cat_face_before_cat_tagged_emoji() {
    let hits = search(Catalog::get(), "cat", Lang::En, Some(Kind::Emoji));
    let pos = |seq: &str| hits.iter().position(|h| id(h) == emoji(seq)).unwrap();
    let (cat, cat_face, grinning_cat, tiger) =
        (pos("1f408"), pos("1f431"), pos("1f63a"), pos("1f405"));
    assert_eq!(hits[cat].rank, Rank::ExactName);
    assert_eq!(hits[cat_face].rank, Rank::NamePrefix);
    assert!(
        cat < cat_face && cat_face < grinning_cat,
        "exact, then name prefix, then name word"
    );
    // 🐅 tiger is only keyword-related at best: it must come after every name match.
    assert!(grinning_cat < tiger || !hits.iter().any(|h| id(h) == emoji("1f405")));
    assert!(
        hits.windows(2).all(|w| w[0].rank <= w[1].rank),
        "sorted by rank"
    );
}

#[test]
fn search_folds_accents_and_kana() {
    // French "café" is a keyword of ☕; "cafe" has no accent.
    assert!(find("cafe", Lang::Fr, Some(Kind::Emoji)).contains(&emoji("2615")));
    assert!(find("CAFÉ", Lang::Fr, None).contains(&emoji("2615")));
    // Hiragana and katakana are the same to the search; so are full-width letters.
    let hira = find("ねこ", Lang::Ja, None);
    assert!(hira.contains(&emoji("1f408")));
    assert_eq!(hira, find("ネコ", Lang::Ja, None));
    assert_eq!(fold("ＣＡＴ Café ネコ"), "cat cafe ねこ");
    assert_eq!(
        find("ｃａｔ", Lang::En, Some(Kind::Emoji)),
        find("cat", Lang::En, Some(Kind::Emoji))
    );
}

#[test]
fn search_terms_are_anded() {
    let both = find("cat face", Lang::En, Some(Kind::Emoji));
    assert!(both.contains(&emoji("1f431")));
    assert!(!both.contains(&emoji("1f408")), "plain 🐈 has no 'face'");
    let cat = find("cat", Lang::En, Some(Kind::Emoji));
    assert!(cat.len() > both.len());
    // Order of terms does not matter; unknown terms empty the result.
    assert_eq!(
        find("face cat", Lang::En, Some(Kind::Emoji)).len(),
        both.len()
    );
    assert_eq!(find("cat zzzzqq", Lang::En, None).len(), 0);
}

#[test]
fn search_finds_an_emoji_pasted_as_the_query() {
    for (q, seq) in [
        ("😀", "1f600"),
        ("🚀", "1f680"),
        ("🧑\u{200d}💻", "1f9d1-200d-1f4bb"),
    ] {
        let hits = search(Catalog::get(), q, Lang::En, None);
        assert_eq!(id(&hits[0]), emoji(seq), "{q}");
        assert_eq!(hits[0].rank, Rank::ExactName);
    }
    // With or without the emoji presentation selector.
    assert_eq!(
        find("\u{2764}", Lang::En, None).first(),
        Some(&emoji("2764-fe0f"))
    );
    assert_eq!(
        find("\u{2764}\u{fe0f}", Lang::En, None).first(),
        Some(&emoji("2764-fe0f"))
    );
}

#[test]
fn kaomoji_shrug_finds_the_shrug() {
    let hits = find("shrug", Lang::En, Some(Kind::Kaomoji));
    assert!(hits.contains(&SymbolId::Kaomoji("shrug-classic".into())));
    assert!(hits.iter().all(|h| h.kind() == Kind::Kaomoji));
    // Category names are keywords too, and the kaomoji can be pasted.
    assert!(find("whatever", Lang::En, None).contains(&SymbolId::Kaomoji("shrug-classic".into())));
    let pasted = find("¯\\_(ツ)_/¯", Lang::En, None);
    assert_eq!(
        pasted.first(),
        Some(&SymbolId::Kaomoji("shrug-classic".into()))
    );
}

#[test]
fn icon_trash_finds_trash_and_tags() {
    let hits = search(Catalog::get(), "trash", Lang::En, Some(Kind::Icon));
    assert_eq!(id(&hits[0]), SymbolId::Icon("trash".into()));
    assert_eq!(hits[0].rank, Rank::ExactName);
    assert_eq!(id(&hits[1]), SymbolId::Icon("trash-simple".into()));
    // A tag finds it too, in any language.
    for lang in [Lang::En, Lang::Ja] {
        assert!(find("garbage", lang, Some(Kind::Icon)).contains(&SymbolId::Icon("trash".into())));
    }
    // A hyphenated name is searched as words.
    assert!(
        find("arrow clockwise", Lang::En, Some(Kind::Icon))
            .contains(&SymbolId::Icon("arrow-clockwise".into()))
    );
}

#[test]
fn empty_query_is_browse_order() {
    let c = Catalog::get();
    let all = search(c, "  ", Lang::En, None);
    assert_eq!(all.len(), c.len());
    assert!(
        all.iter()
            .enumerate()
            .all(|(i, h)| h.index == i && h.rank == Rank::Browse)
    );
    let kao = search(c, "", Lang::En, Some(Kind::Kaomoji));
    assert_eq!(kao.len(), c.range(Kind::Kaomoji).len());
    assert_eq!(kao[0].index, c.range(Kind::Kaomoji).start);
}

#[test]
fn substring_matches_rank_last_and_need_three_characters() {
    // "ogni" sits inside "recognition"-like words only, never at a word start.
    let sub = search(Catalog::get(), "iling", Lang::En, Some(Kind::Emoji));
    assert_ne!(sub.len(), 0);
    assert!(sub.iter().all(|h| h.rank == Rank::Substring));
    assert!(
        search(Catalog::get(), "il", Lang::En, Some(Kind::Emoji))
            .iter()
            .all(|h| h.rank != Rank::Substring)
    );
}
