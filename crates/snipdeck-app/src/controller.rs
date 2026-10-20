//! The controller: everything the Panel does that is not drawing. It owns the library, the
//! [`PanelModel`], the settings, the clock and the platform services, and turns gestures
//! (a row activated, a digit pressed, a hotkey, the Target changing) into model changes, uses
//! and toasts. It knows nothing about Slint: `binding` copies a [`View`] into the window and
//! wires the window's callbacks to the methods here, so nearly all behaviour is tested without
//! a window.

use std::{collections::BTreeMap, time::Duration};

use snipdeck_core::{
    BoardId, Entitlements, Feature, ItemId, ListId, Placement, Timestamp, UseMode, UseOutcome,
    template::{Locale, Template, TimeZone},
};
use snipdeck_platform::{
    pipeline::{Clipboard, Env, Keyboard, Timing},
    route::Route,
    system_look::{HighContrast, SystemLook},
    tray::TrayEvent,
};
use snipdeck_store::{
    Batch, BlobStore, BoardRepo, ItemRepo, ListRepo, RecentRepo, Store, TagRepo, TrashRepo,
    Trashable, crypto::KeyRing,
};

mod commands_ctl;
mod dock_ctl;
mod editor_ctl;
mod lock_ctl;
mod main_ctl;
mod onboarding_ctl;
mod panel;
mod recent_ctl;
mod settings_ctl;

pub use lock_ctl::{MasterProblem, MasterState};
pub use main_ctl::{BoardMenuEntry, MainAction, MainScreen, TagChoice, TagRenameError};
pub use panel::{APP_MENU, AppMenuEntry, HelpTopic, ItemMenuEntry, LIST_MENU, ListMenuEntry};
pub use settings_ctl::{ImportKind, ImportPreview, ImportProblem, ImportSource};

use crate::{
    accent::Accent,
    capture::{self, Captured},
    dock::DockState,
    fill::{self, FillModel},
    frame::{Edge, Monitor, Rect},
    host::{ClipboardText, Clock, TargetSource},
    main_window::{MainModel, reorder},
    panel::{Footer, PanelModel, Row, Tab, UseLabel},
    paste_next::{self, PasteQueue},
    picker::{self, PickerModel, Scope},
    settings::{DockEdge, HotkeyAction, Settings, Theme},
    settings_store::{Poll, SettingsSaver},
    toast::{self, Toast, ToastKind, ToastMessage},
    usecase::{self, UseError, UseRequest},
};

/// How long the Target gets to take the focus back before keys are injected (the M2 spike's
/// 50 ms; Windows restores the focused control asynchronously).
pub const REFOCUS_WAIT: Duration = Duration::from_millis(50);
/// A foreground change this soon after the Panel was shown does not hide it: showing the Panel
/// (a hotkey, the tray) can itself move the foreground around while Windows settles.
pub const BLUR_GRACE: Duration = Duration::from_millis(400);
/// A toast fades in for this long (`Motion.quick`) before it counts as settled.
pub const TOAST_ENTER: Duration = Duration::from_millis(120);
/// A settled toast stays this long (`Toast.hold-duration`, DESIGN.md §4).
pub const TOAST_HOLD: Duration = Duration::from_millis(2500);
/// An undoable toast (a delete) settles for this long, so that it is on screen for 5 s with its
/// fade-in (SPEC §5.7a).
pub const TOAST_UNDO_HOLD: Duration = Duration::from_millis(4880);
/// A hovered toast looks again after this long to see whether the pointer has left.
pub const TOAST_HOVER_RECHECK: Duration = Duration::from_millis(500);
/// A toast is held for at most this many rechecks (a minute), so a missed pointer-leave (its
/// window hid under the pointer) cannot keep it on screen for good.
pub const TOAST_HOVER_RECHECKS: u32 = 120;
/// A toast fades out for this long (`Motion.fade`) before it is gone.
pub const TOAST_EXIT: Duration = Duration::from_millis(90);

/// What the controller needs from the library: every repository trait in one bound, and a way
/// to make several writes land together (or not at all).
pub trait Library:
    BoardRepo + ListRepo + ItemRepo + TagRepo + TrashRepo + BlobStore + RecentRepo
{
    /// Runs `f` in one store transaction: what it does through the [`Batch`] commits together
    /// when `f` returns `Ok`, and none of it happens when it returns `Err`.
    ///
    /// # Errors
    ///
    /// What `f` returns, or the store failing to begin or commit.
    fn atomically<T>(
        &mut self,
        f: impl FnOnce(&mut Batch<'_>) -> snipdeck_store::Result<T>,
    ) -> snipdeck_store::Result<T>;

    /// Whether encryption of sensitive items has been set up.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn key_ring_exists(&self) -> snipdeck_store::Result<bool>;

    /// Whether any sensitive item, a trashed one included, is stored as plain text (it was made
    /// sensitive before a key existed). Setting up encryption encrypts it.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn has_plain_sensitive(&self) -> snipdeck_store::Result<bool>;

    /// Sets up encryption, with a master password if `password` is `Some`, and leaves the library
    /// unlocked.
    ///
    /// # Errors
    ///
    /// The key could not be made or wrapped, or the store failed.
    fn create_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()>;

    /// Opens the stored key. `password` is the master password, `None` when none is set.
    ///
    /// # Errors
    ///
    /// The key does not open (a wrong or missing password, another user's key), or the store
    /// failed.
    fn unlock_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()>;

    /// Whether the stored key needs a master password (`false` with no key).
    ///
    /// # Errors
    ///
    /// The OS binding rejected the record, or the store failed.
    fn has_master_password(&self, ring: &KeyRing<'_>) -> snipdeck_store::Result<bool>;

    /// Re-wraps the key under a new master password (`None` removes it). `current` is the one set.
    ///
    /// # Errors
    ///
    /// A wrong `current`, an empty `new`, or the store failed. Nothing changes on error.
    fn change_master_password(
        &mut self,
        ring: &KeyRing<'_>,
        current: Option<&str>,
        new: Option<&str>,
    ) -> snipdeck_store::Result<()>;

    /// Forgets the key and every decrypted payload held in memory (the idle lock).
    fn lock_key_ring(&mut self);

    /// Whether the key is open.
    fn is_unlocked(&self) -> bool;
}

impl Library for Store {
    fn atomically<T>(
        &mut self,
        f: impl FnOnce(&mut Batch<'_>) -> snipdeck_store::Result<T>,
    ) -> snipdeck_store::Result<T> {
        self.transaction(f)
    }

    fn key_ring_exists(&self) -> snipdeck_store::Result<bool> {
        Store::key_ring_exists(self)
    }

    fn has_plain_sensitive(&self) -> snipdeck_store::Result<bool> {
        Store::has_plain_sensitive(self)
    }

    fn create_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()> {
        Store::create_key_ring(self, ring, password)
    }

    fn unlock_key_ring(
        &mut self,
        ring: &KeyRing<'_>,
        password: Option<&str>,
    ) -> snipdeck_store::Result<()> {
        Store::unlock_key_ring(self, ring, password)
    }

    fn has_master_password(&self, ring: &KeyRing<'_>) -> snipdeck_store::Result<bool> {
        Store::has_master_password(self, ring)
    }

    fn change_master_password(
        &mut self,
        ring: &KeyRing<'_>,
        current: Option<&str>,
        new: Option<&str>,
    ) -> snipdeck_store::Result<()> {
        Store::change_master_password(self, ring, current, new)
    }

    fn lock_key_ring(&mut self) {
        Store::lock_key_ring(self);
    }

    fn is_unlocked(&self) -> bool {
        Store::is_unlocked(self)
    }
}

/// The platform services a use needs (the traits of `snipdeck_platform::pipeline`).
#[derive(Debug)]
pub struct Services<C, K, T> {
    /// The clipboard.
    pub clipboard: C,
    /// The keyboard.
    pub keyboard: K,
    /// The clock the paste sequence sleeps on.
    pub timing: T,
}

/// A keyed hash of a clipboard text: enough to tell whether the clipboard still holds it,
/// without keeping the text itself.
#[derive(Debug, Clone)]
struct ClipboardDigest {
    state: std::hash::RandomState,
    hash: u64,
}

impl ClipboardDigest {
    fn of(text: &str) -> Self {
        let state = std::hash::RandomState::new();
        let hash = std::hash::BuildHasher::hash_one(&state, text);
        Self { state, hash }
    }

    fn matches(&self, text: &str) -> bool {
        std::hash::BuildHasher::hash_one(&self.state, text) == self.hash
    }
}

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
    /// ([`Controller::open_new_item`]); the host has nothing more to do.
    NewItem,
    /// Show the Main window with the command palette open (the Panel's "All commands…" and Ctrl+K).
    OpenCommands,
    /// Open a help page (not built yet: the host logs it).
    OpenHelp(HelpTopic),
    /// Show the list editor the controller just opened, even if the Main window never was
    /// (the Panel's list menu and tab double-click).
    OpenListEditor,
    /// Open the item editor on an item. The Panel's binding does it
    /// ([`Controller::open_item_editor`]); the host has nothing more to do.
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
    /// It carries an Undo button ([`Controller::main_undo`]).
    pub undo: bool,
    /// It carries an "Open log" button ([`Controller::toast_action`]).
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

/// A filled-in form, ready to be used: see [`Controller::take_fill`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillSubmission {
    item: ItemId,
    mode: UseMode,
    answers: BTreeMap<String, String>,
}

/// How a use ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseResult {
    /// It happened (pasted or copied) and was counted.
    Done(UseOutcome),
    /// The template needs answers first: the fill form is open, see [`Controller::fill`].
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
    /// The kebab menu's rows while it is open ([`APP_MENU`]).
    pub app_menu: Option<Vec<AppMenuEntry>>,
    /// The context menu of a card or pinned chip while it is open.
    pub item_menu: Option<Vec<ItemMenuEntry>>,
    /// The context menu of a list tab while it is open ([`LIST_MENU`]).
    pub list_menu: Option<Vec<ListMenuEntry>>,
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

/// What pressing Undo on the toast `seq` brings back from the Trash.
#[derive(Debug, Clone)]
struct Undo {
    seq: u64,
    targets: Vec<Trashable>,
}

struct ActiveToast {
    toast: Toast,
    phase: ToastPhase,
    seq: u64,
    /// The pointer is on the toast (or the Undo button has focus): its hold does not end.
    hovered: bool,
    /// How many times the hold has been extended for `hovered`, at most [`TOAST_HOVER_RECHECKS`].
    extended: u32,
}

/// The Panel's brain. See the module documentation.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent UI states, not a hidden state machine"
)]
pub struct Controller<S, C, K, T> {
    store: S,
    services: Services<C, K, T>,
    target: Box<dyn TargetSource>,
    clock: Box<dyn Clock>,
    /// The zone `{date}` and `{time}` show; the system's, except in tests.
    tz: TimeZone,
    /// A sensitive item's text left on the clipboard by a copy, and when to take it off again
    /// (if the clipboard still holds exactly that text). Only a keyed hash of the text is kept,
    /// so the secret does not sit in memory for the wait.
    clipboard_clear: Option<(Timestamp, ClipboardDigest)>,
    model: PanelModel,
    /// The search index shared by the Panel and the Picker, rebuilt only after a write.
    index_cache: crate::index_cache::IndexCache,
    settings: Settings,
    entitlements: Entitlements,
    visible: bool,
    /// When the Panel was last shown (or created): the start of [`BLUR_GRACE`].
    shown_at: Timestamp,
    on_top: bool,
    /// A regular Snipdeck window (not the Panel) holds the focus.
    own_window_active: bool,
    toast: Option<ActiveToast>,
    toast_seq: u64,
    /// The Picker, while it is open.
    picker: Option<PickerModel>,
    /// The template fill form, while it is open.
    fill: Option<FillModel>,
    fill_seq: u64,
    /// Where "paste next" is in its run.
    queue: PasteQueue,
    /// The items marked for a paste stack, in marking order.
    stack: Vec<ItemId>,
    /// The marked items are being pasted one by one ("Start stack" was chosen).
    stack_running: bool,
    /// Which of the Panel's two menus is open (never both).
    menu: PanelMenu,
    /// The Main window's state.
    main: MainModel,
    /// The list or board editor, while one is open.
    editor: Option<main_ctl::EditorSlot>,
    editor_seq: u64,
    /// The item editor, while one is open.
    item_editor: Option<editor_ctl::ItemEditorSlot>,
    item_editor_seq: u64,
    /// What the item editor needs from the machine (drafts folder, key protection, file dialog).
    editor_env: editor_ctl::EditorEnv,
    /// Opens the folder of the crash reports (the "Open log" button of the crash toast).
    log_opener: Option<LogOpener>,
    /// What the toast on screen undoes.
    undo: Option<Undo>,
    /// The toast whose timer chain a window has started.
    timer_seq: u64,
    /// A sensitive item held revealed in the Main window.
    revealed: Option<snipdeck_core::ItemId>,
    /// A sensitive item held revealed in the Panel (its own: the two windows hold separately).
    panel_revealed: Option<snipdeck_core::ItemId>,
    /// What Windows says about light / dark and high contrast.
    os_look: SystemLook,
    /// The words a new list's and a new board's editor name starts from, in the language shown
    /// (the Main window's binding sets them; see [`Controller::set_new_name_bases`]).
    new_list_base: String,
    new_board_base: String,
    session: Session,
    saver: Option<SettingsSaver>,
    /// Where the edge dock is (see [`crate::dock`]).
    dock: DockState,
    /// The monitor the Panel is on, as the host last reported it.
    monitor: Option<Monitor>,
    /// A drag from the header is under way on a docked Panel: the first move undocks it.
    drag_started: bool,
    /// Placements decided outside a Panel gesture, waiting for the binding.
    pending: Vec<Effect>,
    /// The master password: whether one is set, whether its prompt is open, the idle clock.
    lock: lock_ctl::LockState,
    /// Switches the clipboard listener of the Recent inbox (see `recent_ctl.rs`).
    recent_gate: Option<snipdeck_platform::clipboard_watch::WatchGate>,
}

/// Opens the folder of the crash reports in the file manager.
pub type LogOpener = Box<dyn Fn() -> std::io::Result<()>>;

/// The menu drawn inside the Panel, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelMenu {
    None,
    /// The board switcher's.
    Board,
    /// The kebab's ([`APP_MENU`]).
    App,
    /// The context menu of an item's card or pinned chip.
    Item(ItemId),
    /// The context menu of a list tab.
    List(ListId),
}

/// Facts about the running app that are not settings and not on screen.
#[derive(Debug, Default, Clone, Copy)]
#[allow(clippy::struct_excessive_bools, reason = "independent session flags")]
struct Session {
    /// A real tray icon is in the notification area now (from `TrayEvent::IconPresent`):
    /// closing the Panel may hide it.
    tray_available: bool,
    /// "Pause hotkeys" is on.
    hotkeys_paused: bool,
    /// "Pause history" is on: the Recent inbox collects nothing until Snipdeck restarts.
    history_paused: bool,
    /// The Panel's clock tab is the one selected.
    recent_open: bool,
    /// The last settings save failed and the user was told.
    save_failed: bool,
    /// The welcome tour's step while it is open.
    tour: Option<crate::tour::TourStep>,
}

