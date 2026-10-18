//! `Store::transaction`: several repository operations in one SQLite transaction that commits
//! or rolls back as a whole, with blob files following the outcome.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::*;
use snipdeck_core::{
    Board, Content, Format, Formats, INLINE_LIMIT, Item, ItemId, ItemKind, List, ListId, Placement,
    Tag,
};
use snipdeck_store::crypto::{KdfParams, KeyRing, MemoryKeyProvider};
use snipdeck_store::{
    Batch, BlobStore, BoardRepo, Error, ItemRepo, ListRepo, Store, TagRepo, TrashRepo, Trashable,
};

/// The caller's own error type: `transaction` takes any error a store error converts into.
#[derive(Debug, PartialEq)]
enum Oops {
    Store(String),
    Boom,
}

impl From<Error> for Oops {
    fn from(e: Error) -> Self {
        Self::Store(e.to_string())
    }
}

fn blob(byte: u8) -> Vec<u8> {
    vec![byte; INLINE_LIMIT + 1]
}

fn blob_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for shard in std::fs::read_dir(dir.join("blobs"))
        .into_iter()
        .flatten()
        .flatten()
    {
        found.extend(
            std::fs::read_dir(shard.path())
                .unwrap()
                .flatten()
                .map(|e| e.path()),
        );
    }
    found.sort();
    found
}

/// Everything a reader can see, for before/after comparisons.
#[derive(Debug, PartialEq)]
struct Snapshot {
    boards: Vec<String>,
    lists: usize,
    items: Vec<String>,
    tags: Vec<String>,
    trash: Vec<String>,
}

fn snapshot(s: &Store) -> Snapshot {
    let boards = s.boards().unwrap();
    let lists = boards.iter().map(|b| s.lists(b.id).unwrap().len()).sum();
    Snapshot {
        boards: boards.iter().map(|b| b.name.to_string()).collect(),
        lists,
        items: s
            .all_items()
            .unwrap()
            .iter()
            .map(|i| i.title.clone())
            .collect(),
        tags: s
            .tags()
            .unwrap()
            .iter()
            .map(|t| t.name.to_string())
            .collect(),
        trash: s
            .trashed()
            .unwrap()
            .iter()
            .map(|t| format!("{} @ {}", t.label, t.deleted_at))
            .collect(),
    }
}

fn healthy(s: &Store) {
    assert_eq!(s.check_integrity().unwrap(), Vec::<String>::new());
}

/// A board with a list and an item holding a stored blob, plus a tag, inside `b`.
fn fill(b: &mut Batch<'_>, bytes: Vec<u8>) -> Result<(Board, List, Item), Error> {
    let board = Board::new(name("Imported"), at(1));
    b.insert_board(&board, Placement::Last)?;
    let list = List::new(board.id, name("Inbox"), at(1));
    b.insert_list(&list, Placement::Last)?;
    let tag = Tag::new(name("fresh"));
    b.insert_tag(&tag)?;
    let mut item = Item::new(list.id, "big".to_owned(), ItemKind::Text, at(1));
    item.tags.insert(tag.id);
    let content = b.put(bytes)?;
    b.insert_item(
        &item,
        &Formats::new(vec![Format::Text(content)]).unwrap(),
        Placement::Last,
    )?;
    Ok((board, list, item))
}

fn seeded() -> (Store, ListId, ItemId) {
    let mut s = store();
    let b = board(&mut s, "Mine");
    let l = list(&mut s, b.id, "Mine");
    let it = item(&mut s, l.id, "keep", "keep");
    s.insert_tag(&Tag::new(name("old"))).unwrap();
    (s, l.id, it.id)
}

#[test]
fn a_committed_transaction_makes_everything_visible_at_once() {
    let (mut s, _, _) = seeded();
    let (board, list, item) = s
        .transaction(|b| {
            let made = fill(b, blob(7))?;
            // Reads inside see the transaction's own writes, search included.
            assert_eq!(b.boards().unwrap().len(), 2);
            assert_eq!(b.lists(made.0.id).unwrap().len(), 1);
            assert_eq!(b.item(made.2.id).unwrap().title, "big");
            assert_eq!(b.tags().unwrap().len(), 2);
            Ok::<_, Error>(made)
        })
        .unwrap();
    assert_eq!(board_names(&s), ["Mine", "Imported"]);
    assert_eq!(s.lists(board.id).unwrap()[0].id, list.id);
    assert_eq!(s.item(item.id).unwrap().tags.len(), 1);
    assert_eq!(s.formats(item.id).unwrap().iter().count(), 1);
    assert_eq!(s.search("big", 5).unwrap(), [item.id]);
    healthy(&s);
}

