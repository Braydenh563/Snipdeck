//! [`Batch`]: several repository operations in one SQLite transaction.
//!
//! Every write in this crate is a function over a [`Ctx`] (the connection inside an open
//! transaction, the blob folder, the data key, and a list of work to do once the transaction
//! has committed). A [`Batch`] owns the transaction; [`Store`]'s own repository methods are
//! one-operation batches, so there is exactly one implementation of each write.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use rusqlite::{Connection, Transaction};
use snipdeck_core::{
    Board, BoardId, Content, Formats, Item, ItemId, List, ListId, Placement, Tag, TagId, Timestamp,
};

use crate::blobs::{BlobMark, Blobs};
use crate::crypto::DataKey;
use crate::items::Written;
use crate::store::begin;
use crate::{
    BlobStore, BoardRepo, Error, ItemRepo, ListRepo, Result, Store, TagRepo, TrashEntry, TrashRepo,
    Trashable, boards, items, tags, trash,
};

/// Everything a write needs, inside an open transaction.
pub(crate) struct Ctx<'a> {
    pub conn: &'a Connection,
    pub blobs: &'a mut Blobs,
    pub key: Option<&'a DataKey>,
    pub post: &'a mut Post,
}

/// Everything a read needs.
pub(crate) struct Reader<'a> {
    pub conn: &'a Connection,
    pub blobs: &'a Blobs,
    pub key: Option<&'a DataKey>,
}

/// Work that follows a commit and never fails the commit (review M4): blob bookkeeping,
/// deleting files that nothing uses any more, folding the write-ahead log.
#[derive(Default)]
pub(crate) struct Post {
    /// Blobs the committed rows use: a `put` each stops being pending.
    attached: Vec<BlobRef>,
    /// Candidates for deletion, deleted if no row uses them and no `put` still waits for them.
    release: Vec<BlobRef>,
    /// The same, but zeroed first: they held plaintext of a sensitive item.
    release_scrubbed: Vec<BlobRef>,
    /// Fold the WAL, so page images of deleted plaintext do not linger (review H1).
    checkpoint: bool,
}

use snipdeck_core::BlobRef;

impl Post {
    /// After a write of content: the blobs it used are attached, the old or consumed ones are
    /// released. With `cleared_plaintext`, the write removed plaintext of a sensitive item:
    /// released blobs are scrubbed and the WAL is folded (review H1).
    pub(crate) fn written(&mut self, written: Written, old: Vec<BlobRef>, cleared_plaintext: bool) {
        let Written { used, consumed } = written;
        self.attached.extend(used);
        self.attached.extend(consumed.iter().copied());
        let list = if cleared_plaintext {
            &mut self.release_scrubbed
        } else {
            &mut self.release
        };
        list.extend(old);
        list.extend(consumed);
        self.checkpoint |= cleared_plaintext;
    }

    /// Blobs that may have become unused, scrubbed before they are deleted.
    pub(crate) fn release_scrubbed(&mut self, candidates: Vec<BlobRef>) {
        self.release_scrubbed.extend(candidates);
    }

    /// Fold the write-ahead log after the commit.
    pub(crate) fn checkpoint(&mut self) {
        self.checkpoint = true;
    }

    fn merge(&mut self, later: Post) {
        self.attached.extend(later.attached);
        self.release.extend(later.release);
        self.release_scrubbed.extend(later.release_scrubbed);
        self.checkpoint |= later.checkpoint;
    }
}

/// How the repository traits reach the database, for [`Store`] and [`Batch`] alike.
pub(crate) trait Access {
    fn reader(&self) -> Reader<'_>;

    /// Runs a write: in its own transaction for a [`Store`], inside the batch's for a
    /// [`Batch`]. All or nothing either way.
    fn writer<T>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> Result<T>) -> Result<T>;
}

