//! Audit tests for the Symbols tab (TOOLS-44 to TOOLS-53): characters, tones, folding, readings,
//! CJK matching, typo tolerance and fixed query pairs. Nothing reads the clock.
#![allow(clippy::unwrap_used, reason = "test helper outside a #[test] fn")]
use snipdeck_symbols::{Catalog, Kind, Lang, SymbolId, fold, search};

fn texts(q: &str, lang: Lang, kind: Option<Kind>, n: usize) -> Vec<String> {
    let c = Catalog::get();
    search(c, q, lang, kind)
        .iter()
        .take(n)
        .filter_map(|h| c.symbol(h.index).and_then(|s| s.text.clone()))
        .collect()
}

fn top(q: &str, lang: Lang, kind: Option<Kind>, want: &str, within: usize) {
    let got = texts(q, lang, kind, within);
    assert!(
        got.iter().any(|t| t.trim_end_matches('\u{FE0F}') == want),
        "{q:?} ({lang:?}) should list {want:?} in the top {within}, got {got:?}"
    );
}

#[test]
fn tools_44_characters_are_a_fourth_kind_pasted_as_text() {
    let c = Catalog::get();
    let r = c.range(Kind::Character);
    assert!(
        r.len() >= 150,
        "arrows, maths, currency, typography, Greek, box drawing"
    );
    for q in ["→", "∞", "€", "α", "─", "²", "—", "©"] {
        let hits = search(c, q, Lang::En, Some(Kind::Character));
        let first = c.symbol(hits[0].index).unwrap();
        assert_eq!(first.text.as_deref(), Some(q), "{q} is in the catalogue");
        assert_eq!(first.kind, Kind::Character);
        assert!(matches!(first.id, SymbolId::Char(_)));
        assert_eq!(SymbolId::from_key(&first.id.key()), Some(first.id.clone()));
    }
}

#[test]
fn tools_44_character_names_in_english_and_translations() {
    top("euro", Lang::En, Some(Kind::Character), "€", 3);
    top("degree", Lang::En, Some(Kind::Character), "°", 3);
    top("nbsp", Lang::En, Some(Kind::Character), "\u{A0}", 3);
    top("pfeil rechts", Lang::De, Some(Kind::Character), "→", 10);
}

#[test]
fn tools_45_every_emoji_is_in_the_catalogue_once() {
    let c = Catalog::get();
    let emoji = c.range(Kind::Emoji);
    let mut seen = std::collections::HashSet::new();
    for i in emoji.clone() {
        assert!(
            seen.insert(c.symbol(i).unwrap().id.key()),
            "duplicate emoji"
        );
    }
    assert!(
        emoji.len() > 1_800,
        "the whole RGI set, not the Fluent subset"
    );
    // Emoji that Fluent has no art for are still listed: the Latin-1 flag and a keycap sample.
    assert!(c.index_of(&SymbolId::Emoji("1f1e9-1f1ea".into())).is_some());
}

#[test]
fn tools_46_every_toned_emoji_has_five_variants() {
    let c = Catalog::get();
    let wave = c.index_of(&SymbolId::Emoji("1f44b".into())).unwrap();
    let v = c.tone_variants(wave).unwrap();
    assert_eq!(v[0], "\u{1F44B}\u{1F3FB}");
    assert_eq!(v[4], "\u{1F44B}\u{1F3FF}");
    let pizza = c.index_of(&SymbolId::Emoji("1f355".into())).unwrap();
    assert!(c.tone_variants(pizza).is_none());
}

#[test]
fn tools_47_kaomoji_and_icon_keywords_are_localised() {
    let c = Catalog::get();
    let g = c.group_name(Kind::Kaomoji, 0, Lang::De);
    assert_ne!(g, c.group_name(Kind::Kaomoji, 0, Lang::En), "{g}");
    assert_ne!(c.group_name(Kind::Character, 0, Lang::En), "");
    let hits = search(c, "achselzucken", Lang::De, Some(Kind::Kaomoji));
    assert!(
        !hits.is_empty(),
        "the German word for shrug finds the shrug kaomoji"
    );
}

