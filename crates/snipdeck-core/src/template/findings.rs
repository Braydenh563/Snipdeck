//! Tests for the template findings LIBRARY-13 to LIBRARY-25 of the library audit. Every test
//! uses the fixed clock of `now()`: Saturday 17 October 2026, 14:05:09 UTC.

use std::collections::BTreeMap;

use super::*;

fn now() -> Timestamp {
    "2026-10-17T14:05:09Z".parse().unwrap()
}

fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

fn render_in(src: &str, locale: Locale, pairs: &[(&str, &str)]) -> Rendered {
    let given = answers(pairs);
    let ctx = RenderContext::new(now(), &given).with_locale(locale);
    Template::parse(src).unwrap().render(&ctx)
}

fn text(src: &str) -> String {
    render_in(src, Locale::default(), &[]).text
}

fn answered(src: &str, pairs: &[(&str, &str)]) -> String {
    render_in(src, Locale::default(), pairs).text
}

fn kind(src: &str) -> TemplateErrorKind {
    Template::parse(src).unwrap_err().kind
}

/// A fixed library for `{snippet:…}`.
struct Library(Vec<(&'static str, &'static str)>);

impl SnippetSource for Library {
    fn snippet(&self, name: &str) -> SnippetLookup {
        let found: Vec<_> = self
            .0
            .iter()
            .filter(|(title, _)| title.eq_ignore_ascii_case(name.trim()))
            .collect();
        match found.as_slice() {
            [] => SnippetLookup::NotFound,
            [(_, body)] => SnippetLookup::Found((*body).to_owned()),
            _ => SnippetLookup::Ambiguous,
        }
    }
}

fn innermost(kind: TemplateErrorKind) -> SnippetProblem {
    match kind {
        TemplateErrorKind::Snippet { problem, .. } => problem,
        TemplateErrorKind::InSnippet { error, .. } => innermost(*error),
        other => panic!("{other:?}"),
    }
}

fn with_library(src: &str, library: &Library) -> Result<Template, TemplateError> {
    Template::parse_with(src, library)
}

// ---- LIBRARY-13: an optional text field ------------------------------------------------

#[test]
fn library_13_an_empty_default_makes_a_field_optional() {
    let t = Template::parse("{field:CC|} {multiline:Notes|} {field:To}").unwrap();
    let p = t.prompts();
    assert_eq!(
        p[0].kind,
        PromptKind::Field {
            default: Some(String::new())
        }
    );
    assert_eq!(
        p[1].kind,
        PromptKind::Multiline {
            default: Some(String::new())
        }
    );
    assert_eq!(p[2].kind, PromptKind::Field { default: None });
    assert_eq!(answered("[{field:CC|}]", &[]), "[]");
}

// ---- LIBRARY-14: prompts in an unticked section ----------------------------------------

#[test]
fn library_14_a_prompt_only_in_an_unticked_section_is_not_active() {
    let t = Template::parse("{field:Name} {optional:Extra}{field:Phone} {number:N}{/optional}")
        .unwrap();
    let off = t.active_prompts(&answers(&[]));
    let names: Vec<_> = t.prompts().iter().map(|p| p.name.clone()).collect();
    assert_eq!(names, ["Name", "Extra", "Phone", "N"]);
    assert_eq!(off, [true, true, false, false]);
    let on = t.active_prompts(&answers(&[("Extra", "true")]));
    assert_eq!(on, [true, true, true, true]);
}

#[test]
fn library_14_a_prompt_used_inside_and_outside_stays_active() {
    let t = Template::parse("{field:A}{optional:X}{field:A}{/optional}").unwrap();
    assert_eq!(t.active_prompts(&answers(&[])), [true, true]);
}

// ---- LIBRARY-15: month and weekday names per language ----------------------------------

