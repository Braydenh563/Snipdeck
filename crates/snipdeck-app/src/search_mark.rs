//! Where a search matched, as the pieces a row draws on the `search-mark` fill (shared-look spec
//! decision 3): the Panel, the Picker, the Main window and the command palette all call this, so
//! there is one matcher and one test set. Pure text; the hosts copy the pieces into their rows.

use std::cmp::Reverse;

use snipdeck_core::search::{Query, find, locate};

/// How many characters of the preview may precede a mark; more are cut and replaced by an ellipsis so the match stays in view.
const PREVIEW_LEAD: usize = 12;

/// The marks of one search result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// The longest match in the title: the text before it and the match.
    pub title: Option<(String, String)>,
    /// The first line of the preview with a match: the text before it (cut), the match and the
    /// rest of the line. Never set for a sensitive item.
    pub preview: Option<(String, String, String)>,
    /// The tag that matched, when neither the title nor the preview shows the match ("tag: x").
    /// Never set for a sensitive item.
    pub tag: Option<String>,
}

/// The marks as the strings a row's properties hold: what the Slint rows (`mark-before`, `mark-text`,
/// `preview-before`, `preview-match`, `tag-hit`) are filled from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkFields {
    /// The title before the match.
    pub mark_before: String,
    /// The title's match.
    pub mark_text: String,
    /// The windowed first line of the preview (before, match and rest), when the preview is marked.
    pub preview_line: Option<String>,
    /// The preview before the match.
    pub preview_before: String,
    /// The preview's match.
    pub preview_match: String,
    /// The tag that matched.
    pub tag_hit: String,
}

impl Marks {
    /// The marks as row strings.
    #[must_use]
    pub fn fields(&self) -> MarkFields {
        let (mark_before, mark_text) = self.title.clone().unwrap_or_default();
        let (preview_line, preview_before, preview_match) = match &self.preview {
            Some((before, found, after)) => (
                Some(format!("{before}{found}{after}")),
                before.clone(),
                found.clone(),
            ),
            None => (None, String::new(), String::new()),
        };
        MarkFields {
            mark_before,
            mark_text,
            preview_line,
            preview_before,
            preview_match,
            tag_hit: self.tag.clone().unwrap_or_default(),
        }
    }
}

/// The marks of a result for `query`, from the same matcher that ranked it
/// ([`snipdeck_core::search::locate`]): folded, word by word, fuzzy in the title. A sensitive
/// item gets a title mark only: its preview and tags are never shown, so they carry nothing.
/// `hit_tag` is the tag the search matched; it is shown when some word of the query is in
/// neither the title nor the preview.
#[must_use]
pub fn marks(
    title: &str,
    preview: &str,
    sensitive: bool,
    hit_tag: Option<&str>,
    query: &str,
) -> Marks {
    let query = Query::parse(query);
    let title_mark = mark_in(title, &query, true);
    let preview_mark = if sensitive {
        None
    } else {
        preview_mark_of(preview, &query)
    };
    let unshown = query.wanted().any(|term| {
        find(title, term, true).is_empty()
            && (sensitive
                || preview
                    .lines()
                    .all(|line| find(line, term, false).is_empty()))
    });
    let tag = hit_tag.filter(|_| !sensitive && unshown).map(str::to_owned);
    Marks {
        title: title_mark,
        preview: preview_mark,
        tag,
    }
}

/// The longest run `query` matches in `text` (the earliest of equal ones): the text before it
/// and the run, as written in `text`.
fn mark_in(text: &str, query: &Query, fuzzy: bool) -> Option<(String, String)> {
    let best = locate(text, query, fuzzy)
        .into_iter()
        .min_by_key(|r| (Reverse(r.len()), r.start))?;
    let before = text.chars().take(best.start).collect();
    let found = text.chars().skip(best.start).take(best.len()).collect();
    Some((before, found))
}

/// The mark in `title` for `query`, as [`marks`] makes it: the text before it and the match.
/// `None` for an empty query or when only other fields (preview, tags) matched.
#[must_use]
pub fn title_mark(title: &str, query: &str) -> Option<(String, String)> {
    mark_in(title, &Query::parse(query), true)
}

/// The mark in the first line of `preview` that `query` matches: the text before it (cut to
/// its last `PREVIEW_LEAD` characters after an ellipsis), the match and the rest of the line.
#[must_use]
pub fn preview_mark(preview: &str, query: &str) -> Option<(String, String, String)> {
    preview_mark_of(preview, &Query::parse(query))
}

fn preview_mark_of(preview: &str, query: &Query) -> Option<(String, String, String)> {
    let (line, (before, found)) = preview
        .lines()
        .find_map(|line| Some((line, mark_in(line, query, false)?)))?;
    let after: String = line
        .chars()
        .skip(before.chars().count() + found.chars().count())
        .collect();
    let lead = before.chars().count();
    let before = if lead > PREVIEW_LEAD {
        let tail: String = before.chars().skip(lead - PREVIEW_LEAD).collect();
        format!("\u{2026}{tail}")
    } else {
        before
    };
    Some((before, found, after))
}

