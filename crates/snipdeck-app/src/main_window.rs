//! The Main window's state (SPEC §5.3, DESIGN.md "Main window (3-pane)"): which board and
//! sidebar section are open, the search, the rows, the multi-selection and the keyboard cursor.
//! Pure data: [`main_binding`](crate::main_binding) copies it into Slint models, and the
//! controller's `main_*` methods change it and the library.
//!
//! The Panel works on one board and one list; the Main window sees the whole library: *All
//! items*, *Pinned* and the *Trash* span every board, the lists in the sidebar belong to the
//! open board, and a search covers every board through the store's full-text index (SPEC §5.1).

use std::collections::{HashMap, HashSet};

use snipdeck_core::{
    Board, BoardId, Item, ItemId, ItemKind, List, ListAccent, ListId, ListSort, Placement, Tag,
    TagId,
};
use snipdeck_store::{
    BoardRepo, ItemRepo, ListRepo, TagRepo, TrashEntry, TrashRepo, Trashable,
};

use crate::panel::ListLabel;

/// How many days the Trash keeps things (SPEC §5.7a).
pub const TRASH_DAYS: u32 = 30;
/// How many search results the Main window shows.
const SEARCH_LIMIT: usize = 200;
/// The preview of a trashed list.
pub const LIST_NOTE: &str = "List";
/// The preview of a trashed board.
pub const BOARD_NOTE: &str = "Board";

/// Something a row stands for: an item, or (in the Trash view only) a trashed list or board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// An item.
    Item(ItemId),
    /// A list, in the Trash.
    List(ListId),
    /// A board, in the Trash.
    Board(BoardId),
}

impl From<Key> for Trashable {
    fn from(key: Key) -> Self {
        match key {
            Key::Item(id) => Self::Item(id),
            Key::List(id) => Self::List(id),
            Key::Board(id) => Self::Board(id),
        }
    }
}

impl From<Trashable> for Key {
    fn from(target: Trashable) -> Self {
        match target {
            Trashable::Item(id) => Self::Item(id),
            Trashable::List(id) => Self::List(id),
            Trashable::Board(id) => Self::Board(id),
        }
    }
}

/// What the sidebar has open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Every item of every board.
    All,
    /// The pinned items.
    Pinned,
    /// The Recent inbox (opt-in clipboard history, not built yet: always empty).
    Recent,
    /// Everything deleted in the last [`TRASH_DAYS`] days.
    Trash,
    /// One list of the open board.
    List(ListId),
    /// The items with one tag.
    Tag(TagId),
}

/// How the item area draws its rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Two or three preview lines.
    Comfortable,
    /// One line.
    Compact,
    /// A grid of tiles.
    Tile,
}

/// How a click or key changes the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectMode {
    /// Only this row.
    Replace,
    /// Flip this row.
    Toggle,
    /// Everything from the anchor to this row.
    Extend,
}

/// One row of the item area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What it stands for.
    pub key: Key,
    /// What the item holds.
    pub kind: ItemKind,
    /// Masked in the card.
    pub sensitive: bool,
    /// The title, or the first line of the preview when the item has none.
    pub title: String,
    /// The text preview (empty for sensitive items); a trashed list or board says which it is.
    pub preview: String,
    /// In mixed views: the list the item lives in.
    pub list: Option<ListLabel>,
    /// Pinned.
    pub pinned: bool,
    /// In the selection.
    pub selected: bool,
}

/// A list in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideList {
    /// The list.
    pub id: ListId,
    /// Its name.
    pub name: String,
    /// Its colour.
    pub accent: Option<ListAccent>,
    /// How many items it holds.
    pub count: usize,
}

/// A tag in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideTag {
    /// The tag.
    pub id: TagId,
    /// Its name.
    pub name: String,
    /// How many items carry it.
    pub count: usize,
}

/// The numbers next to the system views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// All items.
    pub all: usize,
    /// Pinned items.
    pub pinned: usize,
    /// The Recent inbox.
    pub recent: usize,
    /// Trash entries.
    pub trash: usize,
}

