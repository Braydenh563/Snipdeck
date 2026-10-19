//! Repository traits: the storage contract the app and UI build on.
//!
//! Rules shared by every trait:
//! - **Live only.** Reads return live records; anything trashed, or inside a trashed list or
//!   board, is invisible until restored (see [`TrashRepo`]).
//! - **Order belongs to storage.** Collections come back in the user's manual order, and
//!   [`Placement`] says where an insert or move goes.
//! - **Whole writes.** `update_*` overwrites every field of the record with what it is given,
//!   except an item's usage (`use_count`, `last_used_at`), which only
//!   [`ItemRepo::record_use`] changes after the insert. If the parent id changed (an item's
//!   `list_id`, a list's `board_id`), the record moves to the end of the new parent; use
//!   `move_*` to choose the position.
//! - **Missing ids** fail with [`Error::NotFound`](crate::Error::NotFound), including a
//!   [`Placement::After`] sibling that is not in the target container, and the parent of a
//!   listing ([`ListRepo::lists`], [`ItemRepo::items`]) that is missing or in the trash.
//! - **Sensitive items** ([`Item::sensitive`](snipdeck_core::Item::sensitive)) are indexed by
//!   title only. Once encryption is set up
//!   ([`Store::create_key_ring`](crate::Store::create_key_ring)), their content is stored
//!   encrypted and read back decrypted while the key ring is unlocked; while it is locked,
//!   reading that content, writing new content of a sensitive item, marking an item
//!   sensitive or clearing the flag fails with
//!   [`Error::EncryptionLocked`](crate::Error::EncryptionLocked).

use snipdeck_core::{
    Board, BoardId, Formats, Item, ItemId, List, ListId, Placement, Tag, TagId, Timestamp,
};

use crate::Result;

/// Boards.
pub trait BoardRepo {
    /// Live boards, in manual order.
    fn boards(&self) -> Result<Vec<Board>>;

    /// One live board.
    fn board(&self, id: BoardId) -> Result<Board>;

    /// Adds a new board at `at`. The Free board limit is the caller's check
    /// ([`Entitlements::can_create_board`](snipdeck_core::Entitlements::can_create_board));
    /// restoring from the trash is limited by
    /// [`Store::restore_with`](crate::Store::restore_with).
    fn insert_board(&mut self, board: &Board, at: Placement<BoardId>) -> Result<()>;

    /// Overwrites a board.
    fn update_board(&mut self, board: &Board) -> Result<()>;

    /// Reorders a board.
    fn move_board(&mut self, id: BoardId, at: Placement<BoardId>) -> Result<()>;
}

/// Lists.
pub trait ListRepo {
    /// The board's live lists, in manual order.
    fn lists(&self, board: BoardId) -> Result<Vec<List>>;

    /// One live list.
    fn list(&self, id: ListId) -> Result<List>;

    /// Adds a new list to `list.board_id` at `at`.
    fn insert_list(&mut self, list: &List, at: Placement<ListId>) -> Result<()>;

    /// Overwrites a list.
    fn update_list(&mut self, list: &List) -> Result<()>;

    /// Moves a list to `at` within `board`, which may be its current board.
    fn move_list(&mut self, id: ListId, board: BoardId, at: Placement<ListId>) -> Result<()>;
}

/// Items and their content.
pub trait ItemRepo {
    /// The list's live items, in manual order, without content.
    fn items(&self, list: ListId) -> Result<Vec<Item>>;

    /// Every live item, without content, for the picker's in-memory search index.
    fn all_items(&self) -> Result<Vec<Item>>;

    /// A number that grows whenever anything is written to the library, so a cache of what
    /// was read can tell that it may be stale: equal values mean nothing was written in
    /// between.
    ///
    /// It counts every row SQLite changed through this handle (boards, lists, items, tags,
    /// the trash, the search tables), in a [`Store`](crate::Store) and in a
    /// [`Batch`](crate::Batch) alike. It never decreases, and it is not undone by a rollback,
    /// so a failed write can at worst cost one needless refresh. Reads leave it alone. It is
    /// only comparable with values from the same open handle (it starts at a small number on
    /// every open) and says nothing about writes by another process, which the single-writer
    /// lock rules out.
    fn change_counter(&self) -> u64;

