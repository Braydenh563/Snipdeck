//! Templates: item text with placeholders that are resolved when the item is used
//! (SPEC §4, §5.1a).
//!
//! [`Template::parse`] checks the text once and keeps it in parsed form; [`Template::render`]
//! then resolves it for a [`RenderContext`]. Nothing here reads the clock, the clipboard or
//! the system locale: the caller passes them in, so rendering is a pure function.
//!
//! # Syntax
//!
//! | Placeholder | Tier | Renders as |
//! |---|---|---|
//! | `{date}` | Free | the date in the context's [`Locale`] (ISO `2026-10-17` by default) |
//! | `{time}` | Free | the time in the context's [`Locale`] (`14:05`, or `2:05 PM` for a 12-hour locale) |
//! | `{datetime:FORMAT}` | Free | the date and time formatted by `FORMAT`, see below |
//! | `{clipboard}` | Free | the clipboard text the caller supplied, or nothing |
//! | `{cursor}` | Free | nothing; marks where the caret should land, see [`Rendered::cursor_from_end`] |
//! | `{field:Name}` / `{field:Name\|default}` | Pro | the answer to a text prompt |
//! | `{choice:Name\|a\|b\|c}` | Pro | the answer to a pick-one prompt |
//!
//! `{{` and `}}` produce literal braces ([`escape`] does this for arbitrary text). Any other
//! use of a brace is a [`TemplateError`] with the byte span of the problem: an unknown
//! placeholder is never silently kept as text. Names, defaults and options are trimmed, and
//! cannot contain `{`, `}` or `|`. A name used twice is one prompt (see [`Template::prompts`]).
//!
//! Substituted text (clipboard, answers) is never parsed again.
//!
//! # `{datetime:FORMAT}`
//!
//! A strftime-like subset. Everything except `%` is copied as written; these are replaced
//! (names are English, whatever the locale):
//!
//! | Spec | Meaning | Example |
//! |---|---|---|
//! | `%Y` `%y` | year, 4 digits / 2 digits | `2026` `26` |
//! | `%m` | month, 01-12 | `10` |
//! | `%B` `%b` | month name, full / abbreviated | `October` `Oct` |
//! | `%d` `%e` | day of month, zero / space padded | `07` ` 7` |
//! | `%j` | day of year, 001-366 | `290` |
//! | `%A` `%a` | weekday name, full / abbreviated | `Saturday` `Sat` |
//! | `%H` `%I` | hour, 00-23 / 01-12 | `14` `02` |
//! | `%M` `%S` | minute / second, 00-59 | `05` `09` |
//! | `%p` | `AM` or `PM` | `PM` |
//! | `%z` | UTC offset | `+0200` |
//! | `%F` `%T` | `%Y-%m-%d` / `%H:%M:%S` | `2026-10-17` `14:05:09` |
//! | `%%` | a literal `%` | `%` |
//!
//! Any other `%x`, and a `%` at the end, is rejected when the template is parsed, so
//! rendering cannot fail.

use core::fmt::{self, Write as _};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use jiff::Zoned;
pub use jiff::tz::TimeZone;

use crate::{Entitlements, Error, Feature, Timestamp};

#[cfg(test)]
mod tests;

/// Placeholder keywords, for hints in error messages.
const KEYWORDS: [&str; 7] = [
    "date",
    "time",
    "datetime",
    "clipboard",
    "cursor",
    "field",
    "choice",
];