/// Why a name is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// Nothing but spaces.
    Empty,
    /// Longer than `max` characters.
    TooLong {
        /// The limit.
        max: usize,
    },
    /// A line break or other control character.
    ControlChar,
}

impl NameError {
    /// The reason `error` (from `Name::new`) gives, or `None` for an error that is not about
    /// the name's shape.
    #[must_use]
    pub fn of(error: &snipdeck_core::Error) -> Option<Self> {
        match error {
            snipdeck_core::Error::EmptyName => Some(Self::Empty),
            snipdeck_core::Error::NameTooLong { max } => Some(Self::TooLong { max: *max }),
            snipdeck_core::Error::NameHasControlChar => Some(Self::ControlChar),
            _ => None,
        }
    }
}

/// Why an editor's Delete is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteHint {
    /// A library keeps at least one board.
    LastBoard,
}

/// Which editor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKind {
    /// The list editor.
    List,
    /// The board editor.
    Board,
}

/// What an open list or board editor shows. `seq` changes with every editor that opens, so
/// the window sets the fields for a new one and leaves a typing user's text alone on a redraw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorView {
    /// Changes with every editor that opens.
    pub seq: u64,
    /// List or board.
    pub kind: EditorKind,
    /// Making a new one.
    pub is_new: bool,
    /// The name it opens with.
    pub name: String,
    /// A list's colour.
    pub accent: Option<ListAccent>,
    /// A list's item order.
    pub sort: ListSort,
    /// How many items deleting it affects.
    pub item_count: usize,
    /// Whether it can be deleted.
    pub can_delete: bool,
    /// Why not.
    pub delete_hint: Option<DeleteHint>,
    /// Why the last save was refused.
    pub name_error: Option<NameError>,
    /// The delete confirmation opens with the window (the board menu's "Delete board…").
    pub open_delete: bool,
}

/// The Main window's state.
#[derive(Debug)]
pub struct MainModel {
    stale: bool,
    boards: Vec<Board>,
    board: Option<BoardId>,
    lists: Vec<List>,
    list_counts: HashMap<ListId, usize>,
    tags: Vec<Tag>,
    tag_counts: HashMap<TagId, usize>,
    items: Vec<Item>,
    list_labels: HashMap<ListId, ListLabel>,
    trash: Vec<TrashEntry>,
    section: Section,
    query: String,
    hits: Vec<ItemId>,
    view: ViewMode,
    selected: HashSet<Key>,
    anchor: Option<Key>,
    cursor: usize,
    /// The rows a Move / Tags chooser acts on, fixed when it opened.
    chooser: Vec<Key>,
}

impl Default for MainModel {
    fn default() -> Self {
        Self::new()
    }
}

