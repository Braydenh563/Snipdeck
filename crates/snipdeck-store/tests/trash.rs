//! Soft delete, restore and purge. Cascade rules (also in docs/store.md):
//!
//! - Trashing marks only the record you name; what is inside it is hidden because its parent is.
//! - Restoring a record restores its trashed ancestors too; things trashed on their own earlier
//!   stay in the trash.
//! - Purging a list or board permanently deletes everything inside it.
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

use common::*;
use snipdeck_core::{Format, Formats, INLINE_LIMIT, Item, ItemId, ItemKind, Placement};
use snipdeck_store::{
    BlobStore, BoardRepo, Error, ItemRepo, ListRepo, Store, TrashEntry, TrashRepo, Trashable,
};

fn tree() -> (Store, snipdeck_core::Board, snipdeck_core::List, Vec<Item>) {
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let items = ["one", "two", "three"]
        .map(|t| item(&mut s, l.id, t, t))
        .to_vec();
    (s, b, l, items)
}

#[test]
fn a_trashed_item_disappears_everywhere_and_comes_back_in_place() {
    let (mut s, _b, l, it) = tree();
    s.trash(Trashable::Item(it[1].id), at(100)).unwrap();
    assert_eq!(titles(&s, l.id), ["one", "three"]);
    assert_eq!(s.all_items().unwrap().len(), 2);
    assert!(matches!(
        s.item(it[1].id),
        Err(Error::NotFound { entity: "item", .. })
    ));
    assert!(matches!(s.formats(it[1].id), Err(Error::NotFound { .. })));
    assert!(s.search("two", 10).unwrap().is_empty());

    s.restore(Trashable::Item(it[1].id)).unwrap();
    assert_eq!(titles(&s, l.id), ["one", "two", "three"]);
    assert_eq!(s.search("two", 10).unwrap(), [it[1].id]);
    assert_healthy(&s);
}

#[test]
fn trashed_things_cannot_be_changed_or_used_as_targets() {
    let (mut s, b, l, it) = tree();
    s.trash(Trashable::Item(it[0].id), at(1)).unwrap();
    assert!(matches!(
        s.update_item(&it[0], None),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        s.record_use(it[0].id, at(2)),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        s.move_item(it[1].id, l.id, Placement::After(it[0].id)),
        Err(Error::NotFound { entity: "item", .. })
    ));
    assert!(matches!(
        s.trash(Trashable::Item(it[0].id), at(3)),
        Err(Error::NotFound { .. })
    ));

    let other = list(&mut s, b.id, "other");
    s.trash(Trashable::List(other.id), at(4)).unwrap();
    let mut fresh = Item::new(other.id, "late".to_owned(), ItemKind::Text, at(5));
    fresh.preview = String::new();
    assert!(matches!(
        s.insert_item(&fresh, &text_formats("x"), Placement::Last),
        Err(Error::NotFound { entity: "list", .. })
    ));
    assert!(matches!(
        s.move_item(it[1].id, other.id, Placement::Last),
        Err(Error::NotFound { entity: "list", .. })
    ));
    assert!(matches!(s.list(other.id), Err(Error::NotFound { .. })));
}

#[test]
fn trashing_a_list_hides_its_items_and_is_one_trash_entry() {
    let (mut s, b, l, it) = tree();
    s.trash(Trashable::Item(it[0].id), at(10)).unwrap(); // trashed earlier, on its own
    s.trash(Trashable::List(l.id), at(20)).unwrap();
    assert!(s.lists(b.id).unwrap().is_empty());
    assert!(s.all_items().unwrap().is_empty());
    assert!(matches!(s.item(it[1].id), Err(Error::NotFound { .. })));
    assert_eq!(
        s.trashed().unwrap(),
        [TrashEntry {
            target: Trashable::List(l.id),
            label: "Replies".to_owned(),
            deleted_at: at(20)
        }]
    );

    s.restore(Trashable::List(l.id)).unwrap();
    assert_eq!(
        titles(&s, l.id),
        ["two", "three"],
        "the item trashed on its own stays in the trash"
    );
    assert_eq!(
        s.trashed().unwrap(),
        [TrashEntry {
            target: Trashable::Item(it[0].id),
            label: "one".to_owned(),
            deleted_at: at(10)
        }]
    );
}

