//! The date, time and number conventions a template renders with: how `{date}` and `{time}`
//! are written, the names of months and weekdays, the decimal mark of a `{calc}`, and how a
//! typed date is read back.
//!
//! A [`Locale`] has two sources. [`Locale::for_tag`] maps a UI language tag (`de`, `pt-BR`)
//! to a table and gives the *names* (months, weekdays, AM/PM). [`Locale::with_regional`]
//! then replaces the numeric date and clock style with the user's own short-date and
//! short-time patterns (`dd.MM.yyyy`, `h:mm tt`), which the OS knows better than any table.
//! Both are pure: the caller reads the system.

use jiff::civil::Date;

/// The language of month and weekday names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English.
    #[default]
    En,
    /// Spanish.
    Es,
    /// German.
    De,
    /// French.
    Fr,
    /// Portuguese.
    Pt,
    /// Japanese.
    Ja,
    /// Chinese (simplified).
    Zh,
}

impl Lang {
    /// The language of a tag such as `de`, `pt-BR` or `zh_Hans`; English if unknown.
    #[must_use]
    pub fn of_tag(tag: &str) -> Self {
        let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
        match tag.split('-').next().unwrap_or_default() {
            "es" => Self::Es,
            "de" => Self::De,
            "fr" => Self::Fr,
            "pt" => Self::Pt,
            "ja" => Self::Ja,
            "zh" => Self::Zh,
            _ => Self::En,
        }
    }

    /// The full month names, January first.
    #[must_use]
    pub const fn months(self) -> [&'static str; 12] {
        match self {
            Self::En => [
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
            ],
            Self::Es => [
                "enero",
                "febrero",
                "marzo",
                "abril",
                "mayo",
                "junio",
                "julio",
                "agosto",
                "septiembre",
                "octubre",
                "noviembre",
                "diciembre",
            ],
            Self::De => [
                "Januar",
                "Februar",
                "März",
                "April",
                "Mai",
                "Juni",
                "Juli",
                "August",
                "September",
                "Oktober",
                "November",
                "Dezember",
            ],
            Self::Fr => [
                "janvier",
                "février",
                "mars",
                "avril",
                "mai",
                "juin",
                "juillet",
                "août",
                "septembre",
                "octobre",
                "novembre",
                "décembre",
            ],
            Self::Pt => [
                "janeiro",
                "fevereiro",
                "março",
                "abril",
                "maio",
                "junho",
                "julho",
                "agosto",
                "setembro",
                "outubro",
                "novembro",
                "dezembro",
            ],
            Self::Ja => [
                "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月",
                "12月",
            ],
            Self::Zh => [
                "一月",
                "二月",
                "三月",
                "四月",
                "五月",
                "六月",
                "七月",
                "八月",
                "九月",
                "十月",
                "十一月",
                "十二月",
            ],
        }
    }

    /// The abbreviated month names, January first.
    #[must_use]
    pub const fn months_abbr(self) -> [&'static str; 12] {
        match self {
            Self::En => [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ],
            Self::Es => [
                "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
            ],
            Self::De => [
                "Jan", "Feb", "Mär", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez",
            ],
            Self::Fr => [
                "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.",
                "nov.", "déc.",
            ],
            Self::Pt => [
                "jan.", "fev.", "mar.", "abr.", "mai.", "jun.", "jul.", "ago.", "set.", "out.",
                "nov.", "dez.",
            ],
            Self::Ja | Self::Zh => [
                "1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月",
                "12月",
            ],
        }
    }

    /// The full weekday names, Monday first.
    #[must_use]
    pub const fn weekdays(self) -> [&'static str; 7] {
        match self {
            Self::En => [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ],
            Self::Es => [
                "lunes",
                "martes",
                "miércoles",
                "jueves",
                "viernes",
                "sábado",
                "domingo",
            ],
            Self::De => [
                "Montag",
                "Dienstag",
                "Mittwoch",
                "Donnerstag",
                "Freitag",
                "Samstag",
                "Sonntag",
            ],
            Self::Fr => [
                "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
            ],
            Self::Pt => [
                "segunda-feira",
                "terça-feira",
                "quarta-feira",
                "quinta-feira",
                "sexta-feira",
                "sábado",
                "domingo",
            ],
            Self::Ja => [
                "月曜日",
                "火曜日",
                "水曜日",
                "木曜日",
                "金曜日",
                "土曜日",
                "日曜日",
            ],
            Self::Zh => [
                "星期一",
                "星期二",
                "星期三",
                "星期四",
                "星期五",
                "星期六",
                "星期日",
            ],
        }
    }

