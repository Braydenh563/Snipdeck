//! Search: one query pipeline for the Panel, the Picker and the Main window (SPEC §5.2, §9).
//!
//! A [`Query`] is parsed once from what the user typed (words, `"phrases"`, `-negation`,
//! `tag:`, `list:`, `kind:`, `is:pinned`; see [`Query::parse`]). A [`SearchIndex`] holds, per
//! item, the folded text of its trigger, title, tag names, list name and the first
//! [`CONTENT_CHARS`] characters of its preview, and answers [`SearchIndex::query`]. The rest of
//! the content lives in the store's full-text index; the store answers which items contain
//! each word there ([`BodyMatches`]), and the index counts that as a match in the weakest
//! field. Every surface goes through the same two steps, so the same query finds the same
//! items everywhere.
//!
//! **Matching.** Every word must match (or, negated, must not), each in *any* field: `invoice
//! work` finds an item titled "Invoice" tagged "work". Matching is on folded text ([`fold`]):
//! case, accents and full-width forms never hide an item. A word matches a field exactly, as a
//! prefix, at a word start, anywhere inside, or (titles and tags only, two characters or more,
//! not a phrase) as a subsequence: `emsig` finds "Email signature".
//!
//! **Ranking.** Each word scores its best field: match quality first (exact, prefix, word
//! start, inside, subsequence), then the field (trigger, title, tag, list, content, rest of
//! the body); an item's score is the sum over its words. So an exact trigger is the first hit,
//! and an exact tag beats a scattered subsequence in a title. Equal scores go to the item used
//! more often, then to the one used last, and only then keep the order the items were added in
//! (the user's order).
//!
//! **Marks.** [`find`] and [`locate`] say where a query matches in a piece of text, with the
//! same folding and the same per-word rules as the ranking, so a row can always show why it is
//! there.
//!
//! **Tags, lists and triggers.** [`Item`] stores ids, so the index cannot see names by itself:
//! pass them with [`SearchIndex::upsert`]. Renaming a tag or a list means upserting the items
//! that carry it again.

mod query;

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

pub use query::{
    Condition, Filter, MAX_QUERY_CHARS, MAX_TERMS, Query, Term, fold, is_unspaced_script,
};

use crate::{Item, ItemId, ItemKind};

/// How many characters of an item's preview are matched in memory (SPEC §5.1); the rest of the
/// content is the full-text index's.
pub const CONTENT_CHARS: usize = 64;

/// Titles, tag, list and trigger names are matched up to this many characters; the rest is
/// ignored so a pathological title cannot slow every query down.
const FIELD_CHARS: usize = 256;

/// Below this many items a query is scored on the calling thread; spawning threads costs
/// more than it saves.
const PARALLEL_FROM: usize = 2_000;

/// The most scoring threads a query uses.
const MAX_THREADS: usize = 8;

/// Where a word matched, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    /// The content past the preview, from the full-text index.
    Body = 0,
    /// The first [`CONTENT_CHARS`] characters of the preview.
    Content = 1,
    /// The name of the item's list.
    List = 2,
    /// A tag name.
    Tag = 3,
    /// The title.
    Title = 4,
    /// The trigger (abbreviation), such as `;sig`.
    Trigger = 5,
}

/// How well a word matched a field, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Quality {
    /// The word's characters in order, with gaps.
    Subsequence = 1,
    /// Somewhere inside.
    Inside = 2,
    /// At the start of a word of the field.
    WordStart = 3,
    /// At the start of the field.
    Prefix = 4,
    /// The whole field.
    Exact = 5,
}

/// The score of one word: quality first, field second.
fn term_score(quality: Quality, field: Field) -> u32 {
    quality as u32 * 8 + field as u32
}

/// One matching item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The matching item.
    pub id: ItemId,
    /// Higher is better. Only comparable within one query. Zero for every item of a query
    /// without words.
    pub score: u32,
    /// The name of a tag that was the best match of one of the words, as the user wrote it.
    /// `None` when no word was best explained by a tag.
    pub tag: Option<String>,
}

/// The names an item is searched by besides its own fields.
#[derive(Debug, Clone, Copy, Default)]
pub struct Names<'a> {
    /// The names of its tags.
    pub tags: &'a [&'a str],
    /// The name of its list.
    pub list: &'a str,
    /// Its trigger, if it has one.
    pub trigger: Option<&'a str>,
}

/// Which items contain each word of a query in the content the index does not hold, as the
/// store's full-text index answers it. Aligned with [`Query::terms`]; `None` for a word that
/// was not looked up (too short, see [`Term::searches_body`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodyMatches(Vec<Option<HashSet<ItemId>>>);

