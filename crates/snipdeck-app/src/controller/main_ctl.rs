//! The Main window's half of the [`Controller`]: sections, search, selection, the item actions,
//! the Trash with its 5 s Undo, list and board editing and reordering. Slint-free like the rest
//! of the controller: `main_binding` copies a [`MainScreen`] into the window and calls these
//! methods from its callbacks.
//!
//! Every method that changes the library ends in [`Controller::library_changed`], which marks
//! the Main window's model stale (it reads the library again before the next
//! [`main_screen`](Controller::main_screen)) and refreshes the Panel, which shows the same
//! library.

use snipdeck_core::{BoardId, Item, ItemId, ItemLabel, ListId};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::HighContrast,
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, ToastView, failed};
use crate::{
    host::ClipboardText,
    main_window::{
        Counts, EditorView, Key, LabelChip, NameError, Row, Section, SelectMode, SideList, SideTag,
        ViewMode,
    },
    panel::UseLabel,
    settings::HotkeyAction,
    usecase,
};

mod editors;
mod items;
mod lists;

/// Why a tag could not be renamed in the Tag manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagRenameError {
    /// The new name is not a valid name (blank, too long, a control character).
    Name(NameError),
    /// Another tag already has that name (compared ignoring case).
    Exists,
    /// The library could not be written; a toast says so.
    Failed,
}

/// What the context menu, the bulk bar and the shortcuts of the Main window ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAction {
    /// Paste (or copy) the item.
    Use,
    /// Copy the item.
    Copy,
    /// Edit the item in the item editor.
    Edit,
    /// Pin, or unpin when every target is pinned.
    Pin,
    /// Copy each item, directly after it in its list.
    Duplicate,
    /// Open the *Move to list* chooser for the targets.
    Move,
    /// Open the *Tags* chooser for the targets.
    Tag,
    /// Move the targets to the Trash, with an Undo toast.
    Delete,
    /// Bring the targets back from the Trash.
    Restore,
    /// Delete the targets from the Trash for good.
    DeleteForever,
}

/// An entry of the board menu, before wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardMenuEntry {
    /// Switch to this board.
    Board {
        /// Its name.
        name: String,
        /// It is the open one.
        current: bool,
    },
    /// A divider.
    Separator,
    /// "New board…".
    New,
    /// "Rename board…" (the board editor for the open board).
    Rename,
    /// "Move board up".
    MoveUp {
        /// There is a board above.
        enabled: bool,
    },
    /// "Move board down".
    MoveDown {
        /// There is a board below.
        enabled: bool,
    },
    /// "Delete board…": the Main window's confirmation, and nothing else. Off for the last
    /// board, which cannot be deleted.
    Delete {
        /// More than one board exists.
        enabled: bool,
    },
}

/// A tag in the *Tags* chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagChoice {
    /// The tag's name.
    pub name: String,
    /// How many items carry it.
    pub count: usize,
    /// Every item the chooser acts on carries it.
    pub checked: bool,
}