/// A parsed template. Build with [`Template::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    parts: Vec<Part>,
    prompts: Vec<Prompt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Date,
    Time,
    DateTime(Vec<DtPart>),
    Clipboard,
    Cursor,
    /// Index into `Template::prompts`.
    Prompt(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DtPart {
    Lit(String),
    Spec(Spec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spec {
    Year4,
    Year2,
    Month,
    MonthName,
    MonthAbbr,
    Day,
    DaySpace,
    DayOfYear,
    Weekday,
    WeekdayAbbr,
    Hour24,
    Hour12,
    Minute,
    Second,
    AmPm,
    Offset,
    IsoDate,
    IsoTime,
}

/// A question to put to the user before rendering: one per distinct name, from
/// [`Template::prompts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// The label, and the key in [`RenderContext::answers`].
    pub name: String,
    /// What kind of answer is wanted.
    pub kind: PromptKind,
}

/// The kind of answer a [`Prompt`] wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    /// Free text (`{field:Name}`), pre-filled with `default` if the template gave one.
    Field {
        /// The text to pre-fill, from the first use that has one.
        default: Option<String>,
    },
    /// One of a fixed set (`{choice:Name|a|b}`); the first option is the default.
    Choice {
        /// The options, in the order written. Never empty.
        options: Vec<String>,
    },
}

/// The result of [`Template::render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The resolved text.
    pub text: String,
    /// Where the caret should end up, if the template had a `{cursor}`: how many characters of
    /// [`Rendered::text`] come after it, i.e. how many Left-arrow presses move the caret there
    /// after pasting. Counted in Unicode scalar values, with `\r\n` as one.
    pub cursor_from_end: Option<usize>,
}

/// How `{date}` is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateStyle {
    /// `2026-10-17`.
    #[default]
    Iso,
    /// `17/10/2026`.
    DayMonthYear,
    /// `10/17/2026`.
    MonthDayYear,
    /// `17.10.2026`.
    DotDayMonthYear,
    /// `2026/10/17`.
    YearMonthDaySlash,
}

/// How `{time}` is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClockStyle {
    /// `14:05`.
    #[default]
    H24,
    /// `2:05 PM`.
    H12,
}

/// The date and time conventions of a UI locale. The default is ISO dates and a 24-hour
/// clock, which reads the same everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Locale {
    /// How `{date}` is written.
    pub date: DateStyle,
    /// How `{time}` is written.
    pub clock: ClockStyle,
}

impl Locale {
    /// The conventions for a language tag such as `de`, `pt-BR` or `zh_Hans`, covering
    /// Snipdeck's launch locales (SPEC §7). Matching ignores case and treats `_` as `-`;
    /// an unknown tag gives the ISO default.
    pub fn for_tag(tag: &str) -> Self {
        let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
        let (date, clock) = match tag.as_str() {
            "en-gb" | "en-au" | "en-nz" | "en-ie" | "en-in" => {
                (DateStyle::DayMonthYear, ClockStyle::H24)
            }
            _ => match tag.split('-').next().unwrap_or_default() {
                "en" => (DateStyle::MonthDayYear, ClockStyle::H12),
                "es" | "fr" | "pt" => (DateStyle::DayMonthYear, ClockStyle::H24),
                "de" => (DateStyle::DotDayMonthYear, ClockStyle::H24),
                "ja" | "zh" => (DateStyle::YearMonthDaySlash, ClockStyle::H24),
                _ => (DateStyle::Iso, ClockStyle::H24),
            },
        };
        Self { date, clock }
    }
}

/// Everything [`Template::render`] needs from the outside world.
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    /// The moment `{date}`, `{time}` and `{datetime:…}` show.
    pub now: Timestamp,
    /// The time zone to show it in. The app passes the user's; the default is UTC.
    pub tz: TimeZone,
    /// How `{date}` and `{time}` are written. Defaults to ISO, 24-hour.
    pub locale: Locale,
    /// The clipboard text for `{clipboard}`; `None` renders as nothing.
    pub clipboard: Option<&'a str>,
    /// Answers to the template's [`Prompt`]s, by name.
    pub answers: &'a BTreeMap<String, String>,
}

impl<'a> RenderContext<'a> {
    /// A context for `now` in UTC with ISO formats, no clipboard, and these answers.
    pub fn new(now: Timestamp, answers: &'a BTreeMap<String, String>) -> Self {
        Self {
            now,
            tz: TimeZone::UTC,
            locale: Locale::default(),
            clipboard: None,
            answers,
        }
    }

    /// Shows times in `tz`.
    #[must_use]
    pub fn with_tz(mut self, tz: TimeZone) -> Self {
        self.tz = tz;
        self
    }

