//! The controller: everything the Panel does that is not drawing. It owns the library, the
//! [`PanelModel`], the settings, the clock and the platform services, and turns gestures
//! (a row activated, a digit pressed, a hotkey, the Target changing) into model changes, uses
//! and toasts. It knows nothing about Slint: `binding` copies a [`View`] into the window and
//! wires the window's callbacks to the methods here, so nearly all behaviour is tested without
//! a window.

use std::{collections::BTreeMap, time::Duration};

use snipdeck_core::{
    BoardId, Entitlements, ItemId, UseMode, UseOutcome,
    template::{Locale, Prompt, TimeZone},
};
use snipdeck_platform::{
    pipeline::{Clipboard, Env, Keyboard, Timing},
    route::Route,
};
use snipdeck_store::{BlobStore, BoardRepo, ItemRepo, ListRepo, TagRepo};

use crate::{
    host::{Clock, TargetSource},
    panel::{Footer, PanelModel, Row, Tab, UseLabel},
    settings::{HotkeyAction, Settings, Theme},
    toast::{self, Toast, ToastKind, ToastMessage},
    usecase::{self, UseError, UseRequest},
};

/// How long the Target gets to take the focus back before keys are injected (the M2 spike's
/// 50 ms; Windows restores the focused control asynchronously).
pub const REFOCUS_WAIT: Duration = Duration::from_millis(50);
/// A toast fades in for this long (`Motion.quick`) before it counts as settled.
pub const TOAST_ENTER: Duration = Duration::from_millis(120);
/// A settled toast stays this long (`Toast.hold-duration`, DESIGN.md §4).
pub const TOAST_HOLD: Duration = Duration::from_millis(2500);
/// A toast fades out for this long (`Motion.fade`) before it is gone.
pub const TOAST_EXIT: Duration = Duration::from_millis(90);

/// What the controller needs from the library: every repository trait in one bound.
pub trait Library: BoardRepo + ListRepo + ItemRepo + TagRepo + BlobStore {}
impl<T: BoardRepo + ListRepo + ItemRepo + TagRepo + BlobStore> Library for T {}

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

/// Something the host (the window and the process) must do. The controller decides, the host
/// acts: nothing here touches a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Show the Panel without taking focus.
    ShowPanel,
    /// Hide the Panel.
    HidePanel,
    /// Open the Picker at the caret (M4).
    OpenPicker,
    /// Start a screen capture (M5).
    StartCapture,
    /// Paste the head of the paste stack (Pro; the stack itself is not built yet).
    PasteNext,
    /// Start the OS window drag from the Panel header.
    DragWindow,
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
}

/// A template waiting for the user's answers (the fill form comes with M4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPrompt {
    /// The template item.
    pub item: ItemId,
    /// How the user asked to use it.
    pub mode: UseMode,
    /// What to ask.
    pub prompts: Vec<Prompt>,
}

/// How a use ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseResult {
    /// It happened (pasted or copied) and was counted.
    Done(UseOutcome),
    /// The template needs answers first: see [`Controller::pending`].
    NeedsAnswers,
    /// It did not happen; a toast says why.
    Failed,
}

/// Everything the Panel window shows, as plain data.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The board's name.
    pub board_name: String,
    /// The list tabs.
    pub tabs: Vec<Tab>,
    /// The active tab (0 when the board has no lists).
    pub current_tab: usize,
    /// The pinned strip.
    pub pinned: Vec<Row>,
    /// The item rows.
    pub rows: Vec<Row>,
    /// The search text.
    pub query: String,
    /// The keyboard-selected row.
    pub selected: usize,
    /// Where a click goes. The window derives its "Paste ↵" / "Copy ↵" label from it.
    pub footer: Footer,
    /// What a click does.
    pub use_label: UseLabel,
    /// Keep on top.
    pub on_top: bool,
    /// Dark theme.
    pub dark: bool,
    /// The toast, if one is showing.
    pub toast: Option<ToastView>,
    /// The board menu's entries while it is open.
    pub board_menu: Option<Vec<String>>,
}

struct ActiveToast {
    toast: Toast,
    phase: ToastPhase,
    seq: u64,
}

