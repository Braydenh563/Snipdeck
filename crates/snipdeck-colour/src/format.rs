//! Output formats and their copy text.

use crate::model::{Rgba8, to_cmyk, to_hsl, to_hsv, to_oklch, unit_to_u8};
use crate::names::nearest_name;
use crate::parse::parse;
use serde::{Deserialize, Serialize};

/// A colour format shown as a row and offered for copying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// `#rrggbb[aa]`, the default format.
    #[default]
    Hex,
    /// `rgb(r, g, b)` / `rgba(...)`.
    Rgb,
    /// `hsl(h, s%, l%)` / `hsla(...)`.
    Hsl,
    /// `hsb(h, s%, b%)` (HSV).
    Hsb,
    /// `cmyk(c%, m%, y%, k%)`.
    Cmyk,
    /// `oklch(L% C h)`.
    Oklch,
    /// The nearest CSS keyword; lossy, so never the default.
    Name,
}

impl Format {
    /// Every format in display order (the rows).
    pub const ALL: [Self; 7] = [
        Self::Hex,
        Self::Rgb,
        Self::Hsl,
        Self::Hsb,
        Self::Cmyk,
        Self::Oklch,
        Self::Name,
    ];

    /// Whether the format may be the default copy format (everything but [`Format::Name`]).
    pub const fn can_be_default(self) -> bool {
        !matches!(self, Self::Name)
    }

    /// The row label: `HEX`, `RGB`, `HSL`, `HSB`, `CMYK`, `OKLCH`. For `Name` this returns `"Name"`;
    /// the UI uses its own translated text.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hex => "HEX",
            Self::Rgb => "RGB",
            Self::Hsl => "HSL",
            Self::Hsb => "HSB",
            Self::Cmyk => "CMYK",
            Self::Oklch => "OKLCH",
            Self::Name => "Name",
        }
    }
}

/// `round(x * 10^dp) / 10^dp` printed with trailing zeros and a trailing dot trimmed.
pub(crate) fn num(x: f64, dp: i32) -> String {
    let scale = 10f64.powi(dp);
    let r = (x * scale).round() / scale;
    if r.abs() < 0.5 / scale {
        return "0".to_owned();
    }
    let mut s = format!("{r:.prec$}", prec = usize::try_from(dp).unwrap_or(0));
    if s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
    s
}

/// A percentage of a 0..1 value with `dp` decimals.
fn pct(x: f64, dp: i32) -> String {
    num(x * 100.0, dp)
}

/// A hue with `dp` decimals in 0..360 (360 prints as 0).
fn hue_dp(h: f64, dp: i32) -> String {
    let t = num(h, dp);
    if t.parse::<f64>().is_ok_and(|v| v >= 360.0) {
        "0".to_owned()
    } else {
        t
    }
}

/// The shortest alpha text (2 decimals, else 3) that parses back to the same byte, so every one of
/// the 256 alpha values survives a copy and a paste.
pub(crate) fn alpha(a: u8) -> String {
    let x = f64::from(a) / 255.0;
    for dp in 2..=3 {
        let t = num(x, dp);
        if t.parse::<f64>().is_ok_and(|v| unit_to_u8(v) == a) {
            return t;
        }
    }
    num(x, 4)
}

/// The first of `make(0)`, `make(1)`, ... (one more decimal each) that parses back to `c`: one
/// rule for every format, so copy and paste never changes a colour. The last attempt is returned
/// when none does (not seen over the whole sRGB cube).
fn shortest(c: Rgba8, make: impl Fn(i32) -> String) -> String {
    let mut last = String::new();
    for dp in 0..=4 {
        last = make(dp);
        if parse(&last) == Ok(c) {
            return last;
        }
    }
    last
}

/// The copy text of `c` in format `f`: always parses back to `c` (except [`Format::Name`], which
/// is the nearest keyword; see [`copy_text()`]). Alpha is written in every format.
pub fn format(c: Rgba8, f: Format) -> String {
    let opaque = c.a == 255;
    match f {
        Format::Hex => c.to_hex(),
        Format::Rgb => {
            if opaque {
                format!("rgb({}, {}, {})", c.r, c.g, c.b)
            } else {
                format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, alpha(c.a))
            }
        }
        Format::Hsl => shortest(c, |dp| {
            let v = to_hsl(c);
            let (h, s, l) = (hue_dp(v.h, dp), pct(v.s, dp), pct(v.l, dp));
            if opaque {
                format!("hsl({h}, {s}%, {l}%)")
            } else {
                format!("hsla({h}, {s}%, {l}%, {})", alpha(c.a))
            }
        }),
        Format::Hsb => shortest(c, |dp| {
            let v = to_hsv(c);
            let (h, s, b) = (hue_dp(v.h, dp), pct(v.s, dp), pct(v.v, dp));
            if opaque {
                format!("hsb({h}, {s}%, {b}%)")
            } else {
                format!("hsba({h}, {s}%, {b}%, {})", alpha(c.a))
            }
        }),
        Format::Cmyk => shortest(c, |dp| {
            let v = to_cmyk(c);
            let body = format!(
                "{}%, {}%, {}%, {}%",
                pct(v.c, dp),
                pct(v.m, dp),
                pct(v.y, dp),
                pct(v.k, dp)
            );
            if opaque {
                format!("cmyk({body})")
            } else {
                format!("cmyk({body} / {})", alpha(c.a))
            }
        }),
        Format::Oklch => shortest(c, |dp| oklch_text(c, dp)),
        Format::Name => nearest_name(c).name.to_owned(),
    }
}

/// What a row copies: [`format()`], except [`Format::Name`], which is the keyword only when `c` is
/// exactly that keyword (the nearest one is a different colour, so pasting it would change it).
pub fn copy_text(c: Rgba8, f: Format) -> Option<String> {
    if f == Format::Name {
        let n = nearest_name(c);
        return (n.exact && c.a == 255).then(|| n.name.to_owned());
    }
    Some(format(c, f))
}

fn oklch_text(c: Rgba8, extra: i32) -> String {
    let v = to_oklch(c);
    let l = num(v.l * 100.0, 2 + extra);
    let chroma = num(v.c, 4 + extra);
    let tail = if chroma == "0" {
        "0 0".to_owned()
    } else {
        let h = hue_dp(v.h, 2 + extra);
        format!("{chroma} {h}")
    };
    if c.a == 255 {
        format!("oklch({l}% {tail})")
    } else {
        format!("oklch({l}% {tail} / {})", alpha(c.a))
    }
}