impl BodyMatches {
    /// The answer for each term, in [`Query::terms`] order.
    #[must_use]
    pub fn new(per_term: Vec<Option<HashSet<ItemId>>>) -> Self {
        Self(per_term)
    }

    /// The items found for term `term`, or `None` if it was not looked up.
    #[must_use]
    pub fn term(&self, term: usize) -> Option<&HashSet<ItemId>> {
        self.0.get(term).and_then(Option::as_ref)
    }

    fn has(&self, term: usize, id: ItemId) -> bool {
        self.term(term).is_some_and(|ids| ids.contains(&id))
    }
}

#[derive(Debug, Clone)]
struct Entry {
    id: ItemId,
    /// Insertion order, for ties (positions move when an item is removed).
    order: u64,
    title: String,
    trigger: String,
    /// Folded, and as written.
    tags: Vec<(String, String)>,
    list: String,
    content: String,
    kind: ItemKind,
    pinned: bool,
    /// How often the item was used, for ties.
    use_count: u32,
    /// When it was last used (Unix seconds), `i64::MIN` if never, for ties.
    last_used: i64,
}

/// An in-memory index over the library's items, shared by every search surface.
#[derive(Debug, Default, Clone)]
pub struct SearchIndex {
    /// Shared entries: cloning the index copies pointers, and a patch replaces one entry.
    entries: Vec<Arc<Entry>>,
    /// Shared too: it changes only when an item is added or removed, not on an edit.
    positions: Arc<HashMap<ItemId, usize>>,
    next_order: u64,
}

fn last_used_of(item: &Item) -> i64 {
    item.last_used_at
        .map_or(i64::MIN, crate::Timestamp::as_second)
}

fn folded(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => fold(&text[..end]),
        None => fold(text),
    }
}

