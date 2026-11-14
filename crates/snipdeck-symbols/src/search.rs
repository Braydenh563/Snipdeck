//! Ranked search over names and keywords.
//!
//! The query and every keyword are folded once ([`crate::fold`]). The query is split on spaces and
//! every term must match (AND): as a prefix of a word of the name or keywords, or, for terms of 3+
//! characters, anywhere inside them. Names and keywords in the UI language and in English are both
//! searched; a symbol takes its best rank over the two.

use std::collections::HashMap;

use crate::catalog::{Catalog, Kind, Lang, View};
use crate::fold::fold;

/// Why a symbol matched, best first. `Browse` is an empty query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// The query is the whole name, or the pasted symbol itself.
    ExactName,
    /// The name starts with the query.
    NamePrefix,
    /// Every term starts a word of the name.
    NameWord,
    /// A keyword equals the query.
    KeywordExact,
    /// Every term starts a word of the name or keywords.
    KeywordWord,
    /// Every term (3+ characters) occurs inside the name or a keyword.
    Substring,
    /// No query: catalog order.
    Browse,
}

/// One result: the catalog index and why it matched. Results are sorted by rank, then catalog order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Index into [`Catalog::symbols`].
    pub index: usize,
    /// Why it matched.
    pub rank: Rank,
}

fn rank_view(v: &View, q: &str, terms: &[&str]) -> Option<Rank> {
    let all = |f: &dyn Fn(&str) -> bool| terms.iter().all(|t| f(t));
    if v.name == q {
        Some(Rank::ExactName)
    } else if v.name.starts_with(q) {
        Some(Rank::NamePrefix)
    } else if all(&|t| v.name_words.iter().any(|w| w.starts_with(t))) {
        Some(Rank::NameWord)
    } else if v.keywords.iter().any(|k| k == q) {
        Some(Rank::KeywordExact)
    } else if all(&|t| v.words.iter().any(|w| w.starts_with(t))) {
        Some(Rank::KeywordWord)
    } else if all(&|t| {
        t.chars().count() >= 3 && (v.name.contains(t) || v.keywords.iter().any(|k| k.contains(t)))
    }) {
        Some(Rank::Substring)
    } else {
        None
    }
}

/// A symbol's text without variation selectors, so "☺" finds "☺️".
fn plain(s: &str) -> String {
    s.chars().filter(|c| *c != '\u{FE0F}').collect()
}

/// Searches `catalog`. An empty query lists `kind` (or everything) in browse order.
#[must_use]
pub fn search(catalog: &Catalog, query: &str, lang: Lang, kind: Option<Kind>) -> Vec<Hit> {
    let in_kind = |i: usize| kind.is_none_or(|k| catalog.range(k).contains(&i));
    let q = fold(query.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if q.is_empty() {
        let range = kind.map_or(0..catalog.len(), |k| catalog.range(k));
        return range
            .map(|index| Hit {
                index,
                rank: Rank::Browse,
            })
            .collect();
    }
    let terms: Vec<&str> = q.split(' ').collect();
    let pasted = plain(query.trim());
    let names = catalog.lang_names(lang);
    let mut best: HashMap<usize, Rank> = HashMap::new();
    for (index, sym) in catalog.symbols().iter().enumerate() {
        if !in_kind(index) {
            continue;
        }
        let mut rank = None;
        if sym.text.as_deref().is_some_and(|t| plain(t) == pasted) {
            rank = Some(Rank::ExactName);
        }
        if let Some(v) = catalog.view(index) {
            rank = rank.into_iter().chain(rank_view(v, &q, &terms)).min();
        }
        if let Some(e) = names
            .and_then(|n| n.entries.get(index))
            .and_then(Option::as_ref)
        {
            rank = rank.into_iter().chain(rank_view(&e.view, &q, &terms)).min();
        }
        if let Some(r) = rank {
            best.insert(index, r);
        }
    }
    let mut hits: Vec<Hit> = best
        .into_iter()
        .map(|(index, rank)| Hit { index, rank })
        .collect();
    hits.sort_by_key(|h| (h.rank, h.index));
    hits
}

impl Catalog {
    /// [`search`] as a method.
    #[must_use]
    pub fn search(&self, query: &str, lang: Lang, kind: Option<Kind>) -> Vec<Hit> {
        search(self, query, lang, kind)
    }
}
