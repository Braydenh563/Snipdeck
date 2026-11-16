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

/// When a trigger fires (TextExpander's "expand immediately or at a delimiter").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum TriggerEnd {
    /// On the typing of its last character, once a boundary stands before it (the default).
    #[default]
    Immediate,
    /// When a delimiter (a space, Enter, Tab or any punctuation) is typed right after it; the
    /// delimiter is kept after the expansion. Safe for short, all-letter codes such as `sig`.
    Delimiter,
}

impl TriggerEnd {
    /// Both, in the order the editor lists them.
    pub const ALL: [Self; 2] = [Self::Immediate, Self::Delimiter];

    /// The stable key stored in the library and the archive.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::Delimiter => "delimiter",
        }
    }

    /// The end for a stored key.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.key() == key)
    }
}

/// How a trigger treats the case of what is typed (TextExpander and Alfred: "ignore case" and
/// "adapt to typed case").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum CaseMode {
    /// `;Sig` and `;sig` are two codes (the default).
    #[default]
    Exact,
    /// Any case of the letters fires it; the text is inserted as it is.
    Ignore,
    /// Any case fires it, and the text follows what was typed: ALL CAPS gives upper case and a
    /// capital first letter gives a capital first letter.
    Adapt,
}

impl CaseMode {
    /// All three, in the order the editor lists them.
    pub const ALL: [Self; 3] = [Self::Exact, Self::Ignore, Self::Adapt];

    /// The stable key stored in the library and the archive.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Ignore => "ignore",
            Self::Adapt => "adapt",
        }
    }

    /// The mode for a stored key.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.key() == key)
    }

    /// Whether `a` and `b` are the same typed character under this mode.
    fn same(self, a: char, b: char) -> bool {
        a == b || (self != Self::Exact && a.to_lowercase().eq(b.to_lowercase()))
    }
}

/// How the text a fired trigger stands for follows the case that was typed
/// ([`CaseMode::Adapt`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseShape {
    /// Inserted as it is.
    #[default]
    AsIs,
    /// Typed in capitals (two or more letters, none lower case): all upper case.
    Upper,
    /// Typed with a capital first letter and the rest lower case: a capital first letter.
    Capital,
}

impl CaseShape {
    /// The shape of `typed` against the stored `code`. A code with no letters, or typed exactly
    /// as stored, is [`CaseShape::AsIs`].
    #[must_use]
    pub fn of(typed: &[char], code: &[char]) -> Self {
        if typed == code {
            return Self::AsIs;
        }
        let letters: Vec<char> = typed.iter().copied().filter(|c| c.is_alphabetic()).collect();
        let upper = |c: &char| c.is_uppercase();
        let lower = |c: &char| c.is_lowercase();
        match letters.as_slice() {
            [] => Self::AsIs,
            [only] if upper(only) => Self::Capital,
            all if all.iter().all(upper) => Self::Upper,
            [first, rest @ ..] if upper(first) && rest.iter().all(lower) => Self::Capital,
            _ => Self::AsIs,
        }
    }
}

impl CaseShape {
    /// `text` in this shape: upper case, or with its first letter in capitals and the rest left
    /// as written.
    #[must_use]
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::AsIs => text.to_owned(),
            Self::Upper => text.to_uppercase(),
            Self::Capital => {
                let mut chars = text.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        }
    }
}

/// What a trigger does besides standing for an item: when it fires and how it reads case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct TriggerOptions {
    /// When it fires.
    pub end: TriggerEnd,
    /// How it reads case.
    pub case: CaseMode,
}

/// A trigger: 1 to [`BUFFER_CHARS`] characters, none of them whitespace or a control
/// character, kept in Unicode normalisation form C, with its [`TriggerOptions`]. By default case
/// counts: `;Sig` and `;sig` are two triggers (see [`CaseMode`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Trigger {
    text: String,
    options: TriggerOptions,
}

