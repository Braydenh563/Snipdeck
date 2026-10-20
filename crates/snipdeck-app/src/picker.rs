//! The Picker's state (SPEC 5.2, DESIGN.md §5 Picker): search over every board and list, the
//! scope (all, the board the Panel shows, pinned), and the keyboard selection. Pure data: the
//! binding copies it into the `PickerWindow`, and the controller turns "use row N" into a use.
//!
//! Like the Panel, matching runs in memory over title, tags and the start of the content.

use std::collections::HashMap;
use std::rc::Rc;

use crate::index_cache::IndexCache;
use snipdeck_core::{BoardId, Item, ItemId, ItemKind, ListAccent, ListId, search::SearchIndex};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, TagRepo};

/// How many results the Picker shows (the window scrolls past nine).
pub const RESULT_LIMIT: usize = 50;

/// What a search covers. Tab cycles through these in this order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    /// Every board.
    #[default]
    All,
    /// The board the Panel was showing when the Picker opened.
    Board,
    /// Pinned items, on any board.
    Pinned,
}

/// One result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The item.
    pub id: ItemId,
    /// What it holds.
    pub kind: ItemKind,
    /// Masked and locked in the card.
    pub sensitive: bool,
    /// The title, or the first line of the preview when the item has none.
    pub title: String,
    /// The text preview; shown masked for sensitive items.
    pub preview: String,
    /// The board it lives on.
    pub board: String,
    /// The list it lives in.
    pub list: String,
    /// The list's colour, if it has one.
    pub accent: Option<ListAccent>,
}

struct Place {
    item: Item,
    board: BoardId,
    board_name: String,
    list_name: String,
    accent: Option<ListAccent>,
}

/// The Picker's state.
pub struct PickerModel {
    /// Every live item, in library order (board, list, manual order).
    places: Vec<Place>,
    index: Rc<SearchIndex>,
    /// The board the Panel showed when the Picker opened (the "This board" scope).
    board: Option<BoardId>,
    query: String,
    scope: Scope,
    /// Positions in `places` of the current results, best first.
    results: Vec<usize>,
    selected: usize,
}

impl std::fmt::Debug for PickerModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickerModel")
            .field("scope", &self.scope)
            .field("results", &self.results.len())
            .finish_non_exhaustive()
    }
}

