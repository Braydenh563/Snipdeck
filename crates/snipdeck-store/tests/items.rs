//! Items, their content, and tags on items.
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

use std::collections::BTreeSet;

use common::*;
use snipdeck_core::{
    BlobRef, Content, FileEntry, Format, FormatKind, Formats, INLINE_LIMIT, Item, ItemId, ItemKind,
    ListId, Placement, Tag, Timestamp,
};
use snipdeck_store::{BlobStore, Error, ItemRepo, Store, TagRepo};

fn setup() -> (Store, snipdeck_core::List) {
    let mut s = store();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    (s, l)
}

fn on_disk() -> (tempfile::TempDir, Store, snipdeck_core::List) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    let b = board(&mut s, "b");
    let l = list(&mut s, b.id, "l");
    (dir, s, l)
}

fn blob_files(dir: &std::path::Path) -> usize {
    let root = dir.join("blobs");
    let Ok(shards) = std::fs::read_dir(root) else {
        return 0;
    };
    shards
        .flatten()
        .map(|shard| std::fs::read_dir(shard.path()).map_or(0, |files| files.flatten().count()))
        .sum()
}

#[test]
fn an_item_round_trips_every_field() {
    let (mut s, l) = setup();
    let tag = Tag::new(name("invoices"));
    s.insert_tag(&tag).unwrap();
    let mut it = Item::new(l.id, "Signature".to_owned(), ItemKind::Template, at(0));
    it.preview = "Kind regards".to_owned();
    it.tags = BTreeSet::from([tag.id]);
    it.pinned = true;
    it.always_plain = true;
    it.use_count = 42;
    it.last_used_at = Some(Timestamp::new(1_800_000_123, 456_789_012).unwrap());
    it.source_app = Some("Outlook".to_owned());
    it.created_at = Timestamp::new(1_700_000_000, 1).unwrap(); // nanosecond precision survives
    it.updated_at = Timestamp::new(1_700_000_001, 999_999_999).unwrap();
    s.insert_item(&it, &text_formats("Kind regards"), Placement::Last)
        .unwrap();
    assert_eq!(s.item(it.id).unwrap(), it);
    assert_eq!(s.items(l.id).unwrap(), [it.clone()]);
    assert_eq!(s.all_items().unwrap(), [it]);
}

#[test]
fn unused_optional_fields_stay_none() {
    let (mut s, l) = setup();
    let it = item(&mut s, l.id, "plain", "x");
    let back = s.item(it.id).unwrap();
    assert_eq!((back.last_used_at, back.source_app.clone()), (None, None));
    assert_eq!(back.tags.len(), 0);
}

#[test]
fn formats_round_trip_in_preference_order() {
    let (mut s, l) = setup();
    let it = Item::new(l.id, "rich".to_owned(), ItemKind::Text, at(0));
    let formats = Formats::new(vec![
        Format::Html(text("<b>hi</b>")),
        Format::Text(text("hi")),
        Format::Rtf(text("{\\rtf1 hi}")),
        Format::Png(text("png-bytes")),
    ])
    .unwrap();
    s.insert_item(&it, &formats, Placement::Last).unwrap();
    assert_eq!(s.formats(it.id).unwrap(), formats);
}

#[test]
fn files_formats_round_trip_with_inline_and_blob_payloads() {
    let (_dir, mut s, l) = on_disk();
    let small = FileEntry::new("note.txt", s.put(b"small".to_vec()).unwrap()).unwrap();
    let big = FileEntry::new("scan.pdf", s.put(vec![5; INLINE_LIMIT + 10]).unwrap()).unwrap();
    assert!(matches!(big.content(), Content::Blob { .. }));
    let formats = Formats::new(vec![
        Format::Files(vec![big, small]),
        Format::Text(text("2 files")),
    ])
    .unwrap();
    let it = Item::new(l.id, "files".to_owned(), ItemKind::File, at(0));
    s.insert_item(&it, &formats, Placement::Last).unwrap();
    assert_eq!(s.formats(it.id).unwrap(), formats);
    assert_healthy(&s);
}