#[test]
fn library_15_names_follow_the_language() {
    for (tag, want) in [
        ("en", "Saturday 17 October Sat Oct"),
        ("es", "sábado 17 octubre sáb oct"),
        ("de", "Samstag 17 Oktober Sa Okt"),
        ("fr", "samedi 17 octobre sam. oct."),
        ("pt-BR", "sábado 17 outubro sáb. out."),
        ("ja", "土曜日 17 10月 土 10月"),
        ("zh-Hans", "星期六 17 十月 周六 10月"),
    ] {
        let got = render_in("{datetime:%A %d %B %a %b}", Locale::for_tag(tag), &[]).text;
        assert_eq!(got, want, "{tag}");
    }
}

#[test]
fn library_15_every_language_has_twelve_distinct_months_and_seven_weekdays() {
    for tag in ["en", "es", "de", "fr", "pt-BR", "ja", "zh-Hans"] {
        let lang = Lang::of_tag(tag);
        let months = lang.months();
        let days = lang.weekdays();
        for (i, a) in months.iter().enumerate() {
            assert!(!a.is_empty() && !months[..i].contains(a), "{tag} {a}");
        }
        for (i, a) in days.iter().enumerate() {
            assert!(!a.is_empty() && !days[..i].contains(a), "{tag} {a}");
        }
        assert_eq!(lang.months_abbr().len(), 12);
        assert_eq!(lang.weekdays_abbr().len(), 7);
    }
}

#[test]
fn library_15_the_am_pm_word_follows_the_language() {
    let mut locale = Locale::for_tag("en");
    locale.clock = ClockStyle::H12;
    assert_eq!(
        render_in("{datetime:%I:%M %p}", locale, &[]).text,
        "02:05 PM"
    );
    locale.lang = Lang::of_tag("ja");
    let got = render_in("{datetime:%p}", locale, &[]).text;
    assert_eq!(got, "午後");
}

// ---- LIBRARY-16: more date specifiers --------------------------------------------------

#[test]
fn library_16_flags_drop_or_space_the_padding() {
    assert_eq!(text("{datetime:%-d/%-m/%y}"), "17/10/26");
    assert_eq!(text("{datetime:%_d|%-H|%_m|%-I}"), "17|14|10|2");
    // 2026-03-05 for a one-digit day.
    assert_eq!(
        text("{datetime:%-d %_d %d}").split(' ').count(),
        3,
        "three groups"
    );
}

#[test]
fn library_16_one_digit_days_show_the_flags() {
    let early: Timestamp = "2026-03-05T07:08:09Z".parse().unwrap();
    let given = answers(&[]);
    let ctx = RenderContext::new(early, &given);
    let out = Template::parse("{datetime:%-d|%_d|%d|%-m|%-H|%_H|%e}")
        .unwrap()
        .render(&ctx);
    assert_eq!(out.text, "5| 5|05|3|7| 7| 5");
}

#[test]
fn library_16_week_quarter_year_and_zone_specifiers() {
    // 17 October 2026 is a Saturday in ISO week 42, quarter 4.
    assert_eq!(text("{datetime:%V %G %q %u %j}"), "42 2026 4 6 290");
    assert_eq!(text("{datetime:%Z}"), "UTC");
    assert_eq!(text("{datetime:%R}"), "14:05");
    // 1 January 2027 belongs to ISO week 53 of 2026.
    let new_year: Timestamp = "2027-01-01T12:00:00Z".parse().unwrap();
    let given = answers(&[]);
    let ctx = RenderContext::new(new_year, &given);
    let out = Template::parse("{datetime:%G-W%V}").unwrap().render(&ctx);
    assert_eq!(out.text, "2026-W53");
}