    /// The abbreviated weekday names, Monday first.
    #[must_use]
    pub const fn weekdays_abbr(self) -> [&'static str; 7] {
        match self {
            Self::En => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
            Self::Es => ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
            Self::De => ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"],
            Self::Fr => ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
            Self::Pt => ["seg.", "ter.", "qua.", "qui.", "sex.", "sáb.", "dom."],
            Self::Ja => ["月", "火", "水", "木", "金", "土", "日"],
            Self::Zh => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"],
        }
    }

    /// The words for before and after noon.
    #[must_use]
    pub const fn am_pm(self) -> (&'static str, &'static str) {
        match self {
            Self::Ja => ("午前", "午後"),
            Self::Zh => ("上午", "下午"),
            _ => ("AM", "PM"),
        }
    }
}

/// The order of day, month and year in a numeric date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    /// Day, month, year (`17/10/2026`).
    DayMonthYear,
    /// Month, day, year (`10/17/2026`).
    MonthDayYear,
    /// Year, month, day (`2026-10-17`).
    YearMonthDay,
}

/// How `{date}` is written: an order, one separator and the padding of each part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateStyle {
    /// Which part comes first.
    pub order: DateOrder,
    /// The character between the parts.
    pub separator: char,
    /// Day written with two digits (`07`) rather than one (`7`).
    pub pad_day: bool,
    /// Month written with two digits.
    pub pad_month: bool,
    /// Year written with two digits (`26`) rather than four.
    pub short_year: bool,
}

impl Default for DateStyle {
    fn default() -> Self {
        Self::ISO
    }
}

impl DateStyle {
    /// `2026-10-17`.
    pub const ISO: Self = Self::new(DateOrder::YearMonthDay, '-');
    /// `17/10/2026`.
    pub const DAY_MONTH_YEAR: Self = Self::new(DateOrder::DayMonthYear, '/');
    /// `10/17/2026`.
    pub const MONTH_DAY_YEAR: Self = Self::new(DateOrder::MonthDayYear, '/');
    /// `17.10.2026`.
    pub const DOT_DAY_MONTH_YEAR: Self = Self::new(DateOrder::DayMonthYear, '.');
    /// `2026/10/17`.
    pub const YEAR_MONTH_DAY_SLASH: Self = Self::new(DateOrder::YearMonthDay, '/');

    const fn new(order: DateOrder, separator: char) -> Self {
        Self {
            order,
            separator,
            pad_day: true,
            pad_month: true,
            short_year: false,
        }
    }

