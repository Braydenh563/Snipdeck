//! The Main window's state (SPEC §5.3, DESIGN.md "Main window (3-pane)"): which board and
//! sidebar section are open, the search, the rows, the multi-selection and the keyboard cursor.
//! Pure data: [`main_binding`](crate::main_binding) copies it into Slint models, and the
//! controller's `main_*` methods change it and the library.
//!
//! The Panel works on one board and one list; the Main window sees the whole library: *All
//! items*, *Pinned* and the *Trash* span every board, the lists in the sidebar belong to the
//! open board, and a search covers every board through the store's full-text index (SPEC §5.1).

use std::collections::{BTreeMap, HashMap, HashSet};

use snipdeck_core::search::{Hit, Query, SearchIndex};
use snipdeck_core::{
    Board, BoardId, GroupId, Item, ItemId, ItemKind, ItemLabel, List, ListAccent, ListIcon, ListId,
    Placement, Sort, Tag, TagId, TagTree, ViewFilter,
};
use snipdeck_store::{
    BoardRepo, GroupRepo, GroupRun, ItemRepo, ListRepo, TagRepo, TrashEntry, TrashRepo, Trashable,
    TriggerRepo,
};

use crate::group_walk::GroupWalk;
use crate::panel::{GroupHeader, ListLabel};
use crate::search_mark::{self, Marks};
use crate::settings::RowDensity;

/// How many search results the Main window shows.
const SEARCH_LIMIT: usize = 200;

/// The best [`SEARCH_LIMIT`] matches of `query`: `index` for the names and the start of the
/// content, the store's full-text index for the rest of it.
fn search<S: ItemRepo>(
    store: &S,
    index: &SearchIndex,
    query: &str,
) -> snipdeck_store::Result<Vec<Hit>> {
    let query = Query::parse(query);
    let body = store.search(&query)?;
    Ok(index.query(&query, &body, SEARCH_LIMIT))
}
/// One criterion of the view filter: a chip in the toolbar and a row of the Filter menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTerm {
    /// Items of this kind (several kinds are alternatives).
    Kind(ItemKind),
    /// Items with a typed trigger.
    HasTrigger,
    /// Sensitive items.
    Sensitive,
    /// Items with no tag.
    Untagged,
    /// Items with this tag (several tags are any or all, by the filter's match).
    Tag(TagId),
}

/// Whether a trash entry answers `query`: every wanted word is in its title or its preview and
/// no excluded one is (the same word matching as the search marks; filters need data a trash
/// entry does not carry and are not applied).
fn trash_matches(entry: &TrashEntry, query: &Query) -> bool {
    let found = |term: &snipdeck_core::search::Term| {
        !snipdeck_core::search::find(&entry.label, term, true).is_empty()
            || entry
                .preview
                .lines()
                .any(|line| !snipdeck_core::search::find(line, term, false).is_empty())
    };
    query.terms.iter().all(|t| found(t) != t.negated)
}

/// The preview of a trashed list.
pub const LIST_NOTE: &str = "List";
/// The preview of a trashed board.
pub const BOARD_NOTE: &str = "Board";

/// Something a row stands for: an item, a group's header, or (in the Trash view only) a trashed list or board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// An item.
    Item(ItemId),
    /// A list, in the Trash.
    List(ListId),
    /// A board, in the Trash.
    Board(BoardId),
    /// A group's header in a list in manual order: it is never selected or acted on like an item.
    Group(GroupId),
}

impl Key {
    /// What the Trash would hold for this row; a group header is not an entry.
    #[must_use]
    pub fn trashable(self) -> Option<Trashable> {
        match self {
            Self::Item(id) => Some(Trashable::Item(id)),
            Self::List(id) => Some(Trashable::List(id)),
            Self::Board(id) => Some(Trashable::Board(id)),
            Self::Group(_) => None,
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
    /// The Recent inbox (opt-in clipboard history). Its entries are not items: the controller lists
    /// them (see `recent_ctl.rs`), so the model's rows stay empty here.
    Recent,
    /// Everything deleted in the last `trash_days` days.
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

impl From<RowDensity> for ViewMode {
    fn from(density: RowDensity) -> Self {
        match density {
            RowDensity::Comfortable => Self::Comfortable,
            RowDensity::Compact => Self::Compact,
        }
    }
}

impl ViewMode {
    /// The density this style is, or `None` for tiles (a different layout, not a row height).
    #[must_use]
    pub const fn density(self) -> Option<RowDensity> {
        match self {
            Self::Comfortable => Some(RowDensity::Comfortable),
            Self::Compact => Some(RowDensity::Compact),
            Self::Tile => None,
        }
    }
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
    /// The item's colour label.
    pub label: Option<ItemLabel>,
    /// In the selection.
    pub selected: bool,
    /// While searching: where the query matched (shared-look spec decision 3), the matched tag
    /// included when the title and preview do not show the match.
    pub marks: Marks,
    /// The row is a group's header, not an item: `title` is its name.
    pub header: Option<GroupHeader>,
    /// How many groups the item sits in (0: not in one): the card is indented by one step each.
    pub depth: usize,
    /// An image item: when it last changed, which the thumbnail cache is keyed with.
    pub image_stamp: Option<snipdeck_core::Timestamp>,
    /// An image item: its thumbnail, once decoded (MAIN-21).
    pub thumb: Option<crate::thumb_cache::Thumb>,
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
    /// Its icon.
    pub icon: Option<ListIcon>,
    /// How many items it holds.
    pub count: usize,
}

/// What the Detail pane shows for the item at the cursor (SPEC 5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailView {
    /// The item.
    pub item: ItemId,
    /// What it holds.
    pub kind: ItemKind,
    /// Masked: the text is the mask until the item is held revealed.
    pub sensitive: bool,
    /// The title, or the first line of the text when there is none.
    pub title: String,
    /// The start of the text (the mask for a hidden secret, the file names for a file item).
    pub text: String,
    /// The text was cut: the item holds more.
    pub text_cut: bool,
    /// A picture item's picture, shrunk to fit the pane.
    pub thumb: Option<crate::thumb_cache::Thumb>,
    /// The formats the item keeps, in the item's order.
    pub formats: Vec<snipdeck_core::FormatKind>,
    /// How many times it was used.
    pub use_count: u32,
    /// When it was last used (empty when never), as local `YYYY-MM-DD HH:MM`.
    pub last_used: String,
    /// When it was made.
    pub created: String,
    /// When it last changed.
    pub updated: String,
    /// The program it was captured from.
    pub source_app: String,
    /// The list it lives in (with its board once there are several).
    pub list_name: String,
    /// Its tags, comma separated.
    pub tags: String,
}

/// A list the *Move to list* chooser offers: lists of the open board first, then those of the
/// other boards in board order (MAIN-23).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveTarget {
    /// The list.
    pub list: ListId,
    /// The board it lives on.
    pub board: BoardId,
    /// What the chooser shows: the list's name, with its board in front for another board's list.
    pub label: String,
    /// Its colour.
    pub accent: Option<ListAccent>,
    /// Its icon.
    pub icon: Option<ListIcon>,
    /// How many items it holds.
    pub count: usize,
}

/// A chip of the colour-label filter row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabelChip {
    /// The label.
    pub label: ItemLabel,
    /// How many items of the open view carry it.
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
    /// Its colour (one of the eight item colours), if it has one.
    pub colour: Option<ItemLabel>,
    /// How many items carry it itself (`count` also counts the tags nested under it).
    pub own_count: usize,
    /// How many tags it is nested under (0 at the top).
    pub depth: usize,
    /// Its path with the names above it (`work/invoices`).
    pub path: String,
}

