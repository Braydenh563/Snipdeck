//! Tags: unique ignoring case, not trashed.
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
use snipdeck_core::{ItemLabel, Tag, TagId};
use snipdeck_store::{Error, TagRepo};

fn names(s: &impl TagRepo) -> Vec<String> {
    s.tags()
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

#[test]
fn tags_come_back_sorted_ignoring_case() {
    let mut s = store();
    for n in ["banana", "Apple", "cherry", "apricot"] {
        s.insert_tag(&Tag::new(name(n))).unwrap();
    }
    assert_eq!(names(&s), ["Apple", "apricot", "banana", "cherry"]);
}

#[test]
fn a_tag_keeps_the_spelling_it_was_given() {
    let mut s = store();
    let t = Tag::new(name("iOS Tips"));
    s.insert_tag(&t).unwrap();
    assert_eq!(s.tags().unwrap(), [t]);
}

#[test]
fn names_clash_ignoring_case_including_non_ascii() {
    let mut s = store();
    s.insert_tag(&Tag::new(name("École"))).unwrap();
    s.insert_tag(&Tag::new(name("Work"))).unwrap();
    for clash in ["école", "ÉCOLE", "work", "WORK"] {
        let err = s.insert_tag(&Tag::new(name(clash)));
        assert!(
            matches!(err, Err(Error::TagExists(ref n)) if n == clash),
            "{clash}: {err:?}"
        );
    }
    assert_eq!(s.tags().unwrap().len(), 2);
}

#[test]
fn renaming_checks_clashes_but_allows_changing_only_the_case() {
    let mut s = store();
    let work = Tag::new(name("work"));
    let home = Tag::new(name("home"));
    s.insert_tag(&work).unwrap();
    s.insert_tag(&home).unwrap();

    let mut renamed = work.clone();
    renamed.name = name("Work");
    s.update_tag(&renamed).unwrap();
    assert_eq!(names(&s), ["home", "Work"]);

    renamed.name = name("HOME");
    assert!(matches!(s.update_tag(&renamed), Err(Error::TagExists(_))));
    assert_eq!(names(&s), ["home", "Work"]);
}

#[test]
fn inserting_the_same_tag_id_twice_fails() {
    let mut s = store();
    let t = Tag::new(name("a"));
    s.insert_tag(&t).unwrap();
    let mut again = t.clone();
    again.name = name("b");
    assert!(matches!(
        s.insert_tag(&again),
        Err(Error::AlreadyExists { entity: "tag", .. })
    ));
}

#[test]
fn missing_tags_are_not_found() {
    let mut s = store();
    let ghost = Tag {
        id: TagId::generate(),
        name: name("ghost"),
        colour: None,
        parent: None,
    };
    assert!(matches!(
        s.update_tag(&ghost),
        Err(Error::NotFound { entity: "tag", .. })
    ));
    assert!(matches!(
        s.delete_tag(ghost.id),
        Err(Error::NotFound { entity: "tag", .. })
    ));
}

#[test]
fn a_deleted_tag_name_can_be_reused() {
    let mut s = store();
    let t = Tag::new(name("temp"));
    s.insert_tag(&t).unwrap();
    s.delete_tag(t.id).unwrap();
    s.insert_tag(&Tag::new(name("Temp"))).unwrap();
}

/// Review N5: uniqueness folds case fully (ß = SS) and ignores Unicode normalisation.
#[test]
fn names_clash_after_full_case_folding_and_normalisation() {
    let mut s = store();
    s.insert_tag(&Tag::new(name("Straße"))).unwrap();
    assert!(matches!(
        s.insert_tag(&Tag::new(name("STRASSE"))),
        Err(Error::TagExists(_))
    ));
    s.insert_tag(&Tag::new(name("\u{c9}cole"))).unwrap(); // É precomposed
    assert!(matches!(
        s.insert_tag(&Tag::new(name("e\u{301}cole"))), // e + combining acute
        Err(Error::TagExists(_))
    ));
    s.insert_tag(&Tag::new(name("\u{3a3}\u{39f}\u{3a3}")))
        .unwrap(); // ΣΟΣ
    assert!(matches!(
        s.insert_tag(&Tag::new(name("\u{3c3}\u{3bf}\u{3c2}"))), // σος
        Err(Error::TagExists(_))
    ));
}

#[test]
fn a_deleted_tag_can_be_linked_back_to_live_and_trashed_items() {
    use snipdeck_store::{ItemRepo, TrashRepo, Trashable};
    let mut s = store();
    let b = board(&mut s, "Work");
    let l = list(&mut s, b.id, "Replies");
    let tag = Tag::new(name("urgent"));
    s.insert_tag(&tag).unwrap();
    let (live, trashed, plain) = (
        item(&mut s, l.id, "live", "a"),
        item(&mut s, l.id, "trashed", "b"),
        item(&mut s, l.id, "plain", "c"),
    );
    for it in [&live, &trashed] {
        let mut tagged = (*it).clone();
        tagged.tags.insert(tag.id);
        s.update_item(&tagged, None).unwrap();
    }
    s.trash(Trashable::Item(trashed.id), at(100)).unwrap();
    let mut linked = s.tag_items(tag.id).unwrap();
    linked.sort();
    let mut want = vec![live.id, trashed.id];
    want.sort();
    assert_eq!(linked, want, "the Trash's item counts too");
    s.delete_tag(tag.id).unwrap();
    assert_eq!(s.tag_items(tag.id).unwrap().len(), 0)
    s.insert_tag(&tag).unwrap();
    s.link_tag(
        tag.id,
        &[
            live.id,
            trashed.id,
            plain.id,
            snipdeck_core::ItemId::generate(),
        ],
    )
    .unwrap();
    assert!(s.item(live.id).unwrap().tags.contains(&tag.id));
    s.restore(Trashable::Item(trashed.id)).unwrap();
    assert!(s.item(trashed.id).unwrap().tags.contains(&tag.id));
    assert!(matches!(
        s.link_tag(TagId::generate(), &[]),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn a_tag_keeps_its_colour_across_a_rename_and_a_reload() {
    let mut s = store();
    let mut t = Tag::new(name("billing"));
    t.colour = Some(ItemLabel::Teal);
    s.insert_tag(&t).unwrap();
    assert_eq!(s.tags().unwrap()[0].colour, Some(ItemLabel::Teal));

    t.name = name("Billing 2026");
    s.update_tag(&t).unwrap();
    assert_eq!(s.tags().unwrap(), [t.clone()]);

    t.colour = None;
    s.update_tag(&t).unwrap();
    assert_eq!(s.tags().unwrap()[0].colour, None);
}

#[test]
fn a_board_keeps_its_accent_and_icon() {
    use snipdeck_core::{ListAccent, ListIcon, Placement, Sort, SortKey};
    use snipdeck_store::{BoardRepo, ListRepo};
    let mut s = store();
    let mut b = snipdeck_core::Board::new(name("Work"), at(0));
    b.accent = Some(ListAccent::Rose);
    b.icon = Some(ListIcon::Briefcase);
    s.insert_board(&b, Placement::Last).unwrap();
    assert_eq!(s.board(b.id).unwrap(), b);
    b.accent = None;
    b.icon = Some(ListIcon::Globe);
    s.update_board(&b).unwrap();
    assert_eq!(s.boards().unwrap(), [b.clone()]);

    // Every sort a list can be given survives storage.
    let mut l = list(&mut s, b.id, "Replies");
    for key in SortKey::ALL {
        for reversed in [false, true] {
            l.sort = Sort::new(*key, reversed);
            s.update_list(&l).unwrap();
            assert_eq!(s.lists(b.id).unwrap()[0].sort, l.sort, "{}", l.sort.key());
        }
    }
}

fn nested(s: &mut impl TagRepo, label: &str, parent: Option<&Tag>) -> Tag {
    let tag = Tag {
        parent: parent.map(|p| p.id),
        ..Tag::new(name(label))
    };
    s.insert_tag(&tag).unwrap();
    tag
}

#[test]
fn a_tag_keeps_its_parent_across_a_rename_and_a_reload() {
    let mut s = store();
    let work = nested(&mut s, "work", None);
    let mut invoices = nested(&mut s, "invoices", Some(&work));
    invoices.name = name("bills");
    s.update_tag(&invoices).unwrap();
    let stored = s.tags().unwrap();
    let bills = stored.iter().find(|t| t.id == invoices.id).unwrap();
    assert_eq!(bills.parent, Some(work.id));
    assert_eq!(
        stored.iter().find(|t| t.id == work.id).unwrap().parent,
        None
    );
}

#[test]
fn a_tag_cannot_be_nested_under_itself_a_descendant_or_a_stranger() {
    let mut s = store();
    let a = nested(&mut s, "a", None);
    let b = nested(&mut s, "b", Some(&a));
    let c = nested(&mut s, "c", Some(&b));
    let mut under_itself = a.clone();
    under_itself.parent = Some(a.id);
    assert!(matches!(s.update_tag(&under_itself), Err(Error::TagParent)));
    let mut loop_ = a.clone();
    loop_.parent = Some(c.id);
    assert!(
        matches!(s.update_tag(&loop_), Err(Error::TagParent)),
        "c is below a"
    );
    let ghost = Tag {
        parent: Some(TagId::generate()),
        ..Tag::new(name("orphan"))
    };
    assert!(matches!(s.insert_tag(&ghost), Err(Error::TagParent)));
    let mut moved = c.clone();
    moved.parent = Some(a.id);
    s.update_tag(&moved).unwrap();
    assert_eq!(names(&s).len(), 3, "nothing was lost");
}

#[test]
fn deleting_a_tag_moves_its_children_up_to_its_own_parent() {
    let mut s = store();
    let a = nested(&mut s, "a", None);
    let b = nested(&mut s, "b", Some(&a));
    let c = nested(&mut s, "c", Some(&b));
    s.delete_tag(b.id).unwrap();
    let stored = s.tags().unwrap();
    assert_eq!(
        stored.iter().find(|t| t.id == c.id).unwrap().parent,
        Some(a.id)
    );
    s.delete_tag(a.id).unwrap();
    let stored = s.tags().unwrap();
    assert_eq!(stored.iter().find(|t| t.id == c.id).unwrap().parent, None);
}
