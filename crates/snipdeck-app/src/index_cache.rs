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
use std::collections::HashMap;
use std::rc::Rc;

use snipdeck_core::{Board, Item, List, search::SearchIndex};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, RecentEntry, RecentRepo, TagRepo};

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
        let all = store.all_items()?;
        let mut boards = Vec::new();
        for b in store.boards()? {
            let lists = store.lists(b.id)?;
            boards.push((b, lists));
        }
        let reuse = {
            let state = self.0.borrow();
            (state.built_at == Some(counter)).then(|| Rc::clone(&state.snapshot.index))
        };
        let (index, built) = if let Some(mut index) = reuse {
            // A counted use changes only the tie-breakers (use count, last use): bring them up
            // to date, copying the index only when one did change.
            if !index.usage_is_current(&all) {
                Rc::make_mut(&mut index).refresh_usage(&all);
            }
            (index, false)
        } else {
            let tag_names: HashMap<_, _> = store
                .tags()?
                .into_iter()
                .map(|t| (t.id, t.name.as_str().to_owned()))
                .collect();
            let mut index = SearchIndex::default();
            for item in &all {
                index.upsert_with_tags(
                    item,
                    item.tags
                        .iter()
                        .filter_map(|t| tag_names.get(t).map(String::as_str)),
                );
            }
            (Rc::new(index), true)
        };
        let mut state = self.0.borrow_mut();
        state.snapshot = Rc::new(Snapshot { all, boards, index });
        state.read_at = Some(at);
        state.built_at = Some(counter);
        state.rebuilds += usize::from(built);
        Ok(Rc::clone(&state.snapshot))
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
        state.recent = None;
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

        s.add_recent("copied", None, at()).unwrap();
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

    /// A store that counts how often the Recent list is read.
    struct Counting {
        inner: Store,
        reads: std::cell::Cell<usize>,
    }

    impl RecentRepo for Counting {
        fn add_recent(
            &mut self,
            text: &str,
            app: Option<&str>,
            now: Timestamp,
        ) -> snipdeck_store::Result<i64> {
            self.inner.add_recent(text, app, now)
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
        s.add_recent("one", None, at()).unwrap();
        let cache = IndexCache::new();
        assert_eq!(cache.recent(&s).len(), 1);
        assert_eq!(cache.recent(&s).len(), 1);
        assert_eq!(s.reads.get(), 1, "the second call reads nothing");
        s.add_recent("two", None, at()).unwrap();
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
