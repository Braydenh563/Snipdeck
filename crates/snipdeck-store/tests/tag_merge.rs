//! Merging a tag into another one, and finding a tag by name.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use common::*;
use snipdeck_core::{Item, ListId, Tag, TagId};
use snipdeck_store::{Error, ItemRepo, Store, TagRepo};

fn names(s: &impl TagRepo) -> Vec<String> {
    s.tags()
        .unwrap()
        .iter()
        .map(|t| t.name.to_string())
        .collect()
}

fn tagged(s: &mut Store, list: ListId, title: &str, tags: &[&Tag]) -> Item {
    let mut it = item(s, list, title, "body");
    it.tags = tags.iter().map(|t| t.id).collect();
    s.update_item(&it, None).unwrap();
    it
}

#[test]
fn merging_moves_items_once_and_removes_the_source_tag() {
    let mut s = store();
    let b = board(&mut s, "B");
    let l = list(&mut s, b.id, "L");
    let (old, new) = (Tag::new(name("old")), Tag::new(name("new")));
    s.insert_tag(&old).unwrap();
    s.insert_tag(&new).unwrap();
    let only_old = tagged(&mut s, l.id, "a", &[&old]);
    let both = tagged(&mut s, l.id, "b", &[&old, &new]);
    let only_new = tagged(&mut s, l.id, "c", &[&new]);

    assert_eq!(s.merge_tag(old.id, new.id).unwrap(), 1);

    assert_eq!(names(&s), ["new"]);
    for it in [&only_old, &both, &only_new] {
        let got = s.item(it.id).unwrap();
        assert_eq!(got.tags.len(), 1, "{}", it.title);
        assert!(got.tags.contains(&new.id), "{}", it.title);
    }
}

#[test]
fn merging_checks_both_tags_and_ignores_a_tag_into_itself() {
    let mut s = store();
    let t = Tag::new(name("t"));
    s.insert_tag(&t).unwrap();
    let ghost = TagId::generate();
    assert!(matches!(
        s.merge_tag(ghost, t.id),
        Err(Error::NotFound { entity: "tag", .. })
    ));
    assert!(matches!(
        s.merge_tag(t.id, ghost),
        Err(Error::NotFound { entity: "tag", .. })
    ));
    assert_eq!(s.merge_tag(t.id, t.id).unwrap(), 0);
    assert_eq!(names(&s), ["t"]);
}

#[test]
fn find_tag_uses_the_same_key_as_the_clash_check() {
    let mut s = store();
    let t = Tag::new(name("Straße"));
    s.insert_tag(&t).unwrap();
    assert_eq!(s.find_tag(&name("STRASSE")).unwrap(), Some(t));
    assert_eq!(s.find_tag(&name("other")).unwrap(), None);
}
