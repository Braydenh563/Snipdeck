//! Templates: item text with placeholders that are resolved when the item is used
//! (SPEC §4, §5.1a).
//!
//! [`Template::parse`] checks the text once and keeps it in parsed form; the render methods
//! then resolve it for a [`RenderContext`]. Nothing here reads the clock, the clipboard, the
//! selection, the system locale or the library: the caller passes them in (the library as a
//! [`SnippetSource`]), so parsing and rendering are pure functions.
//!
//! # Which render method to call
//!
//! The app should use [`Template::try_render`]: it enforces the output size limit,
//! [`MAX_RENDERED_BYTES`], and reports it as a [`RenderError`]. [`Template::render`] is the
//! unchecked primitive: when the output would be over the limit it cuts it there instead of
//! failing.
//!
//! # Syntax
//!
//! | Placeholder | Renders as |
//! |---|---|
//! | `{date}` `{time}` `{datetime}` | the date, the time, or both, in the context's [`Locale`] (ISO `2026-10-17` and `14:05` by default) |
//! | `{date:FORMAT}` `{time:FORMAT}` `{datetime:FORMAT}` | the same, formatted by `FORMAT`, see below |
//! | `{date+3d}`, `{time-90min}`, `{datetime+1mo:%F}` | the same, moved by an offset, see below |
//! | `{uuid}` | a random version 4 UUID, new for each use of the placeholder |
//! | `{clipboard}` `{clipboard:2}` | the clipboard text the caller supplied (`:2` is the copy before it, up to [`MAX_CLIPBOARD_POSITION`]), or nothing |
//! | `{selection}` | the text that was selected in the target app, or nothing |
//! | `{cursor}` | nothing; marks where the caret should land, see [`Rendered::cursor_from_end`] |
//! | `{key:tab}` `{key:enter}` | a key press after the text before it, for filling a form, see [`Rendered::keys`] |
//! | `{field:Name}` / `{field:Name\|default}` | the answer to a text prompt; `{field:Name\|}` may be left empty |
//! | `{multiline:Name\|default}` | the answer to a text prompt of several lines |
//! | `{number:Name\|default}` | the answer to a number prompt |
//! | `{choice:Name\|a\|*b\|c}` | the answer to a pick-one prompt; `*` marks the start option, an empty option means "none" |
//! | `{pickdate:Name\|FORMAT}` | a day picked in the form, shown by `FORMAT` or the locale's date style |
//! | `{optional:Name}...{/optional}` | the text between, only when the box is ticked |
//! | `{value:Name\|upper}` | the answer to prompt `Name` again, shaped by a transform |
//! | `{calc:Qty*Price\|2}` | the sum of number prompts, with 2 decimals (optional) |
//! | `{snippet:Title}` | the text of another item, put in place when the template is read |
//!
//! `{clipboard|upper}` and `{selection|title}` shape the text by a [`transform::Transform`];
//! the keywords are listed by [`transform::Transform::keyword_list`].
//!
//! `{{` and `}}` produce literal braces ([`escape`] does this for arbitrary text). Any other
//! use of a brace is a [`TemplateError`] with the byte span of the problem: an unknown
//! placeholder is never silently kept as text, and neither are surplus parts
//! (`{field:Name|default|extra}`) or repeated choice options. Keywords, names, defaults and
//! options are trimmed (`{ date }` is `{date}`). Inside a placeholder a backslash writes a
//! character that would end a part: `\|`, `\{`, `\}` and `\\` (and `\*` for an option that
//! starts with a star), so a default can be `a|b`. Prompt names are compared without regard to
//! case: `{field:Name}` and `{field:name}` are one prompt, labelled as first written.
//!
//! Substituted text (clipboard, selection, answers, nested snippets' answers) is never parsed
//! again; a nested snippet's own text is, once, when the template is parsed.
//!
//! # Nested snippets
//!
//! `{snippet:Title}` is replaced by the parsed text of the item the [`SnippetSource`] finds
//! under that title (or trigger). Its placeholders join the outer template's: a prompt both
//! use is one prompt. Nesting is limited to [`MAX_SNIPPET_DEPTH`] levels and
//! [`MAX_SNIPPETS`] uses; a snippet that contains itself is refused.
//!
//! # Offsets
//!
//! `date`, `time` and `datetime` take one or more offsets straight after the keyword:
//! a sign, a whole number and a unit, as in `{date+3d}`, `{date-1w:%Y-%m-%d}`,
//! `{time+90min}` or `{date+1mo-1d}`. Units: `d` days, `w` weeks, `mo` months, `y` years
//! (calendar arithmetic in the context's time zone; a month end clamps, so 31 January plus
//! `1mo` is 28 February, and a wall-clock time that a clock change skips moves forward),
//! and `h` hours, `min` minutes (elapsed time). At most [`MAX_OFFSETS`] offsets, each
//! bounded (100 y, 1200 mo, 5214 w, 36500 d, 876000 h, 52560000 min), so rendering stays
//! infallible. There is no `m` or `M`: it would be months or minutes.
//!
//! # Date formats
//!
//! A strftime-like subset. Everything except `%` is copied as written; these are replaced
//! (names are in the [`Locale`]'s language):
//!
//! | Spec | Meaning | Example |
//! |---|---|---|
//! | `%Y` `%y` `%G` | year, 4 digits / 2 digits / ISO week year | `2026` `26` |
//! | `%m` | month, 01-12 | `10` |
//! | `%B` `%b` `%h` | month name, full / abbreviated | `October` `Oct` |
//! | `%d` `%e` | day of month, zero / space padded | `07` ` 7` |
//! | `%j` | day of year, 001-366 | `290` |
//! | `%A` `%a` | weekday name, full / abbreviated | `Saturday` `Sat` |
//! | `%u` | weekday number, Monday is 1 | `6` |
//! | `%V` | ISO week number, 01-53 | `42` |
//! | `%q` | quarter, 1-4 | `4` |
//! | `%H` `%I` | hour, 00-23 / 01-12 | `14` `02` |
//! | `%M` `%S` | minute / second, 00-59 | `05` `09` |
//! | `%p` | AM or PM, as the locale says | `PM` |
//! | `%z` `%Z` | UTC offset / time zone name | `+0200` `Europe/Berlin` |
//! | `%F` `%T` `%R` | `%Y-%m-%d` / `%H:%M:%S` / `%H:%M` | `2026-10-17` `14:05:09` `14:05` |
//! | `%%` | a literal `%` | `%` |
//!
//! A `-` after the `%` drops the padding (`%-d` is `7`), a `_` pads with spaces (`%_m`).
//! Any other `%x`, and a `%` at the end, is rejected when the template is parsed, so
//! rendering cannot fail.

use core::fmt::{self, Write as _};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use jiff::Zoned;
pub use jiff::tz::TimeZone;
use unicode_segmentation::UnicodeSegmentation as _;

use crate::Timestamp;

mod calc;
mod locale;
pub mod transform;

pub use calc::{DIV_ZERO, format_number, parse_number};
pub use locale::{ClockStyle, DateOrder, DateStyle, Lang, Locale};
use transform::Transform;

#[cfg(test)]
mod findings;
#[cfg(test)]
mod tests;

/// The most bytes a render produces: 16 MiB.
///
/// Output grows with (uses of `{clipboard}` or of a prompt) times (the size of what is
/// substituted), so a small template and a large clipboard could otherwise exhaust memory.
/// SPEC §5.4 caps inline editing at 16 KB and the viewer at 5 MB; a pasted result larger than
/// this is a mistake, not a use case.
pub const MAX_RENDERED_BYTES: usize = 16 * 1024 * 1024;

/// The most distinct prompts (fields, choices, numbers, dates and tick boxes) one template may
/// have. The limit is about the cost of the form (it validates and lists every prompt), not
/// about syntax; a template with more is refused when it is parsed.
pub const MAX_PROMPTS: usize = 64;

/// The most offsets one `{date+…}` may chain.
pub const MAX_OFFSETS: usize = 16;

/// The deepest `{snippet:…}` inside `{snippet:…}`.
pub const MAX_SNIPPET_DEPTH: usize = 4;

/// The most `{snippet:…}` uses one template may expand, counting nested ones.
pub const MAX_SNIPPETS: usize = 32;

/// The furthest back `{clipboard:N}` reaches: the 25 entries the Windows clipboard history
/// keeps.
pub const MAX_CLIPBOARD_POSITION: usize = 25;

/// Placeholder keywords, for hints in error messages.
const KEYWORDS: [&str; 17] = [
    "uuid",
    "date",
    "time",
    "datetime",
    "clipboard",
    "selection",
    "cursor",
    "key",
    "field",
    "choice",
    "multiline",
    "number",
    "pickdate",
    "optional",
    "value",
    "calc",
    "snippet",
];

/// The answer that ticks an `{optional:Name}` section; any other answer, or none, leaves it out.
pub const TOGGLE_ON: &str = "true";

/// A parsed template. Build with [`Template::parse`].
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    parts: Vec<Part>,
    prompts: Vec<Prompt>,
}

/// What a clock placeholder shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Moment {
    Date,
    Time,
    DateTime,
}

/// A key a template can press between its pieces of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateKey {
    /// `{key:tab}`: move to the next field of a form.
    Tab,
    /// `{key:enter}`: press Enter.
    Enter,
}