#[test]
fn trashing_a_board_hides_lists_and_items() {
    let (mut s, b, l, _) = tree();
    s.trash(Trashable::Board(b.id), at(5)).unwrap();
    assert!(s.boards().unwrap().is_empty());
    assert!(matches!(
        s.lists(b.id),
        Err(Error::NotFound {
            entity: "board",
            ..
        })
    ));
    assert!(matches!(s.list(l.id), Err(Error::NotFound { .. })));
    assert!(s.all_items().unwrap().is_empty());
    assert_eq!(s.trashed().unwrap().len(), 1);
    s.restore(Trashable::Board(b.id)).unwrap();
    assert_eq!(board_names(&s), ["Work"]);
    assert_eq!(titles(&s, l.id).len(), 3);
}

#[test]
fn restoring_an_item_whose_list_is_trashed_restores_the_list_too() {
    let (mut s, b, l, it) = tree();
    s.trash(Trashable::Item(it[0].id), at(10)).unwrap();
    s.trash(Trashable::Item(it[1].id), at(11)).unwrap();
    s.trash(Trashable::List(l.id), at(20)).unwrap();

    s.restore(Trashable::Item(it[0].id)).unwrap();
    assert_eq!(list_names(&s, b.id), ["Replies"]);
    assert_eq!(
        titles(&s, l.id),
        ["one", "three"],
        "the sibling trashed on its own stays trashed"
    );
    let left: Vec<_> = s.trashed().unwrap().into_iter().map(|e| e.target).collect();
    assert_eq!(left, [Trashable::Item(it[1].id)]);
}

#[test]
fn restoring_through_a_trashed_board_restores_the_whole_chain() {
    let (mut s, b, l, it) = tree();
    s.trash(Trashable::Board(b.id), at(20)).unwrap();
    s.restore(Trashable::Item(it[2].id)).unwrap();
    assert_eq!(board_names(&s), ["Work"]);
    assert_eq!(list_names(&s, b.id), ["Replies"]);
    assert_eq!(titles(&s, l.id).len(), 3);
    assert!(s.trashed().unwrap().is_empty());
}

#[test]
fn restoring_a_list_whose_board_is_trashed_restores_the_board() {
    let (mut s, b, l, _) = tree();
    s.trash(Trashable::List(l.id), at(10)).unwrap();
    s.trash(Trashable::Board(b.id), at(20)).unwrap();
    s.restore(Trashable::List(l.id)).unwrap();
    assert_eq!(board_names(&s), ["Work"]);
    assert_eq!(list_names(&s, b.id), ["Replies"]);
}

#[test]
fn a_restored_board_keeps_its_lists_that_were_trashed_on_their_own_in_the_trash() {
    let (mut s, b, l, _) = tree();
    let keep = list(&mut s, b.id, "Keep");
    s.trash(Trashable::List(l.id), at(10)).unwrap();
    s.trash(Trashable::Board(b.id), at(20)).unwrap();
    s.restore(Trashable::Board(b.id)).unwrap();
    assert_eq!(list_names(&s, b.id), ["Keep"]);
    assert!(
        s.trashed()
            .unwrap()
            .iter()
            .any(|e| e.target == Trashable::List(l.id))
    );
    let _ = keep;
}

#[test]
fn restore_of_a_live_record_is_a_no_op_and_of_a_missing_one_is_not_found() {
    let (mut s, _b, l, it) = tree();
    s.restore(Trashable::Item(it[0].id)).unwrap();
    assert_eq!(titles(&s, l.id).len(), 3);
    assert!(matches!(
        s.restore(Trashable::Item(ItemId::generate())),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        s.trash(Trashable::Board(snipdeck_core::BoardId::generate()), at(1)),
        Err(Error::NotFound {
            entity: "board",
            ..
        })
    ));
}