impl Access for Store {
    fn reader(&self) -> Reader<'_> {
        Reader {
            conn: &self.conn,
            blobs: &self.blobs,
            key: self.key.as_ref(),
        }
    }

    fn writer<T>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> Result<T>) -> Result<T> {
        self.transaction(|batch| batch.run(f))
    }
}

impl Access for Batch<'_> {
    fn reader(&self) -> Reader<'_> {
        Reader {
            conn: &self.tx,
            blobs: &*self.guard.blobs,
            key: self.key,
        }
    }

    fn writer<T>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> Result<T>) -> Result<T> {
        self.scoped(|batch| batch.run(f))
    }
}

/// Deletes the blob files a rolled-back transaction created, when dropped before being
/// disarmed: after an error, and while a panic unwinds.
struct BlobGuard<'a> {
    blobs: &'a mut Blobs,
    mark: Option<BlobMark>,
}

impl BlobGuard<'_> {
    /// The transaction committed: keep what it wrote.
    fn disarm(mut self) {
        self.mark = None;
        self.blobs.end_log();
    }
}

impl Drop for BlobGuard<'_> {
    fn drop(&mut self) {
        if let Some(mark) = self.mark.take() {
            self.blobs.rewind(mark);
            self.blobs.end_log();
        }
    }
}

/// An open transaction on a [`Store`], from [`Store::transaction`].
///
/// A `Batch` implements the same repository traits as `Store` ([`BoardRepo`], [`ListRepo`],
/// [`ItemRepo`], [`TagRepo`], [`TrashRepo`], [`BlobStore`]) with the same rules, so code
/// written against them runs unchanged inside a transaction. Its reads see its own writes.
///
/// # Semantics
///
/// * **One commit.** Nothing a batch writes is visible to anyone else, or durable, until
///   [`Store::transaction`] commits it; if the closure returns `Err`, panics, or the commit
///   fails, the database is as it was before.
/// * **Each operation is atomic.** A failed operation (`Err` from `insert_item`, say) undoes
///   itself even if the closure handles the error and carries on, as a savepoint would.
/// * **Blob files follow the outcome.** [`BlobStore::put`] writes a new blob file at once
///   (content-addressed, so a name is never reused for other bytes); if the transaction rolls
///   back, the files it created are deleted and the set of blobs still waiting to be attached
///   is restored, and files that existed before are never touched. Blob files that writes
///   replace or release (a changed item's old content, a purge) are deleted only after the
///   commit, and not at all on rollback. A crash in the middle leaves unreferenced files that
///   [`Store::gc_blobs`] removes, never a half-applied database.
/// * **Nesting.** [`Batch::transaction`] runs a closure as a savepoint: `Err` or a panic undoes
///   only the inner work (the caller may handle the error and go on), `Ok` leaves it to the
///   outer commit. The `Batch` is a `&mut` borrow of the store, so nothing else can touch the
///   store, or open a second transaction, while it exists: there is nothing to deadlock on.
/// * **Not here.** Key ring operations ([`Store::create_key_ring`], unlock, lock, change
///   password) and [`Store::gc_blobs`] are `Store`-only: they change the unlocked key or
///   sweep files, which a transaction cannot undo.
///
/// ```
/// # use snipdeck_core::{Board, Name, Placement, Tag, Timestamp};
/// # use snipdeck_store::{BoardRepo, Error, Store, TagRepo};
/// # fn main() -> Result<(), Error> {
/// # let mut store = Store::open_in_memory()?;
/// # let at = Timestamp::from_second(1_800_000_000).unwrap();
/// let name = Name::new("Imported").unwrap();
/// let result = store.transaction(|batch| {
///     batch.insert_board(&Board::new(name.clone(), at), Placement::Last)?;
///     batch.insert_tag(&Tag::new(name.clone()))?;
///     Err::<(), Error>(Error::Invalid("changed my mind".to_owned()))
/// });
/// assert!(result.is_err());
/// assert!(store.boards()?.is_empty() && store.tags()?.is_empty());
/// # Ok(())
/// # }
/// ```
pub struct Batch<'a> {
    tx: Transaction<'a>,
    guard: BlobGuard<'a>,
    key: Option<&'a DataKey>,
    post: Post,
    /// A failed step could not be undone: the transaction must not commit.
    poisoned: bool,
}