impl TemplateKey {
    /// The word a placeholder uses.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Enter => "enter",
        }
    }
}

/// A key press in a [`Rendered`] text: after the first `at` bytes of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStop {
    /// Bytes of [`Rendered::text`] that come before the key press (on a character boundary).
    pub at: usize,
    /// The key.
    pub key: TemplateKey,
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Text(String),
    /// `{date…}`, `{time…}`, `{datetime…}`: what to show, the offsets and the format, if any.
    Moment(Moment, Vec<Offset>, Option<Vec<DtPart>>),
    Uuid,
    /// `{clipboard:N|transform}`: position (1 is the clipboard now) and shape.
    Clipboard(usize, Transform),
    Selection(Transform),
    Cursor,
    Key(TemplateKey),
    /// Index into `Template::prompts`.
    Prompt(usize),
    /// `{value:Name|transform}`: the prompt's answer again, shaped.
    Value(ValueRef, Transform),
    /// `{calc:expression|decimals}`, and where it was written (for errors).
    Calc(calc::Expr, Option<usize>, Range<usize>),
    /// `{optional:Name}...{/optional}`: the prompt index of the tick box, and the inside.
    /// Never nested.
    Section(usize, Vec<Part>),
    /// `{pickdate:Name|FORMAT}`: the prompt index and this use's format, if any.
    PickDate(usize, Option<Vec<DtPart>>),
}

/// One `+3d` of a date placeholder, already checked against its unit's bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Offset {
    amount: i64,
    unit: Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Day,
    Week,
    Month,
    Year,
    Hour,
    Minute,
}

impl Unit {
    /// The unit written `word`, if it is one of `d w mo y h min`.
    fn of(word: &str) -> Option<Self> {
        Some(match word {
            "d" => Self::Day,
            "w" => Self::Week,
            "mo" => Self::Month,
            "y" => Self::Year,
            "h" => Self::Hour,
            "min" => Self::Minute,
            _ => return None,
        })
    }

    const fn word(self) -> &'static str {
        match self {
            Self::Day => "d",
            Self::Week => "w",
            Self::Month => "mo",
            Self::Year => "y",
            Self::Hour => "h",
            Self::Minute => "min",
        }
    }

    /// The largest amount, so that a chain of [`MAX_OFFSETS`] stays far inside jiff's range.
    const fn max(self) -> u64 {
        match self {
            Self::Day => 36_500,
            Self::Week => 5_214,
            Self::Month => 1_200,
            Self::Year => 100,
            Self::Hour => 876_000,
            Self::Minute => 52_560_000,
        }
    }

    /// `when` moved by `amount` of this unit; `None` if the result is out of range.
    fn apply(self, when: &Zoned, amount: i64) -> Option<Zoned> {
        let span = jiff::Span::new();
        let span = match self {
            Self::Day => span.try_days(amount),
            Self::Week => span.try_weeks(amount),
            Self::Month => span.try_months(amount),
            Self::Year => span.try_years(amount),
            Self::Hour => span.try_hours(amount),
            Self::Minute => span.try_minutes(amount),
        }
        .ok()?;
        when.checked_add(span).ok()
    }
}

/// `when` moved by each offset in turn. An offset that would leave the representable range
/// (not reachable from a sane clock) is skipped, so rendering cannot fail.
fn shifted(when: &Zoned, offsets: &[Offset]) -> Zoned {
    offsets.iter().fold(when.clone(), |at, offset| {
        offset.unit.apply(&at, offset.amount).unwrap_or(at)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DtPart {
    Lit(String),
    Spec(Spec, Pad),
}

/// The padding a `%-d` or `%_d` flag asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pad {
    /// What the specifier does without a flag.
    Default,
    /// `-`: none.
    None,
    /// `_`: spaces.
    Space,
    /// Zeros: what most specifiers do without a flag.
    Zero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spec {
    Year4,
    Year2,
    IsoYear,
    Month,
    MonthName,
    MonthAbbr,
    Day,
    DayOfYear,
    Weekday,
    WeekdayAbbr,
    WeekdayNumber,
    IsoWeek,
    Quarter,
    Hour24,
    Hour12,
    Minute,
    Second,
    AmPm,
    Offset,
    ZoneName,
    IsoDate,
    IsoTime,
    ShortTime,
}

/// The specifiers of a date format, as the error message lists them.
pub const FORMAT_SPECIFIERS: &str = "%Y %y %G %m %B %b %h %d %e %j %A %a %u %V %q %H %I %M %S %p %z %Z %F %T %R %% \
     (and %-d or %_d for no or space padding)";

/// A question to put to the user before rendering: one per distinct name, from
/// [`Template::prompts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    /// The label, and the key in [`RenderContext::answers`]: the name as first written.
    pub name: String,
    /// What kind of answer is wanted.
    pub kind: PromptKind,
}

/// The kind of answer a [`Prompt`] wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    /// Free text (`{field:Name}`), pre-filled with `default` if the template gave one. A
    /// default of `Some("")` (`{field:Name|}`) makes the field optional and empty.
    Field {
        /// The text to pre-fill, from the first use that has one.
        default: Option<String>,
    },
    /// One of a fixed set (`{choice:Name|a|*b|c}`); the starred option, else the first, is the
    /// start option.
    Choice {
        /// The options, in the order written. Never empty. One may be empty ("none").
        options: Vec<String>,
        /// The index of the start option.
        default: usize,
    },
    /// Free text that may span lines (`{multiline:Name|default}`).
    Multiline {
        /// The text to pre-fill, from the first use that has one.
        default: Option<String>,
    },
    /// A number (`{number:Name|default}`), typed with `.` or `,` as the decimal mark.
    Number {
        /// The text to pre-fill, from the first use that has one.
        default: Option<String>,
    },
    /// A day typed in the locale's order or as `YYYY-MM-DD` (`{pickdate:Name|FORMAT}`);
    /// unanswered it is today. Each use has its own FORMAT (the date specifiers, else the
    /// locale's date style), so the prompt itself carries nothing.
    Date,
    /// A tick box (`{optional:Name}...{/optional}`): answered [`TOGGLE_ON`] it is ticked.
    Toggle,
}

/// The result of [`Template::render`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The resolved text, without the key presses.
    pub text: String,
    /// Where the caret should end up, if the template had a `{cursor}`: how many extended
    /// grapheme clusters of [`Rendered::text`] come after it, i.e. how many Left-arrow presses
    /// move the caret there after pasting. Editors move by cluster, so `e` plus a combining
    /// accent, a flag, an emoji family and `\r\n` each count once. A `{cursor}` must come
    /// after the last `{key:…}`, so this counts within the text after the last key press.
    pub cursor_from_end: Option<usize>,
    /// The `{key:tab}` and `{key:enter}` presses, in order, each after the first
    /// [`KeyStop::at`] bytes of [`Rendered::text`]. Paste the text in these pieces and press
    /// the keys between them.
    pub keys: Vec<KeyStop>,
}

/// What a [`SnippetSource`] knows about a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnippetLookup {
    /// The text of the one item with that title or trigger.
    Found(String),
    /// No item has it.
    NotFound,
    /// More than one item has it.
    Ambiguous,
    /// The item has no text a template may use (an image, a file, a Sensitive item).
    Unusable,
    /// This caller has no library to look in.
    Unavailable,
}

/// Where `{snippet:Title}` finds the text of other items. The app implements it over the
/// store, so this crate does no I/O. The name is as written; match it without regard to
/// case, and by title first, then by trigger.
pub trait SnippetSource {
    /// The text under `name`.
    fn snippet(&self, name: &str) -> SnippetLookup;
}

/// A [`SnippetSource`] with no library: every `{snippet:…}` is refused.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSnippets;

impl SnippetSource for NoSnippets {
    fn snippet(&self, _name: &str) -> SnippetLookup {
        SnippetLookup::Unavailable
    }
}

/// Everything [`Template::render`] needs from the outside world.
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    /// The moment `{date}`, `{time}` and `{datetime:…}` show.
    pub now: Timestamp,
    /// The time zone to show it in. The app passes the user's; the default is UTC.
    pub tz: TimeZone,
    /// How dates, times and numbers are written. Defaults to ISO, 24-hour, English.
    pub locale: Locale,
    /// The clipboard text for `{clipboard}`; `None` renders as nothing.
    pub clipboard: Option<&'a str>,
    /// The copies before the clipboard, newest first, for `{clipboard:2}` (element 0) and on.
    pub clipboard_history: &'a [String],
    /// The text selected in the target app, for `{selection}`; `None` renders as nothing.
    pub selection: Option<&'a str>,
    /// Answers to the template's [`Prompt`]s, by name.
    pub answers: &'a BTreeMap<String, String>,
    /// Random bytes for `{uuid}`, one entry per occurrence in order (see
    /// [`Template::uuid_count`]); a missing entry renders as all-zero bytes. The caller draws
    /// them, so rendering stays a pure function.
    pub uuids: &'a [[u8; 16]],
}

impl<'a> RenderContext<'a> {
    /// A context for `now` in UTC with ISO formats, no clipboard, and these answers.
    pub fn new(now: Timestamp, answers: &'a BTreeMap<String, String>) -> Self {
        Self {
            now,
            tz: TimeZone::UTC,
            locale: Locale::default(),
            clipboard: None,
            clipboard_history: &[],
            selection: None,
            answers,
            uuids: &[],
        }
    }