/// The numbers next to the system views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// All items.
    pub all: usize,
    /// Pinned items.
    pub pinned: usize,
    /// The Recent inbox (the controller fills this from the store; the model only holds items).
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
    /// A list's icon.
    pub icon: Option<ListIcon>,
    /// A list's item order.
    pub sort: Sort,
    /// The boards a list can live on, in board order (empty for a board editor).
    pub boards: Vec<(BoardId, String)>,
    /// The chosen board: a position in `boards`.
    pub board_index: usize,
    /// How many items deleting it affects.
    pub item_count: usize,
    /// Whether it can be deleted.
    pub can_delete: bool,
    /// Why not.
    pub delete_hint: Option<DeleteHint>,
    /// Why the last save was refused.
    pub name_error: Option<NameError>,
    /// Fields the owner typed before the window closed came back.
    pub restored: bool,
}

/// The Main window's state.
#[derive(Debug)]
pub struct MainModel {
    stale: bool,
    board_menu: bool,
    boards: Vec<Board>,
    board: Option<BoardId>,
    lists: Vec<List>,
    list_counts: HashMap<ListId, usize>,
    /// The tags in sidebar order (each with the tags nested under it right after it).
    tags: Vec<Tag>,
    tree: TagTree,
    tag_counts: HashMap<TagId, usize>,
    own_counts: HashMap<TagId, usize>,
    items: Vec<std::rc::Rc<Item>>,
    /// The library snapshot Main reads, shared with the Panel and the Picker: one read of the
    /// library, one search index, one set of suggestions for every window.
    cache: crate::index_cache::IndexCache,
    /// The groups of each list of the open board.
    groups: HashMap<ListId, Vec<GroupRun>>,
    /// Each grouped item's list and the index of its run in `groups` (built by `reload`).
    group_index: HashMap<ItemId, (ListId, usize)>,
    /// Bumped whenever something the rows come from changes (MAIN-29).
    generation: u64,
    /// The rows' sources for `generation`, so the keys and the rows of one redraw share one pass.
    entries_cache: std::cell::RefCell<Option<(u64, std::rc::Rc<Vec<Slot>>)>>,
    list_labels: HashMap<ListId, ListLabel>,
    /// The library snapshot read by `reload`; its suggestions are built on first use.
    snapshot: std::rc::Rc<crate::index_cache::Snapshot>,
    /// The lists of every board, in board order (for the Move chooser).
    board_lists: Vec<(BoardId, Vec<List>)>,
    trash: Vec<TrashEntry>,
    section: Section,
    query: String,
    /// The matches of the search (the snapshot's shared index, the same matcher as the Panel
    /// and the Picker).
    hits: Vec<Hit>,
    view: ViewMode,
    /// What the open view is narrowed to (colour, tags, kind, trigger, sensitive); every section
    /// shares it and a section change clears it.
    filter: ViewFilter,
    /// The items that have a typed trigger, for the filter.
    triggered: HashSet<ItemId>,
    /// The sort of each section that has none of its own in the library (All items, Pinned and
    /// each tag), by [`MainModel::sort_key`]; the controller loads it from the settings.
    sorts: BTreeMap<String, Sort>,
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

/// Where a row's source lives, so the list of rows can be kept between redraws.
#[derive(Debug, Clone, Copy)]
enum Slot {
    /// An item (its place among the items) and how many groups it sits in.
    Item(usize, usize),
    Trashed(usize),
    /// A group's header (its list, the run's place among the list's groups) and its depth.
    Header(ListId, usize, usize),
}

/// A row's source, before it becomes text.
enum Entry<'a> {
    /// An item, with how many groups it sits in.
    Item(&'a Item, usize),
    Trashed(&'a TrashEntry),
    /// A group's header above its members, with how many groups it is nested in.
    Header(&'a GroupRun, usize),
}

/// The row of a group's header.
fn header_row(run: &GroupRun, depth: usize) -> Row {
    Row {
        key: Key::Group(run.group.id),
        kind: ItemKind::Text,
        sensitive: false,
        title: run.group.name.as_str().to_owned(),
        preview: String::new(),
        list: None,
        pinned: false,
        label: None,
        selected: false,
        marks: Marks::default(),
        header: Some(GroupHeader {
            group: run.group.id,
            name: run.group.name.as_str().to_owned(),
            count: run.run.len(),
            collapsed: run.group.collapsed,
            colour: run.group.colour,
            depth,
        }),
        depth: 0,
        image_stamp: None,
        thumb: None,
    }
}

/// `tags` in sidebar order (each tag followed by the tags nested under it) with their nesting.
fn in_walk_order(mut tags: Vec<Tag>) -> (Vec<Tag>, TagTree) {
    let tree = TagTree::new(&tags);
    let order: HashMap<TagId, usize> = tree
        .walk()
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t, i))
        .collect();
    tags.sort_by_key(|t| order.get(&t.id).copied().unwrap_or(usize::MAX));
    (tags, tree)
}

/// What each list is called in a hit from another board: the board's name (empty while there
/// is one board, MAIN-1), the list's name and its accent.
fn list_labels_of(boards: &[(Board, Vec<List>)]) -> HashMap<ListId, ListLabel> {
    boards
        .iter()
        .flat_map(|(b, lists)| lists.iter().map(move |l| (b, l)))
        .map(|(b, l)| {
            (
                l.id,
                ListLabel {
                    board: if boards.len() > 1 {
                        b.name.as_str().to_owned()
                    } else {
                        String::new()
                    },
                    name: l.name.as_str().to_owned(),
                    accent: l.accent,
                },
            )
        })
        .collect()
}

/// How many items are in each list, carry each tag or any tag under it (each item once), and
/// carry each tag itself.
fn counts(
    items: &[std::rc::Rc<Item>],
    tree: &TagTree,
) -> (
    HashMap<ListId, usize>,
    HashMap<TagId, usize>,
    HashMap<TagId, usize>,
) {
    let mut list_counts: HashMap<ListId, usize> = HashMap::new();
    let mut tag_counts: HashMap<TagId, usize> = HashMap::new();
    let mut own_counts: HashMap<TagId, usize> = HashMap::new();
    for item in items {
        *list_counts.entry(item.list_id).or_default() += 1;
        let mut counted: HashSet<TagId> = HashSet::new();
        for tag in &item.tags {
            *own_counts.entry(*tag).or_default() += 1;
            counted.insert(*tag);
            counted.extend(tree.ancestors(*tag));
        }
        for tag in counted {
            *tag_counts.entry(tag).or_default() += 1;
        }
    }
    (list_counts, tag_counts, own_counts)
}

impl MainModel {
    /// An empty model that loads on the first [`reload`](Self::reload).
    #[must_use]
    pub fn new() -> Self {
        Self::with_cache(crate::index_cache::IndexCache::new())
    }