/// Everything the Main window shows, as plain data.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per thing the window shows; they are independent flags, not a state"
)]
pub struct MainScreen {
    /// Dark theme.
    pub dark: bool,
    /// The high-contrast colours, while a Windows high-contrast theme is on.
    pub high_contrast: Option<HighContrast>,
    /// The user's accent, when it is not the built-in blue.
    pub accent: Option<crate::accent::Accent>,
    /// Animations are off.
    pub reduced_motion: bool,
    /// The open board's name.
    pub board_name: String,
    /// The board menu while it is open.
    pub board_menu: Option<Vec<BoardMenuEntry>>,
    /// The board menu's "Delete board…" confirmation while it asks: how many items move to the
    /// Trash with the open board.
    pub board_delete: Option<usize>,
    /// The open board's lists.
    pub lists: Vec<SideList>,
    /// The tags.
    pub tags: Vec<SideTag>,
    /// The *Tags* chooser's entries.
    pub tag_choices: Vec<TagChoice>,
    /// The numbers beside the system views.
    pub counts: Counts,
    /// The Recent inbox is on (its entries are listed in the Panel's clock tab).
    pub recent_on: bool,
    /// The library is locked: the Recent section shows an unlock prompt instead of entries.
    pub recent_locked: bool,
    /// The Recent section's entries, newest first (the search box narrows them); empty while the
    /// inbox is off or another section is open.
    pub recent_rows: Vec<crate::recent::RecentRowView>,
    /// The lists "Save to…" offers for a Recent entry (empty unless the Recent section is open).
    pub recent_targets: Vec<crate::recent::SaveTarget>,
    /// What the sidebar has open.
    pub section: Section,
    /// The index into `lists` or `tags` that `section` names.
    pub section_index: usize,
    /// The row style.
    pub view_mode: ViewMode,
    /// The chips of the colour-label filter row.
    pub label_chips: Vec<LabelChip>,
    /// The colour label the rows are filtered by.
    pub label_filter: Option<ItemLabel>,
    /// The search text.
    pub query: String,
    /// The keyboard cursor.
    pub cursor: usize,
    /// Rows can be dragged into a new place: the open list is in manual order and no search is running.
    pub reorderable: bool,
    /// The rows.
    pub rows: Vec<Row>,
    /// How many rows are selected.
    pub selected_count: usize,
    /// "Use" or "Copy", by what a click can do now.
    pub use_label: UseLabel,
    /// The index of the sensitive row held revealed.
    pub revealed: Option<usize>,
    /// The toast, if one is showing.
    pub toast: Option<ToastView>,
    /// The list or board editor, while one is open.
    pub editor: Option<EditorView>,
    /// The Show/hide Panel hotkey as the user would press it ("Ctrl+Alt+Space"); empty while it is off.
    pub panel_hotkey: String,
}

/// What an open editor edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorTarget {
    NewList(BoardId),
    List(ListId),
    NewBoard,
    Board(BoardId),
}

/// What to show after an editor saved.
enum Opened {
    List(ListId),
    Board(BoardId),
}

/// An open list or board editor.
#[derive(Debug, Clone)]
pub(super) struct EditorSlot {
    view: EditorView,
    target: EditorTarget,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Reading -------------------------------------------------------------------------------

    /// Everything the Main window shows now. Reads the library again first if it changed.
    pub fn main_screen(&mut self) -> MainScreen {
        self.main_fresh();
        let look = self.look();
        let mut rows = self.main.rows();
        let mut revealed = None;
        if let Some(id) = self.revealed
            && let Some(at) = rows
                .iter()
                .position(|r| r.key == Key::Item(id) && r.sensitive)
        {
            revealed = Some(at);
            if let Ok(Some(text)) = usecase::item_text(&self.store, id) {
                rows[at].preview = text.chars().take(Item::PREVIEW_CHARS).collect();
            }
        }
        let in_recent = self.main.section() == Section::Recent;
        MainScreen {
            dark: look.dark,
            high_contrast: look.high_contrast,
            accent: self.accent(),
            reduced_motion: look.reduce_motion,
            board_name: self.main.board_name().to_owned(),
            board_menu: self
                .main
                .board_menu_open()
                .then(|| self.board_menu_entries()),
            board_delete: self.board_delete_count(),
            lists: self.main.side_lists(),
            tags: self.main.side_tags(),
            tag_choices: self.tag_choices(),
            counts: Counts {
                recent: self.recent_entries().len(),
                ..self.main.counts()
            },
            recent_on: self.settings.recent_history,
            recent_locked: self.recent_locked(),
            recent_rows: if in_recent {
                self.main_recent_rows()
            } else {
                Vec::new()
            },
            recent_targets: if in_recent {
                self.save_targets()
            } else {
                Vec::new()
            },
            section: self.main.section(),
            section_index: self.main.section_index(),
            view_mode: self.main.view_mode(),
            label_chips: self.main.label_chips(),
            label_filter: self.main.label_filter(),
            query: self.main.query().to_owned(),
            cursor: self.main.cursor(),
            reorderable: self.main.reorderable(),
            selected_count: self.main.selected_count(),
            rows,
            use_label: self.model.use_label(),
            revealed,
            toast: self.toast_view(),
            editor: self.editor.as_ref().map(|e| e.view.clone()),
            panel_hotkey: self
                .settings
                .hotkeys
                .chord(HotkeyAction::TogglePanel)
                .map(|chord| chord.to_string())
                .unwrap_or_default(),
        }
    }

