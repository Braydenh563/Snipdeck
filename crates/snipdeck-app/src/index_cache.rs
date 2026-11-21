//! The search index, built once per library change instead of once per load (SPEC §9).
//!
//! The Panel and the Picker both search the whole library. Building the index reads every
//! tag and indexes every item, which is work that only has to be repeated when something was
//! written. [`IndexCache`] keeps the last index together with the store's
//! [`change_counter`](snipdeck_store::ItemRepo::change_counter) it was built at, and hands the
//! same [`Rc`] out again while the counter is unchanged. Any write (an edit, a tag renamed, an
//! item trashed or restored, an import) moves the counter (a counted use and the Recent inbox
//! do not, so a paste or a copy does not cost a rebuild), so the next load
//! builds a fresh index. Per-board lookups stay cheap without a per-board index: the shared
//! index covers the library, and a board's results are the hits whose lists belong to it.
//!
//! The same snapshot also holds the items and the boards with their lists, so a load that
//! finds nothing written reads nothing but the shown list.
//!
//! The cache is a cheap handle (clones share one state), so the controller owns one and hands
//! a clone to each model it loads.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::search_suggest::Index;
use snipdeck_core::search::{Names, SearchIndex};
use snipdeck_core::{Board, Item, ItemId, List, ListId, TagId};
use snipdeck_store::{
    BoardRepo, ItemRepo, ListRepo, RecentEntry, RecentRepo, TagRepo, TriggerRepo,
};

/// The names, besides its own fields, that the search index matches an item by: the names of
/// its tags, the name of its list and its trigger. Every surface builds its index with these,
/// so a query finds the same items in the Panel, the Picker and Main.
#[derive(Debug, Default)]
pub(crate) struct SearchNames {
    /// Each tag's own path (`work/invoices`) and its ancestors' paths, so `tag:work` finds an
    /// item carrying a tag nested under `work`.
    tags: HashMap<TagId, Vec<String>>,
    lists: HashMap<ListId, String>,
    triggers: HashMap<ItemId, String>,
}

impl SearchNames {
    /// Reads the tag names and triggers; `lists` are the lists the items can be in.
    pub(crate) fn read<'a, S: TagRepo + TriggerRepo>(
        store: &S,
        lists: impl IntoIterator<Item = &'a List>,
    ) -> snipdeck_store::Result<Self> {
        let tags = store.tags()?;
        let tree = snipdeck_core::TagTree::new(&tags);
        Ok(Self {
            tags: tags
                .iter()
                .map(|t| (t.id, tree.search_paths(t.id)))
                .collect(),
            lists: lists
                .into_iter()
                .map(|l| (l.id, l.name.as_str().to_owned()))
                .collect(),
            triggers: store
                .triggers()?
                .into_iter()
                .map(|(trigger, item)| (item, trigger.as_str().to_owned()))
                .collect(),
        })
    }

    /// Adds `item` to `index`, or replaces its entry, with its names.
    pub(crate) fn upsert(&self, index: &mut SearchIndex, item: &Item) {
        let tags: Vec<&str> = item
            .tags
            .iter()
            .filter_map(|t| self.tags.get(t))
            .flatten()
            .map(String::as_str)
            .collect();
        index.upsert(
            item,
            Names {
                tags: &tags,
                list: self.lists.get(&item.list_id).map_or("", String::as_str),
                trigger: self.triggers.get(&item.id).map(String::as_str),
            },
        );
    }

    /// An index over `items`, in their order.
    pub(crate) fn index<'a>(&self, items: impl IntoIterator<Item = &'a Item>) -> SearchIndex {
        let mut index = SearchIndex::default();
        for item in items {
            self.upsert(&mut index, item);
        }
        index
    }
}

/// What the Panel and the Picker read from the library on every load, kept together: every
/// live item, every board with its lists, and the search index over the items.
#[derive(Debug, Default)]
pub struct Snapshot {
    /// Every live item, in library order.
    pub all: Vec<Item>,
    /// Every board with its lists, in order.
    pub boards: Vec<(Board, Vec<List>)>,
    /// The search index over [`Snapshot::all`].
    pub index: Rc<SearchIndex>,
    /// The names the search suggestions are made of, built with the index.
    pub suggest: Rc<Index>,
    /// The items that keep HTML or RTF next to their text (the Panel's "Formatted" mark).
    /// Sensitive items are left out: their content is never read for a list.
    pub rich: Rc<HashSet<ItemId>>,
}

