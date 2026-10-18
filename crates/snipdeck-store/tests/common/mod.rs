//! Helpers shared by the integration tests.
#![allow(dead_code, reason = "each test binary uses a different subset")]
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

use snipdeck_core::{
    Board, BoardId, Content, Format, Formats, Inline, Item, ItemId, ItemKind, List, ListId, Name,
    Placement, Timestamp,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store};

pub fn at(seconds: i64) -> Timestamp {
    Timestamp::from_second(1_800_000_000 + seconds).unwrap()
}

pub fn name(text: &str) -> Name {
    Name::new(text).unwrap()
}

pub fn store() -> Store {
    Store::open_in_memory().unwrap()
}

pub fn text(s: &str) -> Content {
    Content::Inline(Inline::new(s.as_bytes().to_vec()).unwrap())
}

pub fn text_formats(s: &str) -> Formats {
    Formats::new(vec![Format::Text(text(s))]).unwrap()
}

pub fn board(store: &mut Store, label: &str) -> Board {
    let board = Board::new(name(label), at(0));
    store.insert_board(&board, Placement::Last).unwrap();
    board
}

pub fn list(store: &mut Store, board: BoardId, label: &str) -> List {
    let list = List::new(board, name(label), at(0));
    store.insert_list(&list, Placement::Last).unwrap();
    list
}

pub fn item(store: &mut Store, list: ListId, title: &str, body: &str) -> Item {
    item_at(store, list, title, body, Placement::Last)
}

pub fn item_at(
    store: &mut Store,
    list: ListId,
    title: &str,
    body: &str,
    at_: Placement<ItemId>,
) -> Item {
    let mut item = Item::new(list, title.to_owned(), ItemKind::Text, at(0));
    item.preview = Item::preview_of(body, false);
    store.insert_item(&item, &text_formats(body), at_).unwrap();
    item
}

pub fn board_names(store: &Store) -> Vec<String> {
    store
        .boards()
        .unwrap()
        .iter()
        .map(|b| b.name.to_string())
        .collect()
}

pub fn list_names(store: &Store, board: BoardId) -> Vec<String> {
    store
        .lists(board)
        .unwrap()
        .iter()
        .map(|l| l.name.to_string())
        .collect()
}

pub fn titles(store: &Store, list: ListId) -> Vec<String> {
    store
        .items(list)
        .unwrap()
        .iter()
        .map(|i| i.title.clone())
        .collect()
}

pub fn assert_healthy(store: &Store) {
    let problems = store.check_integrity().unwrap();
    assert!(problems.is_empty(), "integrity problems: {problems:#?}");
}

/// Every file under `dir` that can hold content: the database, the WAL and the blobs, in no
/// particular order. Two files are left out because they hold no content and cannot be read while
/// the store is open on Windows (error 33, a locked region): the WAL index (`*-shm`, page numbers
/// only, byte-range locked by SQLite) and the single-writer `snipdeck.lock` (empty, exclusively
/// locked).
pub fn content_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if name
                .as_deref()
                .is_some_and(|n| n.ends_with("-shm") || n == "snipdeck.lock")
            {
                continue;
            }
            files.push(path);
        }
    }
    files
}
