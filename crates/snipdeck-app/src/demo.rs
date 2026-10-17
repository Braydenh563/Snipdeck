//! A seeded library for `--demo` and the screenshot: enough variety to show every row kind,
//! the pinned strip, three tabs and a second board.

use snipdeck_core::{
    Board, Content, Format, Formats, Inline, Item, ItemId, ItemKind, List, ListAccent, ListId,
    Name, Placement, Timestamp,
};
use snipdeck_platform::route::{Focus, Target};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, Store};

/// The window the demo pretends the user came from.
#[must_use]
pub fn target() -> Target {
    Target {
        app: "OUTLOOK.EXE".into(),
        title: "New message".into(),
        elevated: false,
        focus: Focus::Caret,
    }
}

/// An in-memory store with the demo library.
///
/// # Errors
///
/// Any store error.
pub fn store(now: Timestamp) -> snipdeck_store::Result<Store> {
    let mut store = Store::open_in_memory()?;
    seed(&mut store, now)?;
    Ok(store)
}

struct Seed {
    kind: ItemKind,
    title: &'static str,
    text: &'static str,
    pinned: bool,
    sensitive: bool,
}

const fn seed_item(kind: ItemKind, title: &'static str, text: &'static str) -> Seed {
    Seed {
        kind,
        title,
        text,
        pinned: false,
        sensitive: false,
    }
}

/// Fills `store` with the demo library.
///
/// # Errors
///
/// Any store error.
pub fn seed(store: &mut Store, now: Timestamp) -> snipdeck_store::Result<()> {
    let work = Board::new(Name::new("Work")?, now);
    store.insert_board(&work, Placement::Last)?;
    let personal = Board::new(Name::new("Personal")?, now);
    store.insert_board(&personal, Placement::Last)?;

    let replies = list(store, &work, "Email replies", Some(ListAccent::Teal), now)?;
    let addresses = list(store, &work, "Addresses", Some(ListAccent::Amber), now)?;
    let code = list(store, &work, "Code", Some(ListAccent::Violet), now)?;
    let home = list(store, &personal, "Errands", Some(ListAccent::Green), now)?;

    let replies_items = [
        Seed {
            pinned: true,
            ..seed_item(
                ItemKind::Text,
                "Thanks, received",
                "Thanks for sending this over. I'll review it today and get back to you.",
            )
        },
        seed_item(
            ItemKind::Template,
            "Invoice reminder",
            "Hi {field:Name}, a gentle reminder that invoice {field:Number} is due on {date}.",
        ),
        seed_item(
            ItemKind::Text,
            "Out of office",
            "I'm away until Monday with limited email. For anything urgent, please call the office.",
        ),
        seed_item(
            ItemKind::Text,
            "Meeting follow-up",
            "Following up on today's call. Summary and next steps are below.",
        ),
        seed_item(
            ItemKind::Link,
            "Design spec",
            "https://figma.com/file/snipdeck-index-deck",
        ),
        Seed {
            sensitive: true,
            ..seed_item(ItemKind::Text, "Server password", "hunter2-correct-horse")
        },
    ];
    let address_items = [
        Seed {
            pinned: true,
            ..seed_item(
                ItemKind::Text,
                "Home address",
                "12 Harbour Street\nFremantle WA 6160",
            )
        },
        seed_item(
            ItemKind::Text,
            "Office",
            "Level 4, 100 St Georges Terrace, Perth WA 6000",
        ),
    ];
    let code_items = [
        Seed {
            pinned: true,
            ..seed_item(
                ItemKind::Code,
                "Retry helper",
                "async fn retry<T>(f: impl Fn() -> T, n: usize) -> T",
            )
        },
        seed_item(
            ItemKind::Code,
            "Recent customers",
            "SELECT id, name FROM customers WHERE created_at > now() - interval '7 days'",
        ),
    ];
    let home_items = [seed_item(
        ItemKind::Text,
        "Shopping list",
        "milk, eggs, coffee",
    )];

    for (list, items) in [
        (replies, &replies_items[..]),
        (addresses, &address_items[..]),
        (code, &code_items[..]),
        (home, &home_items[..]),
    ] {
        for seed in items {
            insert(store, list, seed, now)?;
        }
    }
    Ok(())
}

fn list(
    store: &mut Store,
    board: &Board,
    title: &str,
    accent: Option<ListAccent>,
    now: Timestamp,
) -> snipdeck_store::Result<ListId> {
    let mut list = List::new(board.id, Name::new(title)?, now);
    list.accent = accent;
    store.insert_list(&list, Placement::Last)?;
    Ok(list.id)
}

fn insert(
    store: &mut Store,
    list: ListId,
    seed: &Seed,
    now: Timestamp,
) -> snipdeck_store::Result<ItemId> {
    let mut item = Item::new(list, seed.title.into(), seed.kind, now);
    item.pinned = seed.pinned;
    item.sensitive = seed.sensitive;
    item.preview = Item::preview_of(seed.text, seed.sensitive);
    let content = Content::Inline(Inline::new(seed.text.as_bytes().to_vec())?);
    let formats = Formats::new(vec![Format::Text(content)])?;
    store.insert_item(&item, &formats, Placement::Last)?;
    Ok(item.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::PanelModel;

    fn now() -> Timestamp {
        "2026-10-17T09:00:00Z".parse().expect("timestamp")
    }

    #[test]
    fn the_demo_library_has_two_boards_three_tabs_and_pinned_items() {
        let store = store(now()).expect("demo store");
        let model = PanelModel::load(&store, None, None).expect("model");
        assert_eq!(model.boards().len(), 2);
        let tabs = model.tabs();
        assert_eq!(tabs.len(), 3);
        assert_eq!(model.rows().len(), 6);
        assert_eq!(model.pinned().len(), 3);
        assert!(model.rows().iter().any(|r| r.sensitive));
    }

    #[test]
    fn every_demo_name_is_valid() {
        for text in [
            "Work",
            "Personal",
            "Email replies",
            "Addresses",
            "Code",
            "Errands",
        ] {
            assert!(Name::new(text).is_ok(), "{text}");
        }
    }
}
