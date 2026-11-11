//! Typed triggers (v1.1): a short code such as `;sig` that, typed in any app, stands for an item.
//!
//! [`Trigger`] is the validated code; [`Matcher`] is the pure state machine that watches typed
//! characters and says when one was just completed. It sees only characters the caller feeds
//! it and keeps the last [`BUFFER_CHARS`] of them, nothing else. Whether an expansion may
//! happen at all (Sensitive items, the never-paste list, password boxes) is the caller's call.

use std::collections::VecDeque;

use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::{canonical_combining_class, compose};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::{Error, ItemId};

/// The most characters the matcher remembers, and the longest [`Trigger`].
pub const BUFFER_CHARS: usize = 32;

/// A trigger: 1 to [`BUFFER_CHARS`] characters, none of them whitespace or a control
/// character, kept in Unicode normalisation form C. Case counts: `;Sig` and `;sig` are two
/// triggers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Trigger(String);

impl Trigger {
    /// Checks the invariants. The text is normalised to NFC but not trimmed.
    ///
    /// # Errors
    ///
    /// [`Error::EmptyTrigger`], [`Error::TriggerTooLong`] or [`Error::TriggerHasSpace`].
    pub fn new(text: &str) -> Result<Self, Error> {
        if text.is_empty() {
            return Err(Error::EmptyTrigger);
        }
        let text: String = text.nfc().collect();
        let text = text.as_str();
        if text.chars().count() > BUFFER_CHARS {
            return Err(Error::TriggerTooLong { max: BUFFER_CHARS });
        }
        if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(Error::TriggerHasSpace);
        }
        Ok(Self(text.to_owned()))
    }

    /// The trigger as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Trigger {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Trigger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A trigger that was just completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fired {
    /// The item the trigger stands for.
    pub item: ItemId,
    /// How many user-perceived characters (grapheme clusters) were typed for it: the
    /// Backspaces the caller must send.
    pub chars: usize,
}

/// The fewest typed characters that make a suggestion.
pub const SUGGEST_MIN_CHARS: usize = 2;

/// A trigger the user could be offered: what stands behind it and how it ranks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The trigger code.
    pub trigger: Trigger,
    /// The item it stands for.
    pub item: ItemId,
    /// How often the item was used (more is offered first).
    pub uses: u64,
    /// Sensitive items are never suggested.
    pub sensitive: bool,
}

/// The one trigger to suggest while a code is being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suggestion {
    /// The item the full trigger stands for.
    pub item: ItemId,
    /// How many user-perceived characters of the trigger are typed: the Backspaces to send before
    /// inserting.
    pub chars: usize,
}

/// Which trigger the characters typed so far are the start of.
///
/// `typed` is the matcher's buffer ([`Matcher::typed`]) and `at_start` whether a trigger may begin at
/// its first character. A trigger is suggested once at least [`SUGGEST_MIN_CHARS`] of its characters
/// are typed, after a boundary, and not yet all of them (a complete trigger fires instead). Of
/// several, the most used wins and a tie keeps the earlier in `candidates`. Only trigger codes are
/// matched, never titles, and Sensitive items are skipped.
#[must_use]
pub fn prefix_of(typed: &str, at_start: bool, candidates: &[Candidate]) -> Option<Suggestion> {
    let typed: Vec<char> = typed.chars().collect();
    let mut best: Option<(u64, Suggestion)> = None;
    for c in candidates.iter().filter(|c| !c.sensitive) {
        let code: Vec<char> = c.trigger.as_str().chars().collect();
        let longest = code.len().saturating_sub(1).min(typed.len());
        // The longest typed tail first: a shorter one would only be a suffix of a longer typing.
        let hit = (SUGGEST_MIN_CHARS..=longest).rev().find(|&k| {
            let start = typed.len() - k;
            typed[start..] == code[..k]
                && match start {
                    0 => at_start,
                    n => is_boundary(typed[n - 1]),
                }
        });
        let Some(k) = hit else { continue };
        if best.is_none_or(|(uses, _)| c.uses > uses) {
            let chars = c.trigger.as_str().chars().take(k).collect::<String>();
            best = Some((
                c.uses,
                Suggestion {
                    item: c.item,
                    chars: chars.graphemes(true).count(),
                },
            ));
        }
    }
    best.map(|(_, s)| s)
}

