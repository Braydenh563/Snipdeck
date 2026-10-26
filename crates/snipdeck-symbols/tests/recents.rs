//! Logic tests for the symbols crate (recents).
use snipdeck_symbols::{Catalog, Kind, RECENTS_MAX, RecentEntry, Recents, SymbolId};

// A fixed "now": the app passes its injected clock, tests pass these.
const T0: i64 = 1_800_000_000_000;
const T1: i64 = T0 + 60_000;
const T2: i64 = T0 + 120_000;

fn e(seq: &str) -> SymbolId {
    SymbolId::Emoji(seq.into())
}

#[test]
fn recents_are_most_recent_first_without_duplicates() {
    let mut r = Recents::new();
    r.push(e("1f600"), T0);
    r.push(SymbolId::Kaomoji("joy-smile".into()), T1);
    r.push(e("1f600"), T2);
    assert_eq!(
        r.list(None),
        vec![e("1f600"), SymbolId::Kaomoji("joy-smile".into())]
    );
    assert_eq!(
        r.used_at(&e("1f600")),
        Some(T2),
        "the later use replaces the earlier one"
    );
    assert_eq!(
        r.list(Some(Kind::Kaomoji)),
        vec![SymbolId::Kaomoji("joy-smile".into())]
    );
    assert_eq!(r.list(Some(Kind::Icon)).len(), 0);
}

#[test]
fn recents_cap_at_24() {
    let c = Catalog::get();
    let mut r = Recents::new();
    for (n, s) in c.symbols().iter().take(RECENTS_MAX + 6).enumerate() {
        r.push(s.id.clone(), T0 + i64::try_from(n).unwrap());
    }
    let list = r.list(None);
    assert_eq!(list.len(), 24);
    assert_eq!(list[0], c.symbol(29).unwrap().id, "newest first");
    assert_eq!(list[23], c.symbol(6).unwrap().id, "the six oldest fell off");
}

#[test]
fn recents_drop_unknown_ids_on_load() {
    let saved = vec![
        RecentEntry {
            id: "emoji:1f600".into(),
            used_at: T0,
        },
        RecentEntry {
            id: "emoji:ffffff".into(),
            used_at: T2,
        }, // gone in a data update
        RecentEntry {
            id: "not a key".into(),
            used_at: T2,
        },
        RecentEntry {
            id: "icon:trash".into(),
            used_at: T1,
        },
        RecentEntry {
            id: "icon:trash".into(),
            used_at: T0,
        }, // duplicate
    ];
    let r = Recents::from_saved(&saved, Catalog::get());
    assert_eq!(
        r.list(None),
        vec![SymbolId::Icon("trash".into()), e("1f600")]
    );
}

#[test]
fn recents_round_trip_through_settings() {
    let mut r = Recents::new();
    r.push(e("1f680"), T0);
    r.push(SymbolId::Icon("clock".into()), T1);
    r.push(SymbolId::Kaomoji("actions-table-flip".into()), T2);
    // The settings file stores `symbols.recent = [{id, used_at}]` as JSON.
    let json = serde_json::to_string(&r.to_saved()).unwrap();
    assert_eq!(
        json,
        format!(
            r#"[{{"id":"kaomoji:actions-table-flip","used_at":{T2}}},{{"id":"icon:clock","used_at":{T1}}},{{"id":"emoji:1f680","used_at":{T0}}}]"#
        )
    );
    let back: Vec<RecentEntry> = serde_json::from_str(&json).unwrap();
    assert_eq!(Recents::from_saved(&back, Catalog::get()), r);
}