#[test]
fn library_16_the_error_lists_every_valid_specifier() {
    let e = Template::parse("{datetime:%Q}").unwrap_err();
    assert_eq!(
        e.kind,
        TemplateErrorKind::UnknownFormatSpecifier { specifier: 'Q' }
    );
    let message = e.to_string();
    for spec in ["%V", "%G", "%Z", "%-d", "%_d", "%q"] {
        assert!(message.contains(spec), "{spec} in {message}");
    }
    // Every specifier the list names parses.
    for spec in FORMAT_SPECIFIERS
        .split_whitespace()
        .filter(|s| s.len() == 2 && s.starts_with('%'))
    {
        assert!(
            Template::parse(&format!("{{datetime:{spec}}}")).is_ok(),
            "{spec}"
        );
    }
    assert_eq!(
        kind("{datetime:%-Q}"),
        TemplateErrorKind::UnknownFormatSpecifier { specifier: 'Q' }
    );
}

// ---- LIBRARY-17: one rule for the three clock placeholders -----------------------------

#[test]
fn library_17_time_and_datetime_take_a_format_and_default_without() {
    assert_eq!(text("{time:%H.%M}"), "14.05");
    assert_eq!(text("{time}"), "14:05");
    assert_eq!(text("{datetime}"), "2026-10-17 14:05");
    assert_eq!(text("{datetime+1d:%d}"), "18");
    assert_eq!(text("{time+2h}"), "16:05");
    assert_eq!(text("{datetime+1d}"), "2026-10-18 14:05");
}

#[test]
fn library_17_an_empty_format_is_still_an_error_for_all_three() {
    for key in ["date", "time", "datetime"] {
        assert_eq!(
            kind(&format!("{{{key}:}}")),
            TemplateErrorKind::MissingArgument {
                placeholder: match key {
                    "date" => "date",
                    "time" => "time",
                    _ => "datetime",
                }
            }
        );
    }
}

#[test]
fn library_17_datetime_follows_the_locale_without_a_format() {
    let mut locale = Locale::for_tag("en");
    locale.clock = ClockStyle::H12;
    assert_eq!(
        render_in("{datetime}", locale, &[]).text,
        "10/17/2026 2:05 PM"
    );
}

// ---- LIBRARY-18: typed dates in the locale's order -------------------------------------

#[test]
fn library_18_a_typed_date_is_read_in_the_locales_order() {
    let day_first = DateStyle::DAY_MONTH_YEAR;
    let month_first = DateStyle::MONTH_DAY_YEAR;
    let ymd = jiff::civil::date(2027, 3, 4);
    assert_eq!(parse_pick_date("04/03/2027", day_first), Some(ymd));
    assert_eq!(parse_pick_date("4.3.2027", day_first), Some(ymd));
    assert_eq!(parse_pick_date("03/04/2027", month_first), Some(ymd));
    assert_eq!(
        parse_pick_date("2027-03-04", day_first),
        Some(ymd),
        "ISO always works"
    );
    assert_eq!(parse_pick_date(" 2027-3-4 ", month_first), Some(ymd));
    assert_eq!(parse_pick_date("31/02/2027", day_first), None);
    assert_eq!(parse_pick_date("13/13/2027", month_first), None);
    assert_eq!(parse_pick_date("soon", day_first), None);
}

#[test]
fn library_18_the_hint_and_today_text_follow_the_locale() {
    let de = Locale::for_tag("de");
    assert_eq!(de.date.hint(), "DD.MM.YYYY");
    assert_eq!(Locale::for_tag("en").date.hint(), "MM/DD/YYYY");
    let given = answers(&[]);
    let ctx = RenderContext::new(now(), &given).with_locale(de);
    assert_eq!(today_text(&ctx), "17.10.2026");
}

#[test]
fn library_18_a_pickdate_answer_typed_in_the_locale_renders() {
    let got = render_in(
        "{pickdate:When}",
        Locale::for_tag("de"),
        &[("When", "04.03.2027")],
    );
    assert_eq!(got.text, "04.03.2027");
    let got = render_in(
        "{pickdate:When|%F}",
        Locale::for_tag("de"),
        &[("When", "4.3.2027")],
    );
    assert_eq!(got.text, "2027-03-04");
}