    /// Writes dates and times by `locale`.
    #[must_use]
    pub fn with_locale(mut self, locale: Locale) -> Self {
        self.locale = locale;
        self
    }

    /// Supplies the text for `{clipboard}`.
    #[must_use]
    pub fn with_clipboard(mut self, clipboard: &'a str) -> Self {
        self.clipboard = Some(clipboard);
        self
    }
}

/// A template that could not be parsed, with where.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} (bytes {}..{})", .span.start, .span.end)]
pub struct TemplateError {
    /// The byte range of the offending text in the source, on `char` boundaries.
    pub span: Range<usize>,
    /// What is wrong.
    pub kind: TemplateErrorKind,
}

/// What is wrong with a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateErrorKind {
    /// A single `}` that closes nothing.
    UnmatchedClose,
    /// A `{` with no `}` after it before the next `{` or the end.
    Unterminated,
    /// `{}`.
    EmptyPlaceholder,
    /// A `field` or `choice` without a name.
    EmptyName,
    /// A `choice` with no options.
    NoOptions,
    /// A `choice` option that is empty.
    EmptyOption,
    /// A second `{cursor}`.
    DuplicateCursor,
    /// A `%` at the end of a `{datetime:…}` format.
    TrailingPercent,
    /// A placeholder name Snipdeck does not know.
    Unknown {
        /// The name as written.
        found: String,
        /// The closest known name, if one is near enough to be a likely typo.
        suggestion: Option<&'static str>,
    },
    /// `{date:…}`, `{time:…}`, `{clipboard:…}` or `{cursor:…}`.
    UnexpectedArgument {
        /// The placeholder that takes none.
        placeholder: &'static str,
    },
    /// `{datetime}`, `{field}` or `{choice}` without the part after the colon.
    MissingArgument {
        /// The placeholder that needs one.
        placeholder: &'static str,
    },
    /// One name used for two different prompts (a field and a choice, or different options).
    ConflictingPrompt {
        /// The name.
        name: String,
    },
    /// A `%x` in a `{datetime:…}` format that is not in the supported set.
    UnknownFormatSpecifier {
        /// The character after the `%`.
        specifier: char,
    },
}

fn usage(keyword: &str) -> &'static str {
    match keyword {
        "date" => "{date}",
        "time" => "{time}",
        "datetime" => "{datetime:FORMAT}",
        "clipboard" => "{clipboard}",
        "cursor" => "{cursor}",
        "field" => "{field:Name}",
        "choice" => "{choice:Name|a|b}",
        _ => "",
    }
}

impl fmt::Display for TemplateErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmatchedClose => f.write_str("`}` closes nothing; write `}}` for a literal `}`"),
            Self::Unterminated => f.write_str("`{` is never closed; write `{{` for a literal `{`"),
            Self::EmptyPlaceholder => f.write_str("empty placeholder `{}`"),
            Self::EmptyName => f.write_str("the prompt needs a name, e.g. `{field:Name}`"),
            Self::NoOptions => f.write_str("a choice needs options, e.g. `{choice:Name|a|b}`"),
            Self::EmptyOption => f.write_str("a choice option is empty"),
            Self::DuplicateCursor => f.write_str("only one `{cursor}` is allowed"),
            Self::TrailingPercent => {
                f.write_str("a `%` ends the format; write `%%` for a literal `%`")
            }
            Self::Unknown { found, suggestion } => {
                write!(f, "unknown placeholder `{{{found}}}`")?;
                if let Some(keyword) = suggestion {
                    write!(f, ", did you mean `{}`?", usage(keyword))?;
                }
                f.write_str(
                    ". Placeholders are {date}, {time}, {datetime:FORMAT}, {clipboard}, \
                     {cursor}, {field:Name}, {field:Name|default} and {choice:Name|a|b}. \
                     To write a literal brace, double it: {{ or }}",
                )
            }
            Self::UnexpectedArgument { placeholder } => {
                write!(f, "`{{{placeholder}}}` takes no argument")
            }
            Self::MissingArgument { placeholder } => {
                write!(
                    f,
                    "`{placeholder}` needs an argument: `{}`",
                    usage(placeholder)
                )
            }
            Self::ConflictingPrompt { name } => write!(
                f,
                "`{name}` is used for two different prompts; use the same kind and options \
                 each time, or pick another name"
            ),
            Self::UnknownFormatSpecifier { specifier } => write!(
                f,
                "unknown date format `%{specifier}`; supported: \
                 %Y %y %m %B %b %d %e %j %A %a %H %I %M %S %p %z %F %T %%"
            ),
        }
    }
}