    /// Supplies the random bytes for `{uuid}`, one `[u8; 16]` per occurrence.
    #[must_use]
    pub fn with_uuids(mut self, uuids: &'a [[u8; 16]]) -> Self {
        self.uuids = uuids;
        self
    }

    /// Shows times in `tz`.
    #[must_use]
    pub fn with_tz(mut self, tz: TimeZone) -> Self {
        self.tz = tz;
        self
    }

    /// Writes dates, times and numbers by `locale`.
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

    /// Supplies the earlier copies for `{clipboard:2}` and on, newest first.
    #[must_use]
    pub fn with_clipboard_history(mut self, history: &'a [String]) -> Self {
        self.clipboard_history = history;
        self
    }

    /// Supplies the text for `{selection}`.
    #[must_use]
    pub fn with_selection(mut self, selection: &'a str) -> Self {
        self.selection = Some(selection);
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

/// Every placeholder as it is written, for the "unknown placeholder" message.
pub const PLACEHOLDER_LIST: &str = "{uuid}, {date}, {time}, {datetime:FORMAT}, {clipboard}, \
     {clipboard:2}, {selection}, {cursor}, {key:tab}, {key:enter}, {field:Name}, \
     {field:Name|default}, {choice:Name|a|b}, {multiline:Name|default}, {number:Name}, \
     {pickdate:Name|FORMAT}, {optional:Name}...{/optional}, {value:Name|upper}, \
     {calc:Qty*Price|2} and {snippet:Title}";

/// What to write instead of an offset unit Snipdeck does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitHint {
    /// `m`: months or minutes?
    MonthOrMinute,
    /// `day`, `days`.
    Days,
    /// `week`, `weeks`, `wk`.
    Weeks,
    /// `month`, `months`.
    Months,
    /// `year`, `years`, `yr`, `yrs`.
    Years,
    /// `hour`, `hours`, `hr`, `hrs`.
    Hours,
    /// `minute`, `minutes`, `mins`.
    Minutes,
    /// Anything else: the list of units.
    AnyUnit,
}

impl UnitHint {
    /// The hint in English, with the units in backticks.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::MonthOrMinute => "`mo` for months or `min` for minutes",
            Self::Days => "`d` for days",
            Self::Weeks => "`w` for weeks",
            Self::Months => "`mo` for months",
            Self::Years => "`y` for years",
            Self::Hours => "`h` for hours",
            Self::Minutes => "`min` for minutes",
            Self::AnyUnit => "one of d, w, mo, y, h, min",
        }
    }
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
    /// A `{optional:…}` that never meets its `{/optional}`.
    UnclosedSection,
    /// An `{optional:…}` inside another one; sections are flat.
    NestedSection,
    /// A `{/optional}` with no `{optional:…}` before it.
    UnmatchedSectionEnd,
    /// A `field`, `choice`, `multiline`, `number`, `pickdate`, `optional` or `value` without a name.
    EmptyName,
    /// A `choice` with no options.
    NoOptions,
    /// A `choice` option that is empty, other than a trailing one that stands for "none":
    /// kept for an empty option that appears twice.
    EmptyOption,
    /// A `choice` with two starred options.
    TwoStartOptions,
    /// A `choice` option that appears twice.
    DuplicateOption {
        /// The repeated option.
        option: String,
    },
    /// A `field` with more than a name and a default (`{field:Name|default|extra}`).
    TooManyParts,
    /// A second `{cursor}`; sections are flat, so one inside a section plus one outside counts.
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
    /// `{uuid:…}`, `{selection:…}` or `{cursor:…}`.
    UnexpectedArgument {
        /// The placeholder that takes none.
        placeholder: &'static str,
    },
    /// `{datetime}`, `{field}`, `{choice}`, `{multiline}`, `{pickdate}` or `{optional}` without the part after the colon.
    MissingArgument {
        /// The placeholder that needs one.
        placeholder: &'static str,
    },
    /// One name used for two different prompts (a field and a choice, or different options).
    ConflictingPrompt {
        /// The name.
        name: String,
    },
    /// More than [`MAX_PROMPTS`] distinct prompts.
    TooManyPrompts {
        /// The limit, [`MAX_PROMPTS`].
        max: usize,
    },
    /// An offset that is not a sign, a number and a unit (`{date+3}`, `{date+d}`).
    BadOffset,
    /// An offset unit that is not `d w mo y h min`.
    UnknownOffsetUnit {
        /// The unit as written.
        found: String,
        /// What to write instead.
        hint: UnitHint,
    },
    /// An offset over its unit's bound.
    OffsetOutOfRange {
        /// The unit.
        unit: &'static str,
        /// The largest amount allowed.
        max: u64,
    },
    /// More than [`MAX_OFFSETS`] offsets in one placeholder.
    TooManyOffsets {
        /// The limit, [`MAX_OFFSETS`].
        max: usize,
    },
    /// A `%x` in a date format that is not in the supported set.
    UnknownFormatSpecifier {
        /// The character after the `%`.
        specifier: char,
    },
    /// `{key:…}` with a key Snipdeck cannot press.
    UnknownKey {
        /// The key as written.
        found: String,
    },
    /// A `{key:…}` after the `{cursor}`: the caret could not land there.
    CursorBeforeKey,
    /// A `{clipboard:N}` with N that is not a whole number from 1 to [`MAX_CLIPBOARD_POSITION`].
    BadClipboardPosition {
        /// The furthest back position, [`MAX_CLIPBOARD_POSITION`].
        max: usize,
    },
    /// A transform name `{clipboard|name}` does not know.
    UnknownTransform {
        /// The name as written.
        found: String,
    },
    /// A `{calc:…}` whose expression is not a sum of numbers and number names, or whose
    /// decimals are not a whole number from 0 to 10.
    BadCalc,
    /// A name in a `{calc:…}` that is not a `{number:Name}` of the template.
    CalcUnknownName {
        /// The name as written.
        name: String,
    },
    /// A `{value:Name}` for a name no prompt of the template has.
    ValueUnknownName {
        /// The name as written.
        name: String,
    },
    /// A `{snippet:…}` that cannot be put in place.
    Snippet {
        /// The title as written.
        name: String,
        /// Why not.
        problem: SnippetProblem,
    },
    /// A problem in the text of the snippet `name`, which a `{snippet:name}` brought in.
    InSnippet {
        /// The snippet's title as written.
        name: String,
        /// What is wrong in its text.
        error: Box<TemplateErrorKind>,
    },
}

/// Why a `{snippet:…}` was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetProblem {
    /// No item has that title or trigger.
    NotFound,
    /// More than one item has it.
    Ambiguous,
    /// The item has no text a template may use (an image, a file, a Sensitive item).
    Unusable,
    /// The snippet contains itself, directly or through others.
    Cycle,
    /// More than [`MAX_SNIPPET_DEPTH`] levels, or more than [`MAX_SNIPPETS`] uses.
    TooMuch,
    /// This place has no library to look in.
    Unavailable,
}

/// How a placeholder is written (`{date}`, `{field:Name}`), or "" for an unknown keyword.
#[must_use]
pub fn usage(keyword: &str) -> &'static str {
    match keyword {
        "uuid" => "{uuid}",
        "date" => "{date}",
        "time" => "{time}",
        "datetime" => "{datetime:FORMAT}",
        "clipboard" => "{clipboard:2}",
        "cursor" => "{cursor}",
        "selection" => "{selection}",
        "key" => "{key:tab}",
        "number" => "{number:Name}",
        "value" => "{value:Name|upper}",
        "calc" => "{calc:Qty*Price|2}",
        "snippet" => "{snippet:Title}",
        "field" => "{field:Name}",
        "choice" => "{choice:Name|a|b}",
        "multiline" => "{multiline:Name|default}",
        "pickdate" => "{pickdate:Name|FORMAT}",
        "optional" => "{optional:Name}...{/optional}",
        _ => "",
    }
}