/// The earliest match of any word of `query` in `label`, for the command palette (whose query is
/// split into words, each of which must match somewhere): the text before it and the match.
#[must_use]
pub fn word_mark(label: &str, query: &str) -> Option<(String, String)> {
    let first = locate(label, &Query::parse(query), false)
        .into_iter()
        .next()?;
    Some((
        label.chars().take(first.start).collect(),
        label.chars().skip(first.start).take(first.len()).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_mark_case_insensitive() {
        assert_eq!(
            title_mark("Q3 Report final", "report"),
            Some(("Q3 ".into(), "Report".into())),
            "the match keeps the title's own case"
        );
        assert_eq!(title_mark("Thanks", "xyz"), None);
        assert_eq!(title_mark("Thanks", ""), None);
        assert_eq!(title_mark("ab", "abc"), None);
    }

    #[test]
    fn preview_mark_shows_the_first_line_that_matches() {
        assert_eq!(
            preview_mark("first line\nsecond has Needle", "needle"),
            Some(("second has ".into(), "Needle".into(), String::new()))
        );
        assert_eq!(
            preview_mark("a long run of words before the Needle here", "needle"),
            Some((
                "\u{2026} before the ".into(),
                "Needle".into(),
                " here".into()
            ))
        );
    }

    #[test]
    fn tag_hit_when_only_tag_matched() {
        let only_tag = marks("Invoice", "pay now", false, Some("billing"), "billing");
        assert_eq!(only_tag.tag.as_deref(), Some("billing"));
        assert_eq!((only_tag.title, only_tag.preview), (None, None));
        let in_title = marks("Billing run", "x", false, Some("billing"), "billing");
        assert_eq!(in_title.tag, None, "the title already shows the match");
        let in_preview = marks("Run", "send billing", false, Some("billing"), "billing");
        assert_eq!(in_preview.tag, None, "so does the preview");
    }

    #[test]
    fn no_mark_for_sensitive() {
        let m = marks("Hunter login", "hunter2", true, Some("hunter"), "hunter");
        assert_eq!(m.preview, None);
        assert_eq!(m.tag, None);
        assert_eq!(
            m.title,
            Some((String::new(), "Hunter".into())),
            "the title is shown, so it is marked"
        );
    }

    #[test]
    fn fields_split_the_marks_into_row_strings() {
        let m = marks("Q3 Report", "see the report today", false, None, "report");
        let f = m.fields();
        assert_eq!(
            (f.mark_before.as_str(), f.mark_text.as_str()),
            ("Q3 ", "Report")
        );
        assert_eq!(f.preview_line.as_deref(), Some("see the report today"));
        assert_eq!(
            (f.preview_before.as_str(), f.preview_match.as_str()),
            ("see the ", "report")
        );
        assert_eq!(f.tag_hit, "");
        let tagged = marks("Invoice", "pay", false, Some("billing"), "billing").fields();
        assert_eq!(tagged.tag_hit, "billing");
        assert_eq!(Marks::default().fields(), MarkFields::default());
    }

    /// LIBRARY-6: a multi-word, accent-folded, full-width or fuzzy hit is marked where the
    /// matcher found it.
    #[test]
    fn every_kind_of_match_is_marked() {
        assert_eq!(
            title_mark("Invoice template", "template invoice"),
            Some((String::new(), "Invoice".into())),
            "each word is found; the longest run is marked"
        );
        assert_eq!(
            title_mark("Café résumé", "RESUME"),
            Some(("Café ".into(), "résumé".into()))
        );
        assert_eq!(
            title_mark("Café", "ｃａｆｅ"),
            Some((String::new(), "Café".into()))
        );
        assert_eq!(
            title_mark("Email signature", "emsig"),
            Some(("Email ".into(), "sig".into())),
            "a subsequence marks its longest run"
        );
        assert_eq!(
            title_mark("Invoice", "-invoice"),
            None,
            "a negated word marks nothing"
        );
        let m = marks("Invoice", "pay soon", false, Some("work"), "invoice work");
        assert_eq!(
            m.tag.as_deref(),
            Some("work"),
            "the title shows one word, the tag explains the other"
        );
    }

    #[test]
    fn palette_marks_label_prefix() {
        assert_eq!(
            word_mark("New item", "new"),
            Some((String::new(), "New".into()))
        );
        assert_eq!(
            word_mark("Open Picker", "pick open"),
            Some((String::new(), "Open".into())),
            "the earliest word wins"
        );
        assert_eq!(word_mark("Settings", "emoji"), None, "synonym-only hit");
        assert_eq!(word_mark("Settings", "  "), None);
    }
}