/// `text` with every brace doubled, so it parses back to exactly `text`.
pub fn escape(text: &str) -> String {
    text.replace('{', "{{").replace('}', "}}")
}

fn err<T>(span: Range<usize>, kind: TemplateErrorKind) -> Result<T, TemplateError> {
    Err(TemplateError { span, kind })
}

struct Parser {
    parts: Vec<Part>,
    prompts: Vec<Prompt>,
    prompt_index: HashMap<String, usize>,
    has_cursor: bool,
}

impl Template {
    /// Parses `src`.
    ///
    /// # Errors
    ///
    /// The first problem found, as a [`TemplateError`] with its byte span: unbalanced
    /// braces, an unknown placeholder, a missing or unexpected argument, a bad date format,
    /// a second `{cursor}`, or a name used for two different prompts.
    pub fn parse(src: &str) -> Result<Self, TemplateError> {
        let mut parser = Parser {
            parts: Vec::new(),
            prompts: Vec::new(),
            prompt_index: HashMap::new(),
            has_cursor: false,
        };
        let bytes = src.as_bytes();
        let mut text = String::new();
        let mut i = 0;
        while i < bytes.len() {
            // `{` and `}` are ASCII, so every index used to slice `src` is a char boundary.
            let next = bytes[i..]
                .iter()
                .position(|b| matches!(b, b'{' | b'}'))
                .map_or(bytes.len(), |p| i + p);
            text.push_str(&src[i..next]);
            i = next;
            let Some(&brace) = bytes.get(i) else { break };
            if bytes.get(i + 1) == Some(&brace) {
                text.push(char::from(brace));
                i += 2;
            } else if brace == b'}' {
                return err(i..i + 1, TemplateErrorKind::UnmatchedClose);
            } else {
                let body_start = i + 1;
                let close = bytes[body_start..]
                    .iter()
                    .position(|b| matches!(b, b'{' | b'}'))
                    .map(|p| body_start + p);
                let Some(close) = close else {
                    return err(i..bytes.len(), TemplateErrorKind::Unterminated);
                };
                if bytes[close] == b'{' {
                    return err(i..close, TemplateErrorKind::Unterminated);
                }
                if !text.is_empty() {
                    parser.parts.push(Part::Text(core::mem::take(&mut text)));
                }
                parser.placeholder(&src[body_start..close], i..close + 1)?;
                i = close + 1;
            }
        }
        if !text.is_empty() {
            parser.parts.push(Part::Text(text));
        }
        Ok(Self {
            parts: parser.parts,
            prompts: parser.prompts,
        })
    }

    /// The questions to ask before rendering, one per distinct name, in order of first use.
    /// Empty for a template with only static placeholders.
    pub fn prompts(&self) -> Vec<Prompt> {
        self.prompts.clone()
    }

    /// Whether using this template needs `feature`: only [`Feature::PromptedFields`], and only
    /// if it has a `{field}` or `{choice}`.
    pub fn requires(&self, feature: Feature) -> bool {
        match feature {
            Feature::PromptedFields => !self.prompts.is_empty(),
            Feature::UnlimitedBoards
            | Feature::SensitiveEncryption
            | Feature::FloatingLists
            | Feature::PasteStack => false,
        }
    }

    /// Checks `entitlements` allow every feature this template needs.
    ///
    /// # Errors
    ///
    /// [`Error::ProRequired`] naming the missing feature.
    pub fn check(&self, entitlements: &Entitlements) -> Result<(), Error> {
        for &feature in Feature::ALL {
            if self.requires(feature) {
                entitlements.require(feature)?;
            }
        }
        Ok(())
    }