impl<S, C, K, T> std::fmt::Debug for Controller<S, C, K, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("visible", &self.visible)
            .finish_non_exhaustive()
    }
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Builds the controller over `store`, showing the first board and list. The Panel starts
    /// visible; the Target is read now.
    ///
    /// # Errors
    ///
    /// Any store error from loading the Panel.
    pub fn new(
        store: S,
        services: Services<C, K, T>,
        target: Box<dyn TargetSource>,
        clock: Box<dyn Clock>,
        settings: Settings,
        entitlements: Entitlements,
    ) -> snipdeck_store::Result<Self> {
        let index_cache = crate::index_cache::IndexCache::new();
        let mut model = PanelModel::load_cached(&store, None, None, &index_cache)?;
        model.set_policy(policy_of(&settings));
        model.set_target(target.current());
        let shown_at = clock.now();
        Ok(Self {
            store,
            services,
            target,
            clock,
            tz: TimeZone::system(),
            clipboard_clear: None,
            model,
            index_cache,
            settings,
            entitlements,
            visible: true,
            shown_at,
            on_top: true,
            own_window_active: false,
            toast: None,
            toast_seq: 0,
            picker: None,
            fill: None,
            fill_seq: 0,
            queue: PasteQueue::default(),
            stack: Vec::new(),
            stack_running: false,
            menu: PanelMenu::None,
            main: MainModel::new(),
            editor: None,
            editor_seq: 0,
            item_editor: None,
            item_editor_seq: 0,
            editor_env: editor_ctl::EditorEnv::default(),
            log_opener: None,
            undo: None,
            timer_seq: 0,
            revealed: None,
            panel_revealed: None,
            os_look: SystemLook::default(),
            new_list_base: crate::starter::NEW_LIST.to_owned(),
            new_board_base: crate::starter::NEW_BOARD.to_owned(),
            session: Session::default(),
            saver: None,
            dock: DockState::default(),
            monitor: None,
            drag_started: false,
            pending: Vec::new(),
            lock: lock_ctl::LockState::new(shown_at),
            recent_gate: None,
        })
    }

    // ---- The machine around the Panel ----------------------------------------------------------

    /// Windows' light / dark and high-contrast state changed (or was read for the first time).
    /// The next [`view`](Self::view) follows it, unless the `theme` setting is not `system`.
    pub fn set_system_look(&mut self, look: SystemLook) {
        self.os_look = look;
    }

    /// Lets the crash toast's "Open log" button open the reports folder with `opener`.
    pub fn set_log_opener(&mut self, opener: LogOpener) {
        self.log_opener = Some(opener);
    }

    /// Writes setting changes to disk through `saver` from now on. Without one (the demo)
    /// changes stay in memory.
    pub fn set_settings_saver(&mut self, saver: SettingsSaver) {
        self.saver = Some(saver);
    }

    // ---- Reading -----------------------------------------------------------------------------

    /// Everything the window shows now.
    #[must_use]
    pub fn view(&self) -> View {
        let tabs = self.model.tabs();
        let board_name = self
            .model
            .board()
            .and_then(|id| self.model.boards().iter().find(|b| b.id == id))
            .map(|b| b.name.as_str().to_owned())
            .unwrap_or_default();
        let mut rows = self.model.rows();
        let mut revealed = None;
        if let Some(id) = self.panel_revealed
            && let Some(at) = rows.iter().position(|r| r.id == id && r.sensitive)
        {
            revealed = Some(at);
            if let Ok(Some(text)) = usecase::item_text(&self.store, id) {
                rows[at].preview = text
                    .chars()
                    .take(snipdeck_core::Item::PREVIEW_CHARS)
                    .collect();
            }
        }
        View {
            board_name,
            current_tab: tabs.iter().position(|t| t.active).unwrap_or(0),
            tabs,
            pinned: self.model.pinned(),
            rows,
            revealed,
            query: self.model.query().to_owned(),
            selected: self.model.selected_position().unwrap_or(0),
            footer: self.model.footer(),
            use_label: self.model.use_label(),
            rows_reorderable: self.model.reorderable(),
            on_top: self.on_top,
            topmost: self.on_top && !self.own_window_active,
            dock_collapsed: self.dock.is_collapsed(),
            dock_vertical: self.dock_edge().is_none_or(DockEdge::vertical),
            docked: self.dock_edge().is_some(),
            dark: self.look().dark,
            high_contrast: self.look().high_contrast,
            accent: self.accent(),
            stack: self.stack_view(),
            reduced_motion: self.look().reduce_motion,
            toast: self.toast_view(),
            board_menu: (self.menu == PanelMenu::Board).then(|| {
                self.model
                    .boards()
                    .iter()
                    .map(|b| b.name.as_str().to_owned())
                    .collect()
            }),
            app_menu: (self.menu == PanelMenu::App).then(|| APP_MENU.to_vec()),
            item_menu: match self.menu {
                PanelMenu::Item(id) => Some(self.item_menu_entries(id)),
                _ => None,
            },
            list_menu: matches!(self.menu, PanelMenu::List(_)).then(|| LIST_MENU.to_vec()),
            hotkeys_paused: self.session.hotkeys_paused,
            picker_chord: crate::commands::chord_text(&self.settings.hotkeys, HotkeyAction::Picker)
                .unwrap_or_default(),
            picker: self.picker.as_ref().map(|p| PickerView {
                rows: p.rows(),
                scope: p.scope(),
                query: p.query().to_owned(),
                selected: p.selected(),
                footer: self.model.footer(),
            }),
            fill: self.fill.as_ref().map(|f| {
                // A sensitive template's text must not reach the window (the Picker and the
                // lists mask it the same way); an unreadable item masks too.
                let masked = self.store.item(f.item()).is_ok_and(|i| i.sensitive);
                FillView {
                    seq: self.fill_seq,
                    title: f.title().to_owned(),
                    fields: f.fields().to_vec(),
                    preview: if masked {
                        String::new()
                    } else {
                        f.preview(
                            self.clock.now(),
                            &self.tz,
                            Locale::for_tag(&self.settings.language),
                        )
                    },
                    masked,
                }
            }),
            recent: self.recent_view(),
        }
    }

    /// The toast on screen, if any.
    fn toast_view(&self) -> Option<ToastView> {
        self.toast.as_ref().map(|t| ToastView {
            seq: t.seq,
            kind: t.toast.kind,
            message: t.toast.message.clone(),
            phase: t.phase,
            undo: self.undo.as_ref().is_some_and(|u| u.seq == t.seq),
            open_log: t.toast.message == ToastMessage::CrashedLastTime,
        })
    }

    /// The user's accent: the preset they chose, or the Windows accent; `None` keeps the built-in blue.
    #[must_use]
    pub fn accent(&self) -> Option<Accent> {
        let system = self.os_look.accent.map(|c| [c.r, c.g, c.b]);
        crate::accent::resolve(self.settings.accent, system)
    }

    /// The look in force: the `theme` setting over what Windows says; high contrast always wins.
    fn look(&self) -> SystemLook {
        SystemLook {
            dark: match self.settings.theme {
                Theme::System => self.os_look.dark,
                Theme::Light => false,
                Theme::Dark => true,
            },
            high_contrast: self.os_look.high_contrast,
            accent: self.os_look.accent,
            reduce_motion: self.os_look.reduce_motion || self.settings.reduce_motion,
        }
    }

    /// Whether "Pause hotkeys" is on.
    #[must_use]
    pub fn hotkeys_paused(&self) -> bool {
        self.session.hotkeys_paused
    }

    /// Whether the Panel is meant to be on screen.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// The panel's model, for tests and the host.
    #[must_use]
    pub fn model(&self) -> &PanelModel {
        &self.model
    }

    /// The settings in force.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The library, for the host's start-up work and for tests.
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The platform services, for tests that look at what a use did to them.
    #[must_use]
    pub fn services(&self) -> &Services<C, K, T> {
        &self.services
    }

    // ---- Hotkeys, the Target and the window ---------------------------------------------------

    /// A global hotkey fired.
    pub fn on_hotkey(&mut self, action: HotkeyAction) -> Vec<Effect> {
        if self.session.hotkeys_paused {
            // A press that was already queued when the hotkeys were switched off.
            return Vec::new();
        }
        match action {
            // With the dock on, the Panel hotkey opens and closes the handle instead of
            // hiding the window: the handle is what keeps the Panel reachable.
            HotkeyAction::TogglePanel if self.visible && self.dock_active() => {
                self.dock_event(crate::dock::DockEvent::Hotkey)
            }
            HotkeyAction::TogglePanel if self.visible => self.hide(),
            HotkeyAction::TogglePanel => self.show(),
            HotkeyAction::Picker if self.picker.is_some() => {
                self.close_picker();
                Vec::new()
            }
            HotkeyAction::Picker => {
                self.open_picker();
                Vec::new()
            }
            HotkeyAction::Capture => {
                self.quick_capture();
                Vec::new()
            }
            HotkeyAction::PasteNext => {
                self.paste_next();
                Vec::new()
            }
        }
    }

    /// Shows the Panel (a second launch, the tray, the toggle hotkey). The Target is read
    /// first, while it still has the focus.
    pub fn show(&mut self) -> Vec<Effect> {
        self.on_target_changed();
        self.visible = true;
        // A Panel shown again is open: the host places it from the stored frame.
        self.dock = DockState::Expanded;
        self.shown_at = self.clock.now();
        vec![Effect::ShowPanel]
    }

    /// A second launch of the app: shows a hidden Panel, and opens the Main window when the
    /// Panel is already up (the user ran Snipdeck again because they wanted something more).
    pub fn second_launch(&mut self) -> Vec<Effect> {
        if self.visible {
            vec![Effect::OpenMainWindow]
        } else {
            self.show()
        }
    }

    /// Hides the Panel (Esc with nothing to clear, the close button, the toggle hotkey).
    pub fn hide(&mut self) -> Vec<Effect> {
        self.visible = false;
        self.panel_revealed = None;
        self.menu = PanelMenu::None;
        vec![Effect::HidePanel]
    }

    /// Hides the Panel because the user did not ask for it by the way back (the close button, a
    /// click elsewhere), and the first time ever says how to bring it back: a toast naming the
    /// hotkey and the tray click, remembered in the settings so it comes once. The toggle hotkey
    /// and the tray click hide without it: the user just used the way back.
    fn hide_with_hint(&mut self) -> Vec<Effect> {
        let effects = self.hide();
        if !self.settings.hide_hint_shown {
            self.update_settings(|s| s.hide_hint_shown = true);
            let chord = self.settings.hotkeys.toggle_panel.trim();
            let chord = (!chord.is_empty()).then(|| chord.to_owned());
            self.show_toast(Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::PanelHidden { chord },
            });
        }
        effects
    }

    /// The close button or Esc on an empty search. With `close_to_tray` on and a tray icon to
    /// get it back from, the Panel hides; otherwise closing it quits the app.
    pub fn close(&mut self) -> Vec<Effect> {
        if self.settings.close_to_tray && self.session.tray_available {
            self.hide_with_hint()
        } else {
            vec![Effect::Quit]
        }
    }

    /// The user did something to the tray icon.
    pub fn on_tray(&mut self, event: TrayEvent) -> Vec<Effect> {
        match event {
            TrayEvent::TogglePanel if self.visible => self.hide(),
            TrayEvent::TogglePanel | TrayEvent::OpenPanel => self.show(),
            TrayEvent::OpenSnipdeck => vec![Effect::OpenMainWindow],
            TrayEvent::HotkeysPaused(paused) => {
                self.session.hotkeys_paused = paused;
                vec![Effect::SetHotkeysPaused(paused)]
            }
            TrayEvent::Quit => vec![Effect::Quit],
            TrayEvent::IconPresent(present) => {
                // With an icon in the notification area closing the Panel hides it (when
                // `close_to_tray` is on); without one nothing could bring it back, so closing
                // quits. The icon comes and goes (Explorer restarts), so this follows the tray.
                self.session.tray_available = present;
                Vec::new()
            }
        }
    }

    /// The user is dragging the Panel by its header.
    pub fn drag(&mut self) -> Vec<Effect> {
        self.drag_started = self.dock_edge().is_some();
        vec![Effect::DragWindow]
    }

    /// The foreground window changed: the footer and the use label follow it. While the Picker
    /// or the fill form is open the Target stays what it was when they opened: they take the
    /// foreground themselves, and the window they came from is where the result goes.
    pub fn on_target_changed(&mut self) {
        if self.picker.is_none() && self.fill.is_none() {
            self.model.set_target(self.target.current());
        }
    }

    /// The foreground window changed: the Target follows it (see
    /// [`on_target_changed`](Self::on_target_changed)), and an unpinned Panel hides when
    /// "Hide the panel when it loses focus" is on. Only the Panel does this: the Main window,
    /// Settings and the editors are ordinary windows that stay until closed.
    pub fn on_foreground_changed(&mut self) -> Vec<Effect> {
        // A held reveal ends with the focus: the key release goes to the window that has the
        // focus now (Alt+Tab away from the Panel), so it never arrives here.
        self.panel_revealed = None;
        self.on_target_changed();
        if self.hides_on_blur() {
            self.hide_with_hint()
        } else {
            Vec::new()
        }
    }

    /// Whether a foreground change now should hide the Panel.
    fn hides_on_blur(&self) -> bool {
        self.visible
            && !self.on_top
            && self.settings.panel_hide_on_blur
            // The Picker and the fill form take the foreground themselves.
            && self.picker.is_none()
            && self.fill.is_none()
            // The tray, the desktop or the taskbar taking the foreground is no Target: the
            // user who just clicked the tray icon to open the Panel must not see it vanish.
            && self.target.current().is_some()
            && Duration::try_from(self.clock.now().duration_since(self.shown_at))
                .is_ok_and(|since| since > BLUR_GRACE)
    }

    // ---- Settings ----------------------------------------------------------------------------

    /// Changes the settings with `change` (then sanitised). A real change is applied now and
    /// scheduled to be written (see [`settings_wait`](Self::settings_wait)). Hotkey changes are
    /// not re-registered here: the host does that when a Settings window exists.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings)) {
        let mut next = self.settings.clone();
        change(&mut next);
        next.sanitize();
        if next == self.settings {
            return;
        }
        self.settings = next;
        self.model.set_policy(policy_of(&self.settings));
        let now = self.clock.now();
        if let Some(saver) = &mut self.saver {
            saver.changed(&self.settings, now);
        }
        self.dock_resync();
        self.sync_recent_gate();
    }

    /// The Main window was moved, resized, maximised or restored: remembers where it is, to
    /// be written after the quiet time and restored the next time it opens.
    pub fn remember_main_window(&mut self, place: crate::settings::WindowPlace) {
        self.update_settings(|s| s.main_window = Some(place));
    }

    /// The theme setting (`system`, `light`, `dark`).
    pub fn set_theme(&mut self, theme: Theme) {
        self.update_settings(|s| s.theme = theme);
    }

    /// How long until a changed setting is due to be written, if one waits. The host arms a
    /// timer for this and then calls [`save_settings_if_due`](Self::save_settings_if_due).
    #[must_use]
    pub fn settings_wait(&self) -> Option<Duration> {
        self.saver.as_ref()?.wait(self.clock.now())
    }

    /// Writes the changed settings if they are due. A failed write shows a toast (once until a
    /// write succeeds) and is retried by the saver.
    pub fn save_settings_if_due(&mut self) {
        let now = self.clock.now();
        let Some(saver) = &mut self.saver else {
            return;
        };
        match saver.poll(now) {
            Poll::Saved => self.session.save_failed = false,
            Poll::Failed(_) if !self.session.save_failed => {
                self.session.save_failed = true;
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::SettingsNotSaved,
                });
            }
            Poll::Idle | Poll::Waiting(_) | Poll::Failed(_) => {}
        }
    }

    /// Writes any changed settings now (at exit).
    ///
    /// # Errors
    ///
    /// The write error; the settings stay in memory only.
    pub fn flush_settings(&mut self) -> std::io::Result<()> {
        self.saver.as_mut().map_or(Ok(()), SettingsSaver::flush)
    }

    /// The pin button: keep the Panel on top or not.
    pub fn set_on_top(&mut self, on_top: bool) {
        self.on_top = on_top;
    }

    /// Sets the words a new list's and a new board's editor name starts from ("New list" gives
    /// "New list 1"), translated by the window layer: the controller has no `@tr`.
    pub fn set_new_name_bases(&mut self, list: &str, board: &str) {
        list.clone_into(&mut self.new_list_base);
        board.clone_into(&mut self.new_board_base);
    }

    /// A regular Snipdeck window (Main, Settings, an editor, Help) gained or lost the focus
    /// (`active`: one of them has it now). While one has it a pinned Panel is not topmost, so
    /// that window can cover it; once the focus is in another app the Panel is topmost again.
    pub fn set_own_window_active(&mut self, active: bool) {
        self.own_window_active = active;
        if active {
            // The Panel lost the focus to a Snipdeck window: no key release will reach it.
            self.panel_revealed = None;
        }
    }

    // ---- Gestures ----------------------------------------------------------------------------

    /// The search text changed.
    pub fn set_query(&mut self, query: &str) {
        self.model.set_query(query);
    }

    /// The keyboard selection moved by `delta` rows.
    pub fn move_selection(&mut self, delta: isize) {
        self.model.move_selection(delta);
    }

    /// The user picked tab `index`: shows that list and ends the search.
    pub fn select_tab(&mut self, index: usize) {
        let Some(tab) = self.model.tabs().into_iter().nth(index) else {
            return;
        };
        if self.model.select_list(&self.store, tab.id).is_err() {
            self.show_toast(failed());
        }
    }

    /// Row `row` of the Panel was dropped on tab `tab`: the item goes to the end of that list.
    pub fn panel_drop_on_tab(&mut self, row: usize, tab: usize) {
        let Some(item) = self.model.rows().get(row).map(|r| r.id) else {
            return;
        };
        let Some(target) = self.model.tabs().into_iter().nth(tab) else {
            return;
        };
        if self.store.item(item).is_ok_and(|i| i.list_id == target.id) {
            return;
        }
        match self.store.move_item(item, target.id, Placement::Last) {
            Ok(()) => {
                self.library_changed();
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                    return;
                }
                self.show_toast(neutral(ToastMessage::ItemsMoved {
                    count: 1,
                    list: target.name,
                }));
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// A drag of Panel row `from` ended on row `onto`: the item takes that row's place. Like the
    /// Main window, only a list in manual order, with no search running, can be arranged.
    pub fn panel_drop_row(&mut self, from: usize, onto: usize) {
        if !self.model.reorderable() {
            return;
        }
        let rows = self.model.rows();
        let (Some(moved), Some(target)) =
            (rows.get(from).map(|r| r.id), rows.get(onto).map(|r| r.id))
        else {
            return;
        };
        let Some(list) = self.model.list().map(|l| l.id) else {
            return;
        };
        let order = self.model.manual_order();
        let (Some(a), Some(b)) = (
            order.iter().position(|i| *i == moved),
            order.iter().position(|i| *i == target),
        ) else {
            return;
        };
        let Some(delta) = b.checked_signed_diff(a) else {
            return;
        };
        let Some((_, placement)) = reorder(&order, a, delta) else {
            return;
        };
        match self.store.move_item(moved, list, placement) {
            Ok(()) => {
                self.library_changed();
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                    return;
                }
                self.model.select_item(moved);
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Enter or a click on row `index`: uses it with `mode`.
    pub fn activate_row(&mut self, index: usize, mode: UseMode) -> UseResult {
        let Some(id) = self.model.rows().get(index).map(|r| r.id) else {
            return UseResult::Failed;
        };
        self.use_item(id, mode)
    }

    /// A digit 1 to 9 on an empty search.
    pub fn quick_use(&mut self, n: u8) -> UseResult {
        match self.model.quick_use(n) {
            Some(id) => self.use_item(id, UseMode::Paste),
            None => UseResult::Failed,
        }
    }

    /// A click on pinned chip `index`: uses it with `mode`.
    pub fn activate_pinned(&mut self, index: usize, mode: UseMode) -> UseResult {
        let Some(id) = self.model.pinned().get(index).map(|r| r.id) else {
            return UseResult::Failed;
        };
        self.use_item(id, mode)
    }

    /// Opens the board menu.
    pub fn open_board_menu(&mut self) {
        self.menu = PanelMenu::Board;
    }

    /// Closes the board menu without a choice.
    pub fn close_board_menu(&mut self) {
        if self.menu == PanelMenu::Board {
            self.menu = PanelMenu::None;
        }
    }

    /// The user chose board `index` in the menu.
    pub fn choose_board(&mut self, index: usize) {
        self.menu = PanelMenu::None;
        let Some(id) = self.model.boards().get(index).map(|b| b.id) else {
            return;
        };
        self.switch_board(id);
    }

    /// The `+` tab and the "New list" button of an empty board: adds a list to the shown board
    /// (or, in a library without boards, makes the starter board) and shows it.
    pub fn add_list(&mut self) {
        let now = self.clock.now();
        self.main.mark_stale();
        let added = crate::starter::add_list(&mut self.store, self.model.board(), now);
        let shown = added.and_then(|(board, list)| {
            self.model.select_board(&self.store, board)?;
            self.model.select_list(&self.store, list)
        });
        if shown.is_err() {
            self.show_toast(failed());
        }
    }

    /// Reads the library again into the Panel (and tells the Main window to).
    fn refresh_panel(&mut self) -> snipdeck_store::Result<()> {
        self.main.mark_stale();
        self.model.refresh(&self.store)
    }

    fn switch_board(&mut self, board: BoardId) {
        if self.model.select_board(&self.store, board).is_err() {
            self.show_toast(failed());
        }
    }

    // ---- Using an item -----------------------------------------------------------------------

    /// Uses `item`: pastes or copies it as `mode` and the Target allow, counts the use and
    /// raises a toast.
    pub fn use_item(&mut self, item: ItemId, mode: UseMode) -> UseResult {
        self.run_use(item, mode, BTreeMap::new())
    }

    // ---- The Picker --------------------------------------------------------------------------

    /// Opens the Picker over the whole library. The Target is read first, while it still has
    /// the focus: the Picker takes the foreground to type into, and the paste goes back to
    /// the window it came from.
    pub fn open_picker(&mut self) {
        self.fill = None;
        self.on_target_changed();
        match PickerModel::load_cached(&self.store, self.model.board(), &self.index_cache) {
            Ok(picker) => self.picker = Some(picker),
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Closes the Picker without using anything (Esc, the hotkey again) and hands the focus
    /// back to the Target.
    pub fn close_picker(&mut self) {
        if self.picker.take().is_some() {
            self.target.refocus();
        }
    }

    /// The Picker, while it is open.
    #[must_use]
    pub fn picker(&self) -> Option<&PickerModel> {
        self.picker.as_ref()
    }

    /// The Picker's search text changed.
    pub fn picker_query(&mut self, query: &str) {
        if let Some(picker) = &mut self.picker {
            picker.set_query(query);
        }
    }

    /// The Picker's scope changed.
    pub fn picker_scope(&mut self, scope: Scope) {
        if let Some(picker) = &mut self.picker {
            picker.set_scope(scope);
        }
    }

    /// The Picker's keyboard selection moved by `delta` rows.
    pub fn picker_move(&mut self, delta: isize) {
        if let Some(picker) = &mut self.picker {
            picker.move_selection(delta);
        }
    }

    /// Closes the Picker on result row `index` and returns its item, to be used with
    /// [`use_item`](Self::use_item). The host hides the window between the two calls, so the
    /// paste goes into the Target and not into the Picker.
    pub fn picker_take(&mut self, index: usize) -> Option<ItemId> {
        let item = self.picker.as_ref()?.item_at(index);
        // A row that is not there leaves the Picker open.
        if item.is_some() {
            self.picker = None;
        }
        item
    }

    /// [`picker_take`](Self::picker_take) and [`use_item`](Self::use_item) in one: Enter (or a
    /// click) on row `index` pastes it, Shift+Enter copies it.
    pub fn picker_use(&mut self, index: usize, mode: UseMode) -> UseResult {
        match self.picker_take(index) {
            Some(item) => self.use_item(item, mode),
            None => UseResult::Failed,
        }
    }

    /// Where the Target's text caret is on the screen, if it has one. Read it before a window
    /// of ours takes the foreground.
    #[must_use]
    pub fn target_caret(&self) -> Option<snipdeck_platform::route::ScreenRect> {
        self.target.caret()
    }

    /// [`picker_take`](Self::picker_take) for quick-use number `n`.
    pub fn picker_take_number(&mut self, n: u8) -> Option<ItemId> {
        let item = self.picker.as_ref()?.quick_use(n)?;
        self.picker = None;
        Some(item)
    }

    // ---- The template fill form --------------------------------------------------------------

    /// Opens the fill form for template `item`, to be used with `mode` once it is submitted.
    fn open_fill(&mut self, item: ItemId, mode: UseMode) -> Result<(), UseError> {
        let (title, source) =
            usecase::template_source(&self.store, item)?.ok_or(UseError::NotText)?;
        let template = Template::parse(&source)?;
        self.fill_seq += 1;
        self.fill = Some(FillModel::new(item, mode, title, template));
        Ok(())
    }

    /// The fill form's model, while it is open.
    #[must_use]
    pub fn fill(&self) -> Option<&FillModel> {
        self.fill.as_ref()
    }

    /// The user typed or chose `value` in field `index` of the fill form.
    pub fn fill_changed(&mut self, index: usize, value: &str) {
        if let Some(fill) = &mut self.fill {
            fill.set_value(index, value);
        }
    }

    /// Closes the fill form without using the template (Esc, the close button) and hands the
    /// focus back to the Target.
    pub fn cancel_fill(&mut self) {
        if self.fill.take().is_some() {
            self.target.refocus();
        }
    }

    /// The form was submitted with `values`, one per field in order. A required field left
    /// empty keeps the form open and returns `None`; otherwise the form closes and the
    /// result is to be run with [`run_fill`](Self::run_fill) (the host hides the window
    /// first).
    pub fn take_fill(&mut self, values: &[String]) -> Option<FillSubmission> {
        let fill = self.fill.as_mut()?;
        fill.set_values(values);
        if fill.first_missing().is_some() {
            return None;
        }
        let submission = FillSubmission {
            item: fill.item(),
            mode: fill.mode(),
            answers: fill.answers(),
        };
        self.fill = None;
        Some(submission)
    }

    /// Re-runs the use that stopped for answers, now with them.
    pub fn run_fill(&mut self, submission: FillSubmission) -> UseResult {
        self.run_use(submission.item, submission.mode, submission.answers)
    }

    /// [`take_fill`](Self::take_fill) and [`run_fill`](Self::run_fill) in one.
    pub fn submit_fill(&mut self, values: &[String]) -> UseResult {
        match self.take_fill(values) {
            Some(submission) => self.run_fill(submission),
            None => UseResult::NeedsAnswers,
        }
    }

    // ---- Quick capture and paste next --------------------------------------------------------

    /// The capture hotkey: saves the text on the clipboard as a new item at the top of the
    /// default list (the list the Panel shows), and says so in a toast.
    pub fn quick_capture(&mut self) {
        let text = self.services.clipboard.read_text();
        self.save_text(text.as_deref());
    }

    /// Text dropped on the Panel: saved like a capture, at the top of the list the Panel shows.
    pub fn panel_drop_text(&mut self, text: &str) {
        self.save_text(Some(text));
    }

    fn save_text(&mut self, text: Option<&str>) {
        let current = self.model.list().map(|l| l.id);
        let toast = match capture::default_list(&self.store, current) {
            Ok(Some(list)) => {
                match capture::capture_text(&mut self.store, &list, text, self.clock.now()) {
                    Ok(Captured::Saved { list, .. }) => {
                        if self.refresh_panel().is_err() {
                            self.show_toast(failed());
                        }
                        Toast {
                            kind: ToastKind::Success,
                            message: ToastMessage::Saved { list },
                        }
                    }
                    Ok(Captured::Empty) => neutral(ToastMessage::NothingToSave),
                    Ok(Captured::TooLarge) => neutral(ToastMessage::TooLargeToSave),
                    Err(_) => failed(),
                }
            }
            Ok(None) => neutral(ToastMessage::NoListToSave),
            Err(_) => failed(),
        };
        self.show_toast(toast);
    }

    fn stack_view(&self) -> Option<StackView> {
        if self.stack.is_empty() {
            return None;
        }
        let position = if self.stack_running {
            self.queue.position()
        } else {
            0
        };
        let next_title = self
            .stack
            .get(position)
            .and_then(|id| self.store.item(*id).ok())
            .map(|i| i.title)
            .unwrap_or_default();
        Some(StackView {
            count: self.stack.len(),
            running: self.stack_running,
            position,
            next_title,
            items: self.stack.clone(),
        })
    }

    /// Marks `item` for the paste stack, or unmarks it. Ignored while a stack runs.
    pub fn toggle_stack_mark(&mut self, item: ItemId) {
        if self.stack_running {
            return;
        }
        if let Some(at) = self.stack.iter().position(|id| *id == item) {
            self.stack.remove(at);
        } else {
            self.stack.push(item);
        }
    }

    /// "Start stack": Paste next now walks the marked items. A Supporter feature.
    pub fn start_stack(&mut self) {
        if self.stack.is_empty() {
            return;
        }
        if !self.entitlements.allows(Feature::PasteStack) {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::NeedsPro(Feature::PasteStack),
            });
            return;
        }
        self.queue.reset();
        self.stack_running = true;
    }

    /// Drops the marks and ends a running stack (Esc in the Panel, "End stack", or the last paste).
    pub fn end_stack(&mut self) {
        self.stack.clear();
        self.stack_running = false;
        self.queue.reset();
    }

    /// The paste-next hotkey: pastes the next item of the pinned strip (when the board has
    /// pinned items) or of the list the Panel shows. See [`paste_next`].
    pub fn paste_next(&mut self) -> Option<UseResult> {
        if !self.entitlements.allows(Feature::PasteStack) {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::NeedsPro(Feature::PasteStack),
            });
            return None;
        }
        self.on_target_changed();
        let pinned = self.model.pinned();
        let (source, ids) = match (self.model.board(), self.model.list()) {
            _ if self.stack_running => (paste_next::Source::Stack, self.stack.clone()),
            (Some(board), _) if !pinned.is_empty() => (
                paste_next::Source::Pinned(board),
                pinned.iter().map(|r| r.id).collect::<Vec<_>>(),
            ),
            (_, Some(list)) => (
                paste_next::Source::List(list.id),
                self.model.list_item_ids(),
            ),
            _ => {
                self.show_toast(neutral(ToastMessage::NothingToPasteNext));
                return None;
            }
        };
        match self.queue.peek(source, || ids) {
            paste_next::Next::Item(item) => Some(self.use_item(item, UseMode::Paste)),
            paste_next::Next::Empty => {
                self.show_toast(neutral(ToastMessage::NothingToPasteNext));
                None
            }
            paste_next::Next::Finished => {
                self.end_stack();
                self.show_toast(neutral(ToastMessage::PasteNextFinished));
                None
            }
        }
    }

    fn run_use(
        &mut self,
        item: ItemId,
        mode: UseMode,
        answers: BTreeMap<String, String>,
    ) -> UseResult {
        let route = self.model.route();
        let pasted_into = self.model.target().map(|t| t.app.clone());
        if mode != UseMode::Copy && matches!(route, Route::Paste { .. }) && self.target.refocus() {
            self.services.timing.sleep(REFOCUS_WAIT);
        }
        let request = UseRequest {
            item,
            mode,
            route: route.clone(),
            now: self.clock.now(),
            entitlements: self.entitlements,
            answers,
            // Only a template can say `{clipboard}`: other items never read what the user copied.
            clipboard_text: if self
                .store
                .item(item)
                .is_ok_and(|i| i.kind == snipdeck_core::ItemKind::Template)
            {
                self.services.clipboard.read_text()
            } else {
                None
            },
            tz: self.tz.clone(),
            locale: Locale::for_tag(&self.settings.language),
        };
        let mut env = Env {
            clipboard: &mut self.services.clipboard,
            keyboard: &mut self.services.keyboard,
            timing: &mut self.services.timing,
        };
        match usecase::use_item(&mut self.store, &mut env, &request) {
            Ok(used) => {
                self.note_activity();
                self.queue.used(item);
                self.schedule_clipboard_clear(item, used.outcome);
                self.show_toast(toast::after_use(
                    used.outcome,
                    &route,
                    pasted_into.as_deref(),
                ));
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                }
                UseResult::Done(used.outcome)
            }
            Err(UseError::NeedsAnswers(_)) => match self.open_fill(item, mode) {
                Ok(()) => UseResult::NeedsAnswers,
                Err(error) => {
                    self.show_toast(toast::after_error(&error));
                    UseResult::Failed
                }
            },
            Err(error) => {
                self.show_toast(toast::after_error(&error));
                UseResult::Failed
            }
        }
    }

    /// After a copy of a sensitive item, remembers its text on the clipboard to be cleared
    /// `clipboard_clear_secs` from now (0 = never). A paste restores the user's own clipboard,
    /// so it leaves nothing to clear.
    fn schedule_clipboard_clear(&mut self, item: ItemId, outcome: UseOutcome) {
        let life = Duration::from_secs(u64::from(self.settings.clipboard_clear_secs));
        if life.is_zero()
            || matches!(outcome, UseOutcome::Pasted)
            || !self.store.item(item).is_ok_and(|i| i.sensitive)
        {
            return;
        }
        let Ok(due) = self.clock.now().checked_add(life) else {
            return;
        };
        self.clipboard_clear = self
            .services
            .clipboard
            .read_text()
            .map(|text| (due, ClipboardDigest::of(&text)));
    }

    /// How long until the clipboard is to be cleared of a copied sensitive item, if one waits.
    /// The host arms a timer for this and then calls
    /// [`clear_clipboard_if_due`](Self::clear_clipboard_if_due).
    #[must_use]
    pub fn clipboard_clear_wait(&self) -> Option<Duration> {
        let (due, _) = self.clipboard_clear.as_ref()?;
        Some(Duration::try_from(due.duration_since(self.clock.now())).unwrap_or_default())
    }

    /// Clears the clipboard if the wait is over and it still holds the copied text.
    pub fn clear_clipboard_if_due(&mut self) {
        let Some((due, _)) = &self.clipboard_clear else {
            return;
        };
        if self.clock.now() < *due {
            return;
        }
        if let Some((_, digest)) = self.clipboard_clear.take()
            && let Some(text) = self.services.clipboard.read_text()
            && digest.matches(&text)
        {
            self.services.clipboard.clear_if_text(&text);
        }
    }

    // ---- Toasts ------------------------------------------------------------------------------

    /// Shows `toast`, replacing the one on screen. The host then calls
    /// [`toast_tick`](Self::toast_tick) after [`TOAST_ENTER`] and follows the delays it returns.
    pub fn show_toast(&mut self, toast: Toast) {
        // A locked sensitive item behind a master password asks for it, as well as saying so.
        if toast.message == ToastMessage::Locked {
            self.request_unlock();
        }
        self.toast_seq += 1;
        self.undo = None;
        self.toast = Some(ActiveToast {
            toast,
            phase: ToastPhase::Enter,
            seq: self.toast_seq,
            hovered: false,
            extended: 0,
        });
    }

    /// Shows `toast` with an Undo button that brings `targets` back from the Trash
    /// ([`main_undo`](Self::main_undo)). The toast stays for 5 s; a newer toast takes the
    /// button away (what was deleted is still in the Trash).
    pub fn show_undo_toast(&mut self, toast: Toast, targets: Vec<Trashable>) {
        self.show_toast(toast);
        self.undo = Some(Undo {
            seq: self.toast_seq,
            targets,
        });
    }

    /// The toast's button: "Open log" on the crash toast (opens the reports folder and closes
    /// the toast; a folder that cannot be opened says so), "Undo" on any other.
    pub fn toast_action(&mut self) {
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.toast.message == ToastMessage::CrashedLastTime)
        {
            let opened = self.log_opener.as_ref().is_some_and(|open| open().is_ok());
            if opened {
                self.toast = None;
                self.undo = None;
            } else {
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::CannotOpenLog,
                });
            }
            return;
        }
        self.main_undo();
    }

    /// Whether the caller should start the timer chain for toast `seq`: true once per toast,
    /// for whichever window sees it first (the Panel and the Main window share one toast).
    pub fn claim_toast_timer(&mut self, seq: u64) -> bool {
        if self.timer_seq == seq {
            return false;
        }
        self.timer_seq = seq;
        true
    }

    /// Reports hotkeys that could not be registered (the first one is shown).
    pub fn report_hotkey_problems(&mut self, problems: &[crate::host::HotkeyProblem]) {
        if let Some(problem) = problems.first() {
            let chord = problem.chord.clone();
            let message = match problem.failure {
                crate::host::HotkeyFailure::InUse => ToastMessage::HotkeyInUse { chord },
                crate::host::HotkeyFailure::Invalid => ToastMessage::HotkeyInvalid { chord },
                crate::host::HotkeyFailure::Unavailable => {
                    ToastMessage::HotkeyUnavailable { chord }
                }
            };
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message,
            });
        }
    }

    /// Advances toast `seq` to its next phase. Returns how long to wait before ticking again,
    /// or `None` when the toast is gone or `seq` is not the toast on screen any more.
    pub fn toast_tick(&mut self, seq: u64) -> Option<Duration> {
        let active = self.toast.as_mut().filter(|t| t.seq == seq)?;
        match active.phase {
            ToastPhase::Enter => {
                active.phase = ToastPhase::Hold;
                if self.undo.as_ref().is_some_and(|u| u.seq == seq) {
                    Some(TOAST_UNDO_HOLD)
                } else {
                    Some(TOAST_HOLD)
                }
            }
            ToastPhase::Hold if active.hovered && active.extended < TOAST_HOVER_RECHECKS => {
                active.extended += 1;
                Some(TOAST_HOVER_RECHECK)
            }
            ToastPhase::Hold
                if !active.hovered && active.extended > 0 && active.extended != u32::MAX =>
            {
                // The pointer left while the hold was extended: a fresh hold, once.
                active.extended = u32::MAX;
                Some(TOAST_HOLD)
            }
            ToastPhase::Hold => {
                active.phase = ToastPhase::Exit;
                Some(TOAST_EXIT)
            }
            ToastPhase::Exit => {
                self.toast = None;
                self.undo = None;
                None
            }
        }
    }

    /// The pointer entered (`on`) or left the toast, or the Undo button gained or lost focus. A
    /// toast in use stays: its hold is extended while this is on, and runs a fresh hold after.
    pub fn toast_hover(&mut self, on: bool) {
        if let Some(active) = self.toast.as_mut() {
            active.hovered = on;
        }
    }

    /// Logs what a maintenance run did (`SNIPDECK_LOG=1`) and shows its toast when it has one:
    /// a damaged library, or a crash last time. See [`crate::maintenance`].
    pub fn report_maintenance(&mut self, outcome: crate::maintenance::MaintenanceOutcome) {
        crate::trace!("maintenance: {}", outcome.log_line());
        if let Some(toast) = outcome.into_toast() {
            self.show_toast(toast);
        }
    }
}

