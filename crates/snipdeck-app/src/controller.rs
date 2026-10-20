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
    Trashable,
    crypto::{DataKey, KeyRecord, KeyRing},
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

    /// Whether any Recent inbox entry is stored as plain text (kept before a key existed, or by
    /// version 4). Setting up encryption encrypts it.
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn has_plain_recent(&self) -> snipdeck_store::Result<bool>;

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

    /// The stored key record, if encryption was set up. A master password step re-wraps a copy of
    /// it elsewhere ([`crate::master_job::MasterJob`]).
    ///
    /// # Errors
    ///
    /// The store failing to read.
    fn key_record(&self) -> snipdeck_store::Result<Option<KeyRecord>>;

    /// Finishes setting up encryption with a key and record made elsewhere, and leaves the library
    /// unlocked.
    ///
    /// # Errors
    ///
    /// A key ring exists already, or the store failed. Nothing changes on error.
    fn install_key_ring(&mut self, key: DataKey, record: &KeyRecord) -> snipdeck_store::Result<()>;

    /// Opens the library with a key opened elsewhere. Does nothing when it is open.
    ///
    /// # Errors
    ///
    /// The store failed.
    fn open_with_key(&mut self, key: DataKey) -> snipdeck_store::Result<()>;

    /// Stores a key record re-wrapped elsewhere (a changed or removed master password).
    ///
    /// # Errors
    ///
    /// There is no key ring, or the store failed. Nothing changes on error.
    fn replace_key_record(&mut self, record: &KeyRecord) -> snipdeck_store::Result<()>;

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

    fn has_plain_recent(&self) -> snipdeck_store::Result<bool> {
        Store::has_plain_recent(self)
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

    fn key_record(&self) -> snipdeck_store::Result<Option<KeyRecord>> {
        Store::key_record(self)
    }

    fn install_key_ring(&mut self, key: DataKey, record: &KeyRecord) -> snipdeck_store::Result<()> {
        Store::install_key_ring(self, key, record)
    }

    fn open_with_key(&mut self, key: DataKey) -> snipdeck_store::Result<()> {
        Store::open_with_key(self, key)
    }

    fn replace_key_record(&mut self, record: &KeyRecord) -> snipdeck_store::Result<()> {
        Store::replace_key_record(self, record)
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

/// A clear of a sensitive item's clipboard content that is waiting for its time.
#[derive(Debug, Clone)]
struct PendingClear {
    due: Timestamp,
    /// The copied text, when the item had one: cleared only if the clipboard still holds it.
    digest: Option<ClipboardDigest>,
    /// The clipboard sequence number right after our write. Covers every format (HTML, images,
    /// files): the clipboard is emptied only if nothing has changed it since.
    sequence: Option<u32>,
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
    clipboard_clear: Option<PendingClear>,
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
    /// Told whether clipboard history is on and whether it is paused, for the tray menu's
    /// "Pause history" entry (see `recent_ctl.rs`).
    tray_history: Option<recent_ctl::HistorySink>,
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
            tray_history: None,
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
            TrayEvent::HistoryPaused(paused) => {
                self.set_recent_paused(paused);
                Vec::new()
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
        let rows = self.model.rows();
        let (Some(moved), Some(target)) =
            (rows.get(from).map(|r| r.id), rows.get(onto).map(|r| r.id))
        else {
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
        self.panel_place(moved, delta, false);
    }

    /// Alt+Up / Alt+Down in the Panel: moves the selected row one place.
    pub fn panel_move_selected(&mut self, delta: isize) {
        if let Some(id) = self.model.selected() {
            self.panel_move_item(id, delta);
        }
    }

    /// Moves item `id` by `delta` places in the shown list and says where it went.
    pub(crate) fn panel_move_item(&mut self, id: ItemId, delta: isize) {
        self.panel_place(id, delta, true);
    }

    /// Whether item `id` can move by `delta` places in the shown list: it is in manual order,
    /// no search runs and the place exists.
    pub(crate) fn can_move_item(&self, id: ItemId, delta: isize) -> bool {
        if !self.model.reorderable() {
            return false;
        }
        let order = self.model.manual_order();
        order
            .iter()
            .position(|i| *i == id)
            .is_some_and(|at| reorder(&order, at, delta).is_some())
    }

    /// Moves `moved` by `delta` places, keeps it selected and, when `announce`, raises a toast
    /// with its new place.
    fn panel_place(&mut self, moved: ItemId, delta: isize, announce: bool) {
        if !self.model.reorderable() {
            return;
        }
        let Some(list) = self.model.list().map(|l| l.id) else {
            return;
        };
        let order = self.model.manual_order();
        let Some(a) = order.iter().position(|i| *i == moved) else {
            return;
        };
        let Some((to, placement)) = reorder(&order, a, delta) else {
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
                if announce {
                    self.announce_placed(to + 1, order.len());
                }
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

    /// After a copy of a sensitive item, remembers its content on the clipboard to be cleared
    /// `clipboard_clear_secs` from now (0 = never). A paste that restored the user's own
    /// clipboard leaves nothing to clear; one whose snapshot or restore failed
    /// ([`UseOutcome::PastedNotRestored`]) leaves the item there, so it is cleared like a copy.
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
        let digest = self
            .services
            .clipboard
            .read_text()
            .map(|text| ClipboardDigest::of(&text));
        let sequence = self.services.clipboard.sequence_number();
        self.clipboard_clear = (digest.is_some() || sequence.is_some()).then_some(PendingClear {
            due,
            digest,
            sequence,
        });
    }

    /// How long until the clipboard is to be cleared of a copied sensitive item, if one waits.
    /// The host arms a timer for this and then calls
    /// [`clear_clipboard_if_due`](Self::clear_clipboard_if_due).
    #[must_use]
    pub fn clipboard_clear_wait(&self) -> Option<Duration> {
        let pending = self.clipboard_clear.as_ref()?;
        Some(Duration::try_from(pending.due.duration_since(self.clock.now())).unwrap_or_default())
    }

    /// Clears the clipboard if the wait is over and it still holds the copied text.
    pub fn clear_clipboard_if_due(&mut self) {
        let Some(pending) = &self.clipboard_clear else {
            return;
        };
        if self.clock.now() < pending.due {
            return;
        }
        let Some(pending) = self.clipboard_clear.take() else {
            return;
        };
        if let Some(digest) = &pending.digest
            && let Some(text) = self.services.clipboard.read_text()
            && digest.matches(&text)
        {
            self.services.clipboard.clear_if_text(&text);
            return;
        }
        // Not text, or the text could not be read: empty it entirely, but only if nothing has
        // touched the clipboard since our write.
        if let Some(sequence) = pending.sequence {
            self.services.clipboard.clear_if_sequence(sequence);
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

    /// Says where a keyboard or menu move put an item. A live Undo toast is not replaced (it
    /// would lose its button), and a repeat while the same words show only updates them, so a
    /// held key neither restarts the toast nor repeats the announcement.
    pub(crate) fn announce_placed(&mut self, position: usize, total: usize) {
        let message = ToastMessage::ItemPlaced { position, total };
        if self.undo.is_some() {
            return;
        }
        match &mut self.toast {
            Some(active) if matches!(active.toast.message, ToastMessage::ItemPlaced { .. }) => {
                active.toast.message = message;
            }
            _ => self.show_toast(neutral(message)),
        }
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
