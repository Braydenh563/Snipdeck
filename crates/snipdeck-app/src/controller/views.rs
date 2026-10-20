//! What the controller hands the window (`View` and its parts) and the effects it asks the host for.

use std::collections::BTreeMap;

use snipdeck_core::{ItemId, UseMode, UseOutcome};
use snipdeck_platform::system_look::HighContrast;

use crate::{
    accent::Accent,
    fill::{self},
    frame::{Edge, Rect},
    panel::{Footer, Row, Tab, UseLabel},
    picker::{self, Scope},
    settings::DockEdge,
    toast::{ToastKind, ToastMessage},
};

use super::{AppMenuEntry, HelpTopic, ItemMenuEntry, ListMenuEntry};

/// Something the host (the window and the process) must do. The controller decides, the host
/// acts: nothing here touches a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Show the Panel without taking focus.
    ShowPanel,
    /// Hide the Panel.
    HidePanel,
    /// Start the OS window drag from the Panel header.
    DragWindow,
    /// Show the Main window, creating it the first time (the tray's "Open Snipdeck").
    OpenMainWindow,
    /// Switch the global hotkeys off (`true`) or back on (`false`), per the settings.
    SetHotkeysPaused(bool),
    /// Exit the app.
    Quit,
    /// Open the Settings window, creating it the first time (the Panel menu's "Settings").
    OpenSettings,
    /// Start a new snippet. The Panel's binding opens the item editor on it
    /// ([`Controller::open_new_item`](crate::controller::Controller::open_new_item)); the host has nothing more to do.
    NewItem,
    /// Show the Main window with the command palette open (the Panel's "All commands…" and Ctrl+K).
    OpenCommands,
    /// Open a help page (not built yet: the host logs it).
    OpenHelp(HelpTopic),
    /// Show the list editor the controller just opened, even if the Main window never was
    /// (the Panel's list menu and tab double-click).
    OpenListEditor,
    /// Open the item editor on an item. The Panel's binding does it
    /// ([`Controller::open_item_editor`](crate::controller::Controller::open_item_editor)); the host has nothing more to do.
    EditItem(ItemId),
    /// Start the OS resize of the Panel from an edge or corner.
    ResizeWindow(Edge),
    /// Move and size the Panel to this frame (a stored place restored, or a frame clamped).
    PlaceWindow(Rect),
    /// Show the window that asks for the master password (a locked sensitive item was needed).
    OpenUnlock,
}

/// How far a toast is through its life (the Slint `ToastPhase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastPhase {
    /// Fading in.
    Enter,
    /// Settled.
    Hold,
    /// Fading out.
    Exit,
}

/// The paste stack strip of the Panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StackView {
    /// How many items are marked.
    pub count: usize,
    /// "Start stack" was chosen: Paste next walks the marks.
    pub running: bool,
    /// How many have been pasted so far (the index of the next one).
    pub position: usize,
    /// The title of the item Paste next pastes next (empty when it has no title).
    pub next_title: String,
    /// The marked items, in the order they will be pasted.
    pub items: Vec<ItemId>,
}

/// The toast on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastView {
    /// Changes with every new toast, so the host can tell a new one from a redraw.
    pub seq: u64,
    /// Style.
    pub kind: ToastKind,
    /// What it says, before wording.
    pub message: ToastMessage,
    /// Where it is in its life.
    pub phase: ToastPhase,
    /// It carries an Undo button ([`Controller::main_undo`](crate::controller::Controller::main_undo)).
    pub undo: bool,
    /// It carries an Edit button ([`Controller::toast_edit`](crate::controller::Controller::toast_edit)).
    pub edit: bool,
    /// It carries an "Open log" button ([`Controller::toast_action`](crate::controller::Controller::toast_action)).
    pub open_log: bool,
}

/// The Picker window's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerView {
    /// The results, best first.
    pub rows: Vec<picker::Entry>,
    /// What the search covers.
    pub scope: Scope,
    /// The search text.
    pub query: String,
    /// The keyboard-selected row.
    pub selected: usize,
    /// Where Enter goes ("Paste" or "Copy" in the window's footer).
    pub footer: Footer,
}

/// The template fill form's content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillView {
    /// Changes with every form that opens, so the window resets its fields for a new one and
    /// keeps what the user typed while the same form stays up.
    pub seq: u64,
    /// The template item's name.
    pub title: String,
    /// One per prompt.
    pub fields: Vec<fill::Field>,
    /// The text as it would be pasted now; empty while `masked`.
    pub preview: String,
    /// The template item is sensitive: the preview would show its secret text, so it is left
    /// out and the window draws a mask instead.
    pub masked: bool,
}

/// A filled-in form, ready to be used: see [`Controller::take_fill`](crate::controller::Controller::take_fill).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillSubmission {
    pub(super) item: ItemId,
    pub(super) mode: UseMode,
    pub(super) answers: BTreeMap<String, String>,
}

/// How a use ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseResult {
    /// It happened (pasted or copied) and was counted.
    Done(UseOutcome),
    /// The template needs answers first: the fill form is open, see [`Controller::fill`](crate::controller::Controller::fill).
    NeedsAnswers,
    /// It did not happen; a toast says why.
    Failed,
}