/// A change the controller announced, so the next snapshot patches those items alone.
#[derive(Debug)]
enum Hint {
    /// Counted uses: only the usage counter moved, by one per item.
    Used(Vec<ItemId>),
    /// One item written through [`IndexCache::edit_item`]; the counters before and after.
    Edited {
        id: ItemId,
        from: (u64, u64),
        to: (u64, u64),
    },
}

#[derive(Debug, Default)]
struct State {
    /// The `(change_counter, usage_counter)` the snapshot was read at; `None` before the
    /// first read.
    read_at: Option<(u64, u64)>,
    /// The `change_counter` the index was built at.
    built_at: Option<u64>,
    suggest: Rc<Index>,
    snapshot: Rc<Snapshot>,
    rebuilds: usize,
    /// How many times one item was patched into the snapshot instead of a re-read.
    patches: usize,
    /// What the controller said it just changed (see [`Hint`]); trusted only when the store's
    /// counters are exactly the ones that change implies.
    hint: Option<Hint>,
    /// The decrypted Recent entries and the `recent_revision` they were read at.
    recent: Option<(u64, Rc<Vec<RecentEntry>>)>,
}

/// A shared handle on the last-read library snapshot.
#[derive(Debug, Clone, Default)]
pub struct IndexCache(Rc<RefCell<State>>);

impl IndexCache {
    /// An empty cache: the first [`IndexCache::snapshot`] reads and builds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The library snapshot, reused when the store has not been written since it was read.
    ///
    /// A counted use changes `use_count` and `last_used_at`, which the index keeps only as
    /// tie-breakers: it re-reads the items and keeps the index, refreshing those two.
    ///
    /// # Errors
    ///
    /// Any store error while reading.
    pub fn snapshot<S>(&self, store: &S) -> snipdeck_store::Result<Rc<Snapshot>>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo + TriggerRepo,
    {
        let counter = store.change_counter();
        let at = (counter, store.usage_counter());
        {
            let state = self.0.borrow();
            if state.read_at == Some(at) {
                return Ok(Rc::clone(&state.snapshot));
            }
        }
        if let Some(patched) = self.patched(store, at) {
            return Ok(patched);
        }
        let all = store.all_items()?;
        // A counted use moves only the usage counter: the boards and lists are what they were.
        let kept_boards = {
            let state = self.0.borrow();
            (state.built_at == Some(counter)).then(|| state.snapshot.boards.clone())
        };
        let boards = if let Some(boards) = kept_boards {
            boards
        } else {
            let mut boards = Vec::new();
            for b in store.boards()? {
                let lists = store.lists(b.id)?;
                boards.push((b, lists));
            }
            boards
        };
        let reuse = {
            let state = self.0.borrow();
            (state.built_at == Some(counter)).then(|| Rc::clone(&state.snapshot.index))
        };
        let rich = {
            let state = self.0.borrow();
            match (state.built_at == Some(counter)).then(|| Rc::clone(&state.snapshot.rich)) {
                Some(rich) => rich,
                None => Rc::new(store.rich_item_ids()?.into_iter().collect()),
            }
        };
        let (index, suggest, built) = if let Some(mut index) = reuse {
            // A counted use changes only the tie-breakers (use count, last use): bring them up
            // to date, copying the index only when one did change.
            if !index.usage_is_current(&all) {
                Rc::make_mut(&mut index).refresh_usage(&all);
            }
            (index, Rc::clone(&self.0.borrow().suggest), false)
        } else {
            let started = std::time::Instant::now();
            let tags = store.tags()?;
            let names = SearchNames::read(store, boards.iter().flat_map(|(_, lists)| lists))?;
            let index = names.index(&all);
            let suggest = Index::build(
                &all,
                tags.iter().map(|t| t.name.as_str()),
                boards
                    .iter()
                    .flat_map(|(_, lists)| lists.iter().map(|l| l.name.as_str())),
            );
            crate::trace!(
                "index build {} us for {} items",
                started.elapsed().as_micros(),
                all.len()
            );
            (Rc::new(index), Rc::new(suggest), true)
        };
        let mut state = self.0.borrow_mut();
        state.suggest = Rc::clone(&suggest);
        state.snapshot = Rc::new(Snapshot {
            all,
            boards,
            index,
            suggest,
            rich,
        });
        state.read_at = Some(at);
        state.built_at = Some(counter);
        state.rebuilds += usize::from(built);
        Ok(Rc::clone(&state.snapshot))
    }