#[test]
fn an_error_rolls_back_the_database_and_the_blobs() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "Mine");
    let l = list(&mut s, b.id, "Mine");
    // A committed item with a stored blob: it must survive untouched.
    let content = s.put(blob(1)).unwrap();
    let keep = Item::new(l.id, "keep".to_owned(), ItemKind::Text, at(0));
    s.insert_item(
        &keep,
        &Formats::new(vec![Format::Text(content)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    let before = snapshot(&s);
    let files = blob_files(dir.path());
    assert_eq!(files.len(), 1);

    let result = s.transaction(|b| {
        fill(b, blob(2))?;
        // Re-using the committed blob must not make the rollback delete it.
        let again = b.put(blob(1))?;
        let dup = Item::new(l.id, "dup".to_owned(), ItemKind::Text, at(1));
        b.insert_item(
            &dup,
            &Formats::new(vec![Format::Text(again)]).unwrap(),
            Placement::Last,
        )?;
        Err::<(), _>(Oops::Boom)
    });
    assert_eq!(result, Err(Oops::Boom));

    assert_eq!(snapshot(&s), before);
    assert_eq!(blob_files(dir.path()), files, "the new blob file is gone");
    assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 0);
    healthy(&s);
    // The store is fully usable afterwards.
    board(&mut s, "After");
}

#[test]
fn a_failing_operation_rolls_everything_back_through_the_question_mark() {
    let (mut s, _, _) = seeded();
    let before = snapshot(&s);
    let result = s.transaction(|b| {
        fill(b, blob(3))?;
        // `old` exists already: TagExists, after a board, list, tag and item were written.
        b.insert_tag(&Tag::new(name("OLD")))?;
        Ok::<_, Error>(())
    });
    assert!(matches!(result, Err(Error::TagExists { .. })), "{result:?}");
    assert_eq!(snapshot(&s), before);
    assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 0);
    healthy(&s);
}

#[test]
fn an_in_memory_store_rolls_its_blobs_back_too() {
    let (mut s, _, _) = seeded();
    s.transaction(|b| {
        fill(b, blob(4))?;
        Err::<(), _>(Oops::Boom)
    })
    .unwrap_err();
    assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 0);
    // The same bytes can be stored again afterwards.
    s.transaction(|b| fill(b, blob(4)).map(drop)).unwrap();
    healthy(&s);
}

#[test]
fn a_panic_rolls_back_and_leaves_the_store_usable() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    board(&mut s, "Mine");
    let before = snapshot(&s);
    let crashed = catch_unwind(AssertUnwindSafe(|| {
        let _ = s.transaction(|b| {
            fill(b, blob(5))?;
            assert!(!blob_files(dir.path()).is_empty());
            panic!("boom");
            #[allow(unreachable_code, reason = "gives the closure its type")]
            Ok::<_, Error>(())
        });
    }));
    assert!(crashed.is_err());
    assert_eq!(snapshot(&s), before);
    assert!(blob_files(dir.path()).is_empty());
    healthy(&s);
    // No transaction was left open: a new one starts and commits.
    s.transaction(|b| fill(b, blob(5)).map(drop)).unwrap();
    assert_eq!(board_names(&s), ["Mine", "Imported"]);
}

#[test]
fn a_failed_operation_that_is_caught_undoes_only_itself() {
    let (mut s, list, _) = seeded();
    s.transaction(|b| {
        let first = Tag::new(name("first"));
        b.insert_tag(&first)?;
        // A bad operation (an item with a tag that does not exist) is an error the caller may
        // handle; the transaction goes on.
        let mut bad = Item::new(list, "bad".to_owned(), ItemKind::Text, at(1));
        bad.tags.insert(Tag::new(name("never stored")).id);
        assert!(
            b.insert_item(&bad, &text_formats("x"), Placement::Last)
                .is_err()
        );
        assert!(b.insert_tag(&Tag::new(name("FIRST"))).is_err());
        b.insert_tag(&Tag::new(name("second")))?;
        Ok::<_, Error>(())
    })
    .unwrap();
    let tags: Vec<_> = s
        .tags()
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(tags, ["first", "old", "second"]);
    assert_eq!(s.all_items().unwrap().len(), 1, "`bad` is not there");
    healthy(&s);
}

