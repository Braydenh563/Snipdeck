//! Using an item: the Picker, the template fill form, quick capture, stacks and paste next, and the sensitive clipboard clear.

use std::{collections::BTreeMap, time::Duration};

use snipdeck_core::{
    Feature, ItemId, UseMode, UseOutcome,
    template::{Locale, Template},
};
use snipdeck_platform::{
    pipeline::{Clipboard, Env, Keyboard, Timing},
    route::Route,
};

use super::{
    CLIPBOARD_CLEAR_ATTEMPTS, CLIPBOARD_CLEAR_RETRY, ClipboardDigest, Controller, FillSubmission,
    Library, PendingClear, REFOCUS_WAIT, StackView, UseResult, failed, neutral,
};
use crate::{
    capture::{self, Captured},
    fill::FillModel,
    host::ClipboardText,
    paste_next::{self},
    picker::{PickerModel, Scope},
    toast::{self, Toast, ToastKind, ToastMessage},
    usecase::{self, UseError, UseRequest},
};

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
    pub(super) fn open_fill(&mut self, item: ItemId, mode: UseMode) -> Result<(), UseError> {
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

    pub(super) fn save_text(&mut self, text: Option<&str>) {
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

    pub(super) fn run_use(
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
            // Only a template that says `{clipboard}` reads what the user copied (A22).
            clipboard_text: if usecase::template_source(&self.store, item)
                .ok()
                .flatten()
                .and_then(|(_, src)| snipdeck_core::template::Template::parse(&src).ok())
                .is_some_and(|t| t.uses_clipboard())
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
                if let Some(line) = used.count_failed_line() {
                    crate::trace!("{line}");
                }
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
    pub(super) fn schedule_clipboard_clear(&mut self, item: ItemId, outcome: UseOutcome) {
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
        // The sequence number right after OUR write, not now: anything that changed the
        // clipboard in between must not be mistaken for our content (A7).
        let sequence = self.services.clipboard.written_sequence();
        self.clipboard_clear = (digest.is_some() || sequence.is_some()).then_some(PendingClear {
            due,
            digest,
            sequence,
            attempts: 0,
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
            && pending.attempts + 1 < CLIPBOARD_CLEAR_ATTEMPTS
            && let Ok(due) = self.clock.now().checked_add(CLIPBOARD_CLEAR_RETRY)
        {
            pending.attempts += 1;
            pending.due = due;
            self.clipboard_clear = Some(pending);
        }
    }
}
