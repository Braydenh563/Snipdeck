//! Quick capture: save the text on the clipboard as a new item in the default list.
//!
//! Slint-free. The controller reads the clipboard and calls [`capture_text`]; the item and its
//! content are written in one store transaction ([`Library::atomically`]), so a content blob is
//! never left without its item.

use snipdeck_core::{Format, Formats, Item, ItemId, ItemKind, ListId, Placement, Timestamp};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo};

use crate::controller::Library;

/// The largest text a capture saves (SPEC 5.4: over 5 MB is rejected with a toast).
pub const MAX_TEXT_BYTES: usize = 5 * 1024 * 1024;

/// The list a capture goes into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// The list.
    pub id: ListId,
    /// Its name, for the toast.
    pub name: String,
}

/// What a capture did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// The text is a new item at the top of the list.
    Saved {
        /// The new item.
        item: ItemId,
        /// The list it went into.
        list: String,
    },
    /// There was no text, or only whitespace: nothing was saved.
    Empty,
    /// The text is over [`MAX_TEXT_BYTES`]: nothing was saved.
    TooLarge,
}

/// The default list: `current` (the list the Panel shows) when it still exists, otherwise the
/// first list of the first board that has one. `None` when the library has no list at all.
///
/// # Errors
///
/// Any store error.
pub fn default_list<L: BoardRepo + ListRepo>(
    library: &L,
    current: Option<ListId>,
) -> snipdeck_store::Result<Option<Destination>> {
    let destination = |l: snipdeck_core::List| Destination {
        id: l.id,
        name: l.name.as_str().to_owned(),
    };
    if let Some(list) = current.and_then(|id| library.list(id).ok()) {
        return Ok(Some(destination(list)));
    }
    for board in library.boards()? {
        if let Some(list) = library.lists(board.id)?.into_iter().next() {
            return Ok(Some(destination(list)));
        }
    }
    Ok(None)
}

/// Saves `text` as a new item at the top of `list`. A single `http(s)` address becomes a link,
/// anything else is text.
///
/// # Errors
///
/// Any store error; nothing is saved then.
pub fn capture_text<L: Library>(
    library: &mut L,
    list: &Destination,
    text: Option<&str>,
    now: Timestamp,
) -> snipdeck_store::Result<Captured> {
    let kind = text.map_or(ItemKind::Text, kind_of);
    capture_as(library, list, text, kind, Placement::First, now)
}

/// Saves `text` as a new item of `kind` in `list`, at `at`. Blank text saves nothing.
///
/// # Errors
///
/// Any store error; nothing is saved then.
pub fn capture_as<L: Library>(
    library: &mut L,
    list: &Destination,
    text: Option<&str>,
    kind: ItemKind,
    at: Placement<ItemId>,
    now: Timestamp,
) -> snipdeck_store::Result<Captured> {
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
        return Ok(Captured::Empty);
    };
    if text.len() > MAX_TEXT_BYTES {
        return Ok(Captured::TooLarge);
    }
    let mut item = Item::new(list.id, String::new(), kind, now);
    item.preview = Item::preview_of(text, false);
    let id = item.id;
    library.atomically(|batch| {
        let content = batch.put(text.as_bytes().to_vec())?;
        let formats = Formats::new(vec![Format::Text(content)])?;
        batch.insert_item(&item, &formats, at)
    })?;
    Ok(Captured::Saved {
        item: id,
        list: list.name.clone(),
    })
}

/// The kind the quick-add box preselects for `text`: a lone web address is a link, text with a
/// placeholder such as `{date}` or `{field:Name}` that parses is a template, anything else is text.
#[must_use]
pub fn detect_kind(text: &str) -> ItemKind {
    let kind = kind_of(text);
    if kind != ItemKind::Text {
        return kind;
    }
    let has_placeholder = [
        "{date",
        "{time",
        "{clipboard}",
        "{cursor}",
        "{field:",
        "{choice:",
    ]
    .iter()
    .any(|p| text.contains(p));
    if has_placeholder && snipdeck_core::template::Template::parse(text).is_ok() {
        ItemKind::Template
    } else {
        ItemKind::Text
    }
}

