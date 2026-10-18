//! The shared back half of every import: a validated [`Plan`] and the code that writes it.
//!
//! Each source (archive, folder, Ditto, `CopyQ`) first turns its input into a `Plan`, doing all
//! validation on the way. [`apply`] then writes it. Everything that can be checked without
//! touching the library is checked before `apply` starts, so failures after that point are
//! I/O or database errors.
//!
//! # Atomicity
//!
//! `apply` writes the whole plan inside one store transaction
//! ([`Library::atomically`], [`snipdeck_store::Store::transaction`]): the boards, lists, items,
//! tags and blob files it adds all commit together or not at all. On any error, or a panic,
//! the database is as it was and the blob files the import created are deleted; existing
//! records are never modified. A crash in the middle leaves no partial board, only
//! unreferenced blob files that `Store::gc_blobs` removes.

use std::collections::{BTreeSet, HashMap, HashSet};

use snipdeck_core::{
    BlobRef, Board, Content, FileEntry, Format, FormatKind, Formats, Item, ItemId, ItemKind, List,
    ListAccent, ListSort, ListView, Name, Placement, Tag, TagId, Timestamp,
};
use snipdeck_store::crypto::{DataKey, Slot};
use snipdeck_store::{
    Batch, BlobStore, BoardRepo, ItemRepo, ListRepo, TagRepo, TrashRepo, name_key,
};

use crate::error::{Error, Result};
use crate::report::ImportReport;

/// Everything this crate needs from storage; [`snipdeck_store::Store`] implements it.
pub trait Library: BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo + BlobStore {
    /// The library's data key while it is unlocked, for [`KeySource::Library`](crate::KeySource)
    /// and for callers that pass it to an export. `None` if the library has no key or is locked.
    fn data_key(&self) -> Option<&DataKey>;

