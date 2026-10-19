//! The Panel's state (DESIGN.md §5 Panel): which board and list are shown, the rows, the search,
//! the keyboard selection and what a click will do. Pure data: the UI layer copies it into Slint
//! models and translates the footer and use-label variants.

use std::collections::HashMap;
use std::rc::Rc;

use crate::index_cache::IndexCache;
use snipdeck_core::{
    Board, BoardId, Item, ItemId, ItemKind, ItemLabel, List, ListAccent, ListIcon, ListId,
    ListSort, search::SearchIndex,
};
use snipdeck_platform::route::{Policy, Route, Target, decide};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, TagRepo};

/// How many search results the Panel shows.
const SEARCH_LIMIT: usize = 50;

/// One row of the item list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The item.
    pub id: ItemId,
    /// The quick-use number 1 to 9 for the first nine rows, 0 for the rest.
    pub number: u8,
    /// What the item holds, for its leading element.
    pub kind: ItemKind,
    /// Masked and locked in the card.
    pub sensitive: bool,
    /// The title, or the first line of the preview when the item has none.
    pub title: String,
    /// The text preview; empty for sensitive items.
    pub preview: String,
    /// In mixed views (search): the list the item lives in.
    pub list: Option<ListLabel>,
    /// Shown in the pinned strip too.
    pub pinned: bool,
    /// The item's colour label.
    pub label: Option<ItemLabel>,
}

/// A list's name and accent, shown on rows in mixed views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListLabel {
    /// The list's name.
    pub name: String,
    /// Its colour, if it has one.
    pub accent: Option<ListAccent>,
}

/// A tab of the list bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    /// The list.
    pub id: ListId,
    /// Its name.
    pub name: String,
    /// Its colour.
    pub accent: Option<ListAccent>,
    /// Its icon.
    pub icon: Option<ListIcon>,
    /// The list currently shown.
    pub active: bool,
}

/// The Target footer, before translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Footer {
    /// "Pastes into **Outlook** · New message".
    PastesInto {
        /// A readable app name.
        app: String,
        /// The window title, possibly empty.
        title: String,
    },
    /// "Copies to clipboard".
    Copies,
}

/// The use label on a hovered or focused card, before translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseLabel {
    /// "Paste ↵".
    Paste,
    /// "Copy ↵".
    Copy,
}

/// The Panel's state.
#[derive(Debug)]
pub struct PanelModel {
    boards: Vec<Board>,
    board: Option<BoardId>,
    lists: Vec<List>,
    list: Option<ListId>,
    items: Vec<Item>,
    all: Vec<Item>,
    list_labels: HashMap<ListId, ListLabel>,
    index: Rc<SearchIndex>,
    cache: IndexCache,
    query: String,
    selected: usize,
    target: Option<Target>,
    policy: Policy,
}