#[test]
fn nested_transactions_are_savepoints() {
    let (mut s, _, _) = seeded();
    let before = snapshot(&s);

    // The inner one fails and is caught: only its own work is undone.
    s.transaction(|b| {
        b.insert_tag(&Tag::new(name("outer")))?;
        let inner = b.transaction(|b| {
            b.insert_tag(&Tag::new(name("inner")))?;
            Err::<(), _>(Oops::Boom)
        });
        assert_eq!(inner, Err(Oops::Boom));
        // And one that succeeds stays with the outer.
        b.transaction(|b| b.insert_tag(&Tag::new(name("kept"))))?;
        Ok::<_, Error>(())
    })
    .unwrap();
    let tags: Vec<_> = s
        .tags()
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect();
    assert_eq!(tags, ["kept", "old", "outer"]);

    // An outer failure takes a committed inner one with it.
    let before_outer = snapshot(&s);
    s.transaction(|b| {
        b.transaction(|b| fill(b, blob(8)).map(drop))?;
        Err::<(), _>(Error::Invalid("outer".to_owned()))
    })
    .unwrap_err();
    assert_eq!(snapshot(&s), before_outer);
    assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 0);
    assert_ne!(before, before_outer);
    healthy(&s);
}

#[test]
fn a_panic_in_a_nested_transaction_rolls_everything_back() {
    let (mut s, _, _) = seeded();
    let before = snapshot(&s);
    let crashed = catch_unwind(AssertUnwindSafe(|| {
        let _ = s.transaction(|b| {
            b.insert_tag(&Tag::new(name("outer")))?;
            b.transaction(|b| {
                b.insert_tag(&Tag::new(name("inner")))?;
                panic!("boom");
                #[allow(unreachable_code, reason = "gives the closure its type")]
                Ok::<_, Error>(())
            })
        });
    }));
    assert!(crashed.is_err());
    assert_eq!(snapshot(&s), before);
    healthy(&s);
}

#[test]
fn replaced_and_purged_blobs_are_deleted_only_after_the_commit() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let make = |s: &mut Store, byte: u8, title: &str| {
        let content = s.put(blob(byte)).unwrap();
        let it = Item::new(l.id, title.to_owned(), ItemKind::Text, at(0));
        s.insert_item(
            &it,
            &Formats::new(vec![Format::Text(content)]).unwrap(),
            Placement::Last,
        )
        .unwrap();
        it
    };
    let replaced = make(&mut s, 1, "replaced");
    let purged = make(&mut s, 2, "purged");
    s.trash(Trashable::Item(purged.id), at(1)).unwrap();
    assert_eq!(blob_files(dir.path()).len(), 2);

    // Rolled back: nothing is deleted, in or after the transaction.
    s.transaction(|b| {
        let new = b.put(blob(3))?;
        b.update_item(
            &replaced,
            Some(&Formats::new(vec![Format::Text(new)]).unwrap()),
        )?;
        assert_eq!(b.purge(at(10))?, 1);
        assert_eq!(
            blob_files(dir.path()).len(),
            3,
            "old files stay until commit"
        );
        Err::<(), _>(Oops::Boom)
    })
    .unwrap_err();
    assert_eq!(blob_files(dir.path()).len(), 2);
    assert_eq!(s.trashed().unwrap().len(), 1);
    assert!(s.formats(replaced.id).is_ok());
    healthy(&s);

    // Committed: the replaced and purged blobs go, the new one stays.
    s.transaction(|b| {
        let new = b.put(blob(3))?;
        b.update_item(
            &replaced,
            Some(&Formats::new(vec![Format::Text(new)]).unwrap()),
        )?;
        b.purge(at(10))?;
        assert_eq!(
            blob_files(dir.path()).len(),
            3,
            "old files stay until commit"
        );
        Ok::<_, Error>(())
    })
    .unwrap();
    assert_eq!(blob_files(dir.path()).len(), 1);
    assert_eq!(s.trashed().unwrap().len(), 0);
    healthy(&s);
}