#[test]
fn a_trashed_sibling_is_not_a_valid_anchor_but_keeps_its_slot() {
    let (mut s, _b, l, it) = tree();
    s.trash(Trashable::Item(it[1].id), at(1)).unwrap();
    assert!(matches!(
        s.move_item(it[2].id, l.id, Placement::After(it[1].id)),
        Err(Error::NotFound { .. })
    ));
    // An item put right after "one" lands before the trashed "two", and "two" still comes back
    // between its old neighbours.
    item_at(&mut s, l.id, "new", "n", Placement::After(it[0].id));
    s.restore(Trashable::Item(it[1].id)).unwrap();
    assert_eq!(titles(&s, l.id), ["one", "new", "two", "three"]);
}

#[test]
fn trashed_lists_most_recent_first_with_labels() {
    let (mut s, b, l, it) = tree();
    let untitled = {
        let mut i = Item::new(l.id, String::new(), ItemKind::Text, at(0));
        i.preview = "from the preview".to_owned();
        s.insert_item(&i, &text_formats("from the preview"), Placement::Last)
            .unwrap();
        i
    };
    s.trash(Trashable::Item(it[0].id), at(10)).unwrap();
    s.trash(Trashable::Item(untitled.id), at(30)).unwrap();
    let other = board(&mut s, "Other");
    s.trash(Trashable::Board(other.id), at(20)).unwrap();
    let entries = s.trashed().unwrap();
    let summary: Vec<_> = entries
        .iter()
        .map(|e| (e.label.as_str(), e.deleted_at))
        .collect();
    assert_eq!(
        summary,
        [
            ("from the preview", at(30)),
            ("Other", at(20)),
            ("one", at(10))
        ]
    );
    let _ = b;
}

#[test]
fn purge_deletes_only_what_was_trashed_before_the_cutoff() {
    let (mut s, b, l, it) = tree();
    s.trash(Trashable::Item(it[0].id), at(10)).unwrap();
    s.trash(Trashable::Item(it[1].id), at(20)).unwrap();
    s.trash(Trashable::Item(it[2].id), at(30)).unwrap();
    assert_eq!(s.purge(at(20)).unwrap(), 1, "strictly before the cutoff");
    let left: Vec<_> = s.trashed().unwrap().into_iter().map(|e| e.label).collect();
    assert_eq!(left, ["three", "two"]);
    assert_eq!(s.purge(at(1000)).unwrap(), 2);
    assert!(s.trashed().unwrap().is_empty());
    assert_eq!(s.purge(at(1000)).unwrap(), 0);
    assert!(titles(&s, l.id).is_empty());
    assert_eq!(list_names(&s, b.id), ["Replies"]);
    assert_healthy(&s);
}

#[test]
fn purging_a_list_or_board_deletes_everything_inside_and_its_index_entries() {
    let (mut s, b, l, _) = tree();
    let extra = list(&mut s, b.id, "Other");
    item(&mut s, extra.id, "survivor", "survivor");
    s.trash(Trashable::List(l.id), at(10)).unwrap();
    assert_eq!(s.purge(at(11)).unwrap(), 1);
    assert_eq!(s.all_items().unwrap().len(), 1);
    assert!(s.search("one", 10).unwrap().is_empty());
    assert_eq!(s.search("survivor", 10).unwrap().len(), 1);

    s.trash(Trashable::Board(b.id), at(20)).unwrap();
    assert_eq!(s.purge(at(21)).unwrap(), 1);
    assert!(s.boards().unwrap().is_empty());
    assert!(s.trashed().unwrap().is_empty());
    assert_healthy(&s);
}

#[test]
fn purge_counts_entries_not_hidden_children() {
    let (mut s, _b, l, it) = tree();
    s.trash(Trashable::Item(it[0].id), at(1)).unwrap();
    s.trash(Trashable::List(l.id), at(2)).unwrap();
    assert_eq!(
        s.purge(at(100)).unwrap(),
        1,
        "the list is the one entry the trash view showed"
    );
    assert_healthy(&s);
}

