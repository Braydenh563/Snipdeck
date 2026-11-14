//! Where a search matched, as the pieces a row draws on the `search-mark` fill (shared-look spec
//! decision 3): the Panel, the Picker, the Main window and the command palette all call this, so
//! there is one matcher and one test set. Pure text; the hosts copy the pieces into their rows.

/// How many characters of the preview may precede a mark; more are cut and replaced by an ellipsis so the match stays in view.
const PREVIEW_LEAD: usize = 12;

/// The marks of one search result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// The first match in the title: the text before it and the match.
    pub title: Option<(String, String)>,
    /// The first match in the first line of the preview: the text before it (cut), the match and
    /// the rest of the line. Never set for a sensitive item.
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

/// The marks of a result for `query`. A sensitive item gets a title mark only: its preview and
/// tags are never shown, so they carry nothing.
#[must_use]
pub fn marks(
    title: &str,
    preview: &str,
    sensitive: bool,
    hit_tag: Option<&str>,
    query: &str,
) -> Marks {
    let title = title_mark(title, query);
    let preview = if sensitive {
        None
    } else {
        preview_mark(preview, query)
    };
    let tag = hit_tag
        .filter(|_| !sensitive && title.is_none() && preview.is_none())
        .map(str::to_owned);
    Marks {
        title,
        preview,
        tag,
    }
}

/// The first case-insensitive match of `query` in `title`: the text before it and the match, as typed in the title.
/// `None` for an empty query or when only other fields (preview, tags) matched.
#[must_use]
pub fn title_mark(title: &str, query: &str) -> Option<(String, String)> {
    let fold = |c: char| {
        let mut lower = c.to_lowercase();
        match (lower.next(), lower.next()) {
            (Some(one), None) => one,
            _ => c,
        }
    };
    let wanted: Vec<char> = query.trim().chars().map(fold).collect();
    if wanted.is_empty() {
        return None;
    }
    let chars: Vec<char> = title.chars().collect();
    let at = (0..=chars.len().checked_sub(wanted.len())?).find(|&i| {
        chars[i..i + wanted.len()]
            .iter()
            .zip(&wanted)
            .all(|(c, w)| fold(*c) == *w)
    })?;
    Some((
        chars[..at].iter().collect(),
        chars[at..at + wanted.len()].iter().collect(),
    ))
}

/// The first case-insensitive match of `query` in the first line of `preview`: the text before
/// it (cut to its last `PREVIEW_LEAD` characters after an ellipsis), the match and the rest of the line.
#[must_use]
pub fn preview_mark(preview: &str, query: &str) -> Option<(String, String, String)> {
    let line = preview.lines().next()?;
    let (before, found) = title_mark(line, query)?;
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
    query
        .split_whitespace()
        .filter_map(|word| title_mark(label, word))
        .min_by_key(|(before, _)| before.chars().count())
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
    fn preview_mark_first_line_only() {
        assert_eq!(
            preview_mark("first line\nsecond has Needle", "needle"),
            None
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