#[test]
fn large_content_is_read_back_through_the_blob_store() {
    let (_dir, mut s, l) = on_disk();
    let body = "x".repeat(INLINE_LIMIT * 2);
    let content = s.put(body.clone().into_bytes()).unwrap();
    let it = Item::new(l.id, "big".to_owned(), ItemKind::Text, at(0));
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Text(content)]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    let formats = s.formats(it.id).unwrap();
    let Some(Format::Text(content)) = formats.get(FormatKind::Text) else {
        panic!()
    };
    assert_eq!(s.read(content).unwrap().as_ref(), body.as_bytes());
}

#[test]
fn a_reference_to_a_blob_that_does_not_exist_is_refused() {
    let (mut s, l) = setup();
    let ghost = Content::Blob {
        blob: BlobRef::from_bytes([1; 32]),
        len: 100_000,
    };
    let it = Item::new(l.id, "ghost".to_owned(), ItemKind::Image, at(0));
    let err = s.insert_item(
        &it,
        &Formats::new(vec![Format::Png(ghost)]).unwrap(),
        Placement::Last,
    );
    assert!(
        matches!(err, Err(Error::NotFound { entity: "blob", .. })),
        "{err:?}"
    );
    assert!(matches!(s.item(it.id), Err(Error::NotFound { .. })));
}

#[test]
fn inserting_into_a_missing_list_or_twice_fails_cleanly() {
    let (mut s, l) = setup();
    let orphan = Item::new(ListId::generate(), "o".to_owned(), ItemKind::Text, at(0));
    assert!(matches!(
        s.insert_item(&orphan, &text_formats("x"), Placement::Last),
        Err(Error::NotFound { entity: "list", .. })
    ));
    let it = item(&mut s, l.id, "once", "x");
    assert!(matches!(
        s.insert_item(&it, &text_formats("x"), Placement::Last),
        Err(Error::AlreadyExists { entity: "item", .. })
    ));
    assert_eq!(s.items(l.id).unwrap().len(), 1);
}

#[test]
fn an_unknown_tag_on_an_item_is_refused_and_nothing_is_written() {
    let (mut s, l) = setup();
    let mut it = Item::new(l.id, "t".to_owned(), ItemKind::Text, at(0));
    it.tags.insert(snipdeck_core::TagId::generate());
    let err = s.insert_item(&it, &text_formats("x"), Placement::Last);
    assert!(
        matches!(err, Err(Error::NotFound { entity: "tag", .. })),
        "{err:?}"
    );
    assert_eq!(s.items(l.id).unwrap().len(), 0);
    assert_healthy(&s);
}

#[test]
fn a_sensitive_item_cannot_carry_a_preview() {
    let (mut s, l) = setup();
    let mut it = Item::new(l.id, "pin".to_owned(), ItemKind::Text, at(0));
    it.sensitive = true;
    it.preview = "1234".to_owned();
    let err = s.insert_item(&it, &text_formats("1234"), Placement::Last);
    assert!(matches!(err, Err(Error::Invalid(_))), "{err:?}");
    it.preview.clear();
    s.insert_item(&it, &text_formats("1234"), Placement::Last)
        .unwrap();
}

