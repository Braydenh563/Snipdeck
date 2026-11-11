//! The typed-trigger worker: turns the hook's key events into "the user typed this trigger".
//!
//! It runs on its own thread, never the UI thread and never the hook thread: it owns the
//! [`Matcher`] and makes the password-box check, which can block for half a second. Typed
//! characters live only in the matcher's 32-character buffer; nothing here logs or stores them.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{Receiver, RecvTimeoutError},
};
use std::time::Duration;

use snipdeck_core::{Fired, Matcher};
use snipdeck_platform::typing::TypedEvent;

/// How often the worker looks for a new trigger list while no key arrives.
const POLL: Duration = Duration::from_millis(100);

/// What follows an expansion, as far as the matcher is told: the text ends in a letter, so the
/// next trigger needs a boundary of its own (typing `brb` twice must not expand twice). The
/// worker does not know the item's text; a text that ends in a boundary costs one extra
/// boundary character, never a wrong expansion.
const AFTER_EXPANSION: Option<char> = Some('a');

/// A completed trigger on its way to the UI thread.
pub struct Typed {
    /// Which item and how many characters were typed.
    pub fired: Fired,
    /// The focused element is a password box (or could not be told).
    pub password: bool,
    fresh: Box<dyn Fn() -> bool + Send + Sync>,
}

impl Typed {
    /// A trigger that is current as long as `fresh` says so.
    #[must_use]
    pub fn new(
        fired: Fired,
        password: bool,
        fresh: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            fired,
            password,
            fresh: Box::new(fresh),
        }
    }

    /// No key has been pressed since the trigger was completed. Ask it right before the
    /// Backspaces go out: after anything else was typed they would erase the wrong text.
    #[must_use]
    pub fn still_current(&self) -> bool {
        (self.fresh)()
    }
}

impl std::fmt::Debug for Typed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typed")
            .field("item", &self.fired.item)
            .field("chars", &self.fired.chars)
            .field("password", &self.password)
            .finish_non_exhaustive()
    }
}

/// Feeds key events to a matcher.
pub struct Typist {
    matcher: Matcher,
}