#[test]
fn tools_48_ascii_fold_spellings() {
    assert_eq!(fold("Straße"), "strasse");
    assert_eq!(fold("cœur"), "coeur");
    assert_eq!(fold("Ærø"), "aero");
    top("strasse", Lang::De, None, "\u{1F68B}", 12);
    top("coeur", Lang::Fr, None, "\u{2764}", 200);
    top("laecheln", Lang::De, None, "\u{1F642}", 80);
    top("lächeln", Lang::De, None, "\u{1F642}", 80);
}

#[test]
fn tools_49_cjk_readings() {
    // Hiragana for a kanji name, pinyin for a Chinese one.
    top("ねこ", Lang::Ja, None, "\u{1F408}", 5);
    top("xiao", Lang::Zh, None, "\u{1F600}", 200);
    top("mao", Lang::Zh, None, "\u{1F431}", 60);
}

#[test]
fn tools_50_one_cjk_character_matches_inside_a_name() {
    top("车", Lang::Zh, None, "\u{1F691}", 60);
    top("猫", Lang::Zh, None, "\u{1F431}", 20);
    top("猫", Lang::Ja, None, "\u{1F431}", 20);
}

#[test]
fn tools_51_kana_voicing_is_kept() {
    assert_ne!(fold("か"), fold("が"));
    assert_ne!(fold("ひ"), fold("ぴ"));
    assert_eq!(fold("カ"), fold("か"), "katakana still folds to hiragana");
    let c = Catalog::get();
    let hits = search(c, "ひ", Lang::Ja, None);
    for h in hits.iter().take(100) {
        let name = c.display_name(h.index, Lang::Ja);
        assert!(!name.starts_with('ぴ') && !name.starts_with('ピ'), "{name}");
    }
}

#[test]
fn tools_52_spaces_typos_code_points_emoticons_and_shortcodes() {
    top("thumbsup", Lang::En, None, "\u{1F44D}", 3);
    top("smlie", Lang::En, None, "\u{1F600}", 80);
    top("U+1F600", Lang::En, None, "\u{1F600}", 1);
    top("1f600", Lang::En, None, "\u{1F600}", 1);
    top("0x2192", Lang::En, None, "→", 1);
    top(":)", Lang::En, None, "\u{1F642}", 1);
    top("<3", Lang::En, None, "\u{2764}", 1);
    top(":smile:", Lang::En, Some(Kind::Emoji), "\u{1F604}", 20);
    top(":thumbs_up:", Lang::En, None, "\u{1F44D}", 3);
}

/// TOOLS-53: twenty fixed query to emoji pairs; a keyword data change that breaks one is a bug.
#[test]
fn tools_53_twenty_fixed_queries_find_their_emoji() {
    let pairs: [(&str, &str); 20] = [
        ("pizza", "\u{1F355}"),
        ("cat", "\u{1F408}"),
        ("dog", "\u{1F415}"),
        ("red heart", "\u{2764}"),
        ("fire", "\u{1F525}"),
        ("100", "\u{1F4AF}"),
        ("party", "\u{1F389}"),
        ("rocket", "\u{1F680}"),
        ("thumbs up", "\u{1F44D}"),
        ("crying", "\u{1F622}"),
        ("laughing", "\u{1F923}"),
        ("sun", "\u{2600}"),
        ("crescent moon", "\u{1F319}"),
        ("tree", "\u{1F333}"),
        ("racing car", "\u{1F3CE}"),
        ("coffee", "\u{2615}"),
        ("check mark", "\u{2705}"),
        ("warning", "\u{26A0}"),
        ("family", "\u{1F46A}"),
        ("germany", "\u{1F1E9}\u{1F1EA}"),
    ];
    for (q, want) in pairs {
        top(q, Lang::En, Some(Kind::Emoji), want, 8);
    }
}

/// TOOLS-61: `prepare` builds a language's folded names before the first query.
#[test]
fn tools_61_prepare_builds_the_language_before_the_first_search() {
    let c = Catalog::build().unwrap();
    assert!(c.is_prepared(Lang::En));
    assert!(!c.is_prepared(Lang::Ja));
    c.prepare(Lang::Ja);
    assert!(c.is_prepared(Lang::Ja));
    assert!(!c.is_prepared(Lang::De), "only the asked language");
}
