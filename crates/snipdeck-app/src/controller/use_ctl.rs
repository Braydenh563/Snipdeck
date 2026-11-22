//! Using an item: the Picker, the template fill form, quick capture, stacks and paste next, and the sensitive clipboard clear.

use std::{collections::BTreeMap, time::Duration};

use snipdeck_core::{
    ItemId, UseMode, UseOutcome,
    template::{Template, transform::Transform},
};
use snipdeck_platform::{
    pipeline::{Clipboard, Env, Keyboard, Timing},
    route::Route,
};

use super::{
    CLIPBOARD_CLEAR_ATTEMPTS, CLIPBOARD_CLEAR_RETRY, ClipboardDigest, Controller, FillSubmission,
    Library, PendingClear, REFOCUS_WAIT, StackView, UseResult, failed, neutral,
    views::{FillErase, PICKER_DETAIL_CHARS, PickerDetail},
};
use crate::{
    capture::{self, Captured},
    fill::FillModel,
    host::ClipboardText,
    paste_next::{self},
    picker::{PickerModel, Scope},
    toast::{self, Toast, ToastKind, ToastMessage},
    usecase::{self, Retype, UseError, UseRequest},
};

/// A typed trigger to erase with a paste.
pub(super) struct Erasing<'a> {
    /// Backspaces to send.
    pub(super) chars: usize,
    /// The app the trigger was typed into: the paste only goes while it is still in front.
    pub(super) app: Option<String>,
    /// The window of that app (`0` when unknown): another document of the same app gets nothing.
    pub(super) window: usize,
    /// `false` once a key was typed after the trigger (none for a confirmed fill form, whose own
    /// keystrokes the hook also sees).
    pub(super) fresh: Option<&'a dyn Fn() -> bool>,
    /// What the trigger asks of the text that replaces it.
    pub(super) retype: Retype,
}

/// A paste whose key is out and that waits for its target (see [`Controller::paste_tick`]).
pub(super) struct PendingPaste<C: Clipboard> {
    pending: usecase::Pending<C>,
    route: Route,
    pasted_into: Option<String>,
    sequence_before: Option<u32>,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Using an item -----------------------------------------------------------------------

    /// Uses `item`: pastes or copies it as `mode` and the Target allow, counts the use and
    /// raises a toast.
    pub fn use_item(&mut self, item: ItemId, mode: UseMode) -> UseResult {
        self.run_use(item, mode, BTreeMap::new(), None)
    }

    /// "Paste as...": pastes `item` as `transform` says, as plain text with the caret at the end.
    /// A template with questions opens its fill form first, as for a plain use.
    pub fn use_item_as(&mut self, item: ItemId, transform: Transform) -> UseResult {
        self.run_use(item, UseMode::Paste, BTreeMap::new(), Some(transform))
    }

    // ---- The Picker --------------------------------------------------------------------------

    /// Opens the Picker over the whole library. The Target is read first, while it still has
    /// the focus: the Picker takes the foreground to type into, and the paste goes back to
    /// the window it came from.
    pub fn open_picker(&mut self) {
        if self.fill.take().is_some() {
            // The Picker replaces an open form; say so rather than drop it silently.
            self.show_toast(neutral(ToastMessage::FormClosed));
        }
        self.fill_erase = None;
        self.on_target_changed();
        match PickerModel::load_cached(&self.store, self.model.board(), &self.index_cache) {
            Ok(picker) => {
                let mut picker = picker.with_list(self.model.list().map(|l| l.id));
                // Start where the user left off; "This list" only when the Panel shows one.
                let scope = match self.picker_scope {
                    Scope::List if !picker.has_list() => Scope::All,
                    Scope::Board if self.model.board().is_none() => Scope::All,
                    Scope::Tag if !picker.has_tags() => Scope::All,
                    scope => scope,
                };
                picker.set_tag(self.picker_tag);
                picker.set_scope(scope);
                self.picker = Some(picker);
            }
            Err(_) => self.show_toast(failed()),
        }
    }

