//! Typed-trigger expansion: what the controller does when the keyboard hook reports that the user
//! typed an abbreviation (SPEC slice 5b).
//!
//! The hook, the matcher and the password check run on worker threads (`typing_worker`); this
//! side decides, on the UI thread, whether the expansion may happen at all, and then goes through
//! the one injection path: Backspace per typed grapheme, then the item's normal use.
//!
//! Nothing here keeps or logs what was typed: a [`Fired`] is an item and a count.

use std::collections::BTreeMap;

use snipdeck_core::{Matcher, UseMode};
use snipdeck_platform::{
    pipeline::{Clipboard, Keyboard, Timing},
    route::Route,
};

use super::{Controller, Library, UseResult};
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
        self.settings.expand_triggers
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
        let list_changed = on && all != self.typing_sent;
        let matcher = list_changed.then(|| self.trigger_matcher());
        if list_changed {
            self.typing_sent = all;
        }
        let enable = (settings_changed || self.typing_on != Some(on)).then_some(on);
        self.typing_on = Some(on);
        if let Some(typing) = self.typing.as_mut() {
            if let Some(matcher) = matcher {
                typing.set_triggers(matcher);
            }
            if let Some(on) = enable {
                typing.set_enabled(on);
            }
        }
    }

    /// The user typed a trigger. Expands only when the setting is on, the focus is not a
    /// password box, no Snipdeck window is in front or in use, the typed code still belongs to
    /// the item and the item is not Sensitive, the Target would take a paste (not excluded, not
    /// elevated, something focused), and nothing else was typed meanwhile.
    pub fn expand_typed(&mut self, typed: &Typed) -> TypedOutcome {
        let fired = &typed.fired;
        if !self.settings.expand_triggers
            || typed.password
            || self.own_window_active
            // The Panel is in use (a pinned one is not: the probe covers its search box).
            || (self.visible && !self.on_top)
            || self.picker.is_some()
            || self.fill.is_some()
        {
            return TypedOutcome::Skipped;
        }
        // The matcher may be a moment old: the code must still be this item's, on a live item.
        let current = self.store.trigger_holder(&fired.trigger).ok().flatten() == Some(fired.item);
        if !current || self.store.item(fired.item).is_ok_and(|i| i.sensitive) {
            return TypedOutcome::Skipped;
        }
        // The Target is whatever has the focus now.
        self.on_target_changed();
        if !matches!(self.model.route(), Route::Paste { .. }) || !typed.still_current() {
            return TypedOutcome::Skipped;
        }
        // The Backspaces go out with the paste key, once the item can be pasted.
        TypedOutcome::Expanded(self.run_use_erasing(
            fired.item,
            UseMode::Paste,
            BTreeMap::new(),
            None,
            fired.chars,
        ))
    }
}
