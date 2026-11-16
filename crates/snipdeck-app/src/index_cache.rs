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

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::search_suggest::Index;
use snipdeck_core::{Board, Item, ItemId, List, Tag, search::SearchIndex};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, RecentEntry, RecentRepo, TagRepo};

/// What the Panel and the Picker read from the library on every load, kept together: every
/// live item, every board with its lists, and the search index over the items.
#[derive(Debug, Default)]
pub struct Snapshot {
    /// Every live item, in library order. Shared: a patch replaces the one item written and
    /// copies the other pointers, never the items.
    pub all: Vec<Rc<Item>>,
    /// Where each item sits in [`Snapshot::all`] (the order never changes by a patch).
    positions: Rc<HashMap<ItemId, usize>>,
    /// The pinned items, in library order, so the pinned strip reads no other item.
    pub pinned: Rc<Vec<Rc<Item>>>,
    /// Every board with its lists, in order.
    pub boards: Vec<(Board, Vec<List>)>,
    /// Every tag, for the names the index and the suggestions are made of.
    tags: Rc<Vec<Tag>>,
    /// The search index over [`Snapshot::all`] and the names the suggestions are made of: built
    /// on first use (the first search keystroke or suggestion), not when the library is read,
    /// so the first frame waits for neither.
    search: OnceCell<Search>,
    /// Counts the builds of `search` (shared with the cache, for tests and the notes).
    builds: Rc<Cell<usize>>,
    /// The items that keep HTML or RTF next to their text (the Panel's "Formatted" mark).
    /// Sensitive items are left out: their content is never read for a list.
    pub rich: Rc<HashSet<ItemId>>,
}

/// What a search needs: the index over the items, and the names the suggestions are made of.
#[derive(Debug)]
struct Search {
    index: Rc<SearchIndex>,
    suggest: Rc<Index>,
}

impl Snapshot {
    fn search(&self) -> &Search {
        self.search.get_or_init(|| {
            let started = std::time::Instant::now();
            let tag_names: HashMap<_, _> = self
                .tags
                .iter()
                .map(|t| (t.id, t.name.as_str().to_owned()))
                .collect();
            let mut index = SearchIndex::default();
            for item in &self.all {
                index.upsert_with_tags(
                    item,
                    item.tags
                        .iter()
                        .filter_map(|t| tag_names.get(t).map(String::as_str)),
                );
            }
            let suggest = Index::build(
                self.all.iter().map(|i| &**i),
                self.tags.iter().map(|t| t.name.as_str()),
                self.boards
                    .iter()
                    .flat_map(|(_, lists)| lists.iter().map(|l| l.name.as_str())),
            );
            self.builds.set(self.builds.get() + 1);
            crate::trace!(
                "index build {} us for {} items",
                started.elapsed().as_micros(),
                self.all.len()
            );
            Search {
                index: Rc::new(index),
                suggest: Rc::new(suggest),
            }
        })
    }

    /// The search index over every item, built on first use.
    #[must_use]
    pub fn index(&self) -> Rc<SearchIndex> {
        Rc::clone(&self.search().index)
    }

    /// The names the search suggestions are made of, built with the index on first use.
    #[must_use]
    pub fn suggest(&self) -> Rc<Index> {
        Rc::clone(&self.search().suggest)
    }

    /// Item `id`, by one hash lookup.
    #[must_use]
    pub fn item(&self, id: ItemId) -> Option<&Rc<Item>> {
        self.positions.get(&id).and_then(|&at| self.all.get(at))
    }
}

fn positions_of(all: &[Rc<Item>]) -> HashMap<ItemId, usize> {
    all.iter().enumerate().map(|(at, i)| (i.id, at)).collect()
}

fn pinned_of(all: &[Rc<Item>]) -> Vec<Rc<Item>> {
    all.iter().filter(|i| i.pinned).cloned().collect()
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
        /// Whether the title, tags or formats may have changed (an editor save). A pin, a
        /// move or a colour change leave them alone, so the suggestions and the rich set
        /// are kept.
        content: bool,
    },
}