    /// Reads a Windows short-date pattern such as `dd/MM/yyyy`, `d.M.yyyy` or `yyyy-MM-dd`.
    /// `None` for a pattern this style cannot express (month names, two kinds of separator,
    /// literal text, an era).
    #[must_use]
    pub fn from_pattern(pattern: &str) -> Option<Self> {
        let mut order = Vec::new();
        let mut style = Self::ISO;
        let mut separator: Option<char> = None;
        let mut chars = pattern
            .chars()
            .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}'))
            .peekable();
        while let Some(c) = chars.next() {
            if matches!(c, 'd' | 'M' | 'y') {
                let mut run = 1;
                while chars.peek() == Some(&c) {
                    chars.next();
                    run += 1;
                }
                match (c, run) {
                    ('d', 1 | 2) => {
                        order.push('d');
                        style.pad_day = run == 2;
                    }
                    ('M', 1 | 2) => {
                        order.push('M');
                        style.pad_month = run == 2;
                    }
                    ('y', 2) => {
                        order.push('y');
                        style.short_year = true;
                    }
                    ('y', 4) => {
                        order.push('y');
                        style.short_year = false;
                    }
                    _ => return None,
                }
            } else if c.is_alphanumeric() || c == '\'' {
                return None;
            } else if order.len() < 3 && !order.is_empty() {
                match separator {
                    None if !c.is_whitespace() => separator = Some(c),
                    Some(s) if s == c => {}
                    // A space after the separator ("d. M. yyyy") is part of it; anything else
                    // is two kinds of separator.
                    _ if c.is_whitespace() => {}
                    _ => return None,
                }
            }
            // Trailing separators ("yyyy. MM. dd.") after the last part are ignored.
        }
        style.order = match order.as_slice() {
            ['d', 'M', 'y'] => DateOrder::DayMonthYear,
            ['M', 'd', 'y'] => DateOrder::MonthDayYear,
            ['y', 'M', 'd'] => DateOrder::YearMonthDay,
            _ => return None,
        };
        style.separator = separator?;
        Some(style)
    }

    /// The date written in this style.
    #[must_use]
    pub fn write(self, year: i16, month: i8, day: i8) -> String {
        let d = if self.pad_day {
            format!("{day:02}")
        } else {
            day.to_string()
        };
        let m = if self.pad_month {
            format!("{month:02}")
        } else {
            month.to_string()
        };
        let y = if self.short_year {
            format!("{:02}", year.rem_euclid(100))
        } else {
            format!("{year:04}")
        };
        let s = self.separator;
        match self.order {
            DateOrder::DayMonthYear => format!("{d}{s}{m}{s}{y}"),
            DateOrder::MonthDayYear => format!("{m}{s}{d}{s}{y}"),
            DateOrder::YearMonthDay => format!("{y}{s}{m}{s}{d}"),
        }
    }

    /// The format as letters, as a hint for a date field: `DD/MM/YYYY`.
    #[must_use]
    pub fn hint(self) -> String {
        let d = if self.pad_day { "DD" } else { "D" };
        let m = if self.pad_month { "MM" } else { "M" };
        let y = if self.short_year { "YY" } else { "YYYY" };
        let s = self.separator;
        match self.order {
            DateOrder::DayMonthYear => format!("{d}{s}{m}{s}{y}"),
            DateOrder::MonthDayYear => format!("{m}{s}{d}{s}{y}"),
            DateOrder::YearMonthDay => format!("{y}{s}{m}{s}{d}"),
        }
    }

    /// Reads a typed date: ISO (`2026-10-17`, year first with four digits) in any style, or
    /// the three numbers in this style's order with any non-digit between them. A two-digit
    /// year is 20xx. `None` for anything else or a day that does not exist.
    #[must_use]
    pub fn parse(self, text: &str) -> Option<Date> {
        let text = text.trim();
        let groups: Vec<&str> = text
            .split(|c: char| !c.is_ascii_digit())
            .filter(|g| !g.is_empty())
            .collect();
        // Only digits, spaces and the usual separators may appear.
        if text.is_empty()
            || !text
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | '/' | '.' | ' ' | ','))
        {
            return None;
        }
        let [first, second, third] = groups.as_slice() else {
            return None;
        };
        let (y, m, d) = if first.len() == 4 {
            (first, second, third)
        } else {
            match self.order {
                DateOrder::DayMonthYear => (third, second, first),
                DateOrder::MonthDayYear => (third, first, second),
                DateOrder::YearMonthDay => (first, second, third),
            }
        };
        if m.len() > 2 || d.len() > 2 || !matches!(y.len(), 2 | 4) {
            return None;
        }
        let mut year: i16 = y.parse().ok()?;
        if y.len() == 2 {
            year += 2000;
        }
        Date::new(year, m.parse().ok()?, d.parse().ok()?).ok()
    }
}

/// How `{time}` is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockStyle {
    /// Twelve-hour clock with AM/PM.
    pub hour12: bool,
    /// The hour written with two digits.
    pub pad_hour: bool,
    /// The character between hour and minute.
    pub separator: char,
    /// The AM/PM word comes before the time (`午前 9:05`).
    pub ampm_first: bool,
}

impl Default for ClockStyle {
    fn default() -> Self {
        Self::H24
    }
}

impl ClockStyle {
    /// `14:05`.
    pub const H24: Self = Self {
        hour12: false,
        pad_hour: true,
        separator: ':',
        ampm_first: false,
    };
    /// `2:05 PM`.
    pub const H12: Self = Self {
        hour12: true,
        pad_hour: false,
        separator: ':',
        ampm_first: false,
    };

