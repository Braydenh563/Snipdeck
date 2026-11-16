//! The search query grammar and the text folding every surface shares (SPEC §5.2).
//!
//! One parser, used by the Panel, the Picker and the Main window alike:
//!
//! | Typed               | Means                                                         |
//! |---------------------|---------------------------------------------------------------|
//! | `invoice work`      | every word must match, each in any field                      |
//! | `"kind regards"`    | the exact phrase (spaces included)                            |
//! | `-draft`            | leave out items where `draft` matches                         |
//! | `tag:billing`       | items with a tag whose name starts with `billing`             |
//! | `list:"Email replies"` | items in a list whose name starts with that              |
//! | `kind:link`         | items of one kind (`text`, `template`, `code`, `link`, `image`, `file`) |
//! | `is:pinned`         | pinned items                                                  |
//!
//! A filter can be negated too (`-tag:old`, `-is:pinned`). A filter with nothing after the
//! colon is ignored (it is being typed), and an unknown one (`std::vector`, `kind:nope`) is an
//! ordinary word. The query is cut to [`MAX_QUERY_CHARS`] characters and [`MAX_TERMS`] words
//! and filters before anything is matched, so a pasted paragraph cannot freeze a search field.
//!
//! **Folding.** [`fold`] makes text comparable: NFKC compatibility forms (full-width `ｃａｆｅ`,
//! ligatures), lower case (full folding: `ß` is `ss`), and Latin, Greek and Cyrillic
//! diacritics removed (`é` is `e`). Japanese voicing marks and the vowel signs of Indic and
//! Thai scripts are letters, not accents, and are kept. Both the indexed text and the query go
//! through it, on every surface and in the full-text index.

use unicode_normalization::UnicodeNormalization;

use crate::ItemKind;

/// The most characters of a query that are read; the rest is ignored.
pub const MAX_QUERY_CHARS: usize = 256;

/// The most words, phrases and filters of a query that are used; the rest are ignored.
pub const MAX_TERMS: usize = 32;

/// A word or a quoted phrase of a query, folded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// The folded text (see [`fold`]). Never empty.
    pub text: String,
    /// Whether it was quoted: matched as one exact run, never fuzzily.
    pub phrase: bool,
    /// Whether it was written with a leading `-`: items where it matches are left out.
    pub negated: bool,
}

impl Term {
    /// Whether the full-text index is asked about this term: it has three characters or more
    /// (the trigram index), or it is written in a script without spaces between words (CJK,
    /// Thai), where one or two characters are already a word.
    #[must_use]
    pub fn searches_body(&self) -> bool {
        self.text.chars().count() >= 3 || self.text.chars().any(is_unspaced_script)
    }
}

/// What a filter tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// `tag:x`: a tag whose folded name starts with `x`.
    Tag(String),
    /// `list:x`: a list whose folded name starts with `x`.
    List(String),
    /// `kind:x`: the item's kind.
    Kind(ItemKind),
    /// `is:pinned`.
    Pinned,
}

/// A filter of a query, maybe negated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    /// What is tested.
    pub filter: Filter,
    /// Whether it was written with a leading `-`.
    pub negated: bool,
}

/// A parsed search query: its words and phrases, and its filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Words and phrases, in the order typed.
    pub terms: Vec<Term>,
    /// Filters, in the order typed.
    pub filters: Vec<Condition>,
}

/// One whitespace-separated piece of the input, before it is classified.
struct Token {
    negated: bool,
    /// `Some` for `field:value` with a field name before the colon.
    field: Option<String>,
    text: String,
    quoted: bool,
}

impl Query {
    /// Parses what the user typed. Never fails: anything that is not an operator is a word.
    #[must_use]
    pub fn parse(input: &str) -> Self {
        let capped: String = input.chars().take(MAX_QUERY_CHARS).collect();
        let mut query = Self::default();
        for token in tokens(&capped) {
            if query.terms.len() + query.filters.len() >= MAX_TERMS {
                break;
            }
            query.push(&token);
        }
        query
    }

    fn push(&mut self, token: &Token) {
        let negated = token.negated;
        if let Some(field) = &token.field {
            let value = fold(token.text.trim());
            let filter = match field.as_str() {
                _ if value.is_empty() => return,
                "tag" => Some(Filter::Tag(value)),
                "list" => Some(Filter::List(value)),
                "kind" => ItemKind::ALL
                    .iter()
                    .find(|k| k.key() == value)
                    .map(|k| Filter::Kind(*k)),
                "is" => (value == "pinned").then_some(Filter::Pinned),
                _ => None,
            };
            if let Some(filter) = filter {
                self.filters.push(Condition { filter, negated });
                return;
            }
            // Not an operator after all: the whole token is a word.
            let quote = if token.quoted { "\"" } else { "" };
            let raw = format!("{field}:{quote}{}{quote}", token.text);
            self.push_term(&raw, false, negated);
            return;
        }
        self.push_term(&token.text, token.quoted, negated);
    }

