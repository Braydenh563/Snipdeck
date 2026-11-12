//! The editor's Preview: what the item being edited will look like as a card, built from the
//! unsaved fields alone. Pure data and a fixed `now` passed in; nothing is stored, logged or
//! copied, and nothing here reads the clock, the clipboard or the disk.
//!
//! The card fields follow the Panel and Main rows: the title (or the first line of the text
//! when the title is empty), the preview excerpt ([`Item::preview_of`]), the kind, the colour
//! label. A Template also gets its expansion with sample values (the form's defaults, a
//! `‹Name›` stand-in for a field with none), a Link gets its resolved address.

use snipdeck_core::{
    Feature, Item, ItemId, ItemKind, ItemLabel, Timestamp,
    template::{Locale, Template, TimeZone},
};

use crate::{
    fill::{FieldKind, FillModel},
    item_editor::{self, EditorInput, EditorProblem},
};

/// Words the preview puts into sample output; the caller passes them translated.
#[derive(Debug, Clone, Copy)]
pub struct PreviewWords<'a> {
    /// Stands for a fresh `{uuid}`.
    pub new_id: &'a str,
}

/// A link split for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkDisplay {
    /// The scheme, lower case (`https`, `mailto`).
    pub scheme: String,
    /// The host without credentials or port (`example.com`); the address itself for schemes
    /// without a host (`mailto:`).
    pub host: String,
}

/// One item as its card will show it.
#[derive(Debug, Clone, PartialEq)]
pub struct CardPreview {
    /// The kind (the badge and the tile).
    pub kind: ItemKind,
    /// The title, or the first line of the text when the title is blank.
    pub title: String,
    /// The preview line(s) the card shows; empty for a sensitive item (the card masks it).
    pub excerpt: String,
    /// The colour label, if one is chosen.
    pub label: Option<ItemLabel>,
    /// The card masks the excerpt.
    pub sensitive: bool,
    /// A Template with its placeholders filled by sample values.
    pub expanded: Option<String>,
    /// A Link, split for display.
    pub link: Option<LinkDisplay>,
    /// Why the expansion or the link cannot be shown (the same refusal Save would give).
    pub problem: Option<EditorProblem>,
}

/// The card for `input`. `now` is the moment `{date}` and `{time}` show; `tz` and `locale` are
/// the user's. `prompted_allowed` is false when Save would refuse a prompted placeholder here
/// (a free user who just added one): the card then says so instead of expanding it.
#[must_use]
pub fn card_of(
    input: &EditorInput,
    prompted_allowed: bool,
    now: Timestamp,
    tz: &TimeZone,
    locale: Locale,
    words: PreviewWords<'_>,
) -> CardPreview {
    let text_kind = item_editor::is_text_kind(input.kind);
    let sensitive = input.sensitive && text_kind;
    let text = if text_kind {
        input.content.as_str()
    } else {
        ""
    };
    let excerpt = Item::preview_of(text, sensitive);
    // Untitled, the card shows the first line of the stored preview: so cut like the lists cut
    // it, and empty for a sensitive item (its secret is never the title).
    let title = if input.title.trim().is_empty() {
        excerpt.lines().next().unwrap_or_default().to_owned()
    } else {
        input.title.trim().to_owned()
    };
    let mut card = CardPreview {
        kind: input.kind,
        title,
        excerpt,
        label: input.label,
        sensitive,
        expanded: None,
        link: None,
        problem: None,
    };
    match input.kind {
        ItemKind::Link => match item_editor::check_link(text) {
            // A sensitive link's host is as secret as the link.
            Ok(url) if !sensitive => card.link = Some(link_display(&url)),
            Ok(_) => {}
            Err(p) if !text.trim().is_empty() => card.problem = Some(p),
            Err(_) => {}
        },
        ItemKind::Template if !text.trim().is_empty() => match Template::parse(text) {
            Ok(_) if item_editor::check_template(text, prompted_allowed, true).is_err() => {
                card.problem = Some(EditorProblem::NeedsPro(Feature::PromptedFields));
            }
            Ok(template) if !sensitive => {
                card.expanded = Some(expand(&template, now, tz, locale, words));
            }
            Ok(_) => {}
            Err(e) => card.problem = Some(EditorProblem::Template(e.kind.to_string())),
        },
        _ => {}
    }
    card
}