    /// An empty model that reads the library through `cache` (the one the Panel uses).
    #[must_use]
    pub fn with_cache(cache: crate::index_cache::IndexCache) -> Self {
        Self {
            cache,
            stale: true,
            board_menu: false,
            boards: Vec::new(),
            board: None,
            lists: Vec::new(),
            list_counts: HashMap::new(),
            tags: Vec::new(),
            tree: TagTree::default(),
            tag_counts: HashMap::new(),
            own_counts: HashMap::new(),
            items: Vec::new(),
            groups: HashMap::new(),
            group_index: HashMap::new(),
            generation: 0,
            entries_cache: std::cell::RefCell::new(None),
            list_labels: HashMap::new(),
            snapshot: std::rc::Rc::default(),
            board_lists: Vec::new(),
            trash: Vec::new(),
            section: Section::All,
            query: String::new(),
            hits: Vec::new(),
            view: ViewMode::Comfortable,
            filter: ViewFilter::none(),
            triggered: HashSet::new(),
            sorts: BTreeMap::new(),
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

    /// Opens or closes the board menu.
    pub fn set_board_menu(&mut self, open: bool) {
        self.board_menu = open;
    }

    /// Whether the board menu is open.
    #[must_use]
    pub fn board_menu_open(&self) -> bool {
        self.board_menu
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
        S: BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo + GroupRepo + TriggerRepo,
    {
        // One read for every window: the cache hands out the snapshot the Panel holds (read
        // again only when the library was written).
        let snapshot = self.cache.snapshot(store)?;
        let boards: Vec<Board> = snapshot.boards.iter().map(|(b, _)| b.clone()).collect();
        let board = self
            .board
            .filter(|id| boards.iter().any(|b| b.id == *id))
            .or_else(|| boards.first().map(|b| b.id));
        let lists = board
            .and_then(|id| snapshot.boards.iter().find(|(b, _)| b.id == id))
            .map_or_else(Vec::new, |(_, lists)| lists.clone());
        let items = snapshot.all.clone();
        let groups = lists
            .iter()
            .map(|l| Ok((l.id, store.groups_of(l.id)?)))
            .collect::<snipdeck_store::Result<HashMap<_, _>>>()?;
        let (tags, tree) = in_walk_order(store.tags()?);
        let triggered = store
            .triggers()?
            .into_iter()
            .map(|(_, item)| item)
            .collect();
        let trash = store.trashed()?;
        let board_lists: Vec<(BoardId, Vec<List>)> = snapshot
            .boards
            .iter()
            .map(|(b, lists)| (b.id, lists.clone()))
            .collect();
        let list_labels = list_labels_of(&snapshot.boards);
        let hits = if self.searching() {
            search(store, &snapshot.index(), &self.query)?
        } else {
            Vec::new()
        };
        let (list_counts, tag_counts, own_counts) = counts(&items, &tree);
        self.boards = boards;
        self.board = board;
        self.lists = lists;
        self.list_counts = list_counts;
        self.tags = tags;
        self.tree = tree;
        self.tag_counts = tag_counts;
        self.own_counts = own_counts;
        self.items = items;
        self.group_index = groups
            .iter()
            .flat_map(|(list, runs)| {
                runs.iter().enumerate().flat_map(move |(at, run)| {
                    run.items.iter().map(move |item| (*item, (*list, at)))
                })
            })
            .collect();
        self.groups = groups;
        self.generation += 1;
        self.list_labels = list_labels;
        self.board_lists = board_lists;
        self.trash = trash;
        self.triggered = triggered;
        self.hits = hits;
        self.snapshot = std::rc::Rc::clone(&snapshot);
        self.section = match self.section {
            Section::List(id) if !self.lists.iter().any(|l| l.id == id) => Section::All,
            Section::Tag(id) if !self.tags.iter().any(|t| t.id == id) => Section::All,
            other => other,
        };
        let gone: Vec<TagId> = self
            .filter
            .tags
            .iter()
            .filter(|t| !self.tags.iter().any(|known| known.id == **t))
            .copied()
            .collect();
        for tag in gone {
            self.filter.forget_tag(tag);
        }
        let keys: HashSet<Key> = self.keys().into_iter().collect();
        self.selected.retain(|k| keys.contains(k));
        // A chooser outlives its rows leaving the view (untagging in a tag's view), not the
        // items themselves going.
        let alive: HashSet<Key> = self
            .items
            .iter()
            .map(|i| Key::Item(i.id))
            .chain(self.trash.iter().map(|t| Key::from(t.target)))
            .collect();
        self.chooser.retain(|k| alive.contains(k));
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

    /// The board list `id` lives on.
    #[must_use]
    pub fn board_of_list(&self, id: ListId) -> Option<BoardId> {
        self.board_lists
            .iter()
            .find(|(_, lists)| lists.iter().any(|l| l.id == id))
            .map(|(b, _)| *b)
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
                icon: l.icon,
                count: self.list_counts.get(&l.id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// Every list the *Move to list* chooser offers: the open board's, then the other boards'
    /// (named with their board once the library has more than one).
    #[must_use]
    pub fn move_targets(&self) -> Vec<MoveTarget> {
        let several = self.boards.len() > 1;
        let open = self.board;
        let ordered = self
            .board_lists
            .iter()
            .filter(|(b, _)| Some(*b) == open)
            .chain(self.board_lists.iter().filter(|(b, _)| Some(*b) != open));
        ordered
            .flat_map(|(board, lists)| {
                let board_name = self
                    .boards
                    .iter()
                    .find(|b| b.id == *board)
                    .map_or("", |b| b.name.as_str());
                lists.iter().map(move |l| MoveTarget {
                    list: l.id,
                    board: *board,
                    label: if several && Some(*board) != open {
                        crate::picker_binding::picker_place(board_name, l.name.as_str())
                    } else {
                        l.name.as_str().to_owned()
                    },
                    accent: l.accent,
                    icon: l.icon,
                    count: self.list_counts.get(&l.id).copied().unwrap_or(0),
                })
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
                colour: t.colour,
                own_count: self.own_counts.get(&t.id).copied().unwrap_or(0),
                depth: self.tree.depth(t.id),
                path: self.tree.path(t.id),
            })
            .collect()
    }

    /// The nesting of the tags.
    #[must_use]
    pub fn tag_tree(&self) -> &TagTree {
        &self.tree
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

    /// The names the search suggestions are made of.
    #[must_use]
    pub fn suggest_index(&self) -> std::rc::Rc<crate::search_suggest::Index> {
        self.snapshot.suggest()
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether the search stopped at `SEARCH_LIMIT` hits, so more items match than are shown.
    #[must_use]
    pub fn search_capped(&self) -> bool {
        self.searching() && self.hits.len() >= SEARCH_LIMIT
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

    /// The items a drag that starts on row `row` carries: every selected item, in the order the
    /// rows are in, when `row` is one of several selected; else just that row's item.
    #[must_use]
    pub fn dragged_items(&self, row: usize) -> Vec<ItemId> {
        let keys = self.keys();
        let Some(&Key::Item(own)) = keys.get(row) else {
            return Vec::new();
        };
        if self.selected_count() < 2 || !self.selected.contains(&Key::Item(own)) {
            return vec![own];
        }
        keys.iter()
            .filter(|k| self.selected.contains(k))
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn entries(&self) -> Vec<Entry<'_>> {
        let slots = self.slots();
        slots
            .iter()
            .filter_map(|slot| match *slot {
                Slot::Item(at, depth) => self.items.get(at).map(|item| Entry::Item(item, depth)),
                Slot::Trashed(at) => self.trash.get(at).map(Entry::Trashed),
                Slot::Header(list, at, depth) => self
                    .groups
                    .get(&list)
                    .and_then(|runs| runs.get(at))
                    .map(|run| Entry::Header(run, depth)),
            })
            .collect()
    }

    /// The rows' sources, worked out once per change of what the rows come from.
    fn slots(&self) -> std::rc::Rc<Vec<Slot>> {
        if let Some((generation, slots)) = &*self.entries_cache.borrow()
            && *generation == self.generation
        {
            return std::rc::Rc::clone(slots);
        }
        let slots = std::rc::Rc::new(self.build_slots());
        *self.entries_cache.borrow_mut() = Some((self.generation, std::rc::Rc::clone(&slots)));
        slots
    }

    fn build_slots(&self) -> Vec<Slot> {
        if self.section == Section::Trash {
            let query = Query::parse(&self.query);
            return self
                .trash
                .iter()
                .enumerate()
                .filter(|(_, t)| trash_matches(t, &query))
                .map(|(at, _)| Slot::Trashed(at))
                .collect();
        }
        let by_id: HashMap<ItemId, usize> = self
            .items
            .iter()
            .enumerate()
            .map(|(at, i)| (i.id, at))
            .collect();
        let items = self
            .unfiltered_items()
            .into_iter()
            .filter(|i| {
                self.filter
                    .matches(i, self.triggered.contains(&i.id), &self.tree)
            })
            .filter_map(|i| by_id.get(&i.id).copied().map(|at| (at, i)));
        let Section::List(list) = self.section else {
            return items.map(|(at, _)| Slot::Item(at, 0)).collect();
        };
        if !self.shows_groups() {
            return items.map(|(at, _)| Slot::Item(at, 0)).collect();
        }
        // A group is a run of the list's blocks: its header goes above the first member (a nested
        // group's below its parent's), and a folded group shows the header only, with the groups
        // nested in it hidden too.
        let run_at: HashMap<GroupId, usize> = self
            .open_groups()
            .iter()
            .enumerate()
            .map(|(at, run)| (run.group.id, at))
            .collect();
        let mut slots = Vec::new();
        let mut walk = GroupWalk::new(self.open_groups());
        for (at, item) in items {
            let step = walk.step(item.id);
            for (run, depth) in step.headers {
                if let Some(&run_at) = run_at.get(&run.group.id) {
                    slots.push(Slot::Header(list, run_at, depth));
                }
            }
            if !step.hidden {
                slots.push(Slot::Item(at, step.depth));
            }
        }
        slots
    }

    /// Whether the rows carry group headers: the open section is one list, under any sort (the
    /// sort orders the rows inside each block), with no search and no filter (a filter would cut
    /// a group's run).
    #[must_use]
    pub fn shows_groups(&self) -> bool {
        !self.searching() && self.filter.is_none() && matches!(self.section, Section::List(_))
    }

    /// The open list's groups (none when it shows none).
    #[must_use]
    pub fn open_groups(&self) -> &[GroupRun] {
        match self.section {
            Section::List(id) if self.shows_groups() => {
                self.groups.get(&id).map_or(&[], Vec::as_slice)
            }
            _ => &[],
        }
    }

    /// Every group of the open list, folded or not (a folded one has no rows to find it by).
    pub fn all_groups(&self) -> impl Iterator<Item = &GroupRun> {
        match self.section {
            Section::List(id) => self.groups.get(&id).map_or(&[][..], Vec::as_slice),
            _ => &[],
        }
        .iter()
    }

    /// The group whose header is row `index`.
    #[must_use]
    pub fn group_at(&self, index: usize) -> Option<GroupRun> {
        let Some(Key::Group(id)) = self.keys().get(index).copied() else {
            return None;
        };
        self.open_groups()
            .iter()
            .find(|r| r.group.id == id)
            .cloned()
    }

    /// The group item `id` belongs to, in the open list.
    #[must_use]
    pub fn group_of(&self, id: ItemId) -> Option<&GroupRun> {
        let (list, at) = *self.group_index.get(&id)?;
        if self.section != Section::List(list) || !self.shows_groups() {
            return None;
        }
        self.groups.get(&list)?.get(at)
    }

    /// The items of the open view before the colour-label filter: the search hits while
    /// searching, else the section's items.
    fn unfiltered_items(&self) -> Vec<&Item> {
        if self.searching() {
            let by_id: HashMap<ItemId, &Item> = self.items.iter().map(|i| (i.id, &**i)).collect();
            return self
                .hits
                .iter()
                .filter_map(|hit| by_id.get(&hit.id).copied())
                .collect();
        }
        let mut items: Vec<&Item> = match self.section {
            Section::All => self.items.iter().map(|i| &**i).collect(),
            Section::Pinned => self
                .items
                .iter()
                .filter(|i| i.pinned)
                .map(|i| &**i)
                .collect(),
            Section::Recent | Section::Trash => return Vec::new(),
            Section::List(id) => return self.list_items(id),
            Section::Tag(id) => self
                .items
                .iter()
                .filter(|i| {
                    i.tags.contains(&id) || i.tags.iter().any(|own| self.tree.is_under(*own, id))
                })
                .map(|i| &**i)
                .collect(),
        };
        self.sort().apply(&mut items);
        items
    }

    /// The chips of the filter row: one per colour label some item of the open view carries
    /// (in the label order, with how many), plus the active filter even when nothing carries it,
    /// so it can always be cleared.
    #[must_use]
    pub fn label_chips(&self) -> Vec<LabelChip> {
        if self.section == Section::Trash {
            return Vec::new();
        }
        let items = self.unfiltered_items();
        ItemLabel::ALL
            .iter()
            .filter_map(|label| {
                let count = items.iter().filter(|i| i.label == Some(*label)).count();
                (count > 0 || self.filter.label == Some(*label)).then_some(LabelChip {
                    label: *label,
                    count,
                })
            })
            .collect()
    }

    /// The colour label the view is filtered by.
    #[must_use]
    pub fn label_filter(&self) -> Option<ItemLabel> {
        self.filter.label
    }

    /// Filters the view by a colour label (`None` shows every item). The selection clears and the
    /// cursor returns to the top.
    pub fn set_label_filter(&mut self, label: Option<ItemLabel>) {
        self.filter.label = label;
        self.generation += 1;
        self.clear_selection();
        self.cursor = 0;
    }

    /// What the open view is narrowed to.
    #[must_use]
    pub fn filter(&self) -> &ViewFilter {
        &self.filter
    }

    /// Narrows the view: `change` edits the filter. The selection clears and the cursor returns to
    /// the top, as for any change of what is shown.
    pub fn edit_filter(&mut self, change: impl FnOnce(&mut ViewFilter)) {
        change(&mut self.filter);
        self.generation += 1;
        self.clear_selection();
        self.cursor = 0;
    }

    /// What the filter holds, in the order the chips show it: kinds, then trigger, sensitive,
    /// untagged and the chosen tags (by the sidebar's tag order).
    #[must_use]
    pub fn filter_terms(&self) -> Vec<FilterTerm> {
        let f = &self.filter;
        let mut terms: Vec<FilterTerm> = ItemKind::ALL
            .iter()
            .filter(|k| f.kinds.contains(k))
            .map(|k| FilterTerm::Kind(*k))
            .collect();
        terms.extend(f.has_trigger.then_some(FilterTerm::HasTrigger));
        terms.extend(f.sensitive.then_some(FilterTerm::Sensitive));
        terms.extend(f.untagged.then_some(FilterTerm::Untagged));
        terms.extend(
            self.tags
                .iter()
                .filter(|t| f.tags.contains(&t.id))
                .map(|t| FilterTerm::Tag(t.id)),
        );
        terms
    }

    /// Adds `term` to the filter, or takes it off when it is there.
    pub fn toggle_filter(&mut self, term: FilterTerm) {
        self.edit_filter(|f| match term {
            FilterTerm::Kind(kind) => f.toggle_kind(kind),
            FilterTerm::HasTrigger => f.has_trigger = !f.has_trigger,
            FilterTerm::Sensitive => f.sensitive = !f.sensitive,
            FilterTerm::Untagged => f.toggle_untagged(),
            FilterTerm::Tag(tag) => f.toggle_tag(tag),
        });
    }

    /// Whether item `id` has a typed trigger (for the filter's choices).
    #[must_use]
    pub fn has_trigger(&self, id: ItemId) -> bool {
        self.triggered.contains(&id)
    }

    /// The settings key of `section`'s sort: the sections that have none of their own in the
    /// library (All items, Pinned and a tag's view). `None` for a list (its sort is part of it),
    /// the Recent inbox and the Trash.
    #[must_use]
    pub fn sort_key(section: Section) -> Option<String> {
        match section {
            Section::All => Some("all".to_owned()),
            Section::Pinned => Some("pinned".to_owned()),
            Section::Tag(id) => Some(format!("tag:{id}")),
            Section::List(_) | Section::Recent | Section::Trash => None,
        }
    }

    /// Loads the sorts of All items, Pinned and the tag views (see [`sort_key`](Self::sort_key)).
    pub fn set_section_sorts(&mut self, sorts: BTreeMap<String, Sort>) {
        self.sorts = sorts;
        self.generation += 1;
    }

    /// The open section's sort (manual where it has none).
    #[must_use]
    pub fn sort(&self) -> Sort {
        match self.section {
            Section::List(id) => self
                .lists
                .iter()
                .find(|l| l.id == id)
                .map_or(Sort::MANUAL, |l| l.sort),
            other => Self::sort_key(other)
                .and_then(|k| self.sorts.get(&k).copied())
                .unwrap_or(Sort::MANUAL),
        }
    }

    /// Whether the open section has a sort to choose (not the Recent inbox or the Trash).
    #[must_use]
    pub fn sortable(&self) -> bool {
        !matches!(self.section, Section::Recent | Section::Trash) && !self.searching()
    }

    /// The items of list `id` in the list's own order. A group is a block of the manual order and
    /// stays where it is under any sort: the sort orders the rows inside each block (a group, or a
    /// run of loose items). With a filter on there are no groups and the whole list is sorted.
    fn list_items(&self, id: ListId) -> Vec<&Item> {
        let items: Vec<&Item> = self
            .items
            .iter()
            .filter(|i| i.list_id == id)
            .map(|i| &**i)
            .collect();
        let sort = self
            .lists
            .iter()
            .find(|l| l.id == id)
            .map_or(Sort::MANUAL, |l| l.sort);
        if sort.is_manual() {
            return items;
        }
        let runs = self.groups.get(&id).filter(|_| self.filter.is_none());
        let group_of = |item: &Item| {
            runs.and_then(|runs| runs.iter().find(|r| r.items.contains(&item.id)))
                .map(|r| r.group.id)
        };
        let mut sorted = Vec::with_capacity(items.len());
        let mut block: Vec<&Item> = Vec::new();
        let mut block_group = None;
        for item in items {
            let group = group_of(item);
            if group != block_group {
                sort.apply(&mut block);
                sorted.append(&mut block);
                block_group = group;
            }
            block.push(item);
        }
        sort.apply(&mut block);
        sorted.append(&mut block);
        sorted
    }

    /// What the rows stand for, in order.
    #[must_use]
    pub fn keys(&self) -> Vec<Key> {
        self.entries()
            .into_iter()
            .map(|e| match e {
                Entry::Item(i, _) => Key::Item(i.id),
                Entry::Trashed(t) => t.target.into(),
                Entry::Header(r, _) => Key::Group(r.group.id),
            })
            .collect()
    }

    /// The rows of the open section (or of the search).
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let mixed = self.searching() || !matches!(self.section, Section::List(_));
        let hit_tags: HashMap<ItemId, &str> = self
            .hits
            .iter()
            .filter_map(|hit| Some((hit.id, hit.tag.as_deref()?)))
            .collect();
        self.entries()
            .into_iter()
            .map(|entry| match entry {
                Entry::Item(item, depth) => {
                    let title = if item.title.trim().is_empty() {
                        item.preview.lines().next().unwrap_or_default().to_owned()
                    } else {
                        item.title.clone()
                    };
                    let marks = if self.searching() {
                        search_mark::marks(
                            &title,
                            &item.preview,
                            item.sensitive,
                            hit_tags.get(&item.id).copied(),
                            &self.query,
                        )
                    } else {
                        Marks::default()
                    };
                    Row {
                        key: Key::Item(item.id),
                        kind: item.kind,
                        sensitive: item.sensitive,
                        title,
                        marks,
                        preview: item.preview.clone(),
                        list: mixed
                            .then(|| self.list_labels.get(&item.list_id).cloned())
                            .flatten(),
                        pinned: item.pinned,
                        label: item.label,
                        selected: self.selected.contains(&Key::Item(item.id)),
                        header: None,
                        depth,
                        image_stamp: (item.kind == ItemKind::Image && !item.sensitive)
                            .then_some(item.updated_at),
                        thumb: None,
                    }
                }
                Entry::Trashed(entry) => {
                    let key = Key::from(entry.target);
                    let preview = match key {
                        // An untitled item's title is its preview: say it once.
                        Key::Item(_) if entry.preview == entry.label => String::new(),
                        Key::Item(_) | Key::Group(_) => entry.preview.clone(),
                        Key::List(_) => LIST_NOTE.to_owned(),
                        Key::Board(_) => BOARD_NOTE.to_owned(),
                    };
                    let marks = if self.searching() {
                        search_mark::marks(
                            &entry.label,
                            if matches!(key, Key::Item(_)) {
                                &preview
                            } else {
                                ""
                            },
                            false,
                            None,
                            &self.query,
                        )
                    } else {
                        Marks::default()
                    };
                    Row {
                        key,
                        kind: ItemKind::Text,
                        sensitive: false,
                        title: entry.label.clone(),
                        preview,
                        list: None,
                        pinned: false,
                        label: None,
                        selected: self.selected.contains(&key),
                        marks,
                        header: None,
                        depth: 0,
                        image_stamp: None,
                        thumb: None,
                    }
                }
                Entry::Header(run, depth) => header_row(run, depth),
            })
            .collect()
    }

    /// The item for `id`, from the loaded library.
    #[must_use]
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id).map(|i| &**i)
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
    /// and no search is running (under a filter a row swaps with the next one shown).
    #[must_use]
    pub fn reorderable(&self) -> bool {
        !self.searching() && self.open_list().is_some_and(|l| l.sort.is_manual())
    }

    // ---- Changing what is shown ----------------------------------------------------------------

    /// Opens `section`: the search ends, the label filter and the selection clear and the cursor
    /// returns to the top.
    pub fn open(&mut self, section: Section) {
        self.section = section;
        self.generation += 1;
        self.filter = ViewFilter::none();
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
        S: BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo + GroupRepo + TriggerRepo,
    {
        if !self.boards.iter().any(|b| b.id == id) {
            return Ok(());
        }
        self.board = Some(id);
        self.open(Section::All);
        self.reload(store)
    }

    /// Sets the search text and finds the matches (every board): the shared index, with the
    /// store's full-text index for the full content. The selection and cursor start over.
    ///
    /// # Errors
    ///
    /// Any store error from the search; the text is kept and the matches are empty.
    pub fn set_query<S: ItemRepo>(&mut self, store: &S, query: &str) -> snipdeck_store::Result<()> {
        query.clone_into(&mut self.query);
        self.generation += 1;
        self.hits.clear();
        self.clear_selection();
        self.cursor = 0;
        if self.searching() && self.section != Section::Trash {
            self.hits = search(store, &self.snapshot.index(), &self.query)?;
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
        // A header is reached by the cursor but never selected like a card.
        self.selected.retain(|k| !matches!(k, Key::Group(_)));
        if matches!(key, Key::Group(_)) && mode == SelectMode::Replace {
            self.anchor = None;
        }
        self.cursor = index;
    }

    /// Selects every row (headers aside).
    pub fn select_all(&mut self) {
        self.selected = self
            .keys()
            .into_iter()
            .filter(|k| !matches!(k, Key::Group(_)))
            .collect();
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
            keys.get(index)
                .copied()
                .filter(|k| !matches!(k, Key::Group(_)))
                .into_iter()
                .collect()
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

/// Where `order[index]` sits now, as a placement: after the one before it, or first.
#[must_use]
pub fn place_of<I: Copy>(order: &[I], index: usize) -> Placement<I> {
    index
        .checked_sub(1)
        .and_then(|before| order.get(before))
        .map_or(Placement::First, |id| Placement::After(*id))
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

/// Where dropping the `moved` entries on `onto` places them among `order`: one placement per moved
/// entry, to be applied in the order given, so that the block lands where a single dragged entry
/// would (below `onto` when it comes from above, above it otherwise), keeping its current order.
/// A block that straddles `onto` (some moved entries above it, some below) lands after `onto`.
/// `None` when `onto` is one of the moved entries (a drop on a marked row), when `onto` is not in
/// `order`, when no moved entry is in `order`, or when nothing would change.
#[must_use]
pub fn reorder_many<I: Copy + PartialEq>(
    order: &[I],
    moved: &[I],
    onto: I,
) -> Option<Vec<(I, Placement<I>)>> {
    if moved.contains(&onto) {
        return None;
    }
    let onto_at = order.iter().position(|i| *i == onto)?;
    let block: Vec<I> = order
        .iter()
        .copied()
        .filter(|i| moved.contains(i))
        .collect();
    let first = order.iter().position(|i| block.first() == Some(i))?;
    let rest: Vec<I> = order
        .iter()
        .copied()
        .filter(|i| !moved.contains(i))
        .collect();
    let onto_rest = rest.iter().position(|i| *i == onto)?;
    // Coming from above (the block starts before `onto`) it lands after `onto`; else before it.
    let at = if first < onto_at {
        onto_rest + 1
    } else {
        onto_rest
    };
    let mut result = rest;
    result.splice(at..at, block.iter().copied());
    if result == order {
        return None;
    }
    let mut placements = Vec::with_capacity(block.len());
    let mut previous = at.checked_sub(1).map(|p| result[p]);
    for id in block {
        placements.push((id, previous.map_or(Placement::First, Placement::After)));
        previous = Some(id);
    }
    Some(placements)
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
        model
            .reload(&demo::store(now()).expect("store"))
            .expect("reload");
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
        assert_eq!(model.side_tags().len(), 0);
        assert_eq!(model.section(), Section::All);
    }

    #[test]
    fn all_items_spans_every_board_and_mixed_rows_name_their_list() {
        let (_store, model) = loaded();
        let rows = model.rows();
        assert_eq!(rows.len(), 11);
        assert!(titles(&model).contains(&"Shopping list".to_owned()));
        let shopping = rows
            .iter()
            .find(|r| r.title == "Shopping list")
            .expect("row");
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
    fn recent_has_no_item_rows() {
        let (_store, mut model) = loaded();
        model.open(Section::Recent);
        assert_eq!(model.rows().len(), 0);
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
        let code = model
            .lists()
            .iter()
            .find(|l| l.name.as_str() == "Code")
            .expect("list")
            .clone();
        let mut sorted = code.clone();
        sorted.sort = Sort::MOST_USED;
        store.update_list(&sorted).expect("update");
        let second = store.items(code.id).expect("items")[1].id;
        store.record_use(second, now()).expect("use");
        model.reload(&store).expect("reload");
        model.open(Section::List(code.id));
        assert_eq!(titles(&model), ["Recent customers", "Retry helper"]);
        assert!(!model.reorderable());
    }

    /// The list with the most items in the demo library, with its item ids in order.
    fn fullest_list(model: &MainModel, store: &Store) -> (ListId, Vec<ItemId>) {
        model
            .lists()
            .iter()
            .map(|l| (l.id, store.items(l.id).expect("items")))
            .max_by_key(|(_, items)| items.len())
            .map(|(id, items)| (id, items.iter().map(|i| i.id).collect()))
            .expect("a list")
    }

    /// Groups the middle items of the fullest list: [first, header, a, b, (rest)].
    fn grouped(
        store: &mut Store,
        model: &mut MainModel,
    ) -> (ListId, Vec<ItemId>, snipdeck_core::Group) {
        use snipdeck_store::GroupRepo;
        let (list, ids) = fullest_list(model, store);
        assert!(ids.len() >= 3, "the demo list has at least three items");
        let group = snipdeck_core::Group::new(list, Name::new("Pair").expect("name"));
        store.create_group(&group, &ids[1..3]).expect("group");
        model.reload(store).expect("reload");
        model.open(Section::List(list));
        (list, ids, group)
    }

    #[test]
    fn a_manual_list_shows_a_header_above_its_group_and_indents_the_members() {
        let (mut store, mut model) = loaded();
        let (_, ids, group) = grouped(&mut store, &mut model);
        let rows = model.rows();
        assert_eq!(rows[0].key, Key::Item(ids[0]));
        assert!(rows[0].header.is_none() && rows[0].depth == 0);
        assert_eq!(rows[1].key, Key::Group(group.id));
        let header = rows[1].header.as_ref().expect("header");
        assert_eq!(
            (header.name.as_str(), header.count, header.collapsed),
            ("Pair", 2, false)
        );
        assert_eq!(rows[2].key, Key::Item(ids[1]));
        assert!(rows[2].depth == 1 && rows[3].depth == 1);
        assert_eq!(model.keys().len(), model.rows().len());
    }

    /// Groups the first three items of the fullest list as Outer, with Inner nested around the
    /// last two: rows are [Outer, a, Inner, b, c, (rest)].
    fn nested(
        store: &mut Store,
        model: &mut MainModel,
    ) -> (
        ListId,
        Vec<ItemId>,
        snipdeck_core::Group,
        snipdeck_core::Group,
    ) {
        use snipdeck_store::GroupRepo;
        let (list, ids) = fullest_list(model, store);
        let outer = snipdeck_core::Group::new(list, Name::new("Outer").expect("name"));
        store.create_group(&outer, &ids[0..3]).expect("outer");
        let inner = snipdeck_core::Group {
            parent: Some(outer.id),
            ..snipdeck_core::Group::new(list, Name::new("Inner").expect("name"))
        };
        store.create_group(&inner, &ids[1..3]).expect("inner");
        model.reload(store).expect("reload");
        model.open(Section::List(list));
        (list, ids, outer, inner)
    }

    #[test]
    fn a_nested_group_shows_its_header_below_its_parents_and_indents_by_depth() {
        let (mut store, mut model) = loaded();
        let (_, ids, outer, inner) = nested(&mut store, &mut model);
        let rows = model.rows();
        assert_eq!(
            model.keys()[..5],
            [
                Key::Group(outer.id),
                Key::Item(ids[0]),
                Key::Group(inner.id),
                Key::Item(ids[1]),
                Key::Item(ids[2]),
            ]
        );
        let depth = |r: &Row| r.header.as_ref().map_or(r.depth, |h| h.depth);
        assert_eq!(
            rows[..5].iter().map(depth).collect::<Vec<_>>(),
            [0, 1, 1, 2, 2]
        );
        assert_eq!(rows[0].header.as_ref().map(|h| h.count), Some(3));
        assert_eq!(rows[2].header.as_ref().map(|h| h.count), Some(2));
        assert_eq!(model.group_of(ids[2]).map(|r| r.group.id), Some(inner.id));
    }

    #[test]
    fn folding_a_parent_hides_the_groups_nested_in_it() {
        use snipdeck_store::GroupRepo;
        let (mut store, mut model) = loaded();
        let (list, ids, mut outer, mut inner) = nested(&mut store, &mut model);
        outer.collapsed = true;
        store.update_group(&outer).expect("fold");
        model.reload(&store).expect("reload");
        model.open(Section::List(list));
        assert_eq!(model.keys()[..2], [Key::Group(outer.id), model.keys()[1]]);
        assert!(
            !model.keys().contains(&Key::Group(inner.id))
                && !model.keys().contains(&Key::Item(ids[0]))
                && !model.keys().contains(&Key::Item(ids[2])),
            "the parent's own members, the nested header and its members are all hidden"
        );
        // Unfolded again, a folded child hides only its own members.
        outer.collapsed = false;
        inner.collapsed = true;
        store.update_group(&outer).expect("unfold");
        store.update_group(&inner).expect("fold child");
        model.reload(&store).expect("reload");
        model.open(Section::List(list));
        assert_eq!(
            model.keys()[..3],
            [
                Key::Group(outer.id),
                Key::Item(ids[0]),
                Key::Group(inner.id)
            ]
        );
        assert!(!model.keys().contains(&Key::Item(ids[1])));
    }

    #[test]
    fn a_sort_orders_the_rows_inside_each_block_of_a_nested_group() {
        let (mut store, mut model) = loaded();
        let (list, ids, _, inner) = nested(&mut store, &mut model);
        let mut sorted = model
            .lists()
            .iter()
            .find(|l| l.id == list)
            .expect("list")
            .clone();
        sorted.sort = Sort::new(snipdeck_core::SortKey::Name, true);
        store.update_list(&sorted).expect("sort");
        model.reload(&store).expect("reload");
        model.open(Section::List(list));
        let keys = model.keys();
        let at = |k: Key| keys.iter().position(|x| *x == k).expect("row");
        assert!(
            at(Key::Item(ids[0])) < at(Key::Group(inner.id)),
            "the parent's member stays above the child"
        );
        let (b, c) = (at(Key::Item(ids[1])), at(Key::Item(ids[2])));
        assert!(
            at(Key::Group(inner.id)) < b.min(c),
            "the child's members stay under its header"
        );
        assert!(b.abs_diff(c) == 1, "and together");
    }

    #[test]
    fn a_collapsed_group_hides_its_members_and_keeps_its_header() {
        let (mut store, mut model) = loaded();
        let (list, ids, mut group) = grouped(&mut store, &mut model);
        group.collapsed = true;
        snipdeck_store::GroupRepo::update_group(&mut store, &group).expect("collapse");
        model.reload(&store).expect("reload");
        let keys = model.keys();
        assert!(keys.contains(&Key::Group(group.id)));
        assert!(!keys.contains(&Key::Item(ids[1])) && !keys.contains(&Key::Item(ids[2])));
        assert_eq!(
            model.manual_order(list).len(),
            ids.len(),
            "the order still holds every item"
        );
        assert_eq!(model.rows()[1].header.as_ref().expect("header").count, 2);
    }

    #[test]
    fn groups_need_no_search_or_filter_and_survive_a_sort() {
        let (mut store, mut model) = loaded();
        let (list, _, group) = grouped(&mut store, &mut model);
        model.set_query(&store, "e").expect("search");
        assert!(
            model
                .rows()
                .iter()
                .all(|r| r.header.is_none() && r.depth == 0),
            "a search is flat"
        );
        model.set_query(&store, "").expect("clear");
        assert!(model.rows().iter().any(|r| r.header.is_some()));
        model.open(Section::All);
        assert!(
            model.rows().iter().all(|r| r.header.is_none()),
            "All items is flat"
        );
        let mut sorted = model
            .lists()
            .iter()
            .find(|l| l.id == list)
            .expect("list")
            .clone();
        sorted.sort = Sort::MOST_USED;
        store.update_list(&sorted).expect("sort");
        model.reload(&store).expect("reload");
        model.open(Section::List(list));
        assert!(
            model.keys().contains(&Key::Group(group.id)),
            "a list that sorts itself keeps its group headers (the items sort inside each group)"
        );
        assert!(!model.reorderable(), "but it cannot be reordered by hand");
    }

    #[test]
    fn a_header_is_never_an_action_target_and_select_all_skips_it() {
        let (mut store, mut model) = loaded();
        let (_, ids, group) = grouped(&mut store, &mut model);
        model.select(1, SelectMode::Replace);
        assert_eq!(model.cursor(), 1);
        assert_eq!(
            model.selected_count(),
            0,
            "a header is not selected like a card"
        );
        assert_eq!(model.targets(1), []);
        model.select_all();
        assert!(!model.rows()[1].selected, "{:?} stays unselected", group.id);
        assert_eq!(model.targets(0).len(), model.keys().len() - 1);
        model.select(0, SelectMode::Replace);
        model.select(3, SelectMode::Extend);
        assert_eq!(
            model.targets(0),
            [Key::Item(ids[0]), Key::Item(ids[1]), Key::Item(ids[2])],
            "a range across the header keeps the items only"
        );
        assert!(
            model.dragged_items(1).is_empty(),
            "a header drags as a group, not as items"
        );
    }

    #[test]
    fn group_at_and_group_of_find_the_group_by_row_and_by_item() {
        let (mut store, mut model) = loaded();
        let (_, ids, group) = grouped(&mut store, &mut model);
        assert_eq!(model.group_at(1).map(|r| r.group.id), Some(group.id));
        assert!(model.group_at(0).is_none() && model.group_at(2).is_none());
        assert_eq!(model.group_of(ids[2]).map(|r| r.group.id), Some(group.id));
        assert!(model.group_of(ids[0]).is_none());
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
    fn a_hit_on_another_board_names_its_board() {
        // MAIN-1: the line says "in all boards", so a hit shows where it lives.
        let (store, mut model) = loaded();
        model.open(Section::List(list_named(&model, "Code")));
        model.set_query(&store, "milk").expect("search");
        let place = model.rows()[0].list.clone().expect("a place");
        assert!(!place.board.is_empty(), "the board of the hit: {place:?}");
    }

    #[test]
    fn a_search_that_fills_its_limit_says_so() {
        // MAIN-2: 200 hits may be a cut, so the status line says "first 200".
        let (store, mut model) = loaded();
        model.set_query(&store, "milk").expect("search");
        assert!(!model.search_capped(), "one hit is not a cut");
        let id = store.all_items().expect("items")[0].id;
        model.hits = vec![
            Hit {
                id,
                score: 0,
                tag: None
            };
            SEARCH_LIMIT
        ];
        assert!(model.search_capped());
        model.set_query(&store, "").expect("clear");
        assert!(!model.search_capped(), "no search, no cut");
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
        assert_eq!(
            model.targets(3),
            [keys[3]],
            "one selected row: the row asked about"
        );
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

    type Case<'a> = (&'a [u8], u8, Option<Vec<u8>>);

    /// Applies `reorder_many`'s placements to `order` one by one, as the controller does.
    fn apply_many(order: &[u8], moved: &[u8], onto: u8) -> Option<Vec<u8>> {
        let mut now = order.to_vec();
        for (id, placement) in reorder_many(order, moved, onto)? {
            now.retain(|i| *i != id);
            let at = match placement {
                Placement::First => 0,
                Placement::After(p) => now.iter().position(|i| *i == p)? + 1,
                Placement::Last => return None,
            };
            now.insert(at, id);
        }
        Some(now)
    }

    #[test]
    fn reorder_many_places_the_block_where_a_single_row_would_land() {
        let order = [1u8, 2, 3, 4, 5, 6];
        let table: [Case<'_>; 9] = [
            // block down: after the target
            (&[1, 2], 4, Some(vec![3, 4, 1, 2, 5, 6])),
            // block up: before the target
            (&[5, 6], 2, Some(vec![1, 5, 6, 2, 3, 4])),
            // up to the very top
            (&[4, 5], 1, Some(vec![4, 5, 1, 2, 3, 6])),
            // non-adjacent marks gather in their current order
            (&[2, 5], 6, Some(vec![1, 3, 4, 6, 2, 5])),
            (&[5, 2], 1, Some(vec![2, 5, 1, 3, 4, 6])),
            // a drop on a marked row is a no-op
            (&[2, 4], 4, None),
            (&[2, 4], 2, None),
            // a block straddling the target lands after it
            (&[2, 5], 4, Some(vec![1, 3, 4, 2, 5, 6])),
            // onto the row just above the block: it swaps past it
            (&[2, 3], 1, Some(vec![2, 3, 1, 4, 5, 6])),
        ];
        for (moved, onto, want) in table {
            assert_eq!(
                apply_many(&order, moved, onto),
                want,
                "{moved:?} onto {onto}"
            );
        }
        assert_eq!(
            apply_many(&order, &[2], 9),
            None,
            "a target outside the order"
        );
        assert_eq!(apply_many(&order, &[], 3), None, "nothing to move");
        assert_eq!(
            apply_many(&order, &[9], 3),
            None,
            "moved rows not in the order"
        );
        // One row is exactly `reorder`.
        let (_, single) = reorder(&order, 0, 2).expect("single");
        assert_eq!(reorder_many(&order, &[1], 3), Some(vec![(1, single)]));
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
