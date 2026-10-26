//! Logic tests for the symbols crate (art).
use snipdeck_symbols::{ArtCache, ArtPack, Catalog, SymbolId};

static SHORT: [u8; 18 + 4] = {
    let mut b = [0u8; 22];
    b[0] = b'S';
    b[1] = b'D';
    b[2] = b'E';
    b[3] = b'1';
    b[4] = 48;
    b[6] = 1; // one entry
    b[10] = 18; // at offset 18, length 4: not a PNG
    b[14] = 4;
    b
};

fn smile() -> SymbolId {
    SymbolId::Emoji("1f600".into())
}

#[test]
fn art_decodes_and_resamples_to_24_30_36_48() {
    let c = Catalog::get();
    for px in [24u32, 30, 36, 48] {
        let rgba = c.art(&smile(), px).unwrap();
        assert_eq!(rgba.len(), (px * px * 4) as usize, "{px}");
        let at = |x: u32, y: u32| &rgba[((y * px + x) * 4) as usize..][..4];
        // 😀 is a yellow disc on a transparent square: corner empty, middle opaque and warm.
        assert_eq!(at(0, 0)[3], 0, "{px}: corner is transparent");
        let mid = at(px / 2, px / 2);
        assert!(mid[3] > 200, "{px}: centre is opaque, got {mid:?}");
        assert!(
            mid[0] > 150,
            "{px}: centre is warm (red channel), got {mid:?}"
        );
    }
    // Not just a fixed picture: different emoji differ.
    assert_ne!(
        c.art(&smile(), 48),
        c.art(&SymbolId::Emoji("1f680".into()), 48)
    );
}

#[test]
fn every_emoji_has_decodable_art() {
    let c = Catalog::get();
    let pack = ArtPack::parse(include_bytes!("../data/emoji48.bin")).unwrap();
    assert_eq!(pack.len(), c.range(snipdeck_symbols::Kind::Emoji).len());
    assert_eq!(pack.px(), 48);
    for i in c.range(snipdeck_symbols::Kind::Emoji) {
        assert!(
            pack.rgba(i, 24).is_some(),
            "art of emoji {i} ({:?})",
            c.symbol(i).unwrap().id
        );
    }
}

#[test]
fn missing_art_falls_back_to_text() {
    let c = Catalog::get();
    // Not art-backed kinds: the caller draws `text` instead.
    assert!(c.art(&SymbolId::Kaomoji("joy-smile".into()), 24).is_none());
    assert!(c.art(&SymbolId::Icon("trash".into()), 24).is_none());
    assert!(c.art(&SymbolId::Emoji("ffffff".into()), 24).is_none());
    assert!(c.get_symbol(&smile()).unwrap().text.is_some());
    // A pack with fewer entries than the catalog, and one with a broken PNG.
    let pack = ArtPack::parse(&SHORT).unwrap();
    assert_eq!(pack.len(), 1);
    assert!(pack.rgba(0, 24).is_none(), "garbage PNG");
    assert!(pack.rgba(1, 24).is_none(), "past the end");
    assert!(ArtPack::parse(b"NOPE").is_err());
    assert!(ArtPack::parse(&SHORT[..12]).is_err());
    assert!(ArtPack::none().rgba(0, 24).is_none());
    assert!(c.art(&smile(), 0).is_none());
}

#[test]
fn the_cache_decodes_once_and_evicts_the_least_recently_used() {
    let c = Catalog::get();
    let mut cache = ArtCache::new(2);
    let a = SymbolId::Emoji("1f600".into());
    let b = SymbolId::Emoji("1f680".into());
    let d = SymbolId::Emoji("1f408".into());
    let first = cache.get(c, &a, 24).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&first, &cache.get(c, &a, 24).unwrap()),
        "a hit shares the cell"
    );
    cache.get(c, &b, 24).unwrap();
    cache.get(c, &a, 24).unwrap(); // a is now newer than b
    cache.get(c, &d, 24).unwrap(); // evicts b
    assert_eq!(cache.len(), 2);
    assert!(
        std::sync::Arc::ptr_eq(&first, &cache.get(c, &a, 24).unwrap()),
        "a survived"
    );
    assert!(cache.get(c, &SymbolId::Icon("trash".into()), 24).is_none());
    assert_eq!(cache.len(), 2, "a miss that has no art caches nothing");
    // Different sizes are different cells.
    cache.get(c, &a, 36).unwrap();
    assert_eq!(cache.len(), 2);
}
