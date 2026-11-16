//! Ranked search over names and keywords.
//!
//! The query and every keyword are folded once ([`crate::fold`]). The query is split on spaces and
//! every term must match (AND): as a prefix of a word of the name or keywords, or, for terms of 3+
//! characters (1+ for Han, kana and Hangul, which have no spaces), anywhere inside them. Names and
//! keywords in the UI language and in English are both searched; a symbol takes its best rank over
//! the two.
//!
//! Beyond that, a query can be:
//! - written without spaces (`thumbsup`, `tableflip`),
//! - one edit away from a word of 5+ letters (`smlie`),
//! - a code point (`U+1F600`, `1f600`, `0x2192`),
//! - an ASCII emoticon (`:)`, `<3`), or
//! - a shortcode (`:thumbs_up:`).

use std::collections::HashMap;

use crate::catalog::{Catalog, Kind, Lang, View};
use crate::fold::{contains, fold, is_cjk, starts_with};

/// Why a symbol matched, best first. `Browse` is an empty query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// The query is the whole name, or the pasted symbol itself.
    ExactName,
    /// The name starts with the query.
    NamePrefix,
    /// Every term starts a word of the name.
    NameWord,
    /// A keyword equals the query, or the query is the symbol's code point.
    KeywordExact,
    /// Every term starts a word of the name or keywords.
    KeywordWord,
    /// Every term (3+ characters, or any CJK character) occurs inside the name or a keyword.
    Substring,
    /// One typo away from a word of the name or keywords.
    Fuzzy,
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

/// Typed faces and their emoji (the CLDR name). The query is compared as typed, lowercase.
const EMOTICONS: &[(&str, &str)] = &[
    (":)", "slightly smiling face"),
    (":-)", "slightly smiling face"),
    ("=)", "slightly smiling face"),
    (":d", "grinning face with big eyes"),
    (":-d", "grinning face with big eyes"),
    ("xd", "grinning squinting face"),
    (";)", "winking face"),
    (";-)", "winking face"),
    (":(", "frowning face"),
    (":-(", "frowning face"),
    (":'(", "crying face"),
    (":p", "face with tongue"),
    (":-p", "face with tongue"),
    (":o", "face with open mouth"),
    (":-o", "face with open mouth"),
    (":/", "confused face"),
    (":-/", "confused face"),
    (":|", "neutral face"),
    (":-|", "neutral face"),
    (":*", "kissing face"),
    ("8)", "smiling face with sunglasses"),
    ("b)", "smiling face with sunglasses"),
    ("<3", "red heart"),
    ("</3", "broken heart"),
    ("o:)", "smiling face with halo"),
    (">:(", "angry face"),
    ("^_^", "smiling face with smiling eyes"),
];

fn rank_view(v: &View, q: &str, terms: &[&str]) -> Option<Rank> {
    let all = |f: &dyn Fn(&str) -> bool| terms.iter().all(|t| f(t));
    let compact: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    if v.name == q {
        Some(Rank::ExactName)
    } else if starts_with(&v.name, q) {
        Some(Rank::NamePrefix)
    } else if terms.len() == 1 && compact.chars().count() >= 4 && v.name_compact == compact {
        Some(Rank::ExactName)
    } else if all(&|t| v.name_words.iter().any(|w| starts_with(w, t))) {
        Some(Rank::NameWord)
    } else if v.keywords.iter().any(|k| k == q) {
        Some(Rank::KeywordExact)
    } else if compact.chars().count() >= 4
        && (starts_with(&v.name_compact, &compact)
            || v.keywords_compact.iter().any(|k| *k == compact))
    {
        Some(Rank::KeywordExact)
    } else if all(&|t| v.words.iter().any(|w| starts_with(w, t))) {
        Some(Rank::KeywordWord)
    } else if all(&|t| {
        (t.chars().count() >= 3 || t.chars().all(is_cjk))
            && (contains(&v.name, t) || v.keywords.iter().any(|k| contains(k, t)))
    }) {
        Some(Rank::Substring)
    } else if terms.len() == 1 && fuzzy(v, terms[0]) {
        Some(Rank::Fuzzy)
    } else {
        None
    }
}

/// True when `a` and `b` differ by at most one insertion, deletion, substitution or swap of two
/// neighbouring letters.
fn within_one(a: &[char], b: &[char]) -> bool {
    let (la, lb) = (a.len(), b.len());
    if la.abs_diff(lb) > 1 {
        return false;
    }
    let mut i = 0;
    while i < la.min(lb) && a[i] == b[i] {
        i += 1;
    }
    if i == la && i == lb {
        return true;
    }
    if la == lb {
        // One substitution, or two neighbours swapped.
        a[i + 1..] == b[i + 1..] || (i + 1 < la && a[i] == b[i + 1] && a[i + 1] == b[i] && a[i + 2..] == b[i + 2..])
    } else if la > lb {
        a[i + 1..] == b[i..]
    } else {
        a[i..] == b[i + 1..]
    }
}

