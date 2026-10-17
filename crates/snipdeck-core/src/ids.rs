//! Typed identifiers.
//!
//! Each id wraps a UUID so a `ListId` can never be passed where an `ItemId` is expected.
//! New ids are UUIDv7: their leading bits are a millisecond timestamp, so ids created later
//! sort later and inserts land at the end of a B-tree index instead of at random pages.
//! Parsing accepts any UUID version, so ids from imports or older data keep working.

use core::fmt;
use core::str::FromStr;

use uuid::Uuid;

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $what:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new, time-ordered (UUIDv7) id.
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wraps a UUID read from storage or an import.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// The underlying UUID, e.g. to bind as a 16-byte SQLite `BLOB`.
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        /// Formats as a lowercase hyphenated UUID.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = crate::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::try_parse(s).map(Self).map_err(|_| crate::Error::Parse {
                    what: $what,
                    value: s.to_owned(),
                })
            }
        }
    };
}

define_id!(
    /// Identifies a [`Board`](crate::Board).
    BoardId,
    "board id"
);
define_id!(
    /// Identifies a [`List`](crate::List).
    ListId,
    "list id"
);
define_id!(
    /// Identifies an [`Item`](crate::Item).
    ItemId,
    "item id"
);
define_id!(
    /// Identifies a [`Tag`](crate::Tag).
    TagId,
    "tag id"
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn generated_ids_are_v7_and_increase() {
        let ids: Vec<ItemId> = (0..1000).map(|_| ItemId::generate()).collect();
        assert!(ids.iter().all(|id| id.as_uuid().get_version_num() == 7));
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn display_round_trips_through_from_str() {
        let id = BoardId::generate();
        let text = id.to_string();
        assert_eq!(text.len(), 36);
        assert_eq!(text, text.to_lowercase());
        assert_eq!(text.parse::<BoardId>(), Ok(id));
    }

    #[test]
    fn parsing_accepts_any_uuid_version_and_rejects_junk() {
        let v4 = "67e55044-10b1-426f-9247-bb680e5fe0c8";
        assert_eq!(
            v4.parse::<TagId>().map(|id| id.to_string()),
            Ok(v4.to_owned())
        );
        assert_eq!(
            "not-an-id".parse::<ListId>(),
            Err(Error::Parse {
                what: "list id",
                value: "not-an-id".to_owned()
            })
        );
    }

    #[test]
    fn uuid_conversions_preserve_the_value() {
        let id = ListId::generate();
        assert_eq!(ListId::from_uuid(Uuid::from(id)), id);
        assert_eq!(*id.as_uuid(), Uuid::from(id));
    }
}
