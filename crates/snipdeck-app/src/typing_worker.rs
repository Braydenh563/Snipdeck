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

/// Polls without any key before a shown hint comes down by itself (5 s): the user went on
/// elsewhere, for example into another field of the same window.
const HINT_IDLE_POLLS: u32 = 50;

/// The number the next shown hint gets (0 is "no hint": the hook's flag value for "off").
static NEXT_HINT: AtomicU64 = AtomicU64::new(1);

/// What follows an expansion, as far as the matcher is told: the text ends in a letter, so the
/// next trigger needs a boundary of its own (typing `brb` twice must not expand twice). The
/// worker does not know the item's text; a text that ends in a boundary costs one extra
/// boundary character, never a wrong expansion.
const AFTER_EXPANSION: Option<char> = Some('a');

/// Reads the foreground window's handle.
pub type Foreground = Arc<dyn Fn() -> isize + Send + Sync>;

/// A completed trigger on its way to the UI thread.
pub struct Typed {
    /// Which item and how many characters were typed.
    pub fired: Fired,
    /// The focused element is a password box (or could not be told).
    pub password: bool,
    fresh: Box<dyn Fn() -> bool + Send + Sync>,
    window: Option<(isize, Foreground)>,
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
            window: None,
        }
    }

    /// The trigger was typed in window `at`; `now` reads the foreground window.
    #[must_use]
    pub fn in_window(mut self, at: isize, now: Foreground) -> Self {
        self.window = Some((at, now));
        self
    }

    /// The foreground window is still the one the trigger was typed in (a window made for a
    /// test, or without a handle, is always the same).
    #[must_use]
    pub fn same_window(&self) -> bool {
        self.window.as_ref().is_none_or(|(at, now)| now() == *at)
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
    /// A suggestion stands: show hint number `.1`. Accepting it would do what the [`Typed`] says.
    Show(Typed, u64),
    /// Hint number `.0` no longer stands (a hide for an older hint changes nothing).
    Hide(u64),
    /// `Tab` was pressed while the hint showed: insert the suggested item.
    Accept(Typed),
    /// The hook swallowed a `Tab` for a hint that was gone by then (or in a password box): the
    /// app should get it after all.
    PassTab,
}

/// What the UI thread tells the worker.
pub enum Update {
    /// A new trigger list.
    List(TypingList),
    /// The expansion for `fired` was not sent (a Snipdeck window, an excluded app, ...): the
    /// code stays in the app, so the next trigger needs a boundary after its last character.
    NotExpanded(Fired),
}

/// What the worker shares with the hook and the platform.
pub struct Env {
    /// The hook's count of events (see [`Typed::still_current`]).
    pub sent: Arc<AtomicU64>,
    /// Events the hook counted but could not send: the worker takes them in at the next reset.
    pub lost: Arc<AtomicU64>,
    /// The hook's swallow flag: the number of the hint a plain `Tab` belongs to, 0 for none. Only
    /// the worker sets it (when a hint is handed over) and clears it (when it comes down).
    pub tab: Arc<AtomicU64>,
    /// The foreground window.
    pub foreground: Foreground,
    /// How often the worker looks around while no key arrives.
    pub poll: Duration,
    /// Polls without a key before a shown hint comes down.
    pub idle_polls: u32,
}