/// The Panel's brain. See the module documentation.
pub struct Controller<S, C, K, T> {
    store: S,
    services: Services<C, K, T>,
    target: Box<dyn TargetSource>,
    clock: Box<dyn Clock>,
    model: PanelModel,
    settings: Settings,
    entitlements: Entitlements,
    visible: bool,
    on_top: bool,
    toast: Option<ActiveToast>,
    toast_seq: u64,
    pending: Option<PendingPrompt>,
    board_menu_open: bool,
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
    C: Clipboard,
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
        let mut model = PanelModel::load(&store, None, None)?;
        model.set_policy(policy_of(&settings));
        model.set_target(target.current());
        Ok(Self {
            store,
            services,
            target,
            clock,
            model,
            settings,
            entitlements,
            visible: true,
            on_top: true,
            toast: None,
            toast_seq: 0,
            pending: None,
            board_menu_open: false,
        })
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
        View {
            board_name,
            current_tab: tabs.iter().position(|t| t.active).unwrap_or(0),
            tabs,
            pinned: self.model.pinned(),
            rows: self.model.rows(),
            query: self.model.query().to_owned(),
            selected: self.model.selected_position().unwrap_or(0),
            footer: self.model.footer(),
            use_label: self.model.use_label(),
            on_top: self.on_top,
            dark: self.settings.theme == Theme::Dark,
            toast: self.toast.as_ref().map(|t| ToastView {
                seq: t.seq,
                kind: t.toast.kind,
                message: t.toast.message.clone(),
                phase: t.phase,
            }),
            board_menu: self.board_menu_open.then(|| {
                self.model
                    .boards()
                    .iter()
                    .map(|b| b.name.as_str().to_owned())
                    .collect()
            }),
        }
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

    /// The template waiting for answers, if a use stopped for them.
    #[must_use]
    pub fn pending(&self) -> Option<&PendingPrompt> {
        self.pending.as_ref()
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
        match action {
            HotkeyAction::TogglePanel if self.visible => self.hide(),
            HotkeyAction::TogglePanel => self.show(),
            HotkeyAction::Picker => vec![Effect::OpenPicker],
            HotkeyAction::Capture => vec![Effect::StartCapture],
            HotkeyAction::PasteNext => vec![Effect::PasteNext],
        }
    }

    /// Shows the Panel (a second launch, the tray, the toggle hotkey). The Target is read
    /// first, while it still has the focus.
    pub fn show(&mut self) -> Vec<Effect> {
        self.on_target_changed();
        self.visible = true;
        vec![Effect::ShowPanel]
    }

    /// Hides the Panel (Esc with nothing to clear, the close button, the toggle hotkey).
    pub fn hide(&mut self) -> Vec<Effect> {
        self.visible = false;
        self.board_menu_open = false;
        vec![Effect::HidePanel]
    }

    /// The close button or Esc on an empty search.
    pub fn close(&mut self) -> Vec<Effect> {
        self.hide()
    }

    /// The user is dragging the Panel by its header.
    pub fn drag(&mut self) -> Vec<Effect> {
        vec![Effect::DragWindow]
    }

    /// The foreground window changed: the footer and the use label follow it.
    pub fn on_target_changed(&mut self) {
        self.model.set_target(self.target.current());
    }

    /// The pin button: keep the Panel on top or not.
    pub fn set_on_top(&mut self, on_top: bool) {
        self.on_top = on_top;
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
        self.board_menu_open = true;
    }

    /// Closes the board menu without a choice.
    pub fn close_board_menu(&mut self) {
        self.board_menu_open = false;
    }

    /// The user chose board `index` in the menu.
    pub fn choose_board(&mut self, index: usize) {
        self.board_menu_open = false;
        let Some(id) = self.model.boards().get(index).map(|b| b.id) else {
            return;
        };
        self.switch_board(id);
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

    /// Finishes the use that stopped for a template's answers.
    pub fn submit_answers(&mut self, answers: BTreeMap<String, String>) -> UseResult {
        match self.pending.take() {
            Some(pending) => self.run_use(pending.item, pending.mode, answers),
            None => UseResult::Failed,
        }
    }

    /// Drops the use that was waiting for answers.
    pub fn cancel_prompt(&mut self) {
        self.pending = None;
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
            clipboard_text: None,
            tz: TimeZone::UTC,
            locale: Locale::for_tag(&self.settings.language),
        };
        let mut env = Env {
            clipboard: &mut self.services.clipboard,
            keyboard: &mut self.services.keyboard,
            timing: &mut self.services.timing,
        };
        match usecase::use_item(&mut self.store, &mut env, &request) {
            Ok(used) => {
                self.pending = None;
                self.show_toast(toast::after_use(
                    used.outcome,
                    &route,
                    pasted_into.as_deref(),
                ));
                if self.model.refresh(&self.store).is_err() {
                    self.show_toast(failed());
                }
                UseResult::Done(used.outcome)
            }
            Err(UseError::NeedsAnswers(prompts)) => {
                self.pending = Some(PendingPrompt {
                    item,
                    mode,
                    prompts: prompts.clone(),
                });
                self.show_toast(toast::after_error(&UseError::NeedsAnswers(prompts)));
                UseResult::NeedsAnswers
            }
            Err(error) => {
                self.show_toast(toast::after_error(&error));
                UseResult::Failed
            }
        }
    }

    // ---- Toasts ------------------------------------------------------------------------------

