//! The Panel's state (DESIGN.md §5 Panel): which board and list are shown, the rows, the search,
//! the keyboard selection and what a click will do. Pure data: the UI layer copies it into Slint
//! models and translates the footer and use-label variants.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::{
    index_cache::{IndexCache, Snapshot},
    main_window::SelectMode,
    search_mark,
};
use snipdeck_core::{
    Board, BoardId, Item, ItemId, ItemKind, ItemLabel, List, ListAccent, ListIcon, ListId,
    ListSort, search::SearchIndex,
};
use snipdeck_platform::{
    keys::{PasteRule, builtin_paste_rules},
    route::{Policy, Route, Target, decide_with},
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, TagRepo};

/// How many search results the Panel shows.
const SEARCH_LIMIT: usize = 50;

/// One row of the item list.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent row flags the view shows, not a state machine"
)]
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
    /// In a search: the first match of the query in the title, as the text before it and the match.
    pub title_mark: Option<(String, String)>,
    /// In a search: the first match of the query in the first line of the preview, as the text
    /// before it, the match and the text after it (the line is cut to a window around the match).
    /// Never set for a sensitive item.
    pub preview_mark: Option<(String, String, String)>,
    /// In a search: the name of the tag that matched, when the title and the preview did not
    /// show the match. Never set for a sensitive item.
    pub tag_hit: Option<String>,
    /// Shown in the pinned strip too.
    pub pinned: bool,
    /// The item's colour label.
    pub label: Option<ItemLabel>,
    /// In the multi-selection (Ctrl+click, Shift+click, Ctrl+A).
    pub marked: bool,
    /// Delete mode: the item is in the Trash and the row shows "Deleted · Undo" in its place until
    /// the mode ends. Only the controller sets it.
    pub deleted_pending: bool,
}

/// A list's name and accent, shown on rows in mixed views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListLabel {
    /// The name of the board the list is on (search results say "Board · List").
    pub board: String,
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
    /// Every item, the boards and the index, as read when the library last changed.
    all: Rc<Snapshot>,
    list_labels: HashMap<ListId, ListLabel>,
    index: Rc<SearchIndex>,
    cache: IndexCache,
    query: String,
    selected: usize,
    /// The multi-selection, as Main's `pick-mode` keeps one: the items marked with Ctrl+click,
    /// Shift+click, Ctrl+A or Shift+Up/Down. Separate from the keyboard cursor `selected`.
    marked: HashSet<ItemId>,
    /// Where a Shift+click or Shift+arrow range starts.
    anchor: Option<ItemId>,
    target: Option<Target>,
    policy: Policy,
    /// The paste key of each program (Settings > Programs). Until the controller sets them
    /// ([`set_paste_rules`](Self::set_paste_rules)) the built-in ones.
    paste_rules: Vec<PasteRule>,
    /// Delete mode: the rows deleted in it that still show as "Deleted · Undo". They belong to
    /// one list and sit after the row that was above them (see [`Ghost`]).
    ghosts: Vec<Ghost>,
}

/// A row deleted in Delete mode. The item is in the Trash; the row stays in
/// [`PanelModel::rows`] (flagged `deleted_pending`) so the shown index and the model index are
/// one and the same, until the mode ends or the shown list changes.
#[derive(Debug, Clone)]
struct Ghost {
    /// The list the row was in: ghosts of other lists are not shown.
    list: ListId,
    /// The row that was shown above it (a live row or another ghost); `None` = it was first.
    after: Option<ItemId>,
    /// The row as it was.
    row: Row,
    /// The history step that Undo takes back.
    step: Option<u64>,
}

impl PanelModel {
    /// Loads the Panel for `board` and `list` (the first of each when `None` or no longer
    /// present).
    ///
    /// # Errors
    ///
    /// Any store error.
    #[cfg(test)]
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