    /// Reads the library again for an open Picker after one of its own item keys changed something,
    /// keeping its query, scope and selected row.
    fn refresh_picker(&mut self) {
        let Some(old) = self.picker.take() else {
            return;
        };
        let (query, scope, at) = (old.query().to_owned(), old.scope(), old.selected());
        let (list, tag) = (old.list(), old.tag());
        drop(old);
        let loaded = PickerModel::load_cached(&self.store, self.model.board(), &self.index_cache);
        if let Ok(picker) = loaded {
            let mut picker = picker.with_list(list);
            picker.set_tag(tag);
            picker.set_scope(scope);
            picker.set_query(&self.store, &query).ok();
            picker.move_selection(isize::try_from(at).unwrap_or(0));
            self.picker = Some(picker);
        }
    }

    /// Ctrl+P in the Picker on result row `index`: pins the item, or unpins it when it is pinned.
    pub fn picker_pin(&mut self, index: usize) {
        let Some(id) = self.picker.as_ref().and_then(|p| p.item_at(index)) else {
            return;
        };
        let pinned = self.store.item(id).is_ok_and(|i| i.pinned);
        self.set_item_pinned(id, !pinned);
        self.refresh_picker();
    }

    /// Ctrl+D in the Picker on result row `index`: a copy of the item right after it.
    pub fn picker_duplicate(&mut self, index: usize) {
        let Some(id) = self.picker.as_ref().and_then(|p| p.item_at(index)) else {
            return;
        };
        self.duplicate(&[id]);
        self.refresh_picker();
    }

    /// Delete in the Picker (with an empty search) on result row `index`: the item goes to the Trash
    /// with an Undo toast.
    pub fn picker_trash(&mut self, index: usize) {
        let Some(id) = self.picker.as_ref().and_then(|p| p.item_at(index)) else {
            return;
        };
        self.trash_items(&[id]);
        self.refresh_picker();
    }

    /// Closes the Picker without using anything (Esc, the hotkey again) and hands the focus
    /// back to the Target.
    pub fn close_picker(&mut self) {
        if self.picker.take().is_some() {
            self.target.refocus();
        }
    }

    /// Closes the Picker because the user clicked another window: that window has the focus
    /// now, so the Target is not refocused (it would take the focus back from where they
    /// clicked).
    pub fn dismiss_picker(&mut self) {
        self.picker = None;
    }

    /// The Picker's empty state "Open Snipdeck": the Picker closes and the Panel shows.
    pub fn picker_open_panel(&mut self) {
        self.picker = None;
        let effects = self.show();
        self.pending.extend(effects);
    }

    /// The Picker, while it is open.
    #[must_use]
    pub fn picker(&self) -> Option<&PickerModel> {
        self.picker.as_ref()
    }