/// The template with each prompt answered by its default, or `‹Name›` where it has none.
fn expand(
    template: &Template,
    now: Timestamp,
    tz: &TimeZone,
    locale: Locale,
    words: PreviewWords<'_>,
) -> String {
    let mut form = FillModel::new(
        ItemId::generate(),
        snipdeck_core::UseMode::Paste,
        String::new(),
        template.clone(),
    );
    let empty: Vec<usize> = form
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            f.value.is_empty() && matches!(f.kind, FieldKind::Text | FieldKind::Multiline)
        })
        .map(|(i, _)| i)
        .collect();
    for i in empty {
        let stand_in = format!("‹{}›", form.fields()[i].label);
        form.set_value(i, &stand_in);
    }
    let text = form.preview(now, tz, locale);
    text.replace(&crate::fill::preview_uuid(), words.new_id)
}

/// A web or mail address split for display.
fn link_display(url: &str) -> LinkDisplay {
    let (scheme, rest) = url.split_once(':').unwrap_or(("", url));
    let scheme = scheme.to_ascii_lowercase();
    let Some(after) = rest.strip_prefix("//") else {
        return LinkDisplay {
            scheme,
            host: rest.to_owned(),
        };
    };
    let authority = after.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if host_port.starts_with('[') {
        host_port
            .split_once(']')
            .map_or(host_port, |(h, _)| h)
            .trim_start_matches('[')
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    LinkDisplay {
        scheme,
        host: host.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORDS: PreviewWords<'static> = PreviewWords { new_id: "(new ID)" };

    /// A fixed moment: 2026-03-04 05:06:07 UTC.
    fn now() -> Timestamp {
        Timestamp::from_second(1_772_600_767).expect("a moment")
    }

    fn input(kind: ItemKind, title: &str, content: &str) -> EditorInput {
        EditorInput {
            kind,
            title: title.to_owned(),
            content: content.to_owned(),
            sensitive: false,
            list_index: 0,
            source_path: String::new(),
            label: None,
        }
    }

    fn card(input: &EditorInput) -> CardPreview {
        card_of(input, true, now(), &TimeZone::UTC, Locale::default(), WORDS)
    }

    #[test]
    fn the_title_is_the_first_line_when_blank() {
        let c = card(&input(ItemKind::Text, "  ", "\n  hello\nworld"));
        assert_eq!(c.title, "hello");
        assert_eq!(c.excerpt, "hello\nworld");
        let c = card(&input(ItemKind::Text, " Greeting ", "hello"));
        assert_eq!(c.title, "Greeting");
    }

    #[test]
    fn the_excerpt_is_cut_like_a_stored_preview() {
        let long = "x".repeat(Item::PREVIEW_CHARS + 50);
        let c = card(&input(ItemKind::Text, "t", &long));
        assert_eq!(c.excerpt.chars().count(), Item::PREVIEW_CHARS);
    }

    #[test]
    fn a_sensitive_item_shows_no_excerpt() {
        let mut i = input(ItemKind::Text, "Card", "4111 1111 1111 1111");
        i.sensitive = true;
        let c = card(&i);
        assert!(c.sensitive);
        assert_eq!(c.excerpt, "");
        assert_eq!(c.title, "Card");
    }

    #[test]
    fn colour_and_kind_carry_over() {
        let mut i = input(ItemKind::Code, "c", "let x = 1;");
        i.label = Some(ItemLabel::Red);
        let c = card(&i);
        assert_eq!((c.kind, c.label), (ItemKind::Code, Some(ItemLabel::Red)));
        assert!(c.expanded.is_none() && c.link.is_none());
    }

    #[test]
    fn a_template_expands_with_defaults_the_fixed_clock_and_stand_ins() {
        let t = "Hi {field:Name}, on {date} at {time}. Re: {field:Topic|Rust} {choice:Tone|kind|firm} {uuid}";
        let c = card(&input(ItemKind::Template, "", t));
        assert_eq!(
            c.expanded.as_deref(),
            Some("Hi ‹Name›, on 2026-03-04 at 05:06. Re: Rust kind (new ID)")
        );
        assert!(c.problem.is_none());
        // The card itself still shows the source text.
        assert_eq!(c.excerpt, t);
    }

    #[test]
    fn a_template_that_does_not_parse_says_why_instead_of_expanding() {
        let c = card(&input(ItemKind::Template, "t", "Hi {field:}"));
        assert!(c.expanded.is_none());
        assert!(matches!(c.problem, Some(EditorProblem::Template(_))));
    }

    #[test]
    fn an_empty_template_or_link_is_not_a_problem_yet() {
        for kind in [ItemKind::Template, ItemKind::Link] {
            let c = card(&input(kind, "t", "  "));
            assert!(c.problem.is_none() && c.expanded.is_none() && c.link.is_none());
        }
    }

    #[test]
    fn a_link_resolves_to_its_host() {
        let c = card(&input(
            ItemKind::Link,
            "",
            " HTTPS://user:pw@Docs.Example.com:8080/a/b?q=1#x ",
        ));
        assert_eq!(
            c.link,
            Some(LinkDisplay {
                scheme: "https".into(),
                host: "Docs.Example.com".into()
            })
        );
        let c = card(&input(ItemKind::Link, "", "mailto:me@example.com"));
        assert_eq!(c.link.map(|l| l.host).as_deref(), Some("me@example.com"));
        let c = card(&input(ItemKind::Link, "", "http://[::1]:80/x"));
        assert_eq!(c.link.map(|l| l.host).as_deref(), Some("::1"));
    }

    #[test]
    fn a_bad_link_gives_the_refusal_save_would_give() {
        let c = card(&input(ItemKind::Link, "", "example.com"));
        assert_eq!(c.problem, Some(EditorProblem::LinkNeedsScheme));
        let c = card(&input(ItemKind::Link, "", "https://a b"));
        assert_eq!(c.problem, Some(EditorProblem::LinkHasSpace));
    }

    #[test]
    fn an_image_or_file_has_no_text_card_fields() {
        let c = card(&input(ItemKind::File, "", "ignored"));
        assert_eq!((c.excerpt.as_str(), c.title.as_str()), ("", ""));
    }
}

#[cfg(test)]
mod secret_tests {
    use super::*;

    fn sensitive(kind: ItemKind, title: &str, content: &str) -> CardPreview {
        let input = EditorInput {
            kind,
            title: title.to_owned(),
            content: content.to_owned(),
            sensitive: true,
            list_index: 0,
            source_path: String::new(),
            label: None,
        };
        card_of(
            &input,
            true,
            Timestamp::from_second(1_772_600_767).expect("a moment"),
            &TimeZone::UTC,
            Locale::default(),
            PreviewWords { new_id: "(new ID)" },
        )
    }

    /// Nothing the card carries may contain the secret.
    fn assert_no_secret(c: &CardPreview, secret: &str) {
        let all = format!("{c:?}");
        assert!(!all.contains(secret), "the secret leaked into {all}");
    }

    #[test]
    fn an_untitled_sensitive_text_has_no_title_from_its_secret() {
        let c = sensitive(ItemKind::Text, "", "hunter2-secret\nmore");
        assert_eq!(c.title, "");
        assert_no_secret(&c, "hunter2");
        assert_eq!(sensitive(ItemKind::Text, "Bank", "hunter2").title, "Bank");
    }

    #[test]
    fn a_sensitive_template_shows_no_expansion() {
        let c = sensitive(ItemKind::Template, "", "pw {field:Who|hunter2-secret}");
        assert!(c.expanded.is_none());
        assert_no_secret(&c, "hunter2");
    }

    #[test]
    fn a_sensitive_link_shows_no_host() {
        let c = sensitive(ItemKind::Link, "", "https://hunter2-secret.example/x");
        assert!(c.link.is_none());
        assert_no_secret(&c, "hunter2");
    }

    #[test]
    fn a_new_prompted_placeholder_is_refused_for_a_free_user_like_save() {
        let input = EditorInput {
            kind: ItemKind::Template,
            title: "t".to_owned(),
            content: "Dear {field:Name}".to_owned(),
            sensitive: false,
            list_index: 0,
            source_path: String::new(),
            label: None,
        };
        let c = card_of(
            &input,
            false,
            Timestamp::from_second(1_772_600_767).expect("a moment"),
            &TimeZone::UTC,
            Locale::default(),
            PreviewWords { new_id: "(new ID)" },
        );
        assert_eq!(
            c.problem,
            Some(EditorProblem::NeedsPro(Feature::PromptedFields))
        );
        assert!(c.expanded.is_none());
    }
}
