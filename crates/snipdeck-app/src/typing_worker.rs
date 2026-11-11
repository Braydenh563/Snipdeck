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

use snipdeck_core::{Candidate, Fired, ItemId, Matcher, Trigger, prefix_of};
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

/// What the worker hands the UI thread.
#[derive(Debug)]
pub enum Out {
    /// A trigger was completed (expansion).
    Expand(Typed),
    /// A suggestion stands: show the hint. Accepting it would do what the [`Typed`] says.
    Show(Typed),
    /// The hint that was shown no longer stands.
    Hide,
    /// `Tab` was pressed while the hint showed: insert the suggested item.
    Accept(Typed),
}

/// What the worker watches for: the triggers that expand and, when suggestions are on, the
/// candidates a prefix is matched against.
pub struct TypingList {
    /// The complete triggers.
    pub matcher: Matcher,
    /// Suggestion candidates; empty when suggestions are off.
    pub candidates: Vec<Candidate>,
}

/// The suggestion the worker holds for the code being typed.
struct Hint {
    item: ItemId,
    trigger: Trigger,
    chars: usize,
    /// The hint was handed to the UI thread (not in a password box).
    shown: bool,
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

    /// Forgets what was typed after a suggestion was inserted: the text that came in is not
    /// known, so the next trigger needs a boundary of its own.
    pub fn inserted(&mut self) {
        self.matcher.reset();
        self.matcher.after_expansion(AFTER_EXPANSION);
    }

    /// The expansion `fired` reported was not sent after all (a password box, a key typed on):
    /// the code stays in the app, so the next trigger needs a boundary after ITS last character,
    /// not after an inserted text.
    pub fn not_expanded(&mut self, fired: &Fired) {
        self.matcher
            .after_expansion(fired.trigger.as_str().chars().last());
    }

    /// The trigger the typed characters are the start of, if one stands out (see [`prefix_of`]).
    #[must_use]
    pub fn suggestion(&self, candidates: &[Candidate]) -> Option<(ItemId, Trigger, usize)> {
        let (typed, at_start) = self.matcher.typed();
        let found = prefix_of(&typed, at_start, candidates)?;
        let trigger = candidates
            .iter()
            .find(|c| c.item == found.item)?
            .trigger
            .clone();
        Some((found.item, trigger, found.chars))
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
            // The text a hint inserts is not known: start over, with a boundary needed.
            TypedEvent::Tab => {
                self.inserted();
                None
            }
        }
    }
}

