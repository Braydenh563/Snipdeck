//! The Recent inbox's half of the [`Controller`] (SPEC 5.6): what a clipboard change does, the
//! entries the Panel and the Main window list, deleting, clearing and promoting to a list.
//!
//! Slint-free like the rest of the controller. The policy is [`crate::recent`]; this file adds
//! the clock, the settings and the toasts. Nothing here logs or traces clipboard text.

use snipdeck_core::ListId;
use snipdeck_platform::{
    payload::{Data, Payload},
    pipeline::{Clipboard, Keyboard, Timing},
};
use snipdeck_store::RecentEntry;

use super::{Controller, Library, Toast, ToastKind, ToastMessage};
use crate::{
    capture::{self, Captured},
    host::ClipboardText,
    recent::{
        self, ClipboardObservation, RecentCapture, RecentRowView, RecentSettings, RecentView,
        SaveTarget, row_view,
    },
};

/// What the tray menu is told: (clipboard history is on, collecting is paused).
pub(super) type HistorySink = Box<dyn Fn(bool, bool)>;

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// The policy's settings now.
    fn recent_settings(&self) -> RecentSettings {
        RecentSettings::from_settings(&self.settings, self.session.history_paused)
    }

    /// The Recent inbox is on in the settings.
    #[must_use]
    pub fn recent_enabled(&self) -> bool {
        self.settings.recent_history
    }

    /// "Pause history" is on (tray; lasts until Snipdeck quits).
    #[must_use]
    pub fn recent_paused(&self) -> bool {
        self.session.history_paused
    }

    /// Pauses or resumes collecting. Not saved: a restart resumes.
    pub fn set_recent_paused(&mut self, paused: bool) {
        self.session.history_paused = paused;
        self.sync_recent_gate();
    }

    /// The switch of the platform listener. The listener reads nothing while it is closed.
    pub fn set_recent_gate(&mut self, gate: snipdeck_platform::clipboard_watch::WatchGate) {
        self.recent_gate = Some(gate);
        self.sync_recent_gate();
    }

    /// Where the tray menu hears whether "Pause history" is offered and checked. Told at once,
    /// then after every change to either.
    pub fn set_tray_history_sink(&mut self, sink: impl Fn(bool, bool) + 'static) {
        self.tray_history = Some(Box::new(sink));
        self.sync_recent_gate();
    }

    /// Opens the gate only for an opted-in, unpaused inbox.
    pub(super) fn sync_recent_gate(&self) {
        if let Some(sink) = &self.tray_history {
            sink(self.settings.recent_history, self.session.history_paused);
        }
        if let Some(gate) = &self.recent_gate {
            gate.set(self.recent_settings().collecting());
        }
    }

    /// The clipboard changed (the listener's event). Stores the text when the policy allows;
    /// a failure is dropped silently: the inbox is a convenience and must never interrupt.
    pub fn on_clipboard_change(&mut self, observation: &ClipboardObservation) {
        let settings = self.recent_settings();
        let now = self.clock.now();
        let _ = RecentCapture::consider(&mut self.store, observation, &settings, now);
    }

    /// Applies the retention limits (start-up).
    pub fn prune_recent(&mut self) {
        let settings = self.recent_settings();
        let now = self.clock.now();
        let _ = RecentCapture::prune(&mut self.store, &settings, now);
    }

    /// The entries, newest first. Empty when the inbox is off, even if old entries remain, so a
    /// switched-off inbox shows nothing.
    #[must_use]
    pub fn recent_entries(&self) -> Vec<RecentEntry> {
        if !self.settings.recent_history {
            return Vec::new();
        }
        self.store.recent().unwrap_or_default()
    }

    /// The id of the entry on row `index` of the list the Panel shows (the search filters it).
    fn recent_id_at(&self, index: usize) -> Option<i64> {
        self.recent_view()?.rows.get(index).map(|r| r.id)
    }

    /// [`recent_copy`](Self::recent_copy) for row `index` of the Panel's Recent list.
    pub fn recent_copy_at(&mut self, index: usize) {
        if let Some(id) = self.recent_id_at(index) {
            self.recent_copy(id);
        }
    }

    /// [`recent_delete`](Self::recent_delete) for row `index` of the Panel's Recent list.
    pub fn recent_delete_at(&mut self, index: usize) {
        if let Some(id) = self.recent_id_at(index) {
            self.recent_delete(id);
        }
    }

    /// "Save to…": saves row `index` into the `target`th list of [`RecentView::targets`].
    pub fn recent_promote_at(&mut self, index: usize, target: usize) {
        let (Some(id), Some(view)) = (self.recent_id_at(index), self.recent_view()) else {
            return;
        };
        if let Some(to) = view.targets.get(target) {
            self.recent_promote(id, to.id);
        }
    }

    /// What the Panel shows for the inbox; `None` while it is off.
    pub(super) fn recent_view(&self) -> Option<RecentView> {
        if !self.settings.recent_history {
            return None;
        }
        let now = self.clock.now();
        Some(RecentView {
            open: self.session.recent_open,
            paused: self.session.history_paused,
            rows: self
                .recent_entries()
                .iter()
                .filter(|e| matches_query(&e.text, self.model.query()))
                .map(|e| row_view(e, now))
                .collect(),
            targets: self.save_targets(),
        })
    }

    /// The lists "Save to…" offers: every list, labelled with its board's name when there are
    /// several boards.
    pub(super) fn save_targets(&self) -> Vec<SaveTarget> {
        let boards = self.store.boards().unwrap_or_default();
        let several = boards.len() > 1;
        let mut targets = Vec::new();
        for board in &boards {
            for list in self.store.lists(board.id).unwrap_or_default() {
                let label = if several {
                    format!("{} \u{203a} {}", board.name.as_str(), list.name.as_str())
                } else {
                    list.name.as_str().to_owned()
                };
                targets.push(SaveTarget {
                    id: list.id,
                    label,
                    accent: list.accent,
                });
            }
        }
        targets
    }

    /// The Main window's Recent section: the entries (newest first) that the window's search
    /// box matches, as of now. Empty while the inbox is off.
    pub(super) fn main_recent_rows(&self) -> Vec<RecentRowView> {
        let now = self.clock.now();
        self.main_recent_entries()
            .iter()
            .map(|e| row_view(e, now))
            .collect()
    }

    fn main_recent_entries(&self) -> Vec<RecentEntry> {
        let query = self.main.query();
        self.recent_entries()
            .into_iter()
            .filter(|e| matches_query(&e.text, query))
            .collect()
    }

    fn main_recent_id_at(&self, index: usize) -> Option<i64> {
        self.main_recent_entries().get(index).map(|e| e.id)
    }

    /// [`recent_copy`](Self::recent_copy) for row `index` of the Main window's Recent list.
    pub fn main_recent_copy(&mut self, index: usize) {
        if let Some(id) = self.main_recent_id_at(index) {
            self.recent_copy(id);
        }
    }

    /// [`recent_delete`](Self::recent_delete) for row `index` of the Main window's Recent list.
    pub fn main_recent_delete(&mut self, index: usize) {
        if let Some(id) = self.main_recent_id_at(index) {
            self.recent_delete(id);
        }
    }

    /// "Save to…" for row `index` of the Main window's Recent list, into the `target`th of
    /// [`super::MainScreen::recent_targets`].
    pub fn main_recent_promote(&mut self, index: usize, target: usize) {
        let targets = self.save_targets();
        if let (Some(id), Some(to)) = (self.main_recent_id_at(index), targets.get(target)) {
            self.recent_promote(id, to.id);
        }
    }

    /// The clock tab was chosen (`true`) or a list tab was (`false`).
    pub fn set_recent_open(&mut self, open: bool) {
        self.session.recent_open = open && self.settings.recent_history;
    }

    /// A click or Enter on an entry: puts its text on the clipboard. (Pasting a Recent entry
    /// straight into the Target is not in v1: "Save to…" first, or paste with Ctrl+V.)
    pub fn recent_copy(&mut self, id: i64) {
        let Some(entry) = self.recent_entries().into_iter().find(|e| e.id == id) else {
            return;
        };
        let Some(payload) = Payload::new(vec![Data::Text(entry.text)]) else {
            self.show_toast(failed());
            return;
        };
        let toast = match self.services.clipboard.write_persistent(&payload) {
            Ok(()) => Toast {
                kind: ToastKind::Success,
                message: ToastMessage::Copied,
            },
            Err(_) => Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::ClipboardBusy,
            },
        };
        self.show_toast(toast);
    }

    /// Deletes one entry.
    pub fn recent_delete(&mut self, id: i64) {
        if self.store.delete_recent(id).is_err() {
            self.show_toast(failed());
        }
    }

    /// Deletes every entry ("Clear recent history"); returns how many went.
    pub fn recent_clear(&mut self) -> usize {
        self.store.clear_recent().unwrap_or_else(|_| {
            self.show_toast(failed());
            0
        })
    }

    /// Saves the entry as a new item at the top of `list` ("Save to…"), and says so. The entry
    /// stays in the inbox.
    pub fn recent_promote(&mut self, id: i64, list: ListId) {
        let Some(entry) = self.recent_entries().into_iter().find(|e| e.id == id) else {
            return;
        };
        let Ok(Some(to)) = capture::default_list(&self.store, Some(list)) else {
            self.show_toast(failed());
            return;
        };
        let now = self.clock.now();
        let toast = match recent::promote(&mut self.store, &entry, &to, now) {
            Ok(Captured::Saved { list, .. }) => {
                if self.refresh_panel().is_err() {
                    self.show_toast(failed());
                }
                Toast {
                    kind: ToastKind::Success,
                    message: ToastMessage::Saved { list },
                }
            }
            Ok(Captured::Empty) => Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::NothingToSave,
            },
            Ok(Captured::TooLarge) => Toast {
                kind: ToastKind::Neutral,
                message: ToastMessage::TooLargeToSave,
            },
            Err(_) => failed(),
        };
        self.show_toast(toast);
    }
}

/// The Panel's search box narrows the Recent list too: every word of `query` must appear in
/// `text`, ignoring case.
fn matches_query(text: &str, query: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

fn failed() -> Toast {
    Toast {
        kind: ToastKind::Danger,
        message: ToastMessage::NotSaved,
    }
}