    fn push_term(&mut self, raw: &str, phrase: bool, negated: bool) {
        let text = fold(raw.trim());
        if !text.is_empty() {
            self.terms.push(Term {
                text,
                phrase,
                negated,
            });
        }
    }

    /// Whether nothing is asked: no word, phrase or filter.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.filters.is_empty()
    }

    /// The words and phrases an item must match (not the negated ones).
    pub fn wanted(&self) -> impl Iterator<Item = &Term> {
        self.terms.iter().filter(|t| !t.negated)
    }
}

/// Splits the input into tokens: whitespace separates, `"` quotes (an unclosed quote runs to
/// the end), a leading `-` negates, and `name:` before the value names a field.
fn tokens(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at].is_whitespace() {
            at += 1;
            continue;
        }
        let mut negated = false;
        if chars[at] == '-' && chars.get(at + 1).is_some_and(|c| !c.is_whitespace()) {
            negated = true;
            at += 1;
        }
        if chars[at] == '"' {
            let (text, next) = quoted(&chars, at + 1);
            out.push(Token {
                negated,
                field: None,
                text,
                quoted: true,
            });
            at = next;
            continue;
        }
        let start = at;
        let mut colon = at;
        while colon < chars.len() && chars[colon].is_ascii_alphabetic() {
            colon += 1;
        }
        let name: String = chars[start..colon]
            .iter()
            .collect::<String>()
            .to_lowercase();
        if chars.get(colon) == Some(&':') && is_field(&name) {
            let value = colon + 1;
            if chars.get(value) == Some(&'"') {
                let (text, next) = quoted(&chars, value + 1);
                out.push(Token {
                    negated,
                    field: Some(name),
                    text,
                    quoted: true,
                });
                at = next;
            } else {
                at = run_end(&chars, value);
                out.push(Token {
                    negated,
                    field: Some(name),
                    text: chars[value..at].iter().collect(),
                    quoted: false,
                });
            }
            continue;
        }
        at = run_end(&chars, start);
        out.push(Token {
            negated,
            field: None,
            text: chars[start..at].iter().collect(),
            quoted: false,
        });
    }
    out
}

/// Where the run of non-whitespace starting at `from` ends.
fn run_end(chars: &[char], from: usize) -> usize {
    chars[from..]
        .iter()
        .position(|c| c.is_whitespace())
        .map_or(chars.len(), |n| from + n)
}

fn is_field(name: &str) -> bool {
    matches!(name, "tag" | "list" | "kind" | "is")
}

/// The text of a quote that opens just before `from`, and where reading goes on after it.
fn quoted(chars: &[char], from: usize) -> (String, usize) {
    let end = chars[from.min(chars.len())..]
        .iter()
        .position(|c| *c == '"')
        .map_or(chars.len(), |n| from + n);
    let text = chars[from.min(chars.len())..end].iter().collect();
    (text, (end + 1).min(chars.len()))
}

/// Scripts written without spaces between words, where one or two characters are a word.
#[must_use]
pub fn is_unspaced_script(c: char) -> bool {
    matches!(c,
        '\u{0E00}'..='\u{0EFF}'     // Thai, Lao
        | '\u{1000}'..='\u{109F}'   // Myanmar
        | '\u{1780}'..='\u{17FF}'   // Khmer
        | '\u{1100}'..='\u{11FF}'   // Hangul Jamo
        | '\u{2E80}'..='\u{9FFF}'   // CJK radicals, kana, CJK unified
        | '\u{A960}'..='\u{A97F}'
        | '\u{AC00}'..='\u{D7FF}'   // Hangul syllables
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FF66}'..='\u{FF9F}'   // half-width kana
        | '\u{20000}'..='\u{3FFFF}')
}

/// Accents that folding removes: the combining diacritical mark blocks (Latin, Greek,
/// Cyrillic). Marks of other scripts are letters there and stay.
fn is_diacritic(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}')
}

/// Feeds the folded form of `text` to `emit`, one folded character at a time, with the
/// position (in characters) of the original character it came from. Runs of whitespace become
/// one space; control characters are dropped.
pub(crate) fn fold_each(text: &str, mut emit: impl FnMut(usize, char)) {
    // One folded character is held back, so a combining mark that follows it as a character
    // of its own (half-width `ｶﾞ`) can still join it (`ガ`).
    let mut held: Option<(usize, char)> = None;
    let mut out = |n: usize, c: char| {
        if let Some((m, prev)) = held {
            if let Some(joined) = unicode_normalization::char::compose(prev, c) {
                held = Some((m, joined));
                return;
            }
            emit(m, prev);
        }
        held = Some((n, c));
    };
    fold_chars(text, &mut out);
    if let Some((m, prev)) = held {
        emit(m, prev);
    }
}