/// One typo (see [`within_one`]) between a query of 5+ letters and a word of the name or keywords,
/// or the start of one.
fn fuzzy(v: &View, term: &str) -> bool {
    let t: Vec<char> = term.chars().collect();
    if t.len() < 5 || !t.iter().all(|c| c.is_alphabetic() && !is_cjk(*c)) {
        return false;
    }
    let near = |w: &str| {
        let w: Vec<char> = w.chars().collect();
        within_one(&t, &w)
            || [t.len() - 1, t.len(), t.len() + 1]
                .iter()
                .any(|n| w.len() > *n && within_one(&t, &w[..*n]))
    };
    v.words.iter().any(|w| near(w)) || near(&v.name_compact)
}

/// A symbol's text without variation selectors, so "☺" finds "☺️".
fn plain(s: &str) -> String {
    s.chars().filter(|c| *c != '\u{FE0F}').collect()
}

/// The single code point a query names: `U+1F600`, `u+2192`, `0x2192`, `→`, or four to six
/// bare hex digits that include a digit (`1f600`; "face" and "cafe" are words, not code points).
fn code_point(query: &str) -> Option<char> {
    let q = query.trim().to_ascii_lowercase();
    let (hex, prefixed) = ["u+", "0x", "\\u"]
        .iter()
        .find_map(|p| q.strip_prefix(p))
        .map_or((q.as_str(), false), |h| (h, true));
    let ok_len = if prefixed { 2..=6 } else { 4..=6 };
    if !ok_len.contains(&hex.len())
        || !hex.chars().all(|c| c.is_ascii_hexdigit())
        || (!prefixed && !hex.chars().any(|c| c.is_ascii_digit()))
    {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
}

/// `:thumbs_up:` or `:smi` as the words `thumbs up` and `smi`; `None` when the query is not a
/// shortcode (it must start with a colon followed by a letter).
fn shortcode(query: &str) -> Option<String> {
    let rest = query.trim().strip_prefix(':')?;
    let rest = rest.strip_suffix(':').unwrap_or(rest);
    let mut chars = rest.chars();
    if !chars.next().is_some_and(char::is_alphanumeric) {
        return None;
    }
    Some(rest.replace(['_', '-'], " "))
}

/// Searches `catalog`. An empty query lists `kind` (or everything) in browse order.
#[must_use]
pub fn search(catalog: &Catalog, query: &str, lang: Lang, kind: Option<Kind>) -> Vec<Hit> {
    let in_kind = |i: usize| kind.is_none_or(|k| catalog.range(k).contains(&i));
    let typed = query.trim().to_lowercase();
    let emoticon = EMOTICONS
        .iter()
        .find(|(face, _)| *face == typed)
        .map(|(_, name)| *name);
    let shortcoded = if emoticon.is_some() { None } else { shortcode(query) };
    let text = if emoticon.is_some() {
        ""
    } else {
        shortcoded.as_deref().unwrap_or(query)
    };
    let q = fold(text.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if q.is_empty() && emoticon.is_none() {
        let range = kind.map_or(0..catalog.len(), |k| catalog.range(k));
        return range
            .map(|index| Hit {
                index,
                rank: Rank::Browse,
            })
            .collect();
    }
    let terms: Vec<&str> = q.split(' ').filter(|t| !t.is_empty()).collect();
    let pasted = plain(query.trim());
    let point = code_point(query).map(|c| c.to_string());
    let names = catalog.lang_names(lang);
    let mut best: HashMap<usize, Rank> = HashMap::new();
    for (index, sym) in catalog.symbols().iter().enumerate() {
        if !in_kind(index) {
            continue;
        }
        let mut rank = None;
        if let Some(t) = sym.text.as_deref() {
            let p = plain(t);
            if p == pasted {
                rank = Some(Rank::ExactName);
            } else if point.as_deref() == Some(p.as_str()) {
                rank = Some(Rank::KeywordExact);
            }
        }
        if sym.kind == Kind::Emoji && emoticon == Some(sym.name.as_str()) {
            rank = Some(Rank::ExactName);
        }
        if !terms.is_empty() {
            if let Some(v) = catalog.view(index) {
                rank = rank.into_iter().chain(rank_view(v, &q, &terms)).min();
            }
            if let Some(e) = names
                .and_then(|n| n.entries.get(index))
                .and_then(Option::as_ref)
            {
                rank = rank.into_iter().chain(rank_view(&e.view, &q, &terms)).min();
            }
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
