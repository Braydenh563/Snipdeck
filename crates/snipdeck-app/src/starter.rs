//! What a library needs before the Panel can do anything: a board with a list. A new library
//! has neither, and the Panel cannot make a board itself (only the Main window's board menu can),
//! so without these a first run shows an empty board switcher and an empty board menu, and every
//! control has nothing to act on.

use snipdeck_core::{
    Board, BoardId, Content, Format, Formats, Inline, Item, ItemKind, List, ListId, Name,
    Placement, Timestamp,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo};

/// The kinds of the "Getting started" cards, in order. The titles and texts are words
/// ([`StarterWords::cards`]); the kinds are not.
const CARD_KINDS: [ItemKind; 5] = [
    ItemKind::Text,
    ItemKind::Text,
    ItemKind::Template,
    ItemKind::Text,
    ItemKind::Text,
];

/// The words a new library starts with, in the language selected when they are written: the
/// window layer fills them from the UI crate's `StarterTexts` global (Rust has no catalogues).
/// Names that are persisted are the user's from then on: nothing here looks for them again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarterWords {
    /// The board a new library starts with.
    pub board: String,
    /// The list on it.
    pub list: String,
    /// The first list of a first run: cards that teach by doing (SPEC 5.7b).
    pub getting_started: String,
    /// The `(title, text)` of the five cards of that list, in order.
    pub cards: [(String, String); 5],
    /// The name a list made with the `+` tab starts from (numbered when taken).
    pub new_list: String,
    /// The name a new board's editor starts from (numbered when taken).
    pub new_board: String,
}

impl StarterWords {
    /// The English words: what the controller holds until a window has given its own, and what
    /// tests that do not run a window compare against.
    #[must_use]
    pub fn english() -> Self {
        let card = |title: &str, text: &str| (title.to_owned(), text.to_owned());
        Self {
            board: "My snippets".to_owned(),
            list: "Snippets".to_owned(),
            getting_started: "Getting started".to_owned(),
            cards: [
                card(
                    "Click me to paste",
                    "This text was pasted by clicking its card. Click a card in the Panel and Snipdeck pastes it into the app you were typing in.",
                ),
                card(
                    "Shift+click to copy",
                    "Shift+click (or Shift+Enter) only copies a card to the clipboard, so you can paste it yourself.",
                ),
                card(
                    "Try a template",
                    "Today is {date} and it is {time}. Placeholders in braces fill in when you use the card.",
                ),
                card(
                    "Right-click for more",
                    "Right-click a card, or press the Menu key, to edit it, pin it to the strip above the list, move it, tag it or delete it. Delete this list when you no longer need it.",
                ),
                card(
                    "More tips",
                    "Help > Tips has ten short cards with a picture and a Try it button each. Open it from Help, or from the tray icon's menu.",
                ),
            ],
            new_list: "New list".to_owned(),
            new_board: "New board".to_owned(),
        }
    }
}

/// The name the list and board editors start with for a new one: `base` and the first number
/// (from 1) no name in `taken` already has, compared without regard to case. The editor selects
/// it so typing replaces it.
#[must_use]
pub fn numbered_name<'a>(base: &str, taken: impl IntoIterator<Item = &'a str>) -> String {
    let taken: Vec<String> = taken.into_iter().map(str::to_lowercase).collect();
    // One more candidate than there are names, so one of them is always free.
    (1..=taken.len() + 1)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken.contains(&name.to_lowercase()))
        .unwrap_or_else(|| base.to_owned())
}

/// Gives a library without boards one board with one list. Returns whether it made them.
///
/// # Errors
///
/// Any store error.
pub fn ensure_board<S>(
    store: &mut S,
    words: &StarterWords,
    now: Timestamp,
) -> snipdeck_store::Result<bool>
where
    S: BoardRepo + ListRepo,
{
    if !store.boards()?.is_empty() {
        return Ok(false);
    }
    make_starter(store, words, now)?;
    Ok(true)
}