    /// The Picker's search text changed.
    pub fn picker_query(&mut self, query: &str) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        if picker.set_query(&self.store, query).is_err() {
            self.show_toast(failed());
        }
    }

    /// The suggestions for the word at `caret` in the Picker's search `query` (none without a Picker).
    #[must_use]
    pub fn picker_suggestions(
        &self,
        query: &str,
        caret: usize,
    ) -> Vec<crate::search_suggest::Suggestion> {
        self.picker.as_ref().map_or_else(Vec::new, |picker| {
            crate::search_suggest::suggest(query, caret, &picker.suggest_index())
        })
    }

    /// The Picker's scope changed.
    pub fn picker_scope(&mut self, scope: Scope) {
        if let Some(picker) = &mut self.picker {
            picker.set_scope(scope);
            self.picker_scope = scope;
            self.picker_tag = picker.tag();
        }
    }

    /// Ctrl+Tab (`dir` 1) or Ctrl+Shift+Tab (-1) in the Picker's tag scope: the next or previous tag.
    pub fn picker_step_tag(&mut self, dir: isize) {
        if let Some(picker) = &mut self.picker {
            if picker.scope() != Scope::Tag {
                return;
            }
            picker.next_tag(dir);
            self.picker_tag = picker.tag();
        }
    }

    /// F3 in the Picker: opens or closes the preview pane.
    pub fn picker_toggle_detail(&mut self) {
        self.picker_detail = !self.picker_detail;
    }

    /// The preview pane for the selected row: the whole text (cut at `PICKER_DETAIL_CHARS`),
    /// or a mask for a sensitive item. `None` while the pane is closed or no row is selected.
    pub(super) fn picker_detail_view(&self, picker: &PickerModel) -> Option<PickerDetail> {
        if !self.picker_detail {
            return None;
        }
        let id = picker.item_at(picker.selected())?;
        let item = self.store.item(id).ok()?;
        if item.sensitive {
            return Some(PickerDetail {
                text: String::new(),
                hidden: true,
            });
        }
        let text = crate::usecase::item_text(&self.store, id)
            .ok()
            .flatten()
            .unwrap_or_default();
        let mut shown: String = text.chars().take(PICKER_DETAIL_CHARS).collect();
        if text.chars().nth(PICKER_DETAIL_CHARS).is_some() {
            shown.push('…');
        }
        Some(PickerDetail {
            text: shown,
            hidden: false,
        })
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
    pub(super) fn open_fill(
        &mut self,
        item: ItemId,
        mode: UseMode,
        transform: Option<Transform>,
        selection: Option<String>,
    ) -> Result<(), UseError> {
        let (title, source) =
            usecase::template_source(&self.store, item)?.ok_or(UseError::NotText)?;
        let template = usecase::parse_template(&self.store, &source)?;
        let locale = usecase::template_locale(&self.settings.language);
        self.fill_erase = None;
        self.fill_seq += 1;
        self.fill = Some(
            FillModel::new(item, mode, title, template, locale)
                .with_transform(transform)
                .with_selection(selection),
        );
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
            // Typing the answers is use: the idle lock must not close the form under the user.
            self.note_activity();
        }
    }

    /// A day was picked in the month grid of date field `index`: the field takes it in the
    /// user's date order. Returns the text the field now holds.
    pub fn fill_date_picked(
        &mut self,
        index: usize,
        year: i32,
        month: i32,
        day: i32,
    ) -> Option<String> {
        let text = self.fill.as_mut()?.pick_date(index, year, month, day)?;
        self.note_activity();
        Some(text)
    }

    /// Closes the fill form if it belongs to a sensitive template, as the lock does: the form
    /// holds the decrypted template. Returns whether one was open.
    pub(super) fn close_sensitive_fill(&mut self) -> bool {
        let sensitive = self
            .fill
            .as_ref()
            .is_some_and(|f| self.store.item(f.item()).map_or(true, |i| i.sensitive));
        if sensitive {
            self.fill = None;
            self.fill_erase = None;
        }
        sensitive
    }

    /// Closes the fill form without using the template (Esc, the close button) and hands the
    /// focus back to the Target.
    pub fn cancel_fill(&mut self) {
        self.fill_erase = None;
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
            transform: fill.transform(),
            selection: fill.selection().map(str::to_owned),
            erase: self.fill_erase.take(),
        };
        self.fill = None;
        Some(submission)
    }

    /// Re-runs the use that stopped for answers, now with them.
    pub fn run_fill(&mut self, submission: FillSubmission) -> UseResult {
        self.selection_for_use = submission.selection;
        let erase = submission.erase.map(|e| Erasing {
            chars: e.chars,
            app: e.app,
            window: e.window,
            fresh: None,
            retype: e.retype,
        });
        self.run_use_erasing(
            submission.item,
            submission.mode,
            submission.answers,
            submission.transform,
            erase,
        )
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
        let clip = self.services.clipboard.read_rich();
        if clip.excluded {
            // The source asked to be left out (a password manager): nothing is saved.
            self.show_toast(neutral(ToastMessage::NothingToSave));
            return;
        }
        self.save_rich(
            clip.text.as_deref(),
            clip.html.as_deref(),
            clip.rtf.as_deref(),
        );
    }

    /// Text dropped on the Panel: saved like a capture, at the top of the list the Panel shows.
    pub fn panel_drop_text(&mut self, text: &str) {
        self.save_text(Some(text));
    }

    /// The quick-add box's Create (Enter): saves `text` at the end of the list the Panel shows (or
    /// of the Main window's open list when `main`), as `kind` (`None` picks it from the text), and
    /// turns Sensitive on when asked. The new item is selected and a toast says where it went, with
    /// Undo (to the Trash) and Edit. Blank text does nothing. Returns whether an item was added.
    pub fn quick_add(
        &mut self,
        text: &str,
        kind: Option<snipdeck_core::ItemKind>,
        sensitive: bool,
        origin: crate::item_editor::Origin,
    ) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let shown = match origin {
            crate::item_editor::Origin::Panel => self.model.list().map(|l| l.id),
            crate::item_editor::Origin::Main => self.main.open_list().map(|l| l.id),
        };
        let destination = match capture::default_list(&self.store, shown) {
            Ok(Some(d)) => d,
            Ok(None) => {
                self.show_toast(neutral(ToastMessage::NoListToSave));
                return false;
            }
            Err(_) => {
                self.show_toast(failed());
                return false;
            }
        };
        let kind = kind.unwrap_or_else(|| capture::detect_kind(text));
        let saved = capture::capture_as(
            &mut self.store,
            &destination,
            Some(text),
            kind,
            snipdeck_core::Placement::Last,
            self.clock.now(),
        );
        match saved {
            Ok(Captured::Saved { item, list }) => {
                if !self.library_changed() {
                    return true;
                }
                match origin {
                    crate::item_editor::Origin::Panel => self.model.select_item(item),
                    crate::item_editor::Origin::Main => {
                        self.main_fresh();
                        if let Some(at) = self
                            .main
                            .keys()
                            .iter()
                            .position(|k| *k == crate::main_window::Key::Item(item))
                        {
                            self.main.set_cursor(at);
                        }
                    }
                }
                self.show_saved_toast(
                    Toast {
                        kind: ToastKind::Success,
                        message: ToastMessage::Saved { list },
                    },
                    item,
                );
                if sensitive {
                    self.toggle_item_sensitive(item);
                }
                true
            }
            Ok(Captured::Empty) => false,
            Ok(Captured::TooLarge) => {
                self.show_toast(neutral(ToastMessage::TooLargeToSave));
                false
            }
            Err(_) => {
                self.show_toast(failed());
                false
            }
        }
    }

    pub(super) fn save_text(&mut self, text: Option<&str>) {
        self.save_rich(text, None, None);
    }

    /// Saves `text` with the HTML and RTF flavours that came with it, as a capture.
    fn save_rich(&mut self, text: Option<&str>, html: Option<&str>, rtf: Option<&[u8]>) {
        let current = self.model.list().map(|l| l.id);
        let toast = match capture::default_list(&self.store, current) {
            Ok(Some(list)) => {
                let kind = text.map_or(
                    snipdeck_core::ItemKind::Text,
                    crate::paste_create::kind_of_text,
                );
                // Looked up before the save, so the new copy does not find itself.
                let twin = text.and_then(|t| crate::duplicate::find_text(&self.store, t));
                let saved = capture::capture_with(
                    &mut self.store,
                    &list,
                    text,
                    (html, rtf),
                    capture::Shape::plain(kind),
                    snipdeck_core::Placement::First,
                    self.clock.now(),
                );
                match saved {
                    Ok(Captured::Saved { list, .. }) => {
                        // Saved is saved: a refresh that fails is only logged.
                        self.refresh_logged("a quick save");
                        Toast {
                            kind: ToastKind::Success,
                            message: match twin {
                                Some(twin) => ToastMessage::SavedAgain {
                                    list,
                                    was_in: twin.list_name,
                                },
                                None => ToastMessage::Saved { list },
                            },
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

    pub(super) fn stack_view(&self) -> Option<StackView> {
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
            self.show_toast(neutral(ToastMessage::StackRunning));
            return;
        }
        if let Some(at) = self.stack.iter().position(|id| *id == item) {
            self.stack.remove(at);
        } else {
            self.stack.push(item);
        }
    }

    /// "Start stack": Paste next now walks the marked items.
    pub fn start_stack(&mut self) {
        if self.stack.is_empty() {
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

    /// Brings the Target forward for a paste and, only when that switched windows, waits for its
    /// focused control to come back ([`REFOCUS_WAIT`]); a Target already in front needs no wait.
    pub(super) fn refocus_for_paste(&mut self, mode: UseMode, route: &Route) {
        if mode != UseMode::Copy
            && matches!(route, Route::Paste { .. })
            && self.target.refocus().must_wait()
        {
            self.services.timing.sleep(REFOCUS_WAIT);
        }
    }

    /// Whether a paste is still waiting for its target (the host runs
    /// [`paste_tick`](Self::paste_tick) every [`PASTE_TICK`](super::PASTE_TICK) while so).
    #[must_use]
    pub fn paste_pending(&self) -> bool {
        self.paste.is_some()
    }

    /// Sets what runs when a paste starts waiting for its target. The host arms the polling
    /// timer there; it must not borrow the controller before the call returns.
    pub fn set_on_paste_waiting(&mut self, wake: std::rc::Rc<dyn Fn()>) {
        self.on_paste_waiting = Some(wake);
    }

    /// Whether the caller should start the timer chain that polls the waiting paste: true once
    /// per paste, until [`paste_timer_next`](Self::paste_timer_next) ends the chain.
    pub fn claim_paste_timer(&mut self) -> bool {
        if self.paste.is_none() || self.paste_timer {
            return false;
        }
        self.paste_timer = true;
        true
    }

    /// One tick of the chain: looks at the paste and says whether to tick again.
    pub fn paste_timer_next(&mut self) -> bool {
        self.paste_tick();
        self.paste_timer = self.paste.is_some();
        self.paste_timer
    }

    /// One look at the waiting paste, without sleeping: when it is over, the toast, the
    /// clipboard bookkeeping and the refresh happen now.
    pub fn paste_tick(&mut self) {
        let Some(mut waiting) = self.paste.take() else {
            return;
        };
        let polled = waiting.pending.poll(
            &mut self.store,
            &mut self.services.clipboard,
            &mut self.services.keyboard,
            &mut self.services.timing,
        );
        match polled {
            Some(used) => self.finish_use(
                waiting.pending.item(),
                &waiting.route,
                waiting.pasted_into.as_deref(),
                waiting.sequence_before,
                &used,
            ),
            None => self.paste = Some(waiting),
        }
    }

    /// Runs a waiting paste to its end now (a new use must not start over it: two pastes would
    /// share one clipboard).
    pub fn drain_paste(&mut self) {
        while self.paste.is_some() {
            self.paste_tick();
            if self.paste.is_some() {
                self.services.timing.sleep(super::PASTE_TICK);
            }
        }
    }

    /// The use is over: counts it, schedules the clipboard clear, toasts and refreshes.
    fn finish_use(
        &mut self,
        item: ItemId,
        route: &Route,
        pasted_into: Option<&str>,
        sequence_before: Option<u32>,
        used: &usecase::Used,
    ) {
        if used.outcome == UseOutcome::Pasted {
            self.follow_restored_clipboard(sequence_before);
        }
        if let Some(line) = used.count_failed_line() {
            crate::trace!("{line}");
        }
        self.note_activity();
        self.index_cache.note_use(item);
        self.schedule_clipboard_clear(item, used.outcome);
        self.show_toast(toast::after_use(used.outcome, route, pasted_into));
        // The paste worked: a refresh that fails must not turn that into "failed".
        self.refresh_logged("a paste");
    }

    pub(super) fn run_use(
        &mut self,
        item: ItemId,
        mode: UseMode,
        answers: BTreeMap<String, String>,
        transform: Option<Transform>,
    ) -> UseResult {
        self.run_use_erasing(item, mode, answers, transform, None)
    }

    /// What a template reads from outside before it is rendered: the selection (read first, while
    /// the Target still has the focus, since the fill form takes it, and before the clipboard is
    /// read, since the copy puts the user's clipboard back), the clipboard text (only a template
    /// that says `{clipboard}` reads what the user copied, A22) and the clipboard history (only
    /// one that names a position beyond the first).
    fn read_use_inputs(
        &mut self,
        template: Option<&Template>,
        mode: UseMode,
        route: &Route,
        pasted_into: Option<&str>,
        first_pass: bool,
    ) -> (Option<String>, Option<String>, Vec<String>) {
        let mut selection = self.selection_for_use.take();
        if selection.is_none()
            && first_pass
            && mode != UseMode::Copy
            && template.is_some_and(Template::uses_selection)
            && crate::selection::copy_is_safe(route, pasted_into)
        {
            let mut env = Env {
                clipboard: &mut self.services.clipboard,
                keyboard: &mut self.services.keyboard,
                timing: &mut self.services.timing,
            };
            selection = crate::selection::capture(&mut env);
        }
        let clipboard_text = if template.is_some_and(Template::uses_clipboard) {
            self.services.clipboard.read_text()
        } else {
            None
        };
        let clipboard_history = if template.is_some_and(|t| t.max_clipboard_position() > 1) {
            self.services.clipboard.read_history()
        } else {
            Vec::new()
        };
        (selection, clipboard_text, clipboard_history)
    }

    /// [`run_use`](Self::run_use) that also erases a typed trigger: with the paste key, in one
    /// injection, and only if the user has not typed on or changed window by then. A template
    /// that needs answers erases nothing yet: the erase waits for the confirmed form.
    pub(super) fn run_use_erasing(
        &mut self,
        item: ItemId,
        mode: UseMode,
        answers: BTreeMap<String, String>,
        transform: Option<Transform>,
        erase: Option<Erasing<'_>>,
    ) -> UseResult {
        self.drain_paste();
        self.last_use_chars = None;
        let chars = erase.as_ref().map_or(0, |e| e.chars);
        let route = self.model.route();
        let pasted_into = self.model.target().map(|t| t.app.clone());
        self.refocus_for_paste(mode, &route);
        let template = usecase::template_source(&self.store, item)
            .ok()
            .flatten()
            .and_then(|(_, src)| usecase::parse_template(&self.store, &src).ok());
        let (selection, clipboard_text, clipboard_history) = self.read_use_inputs(
            template.as_ref(),
            mode,
            &route,
            pasted_into.as_deref(),
            answers.is_empty(),
        );
        let request = UseRequest {
            item,
            mode,
            route: route.clone(),
            now: self.clock.now(),
            answers,
            transform,
            uuids: None,
            erase: chars,
            retype: erase.as_ref().map_or_else(Retype::default, |e| e.retype),
            clipboard_text,
            clipboard_history,
            selection: selection.clone(),
            tz: self.tz.clone(),
            locale: usecase::template_locale(&self.settings.language),
        };
        let mut env = Env {
            clipboard: &mut self.services.clipboard,
            keyboard: &mut self.services.keyboard,
            timing: &mut self.services.timing,
        };
        // Asked right before the keys go out: no key typed since the trigger, same app in front.
        let target = &self.target;
        let proceed = || {
            erase.as_ref().is_none_or(|e| {
                e.fresh.is_none_or(|fresh| fresh())
                    && target
                        .current()
                        .is_some_and(|t| Some(t.app) == e.app && t.window == e.window)
            })
        };
        // Where the clipboard stood before the use: a paste puts the user's earlier clipboard
        // back, which can be the sensitive content a pending clear is waiting for.
        let sequence_before = env.clipboard.sequence_number();
        match usecase::begin_use(&mut self.store, &mut env, &request, &proceed) {
            Ok(usecase::Started::Done(used)) => {
                self.queue.used(item);
                self.last_use_chars = used.inserted_chars;
                self.finish_use(item, &route, pasted_into.as_deref(), sequence_before, &used);
                UseResult::Done(used.outcome)
            }
            // The key is out: the rest is polled by `paste_tick`. The caller sees the paste as
            // done (it was sent); the toast waits for the real end.
            Ok(usecase::Started::Waiting(pending)) => {
                // "Paste next" walks on as soon as the key is out, not when the tail ends.
                self.queue.used(item);
                self.last_use_chars = pending.inserted_chars();
                self.paste = Some(PendingPaste {
                    pending,
                    route,
                    pasted_into,
                    sequence_before,
                });
                if let Some(wake) = &self.on_paste_waiting {
                    wake();
                }
                UseResult::Done(UseOutcome::Pasted)
            }
            Err(UseError::Platform(snipdeck_platform::pipeline::Error::Superseded)) => {
                // The clipboard is back as it was, but a sensitive Files item already wrote its
                // plaintext copies: they go on the timer too (`Pasted` arms no clipboard clear).
                self.schedule_clipboard_clear(item, UseOutcome::Pasted);
                UseResult::Cancelled
            }
            // The paste was called off but the item is still on the clipboard: it is cleared
            // like a copy would be.
            Err(UseError::Platform(snipdeck_platform::pipeline::Error::SupersededLeft)) => {
                self.schedule_clipboard_clear(item, UseOutcome::PastedNotRestored);
                UseResult::Cancelled
            }
            Err(UseError::NeedsAnswers(_)) => {
                match self.open_fill(item, mode, transform, selection) {
                    Ok(()) => {
                        // The typed code goes when the form is confirmed, not before.
                        self.fill_erase = erase.filter(|e| e.chars > 0).map(|e| FillErase {
                            chars: e.chars,
                            app: e.app,
                            window: e.window,
                            retype: e.retype,
                        });
                        UseResult::NeedsAnswers
                    }
                    Err(error) => {
                        self.show_toast(toast::after_error(&error));
                        UseResult::Failed
                    }
                }
            }
            Err(error) => {
                self.show_toast(toast::after_error(&error));
                UseResult::Failed
            }
        }
    }

    /// After a copy of a sensitive item, remembers its content on the clipboard to be cleared
    /// `clipboard_clear_secs` from now (at least 5 seconds: there is no "never"). A paste that restored the user's own
    /// clipboard leaves nothing to clear; one whose snapshot or restore failed
    /// ([`UseOutcome::PastedNotRestored`]) leaves the item there, so it is cleared like a copy.
    pub(super) fn schedule_clipboard_clear(&mut self, item: ItemId, outcome: UseOutcome) {
        if self
            .store
            .item(item)
            .is_ok_and(|i| i.sensitive && i.kind == snipdeck_core::ItemKind::File)
        {
            self.secret_files_pasted = true;
        }
        let life = Duration::from_secs(u64::from(self.settings.clipboard_clear_secs));
        // The plaintext copies go on the same timer as the clipboard, with or without a master
        // password; the clipboard's own clear below may not apply (a paste that restored it).
        if self.secret_files_pasted {
            // Keep the earliest: a later use must not postpone the delete.
            if self.secret_files_due.is_none() {
                self.secret_files_due = self.clock.now().checked_add(life).ok();
            }
        }
        if matches!(outcome, UseOutcome::Pasted)
            || !self.store.item(item).is_ok_and(|i| i.sensitive)
        {
            return;
        }
        self.arm_clipboard_clear(life);
    }

    /// Remembers what our latest write left on the clipboard, to be cleared `life` from now.
    fn arm_clipboard_clear(&mut self, life: Duration) {
        let Ok(due) = self.clock.now().checked_add(life) else {
            return;
        };
        // The sequence number right after OUR write, not now: anything that changed the
        // clipboard in between must not be mistaken for our content (A7).
        let sequence = self.services.clipboard.written_sequence();
        // The text is hashed only while the clipboard is still as our write left it. After a
        // restore (even a partial one) it holds the user's text, which must never be matched,
        // and so never wiped.
        let untouched = sequence.is_some() && self.services.clipboard.sequence_number() == sequence;
        let digest = if untouched {
            self.services
                .clipboard
                .read_text()
                .map(|text| ClipboardDigest::of(&text))
        } else {
            None
        };
        self.clipboard_clear = (digest.is_some() || sequence.is_some()).then_some(PendingClear {
            due,
            digest,
            sequence,
            attempts: 0,
        });
    }

    /// Copies the selected part of a sensitive field to the clipboard (a field of a sensitive
    /// item or template; the field's own copy would write with no markers and never clear).
    /// `start` and `end` are byte offsets into `text`, in either order. The copy carries the
    /// privacy markers and is cleared like a sensitive item's copy. Returns whether it copied.
    pub fn copy_private_selection(&mut self, text: &str, start: i32, end: i32) -> bool {
        let Some(range) = selection_range(text, start, end) else {
            return false;
        };
        let Some(payload) =
            snipdeck_platform::payload::Payload::new(vec![snipdeck_platform::payload::Data::Text(
                text[range].to_owned(),
            )])
        else {
            return false;
        };
        if self
            .services
            .clipboard
            .write_persistent(&payload.private())
            .is_err()
        {
            self.show_toast(failed());
            return false;
        }
        let life = Duration::from_secs(u64::from(self.settings.clipboard_clear_secs));
        self.arm_clipboard_clear(life);
        true
    }

    /// [`copy_private_selection`](Self::copy_private_selection), then the text without the
    /// selection (what the field shows after a cut). `None` when nothing was cut.
    pub fn cut_private_selection(&mut self, text: &str, start: i32, end: i32) -> Option<String> {
        let range = selection_range(text, start, end)?;
        if !self.copy_private_selection(text, start, end) {
            return None;
        }
        let mut rest = String::with_capacity(text.len() - range.len());
        rest.push_str(&text[..range.start]);
        rest.push_str(&text[range.end..]);
        Some(rest)
    }

    /// A paste put the earlier clipboard back, which moved the clipboard's sequence number. If
    /// that clipboard was the one a pending clear is waiting for (its number was `before` the
    /// paste), the clear now follows the restored content, so a sensitive image or file that a
    /// later paste restores is still cleared. Content copied since stays untouched.
    fn follow_restored_clipboard(&mut self, before: Option<u32>) {
        let now = self.services.clipboard.sequence_number();
        if let Some(pending) = &mut self.clipboard_clear
            && before.is_some()
            && pending.sequence == before
        {
            pending.sequence = now;
        }
    }

    /// How long until the clipboard is to be cleared of a copied sensitive item, if one waits.
    /// The host arms a timer for this and then calls
    /// [`clear_clipboard_if_due`](Self::clear_clipboard_if_due).
    #[must_use]
    pub fn clipboard_clear_wait(&self) -> Option<Duration> {
        let due = [
            self.clipboard_clear.as_ref().map(|p| p.due),
            self.secret_files_due,
        ]
        .into_iter()
        .flatten()
        .min()?;
        Some(Duration::try_from(due.duration_since(self.clock.now())).unwrap_or_default())
    }

    /// Clears the clipboard if the wait is over and it still holds the copied text.
    pub fn clear_clipboard_if_due(&mut self) {
        self.clear_clipboard(false);
        if self
            .secret_files_due
            .is_some_and(|due| self.clock.now() >= due)
        {
            self.forget_secret_files();
        }
    }

    /// Clears a pending sensitive copy now, whatever its due time (the app is quitting, and the
    /// timer that would have done it dies with the event loop). A clipboard that someone else
    /// has written to since is left alone, as for the timed clear.
    pub fn clear_pending_clipboard_now(&mut self) {
        // A paste still waiting for its target holds the user's clipboard aside (a sensitive item
        // is on it): it ends first, so the restore is not lost with the event loop.
        self.drain_paste();
        self.clear_clipboard(true);
        self.secret_files_due = None;
        // One started by the lock may still be running (`secret_files_deleting`): wait for it too.
        let pasted = std::mem::take(&mut self.secret_files_pasted);
        if std::mem::take(&mut self.secret_files_deleting) || pasted {
            // The process ends right after: a background deletion would be cut short.
            self.services.clipboard.forget_pasted_files_waiting();
        }
    }

    /// Deletes the decrypted files a sensitive Files item left in the temp folder when it was
    /// pasted (the lock and the quit call it). A file the target still holds open stays until
    /// the next start. Nothing to do when no such item was pasted. The deletion runs off the UI
    /// thread.
    pub(super) fn forget_secret_files(&mut self) {
        self.secret_files_due = None;
        if std::mem::take(&mut self.secret_files_pasted) {
            self.secret_files_deleting = true;
            self.services.clipboard.forget_pasted_files();
        }
    }

    fn clear_clipboard(&mut self, force: bool) {
        let Some(pending) = &self.clipboard_clear else {
            return;
        };
        if !force && self.clock.now() < pending.due {
            return;
        }
        let Some(mut pending) = self.clipboard_clear.take() else {
            return;
        };
        let mut ours = false;
        let mut cleared = false;
        if let Some(digest) = &pending.digest
            && let Some(text) = self.services.clipboard.read_text()
            && digest.matches(&text)
        {
            ours = true;
            cleared = self.services.clipboard.clear_if_text(&text);
        } else if let Some(sequence) = pending.sequence {
            // Not text, or the text could not be read: empty it entirely, but only if nothing
            // has touched the clipboard since our write.
            ours = self.services.clipboard.sequence_number() == Some(sequence);
            cleared = ours && self.services.clipboard.clear_if_sequence(sequence);
        }
        // Still ours but not cleared: the clipboard was busy. Try again shortly (A6).
        if ours
            && !cleared
            && !force
            && pending.attempts + 1 < CLIPBOARD_CLEAR_ATTEMPTS
            && let Ok(due) = self.clock.now().checked_add(CLIPBOARD_CLEAR_RETRY)
        {
            pending.attempts += 1;
            pending.due = due;
            self.clipboard_clear = Some(pending);
        }
    }
}

/// The byte range between two offsets (either order) when it is a non-empty selection on
/// character boundaries of `text`.
fn selection_range(text: &str, a: i32, b: i32) -> Option<std::ops::Range<usize>> {
    let (a, b) = (usize::try_from(a).ok()?, usize::try_from(b).ok()?);
    let range = a.min(b)..a.max(b);
    (!range.is_empty() && text.is_char_boundary(range.start) && text.is_char_boundary(range.end))
        .then_some(range)
}
