//! Typed-trigger expansion: what the controller does when the keyboard hook reports that the user
//! typed an abbreviation (SPEC slice 5b).
//!
//! The hook, the matcher and the password check run on worker threads (`typing_worker`); this
//! side decides, on the UI thread, whether the expansion may happen at all, and then goes through
//! the one injection path: Backspace per typed grapheme, then the item's normal use.
//!
//! Nothing here keeps or logs what was typed: a [`Fired`] is an item and a count.

use std::collections::BTreeMap;

use snipdeck_core::{Fired, Matcher, UseMode};
use snipdeck_platform::{
    keys,
    pipeline::{Clipboard, Keyboard, Timing},
    route::Route,
};

use super::{Controller, Library, UseResult};
use crate::host::{ClipboardText, TypingControl};

/// What became of a typed trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedOutcome {
    /// The typed code was erased and the item used as for any use (it may have opened the fill
    /// form).
    Expanded(UseResult),
    /// Nothing happened and nothing was typed into the app.
    Skipped,
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
        self.sync_typing();
    }

    /// Whether any typing feature is on (so the keyboard hook is wanted).
    #[must_use]
    pub fn typing_wanted(&self) -> bool {
        self.settings.expand_triggers || self.settings.suggest_triggers
    }

    /// Tells the typed-trigger service whether it is wanted and which triggers exist. Call it
    /// after a settings change and after anything that changes a trigger.
    pub(super) fn sync_typing(&mut self) {
        if self.typing.is_none() {
            return;
        }
        let on = self.typing_wanted();
        let matcher = on.then(|| self.trigger_matcher());
        if let Some(typing) = self.typing.as_mut() {
            if let Some(matcher) = matcher {
                typing.set_triggers(matcher);
            }
            typing.set_enabled(on);
        }
    }

    /// The user typed a trigger: `fired` says which item and how many characters. `password` is
    /// the answer of the password-box check, made off the UI thread.
    ///
    /// Expands only when the setting is on, the focus is not a password box, Snipdeck is not in
    /// front, the item is still live and not Sensitive, and the Target would take a paste (not
    /// excluded, not elevated, something focused).
    pub fn expand_typed(&mut self, fired: Fired, password: bool) -> TypedOutcome {
        if !self.settings.expand_triggers
            || password
            || self.own_window_active
            || self.picker.is_some()
            || self.fill.is_some()
        {
            return TypedOutcome::Skipped;
        }
        // The matcher may be a moment old: the trigger must still belong to a live item.
        let live = self
            .store
            .triggers()
            .is_ok_and(|all| all.iter().any(|(_, item)| *item == fired.item));
        if !live || self.store.item(fired.item).is_ok_and(|i| i.sensitive) {
            return TypedOutcome::Skipped;
        }
        // The Target is whatever has the focus now.
        self.on_target_changed();
        if !matches!(self.model.route(), Route::Paste { .. }) {
            return TypedOutcome::Skipped;
        }
        if self
            .services
            .keyboard
            .send(&keys::backspaces(fired.chars))
            .is_err()
        {
            return TypedOutcome::Skipped;
        }
        TypedOutcome::Expanded(self.run_use(fired.item, UseMode::Paste, BTreeMap::new(), None))
    }
}
