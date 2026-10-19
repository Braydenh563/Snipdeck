use std::collections::BTreeMap;

use proptest::prelude::*;

use super::*;
use crate::{Entitlements, Error, Feature, Tier};

fn now() -> Timestamp {
    // Saturday 17 October 2026, day 290 of the year.
    "2026-10-17T14:05:09Z".parse().unwrap()
}

fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

static NO_ANSWERS: BTreeMap<String, String> = BTreeMap::new();

fn render_with(
    src: &str,
    build: impl FnOnce(RenderContext<'static>) -> RenderContext<'static>,
) -> Rendered {
    let ctx = build(RenderContext::new(now(), &NO_ANSWERS));
    Template::parse(src).unwrap().render(&ctx)
}

fn render(src: &str) -> String {
    render_with(src, |c| c).text
}

fn parse_err(src: &str) -> TemplateError {
    Template::parse(src).unwrap_err()
}

// ---- static placeholders -------------------------------------------------------------

#[test]
fn plain_text_renders_unchanged() {
    assert_eq!(render("Hello, world\nline two"), "Hello, world\nline two");
    assert_eq!(render(""), "");
}

#[test]
fn date_and_time_default_to_iso_in_utc() {
    assert_eq!(render("{date} {time}"), "2026-10-17 14:05");
}

#[test]
fn date_and_time_follow_the_time_zone() {
    let out = render_with("{date} {time}", |c| {
        c.with_tz(TimeZone::fixed(jiff::tz::offset(10)))
    });
    assert_eq!(out.text, "2026-10-18 00:05");
}

#[test]
fn date_and_time_follow_the_locale() {
    let cases = [
        ("en", "10/17/2026 2:05 PM"),
        ("de", "17.10.2026 14:05"),
        ("fr", "17/10/2026 14:05"),
        ("es", "17/10/2026 14:05"),
        ("pt-BR", "17/10/2026 14:05"),
        ("ja", "2026/10/17 14:05"),
        ("zh-Hans", "2026/10/17 14:05"),
        ("xx", "2026-10-17 14:05"),
    ];
    for (tag, expected) in cases {
        let out = render_with("{date} {time}", |c| c.with_locale(Locale::for_tag(tag)));
        assert_eq!(out.text, expected, "locale {tag}");
    }
}

#[test]
fn locale_tags_are_matched_loosely() {
    assert_eq!(Locale::for_tag("DE_de"), Locale::for_tag("de"));
    assert_eq!(Locale::for_tag("en-GB").date, DateStyle::DayMonthYear);
    assert_eq!(Locale::for_tag("en-GB").clock, ClockStyle::H24);
    assert_eq!(Locale::for_tag(""), Locale::default());
}

#[test]
fn twelve_hour_clock_handles_midnight_and_noon() {
    let twelve = Locale::for_tag("en");
    for (stamp, expected) in [
        ("2026-10-17T00:07:00Z", "12:07 AM"),
        ("2026-10-17T12:07:00Z", "12:07 PM"),
        ("2026-10-17T23:59:00Z", "11:59 PM"),
    ] {
        let empty = BTreeMap::new();
        let ctx = RenderContext::new(stamp.parse().unwrap(), &empty).with_locale(twelve);
        assert_eq!(
            Template::parse("{time}").unwrap().render(&ctx).text,
            expected
        );
    }
}

#[test]
fn datetime_formats_each_supported_specifier() {
    let cases = [
        ("%Y", "2026"),
        ("%y", "26"),
        ("%m", "10"),
        ("%d", "17"),
        ("%e", "17"),
        ("%H", "14"),
        ("%I", "02"),
        ("%M", "05"),
        ("%S", "09"),
        ("%p", "PM"),
        ("%A", "Saturday"),
        ("%a", "Sat"),
        ("%B", "October"),
        ("%b", "Oct"),
        ("%j", "290"),
        ("%z", "+0000"),
        ("%F", "2026-10-17"),
        ("%T", "14:05:09"),
        ("%%", "%"),
        ("%d/%m/%Y at %H:%M", "17/10/2026 at 14:05"),
        ("no specifiers", "no specifiers"),
        ("année %Y", "année 2026"),
    ];
    for (format, expected) in cases {
        assert_eq!(
            render(&format!("{{datetime:{format}}}")),
            expected,
            "{format}"
        );
    }
}

#[test]
fn datetime_space_pads_the_day_for_percent_e() {
    let empty = BTreeMap::new();
    let ctx = RenderContext::new("2026-10-05T01:00:00Z".parse().unwrap(), &empty);
    assert_eq!(
        Template::parse("[{datetime:%e}]")
            .unwrap()
            .render(&ctx)
            .text,
        "[ 5]"
    );
}

#[test]
fn datetime_offset_shows_the_zone_offset() {
    let out = render_with("{datetime:%z}", |c| {
        c.with_tz(TimeZone::fixed(jiff::tz::offset(-5)))
    });
    assert_eq!(out.text, "-0500");
}

#[test]
fn clipboard_is_inserted_or_empty() {
    assert_eq!(render("[{clipboard}]"), "[]");
    let out = render_with("[{clipboard}]", |c| c.with_clipboard("{date}"));
    assert_eq!(out.text, "[{date}]", "clipboard text is never re-parsed");
}

// ---- cursor --------------------------------------------------------------------------

#[test]
fn cursor_reports_characters_after_it() {
    let out = render_with("Dear {cursor},\nThanks", |c| c);
    assert_eq!(out.text, "Dear ,\nThanks");
    assert_eq!(out.cursor_from_end, Some(",\nThanks".chars().count()));
}

#[test]
fn cursor_counts_crlf_as_one() {
    let out = render_with("a{cursor}é😀\r\nb", |c| c);
    assert_eq!(out.cursor_from_end, Some(4));
}

#[test]
fn cursor_at_the_end_is_zero_and_absent_is_none() {
    assert_eq!(render_with("x{cursor}", |c| c).cursor_from_end, Some(0));
    assert_eq!(render_with("x", |c| c).cursor_from_end, None);
}

#[test]
fn cursor_accounts_for_text_inserted_after_it() {
    let out = render_with("{cursor}{clipboard}!", |c| c.with_clipboard("abc"));
    assert_eq!(out.cursor_from_end, Some(4));
}

#[test]
fn a_second_cursor_is_an_error() {
    let err = parse_err("a{cursor}b{cursor}");
    assert_eq!(err.kind, TemplateErrorKind::DuplicateCursor);
    assert_eq!(err.span, 10..18);
}

// ---- prompted ------------------------------------------------------------------------

#[test]
fn fields_and_choices_render_their_answers() {
    let t = Template::parse("Hi {field:Name}, {choice:Tone|warm|formal}").unwrap();
    let a = answers(&[("Name", "Ada"), ("Tone", "formal")]);
    let ctx = RenderContext::new(now(), &a);
    assert_eq!(t.render(&ctx).text, "Hi Ada, formal");
}

#[test]
fn missing_answers_fall_back_to_defaults() {
    let t = Template::parse("{field:A}|{field:B|dflt}|{choice:C|x|y}").unwrap();
    let a = BTreeMap::new();
    assert_eq!(t.render(&RenderContext::new(now(), &a)).text, "|dflt|x");
}

#[test]
fn an_empty_answer_is_respected_over_the_default() {
    let t = Template::parse("{field:B|dflt}").unwrap();
    let a = answers(&[("B", "")]);
    assert_eq!(t.render(&RenderContext::new(now(), &a)).text, "");
}

#[test]
fn answers_are_not_re_parsed() {
    let t = Template::parse("{field:A}").unwrap();
    let a = answers(&[("A", "{date}")]);
    assert_eq!(t.render(&RenderContext::new(now(), &a)).text, "{date}");
}

#[test]
fn prompts_are_deduplicated_in_first_use_order() {
    let t = Template::parse(
        "{field:Zed} {choice:Tone|a|b} {field:Alpha|hi} {field:Zed} {choice:Tone|a|b} {date}",
    )
    .unwrap();
    assert_eq!(
        t.prompts(),
        vec![
            Prompt {
                name: "Zed".into(),
                kind: PromptKind::Field { default: None }
            },
            Prompt {
                name: "Tone".into(),
                kind: PromptKind::Choice {
                    options: vec!["a".into(), "b".into()]
                }
            },
            Prompt {
                name: "Alpha".into(),
                kind: PromptKind::Field {
                    default: Some("hi".into())
                }
            },
        ]
    );
}

#[test]
fn the_first_default_of_a_repeated_field_is_the_prompt_default() {
    let t = Template::parse("{field:N} {field:N|first} {field:N|second}").unwrap();
    assert_eq!(
        t.prompts()[0].kind,
        PromptKind::Field {
            default: Some("first".into())
        }
    );
    let a = BTreeMap::new();
    assert_eq!(
        t.render(&RenderContext::new(now(), &a)).text,
        "first first first"
    );
}

#[test]
fn names_and_options_are_trimmed() {
    let t = Template::parse("{field: Name | hi there }{choice: T | a | b }").unwrap();
    let p = t.prompts();
    assert_eq!(p[0].name, "Name");
    assert_eq!(
        p[0].kind,
        PromptKind::Field {
            default: Some("hi there".into())
        }
    );
    assert_eq!(
        p[1].kind,
        PromptKind::Choice {
            options: vec!["a".into(), "b".into()]
        }
    );
}

#[test]
fn reusing_a_name_for_a_different_prompt_is_an_error() {
    assert_eq!(
        parse_err("{field:X} {choice:X|a|b}").kind,
        TemplateErrorKind::ConflictingPrompt { name: "X".into() }
    );
    assert_eq!(
        parse_err("{choice:X|a|b} {choice:X|a|c}").kind,
        TemplateErrorKind::ConflictingPrompt { name: "X".into() }
    );
}

#[test]
fn more_than_sixteen_distinct_prompts_is_an_error_but_a_reused_name_is_not() {
    let many = |n: usize| {
        (0..n).fold(String::new(), |mut out, i| {
            out.push_str("{field:F");
            out.push_str(&i.to_string());
            out.push('}');
            out
        })
    };
    assert!(Template::parse(&many(16)).is_ok());
    assert_eq!(
        parse_err(&many(17)).kind,
        TemplateErrorKind::TooManyPrompts { max: 16 }
    );
    assert!(Template::parse(&format!("{} {{field:F0}}", many(16))).is_ok());
}

// ---- entitlements --------------------------------------------------------------------

#[test]
fn static_templates_need_no_feature() {
    let t = Template::parse("{date} {time} {datetime:%Y} {clipboard} {cursor}").unwrap();
    assert!(!t.requires(Feature::PromptedFields));
    assert!(t.check(&Entitlements::new(Tier::Free)).is_ok());
    assert_eq!(t.prompts().len(), 0);
}

#[test]
fn prompted_templates_need_pro() {
    for src in ["{field:A}", "x {choice:A|b|c}"] {
        let t = Template::parse(src).unwrap();
        assert!(t.requires(Feature::PromptedFields));
        assert!(!t.requires(Feature::PasteStack));
        assert_eq!(
            t.check(&Entitlements::new(Tier::Free)),
            Err(Error::ProRequired(Feature::PromptedFields))
        );
        assert!(t.check(&Entitlements::new(Tier::Pro)).is_ok());
    }
}

// ---- escaping ------------------------------------------------------------------------

#[test]
fn doubled_braces_are_literal() {
    assert_eq!(render("{{date}} and }}"), "{date} and }");
    assert_eq!(render("{{{date}}}"), "{2026-10-17}");
}

#[test]
fn escape_helper_doubles_braces() {
    assert_eq!(escape("a{b}c"), "a{{b}}c");
}

// ---- errors --------------------------------------------------------------------------

#[test]
fn unknown_placeholders_are_errors_with_a_span_and_a_hint() {
    let err = parse_err("Hi {nam} there");
    assert_eq!(err.span, 3..8);
    assert_eq!(
        err.kind,
        TemplateErrorKind::Unknown {
            found: "nam".into(),
            suggestion: None
        }
    );
    let msg = err.to_string();
    assert!(msg.contains("unknown placeholder `{nam}`"), "{msg}");
    assert!(msg.contains("{{") && msg.contains("{field:Name}"), "{msg}");
}

#[test]
fn near_misses_get_a_suggestion() {
    for (src, want) in [
        ("{Date}", "date"),
        ("{dat}", "date"),
        ("{clipbaord}", "clipboard"),
        ("{feild:A}", "field"),
    ] {
        match parse_err(src).kind {
            TemplateErrorKind::Unknown { suggestion, .. } => {
                assert_eq!(suggestion, Some(want), "{src}");
            }
            other => panic!("{src}: {other:?}"),
        }
    }
    assert!(
        parse_err("{dat}")
            .to_string()
            .contains("did you mean `{date}`")
    );
}

#[test]
fn argument_rules_are_enforced() {
    assert_eq!(
        parse_err("{date:%Y}").kind,
        TemplateErrorKind::UnexpectedArgument {
            placeholder: "date"
        }
    );
    assert_eq!(
        parse_err("{cursor:1}").kind,
        TemplateErrorKind::UnexpectedArgument {
            placeholder: "cursor"
        }
    );
    assert_eq!(
        parse_err("{datetime}").kind,
        TemplateErrorKind::MissingArgument {
            placeholder: "datetime"
        }
    );
    assert_eq!(
        parse_err("{datetime:}").kind,
        TemplateErrorKind::MissingArgument {
            placeholder: "datetime"
        }
    );
    assert_eq!(
        parse_err("{field}").kind,
        TemplateErrorKind::MissingArgument {
            placeholder: "field"
        }
    );
    assert_eq!(parse_err("{field: |x}").kind, TemplateErrorKind::EmptyName);
    assert_eq!(parse_err("{choice:T}").kind, TemplateErrorKind::NoOptions);
    assert_eq!(
        parse_err("{choice:T|a||b}").kind,
        TemplateErrorKind::EmptyOption
    );
    assert_eq!(
        parse_err("{choice:T|}").kind,
        TemplateErrorKind::EmptyOption
    );
    assert_eq!(parse_err("{}").kind, TemplateErrorKind::EmptyPlaceholder);
}

#[test]
fn bad_datetime_formats_point_at_the_specifier() {
    let err = parse_err("at {datetime:%H:%Q}");
    assert_eq!(
        err.kind,
        TemplateErrorKind::UnknownFormatSpecifier { specifier: 'Q' }
    );
    assert_eq!(&"at {datetime:%H:%Q}"[err.span.clone()], "%Q");
    let err = parse_err("{datetime:%Y%}");
    assert_eq!(err.kind, TemplateErrorKind::TrailingPercent);
    assert_eq!(&"{datetime:%Y%}"[err.span.clone()], "%");
    let err = parse_err("{datetime:%é}");
    assert_eq!(
        err.kind,
        TemplateErrorKind::UnknownFormatSpecifier { specifier: 'é' }
    );
    assert_eq!(&"{datetime:%é}"[err.span.clone()], "%é");
}

#[test]
fn braces_must_balance() {
    let err = parse_err("a } b");
    assert_eq!(
        (err.kind, err.span),
        (TemplateErrorKind::UnmatchedClose, 2..3)
    );
    assert!(parse_err("a } b").to_string().contains("}}"));

    let err = parse_err("ab {date");
    assert_eq!(
        (err.kind, err.span),
        (TemplateErrorKind::Unterminated, 3..8)
    );

    let err = parse_err("{date {time}");
    assert_eq!(
        (err.kind, err.span),
        (TemplateErrorKind::Unterminated, 0..6)
    );

    assert_eq!(parse_err("{").kind, TemplateErrorKind::Unterminated);
}

#[test]
fn spans_are_byte_offsets_after_multibyte_text() {
    let src = "é😀 {nope}";
    let err = parse_err(src);
    assert_eq!(&src[err.span], "{nope}");
}

#[test]
fn the_first_error_wins() {
    assert_eq!(parse_err("{a} {b}").span, 0..3);
}

// ---- cursor in grapheme clusters -------------------------------------------------------

#[test]
fn cursor_counts_grapheme_clusters_like_left_arrow_presses() {
    // An editor's caret moves one extended grapheme cluster per Left press.
    for (src, left_presses) in [
        // "e" + combining acute, then a ZWJ family: two clusters.
        ("{cursor}e\u{301}\u{1F468}\u{200D}\u{1F469}", 2),
        // Flags are two regional indicators each.
        ("{cursor}\u{1F1E9}\u{1F1EA}\u{1F1EB}\u{1F1F7}", 2),
        // A skin-tone modifier and a variation selector stay with their base.
        ("x{cursor}\u{1F44D}\u{1F3FD}\u{2764}\u{FE0F}", 2),
        // Hangul syllable built from jamo, and a Devanagari conjunct cluster.
        (
            "{cursor}\u{1112}\u{1161}\u{11AB}\u{0915}\u{094D}\u{0937}",
            2,
        ),
        ("{cursor}a\r\nb", 3),
        ("{cursor}", 0),
    ] {
        assert_eq!(
            render_with(src, |c| c).cursor_from_end,
            Some(left_presses),
            "{src:?}"
        );
    }
}

// ---- output bound ----------------------------------------------------------------------

#[test]
fn rendering_stops_at_the_output_limit_instead_of_exhausting_memory() {
    let t = Template::parse("{clipboard}{clipboard}").unwrap();
    let big = "a".repeat(MAX_RENDERED_BYTES / 2 + 1);
    let a = BTreeMap::new();
    let ctx = RenderContext::new(now(), &a).with_clipboard(&big);
    assert_eq!(
        t.try_render(&ctx),
        Err(RenderError::OutputTooLarge {
            limit: MAX_RENDERED_BYTES
        })
    );
    // The plain `render` degrades: the output is cut at the limit, never larger.
    let cut = t.render(&ctx);
    assert_eq!(cut.text.len(), MAX_RENDERED_BYTES);
    assert!(cut.text.bytes().all(|b| b == b'a'));
}

#[test]
fn output_exactly_at_the_limit_is_allowed() {
    let a = BTreeMap::new();
    let exact = "é".repeat(MAX_RENDERED_BYTES / 2);
    let ctx = RenderContext::new(now(), &a).with_clipboard(&exact);
    let out = Template::parse("{clipboard}")
        .unwrap()
        .try_render(&ctx)
        .unwrap();
    assert_eq!(out.text.len(), MAX_RENDERED_BYTES);
    let one_more = Template::parse("{clipboard}x").unwrap().try_render(&ctx);
    assert!(matches!(one_more, Err(RenderError::OutputTooLarge { .. })));
}

#[test]
fn the_reviewers_amplification_case_is_bounded() {
    // 65 527 bytes of `{clipboard}` with a 10 KB clipboard used to render 59.6 MB.
    let src = "{clipboard}".repeat(5957);
    let clip = "c".repeat(10_000);
    let a = BTreeMap::new();
    let ctx = RenderContext::new(now(), &a).with_clipboard(&clip);
    let t = Template::parse(&src).unwrap();
    assert!(matches!(
        t.try_render(&ctx),
        Err(RenderError::OutputTooLarge { .. })
    ));
    assert!(t.render(&ctx).text.len() <= MAX_RENDERED_BYTES);
}

#[test]
fn truncation_never_splits_a_character_and_keeps_a_valid_cursor() {
    let a = BTreeMap::new();
    let clip = "😀".repeat(MAX_RENDERED_BYTES / 4 + 1);
    let ctx = RenderContext::new(now(), &a).with_clipboard(&clip);
    let out = Template::parse("{cursor}{clipboard}").unwrap().render(&ctx);
    assert_eq!(out.text.len(), MAX_RENDERED_BYTES);
    assert_eq!(out.cursor_from_end, Some(MAX_RENDERED_BYTES / 4));
}

#[test]
fn prompt_answers_count_towards_the_limit() {
    let t = Template::parse("{field:A}{field:A}").unwrap();
    let a = answers(&[("A", &"z".repeat(MAX_RENDERED_BYTES))]);
    let ctx = RenderContext::new(now(), &a);
    assert!(matches!(
        t.try_render(&ctx),
        Err(RenderError::OutputTooLarge { .. })
    ));
}

// ---- checked rendering -----------------------------------------------------------------

#[test]
fn render_checked_enforces_the_entitlement_that_render_does_not() {
    let t = Template::parse("Hi {field:Name|there}").unwrap();
    let a = BTreeMap::new();
    let ctx = RenderContext::new(now(), &a);
    // `render` is the unchecked primitive: it resolves Pro prompts for anyone.
    assert_eq!(t.render(&ctx).text, "Hi there");
    assert_eq!(
        t.render_checked(&Entitlements::new(Tier::Free), &ctx),
        Err(RenderError::ProRequired(Feature::PromptedFields))
    );
    assert_eq!(
        t.render_checked(&Entitlements::new(Tier::Pro), &ctx),
        Ok(t.render(&ctx))
    );
}

#[test]
fn render_checked_lets_free_users_render_static_templates() {
    let t = Template::parse("{date} {clipboard}{cursor}").unwrap();
    let a = BTreeMap::new();
    let ctx = RenderContext::new(now(), &a).with_clipboard("x");
    assert_eq!(
        t.render_checked(&Entitlements::default(), &ctx),
        Ok(t.render(&ctx))
    );
}

#[test]
fn render_checked_also_bounds_the_output() {
    let t = Template::parse("{clipboard}{clipboard}").unwrap();
    let big = "a".repeat(MAX_RENDERED_BYTES);
    let a = BTreeMap::new();
    let ctx = RenderContext::new(now(), &a).with_clipboard(&big);
    assert!(matches!(
        t.render_checked(&Entitlements::new(Tier::Pro), &ctx),
        Err(RenderError::OutputTooLarge { .. })
    ));
}

// ---- extra parts, duplicate options, padded keys ---------------------------------------

#[test]
fn a_field_takes_at_most_a_name_and_a_default() {
    for src in [
        "{field:Name|default|EXTRA}",
        "{field:Name|default|EXTRA|MORE}",
        "{field:Name||x}",
        "{field:Name|default|}",
    ] {
        let err = parse_err(src);
        assert_eq!(err.kind, TemplateErrorKind::TooManyParts, "{src}");
        assert_eq!(err.span, 0..src.len(), "{src}");
        assert!(err.to_string().contains("{field:Name|default}"), "{err}");
    }
    assert!(Template::parse("{field:Name|default}").is_ok());
}

#[test]
fn a_choice_may_not_repeat_an_option() {
    for src in ["{choice:x|a|a|a}", "{choice:x|a|b|a}", "{choice:x| a |a}"] {
        assert_eq!(
            parse_err(src).kind,
            TemplateErrorKind::DuplicateOption {
                option: "a".to_owned()
            },
            "{src}"
        );
    }
    // Options that differ only in case are different options.
    assert!(Template::parse("{choice:x|a|A}").is_ok());
}

#[test]
fn placeholder_keys_are_trimmed_like_names_and_options() {
    assert_eq!(render("{ date }"), "2026-10-17");
    assert_eq!(render("{\tcursor\n}x"), "x");
    assert_eq!(render("{ clipboard }"), "");
    assert_eq!(
        render("{ datetime : %Y}"),
        " 2026",
        "the format itself is not trimmed"
    );
    assert_eq!(render("{ datetime:%Y}"), "2026");
    assert_eq!(render("{datetime :%Y}"), "2026");
    let t = Template::parse("{ field : A }{ choice : B | x | y }").unwrap();
    assert_eq!(t.prompts().len(), 2);
    // Padding does not make a key valid.
    let err = parse_err("{ dat }");
    assert_eq!(
        err.kind,
        TemplateErrorKind::Unknown {
            found: "dat".into(),
            suggestion: Some("date")
        }
    );
    assert_eq!(parse_err("{ }").kind, TemplateErrorKind::EmptyPlaceholder);
    assert_eq!(
        parse_err("{ date:%Y}").kind,
        TemplateErrorKind::UnexpectedArgument {
            placeholder: "date"
        }
    );
}

#[test]
fn spans_stay_exact_when_the_key_is_padded() {
    let src = "x {  datetime : %H:%Q}";
    let err = parse_err(src);
    assert_eq!(&src[err.span], "%Q");
}

// ---- properties ----------------------------------------------------------------------

fn brace_heavy() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[{}a-zA-Z0-9:|% \\n\u{e9}\u{1F600}]{0,60}").unwrap()
}

