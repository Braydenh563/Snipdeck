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

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use snipdeck_core::{
    Board, BoardId, Content, FileEntry, Format, Formats, Inline, Item, ItemId, ItemKind, List,
    ListAccent, ListId, ListSort, ListView, Name, Placement, Tag, TagId, Timestamp,
};
use snipdeck_portable::{ExportOptions, ImportReport, export_to_writer};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, Store, TagRepo};

pub fn at(seconds: i64) -> Timestamp {
    Timestamp::from_second(1_800_000_000 + seconds).unwrap()
}

pub fn name(text: &str) -> Name {
    Name::new(text).unwrap()
}

pub fn store() -> Store {
    Store::open_in_memory().unwrap()
}

pub fn inline(s: &str) -> Content {
    Content::Inline(Inline::new(s.as_bytes().to_vec()).unwrap())
}

pub fn text_formats(s: &str) -> Formats {
    Formats::new(vec![Format::Text(inline(s))]).unwrap()
}

pub fn board(store: &mut Store, label: &str) -> Board {
    let board = Board::new(name(label), at(1));
    store.insert_board(&board, Placement::Last).unwrap();
    board
}

pub fn list(store: &mut Store, board: BoardId, label: &str) -> List {
    let list = List::new(board, name(label), at(2));
    store.insert_list(&list, Placement::Last).unwrap();
    list
}

pub fn text_item(store: &mut Store, list: ListId, title: &str, body: &str) -> Item {
    let mut item = Item::new(list, title.to_owned(), ItemKind::Text, at(3));
    item.preview = Item::preview_of(body, false);
    store
        .insert_item(&item, &text_formats(body), Placement::Last)
        .unwrap();
    item
}

pub fn tag(store: &mut Store, label: &str) -> Tag {
    let tag = Tag::new(name(label));
    store.insert_tag(&tag).unwrap();
    tag
}

/// Bytes that do not compress and do not repeat: a stand-in for an image.
pub fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

/// A library with two boards, ordering that differs from creation order, tags, every format,
/// blobs, pinned and used items, and an empty list.
pub fn sample_store() -> Store {
    let mut s = store();
    let mail = tag(&mut s, "mail");
    let urgent = tag(&mut s, "Urgent");
    let _unused = tag(&mut s, "unused");

    let work = board(&mut s, "Work");
    let mut replies = List::new(work.id, name("Replies"), at(2));
    replies.accent = Some(ListAccent::Blue);
    replies.view = ListView::Compact;
    replies.sort = ListSort::MostUsed;
    s.insert_list(&replies, Placement::Last).unwrap();
    let files = list(&mut s, work.id, "Files");
    let empty = list(&mut s, work.id, "Empty");
    let _ = empty;

    // Inserted C, A, then B between them: the stored order is C, B, A.
    let c = text_item(&mut s, replies.id, "C third in creation", "ccc");
    let mut a = Item::new(replies.id, "A".to_owned(), ItemKind::Template, at(10));
    a.preview = Item::preview_of("Hello {date}", false);
    a.tags = [mail.id, urgent.id].into_iter().collect();
    a.pinned = true;
    a.always_plain = true;
    a.use_count = 5;
    a.last_used_at = Some(at(500));
    a.source_app = Some("Outlook".to_owned());
    a.updated_at = at(20);
    let formats = Formats::new(vec![
        Format::Html(inline("<b>Hello</b> {date}")),
        Format::Text(inline("Hello {date}")),
    ])
    .unwrap();
    s.insert_item(&a, &formats, Placement::Last).unwrap();
    let mut b = Item::new(replies.id, "B".to_owned(), ItemKind::Text, at(11));
    let big = "line of text\n".repeat(10_000);
    b.preview = Item::preview_of(&big, false);
    let content = s.put(big.into_bytes()).unwrap();
    assert!(
        matches!(content, Content::Blob { .. }),
        "large text is a blob"
    );
    s.insert_item(
        &b,
        &Formats::new(vec![Format::Text(content)]).unwrap(),
        Placement::After(c.id),
    )
    .unwrap();

    let mut image = Item::new(files.id, "Screenshot".to_owned(), ItemKind::Image, at(12));
    image.tags = [mail.id].into_iter().collect();
    let png = s.put(noise(80_000, 7)).unwrap();
    let alt = inline("alt text");
    s.insert_item(
        &image,
        &Formats::new(vec![Format::Png(png), Format::Text(alt)]).unwrap(),
        Placement::Last,
    )
    .unwrap();

    let doc = Item::new(files.id, "Two files".to_owned(), ItemKind::File, at(13));
    let small = FileEntry::new("notes.txt", inline("small file")).unwrap();
    let large = FileEntry::new("data.bin", s.put(noise(70_000, 9)).unwrap()).unwrap();
    s.insert_item(
        &doc,
        &Formats::new(vec![Format::Files(vec![small, large])]).unwrap(),
        Placement::First,
    )
    .unwrap();

    let home = board(&mut s, "Home");
    let misc = list(&mut s, home.id, "Misc");
    text_item(&mut s, misc.id, "Wifi", "hunter-wifi-pass");
    let _ = list(&mut s, home.id, "Nothing here");
    s
}

pub fn export_bytes(store: &Store, options: &ExportOptions<'_>) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    export_to_writer(store, at(100), options, &mut out).unwrap();
    out.into_inner()
}

