//! In-memory fuzzy search for the Panel and Picker (SPEC §5.1, §9).
//!
//! A [`SearchIndex`] holds, per item, its title, its tag names and the first
//! [`CONTENT_CHARS`] characters of its preview, and answers [`SearchIndex::query`] with the
//! best matches. Full-content search is the main window's job (SQLite FTS5), not this
//! module's.
//!
//! **Matching.** Queries use `nucleo-matcher`'s pattern syntax limited to fuzzy atoms: words
//! separated by spaces must all match, case is ignored unless the word has a capital, and
//! accents are ignored unless the word has one. A word is matched against one field at a
//! time: `mail sig` finds "Email signature" but not an item whose title has `mail` and whose
//! tag has `sig`.
//!
//! **Ranking.** An item's field tier is the best field that matches: title, then tag, then
//! content. A title match always outranks a tag match, which always outranks a content
//! match, whatever their fuzzy scores; within a tier the fuzzy score decides, and equal
//! scores keep the order the items were added in (the user's order).
//!
//! **Order.** The index keeps items in insertion order. [`SearchIndex::upsert`] of a known
//! item keeps its place; to reorder, rebuild with [`SearchIndex::new`].
//!
//! **Tags.** [`Item`] stores tag *ids*, so the index cannot see names by itself: pass them
//! with [`SearchIndex::upsert_with_tags`]. Renaming a tag means upserting the items that
//! carry it again.

use std::cmp::Reverse;
use std::collections::HashMap;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::{Item, ItemId};

/// How many characters of an item's preview are matched (SPEC §5.1).
pub const CONTENT_CHARS: usize = 64;

/// Titles and tag names are matched up to this many characters; the rest is ignored so a
/// pathological title cannot slow every query down.
const FIELD_CHARS: usize = 256;

/// Below this many items a query is scored on the calling thread; spawning threads costs
/// more than it saves.
const PARALLEL_FROM: usize = 2_000;

/// The most scoring threads a query uses.
const MAX_THREADS: usize = 8;

const TIER_SHIFT: u32 = 24;
const SCORE_MASK: u32 = (1 << TIER_SHIFT) - 1;

/// One matching item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The matching item.
    pub id: ItemId,
    /// Higher is better. Only comparable within one query. Zero for every item of an empty
    /// query.
    pub score: u32,
    /// Which characters of the title matched, as sorted character (not byte) positions, for
    /// highlighting. Empty if the match was on a tag or the content, or for an empty query.
    pub title_indices: Vec<u32>,
}

#[derive(Debug, Clone)]
struct Entry {
    id: ItemId,
    title: String,
    tags: Vec<String>,
    content: String,
}

/// Which field matched, best last so the tier can be packed into a score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Content = 0,
    Tag = 1,
    Title = 2,
}

/// An in-memory index over a board's items for the Panel and Picker.
#[derive(Debug, Default, Clone)]
pub struct SearchIndex {
    entries: Vec<Entry>,
    positions: HashMap<ItemId, usize>,
}