impl fmt::Display for TemplateErrorKind {
    #[allow(clippy::too_many_lines, reason = "one message per kind of error")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmatchedClose => f.write_str("`}` closes nothing; write `}}` for a literal `}`"),
            Self::Unterminated => f.write_str("`{` is never closed; write `{{` for a literal `{`"),
            Self::EmptyPlaceholder => f.write_str(
                "empty placeholder `{}`; to write a literal brace, write `{{` (and `}}` to close it)",
            ),
            Self::EmptyName => f.write_str("the prompt needs a name, e.g. `{field:Name}`"),
            Self::UnclosedSection => {
                f.write_str("`{optional:…}` is never closed; add `{/optional}` after the part")
            }
            Self::NestedSection => f.write_str("optional sections cannot be nested"),
            Self::UnmatchedSectionEnd => f.write_str("`{/optional}` closes nothing"),
            Self::NoOptions => f.write_str("a choice needs options, e.g. `{choice:Name|a|b}`"),
            Self::EmptyOption => {
                f.write_str("a choice needs an option with text, and at most one empty option")
            }
            Self::TwoStartOptions => f.write_str("only one choice option can start with `*`"),
            Self::DuplicateOption { option } => {
                write!(f, "the choice option `{option}` is listed twice")
            }
            Self::TooManyParts => f.write_str(
                "a prompt takes a name and one more part: `{field:Name|default}`; \
                 write `\\|` for a literal bar",
            ),
            Self::DuplicateCursor => f.write_str("only one `{cursor}` is allowed"),
            Self::TrailingPercent => {
                f.write_str("a `%` ends the format; write `%%` for a literal `%`")
            }
            Self::Unknown { found, suggestion } => {
                write!(f, "unknown placeholder `{{{found}}}`")?;
                if let Some(keyword) = suggestion {
                    write!(f, ", did you mean `{}`?", usage(keyword))?;
                }
                write!(
                    f,
                    ". Placeholders are {PLACEHOLDER_LIST}. To write a literal brace, double it: {{{{ or }}}}"
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
            Self::TooManyPrompts { max } => write!(
                f,
                "a template can ask for at most {max} different fields, choices, numbers, dates or sections"
            ),
            Self::BadOffset => {
                f.write_str("an offset is a sign, a number and a unit, like `+3d` or `-90min`")
            }
            Self::UnknownOffsetUnit { found, hint } => {
                write!(f, "unknown offset unit `{found}`; use {}", hint.text())
            }
            Self::OffsetOutOfRange { unit, max } => {
                write!(f, "an offset in `{unit}` can be at most {max}")
            }
            Self::TooManyOffsets { max } => {
                write!(f, "a placeholder can have at most {max} offsets")
            }
            Self::UnknownFormatSpecifier { specifier } => write!(
                f,
                "unknown date format `%{specifier}`; supported: {FORMAT_SPECIFIERS}"
            ),
            Self::UnknownKey { found } => {
                write!(
                    f,
                    "unknown key `{found}`; use `{{key:tab}}` or `{{key:enter}}`"
                )
            }
            Self::CursorBeforeKey => f.write_str("`{cursor}` must come after the last `{key:…}`"),
            Self::BadClipboardPosition { max } => write!(
                f,
                "`{{clipboard:N}}` takes a whole number from 1 to {max}; 1 is the clipboard now"
            ),
            Self::UnknownTransform { found } => write!(
                f,
                "unknown transform `{found}`; use one of {}",
                Transform::keyword_list()
            ),
            Self::BadCalc => f.write_str(
                "`{calc:…}` is a sum of numbers and number names; \
                 the part after the bar is the decimals, 0 to 10",
            ),
            Self::CalcUnknownName { name } => write!(
                f,
                "a calculation uses `{name}`, which is not a `{{number:{name}}}` of this template"
            ),
            Self::ValueUnknownName { name } => write!(
                f,
                "`{{value:{name}}}` repeats an answer, but nothing asks for `{name}`"
            ),
            Self::Snippet { name, problem } => match problem {
                SnippetProblem::NotFound => write!(f, "no snippet is titled `{name}`"),
                SnippetProblem::Ambiguous => {
                    write!(f, "more than one snippet is titled `{name}`; rename one")
                }
                SnippetProblem::Unusable => write!(
                    f,
                    "`{name}` has no text a template can use (a picture, files or a Sensitive item)"
                ),
                SnippetProblem::Cycle => write!(f, "`{name}` contains itself"),
                SnippetProblem::TooMuch => write!(
                    f,
                    "snippets can be nested {MAX_SNIPPET_DEPTH} deep and used {MAX_SNIPPETS} times in one template"
                ),
                SnippetProblem::Unavailable => {
                    write!(f, "`{name}` cannot be looked up here")
                }
            },
            Self::InSnippet { name, error } => write!(f, "in the snippet `{name}`: {error}"),
        }
    }
}

/// Why a template could not be rendered. Both cases leave the template and context as they
/// were; show the user a message and let them try again.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// The result would be longer than [`MAX_RENDERED_BYTES`].
    #[error("the result would be over {limit} bytes; use less clipboard text or shorter answers")]
    OutputTooLarge {
        /// The limit, [`MAX_RENDERED_BYTES`].
        limit: usize,
    },
}

/// `text` with every brace doubled, so it parses back to exactly `text`.
pub fn escape(text: &str) -> String {
    text.replace('{', "{{").replace('}', "}}")
}

/// `text` as a name, default or option inside a placeholder: a backslash before each `\`,
/// `|`, `{` and `}` (and a leading `*`), so the placeholder parses back to exactly `text`.
pub fn escape_arg(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        if matches!(c, '\\' | '|' | '{' | '}') || (i == 0 && c == '*') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn err<T>(span: Range<usize>, kind: TemplateErrorKind) -> Result<T, TemplateError> {
    Err(TemplateError { span, kind })
}

struct Parser<'s> {
    parts: Vec<Part>,
    prompts: Vec<Prompt>,
    prompt_index: HashMap<String, usize>,
    has_cursor: bool,
    /// The open `{optional:…}`, if any; `parts` then holds its inside.
    section: Option<OpenSection>,
    source: &'s dyn SnippetSource,
    /// The keys of the snippets being expanded, outermost first.
    stack: Vec<String>,
    /// `{snippet:…}` expansions so far.
    expansions: usize,
    /// How many snippets deep the text being read is.
    depth: usize,
}

/// An `{optional:…}` waiting for its `{/optional}`.
struct OpenSection {
    prompt: usize,
    /// The parts before it, set aside while the inside is collected.
    outer: Vec<Part>,
    span: Range<usize>,
    /// The snippet depth it was opened at: only text at that depth may close it.
    depth: usize,
}

/// How a template compares two prompt names: trimmed, without regard to case.
fn prompt_key(name: &str) -> String {
    name.trim().to_uppercase().to_lowercase()
}

/// The index of the first `|` in `text` that no backslash escapes.
fn find_unescaped_pipe(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if bytes.get(i + 1).is_some_and(|b| b"\\|{}*".contains(b)) => i += 2,
            b'|' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// `text` cut at every `|` that no backslash escapes, still escaped.
fn split_unescaped(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = find_unescaped_pipe(rest) {
        out.push(&rest[..at]);
        rest = &rest[at + 1..];
    }
    out.push(rest);
    out
}

/// `text` with each `\\`, `\|`, `\{`, `\}` and `\*` replaced by the character after the
/// backslash; any other backslash stays.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && chars
                .peek()
                .is_some_and(|n| matches!(n, '\\' | '|' | '{' | '}' | '*'))
        {
            out.extend(chars.next());
        } else {
            out.push(c);
        }
    }
    out
}

impl Template {
    /// Parses `src`, with no library to look `{snippet:…}` up in (each one is an error).
    ///
    /// # Errors
    ///
    /// The first problem found, as a [`TemplateError`] with its byte span: unbalanced
    /// braces, an unknown placeholder, a missing or unexpected argument, a bad date format,
    /// a second `{cursor}`, or a name used for two different prompts.
    pub fn parse(src: &str) -> Result<Self, TemplateError> {
        Self::parse_with(src, &NoSnippets)
    }