impl PanelModel {
    /// Loads the Panel for `board` and `list` (the first of each when `None` or no longer
    /// present).
    ///
    /// # Errors
    ///
    /// Any store error.
    pub fn load<S>(
        store: &S,
        board: Option<BoardId>,
        list: Option<ListId>,
    ) -> snipdeck_store::Result<Self>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        Self::load_cached(store, board, list, &IndexCache::new())
    }

    /// Like [`PanelModel::load`], taking the search index from `cache` (built only when the
    /// library changed since it was last built). The model keeps a handle on `cache` for its
    /// own reloads.
    ///
    /// # Errors
    ///
    /// Any store error.
    pub fn load_cached<S>(
        store: &S,
        board: Option<BoardId>,
        list: Option<ListId>,
        cache: &IndexCache,
    ) -> snipdeck_store::Result<Self>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        let boards = store.boards()?;
        let board = board
            .filter(|id| boards.iter().any(|b| b.id == *id))
            .or_else(|| boards.first().map(|b| b.id));
        let lists = match board {
            Some(id) => store.lists(id)?,
            None => Vec::new(),
        };
        let list = list
            .filter(|id| lists.iter().any(|l| l.id == *id))
            .or_else(|| lists.first().map(|l| l.id));
        let items = match list {
            Some(id) => store.items(id)?,
            None => Vec::new(),
        };
        let all = store.all_items()?;
        let index = cache.index(store, &all)?;
        let mut list_labels = HashMap::new();
        for b in &boards {
            for l in store.lists(b.id)? {
                list_labels.insert(
                    l.id,
                    ListLabel {
                        name: l.name.as_str().to_owned(),
                        accent: l.accent,
                    },
                );
            }
        }
        Ok(Self {
            boards,
            board,
            lists,
            list,
            items,
            all,
            list_labels,
            index,
            cache: cache.clone(),
            query: String::new(),
            selected: 0,
            target: None,
            policy: Policy::default(),
        })
    }

    /// Loads the library again after it changed (a use counted, an item edited), keeping the
    /// board, list, search, selection, Target and policy.
    ///
    /// # Errors
    ///
    /// Any store error; the model is left as it was.
    pub fn refresh<S>(&mut self, store: &S) -> snipdeck_store::Result<()>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        self.reload(store, self.board, self.list, true)
    }

    /// Shows `list` of the current board. The search ends and the selection returns to the
    /// first row (switching lists leaves the board-wide search, DESIGN.md §5).
    ///
    /// # Errors
    ///
    /// Any store error; the model is left as it was.
    pub fn select_list<S>(&mut self, store: &S, list: ListId) -> snipdeck_store::Result<()>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        self.reload(store, self.board, Some(list), false)
    }

    /// Shows `board` and its first list, ending the search.
    ///
    /// # Errors
    ///
    /// Any store error; the model is left as it was.
    pub fn select_board<S>(&mut self, store: &S, board: BoardId) -> snipdeck_store::Result<()>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        self.reload(store, Some(board), None, false)
    }

    fn reload<S>(
        &mut self,
        store: &S,
        board: Option<BoardId>,
        list: Option<ListId>,
        keep_search: bool,
    ) -> snipdeck_store::Result<()>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo,
    {
        let mut next = Self::load_cached(store, board, list, &self.cache)?;
        next.target = self.target.take();
        next.policy = std::mem::take(&mut self.policy);
        if keep_search {
            next.query = std::mem::take(&mut self.query);
            next.selected = self.selected;
        }
        *self = next;
        Ok(())
    }

    /// The boards, in order.
    #[must_use]
    pub fn boards(&self) -> &[Board] {
        &self.boards
    }

    /// The board shown.
    #[must_use]
    pub fn board(&self) -> Option<BoardId> {
        self.board
    }

    /// The tabs of the shown board's lists.
    #[must_use]
    pub fn tabs(&self) -> Vec<Tab> {
        self.lists
            .iter()
            .map(|l| Tab {
                id: l.id,
                name: l.name.as_str().to_owned(),
                accent: l.accent,
                icon: l.icon,
                active: Some(l.id) == self.list,
            })
            .collect()
    }

    /// The list shown, if the board has one.
    #[must_use]
    pub fn list(&self) -> Option<&List> {
        self.lists.iter().find(|l| Some(l.id) == self.list)
    }

    /// Whether a search is active.
    #[must_use]
    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Sets the search text; the selection returns to the first row.
    pub fn set_query(&mut self, query: &str) {
        query.clone_into(&mut self.query);
        self.selected = 0;
    }

    /// The pinned items of the shown board's lists, as rows.
    #[must_use]
    pub fn pinned(&self) -> Vec<Row> {
        let in_board: Vec<ListId> = self.lists.iter().map(|l| l.id).collect();
        self.all
            .iter()
            .filter(|i| i.pinned && in_board.contains(&i.list_id))
            .map(|i| self.row(i, 0, false))
            .collect()
    }

    /// The rows to show: the search results, or the shown list in its sort order.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let items: Vec<&Item> = if self.searching() {
            self.index
                .query(&self.query, SEARCH_LIMIT)
                .iter()
                .filter_map(|hit| self.all.iter().find(|i| i.id == hit.id))
                .collect()
        } else {
            self.list_items()
        };
        let mixed = self.searching();
        items
            .into_iter()
            .enumerate()
            .map(|(n, item)| self.row(item, n, mixed))
            .collect()
    }

    /// The shown list's items in its sort order, whatever the search says.
    fn list_items(&self) -> Vec<&Item> {
        let mut items: Vec<&Item> = self.items.iter().collect();
        match self.list().map(|l| l.sort) {
            Some(ListSort::MostUsed) => items.sort_by_key(|i| std::cmp::Reverse(i.use_count)),
            Some(ListSort::RecentlyUsed) => {
                items.sort_by_key(|i| std::cmp::Reverse(i.last_used_at));
            }
            _ => {}
        }
        items
    }

    /// The ids of the shown list's items in its sort order, ignoring any search (the order
    /// "paste next" walks).
    #[must_use]
    pub fn list_item_ids(&self) -> Vec<ItemId> {
        self.list_items().iter().map(|i| i.id).collect()
    }

    fn row(&self, item: &Item, position: usize, mixed: bool) -> Row {
        let title = if item.title.trim().is_empty() {
            item.preview.lines().next().unwrap_or_default().to_owned()
        } else {
            item.title.clone()
        };
        Row {
            id: item.id,
            number: u8::try_from(position + 1)
                .ok()
                .filter(|n| *n <= 9)
                .unwrap_or(0),
            kind: item.kind,
            sensitive: item.sensitive,
            title,
            preview: item.preview.clone(),
            list: mixed
                .then(|| self.list_labels.get(&item.list_id).cloned())
                .flatten(),
            pinned: item.pinned,
            label: item.label,
        }
    }

    /// The keyboard-selected row.
    #[must_use]
    pub fn selected(&self) -> Option<ItemId> {
        let rows = self.rows();
        rows.get(self.selected.min(rows.len().saturating_sub(1)))
            .map(|r| r.id)
    }

    /// The position of the keyboard-selected row in [`rows`](Self::rows).
    #[must_use]
    pub fn selected_position(&self) -> Option<usize> {
        let len = self.rows().len();
        (len > 0).then(|| self.selected.min(len - 1))
    }

    /// Moves the selection by `delta` rows, stopping at the ends.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.rows().len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = self.selected.saturating_add_signed(delta).min(len - 1);
    }

    /// The row for quick-use number `n` (1 to 9), if the list has it.
    #[must_use]
    pub fn quick_use(&self, n: u8) -> Option<ItemId> {
        if !(1..=9).contains(&n) {
            return None;
        }
        self.rows().get(usize::from(n) - 1).map(|r| r.id)
    }

    /// Records the window that had focus before the Panel was clicked.
    pub fn set_target(&mut self, target: Option<Target>) {
        self.target = target;
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The window recorded by [`set_target`](Self::set_target).
    #[must_use]
    pub fn target(&self) -> Option<&Target> {
        self.target.as_ref()
    }

    /// Sets the apps that never receive an auto-paste.
    pub fn set_policy(&mut self, policy: Policy) {
        self.policy = policy;
    }

    /// What a click would do now.
    #[must_use]
    pub fn route(&self) -> Route {
        decide(self.target.as_ref(), &self.policy)
    }

    /// "Paste" when a click pastes, "Copy" otherwise.
    #[must_use]
    pub fn use_label(&self) -> UseLabel {
        match self.route() {
            Route::Paste { .. } => UseLabel::Paste,
            Route::Copy(_) => UseLabel::Copy,
        }
    }

    /// The footer: where a click goes.
    #[must_use]
    pub fn footer(&self) -> Footer {
        match (&self.route(), &self.target) {
            (Route::Paste { .. }, Some(t)) => Footer::PastesInto {
                app: friendly_app_name(&t.app),
                title: t.title.clone(),
            },
            _ => Footer::Copies,
        }
    }
}

