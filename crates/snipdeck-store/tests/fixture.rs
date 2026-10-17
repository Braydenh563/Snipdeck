//! A database file written by the first released schema (version 1), kept in the repository so
//! that later versions are always tested against real old data.
//!
//! To regenerate it (only ever needed if the fixture is lost, never to follow schema changes):
//! `cargo test -p snipdeck-store --test fixture -- --ignored regenerate`.
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
use snipdeck_core::{
    Board, BoardId, Format, Formats, Item, ItemId, ItemKind, List, ListId, Placement, Tag, TagId,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store, TagRepo, TrashRepo, Trashable};

const FIXTURE: &[u8] = include_bytes!("fixtures/v1.db");

const BOARD: &str = "0198aaaa-0000-7000-8000-000000000001";
const LIST: &str = "0198aaaa-0000-7000-8000-000000000002";
const ITEM_RICH: &str = "0198aaaa-0000-7000-8000-000000000003";
const ITEM_SECRET: &str = "0198aaaa-0000-7000-8000-000000000004";
const ITEM_TRASHED: &str = "0198aaaa-0000-7000-8000-000000000005";
const TAG: &str = "0198aaaa-0000-7000-8000-000000000006";

fn id<T: std::str::FromStr>(text: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    text.parse().unwrap()
}

#[test]
#[ignore = "writes tests/fixtures/v1.db"]
fn regenerate() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = Store::open(dir.path()).unwrap();
        let mut board = Board::new(name("Work"), at(0));
        board.id = id(BOARD);
        s.insert_board(&board, Placement::Last).unwrap();
        let mut list = List::new(board.id, name("Replies"), at(0));
        list.id = id(LIST);
        s.insert_list(&list, Placement::Last).unwrap();
        let mut tag = Tag::new(name("Email"));
        tag.id = id(TAG);
        s.insert_tag(&tag).unwrap();

        let mut rich = Item::new(list.id, "Greeting".to_owned(), ItemKind::Text, at(1));
        rich.id = id(ITEM_RICH);
        rich.preview = "Hello there".to_owned();
        rich.tags.insert(tag.id);
        rich.pinned = true;
        let formats = Formats::new(vec![
            Format::Html(text("<b>Hello</b> there")),
            Format::Text(text("Hello there")),
        ])
        .unwrap();
        s.insert_item(&rich, &formats, Placement::Last).unwrap();

        let mut secret = Item::new(list.id, "Wifi".to_owned(), ItemKind::Text, at(2));
        secret.id = id(ITEM_SECRET);
        secret.sensitive = true;
        s.insert_item(&secret, &text_formats("hunter2"), Placement::Last)
            .unwrap();

        let mut gone = Item::new(list.id, "Old".to_owned(), ItemKind::Text, at(3));
        gone.id = id(ITEM_TRASHED);
        gone.preview = "obsolete".to_owned();
        s.insert_item(&gone, &text_formats("obsolete"), Placement::Last)
            .unwrap();
        s.trash(Trashable::Item(gone.id), at(4)).unwrap();
    }
    // Fold the write-ahead log into one self-contained file.
    let path = dir.path().join("snipdeck.db");
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.pragma_update(None, "journal_mode", "DELETE").unwrap();
    drop(raw);
    std::fs::copy(
        &path,
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db"),
    )
    .unwrap();
}

fn open_fixture() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("snipdeck.db"), FIXTURE).unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

#[test]
fn a_version_1_database_opens_and_reads_back() {
    let (_dir, s) = open_fixture();
    assert_eq!(s.schema_version(), 1);
    let boards = s.boards().unwrap();
    assert_eq!(boards.len(), 1);
    assert_eq!(
        (boards[0].id, boards[0].name.as_str()),
        (id::<BoardId>(BOARD), "Work")
    );
    let lists = s.lists(boards[0].id).unwrap();
    assert_eq!(
        (lists[0].id, lists[0].name.as_str()),
        (id::<ListId>(LIST), "Replies")
    );

    let items = s.items(lists[0].id).unwrap();
    assert_eq!(
        items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
        ["Greeting", "Wifi"]
    );
    assert!(items[0].pinned && items[0].tags.contains(&id::<TagId>(TAG)));
    assert!(items[1].sensitive && items[1].preview.is_empty());
    assert_eq!(
        s.formats(id::<ItemId>(ITEM_RICH)).unwrap().iter().count(),
        2
    );
    assert_eq!(s.tags().unwrap().len(), 1);
    assert_healthy(&s);
}

#[test]
fn the_fixtures_search_index_and_trash_work() {
    let (_dir, mut s) = open_fixture();
    assert_eq!(s.search("hello", 5).unwrap(), [id::<ItemId>(ITEM_RICH)]);
    assert!(s.search("hunter2", 5).unwrap().is_empty());
    assert!(s.search("obsolete", 5).unwrap().is_empty(), "trashed");
    let trashed = s.trashed().unwrap();
    assert_eq!(trashed.len(), 1);
    s.restore(trashed[0].target).unwrap();
    assert_eq!(
        s.search("obsolete", 5).unwrap(),
        [id::<ItemId>(ITEM_TRASHED)]
    );
}