    /// Says that item `id` was just used (counted). If nothing else is written before the next
    /// [`IndexCache::snapshot`], that snapshot patches this item's two usage fields instead of
    /// re-reading the library.
    pub fn note_use(&self, id: ItemId) {
        let mut state = self.0.borrow_mut();
        match &mut state.hint {
            Some(Hint::Used(ids)) => ids.push(id),
            hint => *hint = Some(Hint::Used(vec![id])),
        }
    }

    /// Runs `write`, a write that changes the one item `id` (an edit, a pin, a move), and says
    /// so: if nothing else is written before the next [`IndexCache::snapshot`], that snapshot
    /// reads that item alone and patches it into the index instead of re-reading the library.
    /// The write must not change other items (a new tag is fine).
    pub fn edit_item<S: ItemRepo, R>(
        &self,
        store: &mut S,
        id: ItemId,
        write: impl FnOnce(&mut S) -> R,
    ) -> R {
        let from = (store.change_counter(), store.usage_counter());
        let out = write(store);
        let to = (store.change_counter(), store.usage_counter());
        self.0.borrow_mut().hint = Some(Hint::Edited { id, from, to });
        out
    }

    /// The snapshot with the hinted items patched in, or `None` when there is no hint, the
    /// store moved in a way the hint does not explain, or an item is not in the snapshot (the
    /// caller then reads everything).
    fn patched<S>(&self, store: &S, at: (u64, u64)) -> Option<Rc<Snapshot>>
    where
        S: ItemRepo + TagRepo + TriggerRepo,
    {
        let (hint, read_at, built_at, old) = {
            let mut state = self.0.borrow_mut();
            (
                state.hint.take(),
                state.read_at,
                state.built_at,
                Rc::clone(&state.snapshot),
            )
        };
        let (read_changes, read_uses) = read_at?;
        if built_at != Some(read_changes) {
            return None;
        }
        let (ids, edited) = match hint? {
            Hint::Used(ids) if at.0 == read_changes && at.1 == read_uses + ids.len() as u64 => {
                (ids, false)
            }
            Hint::Edited { id, from, to } if from == (read_changes, read_uses) && to == at => {
                (vec![id], true)
            }
            _ => return None,
        };
        let mut all = old.all.clone();
        let mut index = Rc::clone(&old.index);
        let mut tags = Vec::new();
        let mut names = SearchNames::default();
        if edited {
            tags = store.tags().ok()?;
            names =
                SearchNames::read(store, old.boards.iter().flat_map(|(_, lists)| lists)).ok()?;
        }
        for id in ids {
            let position = all.iter().position(|i| i.id == id)?;
            let item = store.item(id).ok()?;
            if edited {
                names.upsert(Rc::make_mut(&mut index), &item);
            } else if !index.usage_is_current(std::iter::once(&item)) {
                Rc::make_mut(&mut index).refresh_usage(std::iter::once(&item));
            }
            all[position] = item;
        }
        let (suggest, rich) = if edited {
            let rich: HashSet<ItemId> = store.rich_item_ids().ok()?.into_iter().collect();
            let suggest = Index::build(
                &all,
                tags.iter().map(|t| t.name.as_str()),
                old.boards
                    .iter()
                    .flat_map(|(_, lists)| lists.iter().map(|l| l.name.as_str())),
            );
            (Rc::new(suggest), Rc::new(rich))
        } else {
            (Rc::clone(&old.suggest), Rc::clone(&old.rich))
        };
        let mut state = self.0.borrow_mut();
        state.suggest = Rc::clone(&suggest);
        state.snapshot = Rc::new(Snapshot {
            all,
            boards: old.boards.clone(),
            index,
            suggest,
            rich,
        });
        state.read_at = Some(at);
        state.built_at = Some(at.0);
        state.patches += 1;
        Some(Rc::clone(&state.snapshot))
    }

