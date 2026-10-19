//! The search index, built once per library change instead of once per load (SPEC §9).
//!
//! The Panel and the Picker both search the whole library. Building the index reads every
//! tag and indexes every item, which is work that only has to be repeated when something was
//! written. [`IndexCache`] keeps the last index together with the store's
//! [`change_counter`](snipdeck_store::ItemRepo::change_counter) it was built at, and hands the
//! same [`Rc`] out again while the counter is unchanged. Any write (an edit, a use counted, a
//! tag renamed, an item trashed or restored, an import) moves the counter, so the next load
//! builds a fresh index. Per-board lookups stay cheap without a per-board index: the shared
//! index covers the library, and a board's results are the hits whose lists belong to it.
//!
//! The cache is a cheap handle (clones share one state), so the controller owns one and hands
//! a clone to each model it loads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use snipdeck_core::{Item, search::SearchIndex};
use snipdeck_store::{ItemRepo, TagRepo};

#[derive(Debug, Default)]
struct State {
    /// The counter the cached index was built at; `None` before the first build.
    built_at: Option<u64>,
    index: Rc<SearchIndex>,
    rebuilds: usize,
}

/// A shared handle on the last-built search index.
#[derive(Debug, Clone, Default)]
pub struct IndexCache(Rc<RefCell<State>>);

impl IndexCache {
    /// An empty cache: the first [`IndexCache::index`] builds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The index of `items` (every live item, in library order, as `all_items` returns them),
    /// reused when the store has not been written since it was built.
    ///
    /// # Errors
    ///
    /// Any store error while reading the tags (only read when the index is rebuilt).
    pub fn index<S>(&self, store: &S, items: &[Item]) -> snipdeck_store::Result<Rc<SearchIndex>>
    where
        S: ItemRepo + TagRepo,
    {
        let counter = store.change_counter();
        {
            let state = self.0.borrow();
            if state.built_at == Some(counter) {
                return Ok(Rc::clone(&state.index));
            }
        }
        let tag_names: HashMap<_, _> = store
            .tags()?
            .into_iter()
            .map(|t| (t.id, t.name.as_str().to_owned()))
            .collect();
        let mut index = SearchIndex::default();
        for item in items {
            index.upsert_with_tags(
                item,
                item.tags
                    .iter()
                    .filter_map(|t| tag_names.get(t).map(String::as_str)),
            );
        }
        let mut state = self.0.borrow_mut();
        state.index = Rc::new(index);
        state.built_at = Some(counter);
        state.rebuilds += 1;
        Ok(Rc::clone(&state.index))
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
    use snipdeck_store::{BoardRepo, ListRepo, Store};

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
    fn an_unchanged_library_reuses_the_index() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let items = s.all_items().unwrap();
        let first = cache.index(&s, &items).unwrap();
        let second = cache.index(&s, &s.all_items().unwrap()).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(cache.rebuilds(), 1);
        // Clones share the state.
        let third = cache.clone().index(&s, &items).unwrap();
        assert!(Rc::ptr_eq(&first, &third));
        assert_eq!(cache.rebuilds(), 1);
    }

    #[test]
    fn a_write_builds_a_new_index() {
        let (mut s, l) = seeded();
        add(&mut s, l, "alpha");
        let cache = IndexCache::new();
        let before = cache.index(&s, &s.all_items().unwrap()).unwrap();
        assert_eq!(before.len(), 1);
        add(&mut s, l, "beta");
        let after = cache.index(&s, &s.all_items().unwrap()).unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(cache.rebuilds(), 2);
        // A tag write alone also invalidates (tag names are searchable).
        s.insert_tag(&Tag::new(Name::new("t").unwrap())).unwrap();
        cache.index(&s, &s.all_items().unwrap()).unwrap();
        assert_eq!(cache.rebuilds(), 3);
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

        add(&mut s, l, "beta");
        panel.refresh(&s).unwrap();
        assert_eq!(cache.rebuilds(), 2);
        PickerModel::load_cached(&s, panel.board(), &cache).unwrap();
        assert_eq!(cache.rebuilds(), 2, "the Picker reuses the Panel's rebuild");
    }
}