// What was typed is private: nothing of the matcher's state is shown.
impl std::fmt::Debug for Typist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typist").finish_non_exhaustive()
    }
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
            TypedEvent::Char(c) => {
                let fired = self.matcher.feed(c);
                if fired.is_some() {
                    self.matcher.after_expansion(AFTER_EXPANSION);
                }
                fired
            }
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
/// `sent` is the hook's count of events; `password` says whether the focused element is a
/// password box (it may block); `deliver` gets a completed trigger. If the user typed anything
/// after the trigger, whether during the password check or later (see [`Typed::still_current`]),
/// the expansion is dropped: its Backspaces would erase the wrong text.
pub fn run(
    events: &Receiver<TypedEvent>,
    updates: &Receiver<Matcher>,
    sent: &Arc<AtomicU64>,
    mut password: impl FnMut() -> bool,
    mut deliver: impl FnMut(Typed),
) {
    let mut typist = Typist::new(Matcher::new([]));
    let mut consumed: u64 = 0;
    loop {
        while let Ok(matcher) = updates.try_recv() {
            typist.set_matcher(matcher);
        }
        match events.recv_timeout(POLL) {
            Ok(event) => {
                consumed += 1;
                let Some(fired) = typist.event(event) else {
                    continue;
                };
                let at = consumed;
                let counter = Arc::clone(sent);
                let fresh = move || counter.load(Ordering::SeqCst) == at;
                if !fresh() {
                    continue;
                }
                let in_password_box = password();
                if fresh() {
                    deliver(Typed::new(fired, in_password_box, fresh));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Sender, channel};

    use snipdeck_core::{ItemId, Trigger};

    use super::*;

    fn item_id(n: u8) -> ItemId {
        ItemId::from_uuid(uuid::Uuid::from_bytes([n; 16]))
    }

    fn matcher(code: &str, item: ItemId) -> Matcher {
        Matcher::new([(Trigger::new(code).expect("trigger"), item)])
    }

    /// The hook's side: counts an event, then sends it.
    struct Keys {
        tx: Sender<TypedEvent>,
        sent: Arc<AtomicU64>,
    }

    impl Keys {
        fn new() -> (Self, Receiver<TypedEvent>) {
            let (tx, rx) = channel();
            (
                Self {
                    tx,
                    sent: Arc::new(AtomicU64::new(0)),
                },
                rx,
            )
        }

        fn press(&self, event: TypedEvent) {
            self.sent.fetch_add(1, Ordering::SeqCst);
            self.tx.send(event).expect("send");
        }

        fn type_text(&self, text: &str) {
            for c in text.chars() {
                self.press(TypedEvent::Char(c));
            }
        }
    }

    /// Runs the worker over what was typed; returns what it delivered.
    fn deliver_all(
        keys: Keys,
        events: &Receiver<TypedEvent>,
        item: ItemId,
        code: &str,
        mut password: impl FnMut() -> bool,
    ) -> (Vec<Typed>, Arc<AtomicU64>) {
        let (utx, updates) = channel();
        utx.send(matcher(code, item)).expect("matcher");
        let sent = Arc::clone(&keys.sent);
        drop(keys);
        let mut got = Vec::new();
        run(events, &updates, &sent, &mut password, |t| got.push(t));
        (got, sent)
    }

    #[test]
    fn a_typed_trigger_is_delivered_with_the_password_answer() {
        let item = item_id(1);
        let (keys, events) = Keys::new();
        keys.type_text("hi ;sig");
        let (got, _) = deliver_all(keys, &events, item, ";sig", || true);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].fired.item, got[0].fired.chars), (item, 4));
        assert!(got[0].password);
        assert!(got[0].still_current());
    }

    #[test]
    fn typing_while_the_password_check_runs_drops_the_expansion() {
        let item = item_id(1);
        let (keys, events) = Keys::new();
        keys.type_text(";a");
        let sent = Arc::clone(&keys.sent);
        let (got, _) = deliver_all(keys, &events, item, ";a", || {
            // The user kept typing during the check.
            sent.fetch_add(1, Ordering::SeqCst);
            false
        });
        assert!(got.is_empty());
    }

    #[test]
    fn keys_queued_behind_the_last_character_drop_the_expansion() {
        let item = item_id(1);
        let (keys, events) = Keys::new();
        keys.type_text(";sig ");
        let (got, _) = deliver_all(keys, &events, item, ";sig", || false);
        assert!(got.is_empty(), "the space was already typed: {got:?}");
    }

    #[test]
    fn a_key_after_the_fire_makes_it_stale_before_the_backspaces() {
        let item = item_id(1);
        let (keys, events) = Keys::new();
        keys.type_text(";sig");
        let (got, sent) = deliver_all(keys, &events, item, ";sig", || false);
        assert!(got[0].still_current());
        // The user presses a key while the UI thread gets ready.
        sent.fetch_add(1, Ordering::SeqCst);
        assert!(!got[0].still_current());
    }

    #[test]
    fn the_same_trigger_twice_needs_a_boundary_between() {
        let item = item_id(1);
        let (keys, events) = Keys::new();
        // `brb` straight after `brb`: the second is not at a word start.
        keys.type_text("brbbrb");
        let (got, _) = deliver_all(keys, &events, item, "brb", || false);
        assert_eq!(got.len(), 1, "one expansion, not two");

        let mut typist = Typist::new(matcher("brb", item));
        let mut fires = 0;
        for c in "brb brb".chars() {
            fires += usize::from(typist.event(TypedEvent::Char(c)).is_some());
        }
        assert_eq!(fires, 2, "a space is a boundary");
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

    #[test]
    fn debug_shows_no_typed_text() {
        let mut typist = Typist::new(matcher(";sig", item_id(1)));
        for c in "hunter2".chars() {
            typist.event(TypedEvent::Char(c));
        }
        assert!(!format!("{typist:?}").contains("hunter"));
    }
}