impl Env {
    /// The shared state with the normal timings.
    #[must_use]
    pub fn new(
        sent: Arc<AtomicU64>,
        lost: Arc<AtomicU64>,
        tab: Arc<AtomicU64>,
        foreground: Foreground,
    ) -> Self {
        Self {
            sent,
            lost,
            tab,
            foreground,
            poll: POLL,
            idle_polls: HINT_IDLE_POLLS,
        }
    }
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
    id: u64,
    window: isize,
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
/// `env` is what the worker shares with the hook; `password` says whether the focused element
/// is a password box (it may block); `deliver` gets what the UI thread should act on. If the
/// user typed anything after a trigger, whether during the password check or later (see
/// [`Typed::still_current`]), the expansion is dropped: its Backspaces would erase the wrong text.
pub fn run(
    events: &Receiver<TypedEvent>,
    updates: &Receiver<Update>,
    env: &Env,
    mut password: impl FnMut() -> bool,
    mut deliver: impl FnMut(Out),
) {
    let mut typist = Typist::new(Matcher::new([]));
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut hint: Option<Hint> = None;
    let mut consumed: u64 = 0;
    let mut idle: u32 = 0;
    loop {
        while let Ok(update) = updates.try_recv() {
            match update {
                Update::List(list) => {
                    typist.set_matcher(list.matcher);
                    candidates = list.candidates;
                    drop_hint(&mut hint, env, &mut deliver);
                }
                Update::NotExpanded(fired) => typist.not_expanded(&fired),
            }
        }
        match events.recv_timeout(env.poll) {
            Ok(event) => {
                idle = 0;
                consumed += 1;
                if event == TypedEvent::Reset {
                    // Keys the hook counted but could not send came before this reset.
                    consumed += env.lost.swap(0, Ordering::SeqCst);
                }
                let at = consumed;
                let counter = Arc::clone(&env.sent);
                let fresh = move || counter.load(Ordering::SeqCst) == at;
                if event == TypedEvent::Tab {
                    accept_hint(&mut hint, env, fresh, &mut password, &mut deliver);
                    typist.inserted();
                    continue;
                }
                let fired = typist.event(event);
                if fired.is_some() {
                    drop_hint(&mut hint, env, &mut deliver);
                }
                let Some(fired) = fired else {
                    follow_hint(
                        &typist,
                        &candidates,
                        &mut hint,
                        (env, &fresh),
                        &mut password,
                        &mut deliver,
                    );
                    continue;
                };
                if !fresh() {
                    typist.not_expanded(&fired);
                    continue;
                }
                let window = (env.foreground)();
                let in_password_box = password();
                if in_password_box || !fresh() {
                    typist.not_expanded(&fired);
                }
                if fresh() {
                    let typed = Typed::new(fired, in_password_box, fresh)
                        .in_window(window, Arc::clone(&env.foreground));
                    deliver(Out::Expand(typed));
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                idle += 1;
                if idle >= env.idle_polls && hint.is_some() {
                    // Nothing typed for a while: the hint is stale (the user may have clicked
                    // into another field). What was typed is forgotten with it.
                    drop_hint(&mut hint, env, &mut deliver);
                    typist.reset();
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    // The hook is gone: a hint still on screen must come down.
    drop_hint(&mut hint, env, &mut deliver);
}

/// `Tab` on the hint: hands the suggestion over for insertion, unless the focus is a password
/// box by now (checked again here: the focus may have moved since the hint went up). A `Tab`
/// with no shown hint behind it (the hook swallowed it just before the hint went down) is given
/// back to the app.
fn accept_hint(
    hint: &mut Option<Hint>,
    env: &Env,
    fresh: impl Fn() -> bool + Send + Sync + 'static,
    password: &mut impl FnMut() -> bool,
    deliver: &mut impl FnMut(Out),
) {
    let Some(h) = hint.take().filter(|h| h.shown) else {
        deliver(Out::PassTab);
        return;
    };
    let _ = env
        .tab
        .compare_exchange(h.id, 0, Ordering::SeqCst, Ordering::SeqCst);
    if password() {
        deliver(Out::Hide(h.id));
        deliver(Out::PassTab);
        return;
    }
    let fired = Fired {
        item: h.item,
        trigger: h.trigger,
        chars: h.chars,
    };
    deliver(Out::Accept(
        Typed::new(fired, false, fresh).in_window(h.window, Arc::clone(&env.foreground)),
    ));
}

/// Takes the hint down, telling the UI thread if it was showing. The hook gets its `Tab` back
/// before anything else happens.
fn drop_hint(hint: &mut Option<Hint>, env: &Env, deliver: &mut impl FnMut(Out)) {
    if let Some(h) = hint.take().filter(|h| h.shown) {
        let _ = env
            .tab
            .compare_exchange(h.id, 0, Ordering::SeqCst, Ordering::SeqCst);
        deliver(Out::Hide(h.id));
    }
}

/// Keeps the held suggestion in step with what was just typed: the same item's hint stays up
/// while its code is still being typed; any other key takes it down.
fn follow_hint(
    typist: &Typist,
    candidates: &[Candidate],
    hint: &mut Option<Hint>,
    (env, fresh): (&Env, &impl Fn() -> bool),
    password: &mut impl FnMut() -> bool,
    deliver: &mut impl FnMut(Out),
) {
    let Some((item, trigger, chars)) = typist.suggestion(candidates) else {
        drop_hint(hint, env, deliver);
        return;
    };
    if let Some(h) = hint
        .as_mut()
        .filter(|h| h.item == item && h.trigger == trigger)
    {
        h.chars = chars;
        return;
    }
    drop_hint(hint, env, deliver);
    // Keys queued behind this one: the next event decides, no need to probe for this one.
    if !fresh() {
        return;
    }
    let window = (env.foreground)();
    let in_password_box = password();
    let shown = !in_password_box && fresh();
    let id = NEXT_HINT.fetch_add(1, Ordering::SeqCst);
    if shown {
        let fired = Fired {
            item,
            trigger: trigger.clone(),
            chars,
        };
        // The hint is only for showing; accepting is decided at the `Tab`.
        let typed =
            Typed::new(fired, false, || true).in_window(window, Arc::clone(&env.foreground));
        deliver(Out::Show(typed, id));
        // The hook swallows a plain `Tab` from here until this hint comes down.
        env.tab.store(id, Ordering::SeqCst);
    }
    *hint = Some(Hint {
        id,
        window,
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

    /// The shared state for a test: instant polls, a hint that goes after 3 idle polls, and a
    /// foreground window the test can change.
    fn test_env(sent: &Arc<AtomicU64>) -> (Env, Arc<std::sync::atomic::AtomicIsize>) {
        let window = Arc::new(std::sync::atomic::AtomicIsize::new(7));
        let seen = Arc::clone(&window);
        let mut env = Env::new(
            Arc::clone(sent),
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicU64::new(0)),
            Arc::new(move || seen.load(Ordering::SeqCst)),
        );
        env.poll = Duration::from_millis(1);
        env.idle_polls = 3;
        (env, window)
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
        utx.send(Update::List(TypingList {
            matcher: matcher(code, item),
            candidates: Vec::new(),
        }))
        .expect("matcher");
        let sent = Arc::clone(&keys.sent);
        drop(keys);
        let mut got = Vec::new();
        let (env, _) = test_env(&sent);
        run(events, &updates, &env, &mut password, |out| {
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
    /// `password` answers each probe in turn (the last answer repeats).
    fn hint_run_with(
        candidates: &[Candidate],
        password: &[bool],
        events: &[TypedEvent],
    ) -> (Vec<String>, Env) {
        let (env, _) = test_env(&Arc::new(AtomicU64::new(0)));
        let mut typist = Typist::new(Matcher::new([]));
        let mut hint = None;
        let mut outs = Vec::new();
        let mut probes = 0;
        let mut ask = || {
            let answer = password[probes.min(password.len() - 1)];
            probes += 1;
            answer
        };
        for &event in events {
            let mut deliver = |out: Out| {
                outs.push(match out {
                    Out::Show(t, _) => format!("show {}", t.fired.chars),
                    Out::Hide(_) => "hide".to_owned(),
                    Out::Accept(t) => format!("accept {}", t.fired.chars),
                    Out::Expand(_) => "expand".to_owned(),
                    Out::PassTab => "pass-tab".to_owned(),
                });
            };
            if event == TypedEvent::Tab {
                accept_hint(&mut hint, &env, || true, &mut ask, &mut deliver);
                typist.inserted();
            } else if typist.event(event).is_none() {
                follow_hint(
                    &typist,
                    candidates,
                    &mut hint,
                    (&env, &|| true),
                    &mut ask,
                    &mut deliver,
                );
            }
        }
        (outs, env)
    }

    fn hint_run(candidates: &[Candidate], password: bool, events: &[TypedEvent]) -> Vec<String> {
        hint_run_with(candidates, &[password], events).0
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
    fn other_keys_take_the_hint_down_and_a_tab_without_one_is_passed_on() {
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
        assert_eq!(
            hint_run(&cands, false, &events),
            ["show 2", "hide", "pass-tab"],
            "a Tab the hook swallowed for a hint that was gone goes back to the app"
        );
    }

    #[test]
    fn a_password_box_gets_no_hint_and_no_accept() {
        let cands = [candidate(";sig", 1, 0, false)];
        let mut events = chars(";s");
        events.push(TypedEvent::Tab);
        assert_eq!(hint_run(&cands, true, &events), ["pass-tab"]);
    }

    #[test]
    fn titles_and_one_character_make_no_hint() {
        let cands = [candidate(";sig", 1, 0, false)];
        assert_eq!(hint_run(&cands, false, &chars(";")), Vec::<String>::new());
        assert_eq!(hint_run(&cands, false, &chars("sig")), Vec::<String>::new());
    }

    #[test]
    fn the_worker_owns_the_tab_flag_and_clears_it_before_anything_else_can_see_it() {
        let cands = [candidate(";sig", 1, 0, false)];
        let (outs, env) = hint_run_with(&cands, &[false], &chars(";s"));
        assert_eq!(outs, ["show 2"]);
        assert_ne!(
            env.tab.load(Ordering::SeqCst),
            0,
            "armed with the hint's number"
        );
        for key in [
            TypedEvent::Backspace,
            TypedEvent::Char('x'),
            TypedEvent::Reset,
        ] {
            let mut events = chars(";s");
            events.push(key);
            let (_, env) = hint_run_with(&cands, &[false], &events);
            assert_eq!(
                env.tab.load(Ordering::SeqCst),
                0,
                "{key:?} takes the flag down"
            );
        }
        let mut events = chars(";s");
        events.push(TypedEvent::Tab);
        let (outs, env) = hint_run_with(&cands, &[false], &events);
        assert_eq!(outs, ["show 2", "accept 2"]);
        assert_eq!(env.tab.load(Ordering::SeqCst), 0, "accepting takes it down");
    }

    #[test]
    fn a_tab_swallowed_just_before_the_hint_went_down_is_given_back() {
        // The hook took the Tab while the flag was still up; the worker had already processed a
        // Backspace (hint gone) when the Tab event arrived.
        let cands = [candidate(";sig", 1, 0, false)];
        let mut events = chars(";s");
        events.extend([TypedEvent::Backspace, TypedEvent::Tab]);
        assert_eq!(
            hint_run(&cands, false, &events),
            ["show 2", "hide", "pass-tab"]
        );
    }

    #[test]
    fn the_password_check_runs_again_at_tab_time() {
        let cands = [candidate(";sig", 1, 0, false)];
        let mut events = chars(";s");
        events.push(TypedEvent::Tab);
        // Not a password box when the hint went up, one by the time of the Tab (a click into
        // another field of the same window).
        let (outs, env) = hint_run_with(&cands, &[false, true], &events);
        assert_eq!(outs, ["show 2", "hide", "pass-tab"]);
        assert_eq!(env.tab.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_accepted_hint_remembers_the_window_it_was_shown_in() {
        let (env, window) = test_env(&Arc::new(AtomicU64::new(0)));
        let cands = [candidate(";sig", 1, 0, false)];
        let mut typist = Typist::new(Matcher::new([]));
        let mut hint = None;
        let mut outs = Vec::new();
        for c in ";s".chars() {
            typist.event(TypedEvent::Char(c));
            follow_hint(
                &typist,
                &cands,
                &mut hint,
                (&env, &|| true),
                &mut || false,
                &mut |out| outs.push(out),
            );
        }
        window.store(8, Ordering::SeqCst);
        accept_hint(&mut hint, &env, || true, &mut || false, &mut |out| {
            outs.push(out);
        });
        let Some(Out::Accept(typed)) = outs.pop() else {
            panic!("an accept");
        };
        assert!(!typed.same_window(), "the focus moved to another window");
        window.store(7, Ordering::SeqCst);
        assert!(typed.same_window());
    }

    #[test]
    fn a_shown_hint_comes_down_after_some_idle_time() {
        let (keys, events) = Keys::new();
        let (utx, updates) = channel();
        utx.send(Update::List(TypingList {
            matcher: Matcher::new([]),
            candidates: vec![candidate(";sig", 1, 0, false)],
        }))
        .expect("list");
        keys.type_text(";s");
        let (env, _) = test_env(&keys.sent);
        let (otx, outs) = channel();
        let worker = std::thread::spawn(move || {
            run(
                &events,
                &updates,
                &env,
                || false,
                |out| {
                    let _ = otx.send(match out {
                        Out::Show(..) => "show",
                        Out::Hide(_) => "hide",
                        _ => "other",
                    });
                },
            );
        });
        let wait = Duration::from_secs(10);
        assert_eq!(outs.recv_timeout(wait), Ok("show"));
        assert_eq!(outs.recv_timeout(wait), Ok("hide"), "no key came");
        drop(keys);
        worker.join().expect("worker");
    }

    #[test]
    fn a_hint_that_is_old_when_a_late_hide_arrives_is_told_apart_by_its_number() {
        let cands = [candidate(";sig", 1, 0, false)];
        let ids = |events: &[TypedEvent]| {
            let (env, _) = test_env(&Arc::new(AtomicU64::new(0)));
            let mut typist = Typist::new(Matcher::new([]));
            let mut hint = None;
            let mut ids = Vec::new();
            for &event in events {
                typist.event(event);
                follow_hint(
                    &typist,
                    &cands,
                    &mut hint,
                    (&env, &|| true),
                    &mut || false,
                    &mut |out| {
                        if let Out::Show(_, id) | Out::Hide(id) = out {
                            ids.push(id);
                        }
                    },
                );
            }
            ids
        };
        let first = ids(&chars(";s"));
        let second = ids(&chars(";s"));
        assert_ne!(first[0], second[0], "every hint has a number of its own");
    }

    #[test]
    fn keys_the_hook_could_not_send_do_not_keep_later_triggers_stale_forever() {
        let item = item_id(1);
        let (keys, events) = Keys::new();
        // A key was counted and lost (the channel was full), then the hook sent a reset.
        keys.sent.fetch_add(1, Ordering::SeqCst);
        let lost = Arc::new(AtomicU64::new(1));
        keys.press(TypedEvent::Reset);
        keys.type_text(";a");
        let (utx, updates) = channel();
        utx.send(Update::List(TypingList {
            matcher: matcher(";a", item),
            candidates: Vec::new(),
        }))
        .expect("list");
        let (mut env, _) = test_env(&keys.sent);
        env.lost = lost;
        drop(keys);
        let mut got = Vec::new();
        run(
            &events,
            &updates,
            &env,
            || false,
            |out| {
                if let Out::Expand(t) = out {
                    got.push(t);
                }
            },
        );
        assert_eq!(got.len(), 1);
        assert!(
            got[0].still_current(),
            "the lost key was taken in at the reset"
        );
    }
}