impl PickerModel {
    /// Reads the whole library. `board` is the board the Panel shows, for the "This board"
    /// scope. Starts with an empty query, the All scope and the first row selected.
    ///
    /// # Errors
    ///
    /// Any store error.
    #[cfg(test)]
    pub fn load<S>(store: &S, board: Option<BoardId>) -> snipdeck_store::Result<Self>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        Self::load_cached(store, board, &IndexCache::new())
    }

    /// Reads the whole library through `cache` (read again only when the library changed since).
    /// `board` is the board the Panel shows, for the "This board" scope. Starts with an empty
    /// query, the All scope and the first row selected.
    ///
    /// # Errors
    ///
    /// Any store error.
    pub fn load_cached<S>(
        store: &S,
        board: Option<BoardId>,
        cache: &IndexCache,
    ) -> snipdeck_store::Result<Self>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        let snapshot = cache.snapshot(store)?;
        let mut by_list: HashMap<ListId, Vec<&Item>> = HashMap::new();
        for item in &snapshot.all {
            by_list.entry(item.list_id).or_default().push(item);
        }
        let mut places = Vec::new();
        for (b, lists) in &snapshot.boards {
            for l in lists {
                for item in by_list.remove(&l.id).unwrap_or_default() {
                    places.push(Place {
                        item: item.clone(),
                        board: b.id,
                        board_name: b.name.as_str().to_owned(),
                        list_name: l.name.as_str().to_owned(),
                        accent: l.accent,
                    });
                }
            }
        }
        let index = Rc::clone(&snapshot.index);
        let mut model = Self {
            places,
            index,
            board,
            query: String::new(),
            scope: Scope::All,
            results: Vec::new(),
            selected: 0,
        };
        model.search();
        Ok(model)
    }

    fn in_scope(&self, place: &Place) -> bool {
        match self.scope {
            Scope::All => true,
            Scope::Board => Some(place.board) == self.board,
            Scope::Pinned => place.item.pinned,
        }
    }

    fn search(&mut self) {
        let positions: HashMap<ItemId, usize> = self
            .places
            .iter()
            .enumerate()
            .map(|(n, p)| (p.item.id, n))
            .collect();
        let in_scope = |n: &usize| self.in_scope(&self.places[*n]);
        self.results = if self.query.trim().is_empty() {
            // Nothing typed: what was used most recently first, then most used, then library order.
            let mut all: Vec<usize> = (0..self.places.len()).filter(in_scope).collect();
            all.sort_by_key(|n| {
                let item = &self.places[*n].item;
                (
                    std::cmp::Reverse(item.last_used_at),
                    std::cmp::Reverse(item.use_count),
                )
            });
            all
        } else {
            self.index
                .query(&self.query, self.places.len())
                .iter()
                .filter_map(|hit| positions.get(&hit.id).copied())
                .filter(in_scope)
                .collect()
        };
        self.results.truncate(RESULT_LIMIT);
        self.selected = 0;
    }

    /// Sets the search text; the selection returns to the first row.
    pub fn set_query(&mut self, query: &str) {
        query.clone_into(&mut self.query);
        self.search();
    }

    /// Sets the scope; the selection returns to the first row.
    pub fn set_scope(&mut self, scope: Scope) {
        self.scope = scope;
        self.search();
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// What the search covers.
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The results, best first, at most [`RESULT_LIMIT`].
    #[must_use]
    pub fn rows(&self) -> Vec<Entry> {
        self.results
            .iter()
            .map(|n| {
                let p = &self.places[*n];
                Entry {
                    id: p.item.id,
                    kind: p.item.kind,
                    sensitive: p.item.sensitive,
                    title: if p.item.title.trim().is_empty() {
                        p.item.preview.lines().next().unwrap_or_default().to_owned()
                    } else {
                        p.item.title.clone()
                    },
                    preview: p.item.preview.clone(),
                    board: p.board_name.clone(),
                    list: p.list_name.clone(),
                    accent: p.accent,
                }
            })
            .collect()
    }

    /// The item in result row `index`.
    #[must_use]
    pub fn item_at(&self, index: usize) -> Option<ItemId> {
        self.results.get(index).map(|n| self.places[*n].item.id)
    }

    /// The item for quick-use number `n` (1 to 9).
    #[must_use]
    pub fn quick_use(&self, n: u8) -> Option<ItemId> {
        if !(1..=9).contains(&n) {
            return None;
        }
        self.item_at(usize::from(n) - 1)
    }

    /// The keyboard-selected row (0 when there are no results).
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected.min(self.results.len().saturating_sub(1))
    }

    /// Moves the selection by `delta` rows, stopping at the ends.
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.results.len().saturating_sub(1);
        self.selected = self.selected().saturating_add_signed(delta).min(last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use snipdeck_core::{Content, Format, Formats, Inline, List, Name, Placement, Timestamp};
    use snipdeck_store::Store;

    fn now() -> Timestamp {
        "2026-10-18T09:00:00Z".parse().expect("timestamp")
    }

    fn model() -> (Store, PickerModel) {
        let store = demo::store(now()).expect("demo store");
        let work = store.boards().expect("boards")[0].id;
        let model = PickerModel::load(&store, Some(work)).expect("model");
        (store, model)
    }

    fn titles(model: &PickerModel) -> Vec<String> {
        model.rows().into_iter().map(|e| e.title).collect()
    }

    #[test]
    fn it_opens_on_every_item_of_every_board_with_where_each_lives() {
        let (_, m) = model();
        let rows = m.rows();
        assert_eq!(rows.len(), 11);
        assert_eq!((m.scope(), m.query(), m.selected()), (Scope::All, "", 0));
        let shopping = rows
            .iter()
            .find(|r| r.title == "Shopping list")
            .expect("row");
        assert_eq!(
            (shopping.board.as_str(), shopping.list.as_str()),
            ("Personal", "Errands")
        );
        assert_eq!(shopping.accent, Some(ListAccent::Green));
        let secret = rows
            .iter()
            .find(|r| r.title == "Server password")
            .expect("row");
        assert!(secret.sensitive);
    }

    #[test]
    fn with_no_query_what_was_used_most_recently_comes_first() {
        let (mut store, m) = model();
        let office = m
            .rows()
            .into_iter()
            .find(|r| r.title == "Office")
            .expect("row")
            .id;
        let thanks = m
            .rows()
            .into_iter()
            .find(|r| r.title == "Thanks, received")
            .expect("row")
            .id;
        store.record_use(office, now()).expect("use");
        store
            .record_use(thanks, "2026-10-18T09:05:00Z".parse().expect("time"))
            .expect("use");
        store
            .record_use(thanks, "2026-10-18T09:06:00Z".parse().expect("time"))
            .expect("use");
        let m = PickerModel::load(&store, None).expect("model");
        assert_eq!(m.item_at(0), Some(thanks));
        assert_eq!(m.item_at(1), Some(office));
    }

    #[test]
    fn typing_searches_all_boards_and_resets_the_selection() {
        let (_, mut m) = model();
        m.move_selection(3);
        assert_eq!(m.selected(), 3);
        m.set_query("shopping");
        assert_eq!(titles(&m), ["Shopping list"]);
        assert_eq!(m.selected(), 0);
        m.set_query("zzzzqq");
        assert_eq!(m.rows(), Vec::new());
        assert_eq!(m.item_at(0), None);
        m.set_query("  ");
        assert_eq!(m.rows().len(), 11);
    }

    #[test]
    fn scopes_narrow_the_search_to_the_board_or_the_pinned_items() {
        let (_, mut m) = model();
        m.set_scope(Scope::Board);
        assert_eq!(m.rows().len(), 10, "Work has ten items");
        assert!(m.rows().iter().all(|r| r.board == "Work"));
        m.set_scope(Scope::Pinned);
        let mut pinned = titles(&m);
        pinned.sort();
        assert_eq!(pinned, ["Home address", "Retry helper", "Thanks, received"]);

        // The scope applies to a query too.
        m.set_scope(Scope::Board);
        m.set_query("shopping");
        assert!(m.rows().is_empty(), "Shopping list is on the other board");
        m.set_scope(Scope::All);
        assert_eq!(titles(&m), ["Shopping list"]);
        m.set_query("home");
        m.set_scope(Scope::Pinned);
        assert_eq!(titles(&m), ["Home address"]);
    }

    #[test]
    fn the_board_scope_without_a_board_is_empty() {
        let store = demo::store(now()).expect("demo store");
        let mut m = PickerModel::load(&store, None).expect("model");
        m.set_scope(Scope::Board);
        assert_eq!(m.rows(), Vec::new());
    }

    #[test]
    fn the_selection_stops_at_both_ends_and_quick_use_counts_rows() {
        let (_, mut m) = model();
        m.move_selection(-4);
        assert_eq!(m.selected(), 0);
        m.move_selection(100);
        assert_eq!(m.selected(), 10);
        m.move_selection(-1);
        assert_eq!(m.selected(), 9);
        assert_eq!(m.quick_use(1), m.item_at(0));
        assert_eq!(m.quick_use(9), m.item_at(8));
        assert_eq!(m.quick_use(0), None);
        assert_eq!(m.quick_use(10), None);

        m.set_query("zzzzqq");
        m.move_selection(5);
        assert_eq!(m.selected(), 0, "no results, no selection to move");
    }

    #[test]
    fn a_big_library_shows_the_best_fifty() {
        let mut store = Store::open_in_memory().expect("store");
        let board = snipdeck_core::Board::new(Name::new("B").expect("name"), now());
        store.insert_board(&board, Placement::Last).expect("board");
        let list = List::new(board.id, Name::new("L").expect("name"), now());
        store.insert_list(&list, Placement::Last).expect("list");
        for n in 0..(RESULT_LIMIT + 10) {
            let item = Item::new(list.id, format!("Snippet {n}"), ItemKind::Text, now());
            let content = Content::Inline(Inline::new(b"x".to_vec()).expect("inline"));
            let formats = Formats::new(vec![Format::Text(content)]).expect("formats");
            store
                .insert_item(&item, &formats, Placement::Last)
                .expect("item");
        }
        let mut m = PickerModel::load(&store, Some(board.id)).expect("model");
        assert_eq!(m.rows().len(), RESULT_LIMIT);
        m.set_query("snippet");
        assert_eq!(m.rows().len(), RESULT_LIMIT);
    }
}
