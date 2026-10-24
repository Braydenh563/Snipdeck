//! What a library needs before the Panel can do anything: a board with a list. A new library
//! has neither, and the Panel cannot make a board itself (only the Main window's board menu can),
//! so without these a first run shows an empty board switcher and an empty board menu, and every
//! control has nothing to act on.

use snipdeck_core::{
    Board, BoardId, Content, Format, Formats, Inline, Item, ItemKind, List, ListId, Name,
    Placement, Timestamp,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo};

/// The board a new library starts with.
pub const STARTER_BOARD: &str = "My snippets";
/// The list on it.
pub const STARTER_LIST: &str = "Snippets";
/// The first list of a first run: cards that teach by doing (SPEC 5.7b).
pub const GETTING_STARTED_LIST: &str = "Getting started";
/// The cards of [`GETTING_STARTED_LIST`]: title, kind and text.
const GETTING_STARTED: [(&str, ItemKind, &str); 4] = [
    (
        "Click me to paste",
        ItemKind::Text,
        "This text was pasted by clicking its card. Click a card in the Panel and Snipdeck pastes it into the app you were typing in.",
    ),
    (
        "Shift+click to copy",
        ItemKind::Text,
        "Shift+click (or Shift+Enter) only copies a card to the clipboard, so you can paste it yourself.",
    ),
    (
        "Try a template",
        ItemKind::Template,
        "Today is {date} and it is {time}. Placeholders in braces fill in when you use the card.",
    ),
    (
        "Right-click for more",
        ItemKind::Text,
        "Right-click a card, or press the Menu key, to edit it, pin it to the strip above the list, move it, tag it or delete it. Delete this list when you no longer need it.",
    ),
];
/// The name of a list made with the `+` tab (numbered when taken).
pub const NEW_LIST: &str = "New list";
/// The name a new board's editor starts from (numbered when taken).
pub const NEW_BOARD: &str = "New board";

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
pub fn ensure_board<S>(store: &mut S, now: Timestamp) -> snipdeck_store::Result<bool>
where
    S: BoardRepo + ListRepo,
{
    if !store.boards()?.is_empty() {
        return Ok(false);
    }
    make_starter(store, now)?;
    Ok(true)
}

/// [`ensure_board`] for a first run: when it makes the starter board it also puts a
/// "Getting started" list in front of the starter list. Returns whether it was a first run.
///
/// # Errors
///
/// Any store error.
pub fn ensure_first_run<S>(store: &mut S, now: Timestamp) -> snipdeck_store::Result<bool>
where
    S: BoardRepo + ListRepo + ItemRepo,
{
    if !ensure_board(store, now)? {
        return Ok(false);
    }
    let Some(board) = store.boards()?.first().map(|b| b.id) else {
        return Ok(true);
    };
    let list = List::new(board, Name::new(GETTING_STARTED_LIST)?, now);
    store.insert_list(&list, Placement::First)?;
    for (title, kind, text) in GETTING_STARTED {
        let mut item = Item::new(list.id, title.into(), kind, now);
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
/// # Errors
///
/// Any store error.
pub fn library_is_untouched<S>(store: &S) -> snipdeck_store::Result<bool>
where
    S: BoardRepo + ListRepo + ItemRepo,
{
    for board in store.boards()? {
        for list in store.lists(board.id)? {
            if list.name.as_str() != GETTING_STARTED_LIST && !store.items(list.id)?.is_empty() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn make_starter<S>(store: &mut S, now: Timestamp) -> snipdeck_store::Result<(BoardId, ListId)>
where
    S: BoardRepo + ListRepo,
{
    let board = Board::new(Name::new(STARTER_BOARD)?, now);
    store.insert_board(&board, Placement::Last)?;
    let list = List::new(board.id, Name::new(STARTER_LIST)?, now);
    store.insert_list(&list, Placement::Last)?;
    Ok((board.id, list.id))
}

/// Adds a list at the end of `board` ("New list", "New list 2", ...). Without a board (or with
/// one that is gone) it makes the starter board and returns its list instead.
///
/// # Errors
///
/// Any store error.
pub fn add_list<S>(
    store: &mut S,
    board: Option<BoardId>,
    now: Timestamp,
) -> snipdeck_store::Result<(BoardId, ListId)>
where
    S: BoardRepo + ListRepo,
{
    let board = match board {
        Some(id) if store.boards()?.iter().any(|b| b.id == id) => id,
        _ => return make_starter(store, now),
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
                NEW_LIST.to_owned()
            } else {
                format!("{NEW_LIST} {n}")
            }
        })
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| NEW_LIST.to_owned());
    let list = List::new(board, Name::new(&name)?, now);
    store.insert_list(&list, Placement::Last)?;
    Ok((board, list.id))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test code")]

    use super::*;
    use snipdeck_store::Store;

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
        assert!(ensure_board(&mut store, now()).unwrap());
        assert!(
            !ensure_board(&mut store, now()).unwrap(),
            "only the first time"
        );
        let boards = store.boards().unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].name.as_str(), STARTER_BOARD);
        assert_eq!(names(&store, boards[0].id), [STARTER_LIST]);
    }

    #[test]
    fn a_first_run_gets_a_getting_started_list_in_front_once() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(ensure_first_run(&mut store, now()).unwrap());
        assert!(!ensure_first_run(&mut store, now()).unwrap());
        let board = store.boards().unwrap()[0].id;
        assert_eq!(names(&store, board), [GETTING_STARTED_LIST, STARTER_LIST]);
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
                "Right-click for more"
            ]
        );
        let starter = store.lists(board).unwrap()[1].id;
        assert_eq!(store.items(starter).unwrap().len(), 0);
    }

    #[test]
    fn a_library_with_a_board_gets_no_getting_started_list() {
        let mut store = crate::demo::store(now()).unwrap();
        assert!(!ensure_first_run(&mut store, now()).unwrap());
        let board = store.boards().unwrap()[0].id;
        assert!(!names(&store, board).contains(&GETTING_STARTED_LIST.to_owned()));
    }

    #[test]
    fn the_getting_started_cards_do_not_count_as_the_users_library() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(library_is_untouched(&store).unwrap());
        ensure_first_run(&mut store, now()).unwrap();
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
        assert!(!ensure_board(&mut store, now()).unwrap());
        assert_eq!(store.boards().unwrap().len(), before);
    }

    #[test]
    fn new_lists_are_numbered_and_go_last() {
        let mut store = Store::open_in_memory().unwrap();
        ensure_board(&mut store, now()).unwrap();
        let board = store.boards().unwrap()[0].id;
        let (b1, _) = add_list(&mut store, Some(board), now()).unwrap();
        let (b2, second) = add_list(&mut store, Some(board), now()).unwrap();
        assert_eq!((b1, b2), (board, board));
        assert_eq!(names(&store, board), [STARTER_LIST, NEW_LIST, "New list 2"]);
        assert_eq!(store.lists(board).unwrap()[2].id, second);
    }

    #[test]
    fn adding_a_list_without_a_board_makes_the_starter_board() {
        let mut store = Store::open_in_memory().unwrap();
        let (board, list) = add_list(&mut store, None, now()).unwrap();
        assert_eq!(store.boards().unwrap()[0].id, board);
        assert_eq!(store.lists(board).unwrap()[0].id, list);
    }
}