fn truncated(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

impl SearchIndex {
    /// An index of `items`, in the order given, without tag names (see
    /// [`SearchIndex::upsert_with_tags`]).
    pub fn new<'a>(items: impl IntoIterator<Item = &'a Item>) -> Self {
        let mut index = Self::default();
        for item in items {
            index.upsert(item);
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

    /// Adds `item`, or replaces its entry (keeping its place) if it is already indexed.
    /// Clears its tag names; use [`SearchIndex::upsert_with_tags`] to keep them searchable.
    pub fn upsert(&mut self, item: &Item) {
        self.upsert_with_tags(item, []);
    }

    /// Like [`SearchIndex::upsert`], with the names of the item's tags.
    pub fn upsert_with_tags<'a>(&mut self, item: &Item, tags: impl IntoIterator<Item = &'a str>) {
        let entry = Entry {
            id: item.id,
            title: truncated(&item.title, FIELD_CHARS),
            tags: tags
                .into_iter()
                .map(|tag| truncated(tag, FIELD_CHARS))
                .collect(),
            content: truncated(&item.preview, CONTENT_CHARS),
        };
        let known = self
            .positions
            .get(&item.id)
            .and_then(|&at| self.entries.get_mut(at));
        if let Some(slot) = known {
            *slot = entry;
        } else {
            self.positions.insert(item.id, self.entries.len());
            self.entries.push(entry);
        }
    }

    /// Drops `id` from the index. Returns whether it was there.
    pub fn remove(&mut self, id: ItemId) -> bool {
        let Some(at) = self.positions.remove(&id) else {
            return false;
        };
        self.entries.remove(at);
        for entry in self.entries.iter().skip(at) {
            if let Some(position) = self.positions.get_mut(&entry.id) {
                *position -= 1;
            }
        }
        true
    }

    /// The best `limit` matches for `query`, best first.
    ///
    /// A blank query matches everything: the first `limit` items in index order, with a score
    /// of 0 and no highlights. Large indexes (from 2,000 items) are scored on several threads.
    pub fn query(&self, query: &str, limit: usize) -> Vec<Hit> {
        let threads = std::thread::available_parallelism().map_or(1, usize::from);
        self.run(query, limit, PARALLEL_FROM, threads.min(MAX_THREADS))
    }

    /// [`SearchIndex::query`] with the threading policy exposed for tests.
    fn run(&self, query: &str, limit: usize, parallel_from: usize, threads: usize) -> Vec<Hit> {
        if limit == 0 {
            return Vec::new();
        }
        // `Pattern` only splits on spaces; make every kind of whitespace a separator.
        let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
        let pattern = Pattern::new(
            &query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        if pattern.atoms.is_empty() {
            return self
                .entries
                .iter()
                .take(limit)
                .map(|entry| Hit {
                    id: entry.id,
                    score: 0,
                    title_indices: Vec::new(),
                })
                .collect();
        }

        let mut candidates = if self.entries.len() >= parallel_from && threads > 1 {
            self.score_parallel(&pattern, threads)
        } else {
            score_chunk(&self.entries, 0, &pattern)
        };

        // Best score first; equal scores by position, i.e. the user's order.
        let order =
            |a: &(usize, u32), b: &(usize, u32)| (Reverse(a.1), a.0).cmp(&(Reverse(b.1), b.0));
        if candidates.len() > limit {
            candidates.select_nth_unstable_by(limit - 1, order);
            candidates.truncate(limit);
        }
        candidates.sort_unstable_by(order);

        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        candidates
            .into_iter()
            .filter_map(|(at, score)| {
                let entry = self.entries.get(at)?;
                let mut title_indices = Vec::new();
                if score >> TIER_SHIFT == Tier::Title as u32 {
                    // `Utf32Str::new` counts graphemes (a combining mark joins its base letter) and,
                    // when every grapheme is ASCII, falls back to the raw bytes, so its indices
                    // can point past the title's characters. Highlighting wants character
                    // positions, so build the character haystack ourselves.
                    let haystack = if entry.title.is_ascii() {
                        Utf32Str::Ascii(entry.title.as_bytes())
                    } else {
                        buf.clear();
                        buf.extend(entry.title.chars());
                        Utf32Str::Unicode(&buf)
                    };
                    pattern.indices(haystack, &mut matcher, &mut title_indices);
                    title_indices.sort_unstable();
                    title_indices.dedup();
                }
                Some(Hit {
                    id: entry.id,
                    score,
                    title_indices,
                })
            })
            .collect()
    }

    fn score_parallel(&self, pattern: &Pattern, threads: usize) -> Vec<(usize, u32)> {
        let chunk_len = self.entries.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            let workers: Vec<_> = self
                .entries
                .chunks(chunk_len)
                .enumerate()
                .map(|(n, chunk)| scope.spawn(move || score_chunk(chunk, n * chunk_len, pattern)))
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

/// Scores `entries` (which start at index `first` of the whole index) and returns
/// `(index, score)` for those that match.
fn score_chunk(entries: &[Entry], first: usize, pattern: &Pattern) -> Vec<(usize, u32)> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut score = |text: &str| {
        pattern
            .score(Utf32Str::new(text, &mut buf), &mut matcher)
            .map(|s| s.min(SCORE_MASK))
    };
    entries
        .iter()
        .enumerate()
        .filter_map(|(n, entry)| {
            let (tier, fuzzy) = if let Some(s) = score(&entry.title) {
                (Tier::Title, s)
            } else if let Some(s) = entry.tags.iter().filter_map(|tag| score(tag)).max() {
                (Tier::Tag, s)
            } else {
                (Tier::Content, score(&entry.content)?)
            };
            Some((first + n, ((tier as u32) << TIER_SHIFT) | fuzzy))
        })
        .collect()
}

#[cfg(test)]
mod tests;
