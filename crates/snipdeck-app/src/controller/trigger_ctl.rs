//! Typed-trigger expansion: what the controller does when the keyboard hook reports that the user
//! typed an abbreviation (SPEC slice 5b).
//!
//! The hook, the matcher and the password check run on worker threads (`typing_worker`); this
//! side decides, on the UI thread, whether the expansion may happen at all, and then goes through
//! the one injection path: Backspace per typed grapheme, then the item's normal use.
//!
//! Nothing here keeps or logs what was typed: a [`Fired`] is an item and a count.

use std::collections::BTreeMap;

use snipdeck_core::{Candidate, Matcher, UseMode};
use snipdeck_platform::{
    keys::{Key, KeyEvent},
    pipeline::{Clipboard, Keyboard, Timing},
    route::{Route, ScreenRect},
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, UseResult, use_ctl::Erasing};
use crate::{
    host::{ClipboardText, TypingControl},
    typing_worker::Typed,
};

/// What became of a typed trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedOutcome {
    /// The typed code was erased and the item used as for any use (it may have opened the fill
    /// form).
    Expanded(UseResult),
    /// Nothing happened and nothing was typed into the app.
    Skipped,
}

/// What the hint at the caret should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintOutcome {
    /// Show `title` under the caret.
    Show {
        /// The suggested item's title.
        title: String,
        /// The Target's text caret.
        caret: ScreenRect,
    },
    /// Show nothing (take down a hint that is up).
    Hide,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// A matcher over the triggers of the live items that may expand: a Sensitive item never does
    /// (a short code must not release a secret without the reveal gesture).
    pub fn trigger_matcher(&self) -> Matcher {
        let pairs = self.store.triggers().unwrap_or_default();
        Matcher::new(
            pairs
                .into_iter()
                .filter(|(_, item)| self.store.item(*item).is_ok_and(|i| !i.sensitive)),
        )
    }

    /// Hands over the typed-trigger service and tells it the current state.
    pub fn set_typing(&mut self, typing: Box<dyn TypingControl>) {
        self.typing = Some(typing);
        self.sync_typing(true);
    }

    /// Whether any typing feature is on (so the keyboard hook is wanted).
    #[must_use]
    pub fn typing_wanted(&self) -> bool {
        self.settings.expand_triggers || self.settings.suggest_triggers
    }

    /// The triggers a typed prefix may be suggested for: live, not Sensitive, with their use
    /// counts. Empty while suggestions are off.
    fn suggest_candidates(&self) -> Vec<Candidate> {
        if !self.settings.suggest_triggers {
            return Vec::new();
        }
        self.store
            .triggers()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(trigger, id)| {
                let item = self.store.item(id).ok().filter(|i| !i.sensitive)?;
                Some(Candidate {
                    trigger,
                    item: id,
                    uses: u64::from(item.use_count),
                    sensitive: false,
                })
            })
            .collect()
    }

    /// Whether hint number `id` is from a worker other than the running one.
    #[must_use]
    pub fn hint_is_stale(&self, id: u64) -> bool {
        let running = self.typing.as_ref().map_or(0, |t| t.generation());
        crate::typing_worker::generation_of(id) != running
    }

    /// `Tab` was swallowed for hint `id`'s suggestion. Inserts it if the hint is `on_screen` and
    /// the insertion still goes out; otherwise the swallowed `Tab` is given back to the app (a
    /// slightly late `Tab` beats a lost one).
    pub fn accept_hinted(&mut self, typed: &Typed, on_screen: bool) -> TypedOutcome {
        let outcome = if on_screen {
            self.accept_suggested(typed)
        } else {
            TypedOutcome::Skipped
        };
        if outcome == TypedOutcome::Skipped {
            let tab = |down| KeyEvent {
                key: Key::Tab,
                down,
            };
            let _ = self.services.keyboard.send(&[tab(true), tab(false)]);
        }
        outcome
    }

    /// Hint number `id` is on screen: the hook may take a plain `Tab` for it.
    pub fn hint_up(&mut self, id: u64) {
        if let Some(typing) = self.typing.as_mut() {
            typing.hint_up(id);
        }
    }

    /// Hint number `id` is not showing (or was taken down): the hook lets `Tab` through again.
    pub fn hint_down(&mut self, id: u64) {
        if let Some(typing) = self.typing.as_mut() {
            typing.hint_down(id);
        }
    }

    /// Tells the typed-trigger service which triggers exist (only when the list changed) and
    /// whether it is wanted (when `settings_changed`, or when that answer changed). A hook that
    /// failed to install is thus retried once per settings change, not on every refresh.
    pub(super) fn sync_typing(&mut self, settings_changed: bool) {
        if self.typing.is_none() {
            return;
        }
        let on = self.typing_wanted();
        let all = if on {
            self.store.triggers().unwrap_or_default()
        } else {
            Vec::new()
        };
        let suggest = self.settings.suggest_triggers;
        let list_changed = on && (all != self.typing_sent || suggest != self.typing_suggest);
        let lists = list_changed.then(|| (self.trigger_matcher(), self.suggest_candidates()));
        if list_changed {
            self.typing_sent = all;
            self.typing_suggest = suggest;
        }
        let enable = (settings_changed || self.typing_on != Some(on)).then_some(on);
        self.typing_on = Some(on);
        let mut failed = false;
        if let Some(typing) = self.typing.as_mut() {
            if let Some((matcher, candidates)) = lists {
                typing.set_triggers(matcher, candidates);
            }
            if let Some(on) = enable {
                failed = !typing.set_enabled(on);
            }
        }
        if failed {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::TypingUnavailable,
            });
        }
    }

    /// The checks every typed trigger or accepted suggestion passes before anything is sent:
    /// the feature `enabled`, the focus is not a password box, no Snipdeck window is in front or
    /// in use, the typed code still belongs to the item and the item is not Sensitive.
    fn typed_allowed(&self, enabled: bool, typed: &Typed) -> bool {
        let fired = &typed.fired;
        if !enabled
            || typed.password
            || !typed.same_window()
            || self.own_window_active
            || self.picker.is_some()
            || self.fill.is_some()
        {
            return false;
        }
        // The matcher may be a moment old: the code must still be this item's, on a live item.
        let current = self.store.trigger_holder(&fired.trigger).ok().flatten() == Some(fired.item);
        current && !self.store.item(fired.item).is_ok_and(|i| i.sensitive)
    }

    /// The user typed a trigger. Expands only when the setting is on and `typed_allowed` holds, the
    /// Target would take a paste (not excluded, not elevated,
    /// something focused), and nothing else was typed meanwhile.
    pub fn expand_typed(&mut self, typed: &Typed) -> TypedOutcome {
        let outcome = self.insert_typed(self.settings.expand_triggers, typed);
        if outcome == TypedOutcome::Skipped {
            // The typed code stays in the app: the worker must not treat the text as replaced.
            if let Some(typing) = self.typing.as_mut() {
                typing.not_expanded(typed.fired.clone());
            }
        }
        outcome
    }

    /// `Tab` was pressed on the hint: the same insertion as a typed trigger, gated by the
    /// suggestion setting.
    pub fn accept_suggested(&mut self, typed: &Typed) -> TypedOutcome {
        self.insert_typed(self.settings.suggest_triggers, typed)
    }

    fn insert_typed(&mut self, enabled: bool, typed: &Typed) -> TypedOutcome {
        if !self.typed_allowed(enabled, typed) {
            return TypedOutcome::Skipped;
        }
        let fired = &typed.fired;
        // The Target is whatever has the focus now.
        self.on_target_changed();
        if !matches!(self.model.route(), Route::Paste { .. })
            || !typed.still_current()
            || !typed.same_window()
        {
            return TypedOutcome::Skipped;
        }
        // The Backspaces go out with the paste key, once the item can be pasted.
        let fresh = || typed.still_current();
        let erase = Erasing {
            chars: fired.chars,
            app: self.model.target().map(|t| t.app.clone()),
            fresh: Some(&fresh),
        };
        match self.run_use_erasing(
            fired.item,
            UseMode::Paste,
            BTreeMap::new(),
            None,
            Some(erase),
        ) {
            UseResult::Cancelled => TypedOutcome::Skipped,
            result => TypedOutcome::Expanded(result),
        }
    }

    /// A trigger is being typed: whether a hint with the item's title stands at the caret. The
    /// same gates as an expansion, and the Target must have a caret to hang the hint on (never
    /// the mouse). The hint is only a hint: nothing is typed or copied here.
    pub fn suggest_typed(&mut self, typed: &Typed) -> HintOutcome {
        if !self.typed_allowed(self.settings.suggest_triggers, typed) || !typed.still_current() {
            return HintOutcome::Hide;
        }
        self.on_target_changed();
        if !matches!(self.model.route(), Route::Paste { .. }) {
            return HintOutcome::Hide;
        }
        let (Some(caret), Ok(item)) = (self.target.caret(), self.store.item(typed.fired.item))
        else {
            return HintOutcome::Hide;
        };
        HintOutcome::Show {
            title: if item.title.is_empty() {
                item.preview
            } else {
                item.title
            },
            caret,
        }
    }
}
