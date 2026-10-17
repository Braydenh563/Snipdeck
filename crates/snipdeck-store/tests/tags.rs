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
use snipdeck_core::{Tag, TagId};
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