// ---- LIBRARY-19: escapes ---------------------------------------------------------------

#[test]
fn library_19_a_backslash_escapes_the_separators_in_names_defaults_and_options() {
    let t = Template::parse(r"{field:A\|B|x\|y}{choice:C|a\|b|c\}d|*e\{f}").unwrap();
    let p = t.prompts();
    assert_eq!(p[0].name, "A|B");
    assert_eq!(
        p[0].kind,
        PromptKind::Field {
            default: Some("x|y".into())
        }
    );
    assert_eq!(
        p[1].kind,
        PromptKind::Choice {
            options: vec!["a|b".into(), "c}d".into(), "e{f".into()],
            default: 2
        }
    );
    assert_eq!(
        answered(r"{field:A|x\{y\}}", &[]),
        "x{y}",
        "braces in a default"
    );
}

#[test]
fn library_19_the_error_for_a_stray_brace_names_the_fix() {
    for src in ["function() { return 1; }", "fn main() { }"] {
        let e = Template::parse(src).unwrap_err();
        let message = e.to_string();
        assert!(message.contains("{{"), "{src}: {message}");
    }
    assert_eq!(kind("fn main() { }"), TemplateErrorKind::EmptyPlaceholder);
}

#[test]
fn library_19_doubled_braces_and_escapes_render_literally() {
    assert_eq!(
        text("function() {{ return 1; }}"),
        "function() { return 1; }"
    );
    assert_eq!(answered(r"{field:A|a\\b}", &[]), r"a\b");
}

#[test]
fn library_19_a_default_with_more_pipes_is_still_too_many_parts_unescaped() {
    assert_eq!(kind("{field:A|x|y}"), TemplateErrorKind::TooManyParts);
}

#[test]
fn library_19_escape_arg_round_trips_through_the_parser() {
    for raw in ["a|b", "c{d}", r"e\f", "*star", "plain", "{x}|{y}"] {
        let src = format!("{{field:{}|{}}}", escape_arg(raw), escape_arg(raw));
        let t = Template::parse(&src).unwrap();
        let p = t.prompts();
        assert_eq!(p[0].name, raw.trim(), "{src}");
        assert_eq!(
            p[0].kind,
            PromptKind::Field {
                default: Some(raw.trim().into())
            },
            "{src}"
        );
    }
}

// ---- LIBRARY-20: prompt names compare without case -------------------------------------

#[test]
fn library_20_names_that_differ_in_case_or_padding_are_one_prompt() {
    let t = Template::parse("{field:Name} {field:name} {value: NAME |upper}").unwrap();
    assert_eq!(t.prompts().len(), 1);
    assert_eq!(t.prompts()[0].name, "Name", "the first spelling labels it");
    assert_eq!(
        answered("{field:Name}-{field:name}", &[("Name", "Ada")]),
        "Ada-Ada"
    );
}

#[test]
fn library_20_the_same_name_for_different_kinds_still_conflicts_without_case() {
    assert_eq!(
        kind("{field:Size} {choice:size|S|M}"),
        TemplateErrorKind::ConflictingPrompt {
            name: "size".into()
        }
    );
}

// ---- LIBRARY-21: limits ----------------------------------------------------------------

#[test]
fn library_21_the_limits_are_raised() {
    const { assert!(MAX_PROMPTS >= 64 && MAX_OFFSETS >= 16) };
    let many = (0..MAX_PROMPTS).fold(String::new(), |mut out, i| {
        out.push_str("{field:F");
        out.push_str(&i.to_string());
        out.push('}');
        out
    });
    assert_eq!(Template::parse(&many).unwrap().prompts().len(), MAX_PROMPTS);
    let over = format!("{many}{{field:One more}}");
    assert_eq!(
        kind(&over),
        TemplateErrorKind::TooManyPrompts { max: MAX_PROMPTS }
    );
}

