//! Conversions between core values and the column conventions in `docs/architecture.md`:
//! ids as 16-byte blobs, timestamps as `i64` nanoseconds, booleans as 0/1.

use rusqlite::{Connection, OptionalExtension, params};
use snipdeck_core::{BlobRef, BoardId, Content, Inline, ItemId, ListId, Name, TagId, Timestamp};
use uuid::Uuid;

use crate::{Error, Result};

/// An id type that is stored as a 16-byte blob.
pub(crate) trait StoredId: Copy {
    /// Used in [`Error::NotFound`].
    const ENTITY: &'static str;
    fn uuid(&self) -> Uuid;
    fn from_uuid(uuid: Uuid) -> Self;
}

macro_rules! stored_id {
    ($ty:ty, $entity:literal) => {
        impl StoredId for $ty {
            const ENTITY: &'static str = $entity;
            fn uuid(&self) -> Uuid {
                *self.as_uuid()
            }
            fn from_uuid(uuid: Uuid) -> Self {
                <$ty>::from_uuid(uuid)
            }
        }
    };
}

stored_id!(BoardId, "board");
stored_id!(ListId, "list");
stored_id!(ItemId, "item");
stored_id!(TagId, "tag");

/// The 16 bytes bound as an id column.
pub(crate) fn id_bytes<I: StoredId>(id: I) -> [u8; 16] {
    *id.uuid().as_bytes()
}

/// Reads an id column.
pub(crate) fn id_from<I: StoredId>(bytes: &[u8]) -> Result<I> {
    Uuid::from_slice(bytes)
        .map(I::from_uuid)
        .map_err(|_| Error::Corrupt(format!("a {} id is {} bytes long", I::ENTITY, bytes.len())))
}

/// [`Error::NotFound`] for `id`.
pub(crate) fn not_found<I: StoredId>(id: I) -> Error {
    Error::NotFound {
        entity: I::ENTITY,
        id: id.uuid().to_string(),
    }
}

/// A timestamp as nanoseconds since the epoch.
pub(crate) fn nanos(at: Timestamp) -> Result<i64> {
    i64::try_from(at.as_nanosecond()).map_err(|_| {
        Error::Invalid(format!(
            "{at} is outside the years 1677 to 2262 that storage can hold"
        ))
    })
}

/// A stored timestamp column.
pub(crate) fn from_nanos(nanos: i64) -> Result<Timestamp> {
    Timestamp::from_nanosecond(i128::from(nanos))
        .map_err(|e| Error::Corrupt(format!("timestamp {nanos}: {e}")))
}

/// An optional stored timestamp column.
pub(crate) fn from_opt_nanos(nanos: Option<i64>) -> Result<Option<Timestamp>> {
    nanos.map(from_nanos).transpose()
}

/// The payload columns of a content value: inline bytes, or hash and length.
pub(crate) type Payload<'a> = (Option<&'a [u8]>, Option<[u8; 32]>, Option<i64>);

/// Splits `content` into its columns.
pub(crate) fn payload(content: &Content) -> Result<Payload<'_>> {
    match content {
        Content::Inline(inline) => Ok((Some(inline.as_bytes()), None, None)),
        Content::Blob { blob, len } => {
            let len = i64::try_from(*len)
                .map_err(|_| Error::Invalid("a blob of this size cannot be stored".to_owned()))?;
            Ok((None, Some(*blob.as_bytes()), Some(len)))
        }
    }
}

/// Rebuilds content from its columns.
pub(crate) fn content_from(
    inline: Option<Vec<u8>>,
    hash: Option<Vec<u8>>,
    len: Option<i64>,
) -> Result<Content> {
    match (inline, hash, len) {
        (Some(bytes), None, None) => Ok(Content::Inline(Inline::new(bytes)?)),
        (None, Some(hash), Some(len)) => {
            let hash: [u8; 32] = hash
                .try_into()
                .map_err(|_| Error::Corrupt("a blob hash is not 32 bytes".to_owned()))?;
            let len = u64::try_from(len)
                .map_err(|_| Error::Corrupt("a blob has a negative length".to_owned()))?;
            Ok(Content::Blob {
                blob: BlobRef::from_bytes(hash),
                len,
            })
        }
        _ => Err(Error::Corrupt(
            "a payload is neither inline nor a blob".to_owned(),
        )),
    }
}