    /// One live item, without content.
    fn item(&self, id: ItemId) -> Result<Item>;

    /// The item's content, for using, editing or exporting it; decrypted if it is stored
    /// encrypted. Decrypted payloads too large to be inline are held in memory only and are
    /// read through [`BlobStore::read`](crate::BlobStore::read) like any blob.
    fn formats(&self, id: ItemId) -> Result<Formats>;

    /// Adds a new item with its content to `item.list_id` at `at`.
    fn insert_item(&mut self, item: &Item, formats: &Formats, at: Placement<ItemId>) -> Result<()>;

    /// Overwrites an item (not its usage), and its content too when `formats` is `Some`, in one
    /// transaction. Changing `sensitive` with `formats` `None` encrypts or decrypts the stored
    /// content as needed and updates the search index; other metadata edits do not read the
    /// content.
    fn update_item(&mut self, item: &Item, formats: Option<&Formats>) -> Result<()>;

    /// Moves an item to `at` within `list`, which may be its current list.
    fn move_item(&mut self, id: ItemId, list: ListId, at: Placement<ItemId>) -> Result<()>;

    /// Counts one use at `at`, as [`Item::record_use`] does, without loading the item.
    fn record_use(&mut self, id: ItemId, at: Timestamp) -> Result<()>;

    /// Full-content search (FTS5) over live items, best match first, at most `limit` ids.
    /// Sensitive items match on their title only.
    fn search(&self, query: &str, limit: usize) -> Result<Vec<ItemId>>;
}

/// Tags. Tags are not trashed: deleting one removes it from every item.
pub trait TagRepo {
    /// All tags, by name ignoring case.
    fn tags(&self) -> Result<Vec<Tag>>;

    /// Adds a tag; fails with [`Error::TagExists`](crate::Error::TagExists) on a name clash.
    fn insert_tag(&mut self, tag: &Tag) -> Result<()>;

    /// Renames a tag; same clash rule as [`TagRepo::insert_tag`].
    fn update_tag(&mut self, tag: &Tag) -> Result<()>;

    /// Deletes a tag and removes it from every item.
    fn delete_tag(&mut self, id: TagId) -> Result<()>;
}

/// Something that can be put in the trash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trashable {
    /// A board, with its lists and items.
    Board(BoardId),
    /// A list, with its items.
    List(ListId),
    /// One item.
    Item(ItemId),
}

/// One record in the trash, for the main window's Trash view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashEntry {
    /// What was trashed.
    pub target: Trashable,
    /// Its name or title when it was trashed.
    pub label: String,
    /// When it was trashed.
    pub deleted_at: Timestamp,
}

/// The 30-day trash (SPEC §5.7a). Trashing is also how the 5-second Undo works.
pub trait TrashRepo {
    /// Moves a live record, and everything inside it, to the trash.
    fn trash(&mut self, target: Trashable, at: Timestamp) -> Result<()>;

    /// Brings a record back to where it was. Restoring something whose parent is trashed
    /// restores the parent too; siblings trashed on their own stay in the trash.
    ///
    /// This can bring a board back. On Free, call
    /// [`Store::restore_with`](crate::Store::restore_with) with the board limit instead, so
    /// restoring cannot exceed it.
    fn restore(&mut self, target: Trashable) -> Result<()>;

    /// Everything in the trash, most recently trashed first. Records trashed together with
    /// their parent are not listed separately.
    fn trashed(&self) -> Result<Vec<TrashEntry>>;

    /// Permanently deletes everything trashed before `cutoff`, and blobs no longer referenced.
    /// Returns how many trash entries were deleted.
    fn purge(&mut self, cutoff: Timestamp) -> Result<usize>;

    /// Permanently deletes one trash entry (as [`trashed`](Self::trashed) lists it), with
    /// everything inside it and the blobs nothing else uses: the Trash view's "Delete
    /// forever".
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`](crate::Error::NotFound) if `target` is not a trash entry (it is
    /// live, inside a trashed parent, or already gone).
    fn purge_entry(&mut self, target: Trashable) -> Result<()>;
}