/// A lone `http(s)` address is a link, anything else text.
#[must_use]
pub fn kind_of(text: &str) -> ItemKind {
    let t = text.trim();
    let web = t.starts_with("http://") || t.starts_with("https://");
    if web && !t.contains(char::is_whitespace) && t.len() > "https://".len() {
        ItemKind::Link
    } else {
        ItemKind::Text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{Board, List, Name};
    use snipdeck_store::Store;

    fn now() -> Timestamp {
        "2026-10-18T09:00:00Z".parse().expect("timestamp")
    }

    /// Boards "Empty" (no lists) and "Work" with lists Notes and Links.
    fn library() -> (Store, ListId, ListId) {
        let mut store = Store::open_in_memory().expect("store");
        let empty = Board::new(Name::new("Empty").expect("name"), now());
        store.insert_board(&empty, Placement::Last).expect("board");
        let work = Board::new(Name::new("Work").expect("name"), now());
        store.insert_board(&work, Placement::Last).expect("board");
        let mut ids = Vec::new();
        for name in ["Notes", "Links"] {
            let list = List::new(work.id, Name::new(name).expect("name"), now());
            store.insert_list(&list, Placement::Last).expect("list");
            ids.push(list.id);
        }
        (store, ids[0], ids[1])
    }

    fn dest(store: &Store, id: ListId) -> Destination {
        default_list(store, Some(id)).expect("list").expect("some")
    }

    fn cap(
        store: &mut Store,
        list: ListId,
        text: Option<&str>,
        at: Timestamp,
    ) -> snipdeck_store::Result<Captured> {
        let to = dest(store, list);
        capture_text(store, &to, text, at)
    }

    fn text_of(store: &Store, id: ItemId) -> String {
        let formats = store.formats(id).expect("formats");
        let Some(Format::Text(content)) = formats.get(snipdeck_core::FormatKind::Text) else {
            panic!("no text format")
        };
        String::from_utf8(store.read(content).expect("read").into_owned()).expect("utf8")
    }

    #[test]
    fn text_becomes_a_new_item_at_the_top_of_the_list() {
        let (mut store, notes, _) = library();
        let first = cap(&mut store, notes, Some("first"), now()).expect("capture");
        let second = cap(&mut store, notes, Some("second\nline two"), now()).expect("capture");
        let (Captured::Saved { item: a, list }, Captured::Saved { item: b, .. }) = (first, second)
        else {
            panic!("both saved")
        };
        assert_eq!(list, "Notes");
        let items = store.items(notes).expect("items");
        assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), [b, a]);
        let saved = store.item(b).expect("item");
        assert_eq!(saved.kind, ItemKind::Text);
        assert_eq!(saved.title, "");
        assert_eq!(saved.preview, "second\nline two");
        assert_eq!(saved.use_count, 0);
        assert!(!saved.sensitive);
        assert_eq!(text_of(&store, b), "second\nline two");
    }

    #[test]
    fn a_single_web_address_is_a_link_and_other_text_is_not() {
        for (text, kind) in [
            ("https://example.com/a?b=1", ItemKind::Link),
            ("  http://example.com  ", ItemKind::Link),
            ("https://", ItemKind::Text),
            ("see https://example.com", ItemKind::Text),
            ("https://example.com and more", ItemKind::Text),
            ("ftp://example.com", ItemKind::Text),
        ] {
            assert_eq!(kind_of(text), kind, "{text:?}");
        }
    }

    #[test]
    fn nothing_or_blank_text_saves_nothing() {
        let (mut store, notes, _) = library();
        for text in [None, Some(""), Some(" \n\t ")] {
            let outcome = cap(&mut store, notes, text, now()).expect("capture");
            assert_eq!(outcome, Captured::Empty, "{text:?}");
        }
        assert_eq!(store.items(notes).expect("items"), Vec::new());
    }

    #[test]
    fn text_over_the_limit_is_refused_and_text_at_the_limit_is_kept() {
        let (mut store, notes, _) = library();
        let big = "a".repeat(MAX_TEXT_BYTES + 1);
        let outcome = cap(&mut store, notes, Some(&big), now()).expect("capture");
        assert_eq!(outcome, Captured::TooLarge);
        assert_eq!(store.items(notes).expect("items"), Vec::new());

        // Over the inline limit the bytes go to a blob; they read back whole.
        let large = "b".repeat(snipdeck_core::INLINE_LIMIT + 10);
        let Captured::Saved { item, .. } =
            cap(&mut store, notes, Some(&large), now()).expect("capture")
        else {
            panic!("saved")
        };
        assert_eq!(text_of(&store, item), large);
        assert_eq!(
            store.item(item).expect("item").preview.chars().count(),
            Item::PREVIEW_CHARS
        );
    }

    #[test]
    fn a_failed_write_leaves_no_item_behind() {
        let (mut store, notes, _) = library();
        let gone = Destination {
            id: ListId::generate(),
            name: "Gone".into(),
        };
        let before = store.all_items().expect("items").len();
        assert!(capture_text(&mut store, &gone, Some("text"), now()).is_err());
        assert_eq!(store.all_items().expect("items").len(), before);
        // The library still works afterwards.
        assert!(matches!(
            cap(&mut store, notes, Some("ok"), now()),
            Ok(Captured::Saved { .. })
        ));
    }

    #[test]
    fn the_default_list_is_the_current_one_else_the_first_that_exists() {
        let (store, notes, links) = library();
        assert_eq!(dest(&store, links).name, "Links");
        // No current list, or one that was deleted: the first list of the first board with one
        // (the "Empty" board has none).
        for current in [None, Some(ListId::generate())] {
            let d = default_list(&store, current).expect("list").expect("some");
            assert_eq!((d.id, d.name.as_str()), (notes, "Notes"));
        }
        let bare = Store::open_in_memory().expect("store");
        assert_eq!(default_list(&bare, None).expect("list"), None);
    }
}