    /// The Recent entries (newest first, decrypted), read again only when the store has been
    /// written since. A store error reads as no entries and is not kept.
    pub fn recent<S: RecentRepo>(&self, store: &S) -> Rc<Vec<RecentEntry>> {
        let revision = store.recent_revision();
        if let Some((at, entries)) = &self.0.borrow().recent
            && *at == revision
        {
            return Rc::clone(entries);
        }
        let Ok(entries) = store.recent() else {
            return Rc::default();
        };
        let entries = Rc::new(entries);
        self.0.borrow_mut().recent = Some((revision, Rc::clone(&entries)));
        entries
    }

    /// Drops the decrypted Recent entries held (the library locked, or the inbox switched off).
    pub fn forget_recent(&self) {
        self.0.borrow_mut().recent = None;
    }

    /// Forgets everything read, so the next [`IndexCache::snapshot`] reads and builds again
    /// (after the library was erased).
    pub fn reset(&self) {
        let mut state = self.0.borrow_mut();
        state.read_at = None;
        state.built_at = None;
        state.snapshot = Rc::default();
        state.hint = None;
        state.recent = None;
    }

    /// How many times one item was patched into the snapshot (for tests).
    #[must_use]
    pub fn patches(&self) -> usize {
        self.0.borrow().patches
    }

