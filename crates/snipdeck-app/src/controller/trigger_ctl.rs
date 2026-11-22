//! Typed-trigger expansion: what the controller does when the keyboard hook reports that the user
//! typed an abbreviation (SPEC slice 5b).
//!
//! The hook, the matcher and the password check run on worker threads (`typing_worker`); this
//! side decides, on the UI thread, whether the expansion may happen at all, and then goes through
//! the one injection path: Backspace per typed grapheme, then the item's normal use.
//!
//! Nothing here keeps or logs what was typed: a [`Fired`] is an item and a count.

use std::collections::BTreeMap;

use snipdeck_core::{Candidate, Matcher, UseMode, UseOutcome};
use snipdeck_platform::{
    keys::{Key, KeyEvent},
    payload::{Data, Payload},
    pipeline::{self, Clipboard, Env, Keyboard, Timing},
    route::{CopyReason, Route, ScreenRect},
};

use super::{Controller, Library, Toast, ToastKind, ToastMessage, UseResult, use_ctl::Erasing};
use crate::{
    host::{ClipboardText, TypingControl},
    typing_worker::Typed,
    usecase::Retype,
};

/// A typed expansion that was pasted and can still be taken back with Backspace (PANEL-16).
#[derive(Debug, Clone, Copy)]
pub(super) struct Expansion {
    item: snipdeck_core::ItemId,
    /// Characters the paste put into the app.
    chars: usize,
    at: snipdeck_core::Timestamp,
}

/// How long after an expansion a Backspace as the very next key takes it back.
const UNDO_WINDOW_SECS: i64 = 8;

/// What became of a typed trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedOutcome {
    /// The typed code was erased and the item used as for any use (it may have opened the fill
    /// form).
    Expanded(UseResult),
    /// Nothing happened and nothing was typed into the app.
    Skipped,
}

/// Why the last typed trigger did not expand (Troubleshoot names it, so a trigger that "sometimes does not
/// work" has an answer). Nothing here is typed text: only the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The focused field is a password box, or Windows did not say it is not one in time.
    PasswordField,
    /// The focus moved to another window or field between the last key and the expansion.
    FocusMoved,
    /// A Snipdeck window is in front.
    SnipdeckWindow,
    /// The Picker or a fill form is open.
    SnipdeckDialog,
    /// The item became Sensitive or lost the trigger since the list was sent.
    ItemChanged,
    /// The program is on the no-paste list.
    ExcludedApp,
    /// The program runs as administrator, which Snipdeck may not type into.
    ElevatedApp,
    /// Nothing that takes text has the focus.
    NothingFocused,
    /// Another key was typed before the expansion went out.
    KeyTyped,
}