#[test]
fn timestamps_outside_the_nanosecond_range_are_refused() {
    let (mut s, l) = setup();
    let mut it = Item::new(l.id, "t".to_owned(), ItemKind::Text, at(0));
    it.created_at = Timestamp::MAX;
    assert!(matches!(
        s.insert_item(&it, &text_formats("x"), Placement::Last),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn update_overwrites_every_field_but_keeps_content_when_none() {
    let (mut s, l) = setup();
    let mut it = item(&mut s, l.id, "old", "body");
    it.title = "new".to_owned();
    it.pinned = true;
    it.kind = ItemKind::Code;
    it.updated_at = at(60);
    it.source_app = Some("Code".to_owned());
    s.update_item(&it, None).unwrap();
    assert_eq!(s.item(it.id).unwrap(), it);
    assert_eq!(s.formats(it.id).unwrap(), text_formats("body"));
}

#[test]
fn update_with_formats_replaces_the_content() {
    let (mut s, l) = setup();
    let it = item(&mut s, l.id, "t", "old");
    let new = Formats::new(vec![
        Format::Html(text("<i>new</i>")),
        Format::Text(text("new")),
    ])
    .unwrap();
    s.update_item(&it, Some(&new)).unwrap();
    assert_eq!(s.formats(it.id).unwrap(), new);
    assert_healthy(&s);
}

#[test]
fn updating_a_missing_item_is_not_found() {
    let (mut s, l) = setup();
    let ghost = Item::new(l.id, "g".to_owned(), ItemKind::Text, at(0));
    assert!(matches!(
        s.update_item(&ghost, None),
        Err(Error::NotFound { entity: "item", .. })
    ));
    assert!(matches!(
        s.formats(ghost.id),
        Err(Error::NotFound { entity: "item", .. })
    ));
    assert!(matches!(
        s.item(ItemId::generate()),
        Err(Error::NotFound { entity: "item", .. })
    ));
}

#[test]
fn a_replaced_blob_is_deleted_unless_something_else_uses_it() {
    let (dir, mut s, l) = on_disk();
    let shared = s.put(vec![1; INLINE_LIMIT + 1]).unwrap();
    let private = s.put(vec![2; INLINE_LIMIT + 1]).unwrap();
    let a = Item::new(l.id, "a".to_owned(), ItemKind::Image, at(0));
    let b = Item::new(l.id, "b".to_owned(), ItemKind::Image, at(0));
    s.insert_item(
        &a,
        &Formats::new(vec![Format::Png(shared.clone())]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    s.insert_item(
        &b,
        &Formats::new(vec![
            Format::Png(shared.clone()),
            Format::Rtf(private.clone()),
        ])
        .unwrap(),
        Placement::Last,
    )
    .unwrap();
    assert_eq!(blob_files(dir.path()), 2);

    s.update_item(&b, Some(&text_formats("now small"))).unwrap();
    assert_eq!(
        blob_files(dir.path()),
        1,
        "b's private blob went, the shared one stayed"
    );
    assert!(s.read(&shared).is_ok());
    assert!(matches!(
        s.read(&private),
        Err(Error::NotFound { entity: "blob", .. })
    ));
    assert_healthy(&s);
}

#[test]
fn record_use_counts_without_touching_updated_at_and_saturates() {
    let (mut s, l) = setup();
    let it = item(&mut s, l.id, "t", "x");
    s.record_use(it.id, at(10)).unwrap();
    s.record_use(it.id, at(20)).unwrap();
    let back = s.item(it.id).unwrap();
    assert_eq!(
        (back.use_count, back.last_used_at, back.updated_at),
        (2, Some(at(20)), it.updated_at)
    );

    // Usage is set on insert (an import) and only `record_use` changes it afterwards.
    let mut maxed = Item::new(l.id, "maxed".to_owned(), ItemKind::Text, at(0));
    maxed.preview = "x".to_owned();
    maxed.use_count = u32::MAX;
    s.insert_item(&maxed, &text_formats("x"), Placement::Last)
        .unwrap();
    s.record_use(maxed.id, at(30)).unwrap();
    assert_eq!(s.item(maxed.id).unwrap().use_count, u32::MAX);
    assert!(matches!(
        s.record_use(ItemId::generate(), at(1)),
        Err(Error::NotFound { entity: "item", .. })
    ));
}

#[test]
fn all_items_follow_board_then_list_then_item_order() {
    let mut s = store();
    let b1 = board(&mut s, "b1");
    let b2 = board(&mut s, "b2");
    let l2 = list(&mut s, b2.id, "l2");
    let l1b = list(&mut s, b1.id, "l1b");
    let l1a = list(&mut s, b1.id, "l1a");
    item(&mut s, l2.id, "b2-l2-1", "x");
    item(&mut s, l1b.id, "b1-l1b-1", "x");
    item(&mut s, l1a.id, "b1-l1a-1", "x");
    item(&mut s, l1b.id, "b1-l1b-2", "x");
    let titles: Vec<_> = s
        .all_items()
        .unwrap()
        .into_iter()
        .map(|i| i.title)
        .collect();
    assert_eq!(titles, ["b1-l1b-1", "b1-l1b-2", "b1-l1a-1", "b2-l2-1"]);
}

#[test]
fn tags_on_items_are_replaced_by_update_and_removed_with_the_tag() {
    let (mut s, l) = setup();
    let red = Tag::new(name("red"));
    let blue = Tag::new(name("blue"));
    s.insert_tag(&red).unwrap();
    s.insert_tag(&blue).unwrap();
    let mut it = item(&mut s, l.id, "t", "x");
    it.tags = BTreeSet::from([red.id, blue.id]);
    s.update_item(&it, None).unwrap();
    assert_eq!(
        s.item(it.id).unwrap().tags,
        BTreeSet::from([red.id, blue.id])
    );

    it.tags = BTreeSet::from([blue.id]);
    s.update_item(&it, None).unwrap();
    assert_eq!(s.item(it.id).unwrap().tags, BTreeSet::from([blue.id]));

    s.delete_tag(blue.id).unwrap();
    assert_eq!(s.item(it.id).unwrap().tags.len(), 0);
    assert_eq!(s.all_items().unwrap()[0].tags.len(), 0);
    assert_healthy(&s);
}

#[test]
fn the_store_is_a_blob_store() {
    let mut s = store();
    assert!(matches!(
        s.put(b"tiny".to_vec()).unwrap(),
        Content::Inline(_)
    ));
    let big = s.put(vec![9; INLINE_LIMIT + 1]).unwrap();
    assert_eq!(s.read(&big).unwrap().len(), INLINE_LIMIT + 1);
}

fn image(content: Content) -> Formats {
    Formats::new(vec![Format::Png(content)]).unwrap()
}

/// Review M3: `put` hands out a blob that is not attached yet; an unrelated write that releases
/// the same bytes must not delete it.
#[test]
fn put_then_unrelated_update_keeps_the_pending_blob() {
    let (_dir, mut s, l) = on_disk();
    let big = vec![7; INLINE_LIMIT + 1];
    let first = s.put(big.clone()).unwrap();
    let used = Item::new(l.id, "x".to_owned(), ItemKind::Image, at(0));
    s.insert_item(&used, &image(first.clone()), Placement::Last)
        .unwrap();

    // A second capture of the same bytes: same blob, not attached yet.
    let pending = s.put(big.clone()).unwrap();
    assert_eq!(pending, first);
    // The user edits X's content, which releases the blob X used.
    s.update_item(&used, Some(&text_formats("replaced")))
        .unwrap();

    let later = Item::new(l.id, "y".to_owned(), ItemKind::Image, at(0));
    s.insert_item(&later, &image(pending.clone()), Placement::Last)
        .unwrap();
    assert_eq!(s.read(&pending).unwrap().as_ref(), big.as_slice());
    assert_healthy(&s);
}

/// Review M3, the purge path.
#[test]
fn put_then_purge_keeps_the_pending_blob() {
    use snipdeck_store::{TrashRepo, Trashable};
    let (_dir, mut s, l) = on_disk();
    let big = vec![8; INLINE_LIMIT + 1];
    let first = s.put(big.clone()).unwrap();
    let used = Item::new(l.id, "x".to_owned(), ItemKind::Image, at(0));
    s.insert_item(&used, &image(first), Placement::Last)
        .unwrap();
    let pending = s.put(big.clone()).unwrap();
    s.trash(Trashable::Item(used.id), at(1)).unwrap();
    assert_eq!(s.purge(at(2)).unwrap(), 1);

    let later = Item::new(l.id, "y".to_owned(), ItemKind::Image, at(3));
    s.insert_item(&later, &image(pending.clone()), Placement::Last)
        .unwrap();
    assert_eq!(s.read(&pending).unwrap().as_ref(), big.as_slice());
}

/// Once attached, a released blob is deleted as before (the pending mark is used up).
#[test]
fn an_attached_blob_is_still_deleted_when_released() {
    let (dir, mut s, l) = on_disk();
    let b = s.put(vec![9; INLINE_LIMIT + 1]).unwrap();
    let x = Item::new(l.id, "x".to_owned(), ItemKind::Image, at(0));
    s.insert_item(&x, &image(b), Placement::Last).unwrap();
    assert_eq!(blob_files(dir.path()), 1);
    s.update_item(&x, Some(&text_formats("small"))).unwrap();
    assert_eq!(blob_files(dir.path()), 0);
}

/// Puts a directory where the blob file was, so deleting it fails (as an antivirus handle
/// would on Windows).
fn block_deletion(dir: &std::path::Path, content: &Content) {
    let Content::Blob { blob, .. } = content else {
        panic!("expected a blob")
    };
    let hex = blob.to_string();
    let path = dir.join("blobs").join(&hex[..2]).join(&hex[2..]);
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
}

/// Review M4: the change is committed, so a failed delete afterwards must not be an error.
#[test]
fn a_failed_blob_delete_after_commit_is_not_an_error() {
    let (dir, mut s, l) = on_disk();
    let b = s.put(vec![3; INLINE_LIMIT + 1]).unwrap();
    let x = Item::new(l.id, "x".to_owned(), ItemKind::Image, at(0));
    s.insert_item(&x, &image(b.clone()), Placement::Last)
        .unwrap();
    block_deletion(dir.path(), &b);

    s.update_item(&x, Some(&text_formats("new"))).unwrap();
    assert_eq!(s.formats(x.id).unwrap(), text_formats("new"));
}

/// Review M4, the purge path: the count is returned although a delete failed.
#[test]
fn a_failed_blob_delete_after_purge_still_reports_the_count() {
    use snipdeck_store::{TrashRepo, Trashable};
    let (dir, mut s, l) = on_disk();
    let b = s.put(vec![4; INLINE_LIMIT + 1]).unwrap();
    let x = Item::new(l.id, "x".to_owned(), ItemKind::Image, at(0));
    s.insert_item(&x, &image(b.clone()), Placement::Last)
        .unwrap();
    block_deletion(dir.path(), &b);
    s.trash(Trashable::Item(x.id), at(1)).unwrap();
    assert_eq!(s.purge(at(2)).unwrap(), 1);
    assert_eq!(s.trashed().unwrap().len(), 0);
}

/// Review L11: a metadata-only edit neither reads the content nor loses it from the index, even
/// when the content's blob cannot be read right now.
#[test]
fn metadata_edits_do_not_read_the_content() {
    let (dir, mut s, l) = on_disk();
    let body = format!("needle {}", "x".repeat(INLINE_LIMIT));
    let content = s.put(body.into_bytes()).unwrap();
    let mut it = Item::new(l.id, "title".to_owned(), ItemKind::Text, at(0));
    it.preview = "needle".to_owned();
    s.insert_item(
        &it,
        &Formats::new(vec![Format::Text(content.clone())]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    // Damage the blob without changing its length: any read now fails.
    let Content::Blob { blob, .. } = &content else {
        panic!()
    };
    let hex = blob.to_string();
    let path = dir.path().join("blobs").join(&hex[..2]).join(&hex[2..]);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();

    it.pinned = true;
    it.title = "renamed".to_owned();
    s.update_item(&it, None).unwrap();
    assert_eq!(
        s.search("needle", 5).unwrap(),
        [it.id],
        "body still indexed"
    );
    assert_eq!(s.search("renamed", 5).unwrap(), [it.id], "title updated");
}

/// Review L12: `record_use` owns the usage columns; saving an item loaded earlier keeps them.
#[test]
fn saving_a_stale_item_does_not_roll_back_usage() {
    let (mut s, l) = setup();
    let mut it = item(&mut s, l.id, "t", "body");
    s.record_use(it.id, at(10)).unwrap();
    s.record_use(it.id, at(20)).unwrap();
    // `it` was loaded before the uses: count 0, never used.
    it.title = "edited".to_owned();
    s.update_item(&it, None).unwrap();
    let stored = s.item(it.id).unwrap();
    assert_eq!(stored.title, "edited");
    assert_eq!(stored.use_count, 2);
    assert_eq!(stored.last_used_at, Some(at(20)));
}

/// Review L5: `Debug` shows no content bytes.
#[test]
fn debug_output_of_a_store_holds_no_content() {
    let mut s = store();
    s.put(vec![0xAB; INLINE_LIMIT + 1]).unwrap();
    let shown = format!("{s:?}");
    assert!(shown.len() < 500, "{} characters", shown.len());
    assert!(!shown.contains("171"), "{shown}");
}

/// Review L10: listing the children of a parent that is not live is `NotFound`.
#[test]
fn listing_items_of_a_missing_or_trashed_list_is_not_found() {
    use snipdeck_store::{TrashRepo, Trashable};
    let (mut s, l) = setup();
    assert!(matches!(
        s.items(ListId::generate()),
        Err(Error::NotFound { entity: "list", .. })
    ));
    s.trash(Trashable::List(l.id), at(1)).unwrap();
    assert!(matches!(
        s.items(l.id),
        Err(Error::NotFound { entity: "list", .. })
    ));
}
