//! The editor's whole-text tools: change the case, tidy lines, find and replace, number the lines.
//!
//! Pure functions over a `&str`, so the window only reads the result. Line endings are kept as they are
//! (`\n` or `\r\n`); a tool never changes what it was not asked to.

use std::ops::Range;

/// A change applied to the whole text of an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    /// Every letter in capitals.
    Upper,
    /// Every letter in small letters.
    Lower,
    /// The first letter of each word in a capital, the rest in small letters.
    Title,
    /// No spaces at the end of a line, and no blank lines before or after the text.
    Trim,
    /// A line that repeats an earlier one is dropped; blank lines are kept.
    Dedupe,
    /// Lines in alphabetical order, ignoring case.
    Sort,
}

impl Transform {
    /// Every transform, in the order the editor lists them.
    pub const ALL: [Self; 6] = [
        Self::Upper,
        Self::Lower,
        Self::Title,
        Self::Trim,
        Self::Dedupe,
        Self::Sort,
    ];

    /// `text` after the change.
    #[must_use]
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Upper => text.to_uppercase(),
            Self::Lower => text.to_lowercase(),
            Self::Title => title_case(text),
            Self::Trim => trim(text),
            Self::Dedupe => map_lines(text, |lines| {
                let mut seen = std::collections::HashSet::new();
                lines
                    .into_iter()
                    .filter(|l| l.trim().is_empty() || seen.insert(*l))
                    .collect()
            }),
            Self::Sort => map_lines(text, |mut lines| {
                lines.sort_by_cached_key(|l| (l.to_lowercase(), (*l).to_owned()));
                lines
            }),
        }
    }
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if word_start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            word_start = false;
        } else {
            // An apostrophe stays inside a word ("don't"), anything else ends it.
            if !matches!(c, '\'' | '\u{2019}') {
                word_start = true;
            }
            out.push(c);
        }
    }
    out
}

/// The text's lines without their ends, and the end it uses (`\r\n` when any line has it).
fn lines_of(text: &str) -> (Vec<&str>, &'static str, bool) {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if text.is_empty() {
        return (Vec::new(), eol, false);
    }
    let ended = text.ends_with('\n');
    let body = text.strip_suffix('\n').unwrap_or(text);
    let lines = body
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    (lines, eol, ended)
}

/// Rebuilds the text from `change` of its lines, keeping the line end and a final line end.
fn map_lines<'a>(text: &'a str, change: impl FnOnce(Vec<&'a str>) -> Vec<&'a str>) -> String {
    let (lines, eol, ended) = lines_of(text);
    let mut out = change(lines).join(eol);
    if ended {
        out.push_str(eol);
    }
    out
}

fn trim(text: &str) -> String {
    let (lines, eol, ended) = lines_of(text);
    let mut out = lines
        .iter()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join(eol);
    let leading = out.len() - out.trim_start_matches(['\r', '\n']).len();
    out.drain(..leading);
    out.truncate(out.trim_end().len());
    if ended && !out.is_empty() {
        out.push_str(eol);
    }
    out
}