/// The fewest characters of a trigger made only of letters and digits that is accepted when it
/// fires without a delimiter: a shorter one fires inside ordinary words.
pub const MIN_PLAIN_CHARS: usize = 3;

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
        Ok(Self {
            text: text.to_owned(),
            options: TriggerOptions::default(),
        })
    }

    /// This trigger with `options`.
    #[must_use]
    pub fn with_options(mut self, options: TriggerOptions) -> Self {
        self.options = options;
        self
    }

    /// The trigger as a string slice.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// When it fires and how it reads case.
    #[must_use]
    pub fn options(&self) -> TriggerOptions {
        self.options
    }

    /// Whether this code would fire inside ordinary words: it fires without a delimiter and is
    /// made of fewer than [`MIN_PLAIN_CHARS`] characters, all of them letters or digits (`a`,
    /// `1`, `sg`). A code with a symbol in it (`;s`, `,,`) needs that symbol typed first and is
    /// not "plain".
    #[must_use]
    pub fn fires_in_words(&self) -> bool {
        self.options.end == TriggerEnd::Immediate
            && self.text.chars().count() < MIN_PLAIN_CHARS
            && self.text.chars().all(char::is_alphanumeric)
    }

    /// Whether typing `other` (or the start of it) could make this one fire first, so that
    /// `other` never fires: this code, read with `other`'s case rules, is a proper start of `other`,
    /// or the same code, and fires without a delimiter.
    #[must_use]
    pub fn shadows(&self, other: &Self) -> bool {
        if self.options.end != TriggerEnd::Immediate {
            return false;
        }
        let mode = if self.options.case == CaseMode::Exact && other.options.case == CaseMode::Exact
        {
            CaseMode::Exact
        } else {
            CaseMode::Ignore
        };
        let (mine, theirs): (Vec<char>, Vec<char>) =
            (self.text.chars().collect(), other.text.chars().collect());
        mine.len() <= theirs.len() && mine.iter().zip(&theirs).all(|(a, b)| mode.same(*a, *b))
    }

    /// Whether the two are the same code under their case modes (`;Sig` and `;sig` when one of
    /// them ignores case): they may not both be kept.
    #[must_use]
    pub fn same_code(&self, other: &Self) -> bool {
        let mode = if self.options.case == CaseMode::Exact && other.options.case == CaseMode::Exact
        {
            CaseMode::Exact
        } else {
            CaseMode::Ignore
        };
        let (mut a, mut b) = (self.text.chars(), other.text.chars());
        loop {
            match (a.next(), b.next()) {
                (None, None) => return true,
                (Some(x), Some(y)) if mode.same(x, y) => {}
                _ => return false,
            }
        }
    }
}

impl AsRef<str> for Trigger {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl core::fmt::Display for Trigger {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.text)
    }
}

/// A trigger that was just completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fired {
    /// The item the trigger stands for.
    pub item: ItemId,
    /// The trigger that was typed (so a trigger changed since can be told from this one).
    pub trigger: Trigger,
    /// How many user-perceived characters (grapheme clusters) were typed for it, the delimiter
    /// included: the Backspaces the caller must send.
    pub chars: usize,
    /// The delimiter that ended it ([`TriggerEnd::Delimiter`]), to type again after the text.
    pub delimiter: Option<char>,
    /// How the text follows the case typed ([`CaseMode::Adapt`]).
    pub shape: CaseShape,
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
#[derive(Clone)]
pub struct Matcher {
    /// Longest first.
    triggers: Vec<Entry>,
    buf: VecDeque<char>,
    /// Whether the position just before `buf` is a boundary (the start, or a dropped boundary).
    lead: bool,
}

/// One trigger as the matcher holds it.
#[derive(Clone)]
struct Entry {
    code: Vec<char>,
    trigger: Trigger,
    item: ItemId,
    /// Grapheme clusters in the code.
    graphemes: usize,
}