    /// Shows `toast`, replacing the one on screen. The host then calls
    /// [`toast_tick`](Self::toast_tick) after [`TOAST_ENTER`] and follows the delays it returns.
    pub fn show_toast(&mut self, toast: Toast) {
        self.toast_seq += 1;
        self.toast = Some(ActiveToast {
            toast,
            phase: ToastPhase::Enter,
            seq: self.toast_seq,
        });
    }

    /// Reports hotkeys that could not be registered (the first one is shown).
    pub fn report_hotkey_problems(&mut self, problems: &[crate::host::HotkeyProblem]) {
        if let Some(problem) = problems.first() {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::HotkeyInUse {
                    chord: problem.chord.clone(),
                },
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
                Some(TOAST_HOLD)
            }
            ToastPhase::Hold => {
                active.phase = ToastPhase::Exit;
                Some(TOAST_EXIT)
            }
            ToastPhase::Exit => {
                self.toast = None;
                None
            }
        }
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

    /// Board "Work" with lists Replies (3 items + a template) and Links (1), pasting into
    /// Outlook, on a fixed clock.
    fn world_with(target: Option<Target>, settings: Settings) -> World {
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
        let targets = FakeTargets::new(target);
        let clock = FixedClock::new(at("2026-10-17T10:30:00Z"));
        let services = Services {
            clipboard: FakeClipboard {
                asks_after_polls: Some(1),
                ..FakeClipboard::default()
            },
            keyboard: FakeKeyboard::default(),
            timing: FakeTiming::default(),
        };
        let ctl = Controller::new(
            store,
            services,
            Box::new(targets.clone()),
            Box::new(clock.clone()),
            settings,
            Entitlements::new(Tier::Pro),
        )
        .expect("controller");
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
    fn copy_only_leaves_the_item_on_the_clipboard_and_sends_no_keys() {
        let mut w = world();
        let result = w.ctl.activate_row(0, UseMode::Copy);
        assert_eq!(result, UseResult::Done(UseOutcome::Copied));
        assert!(w.ctl.services().keyboard.sent.is_empty());
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
        assert!(w.ctl.services().keyboard.sent.is_empty());
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
        assert!(w.ctl.services().keyboard.sent.is_empty());
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
    fn a_template_with_prompts_waits_for_answers_then_pastes() {
        let mut w = world();
        let greeting = w.ids[3];
        assert_eq!(
            w.ctl.activate_row(3, UseMode::Paste),
            UseResult::NeedsAnswers
        );
        let pending = w.ctl.pending().expect("pending");
        assert_eq!(pending.item, greeting);
        assert_eq!(pending.prompts.len(), 1);
        assert_eq!(pending.prompts[0].name, "Name");
        assert_eq!(uses(&w, greeting), 0);
        assert!(
            w.ctl.services().clipboard.log.is_empty(),
            "nothing touched the clipboard"
        );
        let toast = w.ctl.view().toast.expect("toast");
        assert_eq!(
            (toast.kind, toast.message),
            (ToastKind::Neutral, ToastMessage::NeedsAnswers)
        );

        let answers = BTreeMap::from([("Name".to_owned(), "Sam".to_owned())]);
        assert_eq!(
            w.ctl.submit_answers(answers),
            UseResult::Done(UseOutcome::Pasted)
        );
        assert!(w.ctl.pending().is_none());
        assert_eq!(
            w.ctl.services().clipboard.written[0].data(),
            [Data::Text("Hi Sam".into())]
        );
        assert_eq!(uses(&w, greeting), 1);
    }

    #[test]
    fn cancelling_a_prompt_forgets_it() {
        let mut w = world();
        w.ctl.activate_row(3, UseMode::Paste);
        w.ctl.cancel_prompt();
        assert!(w.ctl.pending().is_none());
        assert_eq!(w.ctl.submit_answers(BTreeMap::new()), UseResult::Failed);
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

    #[test]
    fn the_other_hotkeys_ask_the_host() {
        let mut w = world();
        assert_eq!(w.ctl.on_hotkey(HotkeyAction::Picker), [Effect::OpenPicker]);
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::Capture),
            [Effect::StartCapture]
        );
        assert_eq!(
            w.ctl.on_hotkey(HotkeyAction::PasteNext),
            [Effect::PasteNext]
        );
        assert!(w.ctl.visible(), "none of them touches the Panel");
    }

    #[test]
    fn close_hides_and_drag_asks_the_host() {
        let mut w = world();
        assert_eq!(w.ctl.close(), [Effect::HidePanel]);
        assert!(!w.ctl.visible());
        assert_eq!(w.ctl.drag(), [Effect::DragWindow]);
        assert_eq!(w.ctl.show(), [Effect::ShowPanel]);
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
    fn the_dark_theme_setting_reaches_the_view() {
        let settings = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        assert!(world_with(None, settings).ctl.view().dark);
    }
}