    /// Parses `src`, putting each `{snippet:Title}` in place from `source`.
    ///
    /// # Errors
    ///
    /// As [`Template::parse`], plus a snippet that is missing, ambiguous, unusable, nested
    /// too deep or inside itself, and a problem in a snippet's own text.
    pub fn parse_with(src: &str, source: &dyn SnippetSource) -> Result<Self, TemplateError> {
        let mut parser = Parser {
            parts: Vec::new(),
            prompts: Vec::new(),
            prompt_index: HashMap::new(),
            has_cursor: false,
            section: None,
            source,
            stack: Vec::new(),
            expansions: 0,
            depth: 0,
        };
        parser.scan(src)?;
        if let Some(open) = parser.section.take() {
            return err(open.span, TemplateErrorKind::UnclosedSection);
        }
        parser.resolve()?;
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

    /// For each prompt of [`Template::prompts`], whether the text the answers produce would
    /// show it: false for a prompt used only inside an `{optional}` section that `answers`
    /// leaves unticked. Such a prompt needs no answer.
    pub fn active_prompts(&self, answers: &BTreeMap<String, String>) -> Vec<bool> {
        let mut active = vec![false; self.prompts.len()];
        self.mark_active(&self.parts, answers, &mut active);
        active
    }

    fn mark_active(&self, parts: &[Part], answers: &BTreeMap<String, String>, active: &mut [bool]) {
        for part in parts {
            match part {
                Part::Prompt(i) | Part::PickDate(i, _) | Part::Value(ValueRef::Index(i), _) => {
                    if let Some(slot) = active.get_mut(*i) {
                        *slot = true;
                    }
                }
                Part::Calc(expr, ..) => {
                    let mut vars = Vec::new();
                    expr.vars(&mut vars);
                    for i in vars {
                        if let Some(slot) = active.get_mut(i) {
                            *slot = true;
                        }
                    }
                }
                Part::Section(i, inner) => {
                    if let Some(slot) = active.get_mut(*i) {
                        *slot = true;
                    }
                    if self.is_ticked(*i, answers) {
                        self.mark_active(inner, answers, active);
                    }
                }
                _ => {}
            }
        }
    }

    fn is_ticked(&self, index: usize, answers: &BTreeMap<String, String>) -> bool {
        self.prompts
            .get(index)
            .is_some_and(|prompt| answers.get(&prompt.name).map(String::as_str) == Some(TOGGLE_ON))
    }

    fn any_part(&self, test: &dyn Fn(&Part) -> bool) -> bool {
        fn any(parts: &[Part], test: &dyn Fn(&Part) -> bool) -> bool {
            parts.iter().any(|p| match p {
                Part::Section(_, inner) => test(p) || any(inner, test),
                _ => test(p),
            })
        }
        any(&self.parts, test)
    }

    /// Whether the template says `{clipboard}`: only then does a use need to read the clipboard.
    pub fn uses_clipboard(&self) -> bool {
        self.max_clipboard_position() > 0
    }

    /// The furthest back clipboard entry the template reads (1 is the clipboard now), or 0
    /// if it reads none.
    pub fn max_clipboard_position(&self) -> usize {
        fn deepest(parts: &[Part]) -> usize {
            parts
                .iter()
                .map(|p| match p {
                    Part::Clipboard(n, _) => *n,
                    Part::Section(_, inner) => deepest(inner),
                    _ => 0,
                })
                .max()
                .unwrap_or(0)
        }
        deepest(&self.parts)
    }

    /// Whether the template says `{selection}`: only then does a use need to read what is
    /// selected in the target app.
    pub fn uses_selection(&self) -> bool {
        self.any_part(&|p| matches!(p, Part::Selection(_)))
    }

    /// Whether the template presses keys (`{key:tab}`, `{key:enter}`).
    pub fn has_keys(&self) -> bool {
        self.any_part(&|p| matches!(p, Part::Key(_)))
    }

    /// How many `{uuid}` the template has: the number of `[u8; 16]` to pass in
    /// [`RenderContext::uuids`].
    pub fn uuid_count(&self) -> usize {
        uuids_in(&self.parts)
    }

    /// Resolves the template, cutting the output at [`MAX_RENDERED_BYTES`]; prefer
    /// [`Template::try_render`].
    ///
    /// Missing pieces degrade rather than fail: no clipboard renders as nothing, an
    /// unanswered field as its default (or nothing), an unanswered choice as its start
    /// option, an unanswered `{pickdate}` as today, an unticked `{optional}` section as nothing.
    /// An answer is used as given, even if empty or not among a choice's options.
    ///
    /// If the output would exceed [`MAX_RENDERED_BYTES`] it is cut at that length (on a
    /// character boundary) rather than grown without bound; [`Template::try_render`] reports
    /// that instead.
    pub fn render(&self, ctx: &RenderContext<'_>) -> Rendered {
        self.render_bounded(ctx).0
    }

    /// Like [`Template::render`], but fails instead of cutting the output.
    ///
    /// # Errors
    ///
    /// [`RenderError::OutputTooLarge`] if the result would be over [`MAX_RENDERED_BYTES`].
    pub fn try_render(&self, ctx: &RenderContext<'_>) -> Result<Rendered, RenderError> {
        match self.render_bounded(ctx) {
            (rendered, false) => Ok(rendered),
            (_, true) => Err(RenderError::OutputTooLarge {
                limit: MAX_RENDERED_BYTES,
            }),
        }
    }

    fn render_parts(
        &self,
        parts: &[Part],
        ctx: &RenderContext<'_>,
        when: &Zoned,
        state: &mut RenderState<'_>,
    ) {
        for part in parts {
            let out = &mut state.out;
            match part {
                Part::Text(text) => out.push_str(text),
                Part::Moment(kind, offsets, format) => {
                    let at = shifted(when, offsets);
                    match (format, kind) {
                        (Some(parts), _) => write_parts(out, &at, parts, ctx.locale),
                        (None, Moment::Date) => out.push_str(&date_string(&at, ctx.locale.date)),
                        (None, Moment::Time) => out.push_str(&time_string(&at, ctx.locale)),
                        (None, Moment::DateTime) => {
                            out.push_str(&date_string(&at, ctx.locale.date));
                            out.push_str(" ");
                            out.push_str(&time_string(&at, ctx.locale));
                        }
                    }
                }
                Part::Uuid => {
                    let bytes = state.uuids.next().copied().unwrap_or_default();
                    out.push_str(
                        &uuid::Builder::from_random_bytes(bytes)
                            .into_uuid()
                            .to_string(),
                    );
                }
                Part::Clipboard(position, shape) => {
                    let text = match position {
                        1 => ctx.clipboard,
                        n => ctx.clipboard_history.get(n - 2).map(String::as_str),
                    };
                    out.push_str(&transform::apply(*shape, text.unwrap_or_default()));
                }
                Part::Selection(shape) => {
                    out.push_str(&transform::apply(*shape, ctx.selection.unwrap_or_default()));
                }
                Part::Cursor => state.cursor = Some(out.text.len()),
                Part::Key(key) => state.keys.push(KeyStop {
                    at: out.text.len(),
                    key: *key,
                }),
                Part::Prompt(index) => {
                    if let Some(prompt) = self.prompts.get(*index) {
                        out.push_str(&answer_text(prompt, ctx));
                    }
                }
                Part::Value(ValueRef::Index(index), shape) => {
                    if let Some(prompt) = self.prompts.get(*index) {
                        out.push_str(&transform::apply(*shape, &answer_text(prompt, ctx)));
                    }
                }
                Part::Value(ValueRef::Name(..), _) => {}
                Part::Calc(expr, decimals, _) => {
                    let number = |i: usize| {
                        self.prompts
                            .get(i)
                            .and_then(|p| parse_number(answer(p, ctx.answers)))
                            .unwrap_or(0.0)
                    };
                    match expr.eval(&number) {
                        Some(value) => {
                            out.push_str(&format_number(value, *decimals, ctx.locale.decimal));
                        }
                        None => out.push_str(DIV_ZERO),
                    }
                }
                Part::PickDate(index, format) => {
                    if let Some(prompt) = self.prompts.get(*index) {
                        render_pick_date(out, prompt, format.as_deref(), ctx, when);
                    }
                }
                Part::Section(index, inner) => {
                    if self.is_ticked(*index, ctx.answers) {
                        self.render_parts(inner, ctx, when, state);
                    } else {
                        // Keep the `{uuid}` draws aligned with their occurrences.
                        for _ in 0..uuids_in(inner) {
                            state.uuids.next();
                        }
                    }
                }
            }
            if state.out.cut {
                break;
            }
        }
    }

    /// Renders, stopping at [`MAX_RENDERED_BYTES`]; the flag says whether output was cut.
    fn render_bounded(&self, ctx: &RenderContext<'_>) -> (Rendered, bool) {
        let when = ctx.now.to_zoned(ctx.tz.clone());
        let mut state = RenderState {
            out: BoundedString::default(),
            cursor: None,
            keys: Vec::new(),
            uuids: ctx.uuids.iter(),
        };
        self.render_parts(&self.parts, ctx, &when, &mut state);
        let RenderState {
            out, cursor, keys, ..
        } = state;
        let cursor_from_end = cursor.map(|at| graphemes_after(&out.text, at));
        let cut = out.cut;
        (
            Rendered {
                text: out.text,
                cursor_from_end,
                keys,
            },
            cut,
        )
    }
}

/// `{uuid}`s in `parts`, including those inside sections (ticked or not).
fn uuids_in(parts: &[Part]) -> usize {
    parts
        .iter()
        .map(|p| match p {
            Part::Uuid => 1,
            Part::Section(_, inner) => uuids_in(inner),
            _ => 0,
        })
        .sum()
}

struct RenderState<'a> {
    out: BoundedString,
    cursor: Option<usize>,
    keys: Vec<KeyStop>,
    uuids: core::slice::Iter<'a, [u8; 16]>,
}

fn write_parts(out: &mut BoundedString, when: &Zoned, parts: &[DtPart], locale: Locale) {
    for part in parts {
        match part {
            DtPart::Lit(text) => out.push_str(text),
            DtPart::Spec(spec, pad) => {
                let mut piece = String::new();
                write_spec(&mut piece, when, *spec, *pad, locale);
                out.push_str(&piece);
            }
        }
    }
}

/// Parses a typed date (surrounding spaces ignored): ISO `YYYY-MM-DD`, or the numbers in the
/// order of `style`. The check for a `pickdate` answer.
pub fn parse_pick_date(text: &str, style: DateStyle) -> Option<jiff::civil::Date> {
    let date = style.parse(text)?;
    // Only a day that can be shown: the last day of year 9999 is past the latest instant.
    date.to_zoned(TimeZone::UTC).ok().map(|_| date)
}

/// The text a `pickdate` shows before the user types: today in `ctx.tz` in the locale's date
/// style.
pub fn today_text(ctx: &RenderContext<'_>) -> String {
    date_string(&ctx.now.to_zoned(ctx.tz.clone()), ctx.locale.date)
}

/// Writes a `pickdate` answer: today if unanswered or blank, nothing if it is not a real date.
fn render_pick_date(
    out: &mut BoundedString,
    prompt: &Prompt,
    format: Option<&[DtPart]>,
    ctx: &RenderContext<'_>,
    today: &Zoned,
) {
    let given = ctx.answers.get(&prompt.name).map_or("", |a| a.trim());
    let date = if given.is_empty() {
        today.date()
    } else if let Some(date) = parse_pick_date(given, ctx.locale.date) {
        date
    } else {
        return;
    };
    // The start of that day in the user's zone; a day the zone cannot show falls back to UTC.
    let Some(when) = date
        .to_zoned(ctx.tz.clone())
        .or_else(|_| date.to_zoned(TimeZone::UTC))
        .ok()
    else {
        return;
    };
    match format {
        Some(parts) => write_parts(out, &when, parts, ctx.locale),
        None => out.push_str(&date_string(&when, ctx.locale.date)),
    }
}