impl core::fmt::Debug for Batch<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Batch").finish_non_exhaustive()
    }
}

impl Store {
    /// Runs `f` in one SQLite transaction: everything it does through the [`Batch`] commits
    /// together when `f` returns `Ok`, and none of it happens if `f` returns `Err` or panics.
    /// See [`Batch`] for the details (blob files, nesting, what is not available).
    ///
    /// The write lock is taken up front, so the transaction does not fail halfway for a busy
    /// database. Keep the closure short and do not wait on anything slow in it: the database
    /// stays locked until it returns. `E` is the caller's error type; store errors convert
    /// into it with `?`.
    pub fn transaction<T, E: From<Error>>(
        &mut self,
        f: impl FnOnce(&mut Batch<'_>) -> core::result::Result<T, E>,
    ) -> core::result::Result<T, E> {
        let (value, post) = {
            let tx = begin(&mut self.conn)?;
            let mark = self.blobs.mark();
            let mut batch = Batch {
                tx,
                guard: BlobGuard {
                    blobs: &mut self.blobs,
                    mark: Some(mark),
                },
                key: self.key.as_ref(),
                post: Post::default(),
                poisoned: false,
            };
            let value = f(&mut batch)?;
            (value, batch.commit()?)
        };
        self.finish(post);
        Ok(value)
    }

    /// The work that follows a commit; see [`Post`].
    pub(crate) fn finish(&mut self, post: Post) {
        self.blobs.attached(post.attached);
        self.release_blobs(&post.release_scrubbed, true);
        self.release_blobs(&post.release, false);
        if post.checkpoint {
            crate::store::checkpoint(&self.conn);
        }
    }
}

impl Batch<'_> {
    fn commit(self) -> Result<Post> {
        let Self {
            tx,
            guard,
            post,
            poisoned,
            ..
        } = self;
        if poisoned {
            return Err(Error::Corrupt(
                "a failed step of the transaction could not be undone; nothing was saved"
                    .to_owned(),
            ));
        }
        // On failure the guard (dropped here) deletes the files this transaction created.
        tx.commit()?;
        guard.disarm();
        Ok(post)
    }