/// Everything a user can see about a library, without ids, for comparing two libraries.
#[derive(Debug, PartialEq, Eq)]
pub struct Snap {
    pub boards: Vec<BoardSnap>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BoardSnap {
    pub name: String,
    pub created: Timestamp,
    pub lists: Vec<ListSnap>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ListSnap {
    pub name: String,
    pub style: String,
    pub items: Vec<ItemSnap>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ItemSnap {
    pub title: String,
    pub kind: String,
    pub flags: (bool, bool, bool),
    pub use_count: u32,
    pub last_used_at: Option<Timestamp>,
    pub source_app: Option<String>,
    pub created: Timestamp,
    pub updated: Timestamp,
    pub preview: String,
    pub tags: Vec<String>,
    pub formats: Vec<(String, Vec<(String, String)>)>,
}

fn digest(bytes: &[u8]) -> String {
    format!(
        "{} bytes, blake3 {}",
        bytes.len(),
        blake3::hash(bytes).to_hex()
    )
}

pub fn snapshot(store: &Store) -> Snap {
    let tag_names: BTreeMap<TagId, String> = store
        .tags()
        .unwrap()
        .into_iter()
        .map(|t| (t.id, t.name.to_string()))
        .collect();
    let mut boards = Vec::new();
    for board in store.boards().unwrap() {
        let mut lists = Vec::new();
        for list in store.lists(board.id).unwrap() {
            let mut items = Vec::new();
            for item in store.items(list.id).unwrap() {
                let mut tags: Vec<String> =
                    item.tags.iter().map(|t| tag_names[t].clone()).collect();
                tags.sort();
                let formats = store
                    .formats(item.id)
                    .unwrap()
                    .iter()
                    .map(|f| match f {
                        Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => (
                            f.kind().key().to_owned(),
                            vec![(String::new(), digest(&store.read(c).unwrap()))],
                        ),
                        Format::Files(files) => (
                            f.kind().key().to_owned(),
                            files
                                .iter()
                                .map(|e| {
                                    (
                                        e.name().to_owned(),
                                        digest(&store.read(e.content()).unwrap()),
                                    )
                                })
                                .collect(),
                        ),
                    })
                    .collect();
                items.push(ItemSnap {
                    title: item.title,
                    kind: item.kind.key().to_owned(),
                    flags: (item.pinned, item.sensitive, item.always_plain),
                    use_count: item.use_count,
                    last_used_at: item.last_used_at,
                    source_app: item.source_app,
                    created: item.created_at,
                    updated: item.updated_at,
                    preview: item.preview,
                    tags,
                    formats,
                });
            }
            lists.push(ListSnap {
                name: list.name.to_string(),
                style: format!(
                    "{:?}/{}/{}/{}/{}",
                    list.accent, list.view, list.sort, list.created_at, list.updated_at
                ),
                items,
            });
        }
        boards.push(BoardSnap {
            name: board.name.to_string(),
            created: board.created_at,
            lists,
        });
    }
    Snap { boards }
}

pub fn counts(store: &Store) -> (usize, usize, usize, usize) {
    let boards = store.boards().unwrap();
    let lists: usize = boards
        .iter()
        .map(|b| store.lists(b.id).unwrap().len())
        .sum();
    (
        boards.len(),
        lists,
        store.all_items().unwrap().len(),
        store.tags().unwrap().len(),
    )
}

pub fn assert_healthy(store: &Store) {
    let problems = store.check_integrity().unwrap();
    assert!(problems.is_empty(), "integrity problems: {problems:#?}");
}

pub fn assert_trash_empty(store: &Store) {
    use snipdeck_store::TrashRepo;
    assert!(store.trashed().unwrap().is_empty());
}

/// The entries of a zip, by name.
pub fn entries(zip_bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(zip_bytes)).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        out.insert(file.name().unwrap().into_owned(), bytes);
    }
    out
}

/// A zip with exactly these entries (deflated).
pub fn build_zip<'a>(entries: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// Rebuilds an archive after `edit` changed its entries.
pub fn rewrite(zip_bytes: &[u8], edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>)) -> Vec<u8> {
    let mut map = entries(zip_bytes);
    edit(&mut map);
    build_zip(map.iter().map(|(k, v)| (k.as_str(), v.as_slice())))
}

/// Edits `items.json` / `manifest.json` as JSON.
pub fn edit_json(
    zip_bytes: &[u8],
    entry: &str,
    edit: impl FnOnce(&mut serde_json::Value),
) -> Vec<u8> {
    rewrite(zip_bytes, |map| {
        let mut value: serde_json::Value = serde_json::from_slice(&map[entry]).unwrap();
        edit(&mut value);
        map.insert(entry.to_owned(), serde_json::to_vec(&value).unwrap());
    })
}

pub fn warnings(report: &ImportReport) -> Vec<String> {
    report.warnings.iter().map(ToString::to_string).collect()
}

pub fn first_item(store: &Store) -> Item {
    store.all_items().unwrap().remove(0)
}

pub fn item_ids(store: &Store) -> Vec<ItemId> {
    store.all_items().unwrap().iter().map(|i| i.id).collect()
}

/// Like `assert_eq!(a, b)` but names the first difference instead of printing both libraries.
pub fn assert_same(actual: &Snap, expected: &Snap) {
    assert_eq!(actual.boards.len(), expected.boards.len(), "board count");
    for (a, e) in actual.boards.iter().zip(&expected.boards) {
        assert_eq!((&a.name, a.created), (&e.name, e.created), "board");
        assert_eq!(a.lists.len(), e.lists.len(), "list count in {}", a.name);
        for (al, el) in a.lists.iter().zip(&e.lists) {
            assert_eq!(
                (&al.name, &al.style),
                (&el.name, &el.style),
                "list in {}",
                a.name
            );
            assert_eq!(al.items.len(), el.items.len(), "item count in {}", al.name);
            for (ai, ei) in al.items.iter().zip(&el.items) {
                assert_eq!(ai, ei, "item `{}` in list {}", ei.title, al.name);
            }
        }
    }
}