    pub(super) fn main_fresh(&mut self) {
        if self.main.is_stale() && self.main.reload(&self.store).is_err() {
            self.main.mark_fresh();
            self.show_toast(failed());
        }
    }

    fn tag_choices(&self) -> Vec<TagChoice> {
        let targets: Vec<ItemId> = self
            .main
            .chooser()
            .iter()
            .filter_map(|k| match k {
                Key::Item(id) => Some(*id),
                _ => None,
            })
            .collect();
        self.main
            .tags()
            .iter()
            .zip(self.main.side_tags())
            .map(|(tag, side)| TagChoice {
                name: side.name,
                count: side.count,
                checked: !targets.is_empty()
                    && targets.iter().all(|id| {
                        self.main
                            .item(*id)
                            .is_some_and(|item| item.tags.contains(&tag.id))
                    }),
            })
            .collect()
    }

    /// The library changed: the Main window reads it again and the Panel follows.
    pub(super) fn library_changed(&mut self) {
        self.main.mark_stale();
        if self.model.refresh(&self.store).is_err() {
            self.show_toast(failed());
        }
    }

    // ---- Navigation, search, selection ---------------------------------------------------------

    /// The sidebar opened `section`: the search ends, the selection clears.
    pub fn main_open(&mut self, section: Section) {
        self.main_fresh();
        self.main.open(section);
        self.revealed = None;
    }

    /// The search box changed (every board is searched).
    pub fn main_set_query(&mut self, query: &str) {
        self.main_fresh();
        if self.main.set_query(&self.store, query).is_err() {
            self.show_toast(failed());
        }
        self.revealed = None;
    }

    /// A label chip was chosen (`None` clears the filter): the rows show only the items with that
    /// colour label.
    pub fn main_set_label_filter(&mut self, label: Option<ItemLabel>) {
        self.main_fresh();
        self.main.set_label_filter(label);
        self.revealed = None;
    }

    /// The row style changed.
    pub fn main_set_view(&mut self, view: ViewMode) {
        self.main.set_view_mode(view);
    }

    /// A click or key on row `index`.
    pub fn main_select(&mut self, index: usize, mode: SelectMode) {
        self.main_fresh();
        self.main.select(index, mode);
    }

    /// Ctrl+A.
    pub fn main_select_all(&mut self) {
        self.main_fresh();
        self.main.select_all();
    }

    /// Esc.
    pub fn main_clear_selection(&mut self) {
        self.main.clear_selection();
    }

    /// Holding Space (or the pointer) on sensitive row `index` reveals its text; letting go
    /// hides it again.
    pub fn main_reveal(&mut self, index: usize, on: bool) {
        self.main_fresh();
        let id = on
            .then(|| {
                self.main
                    .rows()
                    .get(index)
                    .filter(|r| r.sensitive)
                    .and_then(|r| match r.key {
                        Key::Item(id) => Some(id),
                        _ => None,
                    })
            })
            .flatten();
        self.revealed = id.filter(|_| self.reveal_allowed());
    }
}

fn neutral_toast(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Neutral,
        message,
    }
}

/// The toast for a restore that failed: a board past the Free limit says so.
fn restore_failed(error: &snipdeck_store::Error) -> Toast {
    if matches!(
        error,
        snipdeck_store::Error::Core(snipdeck_core::Error::ProRequired(_))
    ) {
        Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::BoardLimit,
        }
    } else {
        failed()
    }
}

impl<C, K, T> Controller<snipdeck_store::Store, C, K, T>
where
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Sweeps blob files nothing uses that are older than `grace` (the daily maintenance job).
    /// Returns how many files went.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    pub fn sweep_blobs(&mut self, grace: std::time::Duration) -> snipdeck_store::Result<usize> {
        let report = self.store.gc_blobs(grace)?;
        Ok(report.blobs + report.leftovers)
    }
}
