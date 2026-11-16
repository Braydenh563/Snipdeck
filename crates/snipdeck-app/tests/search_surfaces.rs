//! One table of queries and the items they find, run against the Panel, the Picker and the Main
//! window (LIBRARY-3): the three surfaces share one query pipeline, so they must agree.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code: a failure is a failed test"
)]

use std::collections::BTreeSet;

use snipdeck_app::index_cache::IndexCache;
use snipdeck_app::main_window::{Key, MainModel};
use snipdeck_app::panel::PanelModel;
use snipdeck_app::picker::PickerModel;
use snipdeck_core::{
    Board, Content, Format, Formats, Inline, Item, ItemId, ItemKind, List, ListId, Name, Placement,
    Tag, Timestamp, Trigger,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo, TriggerRepo};

fn at() -> Timestamp {
    Timestamp::from_second(1_800_000_000).unwrap()
}

struct Library {
    store: Store,
    ids: Vec<(&'static str, ItemId)>,
}

impl Library {
    fn id(&self, title: &str) -> ItemId {
        self.ids.iter().find(|(t, _)| *t == title).unwrap().1
    }
}

fn add(
    store: &mut Store,
    list: ListId,
    title: &'static str,
    kind: ItemKind,
    body: &str,
    edit: impl FnOnce(&mut Item),
) -> (&'static str, ItemId) {
    let mut item = Item::new(list, title.to_owned(), kind, at());
    item.preview = Item::preview_of(body, false);
    edit(&mut item);
    let formats = Formats::new(vec![Format::Text(Content::Inline(
        Inline::new(body.as_bytes().to_vec()).unwrap(),
    ))])
    .unwrap();
    store.insert_item(&item, &formats, Placement::Last).unwrap();
    (title, item.id)
}

fn library() -> Library {
    let mut store = Store::open_in_memory().unwrap();
    let board = Board::new(Name::new("Work").unwrap(), at());
    store.insert_board(&board, Placement::Last).unwrap();
    let replies = List::new(board.id, Name::new("Replies").unwrap(), at());
    let links = List::new(board.id, Name::new("Useful links").unwrap(), at());
    store.insert_list(&replies, Placement::Last).unwrap();
    store.insert_list(&links, Placement::Last).unwrap();
    let work = Tag::new(Name::new("work").unwrap());
    store.insert_tag(&work).unwrap();
    let far = format!("{} pineapple tucked away", "filler ".repeat(30));
    let ids = vec![
        add(
            &mut store,
            replies.id,
            "Invoice",
            ItemKind::Text,
            "Please pay by Friday",
            |i| {
                i.tags.insert(work.id);
            },
        ),
        add(
            &mut store,
            replies.id,
            "Email signature",
            ItemKind::Text,
            "Kind regards, Ana",
            |_| {},
        ),
        add(
            &mut store,
            replies.id,
            "Café résumé",
            ItemKind::Text,
            "cv",
            |_| {},
        ),
        add(
            &mut store,
            replies.id,
            "Shipping guide",
            ItemKind::Text,
            "boxes",
            |i| {
                i.pinned = true;
            },
        ),
        add(&mut store, replies.id, "Long", ItemKind::Text, &far, |_| {}),
        add(
            &mut store,
            replies.id,
            "Tokyo",
            ItemKind::Text,
            "東京都の天気は晴れです",
            |_| {},
        ),
        add(
            &mut store,
            links.id,
            "Docs",
            ItemKind::Link,
            "https://example.com",
            |_| {},
        ),
    ];
    let signature = ids[1].1;
    store
        .set_trigger(signature, Some(&Trigger::new(";sig").unwrap()))
        .unwrap();
    Library { store, ids }
}

/// The query, the titles it finds (in any order) and, where ranking matters, the first hit.
const TABLE: &[(&str, &[&str], Option<&str>)] = &[
    ("invoice work", &["Invoice"], None),
    ("RESUME", &["Café résumé"], None),
    ("ｃａｆｅ", &["Café résumé"], None),
    (";sig", &["Email signature"], Some("Email signature")),
    ("ature", &["Email signature"], None),
    ("pineapple", &["Long"], None),
    ("天気", &["Tokyo"], None),
    ("\"kind regards\"", &["Email signature"], None),
    ("\"regards kind\"", &[], None),
    ("tag:work", &["Invoice"], None),
    ("-tag:work invoice", &[], None),
    ("list:useful", &["Docs"], None),
    ("kind:link", &["Docs"], None),
    ("is:pinned", &["Shipping guide"], None),
    ("invoice -friday", &[], None),
    ("zzzz", &[], None),
];

fn check(surface: &str, lib: &Library, query: &str, found: &[ItemId]) {
    let (_, want, first) = TABLE.iter().find(|(q, _, _)| *q == query).unwrap();
    let want: BTreeSet<ItemId> = want.iter().map(|t| lib.id(t)).collect();
    let got: BTreeSet<ItemId> = found.iter().copied().collect();
    assert_eq!(got, want, "{surface}: {query:?}");
    if let Some(first) = first {
        assert_eq!(
            found.first(),
            Some(&lib.id(first)),
            "{surface}: {query:?} first"
        );
    }
}

#[test]
fn the_panel_finds_what_the_table_says() {
    let lib = library();
    let cache = IndexCache::new();
    let mut panel = PanelModel::load_cached(&lib.store, None, None, &cache).unwrap();
    for (query, _, _) in TABLE {
        panel.set_query(&lib.store, query).unwrap();
        let ids: Vec<ItemId> = panel.rows().iter().map(|r| r.id).collect();
        check("panel", &lib, query, &ids);
    }
}

#[test]
fn the_picker_finds_what_the_table_says() {
    let lib = library();
    let cache = IndexCache::new();
    let mut picker = PickerModel::load_cached(&lib.store, None, &cache).unwrap();
    for (query, _, _) in TABLE {
        picker.set_query(&lib.store, query).unwrap();
        let ids: Vec<ItemId> = picker.rows().iter().map(|r| r.id).collect();
        check("picker", &lib, query, &ids);
    }
}

#[test]
fn the_main_window_finds_what_the_table_says() {
    let lib = library();
    let mut main = MainModel::new();
    main.reload(&lib.store).unwrap();
    for (query, _, _) in TABLE {
        main.set_query(&lib.store, query).unwrap();
        let ids: Vec<ItemId> = main
            .rows()
            .iter()
            .filter_map(|r| match r.key {
                Key::Item(id) => Some(id),
                _ => None,
            })
            .collect();
        check("main", &lib, query, &ids);
    }
}

/// Main shows the tag that explains a hit, as the Panel and the Picker do (LIBRARY-6).
#[test]
fn main_marks_the_tag_that_matched() {
    let lib = library();
    let mut main = MainModel::new();
    main.reload(&lib.store).unwrap();
    main.set_query(&lib.store, "invoice work").unwrap();
    let rows = main.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].marks.tag.as_deref(), Some("work"));
    assert_eq!(
        rows[0].marks.title,
        Some((String::new(), "Invoice".to_owned()))
    );
}