/// Watches typed characters for triggers.
///
/// A trigger fires on the typing of its last character, with no space needed, provided the
/// character before it is a boundary: the start of the text, whitespace or punctuation (not a
/// letter or digit). Of several triggers ending at the same character the longest wins. A fire
/// empties the buffer, so what the caller then types or pastes is not fed back in; the caller
/// then tells the matcher how the expansion ended with [`Matcher::after_expansion`]. A
/// combining mark typed after a letter is composed with it, so `e` then U+0301 matches a
/// trigger written as `é`.
#[derive(Debug, Clone)]
pub struct Matcher {
    /// Longest first.
    triggers: Vec<(Vec<char>, ItemId, usize)>,
    buf: VecDeque<char>,
    /// Whether the position just before `buf` is a boundary (the start, or a dropped boundary).
    lead: bool,
}

/// Whether a trigger may begin right after `c`.
fn is_boundary(c: char) -> bool {
    !c.is_alphanumeric()
}

impl Matcher {
    /// A matcher for these triggers, with an empty buffer. A trigger given twice keeps the
    /// first.
    pub fn new(triggers: impl IntoIterator<Item = (Trigger, ItemId)>) -> Self {
        let mut list: Vec<(Vec<char>, ItemId, usize)> = Vec::new();
        for (t, item) in triggers {
            let chars: Vec<char> = t.as_str().chars().collect();
            if !list.iter().any(|(c, ..)| *c == chars) {
                list.push((chars, item, t.as_str().graphemes(true).count()));
            }
        }
        list.sort_by_key(|(c, ..)| core::cmp::Reverse(c.len()));
        Self {
            triggers: list,
            buf: VecDeque::new(),
            lead: true,
        }
    }

    /// Takes one typed character; returns the trigger it completed, if any.
    pub fn feed(&mut self, c: char) -> Option<Fired> {
        if canonical_combining_class(c) != 0
            && let Some(last) = self.buf.back_mut()
            && let Some(composed) = compose(*last, c)
        {
            *last = composed;
        } else {
            self.push(c);
        }
        let hit = self.triggers.iter().find(|(t, ..)| self.ends_with(t))?;
        let fired = Fired {
            item: hit.1,
            chars: hit.2,
        };
        self.reset();
        Some(fired)
    }

    fn push(&mut self, c: char) {
        if self.buf.len() == BUFFER_CHARS
            && let Some(dropped) = self.buf.pop_front()
        {
            self.lead = is_boundary(dropped);
        }
        self.buf.push_back(c);
    }

    /// Tells the matcher how the expansion that [`Matcher::feed`] just reported ended: with
    /// `last`, the final character of the text typed or pasted. If that is a letter or digit
    /// the next trigger needs a boundary of its own, so it cannot fire straight after.
    pub fn after_expansion(&mut self, last: Option<char>) {
        self.reset();
        self.lead = last.is_none_or(is_boundary);
    }

    /// The user pressed Backspace: forgets the last character. Past the start of the buffer
    /// the text before it is unknown, so no boundary can be assumed.
    pub fn backspace(&mut self) {
        if self.buf.pop_back().is_none() {
            self.lead = false;
        }
    }

