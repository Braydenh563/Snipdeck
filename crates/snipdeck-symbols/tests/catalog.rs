//! Logic tests for the symbols crate (catalog).
#![allow(clippy::unwrap_used, reason = "test helper outside a #[test] fn")]
use snipdeck_symbols::{Catalog, EMOJI_GROUPS, KAOMOJI_GROUPS, Kind, Lang, SymbolId};

fn manifest() -> serde_json::Value {
    serde_json::from_str(include_str!("../data/manifest.json")).unwrap()
}

#[test]
fn the_embedded_data_builds() {
    Catalog::build().unwrap();
}

#[test]
fn catalog_counts_match_the_manifest() {
    let c = Catalog::get();
    let m = manifest();
    let n = |k: &str| usize::try_from(m["counts"][k].as_u64().unwrap()).unwrap();
    assert_eq!(c.range(Kind::Emoji).len(), n("emoji"));
    assert_eq!(c.range(Kind::Kaomoji).len(), n("kaomoji"));
    assert_eq!(c.range(Kind::Icon).len(), n("icons"));
    assert_eq!(c.len(), n("emoji") + n("kaomoji") + n("icons"));
    assert_eq!(c.range(Kind::Emoji).end, c.range(Kind::Kaomoji).start);
    assert_eq!(c.range(Kind::Kaomoji).end, c.range(Kind::Icon).start);
    assert_eq!(
        m["art"]["count"].as_u64().unwrap(),
        m["counts"]["emoji"].as_u64().unwrap()
    );
    // The generator's group lists are the crate's.
    let groups = |k: &str| -> Vec<String> {
        m[k].as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(groups("emoji_groups"), EMOJI_GROUPS);
    assert_eq!(groups("kaomoji_groups"), KAOMOJI_GROUPS);
}

#[test]
fn every_symbol_has_a_name_in_every_bundled_language_or_falls_back_to_en() {
    let c = Catalog::get();
    let m = manifest();
    for lang in [
        Lang::En,
        Lang::De,
        Lang::Es,
        Lang::Fr,
        Lang::Ja,
        Lang::Pt,
        Lang::Zh,
    ] {
        for i in 0..c.len() {
            assert!(!c.display_name(i, lang).is_empty(), "{lang:?} name of {i}");
        }
    }
    // A language that lacks a name shows the English one.
    let fr_missing = m["counts"]["names_missing"]["fr"].as_u64().unwrap();
    let english = (c.range(Kind::Emoji))
        .filter(|&i| c.display_name(i, Lang::Fr) == c.symbol(i).unwrap().name)
        .count();
    assert!(u64::try_from(english).unwrap() >= fr_missing);
    // And the translation really differs elsewhere.
    let cat = c.index_of(&SymbolId::Emoji("1f408".into())).unwrap();
    assert_eq!(c.display_name(cat, Lang::En), "cat");
    assert_eq!(c.display_name(cat, Lang::Ja), "ネコ");
    assert_eq!(c.display_name(cat, Lang::Fr), "chat");
}

#[test]
fn ids_are_stable_across_regeneration() {
    // Persisted in users' recents: if one of these goes missing, ids changed. Never edit to pass.
    let golden = [
        ("emoji:1f600", "😀"),
        ("emoji:1f44b", "👋"),
        ("emoji:2764-fe0f", "❤\u{fe0f}"),
        ("emoji:263a-fe0f", "☺\u{fe0f}"),
        ("emoji:1f9d1-200d-1f4bb", "🧑\u{200d}💻"),
        ("emoji:1f469-200d-1f4bb", "👩\u{200d}💻"),
        ("emoji:1f3f3-fe0f-200d-1f308", "🏳\u{fe0f}\u{200d}🌈"),
        ("emoji:1f4a9", "💩"),
        ("emoji:1f408", "🐈"),
        ("emoji:2615", "☕"),
        ("emoji:1f680", "🚀"),
        ("emoji:1f3f4-200d-2620-fe0f", "🏴\u{200d}☠\u{fe0f}"),
        ("kaomoji:shrug-classic", "¯\\_(ツ)_/¯"),
        ("kaomoji:actions-table-flip", "(╯°□°）╯︵ ┻━┻"),
        ("kaomoji:joy-smile", "(^_^)"),
        ("icon:arrow-clockwise", ""),
        ("icon:trash", ""),
        ("icon:smiley", ""),
        ("icon:magnifying-glass", ""),
        ("icon:clock", ""),
    ];
    let c = Catalog::get();
    for (key, text) in golden {
        let id = SymbolId::from_key(key).unwrap();
        let s = c.get_symbol(&id).unwrap_or_else(|| panic!("{key} is gone"));
        assert_eq!(s.text.as_deref().unwrap_or(""), text, "{key}");
        assert_eq!(s.id.key(), key);
    }
}

#[test]
fn symbol_keys_round_trip_and_reject_junk() {
    for key in ["emoji:1f600", "kaomoji:joy-smile", "icon:trash"] {
        assert_eq!(SymbolId::from_key(key).unwrap().key(), key);
    }
    for junk in ["", "emoji", "emoji:", "sticker:1", ":x"] {
        assert!(SymbolId::from_key(junk).is_none(), "{junk:?}");
    }
}

#[test]
fn kinds_are_contiguous_and_typed() {
    let c = Catalog::get();
    for kind in [Kind::Emoji, Kind::Kaomoji, Kind::Icon] {
        let r = c.range(kind);
        assert!(!r.is_empty());
        for i in r {
            let s = c.symbol(i).unwrap();
            assert_eq!(s.kind, kind);
            assert_eq!(s.id.kind(), kind);
            assert_eq!(s.text.is_some(), kind != Kind::Icon);
            assert!(!s.name.is_empty(), "{:?}", s.id);
        }
    }
    let wide = c
        .get_symbol(&SymbolId::Kaomoji("actions-table-flip".into()))
        .unwrap();
    assert!(wide.wide);
    assert!(
        !c.get_symbol(&SymbolId::Kaomoji("joy-smile".into()))
            .unwrap()
            .wide
    );
}

#[test]
fn language_tags_map_to_bundled_languages() {
    assert_eq!(Lang::from_tag("zh-Hans"), Lang::Zh);
    assert_eq!(Lang::from_tag("pt_BR"), Lang::Pt);
    assert_eq!(Lang::from_tag("DE"), Lang::De);
    assert_eq!(Lang::from_tag("ko"), Lang::En);
}