/// `OUTLOOK.EXE` becomes `Outlook`; `notepad++.exe` becomes `Notepad++`.
#[must_use]
pub fn friendly_app_name(exe: &str) -> String {
    let stem = exe
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe)
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE");
    let mut chars = stem.chars();
    chars.next().map_or_else(String::new, |first| {
        first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::{Content, Format, Formats, Inline, Name, Placement, Timestamp};
    use snipdeck_platform::route::Focus;
    use snipdeck_store::Store;

    fn at() -> Timestamp {
        "2026-10-17T09:00:00Z".parse().expect("timestamp")
    }

    struct Fixture {
        store: Store,
        board: BoardId,
        replies: ListId,
        links: ListId,
    }

    fn fixture() -> Fixture {
        let mut store = Store::open_in_memory().expect("store");
        let board = Board::new(Name::new("Work").expect("name"), at());
        store.insert_board(&board, Placement::Last).expect("board");
        let mut replies = List::new(board.id, Name::new("Replies").expect("n"), at());
        replies.accent = Some(ListAccent::Teal);
        let links = List::new(board.id, Name::new("Links").expect("n"), at());
        store.insert_list(&replies, Placement::Last).expect("list");
        store.insert_list(&links, Placement::Last).expect("list");
        Fixture {
            store,
            board: board.id,
            replies: replies.id,
            links: links.id,
        }
    }

    #[derive(Clone, Copy)]
    enum Which {
        Replies,
        Links,
    }

    fn add_to(f: &mut Fixture, which: Which, title: &str, text: &str) -> ItemId {
        let list = match which {
            Which::Replies => f.replies,
            Which::Links => f.links,
        };
        let mut item = Item::new(list, title.into(), ItemKind::Text, at());
        item.preview = Item::preview_of(text, false);
        let content = Content::Inline(Inline::new(text.as_bytes().to_vec()).expect("inline"));
        let formats = Formats::new(vec![Format::Text(content)]).expect("formats");
        f.store
            .insert_item(&item, &formats, Placement::Last)
            .expect("insert");
        item.id
    }

    fn target(app: &str, focus: Focus) -> Target {
        Target {
            app: app.into(),
            title: "New message".into(),
            elevated: false,
            focus,
        }
    }

    #[test]
    fn it_shows_the_first_board_and_list_with_numbered_rows() {
        let mut f = fixture();
        for n in 0..11 {
            add_to(&mut f, Which::Replies, &format!("Item {n}"), "text");
        }
        let model = PanelModel::load(&f.store, None, None).expect("model");
        assert_eq!(model.board(), Some(f.board));
        let tabs = model.tabs();
        assert_eq!(tabs.len(), 2);
        assert!(tabs[0].active && !tabs[1].active);
        assert_eq!(tabs[0].accent, Some(ListAccent::Teal));
        let rows = model.rows();
        assert_eq!(rows.len(), 11);
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[8].number, 9);
        assert_eq!(rows[9].number, 0, "items 10+ have an empty gutter");
        assert!(
            rows.iter().all(|r| r.list.is_none()),
            "no list label inside one list"
        );
    }

    #[test]
    fn a_missing_hint_falls_back_to_the_first() {
        let f = fixture();
        let model = PanelModel::load(
            &f.store,
            Some(BoardId::generate()),
            Some(ListId::generate()),
        )
        .expect("model");
        assert_eq!(model.board(), Some(f.board));
        assert_eq!(model.list().map(|l| l.id), Some(f.replies));
        let other = PanelModel::load(&f.store, None, Some(f.links)).expect("model");
        assert_eq!(other.list().map(|l| l.id), Some(f.links));
    }

    #[test]
    fn an_untitled_item_shows_its_first_preview_line() {
        let mut f = fixture();
        add_to(&mut f, Which::Replies, "", "first line\nsecond line");
        let rows = PanelModel::load(&f.store, None, None)
            .expect("model")
            .rows();
        assert_eq!(rows[0].title, "first line");
    }

    #[test]
    fn search_spans_lists_and_labels_each_row_with_its_list() {
        let mut f = fixture();
        add_to(&mut f, Which::Replies, "Thanks", "Thanks for your email");
        let link = add_to(&mut f, Which::Links, "Docs", "https://example.com/thanks");
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.set_query("docs");
        let rows = model.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, link);
        assert_eq!(
            rows[0].list.as_ref().map(|l| l.name.as_str()),
            Some("Links")
        );
        model.set_query("   ");
        assert!(!model.searching());
        assert!(model.rows().iter().all(|r| r.list.is_none()));
    }

    #[test]
    fn selection_stops_at_the_ends_and_resets_on_a_new_query() {
        let mut f = fixture();
        let a = add_to(&mut f, Which::Replies, "A", "a");
        let b = add_to(&mut f, Which::Replies, "B", "b");
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        assert_eq!(model.selected(), Some(a));
        model.move_selection(5);
        assert_eq!(model.selected(), Some(b));
        model.move_selection(-9);
        assert_eq!(model.selected(), Some(a));
        model.move_selection(1);
        model.set_query("a");
        assert_eq!(model.selected(), Some(a));
        assert_eq!(model.quick_use(1), Some(a));
        assert_eq!(model.quick_use(0), None);
        assert_eq!(model.quick_use(2), None);
    }

    #[test]
    fn a_most_used_list_puts_the_busiest_item_first() {
        let mut f = fixture();
        let quiet = add_to(&mut f, Which::Replies, "Quiet", "q");
        let busy = add_to(&mut f, Which::Replies, "Busy", "b");
        for _ in 0..3 {
            f.store.record_use(busy, at()).expect("use");
        }
        f.store.record_use(quiet, at()).expect("use");
        let mut list = f.store.list(f.replies).expect("list");
        list.sort = ListSort::MostUsed;
        f.store.update_list(&list).expect("update");
        let rows = PanelModel::load(&f.store, None, None)
            .expect("model")
            .rows();
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [busy, quiet]);
    }

    #[test]
    fn an_empty_panel_has_no_selection() {
        let store = Store::open_in_memory().expect("store");
        let mut model = PanelModel::load(&store, None, None).expect("model");
        assert!(model.rows().is_empty() && model.tabs().is_empty());
        model.move_selection(1);
        assert_eq!(model.selected(), None);
    }

    #[test]
    fn pinned_items_of_the_board_are_listed_across_lists() {
        let mut f = fixture();
        let id = add_to(&mut f, Which::Links, "Docs", "x");
        let mut item = f.store.item(id).expect("item");
        item.pinned = true;
        f.store.update_item(&item, None).expect("update");
        let model = PanelModel::load(&f.store, None, None).expect("model");
        assert_eq!(model.pinned().len(), 1);
        assert!(model.pinned()[0].pinned);
    }

    #[test]
    fn the_footer_and_use_label_follow_the_target() {
        let f = fixture();
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        assert_eq!(model.footer(), Footer::Copies);
        assert_eq!(model.use_label(), UseLabel::Copy);
        model.set_target(Some(target("OUTLOOK.EXE", Focus::Caret)));
        assert_eq!(
            model.footer(),
            Footer::PastesInto {
                app: "Outlook".into(),
                title: "New message".into()
            }
        );
        assert_eq!(model.use_label(), UseLabel::Paste);
        model.set_target(Some(target("OUTLOOK.EXE", Focus::None)));
        assert_eq!(model.use_label(), UseLabel::Copy);
        model.set_target(Some(target("OUTLOOK.EXE", Focus::Caret)));
        model.set_policy(Policy {
            no_paste_apps: vec!["outlook.exe".into()],
        });
        assert_eq!(model.footer(), Footer::Copies);
    }

    #[test]
    fn select_list_switches_lists_and_ends_the_search() {
        let mut f = fixture();
        add_to(&mut f, Which::Replies, "Thanks", "thanks");
        let docs = add_to(&mut f, Which::Links, "Docs", "https://example.com");
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.set_target(Some(target("OUTLOOK.EXE", Focus::Caret)));
        model.set_query("thanks");
        model.select_list(&f.store, f.links).expect("select");
        assert_eq!(model.list().map(|l| l.id), Some(f.links));
        assert!(!model.searching());
        assert_eq!(
            model.rows().iter().map(|r| r.id).collect::<Vec<_>>(),
            [docs]
        );
        assert_eq!(model.use_label(), UseLabel::Paste, "the Target survives");
        assert!(model.tabs()[1].active);
    }

    #[test]
    fn select_board_shows_that_boards_first_list() {
        let mut f = fixture();
        let other = Board::new(Name::new("Home").expect("name"), at());
        f.store
            .insert_board(&other, Placement::Last)
            .expect("board");
        let list = List::new(other.id, Name::new("Chores").expect("n"), at());
        f.store.insert_list(&list, Placement::Last).expect("list");
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.select_board(&f.store, other.id).expect("select");
        assert_eq!(model.board(), Some(other.id));
        assert_eq!(model.list().map(|l| l.id), Some(list.id));
        assert_eq!(model.tabs().len(), 1);
    }

    #[test]
    fn refresh_keeps_the_view_but_sees_new_data() {
        let mut f = fixture();
        let a = add_to(&mut f, Which::Replies, "A", "alpha");
        let b = add_to(&mut f, Which::Replies, "B", "beta");
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.set_policy(Policy {
            no_paste_apps: vec!["x.exe".into()],
        });
        model.move_selection(1);
        assert_eq!(model.selected(), Some(b));
        let mut list = f.store.list(f.replies).expect("list");
        list.sort = ListSort::MostUsed;
        f.store.update_list(&list).expect("update");
        f.store.record_use(a, at()).expect("use");
        model.refresh(&f.store).expect("refresh");
        assert_eq!(
            model.rows().iter().map(|r| r.id).collect::<Vec<_>>(),
            [a, b]
        );
        assert_eq!(
            model.selected_position(),
            Some(1),
            "the selection stays put"
        );
        model.set_query("alp");
        model.refresh(&f.store).expect("refresh");
        assert!(model.searching(), "the search survives a refresh");
        assert_eq!(model.selected_position(), Some(0));
    }

    #[test]
    fn friendly_names_drop_the_extension_and_fix_the_case() {
        assert_eq!(friendly_app_name("OUTLOOK.EXE"), "Outlook");
        assert_eq!(friendly_app_name("notepad++.exe"), "Notepad++");
        assert_eq!(friendly_app_name("C:\\Windows\\notepad.exe"), "Notepad");
        assert_eq!(friendly_app_name(""), "");
    }
}