impl SearchIndex {
    /// An index of `items`, in the order given, without tag, list or trigger names (see
    /// [`SearchIndex::upsert`]).
    pub fn new<'a>(items: impl IntoIterator<Item = &'a Item>) -> Self {
        let mut index = Self::default();
        for item in items {
            index.upsert(item, Names::default());
        }
        index
    }

    /// How many items are indexed.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is indexed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `id` is indexed.
    pub fn contains(&self, id: ItemId) -> bool {
        self.positions.contains_key(&id)
    }

    /// Adds `item` with its `names`, or replaces its entry (keeping its place) if it is
    /// already indexed.
    pub fn upsert(&mut self, item: &Item, names: Names<'_>) {
        let known = self.positions.get(&item.id).copied();
        let order = known
            .and_then(|at| self.entries.get(at))
            .map_or(self.next_order, |e| e.order);
        let entry = Entry {
            id: item.id,
            order,
            title: folded(&item.title, FIELD_CHARS),
            trigger: names
                .trigger
                .map(|t| folded(t, FIELD_CHARS))
                .unwrap_or_default(),
            tags: names
                .tags
                .iter()
                .map(|tag| (folded(tag, FIELD_CHARS), (*tag).to_owned()))
                .collect(),
            list: folded(names.list, FIELD_CHARS),
            content: folded(&item.preview, CONTENT_CHARS),
            kind: item.kind,
            pinned: item.pinned,
            use_count: item.use_count,
            last_used: last_used_of(item),
        };
        if let Some(slot) = known.and_then(|at| self.entries.get_mut(at)) {
            *slot = Arc::new(entry);
        } else {
            Arc::make_mut(&mut self.positions).insert(item.id, self.entries.len());
            self.entries.push(Arc::new(entry));
            self.next_order += 1;
        }
    }

    /// Whether the use counts and last-use times (the tie-breakers) of `items` are the ones
    /// indexed.
    pub fn usage_is_current<'a>(&self, items: impl IntoIterator<Item = &'a Item>) -> bool {
        items.into_iter().all(|item| {
            self.positions
                .get(&item.id)
                .and_then(|&at| self.entries.get(at))
                .is_none_or(|e| e.use_count == item.use_count && e.last_used == last_used_of(item))
        })
    }

    /// Brings the use counts and last-use times (the tie-breakers) of `items` up to date
    /// without touching what is searched. A counted use changes only these. Returns whether
    /// any entry changed.
    pub fn refresh_usage<'a>(&mut self, items: impl IntoIterator<Item = &'a Item>) -> bool {
        let mut changed = false;
        for item in items {
            let Some(entry) = self
                .positions
                .get(&item.id)
                .and_then(|&at| self.entries.get_mut(at))
            else {
                continue;
            };
            let last_used = last_used_of(item);
            if entry.use_count != item.use_count || entry.last_used != last_used {
                let entry = Arc::make_mut(entry);
                entry.use_count = item.use_count;
                entry.last_used = last_used;
                changed = true;
            }
        }
        changed
    }

    /// Drops `id` from the index in constant time. Returns whether it was there.
    pub fn remove(&mut self, id: ItemId) -> bool {
        let Some(at) = Arc::make_mut(&mut self.positions).remove(&id) else {
            return false;
        };
        self.entries.swap_remove(at);
        if let Some(moved) = self.entries.get(at) {
            Arc::make_mut(&mut self.positions).insert(moved.id, at);
        }
        true
    }

    /// The best `limit` matches for `query`, best first. `body` is the store's answer for the
    /// content the index does not hold (pass [`BodyMatches::default`] for none).
    ///
    /// A query without words or filters matches everything: the first `limit` items in the
    /// order they were added, with a score of 0. Large indexes (from 2,000 items) are scored on
    /// several threads.
    pub fn query(&self, query: &Query, body: &BodyMatches, limit: usize) -> Vec<Hit> {
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        self.run(query, body, limit, PARALLEL_FROM, threads.min(MAX_THREADS))
    }

    /// [`SearchIndex::query`] with the threading policy exposed for tests.
    fn run(
        &self,
        query: &Query,
        body: &BodyMatches,
        limit: usize,
        parallel_from: usize,
        threads: usize,
    ) -> Vec<Hit> {
        if limit == 0 {
            return Vec::new();
        }
        let mut candidates = if self.entries.len() >= parallel_from && threads > 1 {
            self.score_parallel(query, body, threads)
        } else {
            score_chunk(&self.entries, 0, query, body)
        };

        // Best score first; equal scores by use count, then by last use, then by the order
        // the items were added in, i.e. the user's order.
        // A query of filters alone ranks nothing: it keeps the user's order.
        let usage = !query.terms.is_empty();
        let rank = |a: &Scored| {
            let e = &self.entries[a.at];
            let (count, last) = if usage {
                (e.use_count, e.last_used)
            } else {
                (0, 0)
            };
            (Reverse(a.score), Reverse(count), Reverse(last), e.order)
        };
        let order = |a: &Scored, b: &Scored| rank(a).cmp(&rank(b));
        if candidates.len() > limit {
            candidates.select_nth_unstable_by(limit - 1, order);
            candidates.truncate(limit);
        }
        candidates.sort_unstable_by(order);
        candidates
            .into_iter()
            .map(|c| {
                let entry = &self.entries[c.at];
                Hit {
                    id: entry.id,
                    score: c.score,
                    tag: c
                        .tag
                        .and_then(|n| entry.tags.get(n))
                        .map(|(_, t)| t.clone()),
                }
            })
            .collect()
    }

    fn score_parallel(&self, query: &Query, body: &BodyMatches, threads: usize) -> Vec<Scored> {
        let chunk_len = self.entries.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            let workers: Vec<_> = self
                .entries
                .chunks(chunk_len)
                .enumerate()
                .map(|(n, chunk)| {
                    scope.spawn(move || score_chunk(chunk, n * chunk_len, query, body))
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
                })
                .collect()
        })
    }
}

/// A matching entry: where it is, its score and the tag that explained a word.
#[derive(Debug, Clone, Copy)]
struct Scored {
    at: usize,
    score: u32,
    tag: Option<usize>,
}

/// Scores `entries` (which start at index `first` of the whole index) and returns those that
/// match.
fn score_chunk(
    entries: &[Arc<Entry>],
    first: usize,
    query: &Query,
    body: &BodyMatches,
) -> Vec<Scored> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(n, entry)| {
            let (score, tag) = evaluate(entry, query, body)?;
            Some(Scored {
                at: first + n,
                score,
                tag,
            })
        })
        .collect()
}

/// Whether `entry` matches `query`, with its score and the index of the tag that explained a
/// word.
fn evaluate(entry: &Entry, query: &Query, body: &BodyMatches) -> Option<(u32, Option<usize>)> {
    for condition in &query.filters {
        let holds = match &condition.filter {
            Filter::Tag(value) => entry
                .tags
                .iter()
                .any(|(t, _)| t.starts_with(value.as_str())),
            Filter::List(value) => entry.list.starts_with(value.as_str()),
            Filter::Kind(kind) => entry.kind == *kind,
            Filter::Pinned => entry.pinned,
        };
        if holds == condition.negated {
            return None;
        }
    }
    let mut score = 0;
    let mut tag = None;
    for (n, term) in query.terms.iter().enumerate() {
        if term.negated {
            if entry.mentions(term) || body.has(n, entry.id) {
                return None;
            }
            continue;
        }
        let (quality, field, which) = match entry.best(term) {
            Some(best) => best,
            None if body.has(n, entry.id) => (Quality::Inside, Field::Body, None),
            None => return None,
        };
        score += term_score(quality, field);
        if field == Field::Tag {
            tag = tag.or(which);
        }
    }
    Some((score, tag))
}

