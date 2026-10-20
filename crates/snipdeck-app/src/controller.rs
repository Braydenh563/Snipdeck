//! The controller: everything the Panel does that is not drawing. It owns the library, the
//! [`PanelModel`], the settings, the clock and the platform services, and turns gestures
//! (a row activated, a digit pressed, a hotkey, the Target changing) into model changes, uses
//! and toasts. It knows nothing about Slint: `binding` copies a [`View`] into the window and
//! wires the window's callbacks to the methods here, so nearly all behaviour is tested without
//! a window.

use std::time::Duration;

use snipdeck_core::{
    BoardId, Entitlements, ItemId, ListId, Timestamp,
    template::{Locale, TimeZone},
};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    system_look::SystemLook,
    tray::TrayEvent,
};
use snipdeck_store::Trashable;

mod commands_ctl;
mod dock_ctl;
mod editor_ctl;
mod library;
mod lock_ctl;
mod main_ctl;
mod onboarding_ctl;
mod panel;
mod panel_ctl;
mod recent_ctl;
mod settings_ctl;
mod toast_ctl;
mod use_ctl;
mod views;

pub use library::Library;
pub use lock_ctl::{MasterProblem, MasterState};
pub use main_ctl::{BoardMenuEntry, MainAction, MainScreen, TagChoice, TagRenameError};
pub use panel::{APP_MENU, AppMenuEntry, HelpTopic, ItemMenuEntry, LIST_MENU, ListMenuEntry};
pub use settings_ctl::{ImportKind, ImportPreview, ImportProblem, ImportSource};
pub use views::{
    Effect, FillSubmission, FillView, ListDeleteView, PickerView, StackView, ToastPhase, ToastView,
    UseResult, View,
};

use crate::{
    accent::Accent,
    dock::DockState,
    fill::FillModel,
    frame::Monitor,
    host::{ClipboardText, Clock, TargetSource},
    main_window::MainModel,
    panel::PanelModel,
    paste_next::PasteQueue,
    picker::PickerModel,
    settings::{DockEdge, HotkeyAction, Settings, Theme},
    settings_store::SettingsSaver,
    toast::{Toast, ToastKind, ToastMessage},
    usecase::{self},
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
    /// The board menu's delete confirmation is asking about this board (the Main window draws
    /// it; no editor opens).
    board_delete: Option<BoardId>,
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
    /// The list menu's delete confirmation, asking about this list.
    DeleteList(ListId),
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
            board_delete: None,
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
            list_delete: self.list_delete_view(),
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