// ---- LIBRARY-22: a choice with a default and a "none" ----------------------------------

#[test]
fn library_22_a_starred_option_is_the_start_option() {
    let t = Template::parse("{choice:Size|S|*M|L}").unwrap();
    assert_eq!(
        t.prompts()[0].kind,
        PromptKind::Choice {
            options: vec!["S".into(), "M".into(), "L".into()],
            default: 1
        }
    );
    assert_eq!(answered("{choice:Size|S|*M|L}", &[]), "M");
    assert_eq!(answered("{choice:Size|S|*M|L}", &[("Size", "L")]), "L");
    assert_eq!(
        kind("{choice:Size|*S|*M}"),
        TemplateErrorKind::TwoStartOptions
    );
}

#[test]
fn library_22_an_empty_option_is_the_none_choice() {
    let t = Template::parse("{choice:Size|S|M|}").unwrap();
    assert_eq!(
        t.prompts()[0].kind,
        PromptKind::Choice {
            options: vec!["S".into(), "M".into(), String::new()],
            default: 0
        }
    );
    assert_eq!(answered("[{choice:Size|*|S|M}]", &[]), "[]");
    assert_eq!(answered("[{choice:Size|S|M|}]", &[("Size", "")]), "[]");
    assert_eq!(kind("{choice:Size|a||}"), TemplateErrorKind::EmptyOption);
    assert_eq!(kind("{choice:Size|}"), TemplateErrorKind::EmptyOption);
}

// ---- LIBRARY-23: snippets, selection, clipboard history, keys, calc ---------------------

#[test]
fn library_23_a_snippet_is_put_in_place_and_its_prompts_join() {
    let lib = Library(vec![
        ("Sign off", "Regards, {field:Name}"),
        ("Greeting", "Hi {field:To}, {snippet:Sign off}"),
    ]);
    let t = with_library("{snippet:greeting}!", &lib).unwrap();
    let names: Vec<_> = t.prompts().iter().map(|p| p.name.clone()).collect();
    assert_eq!(names, ["To", "Name"]);
    let given = answers(&[("To", "Bo"), ("Name", "Ada")]);
    let out = t.render(&RenderContext::new(now(), &given));
    assert_eq!(out.text, "Hi Bo, Regards, Ada!");
}

#[test]
fn library_23_snippet_cycles_depth_and_missing_ones_are_errors() {
    let lib = Library(vec![
        ("a", "A{snippet:b}"),
        ("b", "B{snippet:a}"),
        ("self", "{snippet:self}"),
        ("d1", "{snippet:d2}"),
        ("d2", "{snippet:d3}"),
        ("d3", "{snippet:d4}"),
        ("d4", "{snippet:d5}"),
        ("d5", "{snippet:d6}"),
        ("d6", "end"),
        ("dup", "x"),
        ("dup", "y"),
        ("bad", "{nope}"),
    ]);
    let problem = |src: &str| innermost(with_library(src, &lib).unwrap_err().kind);
    assert_eq!(problem("{snippet:a}"), SnippetProblem::Cycle);
    assert_eq!(problem("{snippet:self}"), SnippetProblem::Cycle);
    assert_eq!(problem("{snippet:d1}"), SnippetProblem::TooMuch);
    assert_eq!(problem("{snippet:none}"), SnippetProblem::NotFound);
    assert_eq!(problem("{snippet:dup}"), SnippetProblem::Ambiguous);
    assert!(matches!(
        with_library("{snippet:bad}", &lib).unwrap_err().kind,
        TemplateErrorKind::InSnippet { .. }
    ));
    assert_eq!(
        kind("{snippet:x}"),
        TemplateErrorKind::Snippet {
            name: "x".into(),
            problem: SnippetProblem::Unavailable
        }
    );
    assert!(with_library("{snippet:a}", &lib).is_err());
    // Using the same snippet twice (not nested) is fine.
    let twice = Library(vec![("t", "T")]);
    assert_eq!(
        with_library("{snippet:t}{snippet:t}", &twice)
            .unwrap()
            .render(&RenderContext::new(now(), &answers(&[])))
            .text,
        "TT"
    );
}