/// Runs until `events` is closed (the hook stopped).
///
/// `sent` is the hook's count of events; `password` says whether the focused element is a
/// password box (it may block); `deliver` gets what the UI thread should act on. If the user
/// typed anything after a trigger, whether during the password check or later (see
/// [`Typed::still_current`]), the expansion is dropped: its Backspaces would erase the wrong text.
pub fn run(
    events: &Receiver<TypedEvent>,
    updates: &Receiver<TypingList>,
    sent: &Arc<AtomicU64>,
    mut password: impl FnMut() -> bool,
    mut deliver: impl FnMut(Out),
) {
    let mut typist = Typist::new(Matcher::new([]));
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut hint: Option<Hint> = None;
    let mut consumed: u64 = 0;
    loop {
        while let Ok(list) = updates.try_recv() {
            typist.set_matcher(list.matcher);
            candidates = list.candidates;
            drop_hint(&mut hint, &mut deliver);
        }
        match events.recv_timeout(POLL) {
            Ok(event) => {
                consumed += 1;
                let at = consumed;
                let counter = Arc::clone(sent);
                let fresh = move || counter.load(Ordering::SeqCst) == at;
                if event == TypedEvent::Tab {
                    accept_hint(&mut hint, fresh, &mut deliver);
                    typist.inserted();
                    continue;
                }
                let fired = typist.event(event);
                if fired.is_some() {
                    drop_hint(&mut hint, &mut deliver);
                }
                let Some(fired) = fired else {
                    follow_hint(
                        &typist,
                        &candidates,
                        &mut hint,
                        &fresh,
                        &mut password,
                        &mut deliver,
                    );
                    continue;
                };
                if !fresh() {
                    typist.not_expanded(&fired);
                    continue;
                }
                let in_password_box = password();
                if in_password_box || !fresh() {
                    typist.not_expanded(&fired);
                }
                if fresh() {
                    deliver(Out::Expand(Typed::new(fired, in_password_box, fresh)));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    // The hook is gone: a hint still on screen must come down.
    drop_hint(&mut hint, &mut deliver);
}

/// `Tab` on the hint: hands the suggestion over for insertion. Without a shown hint nothing
/// happens (the hook only reports a `Tab` while it was told a hint is up).
fn accept_hint(
    hint: &mut Option<Hint>,
    fresh: impl Fn() -> bool + Send + Sync + 'static,
    deliver: &mut impl FnMut(Out),
) {
    if let Some(h) = hint.take().filter(|h| h.shown) {
        let fired = Fired {
            item: h.item,
            trigger: h.trigger,
            chars: h.chars,
        };
        deliver(Out::Accept(Typed::new(fired, false, fresh)));
    }
}

/// Takes the hint down, telling the UI thread if it was showing.
fn drop_hint(hint: &mut Option<Hint>, deliver: &mut impl FnMut(Out)) {
    if hint.take().is_some_and(|h| h.shown) {
        deliver(Out::Hide);
    }
}

/// Keeps the held suggestion in step with what was just typed: the same item's hint stays up
/// while its code is still being typed; any other key takes it down.
fn follow_hint(
    typist: &Typist,
    candidates: &[Candidate],
    hint: &mut Option<Hint>,
    fresh: &impl Fn() -> bool,
    password: &mut impl FnMut() -> bool,
    deliver: &mut impl FnMut(Out),
) {
    let Some((item, trigger, chars)) = typist.suggestion(candidates) else {
        drop_hint(hint, deliver);
        return;
    };
    if let Some(h) = hint
        .as_mut()
        .filter(|h| h.item == item && h.trigger == trigger)
    {
        h.chars = chars;
        return;
    }
    drop_hint(hint, deliver);
    // Keys queued behind this one: the next event decides, no need to probe for this one.
    if !fresh() {
        return;
    }
    let in_password_box = password();
    let shown = !in_password_box && fresh();
    if shown {
        let fired = Fired {
            item,
            trigger: trigger.clone(),
            chars,
        };
        // The hint is only for showing; accepting is decided at the `Tab`.
        deliver(Out::Show(Typed::new(fired, false, || true)));
    }
    *hint = Some(Hint {
        item,
        trigger,
        chars,
        shown,
    });
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
        utx.send(TypingList {
            matcher: matcher(code, item),
            candidates: Vec::new(),
        })
        .expect("matcher");
        let sent = Arc::clone(&keys.sent);
        drop(keys);
        let mut got = Vec::new();
        run(events, &updates, &sent, &mut password, |out| {
            if let Out::Expand(t) = out {
                got.push(t);
            }
        });
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
        // `brb` straight after `brb`: the second is not at a word start.
        let mut typist = Typist::new(matcher("brb", item));
        let mut fires = 0;
        for c in "brbbrb".chars() {
            fires += usize::from(typist.event(TypedEvent::Char(c)).is_some());
        }
        assert_eq!(fires, 1, "one expansion, not two");
        let mut typist = Typist::new(matcher("brb", item));
        let mut fires = 0;
        for c in "brb brb".chars() {
            fires += usize::from(typist.event(TypedEvent::Char(c)).is_some());
        }
        assert_eq!(fires, 2, "a space is a boundary");
    }

    #[test]
    fn an_expansion_that_did_not_go_out_leaves_the_boundary_of_the_typed_code() {
        let item = item_id(1);
        let mut typist = Typist::new(matcher("brb.", item));
        let mut fired = None;
        for c in "brb.".chars() {
            fired = typist.event(TypedEvent::Char(c)).or(fired);
        }
        let fired = fired.expect("fires");
        assert!(
            !typist.matcher.typed().1,
            "an expansion ends in unknown text"
        );
        typist.not_expanded(&fired);
        assert!(
            typist.matcher.typed().1,
            "the code stays, and it ended on punctuation"
        );
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

    /// What the worker hands over for `events`, one at a time with nothing queued behind.
    fn hint_run(candidates: &[Candidate], password: bool, events: &[TypedEvent]) -> Vec<String> {
        let mut typist = Typist::new(Matcher::new([]));
        let mut hint = None;
        let mut outs = Vec::new();
        for &event in events {
            let mut deliver = |out: Out| {
                outs.push(match out {
                    Out::Show(t) => format!("show {}", t.fired.chars),
                    Out::Hide => "hide".to_owned(),
                    Out::Accept(t) => format!("accept {}", t.fired.chars),
                    Out::Expand(_) => "expand".to_owned(),
                });
            };
            if event == TypedEvent::Tab {
                accept_hint(&mut hint, || true, &mut deliver);
                typist.inserted();
            } else if typist.event(event).is_none() {
                follow_hint(
                    &typist,
                    candidates,
                    &mut hint,
                    &|| true,
                    &mut || password,
                    &mut deliver,
                );
            }
        }
        outs
    }

    fn candidate(code: &str, n: u8, uses: u64, sensitive: bool) -> Candidate {
        Candidate {
            trigger: Trigger::new(code).expect("trigger"),
            item: item_id(n),
            uses,
            sensitive,
        }
    }

    fn chars(text: &str) -> Vec<TypedEvent> {
        text.chars().map(TypedEvent::Char).collect()
    }

    #[test]
    fn two_characters_of_a_trigger_show_one_hint_and_tab_hands_it_over() {
        let cands = [
            candidate(";sig", 1, 2, false),
            candidate(";sit", 2, 9, false),
        ];
        let mut events = chars("hi ;s");
        events.push(TypedEvent::Tab);
        assert_eq!(
            hint_run(&cands, false, &events),
            ["show 2", "accept 2"],
            "the most used one, once"
        );
        // A third character of the same code keeps the hint and the count follows.
        let mut events = chars(";si");
        events.push(TypedEvent::Tab);
        assert_eq!(
            hint_run(&cands[..1], false, &events),
            ["show 2", "accept 3"]
        );
    }

    #[test]
    fn other_keys_take_the_hint_down_and_a_tab_without_one_does_nothing() {
        let cands = [candidate(";sig", 1, 0, false)];
        for key in [
            TypedEvent::Reset,
            TypedEvent::Backspace,
            TypedEvent::Char('x'),
        ] {
            let mut events = chars(";s");
            events.push(key);
            assert_eq!(
                hint_run(&cands, false, &events),
                ["show 2", "hide"],
                "{key:?}"
            );
        }
        let mut events = chars(";s");
        events.extend([TypedEvent::Reset, TypedEvent::Tab]);
        assert_eq!(hint_run(&cands, false, &events), ["show 2", "hide"]);
    }

    #[test]
    fn a_password_box_gets_no_hint_and_no_accept() {
        let cands = [candidate(";sig", 1, 0, false)];
        let mut events = chars(";s");
        events.push(TypedEvent::Tab);
        assert_eq!(hint_run(&cands, true, &events), Vec::<String>::new());
    }

    #[test]
    fn titles_and_one_character_make_no_hint() {
        let cands = [candidate(";sig", 1, 0, false)];
        assert_eq!(hint_run(&cands, false, &chars(";")), Vec::<String>::new());
        assert_eq!(hint_run(&cands, false, &chars("sig")), Vec::<String>::new());
    }
}
