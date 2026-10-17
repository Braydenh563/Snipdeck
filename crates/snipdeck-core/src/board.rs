use crate::{BoardId, Name, Timestamp};

/// A top-level workspace such as "Work" or "Personal" (SPEC §4).
///
/// Free users can create up to [`Entitlements::FREE_BOARD_LIMIT`](crate::Entitlements::FREE_BOARD_LIMIT)
/// boards. Manual order is held by storage, not by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    /// Identity.
    pub id: BoardId,
    /// Display name.
    pub name: Name,
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
    }
}