#[derive(Debug, Default)]
struct State {
    /// The `(change_counter, usage_counter)` the snapshot was read at; `None` before the
    /// first read.
    read_at: Option<(u64, u64)>,
    /// The `change_counter` the index was built at.
    built_at: Option<u64>,
    snapshot: Rc<Snapshot>,
    rebuilds: usize,
    /// Builds of the search index, shared with every snapshot.
    builds: Rc<Cell<usize>>,
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
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
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
        let all: Vec<Rc<Item>> = store.all_items()?.into_iter().map(Rc::new).collect();
        // A counted use moves only the usage counter: the boards, lists, tags and rich set are
        // what they were.
        let kept = {
            let state = self.0.borrow();
            (state.built_at == Some(counter)).then(|| Rc::clone(&state.snapshot))
        };
        let (boards, tags, rich) = if let Some(old) = &kept {
            (
                old.boards.clone(),
                Rc::clone(&old.tags),
                Rc::clone(&old.rich),
            )
        } else {
            let mut boards = Vec::new();
            for b in store.boards()? {
                let lists = store.lists(b.id)?;
                boards.push((b, lists));
            }
            (
                boards,
                Rc::new(store.tags()?),
                Rc::new(store.rich_item_ids()?.into_iter().collect()),
            )
        };
        // The index is kept when it was built: a counted use changes only the tie-breakers (use
        // count, last use), brought up to date here, copying the index only when one changed.
        let search = OnceCell::new();
        if let Some(prev) = kept.as_ref().and_then(|old| old.search.get()) {
            let mut index = Rc::clone(&prev.index);
            if !index.usage_is_current(all.iter().map(|i| &**i)) {
                Rc::make_mut(&mut index).refresh_usage(all.iter().map(|i| &**i));
            }
            let _ = search.set(Search {
                index,
                suggest: Rc::clone(&prev.suggest),
            });
        }
        let mut state = self.0.borrow_mut();
        state.snapshot = Rc::new(Snapshot {
            positions: Rc::new(positions_of(&all)),
            pinned: Rc::new(pinned_of(&all)),
            all,
            boards,
            tags,
            search,
            builds: Rc::clone(&state.builds),
            rich,
        });
        state.read_at = Some(at);
        state.built_at = Some(counter);
        state.rebuilds += usize::from(kept.is_none());
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
        self.edit_one(store, id, true, write)
    }

    /// Like [`IndexCache::edit_item`] for a write that leaves the item's title, tags and
    /// formats alone (a pin, a move, a colour): the suggestions and the rich set are kept.
    pub fn edit_item_meta<S: ItemRepo, R>(
        &self,
        store: &mut S,
        id: ItemId,
        write: impl FnOnce(&mut S) -> R,
    ) -> R {
        self.edit_one(store, id, false, write)
    }

    fn edit_one<S: ItemRepo, R>(
        &self,
        store: &mut S,
        id: ItemId,
        content: bool,
        write: impl FnOnce(&mut S) -> R,
    ) -> R {
        let from = (store.change_counter(), store.usage_counter());
        let out = write(store);
        let to = (store.change_counter(), store.usage_counter());
        self.0.borrow_mut().hint = Some(Hint::Edited {
            id,
            from,
            to,
            content,
        });
        out
    }

    /// The snapshot with the hinted items patched in, or `None` when there is no hint, the
    /// store moved in a way the hint does not explain, or an item is not in the snapshot (the
    /// caller then reads everything).
    fn patched<S>(&self, store: &S, at: (u64, u64)) -> Option<Rc<Snapshot>>
    where
        S: ItemRepo + TagRepo,
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
        let (ids, edited, content) = match hint? {
            Hint::Used(ids) if at.0 == read_changes && at.1 == read_uses + ids.len() as u64 => {
                (ids, false, false)
            }
            Hint::Edited {
                id,
                from,
                to,
                content,
            } if from == (read_changes, read_uses) && to == at => (vec![id], true, content),
            _ => return None,
        };
        // Pointers only: the items themselves are shared with the snapshot before this one.
        let mut all = old.all.clone();
        // Nothing is patched into an index nobody has built yet: it is built from these items.
        let mut search = old
            .search
            .get()
            .map(|s| (Rc::clone(&s.index), Rc::clone(&s.suggest)));
        let tags = if edited {
            Rc::new(store.tags().ok()?)
        } else {
            Rc::clone(&old.tags)
        };
        let mut names_changed = false;
        let mut pin_touched = false;
        for id in ids {
            let position = *old.positions.get(&id)?;
            let item = store.item(id).ok()?;
            let before = &old.all[position];
            names_changed |= before.title != item.title
                || before.tags != item.tags
                || before.sensitive != item.sensitive;
            pin_touched |= before.pinned || item.pinned;
            if let Some((index, _)) = search.as_mut() {
                if edited {
                    Rc::make_mut(index).upsert_with_tags(
                        &item,
                        item.tags.iter().filter_map(|t| {
                            tags.iter().find(|x| x.id == *t).map(|x| x.name.as_str())
                        }),
                    );
                } else if !index.usage_is_current(std::iter::once(&item)) {
                    Rc::make_mut(index).refresh_usage(std::iter::once(&item));
                }
            }
            all[position] = Rc::new(item);
        }
        let rich = if content {
            let rich: HashSet<ItemId> = store.rich_item_ids().ok()?.into_iter().collect();
            Rc::new(rich)
        } else {
            Rc::clone(&old.rich)
        };
        if let Some((_, suggest)) = search.as_mut()
            && edited
            && names_changed
        {
            *suggest = Rc::new(Index::build(
                all.iter().map(|i| &**i),
                tags.iter().map(|t| t.name.as_str()),
                old.boards
                    .iter()
                    .flat_map(|(_, lists)| lists.iter().map(|l| l.name.as_str())),
            ));
        }
        let pinned = if pin_touched {
            Rc::new(pinned_of(&all))
        } else {
            Rc::clone(&old.pinned)
        };
        let cell = OnceCell::new();
        if let Some((index, suggest)) = search {
            let _ = cell.set(Search { index, suggest });
        }
        let mut state = self.0.borrow_mut();
        state.snapshot = Rc::new(Snapshot {
            positions: Rc::clone(&old.positions),
            pinned,
            all,
            boards: old.boards.clone(),
            tags,
            search: cell,
            builds: Rc::clone(&state.builds),
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

    /// Whether the search index of the current snapshot is built.
    #[must_use]
    pub fn search_ready(&self) -> bool {
        self.0.borrow().snapshot.search.get().is_some()
    }

    /// Builds the search index and suggestions of the current snapshot now (idle time after
    /// the first frame), so the first keystroke does not.
    pub fn warm(&self) {
        let snapshot = Rc::clone(&self.0.borrow().snapshot);
        let _ = snapshot.search();
    }

    /// How many times the search index was built (on first use after a read; for tests and the
    /// performance notes).
    #[must_use]
    pub fn index_builds(&self) -> usize {
        self.0.borrow().builds.get()
    }

    /// How many times the whole library was read again (for tests and the performance notes).
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

    // A25 smoke check: a 2000-item library is read and indexed exactly once. Counted, not timed:
    // elapsed time belongs to the ignored probe in `tests/budget.rs`.
    #[test]
    fn two_thousand_items_are_read_and_indexed_once() {
        let (mut s, l) = seeded();
        for n in 0..2000 {
            add(&mut s, l, &format!("snippet number {n}"));
        }
        let cache = IndexCache::new();
        let snapshot = cache.snapshot(&s).unwrap();
        assert_eq!(snapshot.index().len(), 2000);
        assert_eq!(cache.rebuilds(), 1);
        assert!(Rc::ptr_eq(&snapshot, &cache.snapshot(&s).unwrap()));
        assert_eq!(cache.rebuilds(), 1);
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
        let snapshot = cache.snapshot(&s).unwrap();
        assert_eq!(snapshot.index().len(), 10_000);
        assert_eq!(
            snapshot.rich.len(),
            100,
            "only the items with an HTML or RTF format"
        );

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
        assert_eq!(edited.index().len(), 10_000);
        assert_eq!((cache.rebuilds(), cache.patches()), (1, 2));
    }

    /// How many items two snapshots hold as the very same allocation.
    fn shared_items(a: &Snapshot, b: &Snapshot) -> usize {
        a.all
            .iter()
            .zip(&b.all)
            .filter(|(x, y)| Rc::ptr_eq(x, y))
            .count()
    }

    // PERF-1/2/16: a pin (or a use) of one item among 10,000 patches that item alone: no rebuild,
    // no copy of the other items, the suggestions and the rich set are the same allocations.
    #[test]
    fn a_pin_among_ten_thousand_items_copies_one_item_and_rebuilds_nothing() {
        let (mut s, l) = seeded();
        s.transaction::<_, snipdeck_store::Error>(|tx| {
            for n in 0..10_000 {
                let item = Item::new(l, format!("item {n}"), ItemKind::Text, at());
                let body = Content::Inline(Inline::new(b"body".to_vec()).unwrap());
                tx.insert_item(
                    &item,
                    &Formats::new(vec![Format::Text(body)]).unwrap(),
                    Placement::Last,
                )?;
            }
            Ok(())
        })
        .unwrap();
        let cache = IndexCache::new();
        let first = cache.snapshot(&s).unwrap();
        // The suggestions are built on first use: use them once so the pin has some to keep.
        let kept_suggest = first.suggest();
        assert!(first.pinned.is_empty());
        let id = first.all[7_000].id;

        let mut item = s.item(id).unwrap();
        item.pinned = true;
        cache
            .edit_item_meta(&mut s, id, |s| s.update_item(&item, None))
            .unwrap();
        let pinned = cache.snapshot(&s).unwrap();
        assert_eq!((cache.rebuilds(), cache.patches()), (1, 1));
        assert_eq!(shared_items(&first, &pinned), 9_999, "one item was copied");
        assert!(pinned.item(id).unwrap().pinned);
        assert!(
            !first.item(id).unwrap().pinned,
            "the old snapshot is intact"
        );
        assert_eq!(pinned.pinned.len(), 1);
        assert!(
            Rc::ptr_eq(&kept_suggest, &pinned.suggest()),
            "titles did not change"
        );
        assert!(
            Rc::ptr_eq(&first.rich, &pinned.rich),
            "formats did not change"
        );
        assert!(Rc::ptr_eq(&first.positions, &pinned.positions));

        // A counted use is the same: one item copied, the pinned list is kept as it was.
        s.record_use(id, at()).unwrap();
        cache.note_use(id);
        let used = cache.snapshot(&s).unwrap();
        assert_eq!(cache.patches(), 2);
        assert_eq!(shared_items(&pinned, &used), 9_999);
        assert_eq!(used.item(id).unwrap().use_count, 1);
        assert_eq!(used.pinned[0].use_count, 1);
    }

    #[test]
    fn a_write_builds_a_new_index() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let before = cache.snapshot(&s).unwrap();
        assert_eq!(before.index().len(), 1);
        add(&mut s, l, "beta");
        let after = cache.snapshot(&s).unwrap();
        assert_eq!(after.index().len(), 2);
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
        assert!(
            after_use
                .index()
                .usage_is_current(after_use.all.iter().map(|i| &**i))
        );
        assert!(
            !first
                .index()
                .usage_is_current(after_use.all.iter().map(|i| &**i))
        );
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
        assert_eq!(after.index().len(), 2);
        assert_eq!(
            first.all[1].title, "beta",
            "the earlier snapshot is untouched"
        );

        // A hint plus a write nobody announced is not trusted: everything is read again.
        s.record_use(id, at()).unwrap();
        cache.note_use(id);
        add(&mut s, l, "delta");
        let full = cache.snapshot(&s).unwrap();
        assert_eq!(full.index().len(), 3);
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

    #[test]
    fn the_index_is_built_on_first_use_and_a_patch_before_it_builds_nothing() {
        let (mut s, l) = seeded();
        for n in 0..50 {
            let item = Item::new(l, format!("item {n}"), ItemKind::Text, at());
            let body = Content::Inline(Inline::new(b"body".to_vec()).unwrap());
            s.insert_item(
                &item,
                &Formats::new(vec![Format::Text(body)]).unwrap(),
                Placement::Last,
            )
            .unwrap();
        }
        let cache = IndexCache::new();
        let first = cache.snapshot(&s).unwrap();
        assert_eq!(first.all.len(), 50, "the items are there at once");
        assert_eq!(
            cache.index_builds(),
            0,
            "reading the library builds no index"
        );
        let id = first.all[3].id;
        let mut item = s.item(id).unwrap();
        item.pinned = true;
        cache
            .edit_item_meta(&mut s, id, |s| s.update_item(&item, None))
            .unwrap();
        let pinned = cache.snapshot(&s).unwrap();
        assert_eq!((cache.rebuilds(), cache.patches()), (1, 1));
        assert_eq!(
            cache.index_builds(),
            0,
            "a patch before the first search builds none"
        );
        assert_eq!(pinned.index().len(), 50);
        let _ = pinned.suggest();
        assert_eq!(cache.index_builds(), 1, "the first search builds it once");
        let _ = pinned.index();
        let _ = cache.snapshot(&s).unwrap().index();
        assert_eq!(cache.index_builds(), 1, "and it is kept");
    }
}