    /// Resolves the template. Missing pieces degrade rather than fail: no clipboard renders
    /// as nothing, an unanswered field as its default (or nothing), an unanswered choice as
    /// its first option. An answer is used as given, even if empty or not among a choice's
    /// options.
    pub fn render(&self, ctx: &RenderContext<'_>) -> Rendered {
        let when = ctx.now.to_zoned(ctx.tz.clone());
        let mut out = String::new();
        let mut cursor = None;
        for part in &self.parts {
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Date => out.push_str(&date_string(&when, ctx.locale.date)),
                Part::Time => out.push_str(&time_string(&when, ctx.locale.clock)),
                Part::DateTime(parts) => {
                    for part in parts {
                        match part {
                            DtPart::Lit(text) => out.push_str(text),
                            DtPart::Spec(spec) => write_spec(&mut out, &when, *spec),
                        }
                    }
                }
                Part::Clipboard => out.push_str(ctx.clipboard.unwrap_or_default()),
                Part::Cursor => cursor = Some(out.len()),
                Part::Prompt(index) => {
                    if let Some(prompt) = self.prompts.get(*index) {
                        out.push_str(&answer(prompt, ctx.answers));
                    }
                }
            }
        }
        let cursor_from_end = cursor.map(|at| chars_after(&out, at));
        Rendered {
            text: out,
            cursor_from_end,
        }
    }
}

fn answer(prompt: &Prompt, answers: &BTreeMap<String, String>) -> String {
    if let Some(given) = answers.get(&prompt.name) {
        return given.clone();
    }
    match &prompt.kind {
        PromptKind::Field { default } => default.clone().unwrap_or_default(),
        PromptKind::Choice { options } => options.first().cloned().unwrap_or_default(),
    }
}

/// Characters after byte offset `at`, counting `\r\n` once.
fn chars_after(text: &str, at: usize) -> usize {
    let mut count = 0;
    let mut chars = text.get(at..).unwrap_or_default().chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
        }
        count += 1;
    }
    count
}

impl Parser {
    /// Handles the text between a pair of braces; `span` covers the braces too.
    fn placeholder(&mut self, body: &str, span: Range<usize>) -> Result<(), TemplateError> {
        if body.is_empty() {
            return err(span, TemplateErrorKind::EmptyPlaceholder);
        }
        let (key, arg) = match body.split_once(':') {
            Some((key, arg)) => (key, Some(arg)),
            None => (body, None),
        };
        let simple = match key {
            "date" => Some(("date", Part::Date)),
            "time" => Some(("time", Part::Time)),
            "clipboard" => Some(("clipboard", Part::Clipboard)),
            "cursor" => Some(("cursor", Part::Cursor)),
            _ => None,
        };
        if let Some((placeholder, part)) = simple {
            if arg.is_some() {
                return err(span, TemplateErrorKind::UnexpectedArgument { placeholder });
            }
            if matches!(part, Part::Cursor) {
                if self.has_cursor {
                    return err(span, TemplateErrorKind::DuplicateCursor);
                }
                self.has_cursor = true;
            }
            self.parts.push(part);
            return Ok(());
        }
        match key {
            "datetime" => {
                let format = arg.unwrap_or_default();
                if format.is_empty() {
                    return err(
                        span,
                        TemplateErrorKind::MissingArgument {
                            placeholder: "datetime",
                        },
                    );
                }
                // `datetime:` is 9 bytes after the opening brace.
                let parts = parse_format(format, span.start + 1 + "datetime:".len())?;
                self.parts.push(Part::DateTime(parts));
                Ok(())
            }
            "field" | "choice" => {
                let Some(arg) = arg else {
                    let placeholder = if key == "field" { "field" } else { "choice" };
                    return err(span, TemplateErrorKind::MissingArgument { placeholder });
                };
                let mut pieces = arg.split('|').map(str::trim);
                let name = pieces.next().unwrap_or_default();
                if name.is_empty() {
                    return err(span, TemplateErrorKind::EmptyName);
                }
                let kind = if key == "field" {
                    let default = pieces.next().filter(|d| !d.is_empty());
                    PromptKind::Field {
                        default: default.map(str::to_owned),
                    }
                } else {
                    let options: Vec<String> = pieces.map(str::to_owned).collect();
                    if options.is_empty() {
                        return err(span, TemplateErrorKind::NoOptions);
                    }
                    if options.iter().any(String::is_empty) {
                        return err(span, TemplateErrorKind::EmptyOption);
                    }
                    PromptKind::Choice { options }
                };
                let index = self.add_prompt(name, kind, span)?;
                self.parts.push(Part::Prompt(index));
                Ok(())
            }
            _ => err(
                span,
                TemplateErrorKind::Unknown {
                    found: key.to_owned(),
                    suggestion: suggest(key),
                },
            ),
        }
    }