/// [`ensure_board`] for a first run: when it makes the starter board it also puts a
/// "Getting started" list in front of the starter list. Returns whether it was a first run.
///
/// # Errors
///
/// Any store error.
pub fn ensure_first_run<S>(
    store: &mut S,
    words: &StarterWords,
    now: Timestamp,
) -> snipdeck_store::Result<bool>
where
    S: BoardRepo + ListRepo + ItemRepo,
{
    if !ensure_board(store, words, now)? {
        return Ok(false);
    }
    let Some(board) = store.boards()?.first().map(|b| b.id) else {
        return Ok(true);
    };
    let list = List::new(board, Name::new(&words.getting_started)?, now);
    store.insert_list(&list, Placement::First)?;
    for ((title, text), kind) in words.cards.iter().zip(CARD_KINDS) {
        let mut item = Item::new(list.id, title.clone(), kind, now);
        item.preview = Item::preview_of(text, false);
        let content = Content::Inline(Inline::new(text.as_bytes().to_vec())?);
        let formats = Formats::new(vec![Format::Text(content)])?;
        store.insert_item(&item, &formats, Placement::Last)?;
    }
    Ok(true)
}

/// Whether the library holds nothing of the user's: no items, or only the "Getting started"
/// cards. The welcome tour opens on such a library.
///
/// That list is found by its shape, not its name (the name is in the language selected when it
/// was made): the first list of the first board, holding exactly the five cards in order, none
/// of them edited since they were made.
///
/// # Errors
///
/// Any store error.
pub fn library_is_untouched<S>(store: &S) -> snipdeck_store::Result<bool>
where
    S: BoardRepo + ListRepo + ItemRepo,
{
    let boards = store.boards()?;
    let seeded = match boards.first() {
        Some(board) => store.lists(board.id)?.first().map(|l| l.id),
        None => None,
    };
    for board in &boards {
        for list in store.lists(board.id)? {
            let items = store.items(list.id)?;
            if items.is_empty() || (Some(list.id) == seeded && is_getting_started(&items)) {
                continue;
            }
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `items` are the five seeded cards, as they were made.
fn is_getting_started(items: &[Item]) -> bool {
    items.len() == CARD_KINDS.len()
        && items
            .iter()
            .zip(CARD_KINDS)
            .all(|(item, kind)| item.kind == kind && item.updated_at == item.created_at)
}

fn make_starter<S>(
    store: &mut S,
    words: &StarterWords,
    now: Timestamp,
) -> snipdeck_store::Result<(BoardId, ListId)>
where
    S: BoardRepo + ListRepo,
{
    let board = Board::new(Name::new(&words.board)?, now);
    store.insert_board(&board, Placement::Last)?;
    let list = List::new(board.id, Name::new(&words.list)?, now);
    store.insert_list(&list, Placement::Last)?;
    Ok((board.id, list.id))
}

/// Adds a list at the end of `board` (`words.new_list`, then "... 2", ...). Without a board (or with
/// one that is gone) it makes the starter board and returns its list instead.
///
/// # Errors
///
/// Any store error.
pub fn add_list<S>(
    store: &mut S,
    board: Option<BoardId>,
    words: &StarterWords,
    now: Timestamp,
) -> snipdeck_store::Result<(BoardId, ListId)>
where
    S: BoardRepo + ListRepo,
{
    let board = match board {
        Some(id) if store.boards()?.iter().any(|b| b.id == id) => id,
        _ => return make_starter(store, words, now),
    };
    let taken: Vec<String> = store
        .lists(board)?
        .iter()
        .map(|l| l.name.as_str().to_owned())
        .collect();
    // One more candidate than there are lists, so one of them is always free.
    let name = (1..=taken.len() + 1)
        .map(|n| {
            if n == 1 {
                words.new_list.clone()
            } else {
                format!("{} {n}", words.new_list)
            }
        })
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| words.new_list.clone());
    let list = List::new(board, Name::new(&name)?, now);
    store.insert_list(&list, Placement::Last)?;
    Ok((board, list.id))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use snipdeck_store::Store;

    fn english() -> StarterWords {
        StarterWords::english()
    }

    /// Words in another language (made up, so no English word can match by accident).
    fn german() -> StarterWords {
        let card = |n: usize| (format!("Karte {n}"), format!("Text der Karte {n}"));
        StarterWords {
            board: "Meine Bausteine".into(),
            list: "Bausteine".into(),
            getting_started: "Erste Schritte".into(),
            cards: [card(1), card(2), card(3), card(4), card(5)],
            new_list: "Neue Liste".into(),
            new_board: "Neues Board".into(),
        }
    }

    fn now() -> Timestamp {
        "2026-10-18T10:00:00Z".parse().unwrap()
    }

    fn names(store: &Store, board: BoardId) -> Vec<String> {
        store
            .lists(board)
            .unwrap()
            .iter()
            .map(|l| l.name.as_str().to_owned())
            .collect()
    }

    #[test]
    fn a_numbered_name_takes_the_first_free_number() {
        assert_eq!(numbered_name("New list", []), "New list 1");
        assert_eq!(
            numbered_name("New list", ["Snippets", "new list 1", "New list 3"]),
            "New list 2"
        );
        assert_eq!(numbered_name("New board", ["New list 1"]), "New board 1");
    }

    #[test]
    fn an_empty_library_gets_one_board_with_one_list_once() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(ensure_board(&mut store, &english(), now()).unwrap());
        assert!(
            !ensure_board(&mut store, &english(), now()).unwrap(),
            "only the first time"
        );
        let boards = store.boards().unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].name.as_str(), english().board.as_str());
        assert_eq!(names(&store, boards[0].id), [english().list.as_str()]);
    }

    #[test]
    fn a_first_run_gets_a_getting_started_list_in_front_once() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(ensure_first_run(&mut store, &english(), now()).unwrap());
        assert!(!ensure_first_run(&mut store, &english(), now()).unwrap());
        let board = store.boards().unwrap()[0].id;
        assert_eq!(
            names(&store, board),
            [english().getting_started.as_str(), english().list.as_str()]
        );
        let first = store.lists(board).unwrap()[0].id;
        let titles: Vec<String> = store
            .items(first)
            .unwrap()
            .iter()
            .map(|i| i.title.clone())
            .collect();
        assert_eq!(
            titles,
            [
                "Click me to paste",
                "Shift+click to copy",
                "Try a template",
                "Right-click for more",
                "More tips"
            ]
        );
        let starter = store.lists(board).unwrap()[1].id;
        assert_eq!(store.items(starter).unwrap().len(), 0);
    }

    #[test]
    fn a_library_with_a_board_gets_no_getting_started_list() {
        let mut store = crate::demo::store(now()).unwrap();
        assert!(!ensure_first_run(&mut store, &english(), now()).unwrap());
        let board = store.boards().unwrap()[0].id;
        assert!(!names(&store, board).contains(&english().getting_started));
    }

    #[test]
    fn the_getting_started_cards_do_not_count_as_the_users_library() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(library_is_untouched(&store).unwrap());
        ensure_first_run(&mut store, &english(), now()).unwrap();
        assert!(
            library_is_untouched(&store).unwrap(),
            "only the seeded cards"
        );
        let board = store.boards().unwrap()[0].id;
        let own = store.lists(board).unwrap()[1].id;
        let item = Item::new(own, "Mine".into(), ItemKind::Text, now());
        let content = Content::Inline(Inline::new(b"x".to_vec()).unwrap());
        store
            .insert_item(
                &item,
                &Formats::new(vec![Format::Text(content)]).unwrap(),
                Placement::Last,
            )
            .unwrap();
        assert!(!library_is_untouched(&store).unwrap());
    }

    #[test]
    fn a_library_with_a_board_is_left_alone() {
        let mut store = crate::demo::store(now()).unwrap();
        let before = store.boards().unwrap().len();
        assert!(!ensure_board(&mut store, &english(), now()).unwrap());
        assert_eq!(store.boards().unwrap().len(), before);
    }

    #[test]
    fn new_lists_are_numbered_and_go_last() {
        let mut store = Store::open_in_memory().unwrap();
        ensure_board(&mut store, &english(), now()).unwrap();
        let board = store.boards().unwrap()[0].id;
        let (b1, _) = add_list(&mut store, Some(board), &english(), now()).unwrap();
        let (b2, second) = add_list(&mut store, Some(board), &english(), now()).unwrap();
        assert_eq!((b1, b2), (board, board));
        assert_eq!(
            names(&store, board),
            [english().list.as_str(), "New list", "New list 2"]
        );
        assert_eq!(store.lists(board).unwrap()[2].id, second);
    }

    #[test]
    fn adding_a_list_without_a_board_makes_the_starter_board() {
        let mut store = Store::open_in_memory().unwrap();
        let (board, list) = add_list(&mut store, None, &english(), now()).unwrap();
        assert_eq!(store.boards().unwrap()[0].id, board);
        assert_eq!(store.lists(board).unwrap()[0].id, list);
    }

    fn put_item(store: &mut Store, list: ListId, title: &str, at: Timestamp) {
        let item = Item::new(list, title.into(), ItemKind::Text, at);
        let content = Content::Inline(Inline::new(b"x".to_vec()).unwrap());
        store
            .insert_item(
                &item,
                &Formats::new(vec![Format::Text(content)]).unwrap(),
                Placement::Last,
            )
            .unwrap();
    }

    #[test]
    fn a_first_run_in_another_language_writes_only_those_words() {
        let words = german();
        let mut store = Store::open_in_memory().unwrap();
        assert!(ensure_first_run(&mut store, &words, now()).unwrap());
        let board = store.boards().unwrap()[0].clone();
        assert_eq!(board.name.as_str(), "Meine Bausteine");
        assert_eq!(names(&store, board.id), ["Erste Schritte", "Bausteine"]);
        let first = store.lists(board.id).unwrap()[0].id;
        let items = store.items(first).unwrap();
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Karte 1", "Karte 2", "Karte 3", "Karte 4", "Karte 5"]
        );
        let (b, added) = add_list(&mut store, Some(board.id), &words, now()).unwrap();
        assert_eq!(b, board.id);
        assert!(names(&store, board.id).contains(&"Neue Liste".to_owned()));
        assert_eq!(store.lists(board.id).unwrap().last().unwrap().id, added);
        // Nothing of the English words leaked into anything that was written.
        let english = english();
        let mut written: Vec<String> = vec![board.name.as_str().to_owned()];
        written.extend(names(&store, board.id));
        written.extend(items.iter().map(|i| i.title.clone()));
        written.extend(items.iter().map(|i| i.preview.clone()));
        let leaks = [
            english.board.as_str(),
            english.list.as_str(),
            english.getting_started.as_str(),
            english.new_list.as_str(),
        ];
        for text in &written {
            for english_word in leaks
                .into_iter()
                .chain(english.cards.iter().map(|c| c.0.as_str()))
            {
                assert!(
                    !text.contains(english_word),
                    "`{text}` holds the English `{english_word}`"
                );
            }
        }
    }

    #[test]
    fn a_seeded_list_in_another_language_does_not_count_as_the_users_library() {
        let mut store = Store::open_in_memory().unwrap();
        ensure_first_run(&mut store, &german(), now()).unwrap();
        assert!(library_is_untouched(&store).unwrap(), "found by its shape");
        let board = store.boards().unwrap()[0].id;
        let own = store.lists(board).unwrap()[1].id;
        put_item(&mut store, own, "Mine", now());
        assert!(!library_is_untouched(&store).unwrap());
    }

    #[test]
    fn an_edited_or_added_card_in_the_seeded_list_makes_the_library_the_users() {
        let mut store = Store::open_in_memory().unwrap();
        ensure_first_run(&mut store, &german(), now()).unwrap();
        let board = store.boards().unwrap()[0].id;
        let seeded = store.lists(board).unwrap()[0].id;
        put_item(&mut store, seeded, "Mine", now());
        assert!(!library_is_untouched(&store).unwrap(), "a sixth card");

        let mut store = Store::open_in_memory().unwrap();
        ensure_first_run(&mut store, &english(), now()).unwrap();
        let board = store.boards().unwrap()[0].id;
        let seeded = store.lists(board).unwrap()[0].id;
        let mut first = store.items(seeded).unwrap().remove(0);
        first.updated_at = "2026-10-19T10:00:00Z".parse().unwrap();
        store.update_item(&first, None).unwrap();
        assert!(!library_is_untouched(&store).unwrap(), "an edited card");
    }
}