#[test]
fn purge_deletes_blob_files_that_nothing_else_uses() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    let shared = s.put(vec![1; INLINE_LIMIT + 1]).unwrap();
    let own = s.put(vec![2; INLINE_LIMIT + 1]).unwrap();
    let keep = Item::new(l.id, "keep".to_owned(), ItemKind::Image, at(0));
    let gone = Item::new(l.id, "gone".to_owned(), ItemKind::Image, at(0));
    s.insert_item(
        &keep,
        &Formats::new(vec![Format::Png(shared.clone())]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    s.insert_item(
        &gone,
        &Formats::new(vec![Format::Png(shared.clone()), Format::Rtf(own.clone())]).unwrap(),
        Placement::Last,
    )
    .unwrap();

    s.trash(Trashable::Item(gone.id), at(5)).unwrap();
    assert!(
        s.read(&own).is_ok(),
        "trashed items keep their blobs for the 30 days"
    );
    s.purge(at(6)).unwrap();
    assert!(matches!(
        s.read(&own),
        Err(Error::NotFound { entity: "blob", .. })
    ));
    assert!(s.read(&shared).is_ok());
    assert_healthy(&s);
}

#[test]
fn purging_leaves_tags_alone() {
    use snipdeck_core::Tag;
    use snipdeck_store::TagRepo;
    let (mut s, _b, _l, mut it) = tree();
    let tag = Tag::new(name("keep me"));
    s.insert_tag(&tag).unwrap();
    it[0].tags.insert(tag.id);
    s.update_item(&it[0], None).unwrap();
    s.trash(Trashable::Item(it[0].id), at(1)).unwrap();
    s.purge(at(2)).unwrap();
    assert_eq!(s.tags().unwrap(), [tag]);
    assert_healthy(&s);
}

/// Review M1: a Free user with 3 boards trashes one, creates another (2 live, so allowed) and
/// restores the first: that must not give 4 live boards.
#[test]
fn restore_cannot_revive_a_board_past_the_callers_limit() {
    use snipdeck_core::{Entitlements, Feature, Tier};
    let free = Entitlements::new(Tier::Free);
    let limit = Some(Entitlements::FREE_BOARD_LIMIT);
    let mut s = store();
    let a = board(&mut s, "A");
    let a_list = list(&mut s, a.id, "inbox");
    let a_item = item(&mut s, a_list.id, "note", "note");
    board(&mut s, "B");
    board(&mut s, "C");
    s.trash(Trashable::Board(a.id), at(1)).unwrap();
    assert!(free.can_create_board(s.boards().unwrap().len()));
    board(&mut s, "D");

    for target in [
        Trashable::Board(a.id),
        Trashable::List(a_list.id),
        Trashable::Item(a_item.id),
    ] {
        let err = s.restore_with(target, limit).unwrap_err();
        assert!(
            matches!(
                err,
                Error::Core(snipdeck_core::Error::ProRequired(Feature::UnlimitedBoards))
            ),
            "{target:?}: {err:?}"
        );
        assert_eq!(
            board_names(&s),
            ["B", "C", "D"],
            "{target:?}: nothing revived"
        );
    }
    assert_eq!(s.trashed().unwrap().len(), 1);

    // Under the limit it works, and says what came back.
    s.trash(Trashable::Board(s.boards().unwrap()[0].id), at(2))
        .unwrap();
    let revived = s.restore_with(Trashable::Item(a_item.id), limit).unwrap();
    assert_eq!(revived, [Trashable::Board(a.id)]);
    assert_eq!(board_names(&s), ["A", "C", "D"]);
}

#[test]
fn restore_with_reports_every_record_it_brought_back() {
    let (mut s, b, l, it) = tree();
    s.trash(Trashable::Item(it[0].id), at(1)).unwrap();
    s.trash(Trashable::List(l.id), at(2)).unwrap();
    s.trash(Trashable::Board(b.id), at(3)).unwrap();
    let revived = s.restore_with(Trashable::Item(it[0].id), None).unwrap();
    assert_eq!(
        revived,
        [
            Trashable::Item(it[0].id),
            Trashable::List(l.id),
            Trashable::Board(b.id)
        ]
    );
    // Restoring something live revives nothing.
    assert!(
        s.restore_with(Trashable::Item(it[1].id), Some(0))
            .unwrap()
            .is_empty()
    );
}
