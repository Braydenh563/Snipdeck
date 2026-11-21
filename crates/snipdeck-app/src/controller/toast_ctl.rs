//! The toast timeline: showing, Undo, hover, ticks and the reports that end in a toast.

use std::time::Duration;

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{
    ActiveToast, Controller, Effect, Library, TOAST_ERROR_GUARD, TOAST_ERROR_HOLD, TOAST_EXIT,
    TOAST_HISTORY, TOAST_HOLD, TOAST_HOVER_RECHECK, TOAST_HOVER_RECHECKS, TOAST_READ_BASE,
    TOAST_READ_MAX, TOAST_READ_PER_CHAR, TOAST_UNDO_HOLD, ToastPhase, Undo, neutral,
};
use crate::{
    host::ClipboardText,
    toast::{Toast, ToastKind, ToastMessage},
};

/// How long a toast is held: `base`, or longer when its `chars` characters need more time to read.
fn hold_for(base: Duration, chars: usize) -> Duration {
    let chars = u32::try_from(chars).unwrap_or(u32::MAX);
    base.max((TOAST_READ_BASE + TOAST_READ_PER_CHAR.saturating_mul(chars)).min(TOAST_READ_MAX))
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Toasts ------------------------------------------------------------------------------

    /// Ctrl+V in a text field: when the clipboard holds a picture and no text, says so with a
    /// toast (the field cannot take a picture) and returns `true` so the paste is swallowed
    /// instead of failing silently. With any text on it the normal paste goes ahead (`false`).
    pub fn refuse_pasted_image(&mut self) -> bool {
        if self.services.clipboard.read_text().is_some() || !self.services.clipboard.has_image() {
            return false;
        }
        self.show_toast(neutral(ToastMessage::ImageNotPasteable));
        true
    }

    /// Shows `toast`, replacing the one on screen: the latest toast wins, whatever action it
    /// belongs to (a retry that works must not hide behind the error of the try before it).
    /// The one exception: a plain message does not replace an error in its first
    /// [`TOAST_ERROR_GUARD`] seconds, it waits and shows after them (the error stays in
    /// [`recent_toasts`](Self::recent_toasts)); a toast with an Undo button always shows at once.
    /// Within one operation the success comes first and a notice about it
    /// (`refresh_or_notice`) last. The host then calls
    /// [`toast_tick`](Self::toast_tick) after [`TOAST_ENTER`](crate::controller::TOAST_ENTER)
    /// and follows the delays it returns.
    pub fn show_toast(&mut self, toast: Toast) {
        if toast.kind != ToastKind::Danger && self.toast.as_ref().is_some_and(|t| t.guard) {
            self.pending_toast = Some(toast);
            return;
        }
        self.show_toast_now(toast);
    }

    /// [`show_toast`](Self::show_toast) without waiting behind an error.
    fn show_toast_now(&mut self, toast: Toast) {
        // A locked sensitive item behind a master password asks for it, as well as saying so.
        if toast.message == ToastMessage::Locked {
            self.request_unlock();
        }
        self.toast_seq += 1;
        self.undo = None;
        self.pending_toast = None;
        if !matches!(toast.message, ToastMessage::ItemPlaced { .. })
            && self.toast_history.back() != Some(&toast)
        {
            if self.toast_history.len() == TOAST_HISTORY {
                self.toast_history.pop_front();
            }
            self.toast_history.push_back(toast.clone());
        }
        self.toast = Some(ActiveToast {
            guard: toast.kind == ToastKind::Danger,
            toast,
            phase: ToastPhase::Enter,
            seq: self.toast_seq,
            hovered: false,
            extended: 0,
            chars: 0,
        });
    }

    /// Shows `toast` once the one on screen is gone: at once when none shows, else it waits
    /// its turn behind the toasts already waiting. A message identical to the one before it (the
    /// last waiting, or the one on screen when none waits) is not queued again. Used for the
    /// notices that come together at start-up; everything else uses
    /// [`show_toast`](Self::show_toast), where the latest wins.
    pub fn queue_toast(&mut self, toast: Toast) {
        let before = self
            .toast_queue
            .back()
            .or_else(|| self.toast.as_ref().map(|t| &t.toast));
        if before.is_some_and(|b| b.message == toast.message) {
            return;
        }
        if self.toast.is_none() {
            self.show_toast(toast);
        } else {
            self.toast_queue.push_back(toast);
        }
    }

    /// The toast on screen has gone (dismissed or timed out): the next waiting one shows.
    fn show_next_queued(&mut self) {
        if let Some(next) = self.toast_queue.pop_front() {
            self.show_toast(next);
        }
    }

    /// The last messages shown as toasts, newest first (at most [`TOAST_HISTORY`]), for the
    /// Help menu's "Recent messages": a toast that went before it could be read is still there.
    #[must_use]
    pub fn recent_toasts(&self) -> Vec<ToastMessage> {
        self.toast_history
            .iter()
            .rev()
            .map(|t| t.message.clone())
            .collect()
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

    /// Shows `toast` with an Undo button that takes back history step `step`
    /// ([`main_undo`](Self::main_undo)); no step, no button. The toast stays for 5 s; a newer
    /// toast takes the button away (Ctrl+Z still takes the step back).
    pub fn show_undo_toast(&mut self, toast: Toast, step: Option<u64>) {
        self.show_toast_now(toast);
        self.undo = step.map(|step| Undo {
            seq: self.toast_seq,
            step: Some(step),
            adds: None,
            list: None,
        });
    }

    /// Shows `toast` ("Saved to …") for the new item `id`, with an Undo button that moves it to
    /// the Trash and an Edit button that opens it in the editor.
    pub fn show_saved_toast(&mut self, toast: Toast, id: snipdeck_core::ItemId) {
        self.show_saved_toast_made(toast, id, None);
    }

    /// [`show_saved_toast`](Self::show_saved_toast) for a save that also made `list`: Undo
    /// trashes the list with the item.
    pub fn show_saved_toast_made(
        &mut self,
        toast: Toast,
        id: snipdeck_core::ItemId,
        list: Option<snipdeck_core::ListId>,
    ) {
        self.show_toast_now(toast);
        self.undo = Some(Undo {
            seq: self.toast_seq,
            step: None,
            adds: Some(id),
            list,
        });
    }

    /// The toast's ×: the toast goes now, and its Undo with it.
    pub fn toast_dismiss(&mut self) {
        if self.toast.is_some() {
            self.clear_toast();
        }
    }

    /// Takes the toast on screen away with its Undo, then shows the next queued one. Every path
    /// that ends a toast goes through here, so the queue never stalls.
    pub(crate) fn clear_toast(&mut self) {
        self.toast = None;
        self.undo = None;
        // A message that waited behind a guarded error comes before the queued ones.
        if let Some(next) = self.pending_toast.take() {
            self.show_toast_now(next);
        } else {
            self.show_next_queued();
        }
    }

    /// The toast's Edit button: opens the editor on the item the toast is about and closes the toast.
    pub fn toast_edit(&mut self) -> Vec<Effect> {
        let Some(undo) = self.undo.take() else {
            return Vec::new();
        };
        if self.toast.as_ref().is_none_or(|t| t.seq != undo.seq) {
            return Vec::new();
        }
        self.clear_toast();
        undo.adds.map(Effect::EditItem).into_iter().collect()
    }

    /// The toast's button: "Switch" on the sort offer (the list goes to manual order), "Open log" on the crash toast (opens the reports folder and closes
    /// the toast; a folder that cannot be opened says so), "Undo" on any other.
    pub fn toast_action(&mut self) {
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.toast.message == ToastMessage::CrashedLastTime)
        {
            let opened = self.log_opener.as_ref().is_some_and(|open| open().is_ok());
            if opened {
                self.clear_toast();
            } else {
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::CannotOpenLog,
                });
            }
            return;
        }
        if let Some(ToastMessage::SwitchToManual { list }) =
            self.toast.as_ref().map(|t| t.toast.message.clone())
        {
            self.set_list_sort(list, snipdeck_core::ListSort::Manual);
            return;
        }
        self.main_undo();
    }

    /// Whether the caller should start the timer chain for toast `seq`: true once per toast,
    /// for whichever window sees it first (the Panel and the Main window share one toast).
    /// `chars` is the length of the words the window shows: a long message is held longer.
    pub fn claim_toast_timer(&mut self, seq: u64, chars: usize) -> bool {
        if self.timer_seq == seq {
            return false;
        }
        self.timer_seq = seq;
        if let Some(active) = self.toast.as_mut().filter(|t| t.seq == seq) {
            active.chars = chars;
        }
        true
    }

    /// Reports the hotkeys that could not be registered: one is worded by why it failed, several
    /// are named together. All of them are kept for the Settings rows ([`Self::hotkey_problem`]);
    /// an empty list (everything registered) forgets the earlier ones.
    pub fn report_hotkey_problems(&mut self, problems: &[crate::host::HotkeyProblem]) {
        self.hotkey_problems = problems.to_vec();
        let message = match problems {
            [] => return,
            [problem] => {
                let chord = problem.chord.clone();
                match problem.failure {
                    crate::host::HotkeyFailure::InUse => ToastMessage::HotkeyInUse { chord },
                    crate::host::HotkeyFailure::OwnAction => {
                        ToastMessage::HotkeyOwnAction { chord }
                    }
                    crate::host::HotkeyFailure::Invalid => ToastMessage::HotkeyInvalid { chord },
                    crate::host::HotkeyFailure::Refused(code) => {
                        ToastMessage::HotkeyRefused { chord, code }
                    }
                    crate::host::HotkeyFailure::Unavailable => {
                        ToastMessage::HotkeyUnavailable { chord }
                    }
                }
            }
            several => ToastMessage::HotkeysFailed {
                chords: several
                    .iter()
                    .map(|p| p.chord.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            },
        };
        self.show_toast(Toast {
            kind: ToastKind::Danger,
            message,
        });
    }

    /// Advances toast `seq` to its next phase. Returns how long to wait before ticking again,
    /// or `None` when the toast is gone or `seq` is not the toast on screen any more.
    pub fn toast_tick(&mut self, seq: u64) -> Option<Duration> {
        let active = self.toast.as_mut().filter(|t| t.seq == seq)?;
        match active.phase {
            ToastPhase::Enter => {
                active.phase = ToastPhase::Hold;
                let chars = active.chars;
                if self.undo.as_ref().is_some_and(|u| u.seq == seq)
                    || matches!(active.toast.message, ToastMessage::SwitchToManual { .. })
                {
                    Some(hold_for(TOAST_UNDO_HOLD, chars))
                } else if active.toast.kind == ToastKind::Danger {
                    // The first stretch is the guard; the rest of the hold follows it.
                    Some(TOAST_ERROR_GUARD.min(hold_for(TOAST_ERROR_HOLD, chars)))
                } else {
                    Some(hold_for(TOAST_HOLD, chars))
                }
            }
            ToastPhase::Hold if active.guard && !active.hovered => {
                active.guard = false;
                let rest =
                    hold_for(TOAST_ERROR_HOLD, active.chars).saturating_sub(TOAST_ERROR_GUARD);
                if let Some(next) = self.pending_toast.take() {
                    // The error has had its time: the message that waited takes over.
                    self.show_toast_now(next);
                    return None;
                }
                Some(rest)
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
                Some(hold_for(
                    if active.toast.kind == ToastKind::Danger {
                        TOAST_ERROR_HOLD
                    } else {
                        TOAST_HOLD
                    },
                    active.chars,
                ))
            }
            ToastPhase::Hold => {
                active.phase = ToastPhase::Exit;
                Some(TOAST_EXIT)
            }
            ToastPhase::Exit => {
                // The next one has its own timer, armed when the windows redraw.
                self.clear_toast();
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