    /// Reads a Windows short-time pattern such as `HH:mm`, `H:mm`, `h:mm tt` or `tt h:mm`.
    /// `None` when it has no hour and minute in the usual shape.
    #[must_use]
    pub fn from_pattern(pattern: &str) -> Option<Self> {
        let pattern: String = pattern
            .chars()
            .filter(|c| !matches!(c, '\u{200e}' | '\u{200f}'))
            .collect();
        let hour_at = pattern.find(['H', 'h'])?;
        let hour_char = pattern[hour_at..].chars().next()?;
        let run = pattern[hour_at..]
            .chars()
            .take_while(|c| *c == hour_char)
            .count();
        if run > 2 {
            return None;
        }
        let after = &pattern[hour_at + run..];
        let separator = after.chars().next().filter(|c| !c.is_alphanumeric())?;
        let minutes = after[separator.len_utf8()..].strip_prefix("mm")?;
        let hour12 = hour_char == 'h';
        let has_ampm = pattern.contains("tt");
        if hour12 != has_ampm {
            return None;
        }
        // Only an AM/PM word and spaces may surround the digits.
        let trailing = minutes.replace("tt", "");
        if trailing.chars().any(|c| !c.is_whitespace()) {
            return None;
        }
        Some(Self {
            hour12,
            pad_hour: run == 2,
            separator,
            ampm_first: has_ampm && pattern.find("tt").is_some_and(|t| t < hour_at),
        })
    }

    /// The time written in this style.
    #[must_use]
    pub fn write(self, lang: Lang, hour: i8, minute: i8) -> String {
        let shown = if self.hour12 {
            match hour % 12 {
                0 => 12,
                h => h,
            }
        } else {
            hour
        };
        let hour_text = if self.pad_hour {
            format!("{shown:02}")
        } else {
            shown.to_string()
        };
        let digits = format!("{hour_text}{}{minute:02}", self.separator);
        if !self.hour12 {
            return digits;
        }
        let (am, pm) = lang.am_pm();
        let word = if hour < 12 { am } else { pm };
        if self.ampm_first {
            format!("{word} {digits}")
        } else {
            format!("{digits} {word}")
        }
    }
}

/// The date, time, name and number conventions of a locale. The default is ISO dates, a
/// 24-hour clock, English names and a decimal point, which reads the same everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    /// How `{date}` is written.
    pub date: DateStyle,
    /// How `{time}` is written.
    pub clock: ClockStyle,
    /// The language of month and weekday names.
    pub lang: Lang,
    /// The decimal mark of a `{calc}` result.
    pub decimal: char,
    /// The week of a calendar starts on Sunday rather than Monday.
    pub week_starts_sunday: bool,
}

impl Default for Locale {
    fn default() -> Self {
        Self {
            date: DateStyle::ISO,
            clock: ClockStyle::H24,
            lang: Lang::En,
            decimal: '.',
            week_starts_sunday: false,
        }
    }
}

impl Locale {
    /// The conventions for a language tag such as `de`, `pt-BR` or `zh_Hans`, covering
    /// Snipdeck's launch locales (SPEC §7). Matching ignores case and treats `_` as `-`;
    /// an unknown tag gives the ISO default with English names.
    #[must_use]
    pub fn for_tag(tag: &str) -> Self {
        let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
        let lang = Lang::of_tag(&tag);
        let (date, clock) = match tag.as_str() {
            "en-gb" | "en-au" | "en-nz" | "en-ie" | "en-in" => {
                (DateStyle::DAY_MONTH_YEAR, ClockStyle::H24)
            }
            _ => match tag.split('-').next().unwrap_or_default() {
                "en" => (DateStyle::MONTH_DAY_YEAR, ClockStyle::H12),
                "es" | "fr" | "pt" => (DateStyle::DAY_MONTH_YEAR, ClockStyle::H24),
                "de" => (DateStyle::DOT_DAY_MONTH_YEAR, ClockStyle::H24),
                "ja" | "zh" => (DateStyle::YEAR_MONTH_DAY_SLASH, ClockStyle::H24),
                _ => (DateStyle::ISO, ClockStyle::H24),
            },
        };
        let decimal = if matches!(lang, Lang::Es | Lang::De | Lang::Fr | Lang::Pt) {
            ','
        } else {
            '.'
        };
        let week_starts_sunday =
            matches!(lang, Lang::Ja) || (lang == Lang::En && date == DateStyle::MONTH_DAY_YEAR);
        Self {
            date,
            clock,
            lang,
            decimal,
            week_starts_sunday,
        }
    }

    /// This locale with the numeric date and clock style of the user's regional short-date and
    /// short-time patterns (Windows `LOCALE_SSHORTDATE` and `LOCALE_SSHORTTIME`). A pattern
    /// that is missing or cannot be read keeps the style from the language table.
    #[must_use]
    pub fn with_regional(mut self, short_date: Option<&str>, short_time: Option<&str>) -> Self {
        if let Some(date) = short_date.and_then(DateStyle::from_pattern) {
            self.date = date;
        }
        if let Some(clock) = short_time.and_then(ClockStyle::from_pattern) {
            self.clock = clock;
        }
        self
    }
}