    /// Runs `f` inside the transaction as it is, with no savepoint of its own.
    fn run<T>(&mut self, f: impl FnOnce(&mut Ctx<'_>) -> Result<T>) -> Result<T> {
        let mut cx = Ctx {
            conn: &self.tx,
            blobs: &mut *self.guard.blobs,
            key: self.key,
            post: &mut self.post,
        };
        f(&mut cx)
    }

    /// Runs `f` as a savepoint: its work is kept if it returns `Ok` and undone otherwise
    /// (`Err` or panic), database, blob files and after-commit work alike.
    fn scoped<T, E: From<Error>>(
        &mut self,
        f: impl FnOnce(&mut Self) -> core::result::Result<T, E>,
    ) -> core::result::Result<T, E> {
        if self.poisoned {
            return Err(Error::Corrupt("the transaction is unusable".to_owned()).into());
        }
        self.tx
            .execute_batch("SAVEPOINT batch")
            .map_err(Error::from)?;
        let mark = self.guard.blobs.mark();
        let outer = std::mem::take(&mut self.post);
        let outcome = catch_unwind(AssertUnwindSafe(|| f(self)));
        let inner = std::mem::replace(&mut self.post, outer);
        match outcome {
            Ok(Ok(value)) => match self.tx.execute_batch("RELEASE batch") {
                Ok(()) => {
                    self.post.merge(inner);
                    Ok(value)
                }
                Err(e) => {
                    self.undo(mark);
                    Err(Error::from(e).into())
                }
            },
            Ok(Err(e)) => {
                self.undo(mark);
                Err(e)
            }
            Err(panic) => {
                self.undo(mark);
                resume_unwind(panic)
            }
        }
    }

    fn undo(&mut self, mark: BlobMark) {
        if self
            .tx
            .execute_batch("ROLLBACK TO batch; RELEASE batch")
            .is_err()
        {
            self.poisoned = true;
        }
        self.guard.blobs.rewind(mark);
    }

    /// A transaction inside this one, as a savepoint: if `f` returns `Err` (or panics) only
    /// its own work is undone, and the caller may handle the error and carry on; if it
    /// returns `Ok`, its work stays pending on the outer transaction.
    pub fn transaction<T, E: From<Error>>(
        &mut self,
        f: impl FnOnce(&mut Batch<'_>) -> core::result::Result<T, E>,
    ) -> core::result::Result<T, E> {
        self.scoped(|batch| f(batch))
    }

    /// [`Store::restore_with`] inside the transaction.
    pub fn restore_with(
        &mut self,
        target: Trashable,
        max_live_boards: Option<usize>,
    ) -> Result<Vec<Trashable>> {
        self.writer(|cx| trash::restore_with(cx, target, max_live_boards))
    }

    /// [`Store::put_sensitive`] inside the transaction.
    pub fn put_sensitive(&mut self, bytes: Vec<u8>) -> Result<Content> {
        self.guard.blobs.put_transient(bytes)
    }
}

impl Store {
    /// [`TrashRepo::restore`] that refuses to bring a board back when `max_live_boards`
    /// boards are already live, and says what it brought back (review M1).
    ///
    /// Restoring a list or item inside a trashed board revives the board too, so the Free
    /// board limit cannot be bypassed by trashing a board, creating another and restoring the
    /// first. Storage stays free of entitlements: the app passes the limit
    /// (`Some(Entitlements::FREE_BOARD_LIMIT)` on Free, `None` on Pro). Over the limit it fails
    /// with `Error::Core(ProRequired(Feature::UnlimitedBoards))`, the upsell path, and changes
    /// nothing.
    ///
    /// Returns the records whose trash flag was cleared: the target (unless it was live),
    /// then its list and board if they were trashed. Empty if everything was live already.
    pub fn restore_with(
        &mut self,
        target: Trashable,
        max_live_boards: Option<usize>,
    ) -> Result<Vec<Trashable>> {
        self.writer(|cx| trash::restore_with(cx, target, max_live_boards))
    }
}

/// Implements every repository trait for a type that is [`Access`]. One body per method, so
/// `Store` and `Batch` cannot drift apart.
macro_rules! repo_impls {
    ($ty:ty) => {
        impl BoardRepo for $ty {
            fn boards(&self) -> Result<Vec<Board>> {
                boards::boards(self.reader().conn)
            }

            fn board(&self, id: BoardId) -> Result<Board> {
                boards::board(self.reader().conn, id)
            }

            fn insert_board(&mut self, board: &Board, at: Placement<BoardId>) -> Result<()> {
                self.writer(|cx| boards::insert_board(cx, board, at))
            }

            fn update_board(&mut self, board: &Board) -> Result<()> {
                self.writer(|cx| boards::update_board(cx, board))
            }

            fn move_board(&mut self, id: BoardId, at: Placement<BoardId>) -> Result<()> {
                self.writer(|cx| boards::move_board(cx, id, at))
            }
        }

        impl ListRepo for $ty {
            fn lists(&self, board: BoardId) -> Result<Vec<List>> {
                boards::lists(self.reader().conn, board)
            }

            fn list(&self, id: ListId) -> Result<List> {
                boards::list(self.reader().conn, id)
            }

            fn insert_list(&mut self, list: &List, at: Placement<ListId>) -> Result<()> {
                self.writer(|cx| boards::insert_list(cx, list, at))
            }

            fn update_list(&mut self, list: &List) -> Result<()> {
                self.writer(|cx| boards::update_list(cx, list))
            }

            fn move_list(
                &mut self,
                id: ListId,
                board: BoardId,
                at: Placement<ListId>,
            ) -> Result<()> {
                self.writer(|cx| boards::move_list(cx, id, board, at))
            }
        }

        impl ItemRepo for $ty {
            fn items(&self, list: ListId) -> Result<Vec<Item>> {
                items::items(self.reader().conn, list)
            }

            fn all_items(&self) -> Result<Vec<Item>> {
                items::all_items(self.reader().conn)
            }

            fn item(&self, id: ItemId) -> Result<Item> {
                items::item(self.reader().conn, id)
            }

            fn formats(&self, id: ItemId) -> Result<Formats> {
                let r = self.reader();
                items::formats(r.conn, r.blobs, r.key, id)
            }

            fn insert_item(
                &mut self,
                item: &Item,
                formats: &Formats,
                at: Placement<ItemId>,
            ) -> Result<()> {
                self.writer(|cx| items::insert_item(cx, item, formats, at))
            }

            fn update_item(&mut self, item: &Item, formats: Option<&Formats>) -> Result<()> {
                self.writer(|cx| items::update_item(cx, item, formats))
            }

            fn move_item(&mut self, id: ItemId, list: ListId, at: Placement<ItemId>) -> Result<()> {
                self.writer(|cx| items::move_item(cx, id, list, at))
            }

            fn record_use(&mut self, id: ItemId, at: Timestamp) -> Result<()> {
                self.writer(|cx| items::record_use(cx, id, at))
            }

            fn search(&self, query: &str, limit: usize) -> Result<Vec<ItemId>> {
                items::search(self.reader().conn, query, limit)
            }
        }

        impl TagRepo for $ty {
            fn tags(&self) -> Result<Vec<Tag>> {
                tags::tags(self.reader().conn)
            }

            fn insert_tag(&mut self, tag: &Tag) -> Result<()> {
                self.writer(|cx| tags::insert_tag(cx, tag))
            }

            fn update_tag(&mut self, tag: &Tag) -> Result<()> {
                self.writer(|cx| tags::update_tag(cx, tag))
            }

            fn delete_tag(&mut self, id: TagId) -> Result<()> {
                self.writer(|cx| tags::delete_tag(cx, id))
            }
        }

        impl TrashRepo for $ty {
            fn trash(&mut self, target: Trashable, at: Timestamp) -> Result<()> {
                self.writer(|cx| trash::trash(cx, target, at))
            }

            fn restore(&mut self, target: Trashable) -> Result<()> {
                self.writer(|cx| trash::restore_with(cx, target, None))
                    .map(drop)
            }

            fn trashed(&self) -> Result<Vec<TrashEntry>> {
                trash::trashed(self.reader().conn)
            }

            fn purge(&mut self, cutoff: Timestamp) -> Result<usize> {
                self.writer(|cx| trash::purge(cx, cutoff))
            }
        }
    };
}

repo_impls!(Store);
repo_impls!(Batch<'_>);

impl BlobStore for Store {
    fn put(&mut self, bytes: Vec<u8>) -> Result<Content> {
        self.blobs.put(bytes)
    }

    fn read<'a>(&self, content: &'a Content) -> Result<std::borrow::Cow<'a, [u8]>> {
        // Decrypted and staged sensitive payloads are served from memory.
        self.blobs.read_any(content)
    }
}

impl BlobStore for Batch<'_> {
    fn put(&mut self, bytes: Vec<u8>) -> Result<Content> {
        self.guard.blobs.put(bytes)
    }

    fn read<'a>(&self, content: &'a Content) -> Result<std::borrow::Cow<'a, [u8]>> {
        self.guard.blobs.read_any(content)
    }
}
