//! The typed-trigger worker: turns the hook's key events into "the user typed this trigger".
//!
//! It runs on its own thread, never the UI thread and never the hook thread: it owns the
//! [`Matcher`] and makes the password-box check, which can block for half a second. Typed
//! characters live only in the matcher's 32-character buffer; nothing here logs or stores them.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use snipdeck_core::{Fired, Matcher};
use snipdeck_platform::typing::TypedEvent;

/// How often the worker looks for a new trigger list while no key arrives.
const POLL: Duration = Duration::from_millis(100);

/// Feeds key events to a matcher.
#[derive(Debug)]
pub struct Typist {
    matcher: Matcher,
}

impl Typist {
    /// A typist over `matcher`.
    #[must_use]
    pub fn new(matcher: Matcher) -> Self {
        Self { matcher }
    }

    /// Swaps in a new trigger list; what was typed so far is forgotten.
    pub fn set_matcher(&mut self, matcher: Matcher) {
        self.matcher = matcher;
    }

    /// Forgets what was typed.
    pub fn reset(&mut self) {
        self.matcher.reset();
    }

    /// Takes one event; returns the trigger it completed, if any.
    pub fn event(&mut self, event: TypedEvent) -> Option<Fired> {
        match event {
            TypedEvent::Char(c) => self.matcher.feed(c),
            TypedEvent::Backspace => {
                self.matcher.backspace();
                None
            }
            TypedEvent::Reset => {
                self.matcher.reset();
                None
            }
        }
    }
}

/// Runs until `events` is closed (the hook stopped).
///
/// `password` says whether the focused element is a password box (it may block); `deliver` gets a
/// completed trigger and that answer. If the user typed anything while the answer was being
/// found, the expansion is dropped: its Backspaces would erase the wrong text.
pub fn run(
    events: &Receiver<TypedEvent>,
    updates: &Receiver<Matcher>,
    mut password: impl FnMut() -> bool,
    mut deliver: impl FnMut(Fired, bool),
) {
    let mut typist = Typist::new(Matcher::new([]));
    loop {
        while let Ok(matcher) = updates.try_recv() {
            typist.set_matcher(matcher);
        }
        match events.recv_timeout(POLL) {
            Ok(event) => {
                if let Some(fired) = typist.event(event) {
                    let in_password_box = password();
                    if events.try_recv().is_ok() {
                        // More keys arrived meanwhile: the typed code is no longer the last thing.
                        typist.reset();
                    } else {
                        deliver(fired, in_password_box);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use snipdeck_core::{ItemId, Trigger};

    use super::*;

    fn item_id(n: u8) -> ItemId {
        ItemId::from_uuid(uuid::Uuid::from_bytes([n; 16]))
    }

    fn matcher(code: &str, item: ItemId) -> Matcher {
        Matcher::new([(Trigger::new(code).expect("trigger"), item)])
    }

    fn feed(tx: &std::sync::mpsc::Sender<TypedEvent>, text: &str) {
        for c in text.chars() {
            tx.send(TypedEvent::Char(c)).expect("send");
        }
    }

    #[test]
    fn a_typed_trigger_is_delivered_with_the_password_answer() {
        let item = item_id(1);
        let (tx, events) = channel();
        let (utx, updates) = channel();
        utx.send(matcher(";sig", item)).expect("matcher");
        feed(&tx, "hi ;sig");
        drop(tx);
        let mut got = Vec::new();
        run(&events, &updates, || true, |f, p| got.push((f.item, f.chars, p)));
        // The extra-key check sees nothing after the last character, so it is delivered.
        assert_eq!(got, [(item, 4, true)]);
    }

    #[test]
    fn typing_while_the_password_check_runs_drops_the_expansion() {
        let item = item_id(1);
        let (tx, events) = channel();
        let (utx, updates) = channel();
        utx.send(matcher(";a", item)).expect("matcher");
        feed(&tx, ";a");
        let more = tx.clone();
        drop(tx);
        let mut got = 0;
        run(
            &events,
            &updates,
            || {
                // The user kept typing during the check.
                more.send(TypedEvent::Char('x')).expect("send");
                false
            },
            |_, _| got += 1,
        );
        assert_eq!(got, 0);
    }

    #[test]
    fn backspace_and_reset_edit_what_was_typed() {
        let item = item_id(1);
        let mut typist = Typist::new(matcher(";ab", item));
        for c in ";ax".chars() {
            typist.event(TypedEvent::Char(c));
        }
        typist.event(TypedEvent::Backspace);
        assert!(typist.event(TypedEvent::Char('b')).is_some());
        for c in ";a".chars() {
            typist.event(TypedEvent::Char(c));
        }
        typist.event(TypedEvent::Reset);
        assert!(typist.event(TypedEvent::Char('b')).is_none());
    }
}