// What was typed is private (a password may pass through): never print the buffer.
impl core::fmt::Debug for Matcher {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Matcher")
            .field("triggers", &self.triggers.len())
            .field("buffered", &self.buf.len())
            .finish_non_exhaustive()
    }
}

/// Whether a trigger may begin right after `c`.
fn is_boundary(c: char) -> bool {
    !c.is_alphanumeric()
}

/// Whether typing `c` right after a [`TriggerEnd::Delimiter`] trigger completes it: a space,
/// Enter, Tab or any punctuation, that is any character that is not a letter or digit and does
/// not combine with the one before.
fn is_delimiter(c: char) -> bool {
    !c.is_alphanumeric() && canonical_combining_class(c) == 0
}

impl Matcher {
    /// A matcher for these triggers, with an empty buffer. A trigger given twice keeps the
    /// first.
    pub fn new(triggers: impl IntoIterator<Item = (Trigger, ItemId)>) -> Self {
        let mut list: Vec<Entry> = Vec::new();
        for (trigger, item) in triggers {
            if !list.iter().any(|e| e.trigger.same_code(&trigger)) {
                list.push(Entry {
                    code: trigger.as_str().chars().collect(),
                    graphemes: trigger.as_str().graphemes(true).count(),
                    trigger,
                    item,
                });
            }
        }
        list.sort_by_key(|e| core::cmp::Reverse(e.code.len()));
        Self {
            triggers: list,
            buf: VecDeque::new(),
            lead: true,
        }
    }