#[test]
fn library_23_a_snippet_cannot_close_or_open_a_section_across_its_edge() {
    let lib = Library(vec![("open", "{optional:X}"), ("close", "{/optional}")]);
    assert!(with_library("{snippet:open}", &lib).is_err());
    assert!(with_library("{optional:X}{snippet:close}", &lib).is_err());
}

#[test]
fn library_23_selection_renders_the_selected_text_with_a_transform() {
    let given = answers(&[]);
    let mut ctx = RenderContext::new(now(), &given);
    ctx.selection = Some("hello world");
    let t = Template::parse("[{selection}] [{selection|upper}]").unwrap();
    assert!(t.uses_selection());
    assert_eq!(t.render(&ctx).text, "[hello world] [HELLO WORLD]");
    assert_eq!(
        Template::parse("[{selection}]")
            .unwrap()
            .render(&RenderContext::new(now(), &given))
            .text,
        "[]"
    );
    assert!(!Template::parse("{clipboard}").unwrap().uses_selection());
    assert_eq!(
        kind("{selection:x}"),
        TemplateErrorKind::UnexpectedArgument {
            placeholder: "selection"
        }
    );
}

#[test]
fn library_23_clipboard_by_position() {
    let history = vec!["second".to_owned(), "third".to_owned()];
    let given = answers(&[]);
    let mut ctx = RenderContext::new(now(), &given);
    ctx.clipboard = Some("first");
    ctx.clipboard_history = &history;
    let t = Template::parse("{clipboard} {clipboard:2} {clipboard:3|upper} {clipboard:4}").unwrap();
    assert_eq!(t.max_clipboard_position(), 4);
    assert!(t.uses_clipboard());
    assert_eq!(t.render(&ctx).text, "first second THIRD ");
    assert_eq!(
        kind("{clipboard:0}"),
        TemplateErrorKind::BadClipboardPosition {
            max: MAX_CLIPBOARD_POSITION
        }
    );
    assert_eq!(
        kind("{clipboard:x}"),
        TemplateErrorKind::BadClipboardPosition {
            max: MAX_CLIPBOARD_POSITION
        }
    );
    assert_eq!(
        kind(&format!("{{clipboard:{}}}", MAX_CLIPBOARD_POSITION + 1)),
        TemplateErrorKind::BadClipboardPosition {
            max: MAX_CLIPBOARD_POSITION
        }
    );
    assert_eq!(
        Template::parse("{clipboard}")
            .unwrap()
            .max_clipboard_position(),
        1
    );
    assert_eq!(Template::parse("x").unwrap().max_clipboard_position(), 0);
}

#[test]
fn library_23_keys_split_the_text_into_pieces() {
    let t = Template::parse("a{key:tab}bc{key:enter}d{cursor}e").unwrap();
    assert!(t.has_keys());
    let out = t.render(&RenderContext::new(now(), &answers(&[])));
    assert_eq!(out.text, "abcde");
    assert_eq!(
        out.keys,
        vec![
            KeyStop {
                at: 1,
                key: TemplateKey::Tab
            },
            KeyStop {
                at: 3,
                key: TemplateKey::Enter
            }
        ]
    );
    assert_eq!(out.cursor_from_end, Some(1));
    assert_eq!(
        kind("{cursor}{key:tab}"),
        TemplateErrorKind::CursorBeforeKey
    );
    assert_eq!(
        kind("{key:f5}"),
        TemplateErrorKind::UnknownKey { found: "f5".into() }
    );
    assert_eq!(
        kind("{key}"),
        TemplateErrorKind::MissingArgument { placeholder: "key" }
    );
    assert!(!Template::parse("abc").unwrap().has_keys());
}