impl Entry {
    /// The best field `term` matches, its quality, and for a tag which one.
    fn best(&self, term: &Term) -> Option<(Quality, Field, Option<usize>)> {
        let fuzzy = !term.phrase && term.text.chars().count() >= 2;
        let mut best: Option<(Quality, Field, Option<usize>)> = None;
        let mut offer = |quality: Option<Quality>, field: Field, which: Option<usize>| {
            if let Some(quality) = quality
                && best.is_none_or(|(q, f, _)| (quality, field) > (q, f))
            {
                best = Some((quality, field, which));
            }
        };
        offer(
            grade(&self.trigger, &term.text, false),
            Field::Trigger,
            None,
        );
        offer(grade(&self.title, &term.text, fuzzy), Field::Title, None);
        for (n, (tag, _)) in self.tags.iter().enumerate() {
            offer(grade(tag, &term.text, fuzzy), Field::Tag, Some(n));
        }
        offer(grade(&self.list, &term.text, false), Field::List, None);
        offer(
            grade(&self.content, &term.text, false),
            Field::Content,
            None,
        );
        best
    }

    /// Whether `term` occurs in any field (for negation: no subsequences).
    fn mentions(&self, term: &Term) -> bool {
        let t = term.text.as_str();
        self.trigger.contains(t)
            || self.title.contains(t)
            || self.tags.iter().any(|(tag, _)| tag.contains(t))
            || self.list.contains(t)
            || self.content.contains(t)
    }
}

fn word_start(hay: &str, at: usize) -> bool {
    hay[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric())
}

/// How well folded `needle` matches folded `hay`.
fn grade(hay: &str, needle: &str, fuzzy: bool) -> Option<Quality> {
    if hay.len() < needle.len() || needle.is_empty() {
        return None;
    }
    if hay == needle {
        return Some(Quality::Exact);
    }
    if hay.starts_with(needle) {
        return Some(Quality::Prefix);
    }
    let mut inside = false;
    for (at, _) in hay.match_indices(needle) {
        if word_start(hay, at) {
            return Some(Quality::WordStart);
        }
        inside = true;
    }
    if inside {
        return Some(Quality::Inside);
    }
    (fuzzy && subsequence(hay, needle).is_some()).then_some(Quality::Subsequence)
}

/// The byte offsets in `hay` of `needle`'s characters, matched in order (leftmost).
fn subsequence(hay: &str, needle: &str) -> Option<Vec<usize>> {
    let mut want = needle.chars().peekable();
    let mut at = Vec::new();
    for (n, c) in hay.char_indices() {
        match want.peek() {
            Some(w) if *w == c => {
                at.push(n);
                want.next();
            }
            Some(_) => {}
            None => break,
        }
    }
    want.peek().is_none().then_some(at)
}

/// Where `term` matches in `text`, as character ranges of `text` (sorted, not overlapping):
/// the first run that contains it, or with `fuzzy` (and no run) the characters of a
/// subsequence. Empty when it does not match. Uses the same folding as the ranking.
#[must_use]
pub fn find(text: &str, term: &Term, fuzzy: bool) -> Vec<Range<usize>> {
    let (hay, map) = query::fold_mapped(text);
    if let Some(at) = hay.find(&term.text) {
        let last = at + term.text.len() - 1;
        return std::iter::once(map[at]..map[last] + 1).collect();
    }
    if !fuzzy || term.phrase || term.text.chars().count() < 2 {
        return Vec::new();
    }
    let Some(at) = subsequence(&hay, &term.text) else {
        return Vec::new();
    };
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for byte in at {
        let c = map[byte];
        match ranges.last_mut() {
            Some(last) if last.end >= c => last.end = last.end.max(c + 1),
            _ => ranges.push(c..c + 1),
        }
    }
    ranges
}

/// Where the wanted words of `query` match in `text` (see [`find`]), merged into sorted
/// character ranges.
#[must_use]
pub fn locate(text: &str, query: &Query, fuzzy: bool) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = query
        .wanted()
        .flat_map(|term| find(text, term, fuzzy))
        .collect();
    ranges.sort_by_key(|r| (r.start, Reverse(r.end)));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        match merged.last_mut() {
            Some(last) if last.end >= r.start => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

#[cfg(test)]
mod tests;