/// Everything the Panel window shows, as plain data.
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per thing the window shows; they are independent flags, not a state"
)]
pub struct View {
    /// The board's name.
    pub board_name: String,
    /// The list tabs.
    pub tabs: Vec<Tab>,
    /// The active tab (0 when the board has no lists).
    pub current_tab: usize,
    /// The pinned strip.
    pub pinned: Vec<Row>,
    /// The item rows. A revealed sensitive row carries its real text in `preview`.
    pub rows: Vec<Row>,
    /// The index in `rows` of the sensitive row held revealed.
    pub revealed: Option<usize>,
    /// The search text.
    pub query: String,
    /// The keyboard-selected row.
    pub selected: usize,
    /// Where a click goes. The window derives its "Paste ↵" / "Copy ↵" label from it.
    pub footer: Footer,
    /// What a click does.
    pub use_label: UseLabel,
    /// Whether a drag can rearrange the rows (manual order, no search).
    pub rows_reorderable: bool,
    /// Keep on top (the pin button's state).
    pub on_top: bool,
    /// Whether the window is topmost now: pinned, and no Snipdeck window of the regular kind
    /// (Main, Settings, an editor, Help) is the active one. Such a window is allowed to cover the
    /// Panel; the Panel goes back to the top when focus moves to another app.
    pub topmost: bool,
    /// Only the dock's handle shows (the window is a thin strip, see [`crate::dock`]).
    pub dock_collapsed: bool,
    /// The handle runs up and down (the Panel is docked to the left or right edge).
    pub dock_vertical: bool,
    /// The Panel is docked to an edge (the kebab offers to undock it).
    pub docked: bool,
    /// The edge the kebab's "Dock" row would dock a floating Panel to (`None` while docked or
    /// before the monitor is known): the row names it.
    pub dock_target: Option<DockEdge>,
    /// The edge a drag under way would dock to if released now; the Panel marks that edge.
    pub dock_preview: Option<DockEdge>,
    /// Dark theme: the `theme` setting, or Windows' app mode when it is `system`.
    pub dark: bool,
    /// The high-contrast colours while a Windows high-contrast theme is on (it overrides the
    /// theme setting), else `None`.
    pub high_contrast: Option<HighContrast>,
    /// The user's accent, when it is not the built-in blue.
    pub accent: Option<Accent>,
    /// The paste stack, while items are marked or it runs.
    pub stack: Option<StackView>,
    /// Animations are off: the `reduce_motion` setting, or Windows' "Animation effects" off.
    pub reduced_motion: bool,
    /// The toast, if one is showing.
    pub toast: Option<ToastView>,
    /// The board menu's entries while it is open.
    pub board_menu: Option<Vec<String>>,
    /// The kebab menu's rows while it is open ([`APP_MENU`](crate::controller::APP_MENU)).
    pub app_menu: Option<Vec<AppMenuEntry>>,
    /// The context menu of a card or pinned chip while it is open.
    pub item_menu: Option<Vec<ItemMenuEntry>>,
    /// The item menu's Move or Colour choice while it is open (it takes the menu's place).
    pub choose_menu: Option<ChooseMenu>,
    /// How many rows the multi-selection holds (the bulk bar shows from one).
    pub marked_count: usize,
    /// The rows show their 1 to 9 numbers (the `show_numbers` setting).
    pub show_numbers: bool,
    /// The context menu of a list tab while it is open ([`LIST_MENU`](crate::controller::LIST_MENU)).
    pub list_menu: Option<Vec<ListMenuEntry>>,
    /// The list menu's "Delete list…" confirmation while it asks (the Panel draws it; no
    /// editor opens).
    pub list_delete: Option<ListDeleteView>,
    /// "Pause hotkeys" is on (its menu row is checked).
    pub hotkeys_paused: bool,
    /// The Picker's configured hotkey as text ("Ctrl+Shift+Space"), empty while it is off: the
    /// Panel's footer names it so the user learns it works anywhere.
    pub picker_chord: String,
    /// The Picker, while it is open.
    pub picker: Option<PickerView>,
    /// The template fill form, while it is open.
    pub fill: Option<FillView>,
    /// The Recent inbox (clock tab), while it is on in the settings.
    pub recent: Option<crate::recent::RecentView>,
}

/// What a row of the Move or Colour choice says, before wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChooseLabel {
    /// A list, by name.
    List(String),
    /// A colour label, or `None` for "No label".
    Colour(Option<snipdeck_core::ItemLabel>),
}

/// One row of the Move or Colour choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooseEntry {
    /// The words.
    pub label: ChooseLabel,
    /// Every target already has it (the row is checked).
    pub checked: bool,
}

/// The item menu's second level: where to move the targets, or which colour to give them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChooseMenu {
    /// The rows, in order.
    pub entries: Vec<ChooseEntry>,
}

/// The Panel's confirmation before a list goes to the Trash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListDeleteView {
    /// The list's name.
    pub name: String,
    /// How many items move to the Trash with it.
    pub count: usize,
}