    /// Takes one typed character; returns the trigger it completed, if any.
    pub fn feed(&mut self, c: char) -> Option<Fired> {
        let mut pushed = true;
        if canonical_combining_class(c) != 0
            && let Some(last) = self.buf.back_mut()
            && let Some(composed) = compose(*last, c)
        {
            *last = composed;
            pushed = false;
        } else {
            self.push(c);
        }
        let hit = self
            .triggers
            .iter()
            .find(|e| e.trigger.options().end == TriggerEnd::Immediate && self.ends_with(e, 0))
            .map(|e| (e, None))
            .or_else(|| {
                if !(pushed && is_delimiter(c)) {
                    return None;
                }
                self.triggers
                    .iter()
                    .find(|e| {
                        e.trigger.options().end == TriggerEnd::Delimiter && self.ends_with(e, 1)
                    })
                    .map(|e| (e, Some(c)))
            })?;
        let (entry, delimiter) = hit;
        let skip = usize::from(delimiter.is_some());
        let typed: Vec<char> = self
            .buf
            .iter()
            .skip(self.buf.len() - skip - entry.code.len())
            .take(entry.code.len())
            .copied()
            .collect();
        let shape = if entry.trigger.options().case == CaseMode::Adapt {
            CaseShape::of(&typed, &entry.code)
        } else {
            CaseShape::AsIs
        };
        let fired = Fired {
            item: entry.item,
            trigger: entry.trigger.clone(),
            chars: entry.graphemes + skip,
            delimiter,
            shape,
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

    /// Changes only what the position before the buffer counts as, keeping the buffer: the
    /// character that stands there is `last` (see [`Matcher::after_expansion`], which also
    /// forgets the buffer). For a correction that arrives while the user already types on.
    pub fn set_lead(&mut self, last: Option<char>) {
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

    /// Whether the buffer, without its last `tail` characters, ends with the entry's code under
    /// its case mode and a boundary stands before it.
    fn ends_with(&self, e: &Entry, tail: usize) -> bool {
        let mode = e.trigger.options().case;
        let (m, n) = (e.code.len(), self.buf.len().saturating_sub(tail));
        if m > n || self.buf.len() < tail {
            return false;
        }
        if !self
            .buf
            .iter()
            .skip(n - m)
            .take(m)
            .zip(&e.code)
            .all(|(a, b)| mode.same(*a, *b))
        {
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
    fn set_lead_changes_the_boundary_and_keeps_what_was_typed() {
        let mut m = matcher(&[(";bb", 1)]);
        m.after_expansion(Some('a'));
        assert!(
            run(&mut m, ";b").is_empty(),
            "no boundary after the letter: nothing fires"
        );
        m.set_lead(None);
        assert_eq!(m.buffered(), 2, "the buffer stays");
        assert_eq!(run(&mut m, "b"), vec![(0, id(1), 3)]);
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
    fn a_fire_names_the_trigger_that_was_typed() {
        let mut m = matcher(&[(";sig", 1)]);
        let fired = ";sig".chars().find_map(|c| m.feed(c)).expect("fires");
        assert_eq!(fired.trigger, Trigger::new(";sig").unwrap());
    }

    #[test]
    fn debug_does_not_show_what_was_typed() {
        let mut m = matcher(&[(";sig", 1)]);
        for c in "hunter2".chars() {
            m.feed(c);
        }
        let shown = format!("{m:?}");
        assert!(
            !shown.contains("hunter") && !shown.contains(";sig"),
            "{shown}"
        );
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

    fn opt(text: &str, end: TriggerEnd, case: CaseMode) -> Trigger {
        Trigger::new(text)
            .unwrap()
            .with_options(TriggerOptions { end, case })
    }

    fn matcher_of(list: Vec<(Trigger, u8)>) -> Matcher {
        Matcher::new(list.into_iter().map(|(t, n)| (t, id(n))))
    }

    fn feed_all(m: &mut Matcher, text: &str) -> Vec<Fired> {
        text.chars().filter_map(|c| m.feed(c)).collect()
    }

    #[test]
    fn a_delimiter_trigger_fires_on_the_character_after_it_and_names_it() {
        let mut m = matcher_of(vec![(opt("sig", TriggerEnd::Delimiter, CaseMode::Exact), 1)]);
        assert!(feed_all(&mut m, "sig").is_empty(), "not yet: it waits");
        let fired = feed_all(&mut m, " ");
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].item, id(1));
        assert_eq!(fired[0].delimiter, Some(' '));
        assert_eq!(fired[0].chars, 4, "the code and the delimiter are erased");
        for d in ['.', ',', '\n', '\t', '!'] {
            let mut m =
                matcher_of(vec![(opt("sig", TriggerEnd::Delimiter, CaseMode::Exact), 1)]);
            feed_all(&mut m, "sig");
            assert_eq!(m.feed(d).and_then(|f| f.delimiter), Some(d), "{d:?}");
        }
    }

    #[test]
    fn a_delimiter_trigger_does_not_fire_inside_a_word_or_on_a_letter() {
        let mut m = matcher_of(vec![(opt("sig", TriggerEnd::Delimiter, CaseMode::Exact), 1)]);
        assert!(feed_all(&mut m, "design ").is_empty(), "needs a boundary before");
        assert!(feed_all(&mut m, "sigh ").is_empty(), "a letter is not a delimiter");
        assert_eq!(feed_all(&mut m, "sig.").len(), 1);
    }

    #[test]
    fn an_immediate_trigger_still_fires_without_a_delimiter() {
        let mut m = matcher_of(vec![(opt(";sig", TriggerEnd::Immediate, CaseMode::Exact), 1)]);
        let fired = feed_all(&mut m, ";sig");
        assert_eq!((fired.len(), fired[0].delimiter, fired[0].chars), (1, None, 4));
    }

    #[test]
    fn ignore_case_matches_any_case_and_exact_does_not() {
        let mut m = matcher_of(vec![(opt(";sig", TriggerEnd::Immediate, CaseMode::Ignore), 1)]);
        assert_eq!(feed_all(&mut m, ";SiG").len(), 1);
        let mut m = matcher_of(vec![(opt(";sig", TriggerEnd::Immediate, CaseMode::Exact), 1)]);
        assert!(feed_all(&mut m, ";SIG").is_empty());
    }

    #[test]
    fn adapt_reports_the_shape_that_was_typed() {
        let shapes = |typed: &str| {
            let mut m =
                matcher_of(vec![(opt("brb", TriggerEnd::Delimiter, CaseMode::Adapt), 1)]);
            feed_all(&mut m, &format!("{typed} "))[0].shape
        };
        assert_eq!(shapes("brb"), CaseShape::AsIs);
        assert_eq!(shapes("BRB"), CaseShape::Upper);
        assert_eq!(shapes("Brb"), CaseShape::Capital);
        assert_eq!(shapes("bRb"), CaseShape::AsIs);
        let mut m = matcher_of(vec![(opt("b", TriggerEnd::Delimiter, CaseMode::Adapt), 1)]);
        assert_eq!(feed_all(&mut m, "B ")[0].shape, CaseShape::Capital, "one capital letter");
    }

    #[test]
    fn a_shape_is_applied_to_the_text() {
        assert_eq!(CaseShape::AsIs.apply("Best regards"), "Best regards");
        assert_eq!(CaseShape::Upper.apply("Best regards"), "BEST REGARDS");
        assert_eq!(CaseShape::Capital.apply("best regards"), "Best regards");
        assert_eq!(CaseShape::Capital.apply("éa"), "Éa");
        assert_eq!(CaseShape::Capital.apply(""), "");
    }

    #[test]
    fn a_trigger_with_no_letters_never_adapts() {
        let code: Vec<char> = ";1".chars().collect();
        assert_eq!(CaseShape::of(&code, &code), CaseShape::AsIs);
        assert_eq!(CaseShape::of(&[';', '2'], &code), CaseShape::AsIs);
    }

    #[test]
    fn two_codes_that_differ_only_by_case_are_one_when_a_mode_ignores_case() {
        let exact = |t| opt(t, TriggerEnd::Immediate, CaseMode::Exact);
        let ignore = |t| opt(t, TriggerEnd::Immediate, CaseMode::Ignore);
        assert!(!exact(";Sig").same_code(&exact(";sig")));
        assert!(exact(";Sig").same_code(&ignore(";sig")));
        assert!(ignore(";sig").same_code(&exact(";SIG")));
        assert!(!ignore(";sig").same_code(&ignore(";sign")));
    }

    #[test]
    fn a_short_code_made_of_letters_fires_in_words_only_without_a_delimiter() {
        let t = |s, e| opt(s, e, CaseMode::Exact);
        assert!(t("ab", TriggerEnd::Immediate).fires_in_words());
        assert!(t("a", TriggerEnd::Immediate).fires_in_words());
        assert!(!t("abc", TriggerEnd::Immediate).fires_in_words());
        assert!(!t("ab", TriggerEnd::Delimiter).fires_in_words());
        assert!(!t(";a", TriggerEnd::Immediate).fires_in_words(), "a symbol starts it");
        assert!(!t(",,", TriggerEnd::Immediate).fires_in_words());
    }

    #[test]
    fn an_immediate_code_shadows_the_longer_ones_it_starts() {
        let t = |s, e| opt(s, e, CaseMode::Exact);
        let imm = |s| t(s, TriggerEnd::Immediate);
        assert!(imm(";s").shadows(&imm(";sig")));
        assert!(imm(";sig").shadows(&imm(";sig")));
        assert!(!imm(";sig").shadows(&imm(";s")));
        assert!(!t(";s", TriggerEnd::Delimiter).shadows(&imm(";sig")));
        assert!(!imm(";x").shadows(&imm(";sig")));
        assert!(!imm(";S").shadows(&imm(";sig")), "exact case: different");
        assert!(
            opt(";S", TriggerEnd::Immediate, CaseMode::Ignore).shadows(&imm(";sig")),
            "ignoring case: the same start"
        );
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