fn fold_chars(text: &str, mut emit: impl FnMut(usize, char)) {
    let mut last_space = false;
    let mut buf = String::new();
    for (n, c) in text.chars().enumerate() {
        if c.is_whitespace() {
            if !last_space {
                emit(n, ' ');
                last_space = true;
            }
            continue;
        }
        if c.is_control() {
            continue;
        }
        last_space = false;
        if c.is_ascii() {
            emit(n, c.to_ascii_lowercase());
            continue;
        }
        buf.clear();
        buf.extend(std::iter::once(c).nfkd().filter(|d| !is_diacritic(*d)));
        for d in buf.nfc() {
            match d {
                'ß' | 'ẞ' => {
                    emit(n, 's');
                    emit(n, 's');
                }
                'ς' => emit(n, 'σ'),
                _ => {
                    for l in d.to_lowercase().filter(|l| !is_diacritic(*l)) {
                        if l.is_whitespace() {
                            if !last_space {
                                emit(n, ' ');
                                last_space = true;
                            }
                        } else {
                            emit(n, l);
                        }
                    }
                }
            }
        }
    }
}

/// `text` folded for matching: see the module documentation.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    fold_each(text, |_, c| out.push(c));
    out
}

/// `text` folded, with the original character position of every folded byte.
pub(crate) fn fold_mapped(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len());
    fold_each(text, |n, c| {
        out.push(c);
        map.extend(std::iter::repeat_n(n, c.len_utf8()));
    });
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(q: &Query) -> Vec<(&str, bool, bool)> {
        q.terms
            .iter()
            .map(|t| (t.text.as_str(), t.phrase, t.negated))
            .collect()
    }

    #[test]
    fn folding_covers_case_accents_width_and_sharp_s() {
        assert_eq!(fold("Café Résumé"), "cafe resume");
        assert_eq!(fold("cafe\u{301}"), "cafe");
        assert_eq!(fold("ｃａｆｅ"), "cafe");
        assert_eq!(fold("STRASSE"), fold("straße"));
        assert_eq!(fold("ﬁle"), "file");
        assert_eq!(fold("ΆΣ"), "ασ");
        assert_eq!(fold("a \t\n b"), "a b");
        // Voicing marks are letters: が is not か.
        assert_ne!(fold("が"), fold("か"));
        assert_eq!(fold("ガ"), fold("ｶﾞ"), "half-width kana fold to full width");
        // Hangul stays syllables, Thai keeps its vowel signs.
        assert_eq!(fold("한국어"), "한국어");
        assert_eq!(fold("ภาษาไทย"), "ภาษาไทย");
    }

    #[test]
    fn fold_mapped_points_every_byte_at_its_source_character() {
        let (text, map) = fold_mapped("Straße é");
        assert_eq!(text, "strasse e");
        assert_eq!(map.len(), text.len());
        assert_eq!(map[4], 4, "the first s of ß");
        assert_eq!(map[5], 4, "the second s of ß");
        assert_eq!(map[8], 7);
    }

    #[test]
    fn words_phrases_and_negation() {
        let q = Query::parse("Kind  \"Best regards\" -draft - \"unclosed phrase");
        assert_eq!(
            words(&q),
            [
                ("kind", false, false),
                ("best regards", true, false),
                ("draft", false, true),
                ("-", false, false),
                ("unclosed phrase", true, false),
            ]
        );
        let q = Query::parse("-\"kind regards\"");
        assert_eq!(words(&q), [("kind regards", true, true)]);
    }

    #[test]
    fn filters_and_words_that_only_look_like_filters() {
        let q = Query::parse("tag:Billing list:\"Email replies\" kind:LINK is:pinned -tag:old");
        assert!(q.terms.is_empty(), "{q:?}");
        assert_eq!(
            q.filters,
            [
                Condition {
                    filter: Filter::Tag("billing".into()),
                    negated: false
                },
                Condition {
                    filter: Filter::List("email replies".into()),
                    negated: false
                },
                Condition {
                    filter: Filter::Kind(ItemKind::Link),
                    negated: false
                },
                Condition {
                    filter: Filter::Pinned,
                    negated: false
                },
                Condition {
                    filter: Filter::Tag("old".into()),
                    negated: true
                },
            ]
        );
        let q = Query::parse("std::vector kind:nope is:great https://x.y tag:");
        assert_eq!(
            words(&q),
            [
                ("std::vector", false, false),
                ("kind:nope", false, false),
                ("is:great", false, false),
                ("https://x.y", false, false),
            ],
            "a filter being typed (`tag:`) is ignored"
        );
        assert!(q.filters.is_empty());
    }

    #[test]
    fn the_query_is_capped_before_matching() {
        let q = Query::parse(&"word ".repeat(2_000));
        assert_eq!(q.terms.len(), MAX_TERMS);
        let q = Query::parse(&"x".repeat(6_000));
        assert_eq!(q.terms[0].text.chars().count(), MAX_QUERY_CHARS);
    }

    #[test]
    fn body_lookups_need_three_characters_or_an_unspaced_script() {
        let term = |s: &str| Query::parse(s).terms.remove(0);
        assert!(!term("ab").searches_body());
        assert!(term("abc").searches_body());
        assert!(term("東京").searches_body());
        assert!(term("語").searches_body());
        assert!(term("ไทย").searches_body());
    }
}