fn neutral(message: ToastMessage) -> Toast {
    Toast {
        kind: ToastKind::Neutral,
        message,
    }
}

fn failed() -> Toast {
    Toast {
        kind: ToastKind::Danger,
        message: ToastMessage::Failed,
    }
}

fn policy_of(settings: &Settings) -> snipdeck_platform::route::Policy {
    snipdeck_platform::route::Policy {
        no_paste_apps: settings.no_paste_apps.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{FakeTargets, FixedClock};
    use snipdeck_core::{
        Board, Content, Format, Formats, Inline, Item, ItemKind, List, Name, Placement, Tier,
        Timestamp,
    };
    use snipdeck_platform::{
        keys::{Key, KeyEvent},
        payload::Data,
        route::{Focus, Target},
        testing::{FakeClipboard, FakeKeyboard, FakeTiming},
    };
    use snipdeck_store::Store;

    mod dock;
    mod onboarding;
    mod recent;

    type Ctl = Controller<Store, FakeClipboard, FakeKeyboard, FakeTiming>;

    fn at(s: &str) -> Timestamp {
        s.parse().expect("timestamp")
    }

    fn outlook() -> Target {
        Target {
            app: "OUTLOOK.EXE".into(),
            title: "New message".into(),
            elevated: false,
            focus: Focus::Caret,
        }
    }

    struct World {
        ctl: Ctl,
        targets: FakeTargets,
        clock: FixedClock,
        ids: Vec<ItemId>,
        links_list: snipdeck_core::ListId,
    }

    fn add(
        store: &mut Store,
        list: snipdeck_core::ListId,
        kind: ItemKind,
        title: &str,
        text: &str,
    ) -> ItemId {
        let mut item = Item::new(list, title.into(), kind, at("2026-10-17T09:00:00Z"));
        item.preview = Item::preview_of(text, false);
        let content = Content::Inline(Inline::new(text.as_bytes().to_vec()).expect("inline"));
        let formats = Formats::new(vec![Format::Text(content)]).expect("formats");
        store
            .insert_item(&item, &formats, Placement::Last)
            .expect("insert");
        item.id
    }

    /// How a test world differs from the default one.
    struct Opts {
        target: Option<Target>,
        settings: Settings,
        tier: Tier,
        /// Indexes into `World::ids` of the items that are pinned.
        pinned: Vec<usize>,
        /// What is on the clipboard (as if the user had just copied it).
        clipboard: Option<&'static str>,
        /// Writes to the clipboard fail.
        clipboard_busy: bool,
    }

    impl Default for Opts {
        fn default() -> Self {
            Self {
                target: Some(outlook()),
                settings: Settings::default(),
                tier: Tier::Pro,
                pinned: Vec::new(),
                clipboard: None,
                clipboard_busy: false,
            }
        }
    }

    /// Board "Work" with lists Replies (3 items + a template) and Links (1), pasting into
    /// Outlook, on a fixed clock.
    fn world_with(target: Option<Target>, settings: Settings) -> World {
        build(Opts {
            target,
            settings,
            ..Opts::default()
        })
    }

    fn build(opts: Opts) -> World {
        let Opts {
            target,
            settings,
            tier,
            pinned,
            clipboard,
            clipboard_busy,
        } = opts;
        let now = at("2026-10-17T09:00:00Z");
        let mut store = Store::open_in_memory().expect("store");
        let board = Board::new(Name::new("Work").expect("n"), now);
        store.insert_board(&board, Placement::Last).expect("board");
        let replies = List::new(board.id, Name::new("Replies").expect("n"), now);
        let links = List::new(board.id, Name::new("Links").expect("n"), now);
        store.insert_list(&replies, Placement::Last).expect("list");
        store.insert_list(&links, Placement::Last).expect("list");
        let mut ids = Vec::new();
        for (title, text) in [("One", "first"), ("Two", "second"), ("Three", "third")] {
            ids.push(add(&mut store, replies.id, ItemKind::Text, title, text));
        }
        ids.push(add(
            &mut store,
            replies.id,
            ItemKind::Template,
            "Greeting",
            "Hi {field:Name}",
        ));
        ids.push(add(
            &mut store,
            links.id,
            ItemKind::Link,
            "Docs",
            "https://example.com",
        ));
        for n in pinned {
            let mut item = store.item(ids[n]).expect("item");
            item.pinned = true;
            store.update_item(&item, None).expect("pin");
        }
        let targets = FakeTargets::new(target);
        let clock = FixedClock::new(at("2026-10-17T10:30:00Z"));
        let mut clipboard_fake = FakeClipboard {
            asks_after_polls: Some(1),
            fail_write: clipboard_busy,
            ..FakeClipboard::default()
        };
        if let Some(text) = clipboard {
            clipboard_fake.written.push(
                snipdeck_platform::payload::Payload::new(vec![Data::Text(text.into())])
                    .expect("payload"),
            );
        }
        let services = Services {
            clipboard: clipboard_fake,
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        };
        let mut ctl = Controller::new(
            store,
            services,
            Box::new(targets.clone()),
            Box::new(clock.clone()),
            settings,
            Entitlements::new(tier),
        )
        .expect("controller");
        // Never the machine's zone: a test's output must not depend on where it runs.
        ctl.tz = TimeZone::UTC;
        World {
            ctl,
            targets,
            clock,
            ids,
            links_list: links.id,
        }
    }

    fn world() -> World {
        world_with(Some(outlook()), Settings::default())
    }

    fn uses(w: &World, id: ItemId) -> u32 {
        w.ctl.store().item(id).expect("item").use_count
    }

    #[test]
    fn the_view_shows_the_board_the_rows_and_where_a_click_goes() {
        let w = world();
        let view = w.ctl.view();
        assert_eq!(view.board_name, "Work");
        assert_eq!(view.tabs.len(), 2);
        assert_eq!(view.current_tab, 0);
        assert_eq!(view.rows.len(), 4);
        assert_eq!(view.rows[0].number, 1);
        assert_eq!(
            view.footer,
            Footer::PastesInto {
                app: "Outlook".into(),
                title: "New message".into()
            }
        );
        assert_eq!(view.use_label, UseLabel::Paste);
        assert!(view.on_top && !view.dark && view.toast.is_none() && view.board_menu.is_none());
    }

    #[test]
    fn the_target_changing_updates_the_footer() {
        let mut w = world();
        w.targets.set(None);
        w.ctl.on_target_changed();
        let view = w.ctl.view();
        assert_eq!(view.footer, Footer::Copies);
        assert_eq!(view.use_label, UseLabel::Copy);
    }

    #[test]
    fn activating_a_row_pastes_it_counts_the_use_and_toasts() {
        let mut w = world();
        let result = w.ctl.activate_row(1, UseMode::Paste);
        assert_eq!(result, UseResult::Done(UseOutcome::Pasted));
        let services = w.ctl.services();
        assert_eq!(
            services.clipboard.written[0].data(),
            [Data::Text("second".into())]
        );
        assert_eq!(
            services.keyboard.sent.len(),
            1,
            "one batch of key events: {:?}",
            services.keyboard.sent
        );
        assert!(
            services.keyboard.sent[0].contains(&KeyEvent {
                key: Key::V,
                down: true
            }),
            "Ctrl+V was injected"
        );
        assert!(services.timing.slept.contains(&REFOCUS_WAIT));
        assert_eq!(w.targets.refocus_count(), 1);
        assert_eq!(uses(&w, w.ids[1]), 1);
        assert_eq!(
            w.ctl.store().item(w.ids[1]).expect("item").last_used_at,
            Some(w.clock.now())
        );
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(toast.kind, ToastKind::Success);
        assert_eq!(
            toast.message,
            ToastMessage::Pasted {
                app: "Outlook".into()
            }
        );
    }

    #[test]
    fn a_template_fills_clipboard_from_what_the_user_copied() {
        let mut w = build(Opts {
            clipboard: Some("from the clipboard"),
            ..Opts::default()
        });
        let list = w.ctl.model.list().expect("list").id;
        let id = add(
            &mut w.ctl.store,
            list,
            ItemKind::Template,
            "Echo",
            "Got: {clipboard}",
        );
        assert_eq!(
            w.ctl.use_item(id, UseMode::Copy),
            UseResult::Done(UseOutcome::Copied)
        );
        assert_eq!(
            w.ctl.services.clipboard.read_text().as_deref(),
            Some("Got: from the clipboard")
        );
    }

    fn make_sensitive(w: &mut World, id: ItemId) {
        let mut item = w.ctl.store.item(id).expect("item");
        item.sensitive = true;
        item.preview.clear();
        w.ctl.store.update_item(&item, None).expect("update");
    }

    #[test]
    fn a_copied_sensitive_item_leaves_the_clipboard_after_thirty_seconds() {
        let mut w = world();
        let id = w.ids[0];
        make_sensitive(&mut w, id);
        w.ctl.activate_row(0, UseMode::Copy);
        assert_eq!(w.ctl.clipboard_clear_wait(), Some(Duration::from_secs(30)));
        w.clock.set(at("2026-10-17T10:30:29Z"));
        w.ctl.clear_clipboard_if_due();
        assert_eq!(
            w.ctl.services.clipboard.read_text().as_deref(),
            Some("first"),
            "not yet"
        );
        w.clock.set(at("2026-10-17T10:30:30Z"));
        w.ctl.clear_clipboard_if_due();
        assert_eq!(w.ctl.services.clipboard.read_text(), None);
        assert_eq!(w.ctl.clipboard_clear_wait(), None);
    }

    #[test]
    fn the_clipboard_clear_time_is_the_setting_and_zero_turns_it_off() {
        let mut w = world();
        let id = w.ids[0];
        make_sensitive(&mut w, id);
        w.ctl.update_settings(|s| s.clipboard_clear_secs = 10);
        w.ctl.activate_row(0, UseMode::Copy);
        assert_eq!(w.ctl.clipboard_clear_wait(), Some(Duration::from_secs(10)));
        w.clock.set(at("2026-10-17T10:30:10Z"));
        w.ctl.clear_clipboard_if_due();
        assert_eq!(w.ctl.services.clipboard.read_text(), None);

        w.ctl.update_settings(|s| s.clipboard_clear_secs = 0);
        w.ctl.activate_row(0, UseMode::Copy);
        assert_eq!(w.ctl.clipboard_clear_wait(), None, "0 = never clear");
    }

    #[test]
    fn a_newer_copy_is_not_cleared_with_the_sensitive_one() {
        let mut w = world();
        let id = w.ids[0];
        make_sensitive(&mut w, id);
        w.ctl.activate_row(0, UseMode::Copy);
        w.ctl.activate_row(1, UseMode::Copy);
        w.clock.set(at("2026-10-17T10:31:00Z"));
        w.ctl.clear_clipboard_if_due();
        assert_eq!(
            w.ctl.services.clipboard.read_text().as_deref(),
            Some("second")
        );
    }

    #[test]
    fn copy_only_leaves_the_item_on_the_clipboard_and_sends_no_keys() {
        let mut w = world();
        let result = w.ctl.activate_row(0, UseMode::Copy);
        assert_eq!(result, UseResult::Done(UseOutcome::Copied));
        assert_eq!(
            w.ctl.services().keyboard.sent,
            Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
        );
        assert_eq!(w.ctl.services().clipboard.log, ["persistent:1"]);
        assert_eq!(
            w.targets.refocus_count(),
            0,
            "a copy never touches the Target"
        );
        assert_eq!(uses(&w, w.ids[0]), 1);
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::Copied
        );
    }

    #[test]
    fn quick_use_pastes_the_nth_row() {
        let mut w = world();
        assert_eq!(w.ctl.quick_use(3), UseResult::Done(UseOutcome::Pasted));
        assert_eq!(uses(&w, w.ids[2]), 1);
        assert_eq!(w.ctl.quick_use(9), UseResult::Failed, "no ninth row");
        assert_eq!(uses(&w, w.ids[0]), 0);
    }

    #[test]
    fn with_no_target_a_click_copies_and_says_so() {
        let mut w = world_with(None, Settings::default());
        assert_eq!(w.ctl.view().footer, Footer::Copies);
        assert_eq!(
            w.ctl.activate_row(0, UseMode::Paste),
            UseResult::Done(UseOutcome::Copied)
        );
        assert_eq!(
            w.ctl.services().keyboard.sent,
            Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
        );
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::Copied
        );
    }

    #[test]
    fn an_app_the_user_excluded_is_copied_into_not_pasted() {
        let settings = Settings {
            no_paste_apps: vec!["outlook.exe".into()],
            ..Settings::default()
        };
        let mut w = world_with(Some(outlook()), settings);
        assert_eq!(w.ctl.view().footer, Footer::Copies);
        w.ctl.activate_row(0, UseMode::Paste);
        assert_eq!(
            w.ctl.services().keyboard.sent,
            Vec::<Vec<snipdeck_platform::keys::KeyEvent>>::new()
        );
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::CopiedExcluded {
                app: "Outlook".into()
            }
        );
    }

    #[test]
    fn a_failed_clipboard_is_a_danger_toast_and_not_counted() {
        let mut w = world();
        w.ctl.services.clipboard.fail_write = true;
        assert_eq!(w.ctl.activate_row(0, UseMode::Paste), UseResult::Failed);
        assert_eq!(uses(&w, w.ids[0]), 0);
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(
            (toast.kind, toast.message),
            (ToastKind::Danger, ToastMessage::ClipboardBusy)
        );
    }

    #[test]
    fn a_row_that_does_not_exist_is_ignored() {
        let mut w = world();
        assert_eq!(w.ctl.activate_row(40, UseMode::Paste), UseResult::Failed);
        assert_eq!(w.ctl.activate_pinned(0, UseMode::Paste), UseResult::Failed);
        assert!(w.ctl.view().toast.is_none());
    }

    #[test]
    fn a_template_with_prompts_opens_the_fill_form_then_pastes_with_the_answers() {
        let mut w = world();
        let greeting = w.ids[3];
        assert_eq!(
            w.ctl.activate_row(3, UseMode::Paste),
            UseResult::NeedsAnswers
        );
        let view = w.ctl.view();
        let form = view.fill.expect("the form is open");
        assert_eq!(form.title, "Greeting");
        assert_eq!(form.fields.len(), 1);
        assert_eq!(form.fields[0].label, "Name");
        assert!(form.fields[0].required);
        assert_eq!(form.preview, "Hi ");
        assert_eq!(view.toast, None, "the form is the answer, not a toast");
        assert_eq!(uses(&w, greeting), 0);
        assert!(
            w.ctl.services().clipboard.log.is_empty(),
            "nothing touched the clipboard"
        );

        w.ctl.fill_changed(0, "Sam");
        assert_eq!(w.ctl.view().fill.expect("form").preview, "Hi Sam");
        assert_eq!(
            w.ctl.submit_fill(&["Sam".to_owned()]),
            UseResult::Done(UseOutcome::Pasted)
        );
        assert!(w.ctl.fill().is_none() && w.ctl.view().fill.is_none());
        assert_eq!(
            w.ctl.services().clipboard.written[0].data(),
            [Data::Text("Hi Sam".into())]
        );
        assert_eq!(uses(&w, greeting), 1);
    }

    #[test]
    fn a_required_field_left_empty_keeps_the_form_open_and_pastes_nothing() {
        let mut w = world();
        w.ctl.activate_row(3, UseMode::Paste);
        assert_eq!(
            w.ctl.submit_fill(&["  ".to_owned()]),
            UseResult::NeedsAnswers
        );
        assert!(w.ctl.view().fill.is_some());
        assert_eq!(w.ctl.services().clipboard.log, Vec::<String>::new());
        assert_eq!(uses(&w, w.ids[3]), 0);
        // Fixing it works.
        assert_eq!(
            w.ctl.submit_fill(&["Ada".to_owned()]),
            UseResult::Done(UseOutcome::Pasted)
        );
    }

    #[test]
    fn copying_a_template_asks_the_same_questions_and_copies() {
        let mut w = world();
        w.ctl.activate_row(3, UseMode::Copy);
        assert_eq!(
            w.ctl.submit_fill(&["Sam".to_owned()]),
            UseResult::Done(UseOutcome::Copied)
        );
        assert_eq!(w.ctl.services().keyboard.sent, Vec::<Vec<KeyEvent>>::new());
    }

    #[test]
    fn cancelling_the_form_forgets_it_and_hands_the_focus_back_once() {
        let mut w = world();
        w.ctl.activate_row(3, UseMode::Paste);
        let refocused = w.targets.refocus_count();
        w.ctl.cancel_fill();
        assert!(w.ctl.view().fill.is_none());
        assert_eq!(w.targets.refocus_count(), refocused + 1);
        w.ctl.cancel_fill();
        assert_eq!(w.targets.refocus_count(), refocused + 1, "nothing was open");
        assert_eq!(
            w.ctl.submit_fill(&["Sam".to_owned()]),
            UseResult::NeedsAnswers
        );
        assert_eq!(uses(&w, w.ids[3]), 0);
    }

    #[test]
    fn every_form_that_opens_is_a_new_one_for_the_window() {
        let mut w = world();
        w.ctl.activate_row(3, UseMode::Paste);
        let first = w.ctl.view().fill.expect("form").seq;
        w.ctl.fill_changed(0, "typed");
        assert_eq!(w.ctl.view().fill.expect("form").seq, first, "same form");
        w.ctl.cancel_fill();
        w.ctl.activate_row(3, UseMode::Paste);
        assert_ne!(w.ctl.view().fill.expect("form").seq, first);
        assert_eq!(w.ctl.view().fill.expect("form").fields[0].value, "");
    }

    #[test]
    fn a_free_user_is_told_about_pro_instead_of_being_asked() {
        let mut w = build(Opts {
            tier: Tier::Free,
            ..Opts::default()
        });
        assert_eq!(w.ctl.activate_row(3, UseMode::Paste), UseResult::Failed);
        assert!(w.ctl.view().fill.is_none());
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(
            (toast.kind, toast.message),
            (
                ToastKind::Danger,
                ToastMessage::NeedsPro(Feature::PromptedFields)
            )
        );
    }

    #[test]
    fn searching_filters_rows_across_lists_and_resets_the_selection() {
        let mut w = world();
        w.ctl.move_selection(2);
        assert_eq!(w.ctl.view().selected, 2);
        w.ctl.set_query("docs");
        let view = w.ctl.view();
        assert_eq!(view.query, "docs");
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0].id, w.ids[4]);
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn picking_a_tab_shows_that_list_and_ends_the_search() {
        let mut w = world();
        w.ctl.set_query("one");
        w.ctl.select_tab(1);
        let view = w.ctl.view();
        assert_eq!(view.current_tab, 1);
        assert_eq!(view.query, "");
        assert_eq!(view.rows.len(), 1);
        assert_eq!(view.rows[0].id, w.ids[4]);
        w.ctl.select_tab(9);
        assert_eq!(
            w.ctl.view().current_tab,
            1,
            "an unknown tab changes nothing"
        );
        assert_eq!(w.ctl.model().list().map(|l| l.id), Some(w.links_list));
    }

    #[test]
    fn the_board_menu_lists_boards_and_choosing_one_closes_it() {
        let mut w = world();
        w.ctl.open_board_menu();
        assert_eq!(w.ctl.view().board_menu, Some(vec!["Work".to_owned()]));
        w.ctl.choose_board(0);
        assert_eq!(w.ctl.view().board_menu, None);
        w.ctl.open_board_menu();
        w.ctl.close_board_menu();
        assert_eq!(w.ctl.view().board_menu, None);
    }

    #[test]
    fn the_toggle_hotkey_hides_and_shows_and_rereads_the_target_on_show() {
        let mut w = world();
        assert!(w.ctl.visible());
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::TogglePanel),
            [Effect::HidePanel]
        );
        assert!(!w.ctl.visible());
        w.targets.set(None);
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::TogglePanel),
            [Effect::ShowPanel]
        );
        assert!(w.ctl.visible());
        assert_eq!(
            w.ctl.view().footer,
            Footer::Copies,
            "the Target was read on show"
        );
    }

    fn picker_titles(w: &World) -> Vec<String> {
        w.ctl
            .view()
            .picker
            .expect("the Picker is open")
            .rows
            .into_iter()
            .map(|r| r.title)
            .collect()
    }

    fn pasted(w: &World) -> Vec<String> {
        w.ctl
            .services()
            .clipboard
            .written
            .iter()
            .filter_map(|p| match &p.data()[0] {
                Data::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_picker_hotkey_opens_the_picker_over_the_library_and_again_closes_it() {
        let mut w = world();
        assert!(w.ctl.view().picker.is_none());
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), []);
        let picker = w.ctl.view().picker.expect("open");
        assert_eq!(picker.rows.len(), 5, "every item of every list");
        assert_eq!(
            (picker.scope, picker.query.as_str(), picker.selected),
            (Scope::All, "", 0)
        );
        assert_eq!(
            picker.footer,
            Footer::PastesInto {
                app: "Outlook".into(),
                title: "New message".into()
            }
        );
        assert!(w.ctl.visible(), "the Panel is not touched");
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), []);
        assert!(w.ctl.view().picker.is_none());
    }

    #[test]
    fn the_picker_searches_by_scope_and_moves_a_selection() {
        let mut w = build(Opts {
            pinned: vec![1, 4],
            ..Opts::default()
        });
        w.ctl.on_hotkey(HotkeyAction::Picker);
        w.ctl.picker_query("docs");
        assert_eq!(picker_titles(&w), ["Docs"]);
        w.ctl.picker_query("");
        w.ctl.picker_scope(Scope::Pinned);
        assert_eq!(picker_titles(&w), ["Two", "Docs"]);
        assert_eq!(w.ctl.view().picker.expect("open").scope, Scope::Pinned);
        w.ctl.picker_scope(Scope::Board);
        assert_eq!(
            picker_titles(&w).len(),
            5,
            "everything is on the Work board"
        );
        w.ctl.picker_move(2);
        assert_eq!(w.ctl.view().picker.expect("open").selected, 2);
        w.ctl.picker_query("o");
        assert_eq!(w.ctl.view().picker.expect("open").selected, 0);
        // With the Picker shut these are ignored.
        w.ctl.close_picker();
        w.ctl.picker_query("x");
        w.ctl.picker_scope(Scope::All);
        w.ctl.picker_move(1);
        assert!(w.ctl.view().picker.is_none());
    }

    #[test]
    fn enter_in_the_picker_pastes_counts_the_use_and_closes_it() {
        let mut w = world();
        w.ctl.on_hotkey(HotkeyAction::Picker);
        w.ctl.picker_query("two");
        let refocused = w.targets.refocus_count();
        assert_eq!(
            w.ctl.picker_use(0, UseMode::Paste),
            UseResult::Done(UseOutcome::Pasted)
        );
        assert!(w.ctl.view().picker.is_none());
        assert_eq!(pasted(&w), ["second"]);
        assert_eq!(uses(&w, w.ids[1]), 1);
        assert_eq!(w.ctl.services().keyboard.sent.len(), 1, "Ctrl+V was sent");
        assert_eq!(w.targets.refocus_count(), refocused + 1);
        assert!(w.ctl.view().toast.is_some());
    }

    #[test]
    fn shift_enter_in_the_picker_only_copies() {
        let mut w = world();
        w.ctl.on_hotkey(HotkeyAction::Picker);
        assert_eq!(
            w.ctl.picker_use(0, UseMode::Copy),
            UseResult::Done(UseOutcome::Copied)
        );
        assert_eq!(w.ctl.services().keyboard.sent, Vec::<Vec<KeyEvent>>::new());
        assert_eq!(pasted(&w), ["first"]);
    }

    #[test]
    fn a_number_key_takes_the_nth_row_and_a_missing_row_leaves_the_picker_open() {
        let mut w = world();
        w.ctl.on_hotkey(HotkeyAction::Picker);
        assert_eq!(w.ctl.picker_take_number(10), None);
        assert_eq!(w.ctl.picker_take_number(0), None);
        assert!(w.ctl.view().picker.is_some());
        assert_eq!(w.ctl.picker_use(9, UseMode::Paste), UseResult::Failed);
        assert!(w.ctl.view().picker.is_some(), "no such row: still open");
        assert_eq!(w.ctl.picker_take_number(2), Some(w.ids[1]));
        assert!(w.ctl.view().picker.is_none());
        // The use itself is a separate call, after the host has hidden the window.
        assert_eq!(
            w.ctl.use_item(w.ids[1], UseMode::Paste),
            UseResult::Done(UseOutcome::Pasted)
        );
    }

    #[test]
    fn a_template_chosen_in_the_picker_closes_it_and_opens_the_fill_form() {
        let mut w = world();
        w.ctl.on_hotkey(HotkeyAction::Picker);
        w.ctl.picker_query("greeting");
        assert_eq!(w.ctl.picker_use(0, UseMode::Paste), UseResult::NeedsAnswers);
        let view = w.ctl.view();
        assert!(view.picker.is_none());
        assert_eq!(view.fill.expect("form").title, "Greeting");
        // And the other way: summoning the Picker drops a form that was open.
        w.ctl.on_hotkey(HotkeyAction::Picker);
        assert!(w.ctl.view().fill.is_none() && w.ctl.view().picker.is_some());
    }

    #[test]
    fn escape_closes_the_picker_and_hands_the_focus_back_once() {
        let mut w = world();
        w.ctl.on_hotkey(HotkeyAction::Picker);
        let refocused = w.targets.refocus_count();
        w.ctl.close_picker();
        assert!(w.ctl.view().picker.is_none());
        assert_eq!(w.targets.refocus_count(), refocused + 1);
        w.ctl.close_picker();
        assert_eq!(w.targets.refocus_count(), refocused + 1);
        assert_eq!(uses(&w, w.ids[0]), 0);
    }

    #[test]
    fn the_target_is_read_when_the_picker_opens_and_kept_while_it_is_open() {
        let mut w = world();
        // The user moved to a window with nothing to paste into; the controller has not heard.
        w.targets.set(None);
        assert!(matches!(w.ctl.view().footer, Footer::PastesInto { .. }));
        w.ctl.on_hotkey(HotkeyAction::Picker);
        assert_eq!(w.ctl.view().picker.expect("open").footer, Footer::Copies);

        // The Picker itself takes the foreground: that must not become the Target.
        w.targets.set(Some(outlook()));
        w.ctl.on_target_changed();
        assert_eq!(w.ctl.view().picker.expect("open").footer, Footer::Copies);
        w.ctl.close_picker();
        w.ctl.on_target_changed();
        assert!(matches!(w.ctl.view().footer, Footer::PastesInto { .. }));
    }

    #[test]
    fn quick_capture_saves_the_clipboard_text_at_the_top_of_the_shown_list() {
        let mut w = build(Opts {
            clipboard: Some("remember the milk"),
            ..Opts::default()
        });
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::Capture), []);
        let view = w.ctl.view();
        assert_eq!(view.rows.len(), 5);
        assert_eq!(view.rows[0].preview, "remember the milk");
        let toast = view.toast.expect("toast");
        assert_eq!(
            (toast.kind, toast.message),
            (
                ToastKind::Success,
                ToastMessage::Saved {
                    list: "Replies".into()
                }
            )
        );
        // The list the Panel shows is the default: switch tabs and capture again.
        w.ctl.select_tab(1);
        w.ctl.on_hotkey(HotkeyAction::Capture);
        let view = w.ctl.view();
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[0].preview, "remember the milk");
        assert_eq!(
            view.toast.expect("toast").message,
            ToastMessage::Saved {
                list: "Links".into()
            }
        );
    }

    #[test]
    fn quick_capture_with_nothing_on_the_clipboard_saves_nothing() {
        let mut w = world();
        w.ctl.on_hotkey(HotkeyAction::Capture);
        assert_eq!(w.ctl.view().rows.len(), 4);
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(
            (toast.kind, toast.message),
            (ToastKind::Neutral, ToastMessage::NothingToSave)
        );
    }

    #[test]
    fn quick_capture_without_any_list_says_so() {
        let now = at("2026-10-17T09:00:00Z");
        let mut store = Store::open_in_memory().expect("store");
        let board = Board::new(Name::new("Empty").expect("n"), now);
        store.insert_board(&board, Placement::Last).expect("board");
        let mut clipboard = FakeClipboard::default();
        clipboard.written.push(
            snipdeck_platform::payload::Payload::new(vec![Data::Text("x".into())])
                .expect("payload"),
        );
        let mut ctl = Controller::new(
            store,
            Services {
                clipboard,
                keyboard: FakeKeyboard::default(),
                timing: FakeTiming::default(),
            },
            Box::new(FakeTargets::new(None)),
            Box::new(FixedClock::new(now)),
            Settings::default(),
            Entitlements::new(Tier::Pro),
        )
        .expect("controller");
        ctl.on_hotkey(HotkeyAction::Capture);
        assert_eq!(
            ctl.view().toast.expect("toast").message,
            ToastMessage::NoListToSave
        );
    }

    #[test]
    fn a_marked_stack_is_pasted_in_the_order_it_was_marked_and_then_ends() {
        let mut w = world();
        w.ctl.toggle_stack_mark(w.ids[2]);
        w.ctl.toggle_stack_mark(w.ids[0]);
        w.ctl.toggle_stack_mark(w.ids[1]);
        w.ctl.toggle_stack_mark(w.ids[1]);
        let stack = w.ctl.view().stack.expect("marks show");
        assert_eq!((stack.count, stack.running), (2, false));

        w.ctl.start_stack();
        let stack = w.ctl.view().stack.expect("running");
        assert_eq!(
            (stack.running, stack.position, stack.next_title.as_str()),
            (true, 0, "Three")
        );

        w.ctl.paste_next();
        assert_eq!(w.ctl.view().stack.expect("running").position, 1);
        w.ctl.paste_next();
        assert_eq!(pasted(&w), ["third", "first"]);

        assert_eq!(w.ctl.paste_next(), None);
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::PasteNextFinished
        );
        assert_eq!(
            w.ctl.view().stack,
            None,
            "finishing the queue ends the stack"
        );
    }

    #[test]
    fn ending_a_stack_forgets_the_marks_and_paste_next_goes_back_to_the_list() {
        let mut w = world();
        w.ctl.toggle_stack_mark(w.ids[2]);
        w.ctl.start_stack();
        w.ctl.end_stack();
        assert_eq!(w.ctl.view().stack, None);
        w.ctl.paste_next();
        assert_eq!(pasted(&w), ["first"]);
    }

    #[test]
    fn starting_a_stack_is_a_pro_feature_and_needs_marks() {
        let mut w = build(Opts {
            tier: Tier::Free,
            ..Opts::default()
        });
        w.ctl.toggle_stack_mark(w.ids[0]);
        w.ctl.start_stack();
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::NeedsPro(Feature::PasteStack)
        );
        assert!(!w.ctl.view().stack.expect("marks stay").running);

        let mut w = world();
        w.ctl.start_stack();
        assert_eq!(w.ctl.view().stack, None, "nothing marked, nothing starts");
    }

    #[test]
    fn the_item_menu_marks_and_unmarks_for_the_stack() {
        let mut w = world();
        let id = w.ids[0];
        assert!(
            w.ctl
                .item_menu_entries(id)
                .contains(&ItemMenuEntry::Stack { marked: false })
        );
        w.ctl.toggle_stack_mark(id);
        assert!(
            w.ctl
                .item_menu_entries(id)
                .contains(&ItemMenuEntry::Stack { marked: true })
        );
    }

    #[test]
    fn paste_next_is_a_pro_feature() {
        let mut w = build(Opts {
            tier: Tier::Free,
            ..Opts::default()
        });
        w.ctl.on_hotkey(HotkeyAction::PasteNext);
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(
            (toast.kind, toast.message),
            (
                ToastKind::Danger,
                ToastMessage::NeedsPro(Feature::PasteStack)
            )
        );
        assert_eq!(pasted(&w), Vec::<String>::new());
    }

    #[test]
    fn paste_next_walks_the_shown_list_then_says_it_is_done_and_starts_again() {
        let mut w = world();
        for _ in 0..3 {
            assert_eq!(w.ctl.on_hotkey(HotkeyAction::PasteNext), []);
        }
        assert_eq!(pasted(&w), ["first", "second", "third"]);
        assert_eq!(uses(&w, w.ids[2]), 1);

        // The fourth is a template: the form opens, and answering it moves the run on.
        assert_eq!(w.ctl.paste_next(), Some(UseResult::NeedsAnswers));
        assert!(w.ctl.view().fill.is_some());
        assert_eq!(
            w.ctl.submit_fill(&["Sam".to_owned()]),
            UseResult::Done(UseOutcome::Pasted)
        );
        assert_eq!(pasted(&w).last().map(String::as_str), Some("Hi Sam"));

        assert_eq!(w.ctl.paste_next(), None);
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::PasteNextFinished
        );
        assert_eq!(
            w.ctl.paste_next(),
            Some(UseResult::Done(UseOutcome::Pasted))
        );
        assert_eq!(pasted(&w).last().map(String::as_str), Some("first"));
    }

    #[test]
    fn paste_next_prefers_the_pinned_strip_and_reads_the_target_when_pressed() {
        let mut w = build(Opts {
            pinned: vec![1, 4],
            ..Opts::default()
        });
        // The window in front now is not the one the controller last heard about.
        w.targets.set(None);
        w.ctl.on_hotkey(HotkeyAction::PasteNext);
        assert!(
            w.ctl.services().keyboard.sent.is_empty(),
            "no Target now, so it is copied, not pasted"
        );
        w.targets.set(Some(outlook()));
        w.ctl.on_hotkey(HotkeyAction::PasteNext);
        assert_eq!(pasted(&w), ["second", "https://example.com"]);
        assert_eq!(w.ctl.services().keyboard.sent.len(), 1);
        assert_eq!(w.ctl.paste_next(), None);
    }

    #[test]
    fn a_clipboard_that_is_busy_fails_the_paste_and_counts_nothing() {
        let mut w = build(Opts {
            clipboard_busy: true,
            ..Opts::default()
        });
        assert_eq!(w.ctl.paste_next(), Some(UseResult::Failed));
        assert_eq!(
            w.ctl.view().toast.expect("toast").message,
            ToastMessage::ClipboardBusy
        );
        assert_eq!(uses(&w, w.ids[0]), 0);
    }

    #[test]
    fn close_hides_with_a_tray_icon_and_drag_asks_the_host() {
        let mut w = world();
        with_tray(&mut w);
        assert_eq!(w.ctl.close(), [Effect::HidePanel]);
        assert!(!w.ctl.visible());
        assert_eq!(w.ctl.drag(), [Effect::DragWindow]);
        assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
    }

    fn hide_hint(w: &World) -> Option<ToastMessage> {
        w.ctl
            .view()
            .toast
            .map(|t| t.message)
            .filter(|m| matches!(m, ToastMessage::PanelHidden { .. }))
    }

    #[test]
    fn the_first_close_of_a_session_tells_how_to_get_the_panel_back() {
        let mut w = world();
        with_tray(&mut w);
        assert_eq!(w.ctl.close(), [Effect::HidePanel]);
        assert_eq!(
            hide_hint(&w),
            Some(ToastMessage::PanelHidden {
                chord: Some(Settings::default().hotkeys.toggle_panel)
            })
        );
        assert!(
            w.ctl.settings().hide_hint_shown,
            "remembered in the settings"
        );
    }

    #[test]
    fn a_second_hide_says_nothing() {
        let mut w = world();
        with_tray(&mut w);
        w.ctl.close();
        let _ = w.ctl.show();
        w.ctl.show_toast(Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::Copied,
        });
        w.ctl.close();
        assert_eq!(hide_hint(&w), None);
    }

    #[test]
    fn the_toggle_hotkey_and_tray_hide_say_nothing() {
        let mut w = world();
        with_tray(&mut w);
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::TogglePanel),
            [Effect::HidePanel]
        );
        assert_eq!(hide_hint(&w), None);
        let _ = w.ctl.show();
        assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::HidePanel]);
        assert_eq!(hide_hint(&w), None);
        assert!(!w.ctl.settings().hide_hint_shown);
    }

    #[test]
    fn without_a_toggle_chord_the_toast_names_only_the_tray() {
        let mut settings = Settings::default();
        settings.hotkeys.toggle_panel = String::new();
        let mut w = world_with(Some(outlook()), settings);
        with_tray(&mut w);
        w.ctl.close();
        assert_eq!(
            hide_hint(&w),
            Some(ToastMessage::PanelHidden { chord: None })
        );
    }

    #[test]
    fn closing_without_a_tray_quits_and_shows_no_toast() {
        let mut w = world();
        assert_eq!(w.ctl.close(), [Effect::Quit]);
        assert_eq!(hide_hint(&w), None);
    }

    #[test]
    fn a_toast_walks_enter_hold_exit_and_goes() {
        let mut w = world();
        w.ctl.show_toast(Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::Copied,
        });
        let seq = w.ctl.view().toast.expect("toast").seq;
        assert_eq!(w.ctl.view().toast.expect("toast").phase, ToastPhase::Enter);
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOLD));
        assert_eq!(w.ctl.view().toast.expect("toast").phase, ToastPhase::Hold);
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
        assert_eq!(w.ctl.view().toast.expect("toast").phase, ToastPhase::Exit);
        assert_eq!(w.ctl.toast_tick(seq), None);
        assert!(w.ctl.view().toast.is_none());
    }

    #[test]
    fn a_toast_under_the_pointer_stays_and_goes_a_moment_after_the_pointer_leaves() {
        let mut w = world();
        w.ctl.delete_row(1);
        let seq = w.ctl.view().toast.expect("toast").seq;
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_UNDO_HOLD));
        w.ctl.toast_hover(true);
        // The hold is over, but the pointer is on the toast: it looks again, and again.
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
        let toast = w.ctl.view().toast.expect("still there");
        assert_eq!((toast.phase, toast.undo), (ToastPhase::Hold, true));
        // The pointer leaves: a fresh hold, then the fade-out as usual.
        w.ctl.toast_hover(false);
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOLD));
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
        assert_eq!(w.ctl.toast_tick(seq), None);
        assert!(w.ctl.view().toast.is_none());
    }

    #[test]
    fn a_hover_that_ends_before_the_hold_does_does_not_change_the_timing() {
        let mut w = world();
        w.ctl.delete_row(1);
        let seq = w.ctl.view().toast.expect("toast").seq;
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_UNDO_HOLD));
        w.ctl.toast_hover(true);
        w.ctl.toast_hover(false);
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
    }

    #[test]
    fn a_pointer_leave_that_never_comes_cannot_keep_a_toast_for_good() {
        let mut w = world();
        w.ctl.delete_row(1);
        let seq = w.ctl.view().toast.expect("toast").seq;
        w.ctl.toast_tick(seq);
        w.ctl.toast_hover(true);
        for _ in 0..TOAST_HOVER_RECHECKS {
            assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOVER_RECHECK));
        }
        assert_eq!(
            w.ctl.toast_tick(seq),
            Some(TOAST_EXIT),
            "a minute is enough"
        );
    }

    #[test]
    fn a_newer_toast_does_not_inherit_the_hover() {
        let mut w = world();
        w.ctl.delete_row(1);
        let old = w.ctl.view().toast.expect("toast").seq;
        w.ctl.toast_tick(old);
        w.ctl.toast_hover(true);
        w.ctl.show_toast(Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::Copied,
        });
        let seq = w.ctl.view().toast.expect("toast").seq;
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_HOLD));
        assert_eq!(w.ctl.toast_tick(seq), Some(TOAST_EXIT));
    }

    #[test]
    fn a_stale_tick_does_not_touch_a_newer_toast() {
        let mut w = world();
        w.ctl.show_toast(Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::Copied,
        });
        let old = w.ctl.view().toast.expect("toast").seq;
        w.ctl.show_toast(Toast {
            kind: ToastKind::Danger,
            message: ToastMessage::Failed,
        });
        assert_eq!(w.ctl.toast_tick(old), None);
        let now = w.ctl.view().toast.expect("toast");
        assert_eq!(now.phase, ToastPhase::Enter);
        assert_ne!(now.seq, old);
    }

    #[test]
    fn hotkey_problems_show_a_toast_naming_the_chord() {
        let mut w = world();
        w.ctl.report_hotkey_problems(&[]);
        assert!(w.ctl.view().toast.is_none());
        w.ctl.report_hotkey_problems(&[crate::host::HotkeyProblem {
            action: HotkeyAction::Picker,
            chord: "Ctrl+Shift+Space".into(),
            failure: crate::host::HotkeyFailure::InUse,
        }]);
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(toast.kind, ToastKind::Danger);
        assert_eq!(
            toast.message,
            ToastMessage::HotkeyInUse {
                chord: "Ctrl+Shift+Space".into()
            }
        );
    }

    #[test]
    fn a_hotkey_problem_is_worded_by_why_it_failed() {
        use crate::host::{HotkeyFailure, HotkeyProblem};
        let mut w = world();
        let mut message = |failure| {
            w.ctl.report_hotkey_problems(&[HotkeyProblem {
                action: HotkeyAction::Picker,
                chord: "F13".into(),
                failure,
            }]);
            w.ctl.view().toast.expect("toast").message
        };
        let chord = String::from("F13");
        assert_eq!(
            message(HotkeyFailure::Invalid),
            ToastMessage::HotkeyInvalid {
                chord: chord.clone()
            }
        );
        assert_eq!(
            message(HotkeyFailure::Unavailable),
            ToastMessage::HotkeyUnavailable { chord }
        );
    }

    #[test]
    fn the_dark_theme_setting_reaches_the_view() {
        let settings = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        assert!(world_with(None, settings).ctl.view().dark);
    }

    // ---- The tray ----------------------------------------------------------------------------

    fn with_tray(w: &mut World) {
        w.ctl.on_tray(TrayEvent::IconPresent(true));
    }

    #[test]
    fn the_tray_toggles_the_panel_and_open_panel_always_shows_it() {
        let mut w = world();
        assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::HidePanel]);
        assert!(!w.ctl.visible());
        w.targets.set(None);
        assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::ShowPanel]);
        assert_eq!(
            w.ctl.view().footer,
            Footer::Copies,
            "the Target is read when the tray shows the Panel"
        );
        assert_eq!(w.ctl.on_tray(TrayEvent::OpenPanel), [Effect::ShowPanel]);
        assert!(w.ctl.visible());
    }

    #[test]
    fn the_tray_menu_opens_snipdeck_and_quits() {
        let mut w = world();
        assert_eq!(
            w.ctl.on_tray(TrayEvent::OpenSnipdeck),
            [Effect::OpenMainWindow]
        );
        assert_eq!(w.ctl.on_tray(TrayEvent::Quit), [Effect::Quit]);
        assert!(w.ctl.visible(), "quitting does not pretend to hide first");
    }

    #[test]
    fn pause_hotkeys_asks_the_host_and_silences_hotkeys_that_were_already_queued() {
        let mut w = world();
        assert_eq!(
            w.ctl.on_tray(TrayEvent::HotkeysPaused(true)),
            [Effect::SetHotkeysPaused(true)]
        );
        assert!(w.ctl.hotkeys_paused());
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::TogglePanel), Vec::new());
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), Vec::new());
        assert!(w.ctl.visible());
        assert_eq!(
            w.ctl.on_tray(TrayEvent::HotkeysPaused(false)),
            [Effect::SetHotkeysPaused(false)]
        );
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::TogglePanel),
            [Effect::HidePanel]
        );
    }

    #[test]
    fn a_second_launch_opens_the_main_window_when_the_panel_is_already_up() {
        let mut w = world();
        assert_eq!(w.ctl.second_launch(), [Effect::OpenMainWindow]);
        w.ctl.hide();
        assert_eq!(w.ctl.second_launch(), [Effect::ShowPanel]);
        assert!(w.ctl.visible());
    }

    #[test]
    fn closing_the_panel_hides_it_when_a_tray_icon_can_bring_it_back() {
        let mut w = world();
        with_tray(&mut w);
        assert_eq!(w.ctl.close(), [Effect::HidePanel]);
        assert!(!w.ctl.visible());
        assert_eq!(w.ctl.on_tray(TrayEvent::TogglePanel), [Effect::ShowPanel]);
    }

    #[test]
    fn closing_the_panel_quits_without_a_tray_icon_or_with_close_to_tray_off() {
        let mut w = world();
        assert_eq!(w.ctl.close(), [Effect::Quit], "no tray icon to return from");
        assert!(w.ctl.visible());

        let mut w = world_with(
            None,
            Settings {
                close_to_tray: false,
                ..Settings::default()
            },
        );
        with_tray(&mut w);
        assert_eq!(w.ctl.close(), [Effect::Quit]);
    }

    /// An unpinned Panel that has been up for a second, with the default settings.
    fn unpinned_world(settings: Settings) -> World {
        let mut w = world_with(Some(outlook()), settings);
        w.ctl.set_on_top(false);
        w.clock.set(at("2026-10-17T10:30:01Z"));
        w
    }

    #[test]
    fn an_unpinned_panel_hides_when_another_app_takes_the_foreground() {
        let mut w = unpinned_world(Settings::default());
        w.targets.set(Some(outlook()));
        assert_eq!(w.ctl.on_foreground_changed(), [Effect::HidePanel]);
        assert!(!w.ctl.visible());
        // Hidden already: a later change has nothing to hide.
        assert_eq!(w.ctl.on_foreground_changed(), []);
    }

    #[test]
    fn a_pinned_panel_is_topmost_unless_a_snipdeck_window_is_active() {
        let mut w = unpinned_world(Settings::default());
        assert!(!w.ctl.view().topmost, "unpinned: never topmost");
        w.ctl.set_on_top(true);
        assert!(w.ctl.view().topmost);
        w.ctl.set_own_window_active(true);
        assert!(!w.ctl.view().topmost, "Main can cover the Panel");
        assert!(w.ctl.view().on_top, "the pin button stays pressed");
        w.ctl.set_own_window_active(false);
        assert!(
            w.ctl.view().topmost,
            "back on top once focus is in another app"
        );
        w.ctl.set_on_top(false);
        w.ctl.set_own_window_active(true);
        w.ctl.set_own_window_active(false);
        assert!(!w.ctl.view().topmost);
    }

    fn reveal_first_row(w: &mut World) {
        let id = w.ids[0];
        make_sensitive(w, id);
        w.ctl.refresh_panel().expect("refresh");
        w.ctl.reveal_row(0, true);
        assert_eq!(w.ctl.view().revealed, Some(0), "held: the row is revealed");
    }

    #[test]
    fn a_held_reveal_ends_when_the_foreground_changes() {
        // Alt+Tab away from a pinned Panel: the Space release goes to the other window.
        let mut w = unpinned_world(Settings::default());
        w.ctl.set_on_top(true);
        reveal_first_row(&mut w);
        assert_eq!(w.ctl.on_foreground_changed(), []);
        assert_eq!(w.ctl.view().revealed, None);
        assert!(w.ctl.visible(), "pinned: the Panel stays, hidden text");
    }

    #[test]
    fn a_held_reveal_ends_when_a_snipdeck_window_takes_the_focus() {
        let mut w = unpinned_world(Settings::default());
        w.ctl.set_on_top(true);
        reveal_first_row(&mut w);
        w.ctl.set_own_window_active(true);
        assert_eq!(w.ctl.view().revealed, None);
    }

    #[test]
    fn a_held_reveal_ends_when_the_panel_hides() {
        let mut w = unpinned_world(Settings::default());
        reveal_first_row(&mut w);
        w.ctl.hide();
        assert_eq!(w.ctl.view().revealed, None);
    }

    #[test]
    fn a_pinned_panel_stays() {
        let mut w = unpinned_world(Settings::default());
        w.ctl.set_on_top(true);
        assert_eq!(w.ctl.on_foreground_changed(), []);
        assert!(w.ctl.visible());
    }

    #[test]
    fn with_the_setting_off_the_panel_stays() {
        let mut w = unpinned_world(Settings {
            panel_hide_on_blur: false,
            ..Settings::default()
        });
        assert_eq!(w.ctl.on_foreground_changed(), []);
        assert!(w.ctl.visible());
    }

    #[test]
    fn the_tray_or_desktop_taking_the_foreground_does_not_hide_the_panel() {
        let mut w = unpinned_world(Settings::default());
        // The tracker reports no Target for the tray, the taskbar or the desktop.
        w.targets.set(None);
        assert_eq!(w.ctl.on_foreground_changed(), []);
        assert!(w.ctl.visible());
    }

    #[test]
    fn a_foreground_change_right_after_show_is_ignored() {
        let mut w = unpinned_world(Settings::default());
        w.ctl.hide();
        w.clock.set(at("2026-10-17T10:31:00Z"));
        assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
        w.clock.set(at("2026-10-17T10:31:00.400Z"));
        assert_eq!(
            w.ctl.on_foreground_changed(),
            [],
            "400 ms is still the grace"
        );
        w.clock.set(at("2026-10-17T10:31:00.401Z"));
        assert_eq!(w.ctl.on_foreground_changed(), [Effect::HidePanel]);
    }

    #[test]
    fn the_picker_open_blocks_the_hide() {
        let mut w = unpinned_world(Settings::default());
        w.ctl.open_picker();
        assert_eq!(w.ctl.on_foreground_changed(), []);
        assert!(w.ctl.visible());
    }

    #[test]
    fn a_foreground_change_still_updates_the_footer() {
        let mut w = unpinned_world(Settings {
            panel_hide_on_blur: false,
            ..Settings::default()
        });
        w.targets.set(None);
        let _ = w.ctl.on_foreground_changed();
        assert_eq!(w.ctl.view().footer, Footer::Copies);
    }

    #[test]
    fn closing_the_main_window_quits_when_keep_running_is_off() {
        let mut w = world_with(
            Some(outlook()),
            Settings {
                close_to_tray: false,
                ..Settings::default()
            },
        );
        with_tray(&mut w);
        assert!(w.ctl.main_window_closed());
    }

    #[test]
    fn closing_the_main_window_does_not_quit_with_a_tray_icon() {
        let mut w = world();
        with_tray(&mut w);
        w.ctl.hide();
        assert!(!w.ctl.main_window_closed());
    }

    #[test]
    fn closing_the_main_window_quits_without_a_tray_when_the_panel_is_hidden() {
        let mut w = world();
        assert!(!w.ctl.main_window_closed(), "the Panel is still up");
        w.ctl.hide();
        assert!(
            w.ctl.main_window_closed(),
            "nothing could bring a window back"
        );
    }

    #[test]
    fn real_tray_clicks_on_the_fake_reach_the_controller() {
        use snipdeck_platform::{testing::FakeTray, tray::Tray, tray::TrayEntry};
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let mut tray = FakeTray::new("Snipdeck", move |e| {
            let _ = tx.lock().expect("lock").send(e);
        });
        let mut w = world();

        // Drain what the user did to the icon into the controller, as the app's UI thread does.
        let pump = |w: &mut World| {
            rx.try_iter()
                .flat_map(|event| w.ctl.on_tray(event))
                .collect::<Vec<_>>()
        };
        tray.show().expect("show");
        assert_eq!(pump(&mut w), [], "the icon appearing needs no effect");
        assert_eq!(w.ctl.close(), [Effect::HidePanel], "but now closing hides");
        assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
        tray.left_click();
        assert_eq!(pump(&mut w), [Effect::HidePanel]);
        tray.left_click();
        assert_eq!(pump(&mut w), [Effect::ShowPanel]);
        assert!(tray.right_click().is_some());
        tray.pick(TrayEntry::PauseHotkeys);
        assert_eq!(pump(&mut w), [Effect::SetHotkeysPaused(true)]);
        assert!(tray.hotkeys_paused(), "the menu check follows");
        tray.pick(TrayEntry::OpenSnipdeck);
        tray.pick(TrayEntry::Quit);
        assert_eq!(pump(&mut w), [Effect::OpenMainWindow, Effect::Quit]);
    }

    /// A controller wired to a `FakeTray` the way the app wires the real one: every event the
    /// tray emits is queued and then handed to `on_tray`.
    struct Wired {
        w: World,
        tray: snipdeck_platform::testing::FakeTray,
        rx: std::sync::mpsc::Receiver<TrayEvent>,
    }

    impl Wired {
        fn new() -> Self {
            let (tx, rx) = std::sync::mpsc::channel();
            let tx = std::sync::Mutex::new(tx);
            let tray = snipdeck_platform::testing::FakeTray::new("Snipdeck", move |e| {
                let _ = tx.lock().expect("lock").send(e);
            });
            Self {
                w: world(),
                tray,
                rx,
            }
        }

        fn pump(&mut self) {
            for event in self.rx.try_iter() {
                self.w.ctl.on_tray(event);
            }
        }

        fn close(&mut self) -> Vec<Effect> {
            self.pump();
            self.w.ctl.close()
        }
    }

    #[test]
    fn a_failed_first_show_then_explorer_starting_lets_close_hide() {
        use snipdeck_platform::tray::Tray;
        let mut t = Wired::new();
        t.tray.set_fail_show(true);
        let mut tray = t.tray.clone();
        assert!(tray.show().is_err());
        assert_eq!(
            t.close(),
            [Effect::Quit],
            "no icon yet: nothing to return from"
        );

        t.tray.set_fail_show(false);
        t.tray.taskbar_created();
        assert!(t.tray.visible());
        assert_eq!(t.close(), [Effect::HidePanel], "the icon appeared later");
    }

    #[test]
    fn explorer_restarting_without_taking_the_icon_back_makes_close_quit() {
        use snipdeck_platform::tray::Tray;
        let mut t = Wired::new();
        let mut tray = t.tray.clone();
        tray.show().expect("show");
        assert_eq!(t.close(), [Effect::HidePanel]);

        // Explorer restarts and the icon cannot be added again: no icon, so Esc must not hide
        // the Panel into nothing.
        t.tray.set_fail_show(true);
        t.tray.taskbar_created();
        assert!(!t.tray.visible());
        assert_eq!(t.close(), [Effect::Quit]);

        t.tray.set_fail_show(false);
        t.tray.taskbar_created();
        assert_eq!(t.close(), [Effect::HidePanel], "back again");
    }

    #[test]
    fn hiding_the_icon_makes_close_quit() {
        use snipdeck_platform::tray::Tray;
        let mut t = Wired::new();
        let mut tray = t.tray.clone();
        tray.show().expect("show");
        tray.hide().expect("hide");
        assert_eq!(t.close(), [Effect::Quit]);
    }

    // ---- The OS theme ------------------------------------------------------------------------

    fn os(dark: bool) -> SystemLook {
        SystemLook {
            dark,
            high_contrast: None,
            accent: None,
            reduce_motion: false,
        }
    }

    #[test]
    fn the_system_theme_follows_windows_and_light_and_dark_ignore_it() {
        let mut w = world();
        assert!(!w.ctl.view().dark, "light until told otherwise");
        w.ctl.set_system_look(os(true));
        assert!(w.ctl.view().dark, "theme = system follows Windows");
        w.ctl.set_system_look(os(false));
        assert!(!w.ctl.view().dark);

        w.ctl.set_theme(Theme::Light);
        w.ctl.set_system_look(os(true));
        assert!(!w.ctl.view().dark, "light stays light");
        w.ctl.set_theme(Theme::Dark);
        w.ctl.set_system_look(os(false));
        assert!(w.ctl.view().dark, "dark stays dark");
    }

    #[test]
    fn motion_is_reduced_by_the_setting_or_by_windows() {
        let mut w = world();
        assert!(!w.ctl.view().reduced_motion);
        w.ctl.update_settings(|s| s.reduce_motion = true);
        assert!(w.ctl.view().reduced_motion, "the setting alone");
        w.ctl.update_settings(|s| s.reduce_motion = false);
        w.ctl.set_system_look(SystemLook {
            reduce_motion: true,
            ..os(false)
        });
        assert!(w.ctl.view().reduced_motion, "Windows alone");
        assert!(w.ctl.settings_look().reduce_motion);
        w.ctl.set_system_look(os(false));
        assert!(!w.ctl.view().reduced_motion, "and it ends");
    }

    #[test]
    fn high_contrast_wins_over_every_theme_setting() {
        let hc = HighContrast::default();
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            let mut w = world_with(
                None,
                Settings {
                    theme,
                    ..Settings::default()
                },
            );
            assert_eq!(w.ctl.view().high_contrast, None);
            w.ctl.set_system_look(SystemLook {
                dark: false,
                high_contrast: Some(hc),
                accent: None,
                reduce_motion: false,
            });
            assert_eq!(w.ctl.view().high_contrast, Some(hc), "{theme:?}");
            w.ctl.set_system_look(os(false));
            assert_eq!(w.ctl.view().high_contrast, None, "{theme:?}: and it ends");
        }
    }

    #[test]
    fn a_fake_source_drives_the_look_through_a_change() {
        use snipdeck_platform::{system_look::SystemLookSource, testing::FakeSystemLook};
        let source = FakeSystemLook::default();
        let mut w = world();
        w.ctl.set_system_look(source.read());
        assert!(!w.ctl.view().dark);
        source.set(os(true));
        w.ctl.set_system_look(source.read());
        assert!(w.ctl.view().dark);
    }

    // ---- Settings are saved ------------------------------------------------------------------

    fn saving_world(dir: &std::path::Path) -> World {
        let mut w = world();
        let saver = SettingsSaver::new(
            Box::new(crate::settings_store::FileSink::new(dir)),
            Settings::default(),
        );
        w.ctl.set_settings_saver(saver);
        w
    }

    #[test]
    fn the_main_window_place_is_remembered_in_the_settings() {
        let mut w = world();
        let place = crate::settings::WindowPlace {
            x: 40,
            y: 60,
            w: 1000,
            h: 640,
            maximized: false,
        };
        w.ctl.remember_main_window(place);
        assert_eq!(w.ctl.settings().main_window, Some(place));
    }

    #[test]
    fn a_setting_is_written_after_the_quiet_time_by_the_injected_clock() {
        let temp = tempfile::tempdir().expect("dir");
        let file = temp.path().join(crate::paths::SETTINGS_FILE);
        let mut w = saving_world(temp.path());
        assert_eq!(w.ctl.settings_wait(), None, "nothing changed yet");

        w.ctl.set_theme(Theme::Dark);
        assert!(w.ctl.view().dark, "applied at once");
        assert_eq!(w.ctl.settings_wait(), Some(Duration::from_millis(500)));
        w.ctl.save_settings_if_due();
        assert!(!file.exists(), "not before the quiet time");

        w.clock.set(at("2026-10-17T10:30:00.4Z"));
        w.ctl.save_settings_if_due();
        assert!(!file.exists());
        assert_eq!(w.ctl.settings_wait(), Some(Duration::from_millis(100)));

        w.clock.set(at("2026-10-17T10:30:00.5Z"));
        w.ctl.save_settings_if_due();
        assert_eq!(crate::paths::load_settings(temp.path()).theme, Theme::Dark);
        assert_eq!(w.ctl.settings_wait(), None);
    }

    #[test]
    fn several_quick_changes_make_one_write_and_flush_writes_at_once() {
        let temp = tempfile::tempdir().expect("dir");
        let mut w = saving_world(temp.path());
        w.ctl.update_settings(|s| s.panel_width = 300);
        w.clock.set(at("2026-10-17T10:30:00.2Z"));
        w.ctl.update_settings(|s| s.panel_width = 9999);
        assert_eq!(w.ctl.settings().panel_width, 480, "sanitised");
        w.ctl.update_settings(|s| s.panel_width = 480);
        // The last two were the same value: still waiting from the second change.
        assert_eq!(w.ctl.settings_wait(), Some(Duration::from_millis(500)));

        w.ctl.flush_settings().expect("flush");
        assert_eq!(crate::paths::load_settings(temp.path()).panel_width, 480);
        assert_eq!(w.ctl.settings_wait(), None);
    }

    #[test]
    fn an_unchanged_setting_schedules_nothing() {
        let temp = tempfile::tempdir().expect("dir");
        let mut w = saving_world(temp.path());
        w.ctl.update_settings(|s| s.close_to_tray = true);
        w.ctl.set_theme(Theme::System);
        assert_eq!(w.ctl.settings_wait(), None);
        w.ctl.flush_settings().expect("flush");
        assert!(!temp.path().join(crate::paths::SETTINGS_FILE).exists());
    }

    #[test]
    fn without_a_saver_changes_stay_in_memory() {
        let mut w = world();
        w.ctl.set_theme(Theme::Dark);
        assert_eq!(w.ctl.settings_wait(), None);
        assert!(w.ctl.flush_settings().is_ok());
        assert_eq!(w.ctl.settings().theme, Theme::Dark);
    }

    #[test]
    fn a_failed_write_toasts_once_and_succeeds_when_the_disk_does() {
        let temp = tempfile::tempdir().expect("dir");
        // The "folder" is a file, so every write fails.
        let blocked = temp.path().join("blocked");
        std::fs::write(&blocked, "x").expect("file");
        let mut w = saving_world(&blocked);
        w.ctl.set_theme(Theme::Dark);

        w.clock.set(at("2026-10-17T10:30:01Z"));
        w.ctl.save_settings_if_due();
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(toast.kind, ToastKind::Danger);
        assert_eq!(toast.message, ToastMessage::SettingsNotSaved);
        let first = toast.seq;
        assert_eq!(
            w.ctl.settings_wait(),
            Some(Duration::from_secs(5)),
            "kept for a retry"
        );

        w.clock.set(at("2026-10-17T10:30:07Z"));
        w.ctl.save_settings_if_due();
        assert_eq!(
            w.ctl.view().toast.expect("toast").seq,
            first,
            "no second toast for the same trouble"
        );
        assert!(w.ctl.flush_settings().is_err());
    }

    // ---- The Panel menu, row actions and window frame ----------------------------------------

    fn app_menu_index(entry: AppMenuEntry) -> usize {
        APP_MENU.iter().position(|e| *e == entry).expect("in menu")
    }

    fn row_titles(w: &World) -> Vec<String> {
        w.ctl.view().rows.iter().map(|r| r.title.clone()).collect()
    }

    fn display() -> crate::frame::Monitor {
        crate::frame::Monitor {
            id: crate::frame::Monitor::id_of("DISPLAY1", 1920, 1080),
            work_area: Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1040,
            },
            scale: 1.0,
        }
    }

    #[test]
    fn the_app_menu_opens_and_closes_without_a_choice() {
        let mut w = world();
        assert_eq!(w.ctl.view().app_menu, None);
        w.ctl.open_app_menu();
        assert_eq!(w.ctl.view().app_menu, Some(APP_MENU.to_vec()));
        w.ctl.close_app_menu();
        assert_eq!(w.ctl.view().app_menu, None);
    }

    #[test]
    fn opening_the_app_menu_closes_the_board_menu_and_hiding_closes_it() {
        let mut w = world();
        w.ctl.open_board_menu();
        w.ctl.open_app_menu();
        assert!(w.ctl.view().board_menu.is_none());
        assert!(w.ctl.view().app_menu.is_some());
        w.ctl.hide();
        assert!(w.ctl.view().app_menu.is_none());
    }

    #[test]
    fn each_app_menu_entry_asks_the_host_for_its_thing_and_closes_the_menu() {
        let cases = [
            (AppMenuEntry::Open, Effect::OpenMainWindow),
            (AppMenuEntry::Commands, Effect::OpenCommands),
            (AppMenuEntry::NewSnippet, Effect::NewItem),
            (AppMenuEntry::Settings, Effect::OpenSettings),
            (
                AppMenuEntry::Troubleshoot,
                Effect::OpenHelp(HelpTopic::Troubleshoot),
            ),
            (AppMenuEntry::UserGuide, Effect::OpenHelp(HelpTopic::Guide)),
            (
                AppMenuEntry::Shortcuts,
                Effect::OpenHelp(HelpTopic::Shortcuts),
            ),
            (AppMenuEntry::About, Effect::OpenHelp(HelpTopic::About)),
            (AppMenuEntry::Quit, Effect::Quit),
        ];
        for (entry, effect) in cases {
            let mut w = world();
            w.ctl.open_app_menu();
            assert_eq!(
                w.ctl.choose_app_menu(app_menu_index(entry)),
                [effect],
                "{entry:?}"
            );
            assert!(w.ctl.view().app_menu.is_none(), "{entry:?} closes the menu");
        }
    }

    #[test]
    fn pause_hotkeys_toggles_and_the_view_says_which_way_it_is() {
        let mut w = world();
        let pause = app_menu_index(AppMenuEntry::PauseHotkeys);
        assert!(!w.ctl.view().hotkeys_paused);
        assert_eq!(
            w.ctl.choose_app_menu(pause),
            [Effect::SetHotkeysPaused(true)]
        );
        assert!(w.ctl.view().hotkeys_paused);
        assert!(w.ctl.hotkeys_paused());
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::TogglePanel), Vec::new());
        assert_eq!(
            w.ctl.choose_app_menu(pause),
            [Effect::SetHotkeysPaused(false)]
        );
        assert!(!w.ctl.view().hotkeys_paused);
    }

    #[test]
    fn a_line_in_the_app_menu_does_nothing_and_a_missing_row_closes_it() {
        let mut w = world();
        w.ctl.open_app_menu();
        let line = app_menu_index(AppMenuEntry::Separator);
        assert_eq!(w.ctl.choose_app_menu(line), Vec::new());
        assert!(w.ctl.view().app_menu.is_some(), "a line is not a choice");
        assert_eq!(w.ctl.choose_app_menu(99), Vec::new());
        assert!(w.ctl.view().app_menu.is_none());
    }

    #[test]
    fn edit_and_rename_ask_for_the_quick_editor_on_that_row() {
        let mut w = world();
        let id = w.ctl.model().rows()[1].id;
        assert_eq!(w.ctl.edit_row(1), [Effect::EditItem(id)]);
        assert_eq!(w.ctl.rename_row(1), [Effect::EditItem(id)]);
        assert_eq!(w.ctl.edit_row(99), Vec::new());
        assert_eq!(w.ctl.rename_row(99), Vec::new());
    }

    #[test]
    fn deleting_a_row_trashes_it_with_an_undo_toast_that_brings_it_back() {
        let mut w = world();
        assert_eq!(row_titles(&w), ["One", "Two", "Three", "Greeting"]);
        w.ctl.delete_row(1);
        assert_eq!(row_titles(&w), ["One", "Three", "Greeting"]);
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(toast.message, ToastMessage::ItemsTrashed { count: 1 });
        assert!(toast.undo, "5 s to undo");
        assert_eq!(w.ctl.store().trashed().expect("trash").len(), 1);

        w.ctl.main_undo();
        assert_eq!(row_titles(&w), ["One", "Two", "Three", "Greeting"]);
        assert_eq!(w.ctl.store().trashed().expect("trash"), Vec::new());
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(toast.message, ToastMessage::Restored { count: 1 });
    }

    #[test]
    fn the_item_menu_opens_on_a_row_and_closes_without_a_choice() {
        let mut w = world();
        assert_eq!(w.ctl.view().item_menu, None);
        w.ctl.open_item_menu(1, false);
        let entries = w.ctl.view().item_menu.expect("menu");
        assert!(entries.contains(&ItemMenuEntry::Pin { pinned: false }));
        w.ctl.close_item_menu();
        assert_eq!(w.ctl.view().item_menu, None);
        w.ctl.open_item_menu(99, false);
        assert_eq!(w.ctl.view().item_menu, None, "no such row");
    }

    #[test]
    fn the_item_menu_edits_pins_and_trashes_the_row_it_was_opened_on() {
        let mut w = world();
        let id = w.ctl.model().rows()[1].id;
        let index_of = |w: &World, entry: ItemMenuEntry| {
            w.ctl
                .view()
                .item_menu
                .expect("menu")
                .iter()
                .position(|e| *e == entry)
                .expect("entry")
        };
        w.ctl.open_item_menu(1, false);
        let edit = index_of(&w, ItemMenuEntry::Edit);
        assert_eq!(w.ctl.choose_item_menu(edit), [Effect::EditItem(id)]);
        assert_eq!(w.ctl.view().item_menu, None);

        w.ctl.open_item_menu(1, false);
        let pin = index_of(&w, ItemMenuEntry::Pin { pinned: false });
        assert_eq!(w.ctl.choose_item_menu(pin), Vec::new());
        assert!(w.ctl.store().item(id).expect("item").pinned);
        w.ctl.open_item_menu(1, false);
        assert!(
            w.ctl
                .view()
                .item_menu
                .expect("menu")
                .contains(&ItemMenuEntry::Pin { pinned: true })
        );
        w.ctl.close_item_menu();

        w.ctl.open_item_menu(1, false);
        let trash = index_of(&w, ItemMenuEntry::Trash);
        assert_eq!(w.ctl.choose_item_menu(trash), Vec::new());
        assert_eq!(row_titles(&w), ["One", "Three", "Greeting"]);
        assert!(w.ctl.view().toast.expect("toast").undo);
    }

    #[test]
    fn a_line_in_the_item_menu_does_nothing_and_hiding_closes_it() {
        let mut w = world();
        w.ctl.open_item_menu(0, false);
        let line = w
            .ctl
            .view()
            .item_menu
            .expect("menu")
            .iter()
            .position(|e| *e == ItemMenuEntry::Separator)
            .expect("line");
        assert_eq!(w.ctl.choose_item_menu(line), Vec::new());
        assert!(w.ctl.view().item_menu.is_some(), "a line is not a choice");
        w.ctl.hide();
        assert_eq!(w.ctl.view().item_menu, None);
    }

    #[test]
    fn deleting_a_row_that_is_not_there_changes_nothing() {
        let mut w = world();
        w.ctl.delete_row(99);
        assert_eq!(row_titles(&w).len(), 4);
        assert!(w.ctl.view().toast.is_none());
    }

    #[test]
    fn a_resize_grip_asks_the_host_to_start_the_os_resize() {
        let mut w = world();
        assert_eq!(
            w.ctl.resize(Edge::BottomRight),
            [Effect::ResizeWindow(Edge::BottomRight)]
        );
    }

    #[test]
    fn a_frame_wider_than_480_is_cut_remembered_and_the_host_told() {
        let mut w = world();
        let m = display();
        let asked = Rect {
            x: 100,
            y: 100,
            w: 1000,
            h: 600,
        };
        let cut = Rect { w: 480, ..asked };
        assert_eq!(w.ctl.frame_changed(&m, asked), [Effect::PlaceWindow(cut)]);
        assert_eq!(w.ctl.placement(&m), Some(cut));
        let stored = &w.ctl.settings().panel_geometry;
        assert_eq!(stored.len(), 1);
        assert_eq!(
            (stored[0].monitor_id.as_str(), stored[0].w, stored[0].h),
            (m.id.as_str(), 480, 600)
        );
    }

    #[test]
    fn a_move_across_a_monitor_edge_is_remembered_but_not_pulled_back() {
        let mut w = world();
        let m = display();
        let straddling = Rect {
            x: 1800,
            y: 100,
            w: 400,
            h: 600,
        };
        assert_eq!(w.ctl.frame_moved(&m, straddling), Vec::new());
        let stored = &w.ctl.settings().panel_geometry;
        assert_eq!((stored[0].x, stored[0].y), (1800, 100));
        // Restoring fits it inside the work area; a resize still clamps and answers.
        let fitted = Rect {
            x: 1520,
            ..straddling
        };
        assert_eq!(w.ctl.placement(&m), Some(fitted));
        assert_eq!(
            w.ctl.frame_changed(&m, straddling),
            [Effect::PlaceWindow(fitted)]
        );
    }

    #[test]
    fn a_frame_inside_the_limits_is_remembered_as_it_is_with_no_effect() {
        let mut w = world();
        let m = display();
        let frame = Rect {
            x: 50,
            y: 60,
            w: 300,
            h: 500,
        };
        assert_eq!(w.ctl.frame_changed(&m, frame), Vec::new());
        assert_eq!(w.ctl.placement(&m), Some(frame));
        let frame = Rect { w: 400, ..frame };
        assert_eq!(w.ctl.frame_changed(&m, frame), Vec::new());
        assert_eq!(w.ctl.settings().panel_geometry.len(), 1, "one per monitor");
        assert_eq!(w.ctl.placement(&m), Some(frame));
    }

    #[test]
    fn a_frame_with_no_size_is_not_a_place_the_user_chose() {
        let mut w = world();
        let nothing = Rect {
            x: -32000,
            y: -32000,
            w: 0,
            h: 0,
        };
        assert_eq!(w.ctl.frame_changed(&display(), nothing), Vec::new());
        assert_eq!(w.ctl.settings().panel_geometry, Vec::new());
    }

    #[test]
    fn each_monitor_keeps_its_own_place_and_restoring_uses_the_one_it_is_on() {
        let mut w = world();
        let first = display();
        let second = crate::frame::Monitor {
            id: crate::frame::Monitor::id_of("DISPLAY2", 1280, 1024),
            work_area: Rect {
                x: 1920,
                y: 0,
                w: 1280,
                h: 984,
            },
            scale: 1.0,
        };
        let on_first = Rect {
            x: 10,
            y: 20,
            w: 300,
            h: 500,
        };
        let on_second = Rect {
            x: 2000,
            y: 30,
            w: 450,
            h: 700,
        };
        w.ctl.frame_changed(&first, on_first);
        w.ctl.frame_changed(&second, on_second);
        assert_eq!(w.ctl.restore_frame(&first), [Effect::PlaceWindow(on_first)]);
        assert_eq!(
            w.ctl.restore_frame(&second),
            [Effect::PlaceWindow(on_second)]
        );
        let third = crate::frame::Monitor {
            id: "DISPLAY3 800x600".into(),
            ..first
        };
        assert_eq!(w.ctl.restore_frame(&third), Vec::new(), "nothing stored");
    }

    #[test]
    fn a_stored_place_is_fitted_to_a_monitor_that_has_become_smaller() {
        let mut w = world();
        w.ctl.update_settings(|s| {
            s.remember_panel_geometry(crate::settings::PanelGeometry {
                monitor_id: display().id,
                x: 1700,
                y: 900,
                w: 480,
                h: 900,
                edge: None,
            });
        });
        assert_eq!(
            w.ctl.placement(&display()),
            Some(Rect {
                x: 1440,
                y: 140,
                w: 480,
                h: 900
            }),
            "pulled back inside the work area"
        );
    }

    #[test]
    fn a_remembered_frame_is_written_after_the_quiet_time() {
        let temp = tempfile::tempdir().expect("dir");
        let mut w = saving_world(temp.path());
        let frame = Rect {
            x: 50,
            y: 60,
            w: 300,
            h: 500,
        };
        w.ctl.frame_changed(&display(), frame);
        w.clock.set(at("2026-10-17T10:30:01Z"));
        w.ctl.save_settings_if_due();
        let loaded = crate::paths::load_settings(temp.path());
        assert_eq!(loaded.panel_geometry.len(), 1);
        assert_eq!(Rect::of(&loaded.panel_geometry[0]), frame);
    }
}