/// A string that stops growing at [`MAX_RENDERED_BYTES`].
#[derive(Default)]
struct BoundedString {
    text: String,
    cut: bool,
}

impl BoundedString {
    /// Appends `piece`, or as much of it as fits (never part of a character).
    fn push_str(&mut self, piece: &str) {
        if self.cut {
            return;
        }
        let room = MAX_RENDERED_BYTES - self.text.len();
        if piece.len() <= room {
            self.text.push_str(piece);
            return;
        }
        let mut end = room;
        while !piece.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&piece[..end]);
        self.cut = true;
    }
}

fn answer<'a>(prompt: &'a Prompt, answers: &'a BTreeMap<String, String>) -> &'a str {
    if let Some(given) = answers.get(&prompt.name) {
        return given;
    }
    match &prompt.kind {
        PromptKind::Field { default }
        | PromptKind::Multiline { default }
        | PromptKind::Number { default } => default.as_deref().unwrap_or_default(),
        PromptKind::Choice { options, default } => options.get(*default).map_or("", String::as_str),
        PromptKind::Date | PromptKind::Toggle => "",
    }
}

/// What a prompt shows where it is used: its answer, and for a number the typed digits with
/// the locale's decimal mark.
fn answer_text(prompt: &Prompt, ctx: &RenderContext<'_>) -> String {
    let given = answer(prompt, ctx.answers);
    if matches!(prompt.kind, PromptKind::Number { .. }) && parse_number(given).is_some() {
        let mark = ctx.locale.decimal;
        return given.trim().replace(['.', ','], &mark.to_string());
    }
    given.to_owned()
}

/// Extended grapheme clusters after byte offset `at`: the Left-arrow presses back to it.
fn graphemes_after(text: &str, at: usize) -> usize {
    text.get(at..).unwrap_or_default().graphemes(true).count()
}

/// What to put where `{value:Name}` or `{calc:…}` names a prompt, until the whole template
/// is known.
#[derive(Debug, Clone, PartialEq)]
enum ValueRef {
    Name(String, Range<usize>),
    Index(usize),
}