fn valid_template() -> impl Strategy<Value = String> {
    let word = "[a-zA-Z][a-zA-Z0-9 ]{0,6}";
    let piece = prop_oneof![
        "[a-z \\n\u{e9}]{0,8}".prop_map(|s| escape(&s)),
        Just("{{".to_owned()),
        Just("}}".to_owned()),
        Just("{date}".to_owned()),
        Just("{time}".to_owned()),
        Just("{clipboard}".to_owned()),
        "[%YmdHMSpAaBbjzFT ,:/-]{1,8}".prop_filter_map("bad %", |f| {
            let ok = Template::parse(&format!("{{datetime:{f}}}")).is_ok();
            ok.then(|| format!("{{datetime:{f}}}"))
        }),
        word.prop_map(|n| format!("{{field:{n}}}")),
        (word, word).prop_map(|(n, d)| format!("{{field:{n}|{d}}}")),
        (word, word, word)
            .prop_filter("options must differ", |(_, a, b)| a.trim() != b.trim())
            .prop_map(|(n, a, b)| format!("{{choice:{n}|{a}|{b}}}")),
    ];
    proptest::collection::vec(piece, 0..8).prop_map(|v| v.concat())
}

/// `(source text, what it renders as)` for rendering at `now()` in UTC with ISO dates, the
/// answers `a = ANS` and `c = q`, and `<clip>` standing for the clipboard.
fn model_piece() -> impl Strategy<Value = (String, String)> {
    prop_oneof![
        "[a-z \\n\u{e9}]{0,8}".prop_map(|s| (escape(&s), s)),
        Just(("{{".to_owned(), "{".to_owned())),
        Just(("}}".to_owned(), "}".to_owned())),
        Just(("{date}".to_owned(), "2026-10-17".to_owned())),
        Just(("{ time }".to_owned(), "14:05".to_owned())),
        Just((
            "{datetime:%F %T}".to_owned(),
            "2026-10-17 14:05:09".to_owned()
        )),
        Just((
            "{datetime:%A %e %B %%}".to_owned(),
            "Saturday 17 October %".to_owned()
        )),
        Just(("{clipboard}".to_owned(), "<clip>".to_owned())),
        Just(("{field:a}".to_owned(), "ANS".to_owned())),
        Just(("{field: a | unused }".to_owned(), "ANS".to_owned())),
        Just(("{field:b|dflt}".to_owned(), "dflt".to_owned())),
        Just(("{field:e}".to_owned(), String::new())),
        Just(("{choice:c|p|q}".to_owned(), "q".to_owned())),
        Just(("{choice:d|p|q}".to_owned(), "p".to_owned())),
    ]
}