    /// Loads the Panel for `board` and `list` (the first of each when `None` or no longer
    /// present), reading the library through `cache` (read again only when the library changed
    /// since). The model keeps a handle on `cache` for its own reloads.
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
        let snapshot = cache.snapshot(store)?;
        let boards: Vec<Board> = snapshot.boards.iter().map(|(b, _)| b.clone()).collect();
        let board = board
            .filter(|id| boards.iter().any(|b| b.id == *id))
            .or_else(|| boards.first().map(|b| b.id));
        let lists = board
            .and_then(|id| snapshot.boards.iter().find(|(b, _)| b.id == id))
            .map_or_else(Vec::new, |(_, lists)| lists.clone());
        let list = list
            .filter(|id| lists.iter().any(|l| l.id == *id))
            .or_else(|| lists.first().map(|l| l.id));
        let items = match list {
            Some(id) => store.items(id)?,
            None => Vec::new(),
        };
        let list_labels = snapshot
            .boards
            .iter()
            .flat_map(|(b, lists)| lists.iter().map(move |l| (b, l)))
            .map(|(b, l)| {
                (
                    l.id,
                    ListLabel {
                        board: b.name.as_str().to_owned(),
                        name: l.name.as_str().to_owned(),
                        accent: l.accent,
                    },
                )
            })
            .collect();
        let index = Rc::clone(&snapshot.index);
        Ok(Self {
            boards,
            board,
            lists,
            list,
            items,
            all: snapshot,
            list_labels,
            index,
            cache: cache.clone(),
            query: String::new(),
            selected: 0,
            ghosts: Vec::new(),
            marked: HashSet::new(),
            anchor: None,
            target: None,
            policy: Policy::default(),
            paste_rules: builtin_paste_rules(),
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
            // A refresh (an item added, moved, trashed) keeps the marks of items still shown here.
            let here: HashSet<ItemId> = next.items.iter().map(|i| i.id).collect();
            next.marked = self
                .marked
                .iter()
                .copied()
                .filter(|i| here.contains(i))
                .collect();
            next.anchor = self.anchor.filter(|a| next.marked.contains(a));
        }
        if keep_search && next.list == self.list {
            next.ghosts = std::mem::take(&mut self.ghosts);
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

    /// The shown list orders itself (not manual) and no search runs: a move is refused, but the
    /// refusal offers to switch the list to manual order.
    #[must_use]
    pub fn sorts_itself(&self) -> bool {
        !self.searching() && self.list().is_some_and(|l| l.sort != ListSort::Manual)
    }

    /// Whether a search is active.
    #[must_use]
    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Whether the rows can be arranged by hand: the shown list is in manual order and no
    /// search is running.
    #[must_use]
    pub fn reorderable(&self) -> bool {
        !self.searching() && self.list().is_some_and(|l| l.sort == ListSort::Manual)
    }

    /// The shown list's items in manual order.
    #[must_use]
    pub fn manual_order(&self) -> Vec<ItemId> {
        self.items.iter().map(|i| i.id).collect()
    }

    /// The names the search suggestions are made of (shared with the Picker's, built with the
    /// search index).
    #[must_use]
    pub fn suggest_index(&self) -> Rc<crate::search_suggest::Index> {
        Rc::clone(&self.all.suggest)
    }

    /// Sets the search text; the selection returns to the first row.
    pub fn set_query(&mut self, query: &str) {
        query.clone_into(&mut self.query);
        self.selected = 0;
        self.clear_marks();
    }

    /// The pinned items of the shown board's lists, as rows.
    #[must_use]
    pub fn pinned(&self) -> Vec<Row> {
        let in_board: Vec<ListId> = self.lists.iter().map(|l| l.id).collect();
        self.all
            .all
            .iter()
            .filter(|i| i.pinned && in_board.contains(&i.list_id))
            .map(|i| self.row(i, 0, false, None))
            .collect()
    }

    /// The rows to show: the search results, or the shown list in its sort order.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        if !self.searching() {
            let mut rows: Vec<Row> = self
                .list_items()
                .into_iter()
                .enumerate()
                .map(|(n, item)| self.row(item, n, false, None))
                .collect();
            self.place_ghosts(&mut rows);
            return rows;
        }
        self.index
            .query(&self.query, SEARCH_LIMIT)
            .iter()
            .filter_map(|hit| {
                let item = self.all.all.iter().find(|i| i.id == hit.id)?;
                Some((item, hit.tag.as_deref()))
            })
            .enumerate()
            .map(|(n, (item, tag))| self.row(item, n, true, tag))
            .collect()
    }