    /// Forgets everything typed: the focus moved, a cursor key was pressed, and so on. The
    /// next character is taken to start a text.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.lead = true;
    }

    /// The characters typed since the last reset and whether a trigger may begin at the first of
    /// them. What [`prefix_of`] looks at.
    pub fn typed(&self) -> (String, bool) {
        (self.buf.iter().collect(), self.lead)
    }

    /// How many characters the buffer holds; never more than [`BUFFER_CHARS`].
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    fn ends_with(&self, t: &[char]) -> bool {
        let (n, m) = (self.buf.len(), t.len());
        if m > n || !self.buf.iter().skip(n - m).eq(t.iter()) {
            return false;
        }
        match n - m {
            0 => self.lead,
            before => self.buf.get(before - 1).copied().is_some_and(is_boundary),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn id(n: u8) -> ItemId {
        ItemId::from_uuid(uuid::Uuid::from_bytes([n; 16]))
    }

    fn matcher(list: &[(&str, u8)]) -> Matcher {
        Matcher::new(list.iter().map(|(t, n)| (Trigger::new(t).unwrap(), id(*n))))
    }

    /// Feeds `text`, returns every fire as (index of the character, item, chars).
    fn run(m: &mut Matcher, text: &str) -> Vec<(usize, ItemId, usize)> {
        text.chars()
            .enumerate()
            .filter_map(|(i, c)| m.feed(c).map(|f| (i, f.item, f.chars)))
            .collect()
    }

    fn cand(t: &str, n: u8, uses: u64, sensitive: bool) -> Candidate {
        Candidate {
            trigger: Trigger::new(t).unwrap(),
            item: id(n),
            uses,
            sensitive,
        }
    }

    #[test]
    fn prefix_needs_two_characters() {
        let c = [cand(";sig", 1, 5, false)];
        assert_eq!(prefix_of("", true, &c), None);
        assert_eq!(prefix_of(";", true, &c), None, "one character");
        assert_eq!(
            prefix_of(";s", true, &c),
            Some(Suggestion {
                item: id(1),
                chars: 2
            })
        );
        assert_eq!(prefix_of(";si", true, &c).map(|s| s.chars), Some(3));
        assert_eq!(
            prefix_of(";sig", true, &c),
            None,
            "complete: it fires instead"
        );
        assert_eq!(prefix_of(";x", true, &c), None, "not a prefix");
        assert_eq!(prefix_of("a;s", true, &c), None, "a letter before");
        assert_eq!(prefix_of("a ;s", true, &c).map(|s| s.chars), Some(2));
        assert_eq!(prefix_of(";s", false, &c), None, "text before the buffer");
        assert_eq!(prefix_of("hi;s", true, &c), None, "a letter before");
        assert_eq!(prefix_of("hi (;s", true, &c).map(|s| s.chars), Some(2));
    }

    #[test]
    fn prefix_picks_the_most_used() {
        let c = [
            cand(";sig", 1, 3, false),
            cand(";sign", 2, 9, false),
            cand(";sit", 3, 9, false),
            cand("zz", 4, 100, false),
        ];
        assert_eq!(
            prefix_of(";si", true, &c).map(|s| s.item),
            Some(id(2)),
            "most used; tie keeps the first"
        );
        assert_eq!(
            prefix_of(";sig", true, &c).map(|s| s.item),
            Some(id(2)),
            "only ;sign is longer"
        );
        assert_eq!(prefix_of(";sit", true, &c), None);
    }

    #[test]
    fn sensitive_items_are_never_suggested() {
        let c = [cand(";pin", 1, 99, true), cand(";pit", 2, 1, false)];
        assert_eq!(prefix_of(";pi", true, &c).map(|s| s.item), Some(id(2)));
        let only = [cand(";pin", 1, 99, true)];
        assert_eq!(prefix_of(";pi", true, &only), None);
    }

    #[test]
    fn a_suggestion_counts_graphemes_and_follows_the_matcher() {
        let c = [cand("é-mail", 1, 1, false)];
        assert_eq!(prefix_of("e\u{301}-", true, &c), None, "not composed here");
        let mut m = matcher(&[("é-mail", 1)]);
        for ch in "e\u{301}-".chars() {
            m.feed(ch);
        }
        let (typed, at_start) = m.typed();
        assert_eq!(prefix_of(&typed, at_start, &c).map(|s| s.chars), Some(2));
    }

    #[test]
    fn a_trigger_is_validated() {
        assert_eq!(Trigger::new(""), Err(Error::EmptyTrigger));
        assert_eq!(Trigger::new(";a b"), Err(Error::TriggerHasSpace));
        assert_eq!(Trigger::new("a\tb"), Err(Error::TriggerHasSpace));
        assert_eq!(Trigger::new("a\u{7}"), Err(Error::TriggerHasSpace));
        assert_eq!(
            Trigger::new(&"x".repeat(33)),
            Err(Error::TriggerTooLong { max: 32 })
        );
        assert!(
            Trigger::new(&"é".repeat(32)).is_ok(),
            "32 characters, not bytes"
        );
        assert_eq!(
            Trigger::new(" ;sig"),
            Err(Error::TriggerHasSpace),
            "not trimmed"
        );
        assert_eq!(Trigger::new(";Sig").unwrap().as_str(), ";Sig");
    }

    #[test]
    fn fires_at_the_last_character() {
        let mut m = matcher(&[(";sig", 1)]);
        assert_eq!(run(&mut m, ";sig"), [(3, id(1), 4)], "no space needed");
        assert_eq!(m.buffered(), 0, "a fire empties the buffer");
        // Typed again after a space, mid-sentence.
        assert_eq!(run(&mut m, "hi ;sig"), [(6, id(1), 4)]);
    }

    #[test]
    fn needs_a_boundary_before() {
        let mut m = matcher(&[(";sig", 1), ("brb", 2)]);
        assert!(run(&mut m, "a;sig").is_empty(), "a letter before");
        m.reset();
        assert!(run(&mut m, "7brb").is_empty(), "a digit before");
        m.reset();
        assert_eq!(run(&mut m, "(brb").len(), 1, "punctuation before");
        assert_eq!(run(&mut m, "\nbrb").len(), 1, "a newline before");
        assert_eq!(run(&mut m, ".brb").len(), 1);
        assert_eq!(run(&mut m, "brb").len(), 1, "start after a fire");
        m.reset();
        assert!(run(&mut m, "ébrb").is_empty(), "a non-ASCII letter before");
    }

    #[test]
    fn case_counts() {
        let mut m = matcher(&[(";Sig", 1)]);
        assert_eq!(run(&mut m, ";sig ;SIG").len(), 0);
        assert_eq!(run(&mut m, " ;Sig").len(), 1);
    }

    #[test]
    fn backspace_pops_one() {
        let mut m = matcher(&[("brb", 1)]);
        assert_eq!(run(&mut m, "brx").len(), 0);
        m.backspace();
        assert_eq!(
            run(&mut m, "b"),
            [(0, id(1), 3)],
            "brb after fixing the typo"
        );
        // Backspacing past what the buffer knows loses the start-of-text.
        m.reset();
        m.backspace();
        assert!(
            run(&mut m, "brb").is_empty(),
            "the character before is unknown"
        );
        m.reset();
        assert_eq!(run(&mut m, "brb").len(), 1);
    }

    #[test]
    fn reset_clears_the_buffer() {
        let mut m = matcher(&[("brb", 1)]);
        run(&mut m, "br");
        m.reset();
        assert!(run(&mut m, "b").is_empty(), "the old characters are gone");
        assert_eq!(m.buffered(), 1);
        m.reset();
        assert_eq!(m.buffered(), 0);
    }

    #[test]
    fn longest_trigger_wins() {
        // Both end at the same character, both start at a boundary.
        let mut m = matcher(&[("-sig", 1), ("sig", 2)]);
        assert_eq!(
            run(&mut m, "-sig"),
            [(3, id(1), 4)],
            "the longer one is taken"
        );
        let mut m = matcher(&[("-sig", 1), ("sig", 2)]);
        assert_eq!(
            run(&mut m, "x-sig"),
            [(4, id(2), 3)],
            "-sig has no boundary before it"
        );
        let mut m = matcher(&[(".sig", 1), ("sig", 2)]);
        assert_eq!(
            run(&mut m, " .sig"),
            [(4, id(1), 4)],
            "the longer one is taken"
        );
    }

    #[test]
    fn a_shorter_trigger_fires_first_when_typed_first() {
        let mut m = matcher(&[("addr", 1), ("addr2", 2)]);
        assert_eq!(
            run(&mut m, "addr2"),
            [(3, id(1), 4)],
            "addr completes before addr2"
        );
    }

    #[test]
    fn a_duplicate_keeps_the_first() {
        let mut m = matcher(&[("go", 1), ("go", 2)]);
        assert_eq!(run(&mut m, "go"), [(1, id(1), 2)]);
    }

    #[test]
    fn buffer_never_exceeds_32() {
        let mut m = matcher(&[(";x", 1)]);
        for _ in 0..100 {
            m.feed('a');
            assert!(m.buffered() <= 32);
        }
        assert_eq!(m.buffered(), 32);
    }

    #[test]
    fn a_32_character_trigger_still_needs_its_boundary() {
        let long = "k".repeat(32);
        let mut m = matcher(&[(&long, 1)]);
        assert_eq!(run(&mut m, &long).len(), 1, "at the start");
        assert_eq!(run(&mut m, &format!(" {long}")).len(), 1, "after a space");
        m.reset();
        assert!(
            run(&mut m, &format!("x{long}")).is_empty(),
            "after a letter"
        );
        m.reset();
        // A boundary that has scrolled out of the buffer is still remembered.
        assert_eq!(run(&mut m, &format!("a b {long}")).len(), 1);
    }

    #[test]
    fn triggers_and_typing_are_compared_in_nfc() {
        // Stored decomposed, stored composed: the same trigger.
        assert_eq!(
            Trigger::new(";cafe\u{301}").unwrap(),
            Trigger::new(";caf\u{e9}").unwrap()
        );
        let mut m = matcher(&[(";cafe\u{301}", 1)]);
        // Typed as e then a combining acute accent.
        let fired = run(&mut m, ";cafe\u{301}");
        assert_eq!(fired.len(), 1);
        // Five graphemes, though six characters were typed: five Backspaces.
        assert_eq!(fired[0].2, 5);
        let mut m = matcher(&[(";caf\u{e9}", 1)]);
        assert_eq!(
            run(&mut m, ";cafe\u{301}").len(),
            1,
            "composed form typed decomposed"
        );
    }

    #[test]
    fn after_an_expansion_ending_in_a_word_character_the_next_trigger_needs_a_boundary() {
        let mut m = matcher(&[("brb", 1)]);
        assert_eq!(run(&mut m, "brb").len(), 1);
        m.after_expansion(Some('x'));
        assert!(
            run(&mut m, "brb").is_empty(),
            "glued to the expansion's last letter"
        );
        m.after_expansion(Some('.'));
        assert_eq!(run(&mut m, "brb").len(), 1, "punctuation is a boundary");
        m.after_expansion(None);
        assert_eq!(run(&mut m, "brb").len(), 1, "nothing known: a fresh start");
        assert_eq!(m.buffered(), 0);
    }

    proptest! {
        #[test]
        fn feed_never_panics(ops in proptest::collection::vec(
            prop_oneof![
                any::<char>().prop_map(Some),
                Just(None),
            ], 0..200), reset_at in 0usize..200)
        {
            let mut m = matcher(&[(";sig", 1), ("é", 2), ("a", 3)]);
            for (i, op) in ops.into_iter().enumerate() {
                match op {
                    Some(c) => { m.feed(c); }
                    None => m.backspace(),
                }
                if i == reset_at { m.reset(); }
                prop_assert!(m.buffered() <= BUFFER_CHARS);
            }
        }
    }
}