    /// Registers a prompt use and returns its index, merging with an earlier use of the name.
    fn add_prompt(
        &mut self,
        name: &str,
        kind: PromptKind,
        span: Range<usize>,
    ) -> Result<usize, TemplateError> {
        let Some(&index) = self.prompt_index.get(name) else {
            let index = self.prompts.len();
            self.prompt_index.insert(name.to_owned(), index);
            self.prompts.push(Prompt {
                name: name.to_owned(),
                kind,
            });
            return Ok(index);
        };
        let conflict = || TemplateErrorKind::ConflictingPrompt {
            name: name.to_owned(),
        };
        let Some(existing) = self.prompts.get_mut(index) else {
            return err(span, conflict());
        };
        match (&mut existing.kind, kind) {
            (PromptKind::Field { default }, PromptKind::Field { default: later }) => {
                if default.is_none() {
                    *default = later;
                }
            }
            (PromptKind::Choice { options }, PromptKind::Choice { options: later })
                if *options == later => {}
            _ => return err(span, conflict()),
        }
        Ok(index)
    }
}

/// The closest keyword to `key` if it is near enough to be a typo.
fn suggest(key: &str) -> Option<&'static str> {
    let lower = key.to_ascii_lowercase();
    KEYWORDS
        .iter()
        .map(|&kw| (edit_distance(&lower, kw), kw))
        .filter(|&(distance, kw)| distance <= if kw.len() <= 4 { 1 } else { 2 })
        .min_by_key(|&(distance, _)| distance)
        .map(|(_, kw)| kw)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row.first().copied().unwrap_or_default();
        if let Some(first) = row.first_mut() {
            *first = i + 1;
        }
        for (j, &cb) in b.iter().enumerate() {
            let above = row.get(j + 1).copied().unwrap_or_default();
            let left = row.get(j).copied().unwrap_or_default();
            let cost = usize::from(ca != cb);
            if let Some(cell) = row.get_mut(j + 1) {
                *cell = (above + 1).min(left + 1).min(diagonal + cost);
            }
            diagonal = above;
        }
    }
    row.last().copied().unwrap_or_default()
}

/// Parses a `{datetime:…}` format; `offset` is where `format` starts in the template source.
fn parse_format(format: &str, offset: usize) -> Result<Vec<DtPart>, TemplateError> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let mut chars = format.char_indices();
    while let Some((at, c)) = chars.next() {
        if c != '%' {
            lit.push(c);
            continue;
        }
        let Some((_, spec)) = chars.next() else {
            return err(
                offset + at..offset + at + 1,
                TemplateErrorKind::TrailingPercent,
            );
        };
        let parsed = match spec {
            '%' => {
                lit.push('%');
                continue;
            }
            'Y' => Spec::Year4,
            'y' => Spec::Year2,
            'm' => Spec::Month,
            'B' => Spec::MonthName,
            'b' => Spec::MonthAbbr,
            'd' => Spec::Day,
            'e' => Spec::DaySpace,
            'j' => Spec::DayOfYear,
            'A' => Spec::Weekday,
            'a' => Spec::WeekdayAbbr,
            'H' => Spec::Hour24,
            'I' => Spec::Hour12,
            'M' => Spec::Minute,
            'S' => Spec::Second,
            'p' => Spec::AmPm,
            'z' => Spec::Offset,
            'F' => Spec::IsoDate,
            'T' => Spec::IsoTime,
            specifier => {
                return err(
                    offset + at..offset + at + 1 + specifier.len_utf8(),
                    TemplateErrorKind::UnknownFormatSpecifier { specifier },
                );
            }
        };
        if !lit.is_empty() {
            parts.push(DtPart::Lit(core::mem::take(&mut lit)));
        }
        parts.push(DtPart::Spec(parsed));
    }
    if !lit.is_empty() {
        parts.push(DtPart::Lit(lit));
    }
    Ok(parts)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn month_name(when: &Zoned) -> &'static str {
    usize::try_from(when.month() - 1)
        .ok()
        .and_then(|i| MONTHS.get(i))
        .copied()
        .unwrap_or_default()
}