/// A boolean column.
pub(crate) fn flag(value: bool) -> i64 {
    i64::from(value)
}

/// Whether any row of `table` has this id.
pub(crate) fn exists(conn: &Connection, table: &'static str, id: [u8; 16]) -> Result<bool> {
    Ok(conn
        .query_row(
            &format!("SELECT 1 FROM {table} WHERE id = ?1"),
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Live means the record and every ancestor are out of the trash.
pub(crate) fn board_is_live(conn: &Connection, id: BoardId) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM boards WHERE id = ?1 AND deleted_at IS NULL",
            [id_bytes(id)],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// See [`board_is_live`].
pub(crate) fn list_is_live(conn: &Connection, id: ListId) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM lists l JOIN boards b ON b.id = l.board_id
             WHERE l.id = ?1 AND l.deleted_at IS NULL AND b.deleted_at IS NULL",
            [id_bytes(id)],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// See [`board_is_live`].
pub(crate) fn item_is_live(conn: &Connection, id: ItemId) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM items i JOIN lists l ON l.id = i.list_id JOIN boards b ON b.id = l.board_id
             WHERE i.id = ?1 AND i.deleted_at IS NULL AND l.deleted_at IS NULL
               AND b.deleted_at IS NULL",
            params![id_bytes(id)],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Fails with [`Error::NotFound`] unless the board is live.
pub(crate) fn require_board(conn: &Connection, id: BoardId) -> Result<()> {
    if board_is_live(conn, id)? {
        Ok(())
    } else {
        Err(not_found(id))
    }
}

/// Fails with [`Error::NotFound`] unless the list is live.
pub(crate) fn require_list(conn: &Connection, id: ListId) -> Result<()> {
    if list_is_live(conn, id)? {
        Ok(())
    } else {
        Err(not_found(id))
    }
}

/// Fails with [`Error::NotFound`] unless the item is live.
pub(crate) fn require_item(conn: &Connection, id: ItemId) -> Result<()> {
    if item_is_live(conn, id)? {
        Ok(())
    } else {
        Err(not_found(id))
    }
}

/// Fails with [`Error::AlreadyExists`] if a row (live or trashed) has this id.
pub(crate) fn require_new<I: StoredId>(
    conn: &Connection,
    table: &'static str,
    id: I,
) -> Result<()> {
    if exists(conn, table, id_bytes(id))? {
        Err(Error::AlreadyExists {
            entity: I::ENTITY,
            id: id.uuid().to_string(),
        })
    } else {
        Ok(())
    }
}

/// Maps a core placement to one over raw UUIDs.
pub(crate) fn raw_placement<I: StoredId>(
    at: snipdeck_core::Placement<I>,
) -> snipdeck_core::Placement<Uuid> {
    use snipdeck_core::Placement;
    match at {
        Placement::First => Placement::First,
        Placement::Last => Placement::Last,
        Placement::After(id) => Placement::After(id.uuid()),
    }
}

/// A stored board, list or tag name. One row that is not a valid [`Name`] (written by another
/// tool, or by an older rule) must not fail a whole listing (review L9): it is shown with its
/// control characters replaced and its length capped, and [`Store::check_integrity`]
/// reports it. Saving the record writes the repaired name back.
///
/// [`Store::check_integrity`]: crate::Store::check_integrity
pub(crate) fn stored_name(raw: &str) -> Result<Name> {
    if let Ok(name) = Name::new(raw) {
        return Ok(name);
    }
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(Name::MAX_CHARS)
        .collect();
    Ok(Name::new(&cleaned).or_else(|_| Name::new("Unnamed"))?)
}

/// Whether `raw` is exactly what [`Name::new`] would store.
pub(crate) fn is_valid_name(raw: &str) -> bool {
    Name::new(raw).is_ok_and(|name| name.as_str() == raw)
}