    /// Puts the "Deleted · Undo" rows of the shown list back, each after the row that was above it.
    /// A ghost whose item is back (Undo from a toast, Ctrl+Z) is not shown twice.
    fn place_ghosts(&self, rows: &mut Vec<Row>) {
        let mut pending: Vec<&Ghost> = self
            .ghosts
            .iter()
            .filter(|g| Some(g.list) == self.list && !rows.iter().any(|r| r.id == g.row.id))
            .collect();
        let ghost_row = |g: &Ghost| {
            let mut row = g.row.clone();
            row.deleted_pending = true;
            row.marked = false;
            row.number = 0;
            row
        };
        while !pending.is_empty() {
            let before = pending.len();
            pending.retain(|g| {
                let at = match g.after {
                    None => Some(0),
                    Some(a) => rows.iter().position(|r| r.id == a).map(|p| p + 1),
                };
                at.is_none_or(|at| {
                    rows.insert(at, ghost_row(g));
                    false
                })
            });
            if pending.len() == before {
                // Its neighbour is gone (moved away): the row goes last.
                rows.extend(pending.iter().map(|g| ghost_row(g)));
                break;
            }
        }
    }

    /// Delete mode: the shown row `index` (a live one) was just trashed; it keeps showing as
    /// "Deleted · Undo" (`step` is what takes the delete back). Call before the refresh.
    pub fn add_ghost(&mut self, index: usize, step: Option<u64>) {
        if self.searching() {
            return;
        }
        let (Some(list), rows) = (self.list, self.rows()) else {
            return;
        };
        let Some(row) = rows.get(index).filter(|r| !r.deleted_pending) else {
            return;
        };
        let after = index.checked_sub(1).and_then(|i| rows.get(i)).map(|r| r.id);
        let ghost = Ghost {
            list,
            after,
            row: row.clone(),
            step,
        };
        self.ghosts.retain(|g| g.row.id != ghost.row.id);
        self.ghosts.push(ghost);
    }

    /// The history step of the ghost of item `id` (`Some(None)`: it had none), if there is one.
    #[must_use]
    pub fn ghost_step(&self, id: ItemId) -> Option<Option<u64>> {
        self.ghosts.iter().find(|g| g.row.id == id).map(|g| g.step)
    }

    /// Forgets the ghost of item `id`.
    pub fn remove_ghost(&mut self, id: ItemId) {
        self.ghosts.retain(|g| g.row.id != id);
    }

    /// Forgets every "Deleted · Undo" row (the mode ended).
    pub fn clear_ghosts(&mut self) {
        self.ghosts.clear();
    }

    /// The index of the live row nearest to `at` (itself, else the next, else the previous one).
    fn nearest_live(rows: &[Row], at: usize) -> Option<usize> {
        let at = at.min(rows.len().checked_sub(1)?);
        (at..rows.len())
            .find(|&i| !rows[i].deleted_pending)
            .or_else(|| (0..at).rev().find(|&i| !rows[i].deleted_pending))
    }