    /// Runs `f` as one atomic unit: everything it writes through the [`Batch`] commits
    /// together when `f` returns `Ok`, and none of it (blob files included) remains if it
    /// returns `Err` or panics. See [`snipdeck_store::Batch`].
    ///
    /// # Errors
    ///
    /// What `f` returns, or a storage error if the transaction cannot start or commit.
    fn atomically<T>(&mut self, f: impl FnOnce(&mut Batch<'_>) -> Result<T>) -> Result<T>;
}

impl Library for snipdeck_store::Store {
    fn data_key(&self) -> Option<&DataKey> {
        snipdeck_store::Store::data_key(self)
    }

    fn atomically<T>(&mut self, f: impl FnOnce(&mut Batch<'_>) -> Result<T>) -> Result<T> {
        self.transaction(f)
    }
}

/// Where the bytes of a payload come from.
#[derive(Debug)]
pub(crate) enum Source {
    /// Already in memory.
    Bytes(Vec<u8>),
    /// Fetched on demand, from the archive.
    Asset(AssetRef),
}

/// An archive asset, and the archive item whose id authenticates it if it is encrypted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AssetRef {
    pub hash: BlobRef,
    /// The archive item and slot that authenticate the payload, when it is encrypted.
    pub encrypted_for: Option<(ItemId, Slot)>,
}

#[derive(Debug)]
pub(crate) struct PlanFile {
    pub name: String,
    pub source: Source,
}

#[derive(Debug)]
pub(crate) enum PlanFormat {
    Single { kind: FormatKind, source: Source },
    Files(Vec<PlanFile>),
}

#[derive(Debug)]
pub(crate) struct PlanItem {
    pub title: String,
    pub kind: ItemKind,
    pub pinned: bool,
    pub sensitive: bool,
    pub always_plain: bool,
    pub use_count: u32,
    pub last_used_at: Option<Timestamp>,
    pub source_app: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    /// Indexes into [`Plan::tags`].
    pub tags: BTreeSet<usize>,
    pub formats: Vec<PlanFormat>,
    /// The source's own preview; when `None` it is derived from the text format.
    pub preview: Option<String>,
    /// See [`ContentHasher`]; compared against the library to skip duplicates.
    pub content_hash: [u8; 32],
}

impl PlanItem {
    /// A plain text item from memory.
    pub(crate) fn text(title: String, text: String, now: Timestamp) -> Self {
        let mut hasher = ContentHasher::new();
        hasher.single(
            FormatKind::Text,
            blake3::hash(text.as_bytes()).as_bytes(),
            text.len() as u64,
        );
        Self {
            title,
            kind: ItemKind::Text,
            pinned: false,
            sensitive: false,
            always_plain: false,
            use_count: 0,
            last_used_at: None,
            source_app: None,
            created_at: now,
            updated_at: now,
            tags: BTreeSet::new(),
            formats: vec![PlanFormat::Single {
                kind: FormatKind::Text,
                source: Source::Bytes(text.into_bytes()),
            }],
            preview: None,
            content_hash: hasher.finish(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PlanList {
    pub name: Name,
    pub accent: Option<ListAccent>,
    pub view: ListView,
    pub sort: ListSort,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub items: Vec<PlanItem>,
}

impl PlanList {
    pub(crate) fn new(name: Name, now: Timestamp) -> Self {
        Self {
            name,
            accent: None,
            view: ListView::Comfortable,
            sort: ListSort::Manual,
            created_at: now,
            updated_at: now,
            items: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct PlanBoard {
    pub name: Name,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub lists: Vec<PlanList>,
}

#[derive(Debug, Default)]
pub(crate) struct Plan {
    pub tags: Vec<Name>,
    pub boards: Vec<PlanBoard>,
}

impl Plan {
    #[cfg(test)]
    pub(crate) fn item_count(&self) -> usize {
        self.boards
            .iter()
            .flat_map(|b| &b.lists)
            .map(|l| l.items.len())
            .sum()
    }

    /// Removes items whose content hash is in `known`, and adds the hashes of the rest, so an
    /// item repeated within the import is dropped too. Lists keep their place, but a board left
    /// without any item is dropped, so an import with nothing new creates nothing. Returns how
    /// many items were dropped.
    pub(crate) fn drop_duplicates(&mut self, known: &mut HashSet<[u8; 32]>) -> usize {
        let mut dropped = 0;
        for item_list in self.boards.iter_mut().flat_map(|b| &mut b.lists) {
            let before = item_list.items.len();
            item_list
                .items
                .retain(|item| known.insert(item.content_hash));
            dropped += before - item_list.items.len();
        }
        self.boards
            .retain(|b| b.lists.iter().any(|l| !l.items.is_empty()));
        dropped
    }
}

/// A hash of an item's content (not its title or flags), used to recognise duplicates.
///
/// `BLAKE3(for each format in order: kind key, 0, plaintext hash, plaintext length)`, with the
/// files of a `files` format listed as name, 0, hash, length. The plaintext hash of stored
/// content is the blob's own name for blobs and a fresh hash for inline bytes.
pub(crate) struct ContentHasher(blake3::Hasher);

impl ContentHasher {
    pub(crate) fn new() -> Self {
        Self(blake3::Hasher::new_derive_key("snipdeck content hash v1"))
    }

    pub(crate) fn single(&mut self, kind: FormatKind, hash: &[u8; 32], len: u64) {
        self.0.update(kind.key().as_bytes());
        self.0.update(&[0]);
        self.0.update(hash);
        self.0.update(&len.to_le_bytes());
    }

    pub(crate) fn files(&mut self, files: &[(&str, [u8; 32], u64)]) {
        self.0.update(FormatKind::Files.key().as_bytes());
        self.0.update(&[0]);
        self.0.update(&(files.len() as u64).to_le_bytes());
        for (name, hash, len) in files {
            self.0.update(name.as_bytes());
            self.0.update(&[0]);
            self.0.update(hash);
            self.0.update(&len.to_le_bytes());
        }
    }

    pub(crate) fn finish(self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

fn content_parts(content: &Content) -> ([u8; 32], u64) {
    match content {
        Content::Inline(inline) => (
            *blake3::hash(inline.as_bytes()).as_bytes(),
            inline.as_bytes().len() as u64,
        ),
        Content::Blob { blob, len } => (*blob.as_bytes(), *len),
    }
}

/// The content hashes of every live item in `lib` that can be read, for skipping duplicates
/// (encrypted items are left out while the library is locked).
pub(crate) fn existing_hashes<L: Library>(lib: &L) -> Result<HashSet<[u8; 32]>> {
    let mut known = HashSet::new();
    for item in lib.all_items()? {
        let formats = match lib.formats(item.id) {
            Ok(formats) => formats,
            // An encrypted item cannot be read while the library is locked. Skipping it only
            // means a duplicate of it is not recognised; refusing would stop a restore of the
            // public items into a locked library.
            Err(snipdeck_store::Error::EncryptionLocked) => continue,
            Err(e) => return Err(e.into()),
        };
        let mut hasher = ContentHasher::new();
        for format in &formats {
            match format {
                Format::Text(c) | Format::Html(c) | Format::Rtf(c) | Format::Png(c) => {
                    let (hash, len) = content_parts(c);
                    hasher.single(format.kind(), &hash, len);
                }
                Format::Files(files) => {
                    let parts: Vec<_> = files
                        .iter()
                        .map(|f| {
                            let (hash, len) = content_parts(f.content());
                            (f.name(), hash, len)
                        })
                        .collect();
                    hasher.files(&parts);
                }
            }
        }
        known.insert(hasher.finish());
    }
    Ok(known)
}

/// The first part of `bytes` as text, for building a preview without decoding megabytes.
fn preview_text(bytes: &[u8]) -> &str {
    const WINDOW: usize = 4096;
    let head = &bytes[..bytes.len().min(WINDOW)];
    match std::str::from_utf8(head) {
        Ok(text) => text,
        Err(e) => std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or(""),
    }
}

/// Writes `plan` into `lib` in one transaction (see the module docs). On any error nothing is
/// added and the error is returned; on success the report counts what was added.
pub(crate) fn apply<L: Library>(
    lib: &mut L,
    plan: &Plan,
    fetch: &mut dyn FnMut(&AssetRef) -> Result<Vec<u8>>,
    mut report: ImportReport,
    max_boards: Option<usize>,
) -> Result<ImportReport> {
    lib.atomically(|batch| write(batch, plan, fetch, &mut report, max_boards))?;
    Ok(report)
}

fn write(
    lib: &mut Batch<'_>,
    plan: &Plan,
    fetch: &mut dyn FnMut(&AssetRef) -> Result<Vec<u8>>,
    report: &mut ImportReport,
    max_boards: Option<usize>,
) -> Result<()> {
    // Counted here, inside the transaction, so nothing can slip in between check and write.
    if let Some(max) = max_boards
        && !plan.boards.is_empty()
        && lib.boards()?.len() + plan.boards.len() > max
    {
        return Err(
            snipdeck_core::Error::ProRequired(snipdeck_core::Feature::UnlimitedBoards).into(),
        );
    }
    // Only tags that a kept item uses are created; existing tags are reused by name.
    let used: HashSet<usize> = plan
        .boards
        .iter()
        .flat_map(|b| &b.lists)
        .flat_map(|l| &l.items)
        .flat_map(|i| i.tags.iter().copied())
        .collect();
    let mut by_key: HashMap<String, TagId> = lib
        .tags()?
        .into_iter()
        .map(|t| (name_key(t.name.as_str()), t.id))
        .collect();
    let mut tag_ids: HashMap<usize, TagId> = HashMap::new();
    for (index, name) in plan
        .tags
        .iter()
        .enumerate()
        .filter(|(i, _)| used.contains(i))
    {
        let key = name_key(name.as_str());
        let id = if let Some(id) = by_key.get(&key) {
            *id
        } else {
            let tag = Tag::new(name.clone());
            lib.insert_tag(&tag)?;
            by_key.insert(key, tag.id);
            report.tags_added += 1;
            tag.id
        };
        tag_ids.insert(index, id);
    }

    for plan_board in &plan.boards {
        let mut board = Board::new(plan_board.name.clone(), plan_board.created_at);
        board.updated_at = plan_board.updated_at;
        lib.insert_board(&board, Placement::Last)?;
        report.boards_added += 1;
        report.boards.push(board.id);

        for plan_list in &plan_board.lists {
            let mut list = List::new(board.id, plan_list.name.clone(), plan_list.created_at);
            list.updated_at = plan_list.updated_at;
            list.accent = plan_list.accent;
            list.view = plan_list.view;
            list.sort = plan_list.sort;
            lib.insert_list(&list, Placement::Last)?;
            report.lists_added += 1;

            for plan_item in &plan_list.items {
                let (item, formats) = build_item(lib, plan_item, list.id, &tag_ids, fetch)?;
                lib.insert_item(&item, &formats, Placement::Last)?;
                report.items_added += 1;
            }
        }
    }
    Ok(())
}

fn build_item(
    lib: &mut Batch<'_>,
    plan: &PlanItem,
    list: snipdeck_core::ListId,
    tag_ids: &HashMap<usize, TagId>,
    fetch: &mut dyn FnMut(&AssetRef) -> Result<Vec<u8>>,
) -> Result<(Item, Formats)> {
    let mut item = Item::new(list, plan.title.clone(), plan.kind, plan.created_at);
    item.updated_at = plan.updated_at;
    item.pinned = plan.pinned;
    item.sensitive = plan.sensitive;
    item.always_plain = plan.always_plain;
    item.use_count = plan.use_count;
    item.last_used_at = plan.last_used_at;
    item.source_app.clone_from(&plan.source_app);
    item.tags = plan
        .tags
        .iter()
        .filter_map(|index| tag_ids.get(index).copied())
        .collect();

    if let Some(preview) = &plan.preview {
        item.preview = Item::preview_of(preview, plan.sensitive);
    }
    let mut formats = Vec::with_capacity(plan.formats.len());
    for format in &plan.formats {
        match format {
            PlanFormat::Single { kind, source } => {
                let bytes = resolve(source, fetch)?;
                if *kind == FormatKind::Text && plan.preview.is_none() {
                    item.preview = Item::preview_of(preview_text(&bytes), plan.sensitive);
                }
                let content = put(lib, bytes, plan.sensitive)?;
                formats.push(match kind {
                    FormatKind::Text => Format::Text(content),
                    FormatKind::Html => Format::Html(content),
                    FormatKind::Rtf => Format::Rtf(content),
                    FormatKind::Png => Format::Png(content),
                    FormatKind::Files => {
                        return Err(Error::malformed(
                            "format",
                            "`files` is not a single payload",
                        ));
                    }
                });
            }
            PlanFormat::Files(files) => {
                let mut entries = Vec::with_capacity(files.len());
                for file in files {
                    let content = put(lib, resolve(&file.source, fetch)?, plan.sensitive)?;
                    entries.push(FileEntry::new(&file.name, content)?);
                }
                formats.push(Format::Files(entries));
            }
        }
    }
    Ok((item, Formats::new(formats)?))
}

/// Stores a payload of an imported item. Sensitive payloads are held in memory until the item
/// is written (encrypted there when the library has a key): `put` would write the plaintext
/// to the blob folder first, and a rollback would leave it on the disk.
fn put(lib: &mut Batch<'_>, bytes: Vec<u8>, sensitive: bool) -> Result<Content> {
    Ok(if sensitive {
        lib.put_sensitive(bytes)?
    } else {
        lib.put(bytes)?
    })
}

fn resolve(
    source: &Source,
    fetch: &mut dyn FnMut(&AssetRef) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    match source {
        Source::Bytes(bytes) => Ok(bytes.clone()),
        Source::Asset(asset) => fetch(asset),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use snipdeck_core::Inline;
    use snipdeck_store::{Store, Trashable};

    use super::*;

    fn at(s: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + s).unwrap()
    }

    fn name(s: &str) -> Name {
        Name::new(s).unwrap()
    }

    fn asset(n: u8) -> Source {
        Source::Asset(AssetRef {
            hash: BlobRef::from_bytes([n; 32]),
            encrypted_for: None,
        })
    }

    fn item(title: &str, source: Source, tags: &[usize]) -> PlanItem {
        let mut item = PlanItem::text(title.to_owned(), String::new(), at(1));
        item.formats = vec![PlanFormat::Single {
            kind: FormatKind::Text,
            source,
        }];
        item.tags = tags.iter().copied().collect();
        item.content_hash = *blake3::hash(title.as_bytes()).as_bytes();
        item
    }

    fn two_boards() -> Plan {
        let mut list_a = PlanList::new(name("A"), at(1));
        list_a.items = vec![
            item("a1", Source::Bytes(b"one".to_vec()), &[0]),
            item("a2", asset(1), &[0, 1]),
        ];
        let mut list_b = PlanList::new(name("B"), at(1));
        list_b.items = vec![item("b1", asset(2), &[1]), item("b2", asset(3), &[])];
        let board = |n: &str, lists| PlanBoard {
            name: name(n),
            created_at: at(1),
            updated_at: at(1),
            lists,
        };
        Plan {
            tags: vec![name("existing"), name("fresh")],
            boards: vec![board("First", vec![list_a]), board("Second", vec![list_b])],
        }
    }

    fn library() -> Store {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_tag(&Tag::new(name("Existing"))).unwrap();
        let board = Board::new(name("Mine"), at(0));
        s.insert_board(&board, Placement::Last).unwrap();
        let list = List::new(board.id, name("Mine"), at(0));
        s.insert_list(&list, Placement::Last).unwrap();
        let keep = Item::new(list.id, "keep".to_owned(), ItemKind::Text, at(0));
        let content = Content::Inline(Inline::new(b"keep".to_vec()).unwrap());
        s.insert_item(
            &keep,
            &Formats::new(vec![Format::Text(content)]).unwrap(),
            Placement::Last,
        )
        .unwrap();
        let gone = Item::new(list.id, "in the trash".to_owned(), ItemKind::Text, at(0));
        let content = Content::Inline(Inline::new(b"gone".to_vec()).unwrap());
        s.insert_item(
            &gone,
            &Formats::new(vec![Format::Text(content)]).unwrap(),
            Placement::Last,
        )
        .unwrap();
        s.trash(Trashable::Item(gone.id), at(5)).unwrap();
        s
    }

    fn state(s: &Store) -> (usize, usize, usize, usize, Vec<String>) {
        let boards = s.boards().unwrap();
        let lists: usize = boards.iter().map(|b| s.lists(b.id).unwrap().len()).sum();
        let trashed: Vec<String> = s
            .trashed()
            .unwrap()
            .iter()
            .map(|t| format!("{} @ {}", t.label, t.deleted_at))
            .collect();
        (
            boards.len(),
            lists,
            s.all_items().unwrap().len(),
            s.tags().unwrap().len(),
            trashed,
        )
    }

    #[test]
    fn apply_writes_everything_and_reuses_existing_tags() {
        let mut s = library();
        let mut fetch = |_: &AssetRef| Ok(b"big".to_vec());
        let report = apply(
            &mut s,
            &two_boards(),
            &mut fetch,
            ImportReport::default(),
            None,
        )
        .unwrap();
        assert_eq!(
            (
                report.boards_added,
                report.lists_added,
                report.items_added,
                report.tags_added
            ),
            (2, 2, 4, 1)
        );
        let tags = s.tags().unwrap();
        assert_eq!(tags.len(), 2, "`existing` matched `Existing` ignoring case");
        assert_eq!(s.check_integrity().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_failure_at_any_point_leaves_the_library_as_it_was() {
        // Fail on the first, second and third asset fetch: after 0, 1 and 2 items of other
        // boards were already written.
        for fail_at in 0..3 {
            let mut s = library();
            let before = state(&s);
            let mut calls = 0;
            let mut fetch = |_: &AssetRef| {
                calls += 1;
                if calls > fail_at {
                    Err(Error::malformed("test", "injected failure"))
                } else {
                    // Big enough to be a blob file, so blobs are exercised too.
                    Ok(vec![u8::try_from(calls).unwrap(); 100_000])
                }
            };
            let err = apply(
                &mut s,
                &two_boards(),
                &mut fetch,
                ImportReport::default(),
                None,
            )
            .unwrap_err();
            assert!(matches!(err, Error::Malformed { .. }), "{err}");
            assert_eq!(state(&s), before, "failing at fetch {fail_at}");
            assert_eq!(s.check_integrity().unwrap(), Vec::<String>::new());
            // The transaction removed the blob files it had written: nothing for a sweep.
            assert_eq!(s.gc_blobs(Duration::ZERO).unwrap().blobs, 0);
        }
    }

    /// Security review 2, finding 5: names the store treats as one tag are one tag here.
    #[test]
    fn tags_clash_by_the_stores_key_not_by_lower_casing() {
        let mut s = library();
        s.insert_tag(&Tag::new(name("\u{c9}cole"))).unwrap();
        let mut list = PlanList::new(name("L"), at(1));
        list.items = vec![item("t", Source::Bytes(b"t".to_vec()), &[0, 1, 2])];
        let plan = Plan {
            tags: vec![name("Straße"), name("STRASSE"), name("e\u{301}cole")],
            boards: vec![PlanBoard {
                name: name("B"),
                created_at: at(1),
                updated_at: at(1),
                lists: vec![list],
            }],
        };
        let mut fetch = |_: &AssetRef| Ok(Vec::new());
        let report = apply(&mut s, &plan, &mut fetch, ImportReport::default(), None).unwrap();
        assert_eq!(report.tags_added, 1);
        assert_eq!(s.tags().unwrap().len(), 3);
        assert_eq!(s.check_integrity().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn rolling_back_never_touches_what_the_user_trashed() {
        let mut s = library();
        let before = state(&s);
        assert_eq!(before.4.len(), 1);
        let mut fetch = |_: &AssetRef| Err(Error::malformed("test", "boom"));
        apply(
            &mut s,
            &two_boards(),
            &mut fetch,
            ImportReport::default(),
            None,
        )
        .unwrap_err();
        assert_eq!(state(&s).4, before.4);
    }

    #[test]
    fn duplicates_are_dropped_by_content_and_empty_boards_vanish() {
        let mut plan = two_boards();
        let hash = plan.boards[0].lists[0].items[0].content_hash;
        plan.boards[1].lists[0].items[0].content_hash = hash;
        let mut known = HashSet::new();
        assert_eq!(plan.drop_duplicates(&mut known), 1, "b1 repeats a1");

        let mut again = two_boards();
        let mut everything: HashSet<[u8; 32]> = again
            .boards
            .iter()
            .flat_map(|b| &b.lists)
            .flat_map(|l| &l.items)
            .map(|i| i.content_hash)
            .collect();
        let items = again.item_count();
        assert!(items > 0);
        let dropped = again.drop_duplicates(&mut everything);
        assert_eq!(dropped, items);
        assert!(again.boards.is_empty());
    }

    #[test]
    fn content_hash_ignores_titles_and_distinguishes_format_and_order() {
        let h = |kind: FormatKind, data: &[u8]| {
            let mut hasher = ContentHasher::new();
            hasher.single(kind, blake3::hash(data).as_bytes(), data.len() as u64);
            hasher.finish()
        };
        assert_eq!(h(FormatKind::Text, b"x"), h(FormatKind::Text, b"x"));
        assert_ne!(h(FormatKind::Text, b"x"), h(FormatKind::Html, b"x"));
        assert_ne!(h(FormatKind::Text, b"x"), h(FormatKind::Text, b"y"));
        let mut ab = ContentHasher::new();
        ab.single(FormatKind::Text, &[1; 32], 1);
        ab.single(FormatKind::Html, &[2; 32], 1);
        let mut ba = ContentHasher::new();
        ba.single(FormatKind::Html, &[2; 32], 1);
        ba.single(FormatKind::Text, &[1; 32], 1);
        assert_ne!(ab.finish(), ba.finish(), "preference order matters");
    }

    #[test]
    fn the_stored_hash_of_an_existing_item_matches_the_plan_hash() {
        let mut s = Store::open_in_memory().unwrap();
        let board = Board::new(name("B"), at(0));
        s.insert_board(&board, Placement::Last).unwrap();
        let list = List::new(board.id, name("L"), at(0));
        s.insert_list(&list, Placement::Last).unwrap();
        let big = "x".repeat(100_000);
        for body in ["small", big.as_str()] {
            let it = Item::new(list.id, "t".to_owned(), ItemKind::Text, at(0));
            let content = s.put(body.as_bytes().to_vec()).unwrap();
            s.insert_item(
                &it,
                &Formats::new(vec![Format::Text(content)]).unwrap(),
                Placement::Last,
            )
            .unwrap();
        }
        let known = existing_hashes(&s).unwrap();
        for body in ["small", big.as_str()] {
            let plan = PlanItem::text("other title".to_owned(), body.to_owned(), at(1));
            assert!(known.contains(&plan.content_hash), "{} bytes", body.len());
        }
    }
}