fn same_char(a: char, b: char, match_case: bool) -> bool {
    a == b || (!match_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// Where `needle` is in `text`, left to right and without overlap. An empty `needle` is nowhere.
#[must_use]
pub fn find_all(text: &str, needle: &str, match_case: bool) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    if needle.is_empty() {
        return found;
    }
    let n = needle.chars().count();
    let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let mut at = 0;
    while at < starts.len() {
        let mut chars = text[starts[at]..].chars();
        let hit = needle
            .chars()
            .all(|w| chars.next().is_some_and(|c| same_char(c, w, match_case)));
        if hit {
            let end = starts.get(at + n).copied().unwrap_or(text.len());
            found.push(starts[at]..end);
            at += n;
        } else {
            at += 1;
        }
    }
    found
}

/// How many times `needle` is in `text`.
#[must_use]
pub fn count_matches(text: &str, needle: &str, match_case: bool) -> usize {
    find_all(text, needle, match_case).len()
}

/// `text` with every match of `needle` replaced by `with`, and how many were.
#[must_use]
pub fn replace_all(text: &str, needle: &str, with: &str, match_case: bool) -> (String, usize) {
    let found = find_all(text, needle, match_case);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for r in &found {
        out.push_str(&text[last..r.start]);
        out.push_str(with);
        last = r.end;
    }
    out.push_str(&text[last..]);
    (out, found.len())
}

/// The line numbers for a gutter, one per line: "1\n2\n3" for three lines. A text that ends in a line end
/// has an empty last line, which is numbered too (the caret can be on it). Empty text has one line.
#[must_use]
pub fn line_numbers(text: &str) -> String {
    let lines = text.matches('\n').count() + 1;
    let mut out = String::with_capacity(lines * 4);
    for n in 1..=lines {
        if n > 1 {
            out.push('\n');
        }
        out.push_str(&n.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_and_lower_change_every_letter() {
        assert_eq!(Transform::Upper.apply("Hello, wörld 1"), "HELLO, WÖRLD 1");
        assert_eq!(Transform::Lower.apply("Hello, WÖRLD 1"), "hello, wörld 1");
    }

    #[test]
    fn title_case_capitalises_words_and_keeps_apostrophes_inside_them() {
        assert_eq!(
            Transform::Title.apply("the QUICK brown-fox don't\nstop"),
            "The Quick Brown-Fox Don't\nStop"
        );
        assert_eq!(Transform::Title.apply(""), "");
    }

    #[test]
    fn trim_drops_trailing_spaces_and_blank_edges_but_keeps_indentation() {
        assert_eq!(
            Transform::Trim.apply("\n\n  one  \n\ttwo\t\n\n\n"),
            "  one\n\ttwo\n"
        );
        assert_eq!(Transform::Trim.apply("a  \r\nb \r\n"), "a\r\nb\r\n");
        assert_eq!(Transform::Trim.apply("  \n \n"), "");
        assert_eq!(Transform::Trim.apply("no end  "), "no end");
    }

    #[test]
    fn dedupe_keeps_the_first_of_each_line_and_every_blank_line() {
        assert_eq!(
            Transform::Dedupe.apply("a\nb\na\n\nc\nb\n\nA\n"),
            "a\nb\n\nc\n\nA\n"
        );
        assert_eq!(Transform::Dedupe.apply("x\r\nx\r\ny"), "x\r\ny");
        assert_eq!(Transform::Dedupe.apply(""), "");
    }

    #[test]
    fn sort_orders_lines_ignoring_case_and_keeps_the_final_line_end() {
        assert_eq!(
            Transform::Sort.apply("pear\nApple\nfig\n"),
            "Apple\nfig\npear\n"
        );
        assert_eq!(Transform::Sort.apply("b\r\na"), "a\r\nb");
        // Same letters in another case: a stable, repeatable order.
        assert_eq!(Transform::Sort.apply("b\nB\na"), "a\nB\nb");
        assert_eq!(Transform::Sort.apply("one"), "one");
    }

    #[test]
    fn a_transform_twice_is_the_same_as_once_for_the_idempotent_ones() {
        let text = " Zed \n alpha\n\nalpha\n";
        for t in [
            Transform::Upper,
            Transform::Lower,
            Transform::Title,
            Transform::Trim,
            Transform::Dedupe,
            Transform::Sort,
        ] {
            let once = t.apply(text);
            assert_eq!(t.apply(&once), once, "{t:?}");
        }
    }

    #[test]
    fn find_counts_without_overlap_and_ignores_case_unless_asked() {
        assert_eq!(count_matches("aaaa", "aa", true), 2);
        assert_eq!(count_matches("Cat cat CAT", "cat", false), 3);
        assert_eq!(count_matches("Cat cat CAT", "cat", true), 1);
        assert_eq!(count_matches("anything", "", true), 0);
        assert_eq!(count_matches("", "x", false), 0);
        assert_eq!(count_matches("naïve NAÏVE", "naïve", false), 2);
    }

    #[test]
    fn find_gives_byte_ranges_on_character_boundaries() {
        let text = "é-é";
        let r = find_all(text, "é", true);
        assert_eq!(r, vec![0..2, 3..5]);
        assert_eq!(&text[r[1].clone()], "é");
    }

    #[test]
    fn replace_all_replaces_each_match_and_says_how_many() {
        assert_eq!(
            replace_all("one Two one", "one", "1", false),
            ("1 Two 1".to_owned(), 2)
        );
        assert_eq!(
            replace_all("One one", "one", "1", true),
            ("One 1".to_owned(), 1)
        );
        assert_eq!(replace_all("a.b.c", ".", "", true), ("abc".to_owned(), 2));
        assert_eq!(
            replace_all("keep", "zzz", "x", true),
            ("keep".to_owned(), 0)
        );
        assert_eq!(replace_all("keep", "", "x", true), ("keep".to_owned(), 0));
        // The replacement is not searched again.
        assert_eq!(replace_all("aa", "a", "aa", true), ("aaaa".to_owned(), 2));
    }

    #[test]
    fn line_numbers_number_every_line_including_the_empty_last_one() {
        assert_eq!(line_numbers(""), "1");
        assert_eq!(line_numbers("a"), "1");
        assert_eq!(line_numbers("a\nb\nc"), "1\n2\n3");
        assert_eq!(line_numbers("a\n"), "1\n2");
        assert_eq!(line_numbers("a\r\nb"), "1\n2");
    }
}
