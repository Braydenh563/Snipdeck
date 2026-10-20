//! The toast timeline: showing, Undo, hover, ticks and the reports that end in a toast.

use std::time::Duration;

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};
use snipdeck_store::Trashable;

use super::{
    ActiveToast, Controller, Effect, Library, TOAST_EXIT, TOAST_HOLD, TOAST_HOVER_RECHECK,
    TOAST_HOVER_RECHECKS, TOAST_UNDO_HOLD, ToastPhase, Undo, neutral,
};
use crate::{
    host::ClipboardText,
    toast::{Toast, ToastKind, ToastMessage},
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    // ---- Toasts ------------------------------------------------------------------------------

    /// Shows `toast`, replacing the one on screen. The host then calls
    /// [`toast_tick`](Self::toast_tick) after [`TOAST_ENTER`](crate::controller::TOAST_ENTER) and follows the delays it returns.
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
            adds: false,
            edit: None,
        });
    }

    /// Shows `toast` ("Saved to …") for the new item `id`, with an Undo button that moves it to
    /// the Trash and an Edit button that opens it in the editor.
    pub fn show_saved_toast(&mut self, toast: Toast, id: snipdeck_core::ItemId) {
        self.show_toast(toast);
        self.undo = Some(Undo {
            seq: self.toast_seq,
            targets: vec![snipdeck_store::Trashable::Item(id)],
            adds: true,
            edit: Some(id),
        });
    }

    /// The toast's Edit button: opens the editor on the item the toast is about and closes the toast.
    pub fn toast_edit(&mut self) -> Vec<Effect> {
        let Some(undo) = self.undo.take() else {
            return Vec::new();
        };
        if self.toast.as_ref().is_none_or(|t| t.seq != undo.seq) {
            return Vec::new();
        }
        self.toast = None;
        undo.edit.map(Effect::EditItem).into_iter().collect()
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