#[test]
fn library_23_a_number_field_and_a_calculation() {
    let t = Template::parse("{number:Qty|2} x {number:Unit price} = {calc:Qty*\"Unit price\"|2}")
        .unwrap();
    assert_eq!(t.prompts().len(), 2);
    assert_eq!(
        t.prompts()[0].kind,
        PromptKind::Number {
            default: Some("2".into())
        }
    );
    let given = answers(&[("Unit price", "3,5")]);
    let out = t.render(&RenderContext::new(now(), &given));
    assert_eq!(out.text, "2 x 3.5 = 7.00");
    // The decimal mark follows the locale.
    let out = render_in(
        "{number:A}+{number:B}={calc:A+B|1}",
        Locale::for_tag("de"),
        &[("A", "1.5"), ("B", "2")],
    );
    assert_eq!(out.text, "1,5+2=3,5");
    assert_eq!(answered("{number:A}{calc:A/0}", &[("A", "4")]), "4#DIV/0!");
    assert_eq!(
        kind("{calc:Missing*2}"),
        TemplateErrorKind::CalcUnknownName {
            name: "Missing".into()
        }
    );
    assert_eq!(kind("{calc:1+}"), TemplateErrorKind::BadCalc);
    assert_eq!(kind("{calc:1|11}"), TemplateErrorKind::BadCalc);
    assert_eq!(
        kind("{field:A}{calc:A+1}"),
        TemplateErrorKind::CalcUnknownName { name: "A".into() },
        "only a number field can be calculated"
    );
}

#[test]
fn library_23_a_value_reuses_an_answer_with_a_transform() {
    let t = Template::parse("{field:Title} / {value:Title|snake} / {value:title|upper}").unwrap();
    assert_eq!(t.prompts().len(), 1);
    assert_eq!(
        answered(
            "{field:Title} / {value:Title|snake} / {value:title|upper}",
            &[("Title", "Big Plan")]
        ),
        "Big Plan / big_plan / BIG PLAN"
    );
    // A `{value}` may come before the prompt it names.
    assert_eq!(
        answered("{value:Name|upper} {field:Name}", &[("Name", "ada")]),
        "ADA ada"
    );
    assert_eq!(
        kind("{value:Nope}"),
        TemplateErrorKind::ValueUnknownName {
            name: "Nope".into()
        }
    );
    assert_eq!(
        kind("{field:A}{value:A|shout}"),
        TemplateErrorKind::UnknownTransform {
            found: "shout".into()
        }
    );
}

#[test]
fn library_23_date_arithmetic_beyond_a_fixed_offset_chains_units() {
    assert_eq!(text("{date+1mo-1d}"), "2026-11-16");
    assert_eq!(
        text("{date+2w+3d:%a}"),
        "Sat".replace("Sat", &text("{date+17d:%a}"))
    );
}

// ---- LIBRARY-25: transforms ------------------------------------------------------------

#[test]
fn library_25_every_transform_has_a_placeholder_keyword() {
    let given = answers(&[("T", "Hello Big-World ok")]);
    let ctx = RenderContext::new(now(), &given);
    for (word, want) in [
        ("sentence", "Hello big-world ok"),
        ("camel", "helloBigWorldOk"),
        ("snake", "hello_big_world_ok"),
        ("urlencode", "Hello%20Big-World%20ok"),
        ("firstword", "Hello"),
        ("lastword", "ok"),
        ("upper", "HELLO BIG-WORLD OK"),
    ] {
        let t = Template::parse(&format!("{{field:T}}{{value:T|{word}}}")).unwrap();
        let got = t.render(&ctx).text;
        assert_eq!(got, format!("Hello Big-World ok{want}"), "{word}");
    }
    assert_eq!(
        Transform::keyword_list().split(", ").count(),
        Transform::ALL.len()
    );
}

// ---- LIBRARY-24: the regional format, not the UI language ------------------------------