proptest! {
    #[test]
    fn parse_and_render_never_panic(src in prop_oneof![brace_heavy(), any::<String>()]) {
        if let Ok(t) = Template::parse(&src) {
            let a = BTreeMap::new();
            let _ = t.render(&RenderContext::new(now(), &a));
            let _ = t.prompts();
        }
    }

    #[test]
    fn errors_have_valid_spans(src in brace_heavy()) {
        if let Err(e) = Template::parse(&src) {
            prop_assert!(e.span.start <= e.span.end && e.span.end <= src.len());
            prop_assert!(src.is_char_boundary(e.span.start) && src.is_char_boundary(e.span.end));
            prop_assert_ne!(e.to_string().len(), 0);
        }
    }

    #[test]
    fn escaped_text_round_trips(text in any::<String>()) {
        let t = Template::parse(&escape(&text)).unwrap();
        let a = BTreeMap::new();
        let out = t.render(&RenderContext::new(now(), &a));
        prop_assert_eq!(out.text, text);
        prop_assert_eq!(out.cursor_from_end, None);
        prop_assert_eq!(t.prompts().len(), 0);
    }

    /// A model check: the expected output is built piece by piece from what each placeholder
    /// means, independently of the renderer, and compared with the real thing. Pieces use
    /// letters that never combine, so the expected cursor is a plain `chars().count()`.
    #[test]
    fn rendering_matches_a_model(
        pieces in proptest::collection::vec(model_piece(), 0..10),
        cursor_at in any::<prop::sample::Index>(),
        with_cursor in any::<bool>(),
        clip in "[a-z \\n\u{e9}]{0,12}",
    ) {
        let a = answers(&[("a", "ANS"), ("c", "q")]);
        let ctx = RenderContext::new(now(), &a).with_clipboard(&clip);
        let at = cursor_at.index(pieces.len() + 1);
        let mut src = String::new();
        let mut expected = String::new();
        let mut after = String::new();
        for (i, (source, shown)) in pieces.iter().enumerate() {
            if with_cursor && i == at {
                src.push_str("{cursor}");
            }
            src.push_str(source);
            let shown = shown.replace("<clip>", &clip);
            expected.push_str(&shown);
            if with_cursor && i >= at {
                after.push_str(&shown);
            }
        }
        if with_cursor && at == pieces.len() {
            src.push_str("{cursor}");
        }
        let out = Template::parse(&src).unwrap().render(&ctx);
        prop_assert_eq!(out.text, expected);
        prop_assert_eq!(out.cursor_from_end, with_cursor.then(|| after.chars().count()));
    }

    #[test]
    fn generated_templates_parse_and_prompts_are_unique(src in valid_template()) {
        let t = Template::parse(&src);
        // Reusing a name with a different prompt kind is the only way a valid mix can fail.
        match t {
            Ok(t) => {
                let names: Vec<_> = t.prompts().into_iter().map(|p| p.name).collect();
                let unique: std::collections::BTreeSet<_> = names.iter().collect();
                prop_assert_eq!(unique.len(), names.len());
            }
            Err(e) => prop_assert!(
                matches!(
                    e.kind,
                    TemplateErrorKind::ConflictingPrompt { .. } | TemplateErrorKind::TooManyPrompts { .. }
                ),
                "{e}"
            ),
        }
    }
}