fn weekday_name(when: &Zoned) -> &'static str {
    usize::try_from(when.weekday().to_monday_zero_offset())
        .ok()
        .and_then(|i| WEEKDAYS.get(i))
        .copied()
        .unwrap_or_default()
}

/// The first three letters of an ASCII month or weekday name.
fn abbr(name: &str) -> &str {
    name.get(..3).unwrap_or(name)
}

fn hour12(when: &Zoned) -> i8 {
    match when.hour() % 12 {
        0 => 12,
        h => h,
    }
}

fn am_pm(when: &Zoned) -> &'static str {
    if when.hour() < 12 { "AM" } else { "PM" }
}

fn date_string(when: &Zoned, style: DateStyle) -> String {
    let (y, m, d) = (when.year(), when.month(), when.day());
    match style {
        DateStyle::Iso => format!("{y:04}-{m:02}-{d:02}"),
        DateStyle::DayMonthYear => format!("{d:02}/{m:02}/{y:04}"),
        DateStyle::MonthDayYear => format!("{m:02}/{d:02}/{y:04}"),
        DateStyle::DotDayMonthYear => format!("{d:02}.{m:02}.{y:04}"),
        DateStyle::YearMonthDaySlash => format!("{y:04}/{m:02}/{d:02}"),
    }
}

fn time_string(when: &Zoned, style: ClockStyle) -> String {
    let minute = when.minute();
    match style {
        ClockStyle::H24 => format!("{:02}:{minute:02}", when.hour()),
        ClockStyle::H12 => format!("{}:{minute:02} {}", hour12(when), am_pm(when)),
    }
}

fn write_spec(out: &mut String, when: &Zoned, spec: Spec) {
    // Writing to a `String` cannot fail.
    let _ = match spec {
        Spec::Year4 => write!(out, "{:04}", when.year()),
        Spec::Year2 => write!(out, "{:02}", when.year().rem_euclid(100)),
        Spec::Month => write!(out, "{:02}", when.month()),
        Spec::MonthName => out.write_str(month_name(when)),
        Spec::MonthAbbr => out.write_str(abbr(month_name(when))),
        Spec::Day => write!(out, "{:02}", when.day()),
        Spec::DaySpace => write!(out, "{:2}", when.day()),
        Spec::DayOfYear => write!(out, "{:03}", when.day_of_year()),
        Spec::Weekday => out.write_str(weekday_name(when)),
        Spec::WeekdayAbbr => out.write_str(abbr(weekday_name(when))),
        Spec::Hour24 => write!(out, "{:02}", when.hour()),
        Spec::Hour12 => write!(out, "{:02}", hour12(when)),
        Spec::Minute => write!(out, "{:02}", when.minute()),
        Spec::Second => write!(out, "{:02}", when.second()),
        Spec::AmPm => out.write_str(am_pm(when)),
        Spec::Offset => {
            let seconds = when.offset().seconds();
            let sign = if seconds < 0 { '-' } else { '+' };
            let minutes = seconds.unsigned_abs() / 60;
            write!(out, "{sign}{:02}{:02}", minutes / 60, minutes % 60)
        }
        Spec::IsoDate => out.write_str(&date_string(when, DateStyle::Iso)),
        Spec::IsoTime => write!(
            out,
            "{:02}:{:02}:{:02}",
            when.hour(),
            when.minute(),
            when.second()
        ),
    };
}