#[test]
fn library_24_windows_short_date_patterns_are_read() {
    let day = |pattern: &str| DateStyle::from_pattern(pattern).map(|s| s.write(2026, 3, 5));
    assert_eq!(day("dd/MM/yyyy").as_deref(), Some("05/03/2026"));
    assert_eq!(day("d/M/yyyy").as_deref(), Some("5/3/2026"));
    assert_eq!(day("M/d/yyyy").as_deref(), Some("3/5/2026"));
    assert_eq!(day("dd.MM.yyyy").as_deref(), Some("05.03.2026"));
    assert_eq!(day("yyyy-MM-dd").as_deref(), Some("2026-03-05"));
    assert_eq!(day("yyyy/MM/dd").as_deref(), Some("2026/03/05"));
    assert_eq!(day("dd-MM-yy").as_deref(), Some("05-03-26"));
    assert_eq!(day("yyyy. MM. dd.").as_deref(), Some("2026.03.05"));
    assert_eq!(
        day("\u{200e}d\u{200e}/\u{200e}M\u{200e}/\u{200e}yyyy").as_deref(),
        Some("5/3/2026")
    );
    for odd in ["", "dd MMMM yyyy", "yyyy年M月d日", "dd/MM-yyyy", "dd/MM"] {
        assert_eq!(DateStyle::from_pattern(odd), None, "{odd:?}");
    }
}

#[test]
fn library_24_windows_short_time_patterns_are_read() {
    let time = |pattern: &str, hour: i8| {
        ClockStyle::from_pattern(pattern).map(|s| s.write(Lang::En, hour, 7))
    };
    assert_eq!(time("HH:mm", 9).as_deref(), Some("09:07"));
    assert_eq!(time("H:mm", 9).as_deref(), Some("9:07"));
    assert_eq!(time("h:mm tt", 15).as_deref(), Some("3:07 PM"));
    assert_eq!(time("hh:mm tt", 0).as_deref(), Some("12:07 AM"));
    assert_eq!(time("tt h:mm", 15).as_deref(), Some("PM 3:07"));
    assert_eq!(time("HH.mm", 9).as_deref(), Some("09.07"));
    for odd in ["", "HH", "h:mm", "HH:mm tt", "HH:mm:ss"] {
        assert_eq!(ClockStyle::from_pattern(odd), None, "{odd:?}");
    }
}

#[test]
fn library_24_the_regional_format_beats_the_ui_language() {
    // A German-language UI on a machine formatted as en-GB: German names, British numbers.
    let locale = Locale::for_tag("de").with_regional(Some("dd/MM/yyyy"), Some("HH:mm"));
    assert_eq!(locale.lang, Lang::of_tag("de"));
    assert_eq!(
        render_in("{date} {time} {datetime:%A}", locale, &[]).text,
        "17/10/2026 14:05 Samstag"
    );
    // An unreadable pattern keeps the table's style.
    let kept = Locale::for_tag("de").with_regional(Some("dd MMMM yyyy"), None);
    assert_eq!(kept, Locale::for_tag("de"));
    // No regional format at all is the tag table, the fallback.
    assert_eq!(
        Locale::for_tag("en").with_regional(None, None),
        Locale::for_tag("en")
    );
    // A 12-hour regional clock on a language that defaults to 24 hours.
    let twelve = Locale::for_tag("fr").with_regional(None, Some("h:mm tt"));
    assert_eq!(
        render_in("{time}", twelve, &[]).text,
        "2:05 PM".replace("PM", Lang::of_tag("fr").am_pm().1)
    );
}

#[test]
fn library_24_regional_tags_beyond_the_launch_locales_use_their_table_entry() {
    assert_eq!(Locale::for_tag("en-CA").lang, Lang::of_tag("en"));
    assert_eq!(Locale::for_tag("de-CH").date, Locale::for_tag("de").date);
}