    /// How many times the index was built (for tests and the performance notes).
    #[must_use]
    pub fn rebuilds(&self) -> usize {
        self.0.borrow().rebuilds
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "a failed unwrap is the test failing")]

    use super::*;
    use snipdeck_core::{
        Board, Content, Format, Formats, Inline, ItemKind, List, Name, Placement, Tag, Timestamp,
    };
    use snipdeck_store::{BoardRepo, ListRepo, RecentRepo, Store};

    fn at() -> Timestamp {
        Timestamp::from_second(1_800_000_000).unwrap()
    }

    fn seeded() -> (Store, snipdeck_core::ListId) {
        let mut s = Store::open_in_memory().unwrap();
        let b = Board::new(Name::new("B").unwrap(), at());
        s.insert_board(&b, Placement::Last).unwrap();
        let l = List::new(b.id, Name::new("L").unwrap(), at());
        s.insert_list(&l, Placement::Last).unwrap();
        (s, l.id)
    }

    fn add(s: &mut Store, list: snipdeck_core::ListId, title: &str) {
        let mut item = Item::new(list, title.to_owned(), ItemKind::Text, at());
        item.preview = Item::preview_of("body", false);
        let body = Content::Inline(Inline::new(b"body".to_vec()).unwrap());
        let formats = Formats::new(vec![Format::Text(body)]).unwrap();
        s.insert_item(&item, &formats, Placement::Last).unwrap();
    }

    #[test]
    fn an_unchanged_library_reuses_the_snapshot() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let first = cache.snapshot(&s).unwrap();
        let second = cache.snapshot(&s).unwrap();
        assert!(Rc::ptr_eq(&first, &second), "nothing was read again");
        assert_eq!(cache.rebuilds(), 1);
        // Clones share the state.
        let third = cache.clone().snapshot(&s).unwrap();
        assert!(Rc::ptr_eq(&first, &third));
        assert_eq!(cache.rebuilds(), 1);
        assert_eq!(first.all.len(), 1);
        assert_eq!(first.boards.len(), 1);
        assert_eq!(first.boards[0].1.len(), 1);
    }

    // A25 smoke check: a 2000-item library is read and indexed once, and quickly. It uses the real
    // clock on purpose: the claim is about elapsed time (the index build is not deferred because it
    // stays well under 50 ms), so a fixed clock would measure nothing. The bound is a generous 2 s
    // so a slow or loaded CI machine does not flake; a real regression is orders of magnitude.
    #[test]
    fn building_the_index_of_two_thousand_items_is_cheap() {
        let (mut s, l) = seeded();
        for n in 0..2000 {
            add(&mut s, l, &format!("snippet number {n}"));
        }
        let cache = IndexCache::new();
        let started = std::time::Instant::now();
        let snapshot = cache.snapshot(&s).unwrap();
        let took = started.elapsed();
        assert_eq!(snapshot.index.len(), 2000);
        assert_eq!(cache.rebuilds(), 1);
        assert!(took < std::time::Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn ten_thousand_items_with_rich_text_snapshot_without_reading_their_content() {
        let (mut s, l) = seeded();
        for n in 0..10_000 {
            let item = Item::new(l, format!("item {n}"), ItemKind::Text, at());
            let mut formats = vec![Format::Text(Content::Inline(
                Inline::new(b"body".to_vec()).unwrap(),
            ))];
            if n % 100 == 0 {
                formats.push(Format::Html(Content::Inline(
                    Inline::new(b"<b>x</b>".to_vec()).unwrap(),
                )));
            }
            s.insert_item(&item, &Formats::new(formats).unwrap(), Placement::Last)
                .unwrap();
        }
        let cache = IndexCache::new();
        let started = std::time::Instant::now();
        let snapshot = cache.snapshot(&s).unwrap();
        let took = started.elapsed();
        assert_eq!(snapshot.index.len(), 10_000);
        assert_eq!(
            snapshot.rich.len(),
            100,
            "only the items with an HTML or RTF format"
        );
        // Generous: a debug build on a loaded machine. The old path read every format's bytes.
        assert!(took < std::time::Duration::from_secs(20), "{took:?}");

        // A use and a single-item edit patch that item; neither rebuilds the index or re-reads
        // the library (counted, not timed).
        let id = snapshot.all[5_000].id;
        s.record_use(id, at()).unwrap();
        cache.note_use(id);
        let used = cache.snapshot(&s).unwrap();
        assert_eq!(used.all[5_000].use_count, 1);
        let mut item = s.item(id).unwrap();
        item.title = "renamed".to_owned();
        cache
            .edit_item(&mut s, id, |s| s.update_item(&item, None))
            .unwrap();
        let edited = cache.snapshot(&s).unwrap();
        assert_eq!(edited.all[5_000].title, "renamed");
        assert_eq!(edited.index.len(), 10_000);
        assert_eq!((cache.rebuilds(), cache.patches()), (1, 2));
    }

    #[test]
    fn a_write_builds_a_new_index() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let before = cache.snapshot(&s).unwrap();
        assert_eq!(before.index.len(), 1);
        add(&mut s, l, "beta");
        let after = cache.snapshot(&s).unwrap();
        assert_eq!(after.index.len(), 2);
        assert_eq!(cache.rebuilds(), 2);
        // A tag write alone also invalidates (tag names are searchable).
        s.insert_tag(&Tag::new(Name::new("t").unwrap())).unwrap();
        cache.snapshot(&s).unwrap();
        assert_eq!(cache.rebuilds(), 3);
    }

    #[test]
    fn a_counted_use_and_a_system_copy_do_not_rebuild_the_index_but_a_use_rereads_the_items() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let first = cache.snapshot(&s).unwrap();
        let id = first.all[0].id;
        assert_eq!(first.all[0].use_count, 0);

        s.add_recent(snipdeck_store::RecentContent::text("copied"), None, at())
            .unwrap();
        let after_copy = cache.snapshot(&s).unwrap();
        assert!(Rc::ptr_eq(&first, &after_copy), "a copy reads nothing");

        s.record_use(id, at()).unwrap();
        let after_use = cache.snapshot(&s).unwrap();
        assert_eq!(after_use.all[0].use_count, 1, "the use shows");
        // Nothing searched changed, so the index is not rebuilt; its tie-breakers (use count and
        // last use) are brought up to date, and the snapshot the Panel still holds keeps its own.
        assert_eq!(cache.rebuilds(), 1);
        assert!(after_use.index.usage_is_current(&after_use.all));
        assert!(!first.index.usage_is_current(&after_use.all));
    }

    #[test]
    fn a_hinted_edit_patches_one_item_and_an_unexplained_write_rebuilds() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        add(&mut s, l, "beta");
        let cache = IndexCache::new();
        let first = cache.snapshot(&s).unwrap();
        let id = first.all[1].id;
        let mut item = s.item(id).unwrap();
        item.title = "gamma".to_owned();
        cache
            .edit_item(&mut s, id, |s| s.update_item(&item, None))
            .unwrap();
        let after = cache.snapshot(&s).unwrap();
        assert_eq!((cache.rebuilds(), cache.patches()), (1, 1));
        assert_eq!(after.all[1].title, "gamma");
        assert_eq!(after.index.len(), 2);
        assert_eq!(
            first.all[1].title, "beta",
            "the earlier snapshot is untouched"
        );

        // A hint plus a write nobody announced is not trusted: everything is read again.
        s.record_use(id, at()).unwrap();
        cache.note_use(id);
        add(&mut s, l, "delta");
        let full = cache.snapshot(&s).unwrap();
        assert_eq!(full.index.len(), 3);
        assert_eq!((cache.rebuilds(), cache.patches()), (2, 1));
    }

    /// A store that counts how often the Recent list is read.
    struct Counting {
        inner: Store,
        reads: std::cell::Cell<usize>,
    }

    impl RecentRepo for Counting {
        fn add_recent(
            &mut self,
            content: snipdeck_store::RecentContent<'_>,
            app: Option<&str>,
            now: Timestamp,
        ) -> snipdeck_store::Result<i64> {
            self.inner.add_recent(content, app, now)
        }
        fn recent_seals_rich(&self) -> bool {
            self.inner.recent_seals_rich()
        }
        fn recent_payload(
            &self,
            id: i64,
        ) -> snipdeck_store::Result<Option<snipdeck_store::RecentPayload>> {
            self.inner.recent_payload(id)
        }
        fn prune_recent_blobs(&mut self, budget: u64) -> snipdeck_store::Result<usize> {
            self.inner.prune_recent_blobs(budget)
        }
        fn recent(&self) -> snipdeck_store::Result<Vec<RecentEntry>> {
            self.reads.set(self.reads.get() + 1);
            self.inner.recent()
        }
        fn recent_revision(&self) -> u64 {
            self.inner.recent_revision()
        }
        fn delete_recent(&mut self, id: i64) -> snipdeck_store::Result<()> {
            self.inner.delete_recent(id)
        }
        fn clear_recent(&mut self) -> snipdeck_store::Result<usize> {
            self.inner.clear_recent()
        }
        fn prune_recent(
            &mut self,
            now: Timestamp,
            max_entries: usize,
            max_age: std::time::Duration,
        ) -> snipdeck_store::Result<usize> {
            self.inner.prune_recent(now, max_entries, max_age)
        }
    }

    #[test]
    fn the_recent_entries_are_read_once_until_the_store_is_written() {
        let mut s = Counting {
            inner: Store::open_in_memory().unwrap(),
            reads: std::cell::Cell::new(0),
        };
        s.add_recent(snipdeck_store::RecentContent::text("one"), None, at())
            .unwrap();
        let cache = IndexCache::new();
        assert_eq!(cache.recent(&s).len(), 1);
        assert_eq!(cache.recent(&s).len(), 1);
        assert_eq!(s.reads.get(), 1, "the second call reads nothing");
        s.add_recent(snipdeck_store::RecentContent::text("two"), None, at())
            .unwrap();
        assert_eq!(cache.recent(&s).len(), 2);
        assert_eq!(s.reads.get(), 2);
        cache.forget_recent();
        cache.recent(&s);
        assert_eq!(s.reads.get(), 3, "forgotten entries are read again");
    }

    #[test]
    fn reset_forgets_the_snapshot() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        cache.snapshot(&s).unwrap();
        cache.reset();
        cache.snapshot(&s).unwrap();
        assert_eq!(cache.rebuilds(), 2);
    }

    #[test]
    fn panel_and_picker_loads_share_one_build_until_a_write() {
        use crate::{panel::PanelModel, picker::PickerModel};
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let mut panel = PanelModel::load_cached(&s, None, None, &cache).unwrap();
        assert_eq!(cache.rebuilds(), 1);
        panel.refresh(&s).unwrap();
        PickerModel::load_cached(&s, panel.board(), &cache).unwrap();
        let list = panel.list().map(|l| l.id).unwrap();
        panel.select_list(&s, list).unwrap();
        assert_eq!(cache.rebuilds(), 1, "reads must not rebuild");
        let held = cache.snapshot(&s).unwrap();
        PanelModel::load_cached(&s, None, None, &cache).unwrap();
        PickerModel::load_cached(&s, panel.board(), &cache).unwrap();
        assert!(
            Rc::ptr_eq(&held, &cache.snapshot(&s).unwrap()),
            "loads with no write read no items, boards or lists again"
        );

        add(&mut s, l, "beta");
        panel.refresh(&s).unwrap();
        assert_eq!(cache.rebuilds(), 2);
        PickerModel::load_cached(&s, panel.board(), &cache).unwrap();
        assert_eq!(cache.rebuilds(), 2, "the Picker reuses the Panel's rebuild");
    }
}