impl Parser<'_> {
    /// Reads `src`, which is the template or the text of a snippet at `self.depth`.
    fn scan(&mut self, src: &str) -> Result<(), TemplateError> {
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
                let mut j = body_start;
                let close = loop {
                    match bytes.get(j) {
                        None => return err(i..bytes.len(), TemplateErrorKind::Unterminated),
                        Some(b'\\') if bytes.get(j + 1).is_some_and(|b| b"\\|{}*".contains(b)) => {
                            j += 2;
                        }
                        Some(b'{') => return err(i..j, TemplateErrorKind::Unterminated),
                        Some(b'}') => break j,
                        Some(_) => j += 1,
                    }
                };
                if !text.is_empty() {
                    self.parts.push(Part::Text(core::mem::take(&mut text)));
                }
                self.placeholder(&src[body_start..close], i..close + 1)?;
                i = close + 1;
            }
        }
        if !text.is_empty() {
            self.parts.push(Part::Text(text));
        }
        Ok(())
    }

    /// Replaces the names `{value:…}` and `{calc:…}` wrote with prompt indexes, now that every
    /// prompt is known.
    fn resolve(&mut self) -> Result<(), TemplateError> {
        let mut parts = core::mem::take(&mut self.parts);
        let result = self.resolve_in(&mut parts);
        self.parts = parts;
        result
    }

    fn resolve_in(&self, parts: &mut [Part]) -> Result<(), TemplateError> {
        for part in parts {
            match part {
                Part::Value(reference, _) => {
                    if let ValueRef::Name(name, span) = reference {
                        let Some(&index) = self.prompt_index.get(&prompt_key(name)) else {
                            return err(
                                span.clone(),
                                TemplateErrorKind::ValueUnknownName { name: name.clone() },
                            );
                        };
                        *reference = ValueRef::Index(index);
                    }
                }
                Part::Calc(expr, _, span) => {
                    let lookup = |name: &str| {
                        self.prompt_index
                            .get(&prompt_key(name))
                            .copied()
                            .filter(|&i| {
                                matches!(
                                    self.prompts.get(i).map(|p| &p.kind),
                                    Some(PromptKind::Number { .. })
                                )
                            })
                    };
                    if let Err(name) = expr.resolve(&lookup) {
                        return err(span.clone(), TemplateErrorKind::CalcUnknownName { name });
                    }
                }
                Part::Section(_, inner) => self.resolve_in(inner)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Handles the text between a pair of braces; `span` covers the braces too.
    #[allow(clippy::too_many_lines, reason = "one arm per placeholder")]
    fn placeholder(&mut self, body: &str, span: Range<usize>) -> Result<(), TemplateError> {
        if body.trim().is_empty() {
            return err(span, TemplateErrorKind::EmptyPlaceholder);
        }
        if body.trim() == "/optional" {
            let Some(open) = self.section.take_if(|open| open.depth == self.depth) else {
                return err(span, TemplateErrorKind::UnmatchedSectionEnd);
            };
            let inner = core::mem::replace(&mut self.parts, open.outer);
            self.parts.push(Part::Section(open.prompt, inner));
            return Ok(());
        }
        let (raw_key, arg) = match body.split_once(':') {
            Some((key, arg)) => (key, Some(arg)),
            None => (body, None),
        };
        // `{clipboard|upper}`: the transform follows the keyword.
        let head = raw_key.split('|').next().unwrap_or_default().trim();
        let (raw_key, key_shape) = match raw_key.split_once('|') {
            Some((key, shape)) if matches!(head, "clipboard" | "selection") => (key, Some(shape)),
            _ => (raw_key, None),
        };
        let (key, offsets) = split_offsets(raw_key, span.start + 1)?;
        let shape_of = |word: Option<&str>| match word {
            None => Ok(Transform::Plain),
            Some(word) => Transform::from_keyword(word).ok_or_else(|| TemplateError {
                span: span.clone(),
                kind: TemplateErrorKind::UnknownTransform {
                    found: word.trim().to_owned(),
                },
            }),
        };
        match key {
            "uuid" | "cursor" | "selection" if arg.is_some() => {
                let placeholder = KEYWORDS
                    .iter()
                    .find(|k| **k == key)
                    .copied()
                    .unwrap_or("uuid");
                err(span, TemplateErrorKind::UnexpectedArgument { placeholder })
            }
            "uuid" => {
                self.parts.push(Part::Uuid);
                Ok(())
            }
            "cursor" => {
                if self.has_cursor {
                    return err(span, TemplateErrorKind::DuplicateCursor);
                }
                self.has_cursor = true;
                self.parts.push(Part::Cursor);
                Ok(())
            }
            "selection" => {
                let shape = shape_of(key_shape)?;
                self.parts.push(Part::Selection(shape));
                Ok(())
            }
            "clipboard" => {
                let (position, shape) = match arg.map(|a| a.split_once('|')) {
                    None => (None, key_shape),
                    Some(None) => (arg, key_shape),
                    Some(Some((position, shape))) => (Some(position), Some(shape)),
                };
                let position = match position {
                    None => 1,
                    Some(text) => match text.trim().parse::<usize>() {
                        Ok(n) if (1..=MAX_CLIPBOARD_POSITION).contains(&n) => n,
                        _ => {
                            return err(
                                span,
                                TemplateErrorKind::BadClipboardPosition {
                                    max: MAX_CLIPBOARD_POSITION,
                                },
                            );
                        }
                    },
                };
                let shape = shape_of(shape)?;
                self.parts.push(Part::Clipboard(position, shape));
                Ok(())
            }
            "key" => {
                let Some(arg) = arg else {
                    return err(
                        span,
                        TemplateErrorKind::MissingArgument { placeholder: "key" },
                    );
                };
                let key = match arg.trim().to_ascii_lowercase().as_str() {
                    "tab" => TemplateKey::Tab,
                    "enter" | "return" => TemplateKey::Enter,
                    other => {
                        return err(
                            span,
                            TemplateErrorKind::UnknownKey {
                                found: other.to_owned(),
                            },
                        );
                    }
                };
                if self.has_cursor {
                    return err(span, TemplateErrorKind::CursorBeforeKey);
                }
                self.parts.push(Part::Key(key));
                Ok(())
            }
            "date" | "time" | "datetime" => {
                let (moment, placeholder) = match key {
                    "date" => (Moment::Date, "date"),
                    "time" => (Moment::Time, "time"),
                    _ => (Moment::DateTime, "datetime"),
                };
                let format = match arg {
                    None => None,
                    Some("") => {
                        return err(span, TemplateErrorKind::MissingArgument { placeholder });
                    }
                    // The format starts after the opening brace, the (padded) key and the colon.
                    Some(format) => Some(parse_format(format, span.start + 1 + raw_key.len() + 1)?),
                };
                self.parts.push(Part::Moment(moment, offsets, format));
                Ok(())
            }
            "snippet" => self.snippet(arg, span),
            "value" => {
                let Some(arg) = arg else {
                    return err(
                        span,
                        TemplateErrorKind::MissingArgument {
                            placeholder: "value",
                        },
                    );
                };
                let pieces = split_unescaped(arg);
                let name = unescape(pieces.first().copied().unwrap_or_default().trim());
                if name.is_empty() {
                    return err(span, TemplateErrorKind::EmptyName);
                }
                if pieces.len() > 2 {
                    return err(span, TemplateErrorKind::TooManyParts);
                }
                let shape = shape_of(pieces.get(1).copied())?;
                self.parts
                    .push(Part::Value(ValueRef::Name(name, span.clone()), shape));
                Ok(())
            }
            "calc" => {
                let Some(arg) = arg else {
                    return err(
                        span,
                        TemplateErrorKind::MissingArgument {
                            placeholder: "calc",
                        },
                    );
                };
                let pieces = split_unescaped(arg);
                let decimals = match pieces.get(1) {
                    None => None,
                    Some(text) => match text.trim().parse::<usize>() {
                        Ok(n) if n <= 10 => Some(n),
                        _ => return err(span, TemplateErrorKind::BadCalc),
                    },
                };
                let expr = (pieces.len() <= 2)
                    .then(|| calc::parse(pieces.first().copied().unwrap_or_default()))
                    .flatten();
                let Some(expr) = expr else {
                    return err(span, TemplateErrorKind::BadCalc);
                };
                self.parts.push(Part::Calc(expr, decimals, span));
                Ok(())
            }
            "field" | "choice" | "multiline" | "number" | "pickdate" | "optional" => {
                let arg_at = span.start + 1 + raw_key.len() + 1;
                self.prompt_placeholder(key, arg, arg_at, span)
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

    /// `{snippet:Title}`: reads the text of that item in place.
    fn snippet(&mut self, arg: Option<&str>, span: Range<usize>) -> Result<(), TemplateError> {
        let Some(arg) = arg else {
            return err(
                span,
                TemplateErrorKind::MissingArgument {
                    placeholder: "snippet",
                },
            );
        };
        let name = unescape(arg.trim());
        if name.is_empty() {
            return err(span, TemplateErrorKind::EmptyName);
        }
        let problem = |problem| {
            err(
                span.clone(),
                TemplateErrorKind::Snippet {
                    name: name.clone(),
                    problem,
                },
            )
        };
        let key = prompt_key(&name);
        if self.stack.contains(&key) {
            return problem(SnippetProblem::Cycle);
        }
        if self.depth >= MAX_SNIPPET_DEPTH || self.expansions >= MAX_SNIPPETS {
            return problem(SnippetProblem::TooMuch);
        }
        let text = match self.source.snippet(&name) {
            SnippetLookup::Found(text) => text,
            SnippetLookup::NotFound => return problem(SnippetProblem::NotFound),
            SnippetLookup::Ambiguous => return problem(SnippetProblem::Ambiguous),
            SnippetLookup::Unusable => return problem(SnippetProblem::Unusable),
            SnippetLookup::Unavailable => return problem(SnippetProblem::Unavailable),
        };
        self.expansions += 1;
        self.stack.push(key);
        self.depth += 1;
        let result = self.scan(&text);
        let left_open = self
            .section
            .as_ref()
            .is_some_and(|open| open.depth == self.depth);
        self.depth -= 1;
        self.stack.pop();
        let wrap = |kind| {
            err(
                span.clone(),
                TemplateErrorKind::InSnippet {
                    name: name.clone(),
                    error: Box::new(kind),
                },
            )
        };
        if let Err(e) = result {
            return wrap(e.kind);
        }
        if left_open {
            return wrap(TemplateErrorKind::UnclosedSection);
        }
        Ok(())
    }

    /// Handles `{field}`, `{choice}`, `{multiline}`, `{number}`, `{pickdate}` and `{optional}`.
    fn prompt_placeholder(
        &mut self,
        key: &str,
        arg: Option<&str>,
        arg_at: usize,
        span: Range<usize>,
    ) -> Result<(), TemplateError> {
        let placeholder = KEYWORDS
            .iter()
            .find(|&&k| k == key)
            .copied()
            .unwrap_or("field");
        let Some(arg) = arg else {
            return err(span, TemplateErrorKind::MissingArgument { placeholder });
        };
        if key == "pickdate" {
            return self.pickdate(arg, arg_at, span);
        }
        let mut pieces = split_unescaped(arg).into_iter();
        let name = unescape(pieces.next().unwrap_or_default().trim());
        if name.is_empty() {
            return err(span, TemplateErrorKind::EmptyName);
        }
        let kind = if key == "choice" {
            let mut options = Vec::new();
            let mut start = None;
            for raw in pieces {
                let raw = raw.trim();
                let (starred, raw) = match raw.strip_prefix('*') {
                    Some(rest) => (true, rest.trim_start()),
                    None => (false, raw),
                };
                if starred {
                    if start.is_some() {
                        return err(span, TemplateErrorKind::TwoStartOptions);
                    }
                    start = Some(options.len());
                }
                options.push(unescape(raw));
            }
            if options.is_empty() {
                return err(span, TemplateErrorKind::NoOptions);
            }
            if options.iter().all(String::is_empty)
                || options.iter().filter(|o| o.is_empty()).count() > 1
            {
                return err(span, TemplateErrorKind::EmptyOption);
            }
            if let Some(option) = options
                .iter()
                .enumerate()
                .find_map(|(i, option)| options[..i].contains(option).then_some(option))
            {
                let option = option.clone();
                return err(span, TemplateErrorKind::DuplicateOption { option });
            }
            PromptKind::Choice {
                options,
                default: start.unwrap_or(0),
            }
        } else {
            // A default, even an empty one: `{field:Name|}` is optional.
            let extra = pieces.next().map(|d| unescape(d.trim()));
            if pieces.next().is_some()
                || (key == "optional" && extra.as_deref().is_some_and(|d| !d.is_empty()))
            {
                return err(span, TemplateErrorKind::TooManyParts);
            }
            match key {
                "field" => PromptKind::Field { default: extra },
                "multiline" => PromptKind::Multiline { default: extra },
                "number" => PromptKind::Number { default: extra },
                _ => PromptKind::Toggle,
            }
        };
        if key == "optional" {
            if self.section.is_some() {
                return err(span, TemplateErrorKind::NestedSection);
            }
            let prompt = self.add_prompt(&name, kind, span.clone())?;
            let outer = core::mem::take(&mut self.parts);
            self.section = Some(OpenSection {
                prompt,
                outer,
                span,
                depth: self.depth,
            });
            return Ok(());
        }
        let index = self.add_prompt(&name, kind, span)?;
        self.parts.push(Part::Prompt(index));
        Ok(())
    }

    /// `{pickdate:Name|FORMAT}`. Like `{datetime:FORMAT}`, the format is everything after the
    /// first `|`, untrimmed, so its error span is exact.
    fn pickdate(
        &mut self,
        arg: &str,
        arg_at: usize,
        span: Range<usize>,
    ) -> Result<(), TemplateError> {
        let (name, format) = match find_unescaped_pipe(arg) {
            Some(at) => (&arg[..at], Some((&arg[at + 1..], arg_at + at + 1))),
            None => (arg, None),
        };
        let name = unescape(name.trim());
        if name.is_empty() {
            return err(span, TemplateErrorKind::EmptyName);
        }
        let format = match format {
            Some((format, at)) if !format.is_empty() => Some(parse_format(format, at)?),
            _ => None,
        };
        let index = self.add_prompt(&name, PromptKind::Date, span)?;
        self.parts.push(Part::PickDate(index, format));
        Ok(())
    }

    /// Registers a prompt use and returns its index, merging with an earlier use of the name
    /// (compared by [`prompt_key`]; the first spelling is the label).
    fn add_prompt(
        &mut self,
        name: &str,
        kind: PromptKind,
        span: Range<usize>,
    ) -> Result<usize, TemplateError> {
        let key = prompt_key(name);
        let Some(&index) = self.prompt_index.get(&key) else {
            let index = self.prompts.len();
            if index >= MAX_PROMPTS {
                return err(span, TemplateErrorKind::TooManyPrompts { max: MAX_PROMPTS });
            }
            self.prompt_index.insert(key, index);
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
            (PromptKind::Field { default }, PromptKind::Field { default: later })
            | (PromptKind::Multiline { default }, PromptKind::Multiline { default: later })
            | (PromptKind::Number { default }, PromptKind::Number { default: later }) => {
                if default.is_none() {
                    *default = later;
                }
            }
            (PromptKind::Choice { options, .. }, PromptKind::Choice { options: later, .. })
                if *options == later => {}
            (PromptKind::Date, PromptKind::Date) | (PromptKind::Toggle, PromptKind::Toggle) => {}
            _ => return err(span, conflict()),
        }
        Ok(index)
    }
}

/// Splits `{date+3d-1w}`'s key into the keyword and its offsets. `raw_key` is the text before
/// the first `:`, starting at byte `at` of the source. A key with no `+` or `-` after a
/// date keyword comes back whole (an unknown name is reported by the caller).
fn split_offsets(raw_key: &str, at: usize) -> Result<(&str, Vec<Offset>), TemplateError> {
    let key = raw_key.trim();
    let Some(sign) = key.find(['+', '-']) else {
        return Ok((key, Vec::new()));
    };
    let head = key[..sign].trim_end();
    if !matches!(head, "date" | "time" | "datetime") {
        return Ok((key, Vec::new()));
    }
    let lead = raw_key.len() - raw_key.trim_start().len();
    Ok((head, parse_offsets(&key[sign..], at + lead + sign)?))
}

/// Parses `+3d-1w`; `at` is where `text` starts in the source.
fn parse_offsets(text: &str, at: usize) -> Result<Vec<Offset>, TemplateError> {
    let bytes = text.as_bytes();
    let mut offsets = Vec::new();
    let mut i = 0;
    loop {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if i >= bytes.len() {
            return Ok(offsets);
        }
        let start = i;
        let bad = |end: usize| {
            err(
                at + start..at + end.max(start + 1),
                TemplateErrorKind::BadOffset,
            )
        };
        let negative = match bytes.get(i) {
            Some(b'+') => false,
            Some(b'-') => true,
            _ => return bad(i + 1),
        };
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let digits = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == digits {
            return bad(i);
        }
        let amount = text[digits..i].parse::<u64>().unwrap_or(u64::MAX);
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let word = i;
        while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
        }
        if i == word {
            return bad(i);
        }
        let span = at + start..at + i;
        let Some(unit) = Unit::of(&text[word..i]) else {
            return err(
                span,
                TemplateErrorKind::UnknownOffsetUnit {
                    found: text[word..i].to_owned(),
                    hint: unit_hint(&text[word..i]),
                },
            );
        };
        if amount > unit.max() {
            return err(
                span,
                TemplateErrorKind::OffsetOutOfRange {
                    unit: unit.word(),
                    max: unit.max(),
                },
            );
        }
        if offsets.len() == MAX_OFFSETS {
            return err(span, TemplateErrorKind::TooManyOffsets { max: MAX_OFFSETS });
        }
        // Bounded above, so it fits an `i64`.
        let amount = i64::try_from(amount).unwrap_or_default();
        offsets.push(Offset {
            amount: if negative { -amount } else { amount },
            unit,
        });
        // The next thing must be whitespace or the next sign.
        if bytes
            .get(i)
            .is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'+' | b'-'))
        {
            return err(at + i..at + i + 1, TemplateErrorKind::BadOffset);
        }
    }
}

/// What to write instead of the unit `found`.
fn unit_hint(found: &str) -> UnitHint {
    match found.to_ascii_lowercase().as_str() {
        "m" => UnitHint::MonthOrMinute,
        "day" | "days" => UnitHint::Days,
        "week" | "weeks" | "wk" => UnitHint::Weeks,
        "month" | "months" => UnitHint::Months,
        "year" | "years" | "yr" | "yrs" => UnitHint::Years,
        "hour" | "hours" | "hr" | "hrs" => UnitHint::Hours,
        "minute" | "minutes" | "mins" => UnitHint::Minutes,
        _ => UnitHint::AnyUnit,
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
    let mut chars = format.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c != '%' {
            lit.push(c);
            continue;
        }
        let mut pad = Pad::Default;
        if let Some(&(_, flag @ ('-' | '_'))) = chars.peek() {
            pad = if flag == '-' { Pad::None } else { Pad::Space };
            chars.next();
        }
        let Some((spec_at, spec)) = chars.next() else {
            return err(
                offset + at..offset + at + 1,
                TemplateErrorKind::TrailingPercent,
            );
        };
        let parsed = match spec {
            '%' if pad == Pad::Default => {
                lit.push('%');
                continue;
            }
            'Y' => Spec::Year4,
            'y' => Spec::Year2,
            'G' => Spec::IsoYear,
            'm' => Spec::Month,
            'B' => Spec::MonthName,
            'b' | 'h' => Spec::MonthAbbr,
            'd' => Spec::Day,
            'e' => {
                pad = if pad == Pad::Default { Pad::Space } else { pad };
                Spec::Day
            }
            'j' => Spec::DayOfYear,
            'A' => Spec::Weekday,
            'a' => Spec::WeekdayAbbr,
            'u' => Spec::WeekdayNumber,
            'V' => Spec::IsoWeek,
            'q' => Spec::Quarter,
            'H' => Spec::Hour24,
            'I' => Spec::Hour12,
            'M' => Spec::Minute,
            'S' => Spec::Second,
            'p' => Spec::AmPm,
            'z' => Spec::Offset,
            'Z' => Spec::ZoneName,
            'F' => Spec::IsoDate,
            'T' => Spec::IsoTime,
            'R' => Spec::ShortTime,
            specifier => {
                return err(
                    offset + at..offset + spec_at + specifier.len_utf8(),
                    TemplateErrorKind::UnknownFormatSpecifier { specifier },
                );
            }
        };
        if !lit.is_empty() {
            parts.push(DtPart::Lit(core::mem::take(&mut lit)));
        }
        parts.push(DtPart::Spec(parsed, pad));
    }
    if !lit.is_empty() {
        parts.push(DtPart::Lit(lit));
    }
    Ok(parts)
}

/// `{date}` with no format: the day in the locale's style.
fn date_string(when: &Zoned, style: DateStyle) -> String {
    style.write(when.year(), when.month(), when.day())
}

/// `{time}` with no format: the time of day in the locale's style.
fn time_string(when: &Zoned, locale: Locale) -> String {
    locale.clock.write(locale.lang, when.hour(), when.minute())
}

/// Writes `value` to `width` digits: zero padded, space padded or not at all.
fn write_number(out: &mut String, value: i64, width: usize, pad: Pad, default: Pad) {
    // Writing to a `String` cannot fail.
    let _ = match if pad == Pad::Default { default } else { pad } {
        Pad::None | Pad::Default => write!(out, "{value}"),
        Pad::Space => write!(out, "{value:width$}"),
        Pad::Zero => write!(out, "{value:0width$}"),
    };
}

fn write_spec(out: &mut String, when: &Zoned, spec: Spec, pad: Pad, locale: Locale) {
    let lang = locale.lang;
    let month_index = usize::try_from(when.month() - 1).unwrap_or_default();
    let weekday_index = usize::try_from(when.weekday().to_monday_zero_offset()).unwrap_or_default();
    let mut number = |value: i64, width: usize| write_number(out, value, width, pad, Pad::Zero);
    match spec {
        Spec::Year4 => number(i64::from(when.year()), 4),
        Spec::Year2 => number(i64::from(when.year().rem_euclid(100)), 2),
        Spec::IsoYear => number(i64::from(when.date().iso_week_date().year()), 4),
        Spec::Month => number(i64::from(when.month()), 2),
        Spec::Day => number(i64::from(when.day()), 2),
        Spec::DayOfYear => number(i64::from(when.day_of_year()), 3),
        Spec::WeekdayNumber => number(i64::from(when.weekday().to_monday_one_offset()), 1),
        Spec::IsoWeek => number(i64::from(when.date().iso_week_date().week()), 2),
        Spec::Quarter => number(i64::from((when.month() - 1) / 3 + 1), 1),
        Spec::Hour24 => number(i64::from(when.hour()), 2),
        Spec::Hour12 => number(i64::from(hour12(when)), 2),
        Spec::Minute => number(i64::from(when.minute()), 2),
        Spec::Second => number(i64::from(when.second()), 2),
        Spec::MonthName => {
            out.push_str(lang.months().get(month_index).copied().unwrap_or_default());
        }
        Spec::MonthAbbr => {
            out.push_str(
                lang.months_abbr()
                    .get(month_index)
                    .copied()
                    .unwrap_or_default(),
            );
        }
        Spec::Weekday => {
            out.push_str(
                lang.weekdays()
                    .get(weekday_index)
                    .copied()
                    .unwrap_or_default(),
            );
        }
        Spec::WeekdayAbbr => {
            out.push_str(
                lang.weekdays_abbr()
                    .get(weekday_index)
                    .copied()
                    .unwrap_or_default(),
            );
        }
        Spec::AmPm => {
            let (am, pm) = lang.am_pm();
            out.push_str(if when.hour() < 12 { am } else { pm });
        }
        Spec::Offset => {
            let seconds = when.offset().seconds();
            let sign = if seconds < 0 { '-' } else { '+' };
            let minutes = seconds.unsigned_abs() / 60;
            // Writing to a `String` cannot fail.
            let _ = write!(out, "{sign}{:02}{:02}", minutes / 60, minutes % 60);
        }
        Spec::ZoneName => match when.time_zone().iana_name() {
            Some(name) => out.push_str(name),
            None => write_spec(out, when, Spec::Offset, Pad::Default, locale),
        },
        Spec::IsoDate => out.push_str(&DateStyle::ISO.write(when.year(), when.month(), when.day())),
        Spec::IsoTime => {
            // Writing to a `String` cannot fail.
            let _ = write!(
                out,
                "{:02}:{:02}:{:02}",
                when.hour(),
                when.minute(),
                when.second()
            );
        }
        Spec::ShortTime => {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "{:02}:{:02}", when.hour(), when.minute());
        }
    }
}

fn hour12(when: &Zoned) -> i8 {
    match when.hour() % 12 {
        0 => 12,
        h => h,
    }
}
