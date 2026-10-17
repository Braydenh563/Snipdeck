use crate::{BoardId, ListId, Name, Timestamp};

keyed_enum! {
    /// A list's colour, shown like an index-card divider tab: the active tab's underline,
    /// the sidebar mark and the label in mixed views, never on cards (DESIGN.md §2).
    pub enum ListAccent ("list accent") {
        /// Blue.
        Blue = "blue",
        /// Teal.
        Teal = "teal",
        /// Green.
        Green = "green",
        /// Amber.
        Amber = "amber",
        /// Orange.
        Orange = "orange",
        /// Red.
        Red = "red",
        /// Rose.
        Rose = "rose",
        /// Violet.
        Violet = "violet",
        /// Slate.
        Slate = "slate",
    }
}

keyed_enum! {
    /// How a list's items are drawn in the panel (SPEC §5.1).
    pub enum ListView ("list view") {
        /// One-line rows.
        Compact = "compact",
        /// Two to three preview lines and image thumbnails.
        Comfortable = "comfortable",
    }
}

keyed_enum! {
    /// How a list orders its items in the panel (SPEC §5.1).
    pub enum ListSort ("list sort") {
        /// The user's drag order.
        Manual = "manual",
        /// Highest [`Item::use_count`](crate::Item::use_count) first.
        MostUsed = "most-used",
        /// Latest [`Item::last_used_at`](crate::Item::last_used_at) first.
        RecentlyUsed = "recently-used",
    }
}

/// An ordered collection of items inside a board, shown as a panel tab (SPEC §4).
///
/// Manual order, of the list within its board, is held by storage, not by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// Identity.
    pub id: ListId,
    /// The board this list belongs to.
    pub board_id: BoardId,
    /// Display name.
    pub name: Name,
    /// Optional colour accent.
    pub accent: Option<ListAccent>,
    /// Panel row style.
    pub view: ListView,
    /// Panel item order.
    pub sort: ListSort,
    /// When the list was created.
    pub created_at: Timestamp,
    /// When the list was last changed.
    pub updated_at: Timestamp,
}

impl List {
    /// A new list with a fresh id, no accent, the comfortable view and manual sort.
    pub fn new(board_id: BoardId, name: Name, now: Timestamp) -> Self {
        Self {
            id: ListId::generate(),
            board_id,
            name,
            accent: None,
            view: ListView::Comfortable,
            sort: ListSort::Manual,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::assert_keys_round_trip;

    #[test]
    fn keys_round_trip() {
        assert_keys_round_trip(ListAccent::ALL, ListAccent::key);
        assert_keys_round_trip(ListView::ALL, ListView::key);
        assert_keys_round_trip(ListSort::ALL, ListSort::key);
    }

    #[test]
    fn there_are_nine_list_accents() {
        assert_eq!(ListAccent::ALL.len(), 9);
    }

    #[test]
    fn new_list_defaults() {
        let now = Timestamp::from_second(1_800_000_000).unwrap();
        let board = BoardId::generate();
        let list = List::new(board, Name::new("Email replies").unwrap(), now);
        assert_eq!(list.board_id, board);
        assert_eq!(
            (list.accent, list.view, list.sort),
            (None, ListView::Comfortable, ListSort::Manual)
        );
        assert_eq!((list.created_at, list.updated_at), (now, now));
    }
}
