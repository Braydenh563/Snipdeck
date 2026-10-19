//! What a library needs before the Panel can do anything: a board with a list. A new library
//! has neither, and the Panel cannot make a board itself (only the Main window's board menu can),
//! so without these a first run shows an empty board switcher and an empty board menu, and every
//! control has nothing to act on.

use snipdeck_core::{Board, BoardId, List, ListId, Name, Placement, Timestamp};
use snipdeck_store::{BoardRepo, ListRepo};

/// The board a new library starts with.
pub const STARTER_BOARD: &str = "My snippets";
/// The list on it.
pub const STARTER_LIST: &str = "Snippets";
/// The name of a list made with the `+` tab (numbered when taken).
pub const NEW_LIST: &str = "New list";

/// The name the list and board editors start with for a new one: `base` and the first number
/// (from 1) no name in `taken` already has, compared without regard to case. The editor selects
/// it so typing replaces it.
#[must_use]
pub fn numbered_name<'a>(base: &str, taken: impl IntoIterator<Item = &'a str>) -> String {
    let taken: Vec<String> = taken.into_iter().map(str::to_lowercase).collect();
    (1_u32..)
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