#[test]
fn a_rolled_back_transaction_restores_which_blobs_are_waiting_to_be_attached() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let holder = Item::new(l.id, "holder".to_owned(), ItemKind::Text, at(0));
    let first = s.put(blob(1)).unwrap();
    s.insert_item(
        &holder,
        &Formats::new(vec![Format::Text(first)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    let replace = |s: &mut Store, byte: u8| {
        let content = s.put(blob(byte)).unwrap();
        s.update_item(
            &holder,
            Some(&Formats::new(vec![Format::Text(content)]).unwrap()),
        )
        .unwrap();
    };

    // A `put` made inside a transaction that rolls back is not left waiting: the old blob it
    // duplicated is released normally by the next write that replaces it.
    s.transaction(|b| {
        b.put(blob(1))?;
        Err::<(), _>(Oops::Boom)
    })
    .unwrap_err();
    replace(&mut s, 2);
    assert_eq!(blob_files(dir.path()).len(), 1, "blob 1 was deleted");

    // A `put` made before the transaction is still waiting afterwards, even though the
    // transaction used it and rolled back.
    let waiting = s.put(blob(2)).unwrap();
    s.transaction(|b| {
        let other = Item::new(l.id, "other".to_owned(), ItemKind::Text, at(1));
        b.insert_item(
            &other,
            &Formats::new(vec![Format::Text(waiting.clone())]).unwrap(),
            Placement::Last,
        )?;
        Err::<(), _>(Oops::Boom)
    })
    .unwrap_err();
    replace(&mut s, 3);
    assert_eq!(
        blob_files(dir.path()).len(),
        2,
        "blob 2 is kept for its item"
    );
    let other = Item::new(l.id, "other".to_owned(), ItemKind::Text, at(1));
    s.insert_item(
        &other,
        &Formats::new(vec![Format::Text(waiting)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    healthy(&s);
}

#[test]
fn restore_and_trash_work_inside_a_transaction() {
    let (mut s, list, item) = seeded();
    s.trash(Trashable::Item(item), at(1)).unwrap();
    s.transaction(|b| {
        assert_eq!(b.trashed().unwrap().len(), 1);
        let revived = b.restore_with(Trashable::Item(item), Some(3))?;
        assert_eq!(revived, [Trashable::Item(item)]);
        b.trash(Trashable::List(list), at(2))?;
        Ok::<_, Error>(())
    })
    .unwrap();
    assert_eq!(s.trashed().unwrap().len(), 1);
    healthy(&s);
}

#[test]
fn sensitive_items_are_encrypted_inside_a_transaction() {
    let provider = MemoryKeyProvider::new().unwrap();
    let ring =
        KeyRing::new(&provider).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap());
    let (mut s, list, _) = seeded();
    s.create_key_ring(&ring, None).unwrap();
    let id = s
        .transaction(|b| {
            let content = b.put_sensitive(blob(9))?;
            let mut it = Item::new(list, "secret".to_owned(), ItemKind::Text, at(1));
            it.sensitive = true;
            b.insert_item(
                &it,
                &Formats::new(vec![Format::Text(content)]).unwrap(),
                Placement::Last,
            )?;
            // Read back decrypted, inside the transaction.
            let formats = b.formats(it.id)?;
            let Format::Text(content) = formats.primary() else {
                panic!("text")
            };
            assert_eq!(b.read(content)?.as_ref(), blob(9).as_slice());
            Ok::<_, Error>(it.id)
        })
        .unwrap();
    let formats = s.formats(id).unwrap();
    let Format::Text(content @ Content::Blob { .. }) = formats.primary() else {
        panic!("blob")
    };
    assert_eq!(s.read(content).unwrap().as_ref(), blob(9).as_slice());
    healthy(&s);
}

#[test]
fn a_locked_store_refuses_sensitive_writes_in_a_transaction_too() {
    let provider = MemoryKeyProvider::new().unwrap();
    let ring =
        KeyRing::new(&provider).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap());
    let (mut s, list, _) = seeded();
    s.create_key_ring(&ring, None).unwrap();
    s.lock_key_ring();
    let before = snapshot(&s);
    let result = s.transaction(|b| {
        b.insert_tag(&Tag::new(name("x")))?;
        let mut it = Item::new(list, "secret".to_owned(), ItemKind::Text, at(1));
        it.sensitive = true;
        b.insert_item(&it, &text_formats("pw"), Placement::Last)
    });
    assert!(matches!(result, Err(Error::EncryptionLocked)), "{result:?}");
    assert_eq!(snapshot(&s), before);
}

#[test]
fn single_operations_still_commit_on_their_own() {
    // The repository methods on `Store` are one-operation transactions.
    let mut s = store();
    let b = board(&mut s, "one");
    assert!(
        s.insert_board(&b, Placement::Last).is_err(),
        "duplicate id leaves nothing behind"
    );
    assert_eq!(board_names(&s), ["one"]);
    healthy(&s);
}