    /// `delta` live rows from `from`, stopping at the ends; "Deleted · Undo" rows are stepped over.
    fn step_live(rows: &[Row], from: usize, delta: isize) -> usize {
        let mut at = from;
        let dir: isize = if delta < 0 { -1 } else { 1 };
        for _ in 0..delta.unsigned_abs() {
            let mut next = at.checked_add_signed(dir);
            while let Some(n) = next.filter(|n| *n < rows.len()) {
                if !rows[n].deleted_pending {
                    at = n;
                    break;
                }
                next = n.checked_add_signed(dir);
            }
        }
        at
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

    fn row(&self, item: &Item, position: usize, mixed: bool, hit_tag: Option<&str>) -> Row {
        let title = if item.title.trim().is_empty() {
            item.preview.lines().next().unwrap_or_default().to_owned()
        } else {
            item.title.clone()
        };
        let marks = if mixed {
            search_mark::marks(&title, &item.preview, item.sensitive, hit_tag, &self.query)
        } else {
            search_mark::Marks::default()
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
            title_mark: marks.title,
            preview_mark: marks.preview,
            tag_hit: marks.tag,
            list: mixed
                .then(|| self.list_labels.get(&item.list_id).cloned())
                .flatten(),
            pinned: item.pinned,
            label: item.label,
            marked: self.marked.contains(&item.id),
            deleted_pending: false,
        }
    }

    /// The keyboard-selected row.
    #[must_use]
    pub fn selected(&self) -> Option<ItemId> {
        let rows = self.rows();
        Self::nearest_live(&rows, self.selected).map(|i| rows[i].id)
    }

    /// The position of the keyboard-selected row in [`rows`](Self::rows).
    #[must_use]
    pub fn selected_position(&self) -> Option<usize> {
        Self::nearest_live(&self.rows(), self.selected)
    }

    /// Puts the selection on item `id` if it is among the rows shown.
    pub fn select_item(&mut self, id: ItemId) {
        if let Some(at) = self.rows().iter().position(|r| r.id == id) {
            self.selected = at;
        }
    }

    /// Moves the selection by `delta` rows, stopping at the ends.
    pub fn move_selection(&mut self, delta: isize) {
        let rows = self.rows();
        let Some(from) = Self::nearest_live(&rows, self.selected) else {
            self.selected = 0;
            return;
        };
        self.selected = Self::step_live(&rows, from, delta);
    }

    /// Applies a click or key on row `index` to the multi-selection (the model Main's `pick-mode`
    /// uses): replace, toggle, or extend from the anchor. The keyboard cursor follows. An index
    /// past the end is ignored.
    pub fn mark(&mut self, index: usize, mode: SelectMode) {
        let rows = self.rows();
        if rows.get(index).is_none_or(|r| r.deleted_pending) {
            return;
        }
        let ids: Vec<ItemId> = rows.iter().map(|r| r.id).collect();
        let id = ids[index];
        match mode {
            SelectMode::Replace => {
                self.marked = HashSet::from([id]);
                self.anchor = Some(id);
            }
            SelectMode::Toggle => {
                if !self.marked.remove(&id) {
                    self.marked.insert(id);
                }
                self.anchor = Some(id);
            }
            SelectMode::Extend => {
                let from = self
                    .anchor
                    .and_then(|a| ids.iter().position(|i| *i == a))
                    .unwrap_or(self.selected);
                let (lo, hi) = (from.min(index), from.max(index));
                self.marked = rows[lo..=hi]
                    .iter()
                    .filter(|r| !r.deleted_pending)
                    .map(|r| r.id)
                    .collect();
                if self.anchor.is_none() {
                    self.anchor = ids.get(from).copied();
                }
            }
        }
        self.selected = index;
    }

    /// Shift+Down / Shift+Up: moves the cursor by `delta` rows and extends the selection to it
    /// from the anchor (the cursor row when nothing is marked yet).
    pub fn extend_marks(&mut self, delta: isize) {
        let rows = self.rows();
        let Some(at) = Self::nearest_live(&rows, self.selected) else {
            return;
        };
        if self.marked.is_empty() {
            self.mark(at, SelectMode::Replace);
        }
        self.mark(Self::step_live(&rows, at, delta), SelectMode::Extend);
    }

    /// Ctrl+A: marks every row shown.
    pub fn mark_all(&mut self) {
        let rows: Vec<Row> = self
            .rows()
            .into_iter()
            .filter(|r| !r.deleted_pending)
            .collect();
        self.marked = rows.iter().map(|r| r.id).collect();
        self.anchor = rows.first().map(|r| r.id);
    }

    /// Esc: marks nothing.
    pub fn clear_marks(&mut self) {
        self.marked.clear();
        self.anchor = None;
    }

    /// How many rows are marked.
    #[must_use]
    pub fn marked_count(&self) -> usize {
        self.rows().iter().filter(|r| r.marked).count()
    }

    /// The marked items in row order.
    #[must_use]
    pub fn marked_ids(&self) -> Vec<ItemId> {
        self.rows()
            .iter()
            .filter(|r| r.marked)
            .map(|r| r.id)
            .collect()
    }

    /// What an action on row `index` applies to: the marked rows when `index` is one of them,
    /// else just that row.
    #[must_use]
    pub fn targets(&self, index: usize) -> Vec<ItemId> {
        let rows = self.rows();
        match rows.get(index) {
            Some(row) if row.deleted_pending => Vec::new(),
            Some(row) if row.marked => rows.iter().filter(|r| r.marked).map(|r| r.id).collect(),
            Some(row) => vec![row.id],
            None => Vec::new(),
        }
    }

    /// The row for quick-use number `n` (1 to 9), if the list has it.
    #[must_use]
    pub fn quick_use(&self, n: u8) -> Option<ItemId> {
        if !(1..=9).contains(&n) {
            return None;
        }
        self.rows()
            .iter()
            .find(|r| r.number == n && !r.deleted_pending)
            .map(|r| r.id)
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

    /// Sets the paste key of each program (`Settings::paste_rules`); programs without one get
    /// `Ctrl+V`.
    pub fn set_paste_rules(&mut self, rules: Vec<PasteRule>) {
        self.paste_rules = rules;
    }

    /// What a click would do now.
    #[must_use]
    pub fn route(&self) -> Route {
        decide_with(self.target.as_ref(), &self.policy, &self.paste_rules)
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
    use crate::controller::Library as _;
    use snipdeck_core::{Content, Format, Formats, Inline, Name, Placement, Tag, Timestamp};
    use snipdeck_platform::route::Focus;
    use snipdeck_store::Store;
    use snipdeck_store::TrashRepo as _;

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

    // A25 guard: the index is built inside the first load (measured well under 50 ms, so it is
    // not deferred). Whatever happens to that timing later, a search typed the moment the model
    // exists must find the library, never an empty "not ready yet" result.
    #[test]
    fn a_search_before_the_index_is_ready_still_finds() {
        let mut f = fixture();
        let id = add_to(&mut f, Which::Replies, "Thanks", "Thanks for your email");
        let cache = IndexCache::new();
        assert_eq!(
            cache.rebuilds(),
            0,
            "nothing is built before the first load"
        );
        let mut model = PanelModel::load_cached(&f.store, None, None, &cache).expect("model");
        model.set_query("thank");
        let rows = model.rows();
        assert_eq!(rows.first().map(|r| r.id), Some(id));
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
        assert_eq!(
            rows[0].title_mark,
            Some((String::new(), "Docs".into())),
            "the match in the title is marked, in the title's own case"
        );
        assert!(
            rows[0].list.as_ref().is_some_and(|l| !l.board.is_empty()),
            "a result says which board it lives on"
        );
        model.set_query("   ");
        assert!(!model.searching());
        assert!(
            model
                .rows()
                .iter()
                .all(|r| r.list.is_none() && r.title_mark.is_none())
        );
    }

    #[test]
    fn a_search_marks_the_preview_and_names_a_tag_only_when_just_the_tag_matched() {
        let mut f = fixture();
        let both = add_to(
            &mut f,
            Which::Replies,
            "Greeting",
            "Hello, Invoice 7 is due",
        );
        let tagged = add_to(&mut f, Which::Replies, "Other", "nothing here");
        let secret = add_to(&mut f, Which::Replies, "Key", "Invoice secret");
        let tag = Tag::new(Name::new("invoice").expect("name"));
        f.store.insert_tag(&tag).expect("tag");
        for id in [tagged, secret] {
            let mut item = f.store.item(id).expect("item");
            item.tags.insert(tag.id);
            if id == secret {
                item.sensitive = true;
                item.preview.clear();
            }
            f.store.update_item(&item, None).expect("update");
        }
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.set_query("invoice");
        let rows = model.rows();
        let row = |id| rows.iter().find(|r| r.id == id).expect("row");
        assert_eq!(
            row(both).preview_mark,
            Some(("Hello, ".into(), "Invoice".into(), " 7 is due".into()))
        );
        assert_eq!(row(both).tag_hit, None);
        assert_eq!(row(tagged).preview_mark, None);
        assert_eq!(row(tagged).tag_hit.as_deref(), Some("invoice"));
        assert_eq!(
            row(secret).preview_mark,
            None,
            "a sensitive row shows no mark"
        );
        assert_eq!(row(secret).tag_hit, None, "nor the tag that matched");
        model.set_query("");
        assert!(
            model
                .rows()
                .iter()
                .all(|r| r.preview_mark.is_none() && r.tag_hit.is_none())
        );
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
    fn a_programs_own_paste_key_decides_the_route() {
        use snipdeck_platform::keys::PasteKey;
        let f = fixture();
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.set_target(Some(target("wt.exe", Focus::Caret)));
        let key = |model: &PanelModel| match model.route() {
            Route::Paste { key } => Some(key),
            Route::Copy(_) => None,
        };
        assert_eq!(key(&model), Some(PasteKey::CtrlV), "nothing says otherwise");
        model.set_paste_rules(vec![PasteRule {
            exe: "WT.exe".into(),
            key: PasteKey::CtrlShiftV,
        }]);
        assert_eq!(key(&model), Some(PasteKey::CtrlShiftV));
        // The built-in terminals are rules in the settings, not a second table.
        model.set_target(Some(target("mintty.exe", Focus::Caret)));
        assert_eq!(key(&model), Some(PasteKey::CtrlV));
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

    fn five(f: &mut Fixture) -> Vec<ItemId> {
        (0..5)
            .map(|n| add_to(f, Which::Replies, &format!("Item {n}"), "text"))
            .collect()
    }

    #[test]
    fn ctrl_click_toggles_and_shift_click_extends_from_the_anchor() {
        let mut f = fixture();
        let ids = five(&mut f);
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        assert_eq!(model.marked_count(), 0);
        model.mark(1, SelectMode::Toggle);
        model.mark(3, SelectMode::Toggle);
        assert_eq!(model.marked_ids(), vec![ids[1], ids[3]]);
        model.mark(1, SelectMode::Toggle);
        assert_eq!(
            model.marked_ids(),
            vec![ids[3]],
            "a second Ctrl+click unmarks"
        );
        model.mark(0, SelectMode::Extend);
        assert_eq!(
            model.marked_ids(),
            ids[0..=1].to_vec(),
            "from the anchor (the last click, row 1) up to row 0"
        );
        assert!(model.rows()[1].marked);
    }

    #[test]
    fn shift_arrows_extend_from_the_cursor_row_and_stop_at_the_ends() {
        let mut f = fixture();
        let ids = five(&mut f);
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.extend_marks(1);
        model.extend_marks(1);
        assert_eq!(model.marked_ids(), ids[0..=2].to_vec());
        assert_eq!(model.selected(), Some(ids[2]));
        model.extend_marks(-1);
        assert_eq!(
            model.marked_ids(),
            ids[0..=1].to_vec(),
            "shrinks back toward the anchor"
        );
        for _ in 0..9 {
            model.extend_marks(1);
        }
        assert_eq!(model.marked_count(), 5);
    }

    #[test]
    fn select_all_and_clear_and_a_search_end_the_selection() {
        let mut f = fixture();
        five(&mut f);
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.mark_all();
        assert_eq!(model.marked_count(), 5);
        model.clear_marks();
        assert_eq!(model.marked_count(), 0);
        model.mark_all();
        model.set_query("Item 2");
        assert_eq!(model.marked_count(), 0, "typing a query clears the marks");
    }

    #[test]
    fn an_action_on_a_marked_row_applies_to_all_marked_rows() {
        let mut f = fixture();
        let ids = five(&mut f);
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.mark(1, SelectMode::Toggle);
        model.mark(2, SelectMode::Toggle);
        assert_eq!(model.targets(2), vec![ids[1], ids[2]]);
        assert_eq!(model.targets(4), vec![ids[4]], "an unmarked row acts alone");
    }

    #[test]
    fn a_refresh_keeps_the_marks_of_items_still_shown() {
        let mut f = fixture();
        let ids = five(&mut f);
        let mut model = PanelModel::load(&f.store, None, None).expect("model");
        model.mark(0, SelectMode::Toggle);
        model.mark(1, SelectMode::Toggle);
        f.store
            .atomically(|b| b.trash(snipdeck_store::Trashable::Item(ids[0]), at()))
            .expect("trash");
        model.refresh(&f.store).expect("refresh");
        assert_eq!(model.marked_ids(), vec![ids[1]]);
    }
}