impl SkipReason {
    /// The sentence Troubleshoot shows (literal English, like the other checks).
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::PasswordField => {
                "The last trigger was not expanded: the field is a password box (or could not be checked in time)."
            }
            Self::FocusMoved => {
                "The last trigger was not expanded: the focus moved to another window or field."
            }
            Self::SnipdeckWindow => {
                "The last trigger was not expanded: a Snipdeck window was in front."
            }
            Self::SnipdeckDialog => {
                "The last trigger was not expanded: the Picker or a fill form was open."
            }
            Self::ItemChanged => {
                "The last trigger was not expanded: its item changed (now Sensitive, or the code was reassigned)."
            }
            Self::ExcludedApp => {
                "The last trigger was not expanded: the program is on the \"never paste\" list."
            }
            Self::ElevatedApp => {
                "The last trigger was not expanded: the program runs as administrator."
            }
            Self::NothingFocused => {
                "The last trigger was not expanded: no text field had the focus."
            }
            Self::KeyTyped => {
                "The last trigger was not expanded: another key was typed before it went out."
            }
        }
    }
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
    /// The triggers of the live items that may expand, with each item's use count: a Sensitive item never
    /// does (a short code must not release a secret without the reveal gesture). One read of the trigger
    /// table and one pass over the cached library, not a query per trigger.
    fn expandable_triggers(
        &self,
        all: Vec<(snipdeck_core::Trigger, snipdeck_core::ItemId)>,
    ) -> Vec<(snipdeck_core::Trigger, snipdeck_core::ItemId, u64)> {
        let Ok(snapshot) = self.index_cache.snapshot(&self.store) else {
            return Vec::new();
        };
        let wanted: std::collections::HashSet<_> = all.iter().map(|(_, id)| *id).collect();
        let items: std::collections::HashMap<_, _> = snapshot
            .all
            .iter()
            .filter(|i| wanted.contains(&i.id) && !i.sensitive)
            .map(|i| (i.id, u64::from(i.use_count)))
            .collect();
        all.into_iter()
            .filter_map(|(trigger, id)| Some((trigger, id, *items.get(&id)?)))
            .collect()
    }

    /// A matcher over the triggers of the live items that may expand (see `expandable_triggers`).
    pub fn trigger_matcher(&self) -> Matcher {
        let all = self.store.triggers().unwrap_or_default();
        Matcher::new(
            self.expandable_triggers(all)
                .into_iter()
                .map(|(trigger, id, _)| (trigger, id)),
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
    fn suggest_candidates(
        &self,
        expandable: &[(snipdeck_core::Trigger, snipdeck_core::ItemId, u64)],
    ) -> Vec<Candidate> {
        if !self.settings.suggest_triggers {
            return Vec::new();
        }
        expandable
            .iter()
            .map(|(trigger, item, uses)| Candidate {
                trigger: trigger.clone(),
                item: *item,
                uses: *uses,
                sensitive: false,
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
        let lists = list_changed.then(|| {
            let expandable = self.expandable_triggers(all.clone());
            let matcher = Matcher::new(expandable.iter().map(|(t, id, _)| (t.clone(), *id)));
            (matcher, self.suggest_candidates(&expandable))
        });
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
    fn typed_allowed(&self, enabled: bool, typed: &Typed) -> Result<(), Option<SkipReason>> {
        let fired = &typed.fired;
        if !enabled {
            // The feature is off: nothing was expected to happen, so there is nothing to explain.
            return Err(None);
        }
        if typed.password {
            return Err(Some(SkipReason::PasswordField));
        }
        if !typed.same_window() {
            return Err(Some(SkipReason::FocusMoved));
        }
        if self.own_window_active {
            return Err(Some(SkipReason::SnipdeckWindow));
        }
        if self.picker.is_some() || self.fill.is_some() {
            return Err(Some(SkipReason::SnipdeckDialog));
        }
        // The matcher may be a moment old: the code must still be this item's, on a live item.
        let current = self.store.trigger_holder(&fired.trigger).ok().flatten() == Some(fired.item);
        if current && !self.store.item(fired.item).is_ok_and(|i| i.sensitive) {
            Ok(())
        } else {
            Err(Some(SkipReason::ItemChanged))
        }
    }

    /// Why the last typed trigger that should have expanded did not, if one did not since the app
    /// started (Troubleshoot shows it).
    #[must_use]
    pub const fn last_skipped_trigger(&self) -> Option<SkipReason> {
        self.last_skip
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
        if let Err(reason) = self.typed_allowed(enabled, typed) {
            self.last_skip = reason.or(self.last_skip);
            return TypedOutcome::Skipped;
        }
        let fired = &typed.fired;
        // The Target is whatever has the focus now.
        self.on_target_changed();
        let route = self.model.route();
        let reason = match route {
            Route::Paste { .. } if !typed.still_current() => Some(SkipReason::KeyTyped),
            Route::Paste { .. } if !typed.same_window() => Some(SkipReason::FocusMoved),
            Route::Paste { .. } => None,
            Route::Copy(reason) => Some(match reason {
                CopyReason::Excluded { .. } => SkipReason::ExcludedApp,
                CopyReason::Elevated { .. } => SkipReason::ElevatedApp,
                CopyReason::NothingFocused { .. } | CopyReason::NoTarget => {
                    SkipReason::NothingFocused
                }
            }),
        };
        if let Some(reason) = reason {
            self.last_skip = Some(reason);
            return TypedOutcome::Skipped;
        }
        // The Backspaces go out with the paste key, once the item can be pasted.
        let fresh = || typed.still_current();
        let erase = Erasing {
            chars: fired.chars,
            app: self.model.target().map(|t| t.app.clone()),
            window: self.model.target().map_or(0, |t| t.window),
            fresh: Some(&fresh),
            retype: Retype {
                shape: fired.shape,
                delimiter: fired.delimiter,
            },
        };
        self.last_expansion = None;
        match self.run_use_erasing(
            fired.item,
            UseMode::Paste,
            BTreeMap::new(),
            None,
            Some(erase),
        ) {
            UseResult::Cancelled => {
                self.last_skip = Some(SkipReason::KeyTyped);
                TypedOutcome::Skipped
            }
            result => {
                // Only a plain paste of known length can be taken back.
                if matches!(
                    result,
                    UseResult::Done(UseOutcome::Pasted | UseOutcome::PastedNotRestored)
                ) && let Some(chars) = self.last_use_chars.filter(|n| *n > 0)
                {
                    self.last_expansion = Some(Expansion {
                        item: fired.item,
                        chars,
                        at: self.clock.now(),
                    });
                }
                TypedOutcome::Expanded(result)
            }
        }
    }

    /// Backspace was the very next key after the expansion of `typed`: the expansion goes and the
    /// typed code comes back (Espanso, `TextExpander` and Beeftext do the same). The user's own
    /// Backspace has erased one character already. Only for the last expansion, within
    /// 8 seconds, in the window it went into, with no key typed since. Returns whether
    /// the code was typed back. **Unverified on Windows** (S1: the key counts are the unknown).
    pub fn undo_expansion(&mut self, typed: &Typed) -> bool {
        let Some(last) = self.last_expansion.take() else {
            return false;
        };
        let fired = &typed.fired;
        let recent = self.clock.now().duration_since(last.at).as_secs().abs() <= UNDO_WINDOW_SECS;
        if !self.settings.expand_triggers
            || last.item != fired.item
            || !recent
            || typed.password
            || !typed.same_window()
            || !typed.still_current()
            || self.own_window_active
        {
            return false;
        }
        self.on_target_changed();
        let route = self.model.route();
        let Route::Paste { .. } = route else {
            return false;
        };
        let (app, window) = self
            .model
            .target()
            .map_or((None, 0), |t| (Some(t.app.clone()), t.window));
        let Some(payload) = Payload::new(vec![Data::Text(fired.trigger.as_str().to_owned())])
        else {
            return false;
        };
        let target = &self.target;
        let proceed = || {
            typed.still_current()
                && target
                    .current()
                    .is_some_and(|t| Some(t.app) == app && t.window == window)
        };
        let mut env = Env {
            clipboard: &mut self.services.clipboard,
            keyboard: &mut self.services.keyboard,
            timing: &mut self.services.timing,
        };
        // The Backspaces and the paste go out together once the clipboard holds the code.
        pipeline::use_item_erasing(
            &mut env,
            &payload,
            UseMode::Paste,
            &route,
            last.chars.saturating_sub(1),
            &proceed,
        )
        .is_ok()
    }

    /// A trigger is being typed: whether a hint with the item's title stands at the caret. The
    /// same gates as an expansion, and the Target must have a caret to hang the hint on (never
    /// the mouse). The hint is only a hint: nothing is typed or copied here.
    pub fn suggest_typed(&mut self, typed: &Typed) -> HintOutcome {
        if self
            .typed_allowed(self.settings.suggest_triggers, typed)
            .is_err()
            || !typed.still_current()
        {
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
