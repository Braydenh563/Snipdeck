//! Output formats and their copy text.

use crate::model::{Rgba8, to_cmyk, to_hsl, to_hsv, to_oklch};
use crate::names::nearest_name;
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
fn num(x: f64, dp: i32) -> String {
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

/// An integer percentage of a 0..1 value.
fn pct(x: f64) -> String {
    num(x * 100.0, 0)
}

/// A hue as an integer 0..360 (360 prints as 0).
fn hue0(h: f64) -> String {
    let r = h.round();
    num(if r >= 360.0 { 0.0 } else { r }, 0)
}

fn alpha(a: u8) -> String {
    num(f64::from(a) / 255.0, 2)
}

/// The copy text of `c` in format `f` (see the spec's D3 and the rounding rules).
pub fn format(c: Rgba8, f: Format) -> String {
    match f {
        Format::Hex => c.to_hex(),
        Format::Rgb => {
            if c.a == 255 {
                format!("rgb({}, {}, {})", c.r, c.g, c.b)
            } else {
                format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, alpha(c.a))
            }
        }
        Format::Hsl => {
            let v = to_hsl(c);
            if c.a == 255 {
                format!("hsl({}, {}%, {}%)", hue0(v.h), pct(v.s), pct(v.l))
            } else {
                format!(
                    "hsla({}, {}%, {}%, {})",
                    hue0(v.h),
                    pct(v.s),
                    pct(v.l),
                    alpha(c.a)
                )
            }
        }
        Format::Hsb => {
            let v = to_hsv(c);
            format!("hsb({}, {}%, {}%)", hue0(v.h), pct(v.s), pct(v.v))
        }
        Format::Cmyk => {
            let v = to_cmyk(c);
            format!(
                "cmyk({}%, {}%, {}%, {}%)",
                pct(v.c),
                pct(v.m),
                pct(v.y),
                pct(v.k)
            )
        }
        Format::Oklch => oklch_text(c),
        Format::Name => nearest_name(c).name.to_owned(),
    }
}

fn oklch_text(c: Rgba8) -> String {
    let v = to_oklch(c);
    let l = num(v.l * 100.0, 2);
    let chroma = num(v.c, 4);
    let tail = if chroma == "0" {
        "0 0".to_owned()
    } else {
        let h = num(v.h, 2);
        format!("{chroma} {}", if h == "360" { "0".to_owned() } else { h })
    };
    if c.a == 255 {
        format!("oklch({l}% {tail})")
    } else {
        format!("oklch({l}% {tail} / {})", alpha(c.a))
    }
}