/// A row's source, before it becomes text.
enum Entry<'a> {
    Item(&'a Item),
    Trashed(&'a TrashEntry),
}

impl MainModel {
    /// An empty model that loads on the first [`reload`](Self::reload).
    #[must_use]
    pub fn new() -> Self {
        Self {
            stale: true,
            boards: Vec::new(),
            board: None,
            lists: Vec::new(),
            list_counts: HashMap::new(),
            tags: Vec::new(),
            tag_counts: HashMap::new(),
            items: Vec::new(),
            list_labels: HashMap::new(),
            trash: Vec::new(),
            section: Section::All,
            query: String::new(),
            hits: Vec::new(),
            view: ViewMode::Comfortable,
            selected: HashSet::new(),
            anchor: None,
            cursor: 0,
            chooser: Vec::new(),
        }
    }

    /// Whether the library changed since the model last read it.
    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// Says the library changed: the next [`reload`](Self::reload) is due.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Gives up on a reload that failed, so it is not tried on every redraw.
    pub fn mark_fresh(&mut self) {
        self.stale = false;
    }

    /// Reads the library again, keeping the open board and section, the search, the selection
    /// (minus rows that are gone) and the cursor. A board, list or tag that no longer exists
    /// falls back to the first board or *All items*.
    ///
    /// # Errors
    ///
    /// Any store error; the model is left as it was.
    pub fn reload<S>(&mut self, store: &S) -> snipdeck_store::Result<()>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo,
    {
        let boards = store.boards()?;
        let board = self
            .board
            .filter(|id| boards.iter().any(|b| b.id == *id))
            .or_else(|| boards.first().map(|b| b.id));
        let lists = match board {
            Some(id) => store.lists(id)?,
            None => Vec::new(),
        };
        let items = store.all_items()?;
        let tags = store.tags()?;
        let trash = store.trashed()?;
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
        let hits = if self.searching() {
            store.search(self.query.trim(), SEARCH_LIMIT)?
        } else {
            Vec::new()
        };
        let mut list_counts: HashMap<ListId, usize> = HashMap::new();
        let mut tag_counts: HashMap<TagId, usize> = HashMap::new();
        for item in &items {
            *list_counts.entry(item.list_id).or_default() += 1;
            for tag in &item.tags {
                *tag_counts.entry(*tag).or_default() += 1;
            }
        }
        self.boards = boards;
        self.board = board;
        self.lists = lists;
        self.list_counts = list_counts;
        self.tags = tags;
        self.tag_counts = tag_counts;
        self.items = items;
        self.list_labels = list_labels;
        self.trash = trash;
        self.hits = hits;
        self.section = match self.section {
            Section::List(id) if !self.lists.iter().any(|l| l.id == id) => Section::All,
            Section::Tag(id) if !self.tags.iter().any(|t| t.id == id) => Section::All,
            other => other,
        };
        let keys: HashSet<Key> = self.keys().into_iter().collect();
        self.selected.retain(|k| keys.contains(k));
        self.chooser.retain(|k| keys.contains(k));
        if self.anchor.is_some_and(|a| !keys.contains(&a)) {
            self.anchor = None;
        }
        self.cursor = self.cursor.min(keys.len().saturating_sub(1));
        self.stale = false;
        Ok(())
    }

    // ---- What is shown -------------------------------------------------------------------------

    /// The boards, in order.
    #[must_use]
    pub fn boards(&self) -> &[Board] {
        &self.boards
    }

    /// The open board.
    #[must_use]
    pub fn board(&self) -> Option<BoardId> {
        self.board
    }

    /// The open board's name (empty without boards).
    #[must_use]
    pub fn board_name(&self) -> &str {
        self.board
            .and_then(|id| self.boards.iter().find(|b| b.id == id))
            .map_or("", |b| b.name.as_str())
    }

    /// Where the open board is in [`boards`](Self::boards).
    #[must_use]
    pub fn board_position(&self) -> Option<usize> {
        self.board
            .and_then(|id| self.boards.iter().position(|b| b.id == id))
    }

    /// The lists of the open board.
    #[must_use]
    pub fn lists(&self) -> &[List] {
        &self.lists
    }

    /// The lists of the open board as the sidebar shows them.
    #[must_use]
    pub fn side_lists(&self) -> Vec<SideList> {
        self.lists
            .iter()
            .map(|l| SideList {
                id: l.id,
                name: l.name.as_str().to_owned(),
                accent: l.accent,
                count: self.list_counts.get(&l.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// The tags as the sidebar shows them.
    #[must_use]
    pub fn side_tags(&self) -> Vec<SideTag> {
        self.tags
            .iter()
            .map(|t| SideTag {
                id: t.id,
                name: t.name.as_str().to_owned(),
                count: self.tag_counts.get(&t.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// The tags.
    #[must_use]
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    /// The numbers next to *All items*, *Pinned*, *Recent* and *Trash*.
    #[must_use]
    pub fn counts(&self) -> Counts {
        Counts {
            all: self.items.len(),
            pinned: self.items.iter().filter(|i| i.pinned).count(),
            recent: 0,
            trash: self.trash.len(),
        }
    }

    /// What the sidebar has open.
    #[must_use]
    pub fn section(&self) -> Section {
        self.section
    }

    /// The index into [`side_lists`](Self::side_lists) or [`side_tags`](Self::side_tags) that
    /// the open section names (0 for the system views).
    #[must_use]
    pub fn section_index(&self) -> usize {
        match self.section {
            Section::List(id) => self.lists.iter().position(|l| l.id == id).unwrap_or(0),
            Section::Tag(id) => self.tags.iter().position(|t| t.id == id).unwrap_or(0),
            _ => 0,
        }
    }

    /// The list open in the sidebar, if one is.
    #[must_use]
    pub fn open_list(&self) -> Option<&List> {
        match self.section {
            Section::List(id) => self.lists.iter().find(|l| l.id == id),
            _ => None,
        }
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether a search is active.
    #[must_use]
    pub fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// The row style.
    #[must_use]
    pub fn view_mode(&self) -> ViewMode {
        self.view
    }

    /// The keyboard cursor.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// How many rows are selected.
    #[must_use]
    pub fn selected_count(&self) -> usize {
        self.keys()
            .iter()
            .filter(|k| self.selected.contains(k))
            .count()
    }

    fn entries(&self) -> Vec<Entry<'_>> {
        if self.section == Section::Trash {
            let needle = self.query.trim().to_lowercase();
            return self
                .trash
                .iter()
                .filter(|t| needle.is_empty() || t.label.to_lowercase().contains(&needle))
                .map(Entry::Trashed)
                .collect();
        }
        if self.searching() {
            let by_id: HashMap<ItemId, &Item> = self.items.iter().map(|i| (i.id, i)).collect();
            return self
                .hits
                .iter()
                .filter_map(|id| by_id.get(id).copied())
                .map(Entry::Item)
                .collect();
        }
        let items: Vec<&Item> = match self.section {
            Section::All => self.items.iter().collect(),
            Section::Pinned => self.items.iter().filter(|i| i.pinned).collect(),
            Section::Recent | Section::Trash => Vec::new(),
            Section::List(id) => self.list_items(id),
            Section::Tag(id) => self.items.iter().filter(|i| i.tags.contains(&id)).collect(),
        };
        items.into_iter().map(Entry::Item).collect()
    }

    /// The items of list `id` in the list's own order (manual, most used or recently used).
    fn list_items(&self, id: ListId) -> Vec<&Item> {
        let mut items: Vec<&Item> = self.items.iter().filter(|i| i.list_id == id).collect();
        match self.lists.iter().find(|l| l.id == id).map(|l| l.sort) {
            Some(ListSort::MostUsed) => items.sort_by_key(|i| std::cmp::Reverse(i.use_count)),
            Some(ListSort::RecentlyUsed) => {
                items.sort_by_key(|i| std::cmp::Reverse(i.last_used_at));
            }
            _ => {}
        }
        items
    }

    /// What the rows stand for, in order.
    #[must_use]
    pub fn keys(&self) -> Vec<Key> {
        self.entries()
            .into_iter()
            .map(|e| match e {
                Entry::Item(i) => Key::Item(i.id),
                Entry::Trashed(t) => t.target.into(),
            })
            .collect()
    }

    /// The rows of the open section (or of the search).
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mixed = self.searching() || !matches!(self.section, Section::List(_));
        self.entries()
            .into_iter()
            .map(|entry| match entry {
                Entry::Item(item) => Row {
                    key: Key::Item(item.id),
                    kind: item.kind,
                    sensitive: item.sensitive,
                    title: if item.title.trim().is_empty() {
                        item.preview.lines().next().unwrap_or_default().to_owned()
                    } else {
                        item.title.clone()
                    },
                    preview: item.preview.clone(),
                    list: mixed
                        .then(|| self.list_labels.get(&item.list_id).cloned())
                        .flatten(),
                    pinned: item.pinned,
                    selected: self.selected.contains(&Key::Item(item.id)),
                },
                Entry::Trashed(entry) => {
                    let key = Key::from(entry.target);
                    Row {
                        key,
                        kind: ItemKind::Text,
                        sensitive: false,
                        title: entry.label.clone(),
                        preview: match key {
                            Key::Item(_) => String::new(),
                            Key::List(_) => LIST_NOTE.to_owned(),
                            Key::Board(_) => BOARD_NOTE.to_owned(),
                        },
                        list: None,
                        pinned: false,
                        selected: self.selected.contains(&key),
                    }
                }
            })
            .collect()
    }

    /// The item for `id`, from the loaded library.
    #[must_use]
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The loaded items in `list`, in the list's manual order.
    #[must_use]
    pub fn manual_order(&self, list: ListId) -> Vec<ItemId> {
        self.items
            .iter()
            .filter(|i| i.list_id == list)
            .map(|i| i.id)
            .collect()
    }

    /// Whether the row at the cursor can be moved by hand: the open list is in manual order
    /// and no search is running.
    #[must_use]
    pub fn reorderable(&self) -> bool {
        !self.searching()
            && self
                .open_list()
                .is_some_and(|l| l.sort == ListSort::Manual)
    }

    /// The trash entry for `key`.
    #[must_use]
    pub fn trash_entry(&self, key: Key) -> Option<&TrashEntry> {
        self.trash.iter().find(|t| Key::from(t.target) == key)
    }

    // ---- Changing what is shown ----------------------------------------------------------------

    /// Opens `section`: the search ends, the selection clears and the cursor returns to the top.
    pub fn open(&mut self, section: Section) {
        self.section = section;
        self.query.clear();
        self.hits.clear();
        self.clear_selection();
        self.cursor = 0;
    }

    /// Opens board `id` (it must be one of [`boards`](Self::boards)): its lists replace the
    /// sidebar's, and *All items* is shown.
    ///
    /// # Errors
    ///
    /// Any store error from reading the board's lists.
    pub fn open_board<S>(&mut self, store: &S, id: BoardId) -> snipdeck_store::Result<()>
    where
        S: BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo,
    {
        if !self.boards.iter().any(|b| b.id == id) {
            return Ok(());
        }
        self.board = Some(id);
        self.open(Section::All);
        self.reload(store)
    }

    /// Sets the search text and reads the matches (every board) from the store. The
    /// selection and cursor start over.
    ///
    /// # Errors
    ///
    /// Any store error from the search; the text is kept and the matches are empty.
    pub fn set_query<S: ItemRepo>(&mut self, store: &S, query: &str) -> snipdeck_store::Result<()> {
        query.clone_into(&mut self.query);
        self.hits.clear();
        self.clear_selection();
        self.cursor = 0;
        if self.searching() && self.section != Section::Trash {
            self.hits = store.search(self.query.trim(), SEARCH_LIMIT)?;
        }
        Ok(())
    }

    /// Sets the row style.
    pub fn set_view_mode(&mut self, view: ViewMode) {
        self.view = view;
    }

    /// Moves the keyboard cursor (clamped to the rows).
    pub fn set_cursor(&mut self, index: usize) {
        self.cursor = index.min(self.keys().len().saturating_sub(1));
    }

    /// Applies a click or key on row `index`; an index past the end is ignored.
    pub fn select(&mut self, index: usize, mode: SelectMode) {
        let keys = self.keys();
        let Some(&key) = keys.get(index) else {
            return;
        };
        match mode {
            SelectMode::Replace => {
                self.selected.clear();
                self.selected.insert(key);
                self.anchor = Some(key);
            }
            SelectMode::Toggle => {
                if !self.selected.remove(&key) {
                    self.selected.insert(key);
                }
                self.anchor = Some(key);
            }
            SelectMode::Extend => {
                let from = self
                    .anchor
                    .and_then(|a| keys.iter().position(|k| *k == a))
                    .unwrap_or(index);
                let (lo, hi) = (from.min(index), from.max(index));
                self.selected = keys[lo..=hi].iter().copied().collect();
            }
        }
        self.cursor = index;
    }

    /// Selects every row.
    pub fn select_all(&mut self) {
        self.selected = self.keys().into_iter().collect();
    }

    /// Selects nothing.
    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// What an action on row `index` applies to: the whole selection when more than one row is
    /// selected, else that row.
    #[must_use]
    pub fn targets(&self, index: usize) -> Vec<Key> {
        let keys = self.keys();
        let selected: Vec<Key> = keys
            .iter()
            .copied()
            .filter(|k| self.selected.contains(k))
            .collect();
        if selected.len() > 1 {
            selected
        } else {
            keys.get(index).copied().into_iter().collect()
        }
    }

    /// Remembers what the Move / Tags chooser that is opening acts on.
    pub fn set_chooser(&mut self, targets: Vec<Key>) {
        self.chooser = targets;
    }

    /// What the open chooser acts on.
    #[must_use]
    pub fn chooser(&self) -> &[Key] {
        &self.chooser
    }
}

/// Where moving the entry at `from` by `delta` places it among `order`, and its new position.
/// `None` when it would leave the ends (or `from` is not in `order`).
#[must_use]
pub fn reorder<I: Copy>(order: &[I], from: usize, delta: isize) -> Option<(usize, Placement<I>)> {
    let to = from.checked_add_signed(delta)?;
    if from >= order.len() || to >= order.len() || to == from {
        return None;
    }
    let mut rest: Vec<I> = order.to_vec();
    rest.remove(from);
    let placement = match to.checked_sub(1) {
        None => Placement::First,
        Some(before) => Placement::After(rest[before]),
    };
    Some((to, placement))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use snipdeck_core::{Content, Format, Formats, Inline, Name, Timestamp};
    use snipdeck_store::Store;

    fn now() -> Timestamp {
        "2026-10-17T09:00:00Z".parse().expect("timestamp")
    }

    fn loaded() -> (Store, MainModel) {
        let store = demo::store(now()).expect("demo store");
        let mut model = MainModel::new();
        model.reload(&store).expect("reload");
        (store, model)
    }

    fn titles(model: &MainModel) -> Vec<String> {
        model.rows().into_iter().map(|r| r.title).collect()
    }

    fn list_named(model: &MainModel, name: &str) -> ListId {
        model
            .lists()
            .iter()
            .find(|l| l.name.as_str() == name)
            .expect("list")
            .id
    }

    #[test]
    fn a_fresh_model_is_stale_until_it_loads() {
        let mut model = MainModel::new();
        assert!(model.is_stale());
        model.reload(&demo::store(now()).expect("store")).expect("reload");
        assert!(!model.is_stale());
        model.mark_stale();
        assert!(model.is_stale());
    }

    #[test]
    fn the_demo_library_has_the_sections_and_counts_the_sidebar_shows() {
        let (_store, model) = loaded();
        assert_eq!(model.board_name(), "Work");
        assert_eq!(
            model.counts(),
            Counts {
                all: 11,
                pinned: 3,
                recent: 0,
                trash: 0
            }
        );
        let lists = model.side_lists();
        assert_eq!(
            lists
                .iter()
                .map(|l| (l.name.as_str(), l.count))
                .collect::<Vec<_>>(),
            [("Email replies", 6), ("Addresses", 2), ("Code", 2)]
        );
        assert!(model.side_tags().is_empty());
        assert_eq!(model.section(), Section::All);
    }

    #[test]
    fn all_items_spans_every_board_and_mixed_rows_name_their_list() {
        let (_store, model) = loaded();
        let rows = model.rows();
        assert_eq!(rows.len(), 11);
        assert!(titles(&model).contains(&"Shopping list".to_owned()));
        let shopping = rows.iter().find(|r| r.title == "Shopping list").expect("row");
        assert_eq!(shopping.list.as_ref().expect("label").name, "Errands");
    }

    #[test]
    fn pinned_shows_the_pinned_items_of_every_board() {
        let (_store, mut model) = loaded();
        model.open(Section::Pinned);
        assert_eq!(
            titles(&model),
            ["Thanks, received", "Home address", "Retry helper"]
        );
    }

    #[test]
    fn recent_is_empty_until_the_inbox_exists() {
        let (_store, mut model) = loaded();
        model.open(Section::Recent);
        assert!(model.rows().is_empty());
        assert_eq!(model.counts().recent, 0);
    }

    #[test]
    fn a_list_shows_its_items_in_order_without_list_labels() {
        let (_store, mut model) = loaded();
        let addresses = list_named(&model, "Addresses");
        model.open(Section::List(addresses));
        assert_eq!(titles(&model), ["Home address", "Office"]);
        assert!(model.rows().iter().all(|r| r.list.is_none()));
        assert_eq!(model.section_index(), 1);
    }

    #[test]
    fn a_most_used_list_sorts_by_use_and_is_not_reorderable() {
        let (mut store, mut model) = loaded();
        let code = model.lists().iter().find(|l| l.name.as_str() == "Code").expect("list").clone();
        let mut sorted = code.clone();
        sorted.sort = ListSort::MostUsed;
        store.update_list(&sorted).expect("update");
        let second = store.items(code.id).expect("items")[1].id;
        store.record_use(second, now()).expect("use");
        model.reload(&store).expect("reload");
        model.open(Section::List(code.id));
        assert_eq!(titles(&model), ["Recent customers", "Retry helper"]);
        assert!(!model.reorderable());
    }

    #[test]
    fn a_tag_shows_the_items_that_carry_it() {
        let (mut store, mut model) = loaded();
        let tag = Tag::new(Name::new("invoices").expect("name"));
        store.insert_tag(&tag).expect("tag");
        let mut item = store
            .all_items()
            .expect("items")
            .into_iter()
            .find(|i| i.title == "Invoice reminder")
            .expect("item");
        item.tags.insert(tag.id);
        store.update_item(&item, None).expect("update");
        model.reload(&store).expect("reload");
        assert_eq!(model.side_tags()[0].count, 1);
        model.open(Section::Tag(tag.id));
        assert_eq!(titles(&model), ["Invoice reminder"]);
        assert_eq!(model.section_index(), 0);
    }

    #[test]
    fn the_trash_lists_entries_with_a_note_for_lists_and_boards() {
        let (mut store, mut model) = loaded();
        let item = store.all_items().expect("items")[0].id;
        let code = list_named(&model, "Code");
        store.trash(Trashable::Item(item), now()).expect("trash");
        store.trash(Trashable::List(code), now()).expect("trash");
        model.reload(&store).expect("reload");
        assert_eq!(model.counts().trash, 2);
        assert_eq!(model.counts().all, 11 - 1 - 2);
        model.open(Section::Trash);
        let rows = model.rows();
        assert_eq!(rows.len(), 2);
        let code_row = rows.iter().find(|r| r.title == "Code").expect("row");
        assert_eq!(code_row.preview, LIST_NOTE);
        assert!(matches!(code_row.key, Key::List(_)));
        // The sidebar no longer offers the trashed list.
        assert_eq!(model.side_lists().len(), 2);
    }

    #[test]
    fn searching_covers_every_board_and_ends_when_a_section_opens() {
        let (store, mut model) = loaded();
        model.open(Section::List(list_named(&model, "Code")));
        model.set_query(&store, "milk").expect("search");
        assert_eq!(titles(&model), ["Shopping list"], "a hit on another board");
        assert!(model.rows()[0].list.is_some(), "search rows are mixed");
        model.set_query(&store, "   ").expect("search");
        assert_eq!(titles(&model), ["Retry helper", "Recent customers"]);
        model.set_query(&store, "milk").expect("search");
        model.open(Section::All);
        assert_eq!(model.query(), "");
        assert_eq!(model.rows().len(), 11);
    }

    #[test]
    fn searching_the_trash_filters_its_entries_by_label() {
        let (mut store, mut model) = loaded();
        for item in store.all_items().expect("items").into_iter().take(2) {
            store.trash(Trashable::Item(item.id), now()).expect("trash");
        }
        model.reload(&store).expect("reload");
        model.open(Section::Trash);
        model.set_query(&store, "thanks").expect("search");
        assert_eq!(titles(&model), ["Thanks, received"]);
    }

    #[test]
    fn a_hostile_query_is_a_search_not_an_error() {
        let (store, mut model) = loaded();
        model.set_query(&store, "\"(*) AND NOT").expect("search");
        assert!(model.rows().len() <= 11);
    }

    #[test]
    fn replace_toggle_and_extend_build_the_selection() {
        let (_store, mut model) = loaded();
        model.select(2, SelectMode::Replace);
        assert_eq!(model.selected_count(), 1);
        model.select(4, SelectMode::Extend);
        assert_eq!(model.selected_count(), 3);
        assert_eq!(model.cursor(), 4);
        model.select(0, SelectMode::Toggle);
        assert_eq!(model.selected_count(), 4);
        model.select(0, SelectMode::Toggle);
        assert_eq!(model.selected_count(), 3);
        model.select(1, SelectMode::Replace);
        assert_eq!(model.selected_count(), 1);
        model.select_all();
        assert_eq!(model.selected_count(), 11);
        model.clear_selection();
        assert_eq!(model.selected_count(), 0);
        model.select(99, SelectMode::Replace);
        assert_eq!(model.selected_count(), 0, "past the end is ignored");
        assert!(model.rows().iter().all(|r| !r.selected));
    }

    #[test]
    fn an_action_applies_to_the_selection_when_it_is_larger_than_one() {
        let (_store, mut model) = loaded();
        let keys = model.keys();
        assert_eq!(model.targets(3), [keys[3]]);
        model.select(1, SelectMode::Replace);
        assert_eq!(model.targets(3), [keys[3]], "one selected row: the row asked about");
        model.select(2, SelectMode::Extend);
        assert_eq!(model.targets(5), [keys[1], keys[2]]);
    }

    #[test]
    fn a_reload_drops_selected_rows_that_are_gone_and_clamps_the_cursor() {
        let (mut store, mut model) = loaded();
        let keys = model.keys();
        model.select(0, SelectMode::Replace);
        model.select(10, SelectMode::Extend);
        assert_eq!(model.selected_count(), 11);
        let Key::Item(first) = keys[0] else {
            panic!("an item")
        };
        store.trash(Trashable::Item(first), now()).expect("trash");
        model.reload(&store).expect("reload");
        assert_eq!(model.selected_count(), 10);
        assert_eq!(model.cursor(), 9);
    }

    #[test]
    fn a_list_that_disappears_falls_back_to_all_items() {
        let (mut store, mut model) = loaded();
        let code = list_named(&model, "Code");
        model.open(Section::List(code));
        store.trash(Trashable::List(code), now()).expect("trash");
        model.reload(&store).expect("reload");
        assert_eq!(model.section(), Section::All);
    }

    #[test]
    fn opening_a_board_shows_its_lists() {
        let (store, mut model) = loaded();
        let personal = model.boards()[1].id;
        model.open_board(&store, personal).expect("board");
        assert_eq!(model.board_name(), "Personal");
        assert_eq!(model.side_lists().len(), 1);
        assert_eq!(model.board_position(), Some(1));
        assert_eq!(model.section(), Section::All);
    }

    #[test]
    fn a_pinned_flag_and_a_blank_title_show_in_the_row() {
        let (mut store, mut model) = loaded();
        let list = list_named(&model, "Addresses");
        let mut item = snipdeck_core::Item::new(list, String::new(), ItemKind::Text, now());
        item.preview = "first line\nsecond".into();
        let content = Content::Inline(Inline::new(b"first line\nsecond".to_vec()).expect("inline"));
        store
            .insert_item(
                &item,
                &Formats::new(vec![Format::Text(content)]).expect("formats"),
                Placement::Last,
            )
            .expect("insert");
        model.reload(&store).expect("reload");
        model.open(Section::List(list));
        assert_eq!(titles(&model)[2], "first line");
    }

    #[test]
    fn reorder_computes_the_placement_for_a_step_up_or_down() {
        let order = ['a', 'b', 'c', 'd'];
        assert_eq!(reorder(&order, 2, -1), Some((1, Placement::After('a'))));
        assert_eq!(reorder(&order, 1, -1), Some((0, Placement::First)));
        assert_eq!(reorder(&order, 0, 1), Some((1, Placement::After('b'))));
        assert_eq!(reorder(&order, 2, 1), Some((3, Placement::After('d'))));
        assert_eq!(reorder(&order, 0, -1), None, "off the top");
        assert_eq!(reorder(&order, 3, 1), None, "off the bottom");
        assert_eq!(reorder(&order, 9, -1), None, "not in the order");
        assert_eq!(reorder(&order, 1, 0), None, "no move");
    }
}
