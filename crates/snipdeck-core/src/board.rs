use crate::{BoardId, ListAccent, ListIcon, Name, Timestamp};

/// A top-level workspace such as "Work" or "Personal" (SPEC §4).
///
/// There is no limit on how many boards exist. Manual order is held by storage, not by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    /// Identity.
    pub id: BoardId,
    /// Display name.
    pub name: Name,
    /// Optional colour accent, like a list's.
    pub accent: Option<ListAccent>,
    /// Optional icon, like a list's.
    pub icon: Option<ListIcon>,
    /// When the board was created.
    pub created_at: Timestamp,
    /// When the board was last changed.
    pub updated_at: Timestamp,
}

impl Board {
    /// A new board with a fresh id, created and updated at `now`.
    pub fn new(name: Name, now: Timestamp) -> Self {
        Self {
            id: BoardId::generate(),
            name,
            accent: None,
            icon: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_board_is_fresh() {
        let now = Timestamp::from_second(1_800_000_000).unwrap();
        let work = Board::new(Name::new("Work").unwrap(), now);
        let home = Board::new(Name::new("Home").unwrap(), now);
        assert_ne!(work.id, home.id);
        assert_eq!((work.created_at, work.updated_at), (now, now));
        assert_eq!((work.accent, work.icon), (None, None));
    }
}
