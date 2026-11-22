//! Extra copy formats beyond the seven rows: upper-case and bare hex, `0x`, decimal, the CSS Color 4
//! spaces, floats, and the code forms of Swift, `SwiftUI`, Compose and .NET, plus user templates.

use crate::css4::{gamma, to_lab, to_p3_linear};
use crate::format::{alpha, num};
use crate::model::{Rgba8, to_hsl, to_hsv, to_oklch};
use crate::parse::parse;
use serde::{Deserialize, Serialize};

/// A copy format offered in the "More formats" menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtraFormat {
    /// `#3A7BD5`.
    HexUpper,
    /// `3a7bd5` (no `#`).
    HexBare,
    /// `0x3a7bd5`, or `0xAARRGGBB` with alpha.
    ZeroX,
    /// `hwb(h w% b%)`.
    Hwb,
    /// `lab(L a b)` (CSS Color 4, D50).
    Lab,
    /// `lch(L C h)`.
    Lch,
    /// `oklab(L a b)`.
    Oklab,
    /// `color(srgb r g b)`, channels 0..1.
    SrgbFloat,
    /// `color(display-p3 r g b)`.
    DisplayP3,
    /// `0.227, 0.482, 0.835` (0..1 floats, a fourth for alpha).
    Floats,
    /// `UIColor(red: 0.227, green: 0.482, blue: 0.835, alpha: 1)`.
    UiColor,
    /// `Color(red: 0.227, green: 0.482, blue: 0.835, opacity: 1)` (`SwiftUI`).
    SwiftUi,
    /// `Color(0xFF3A7BD5)` (Jetpack Compose).
    Compose,
    /// `Color.FromArgb(255, 58, 123, 213)` (.NET).
    FromArgb,
}

impl ExtraFormat {
    /// Every extra format in menu order.
    pub const ALL: [Self; 14] = [
        Self::HexUpper,
        Self::HexBare,
        Self::ZeroX,
        Self::Hwb,
        Self::Lab,
        Self::Lch,
        Self::Oklab,
        Self::SrgbFloat,
        Self::DisplayP3,
        Self::Floats,
        Self::UiColor,
        Self::SwiftUi,
        Self::Compose,
        Self::FromArgb,
    ];

    /// The menu label (code names, never translated).
    pub const fn label(self) -> &'static str {
        match self {
            Self::HexUpper => "HEX (upper-case)",
            Self::HexBare => "HEX without #",
            Self::ZeroX => "0xRRGGBB",
            Self::Hwb => "HWB",
            Self::Lab => "Lab",
            Self::Lch => "LCH",
            Self::Oklab => "OKLab",
            Self::SrgbFloat => "color(srgb)",
            Self::DisplayP3 => "Display P3",
            Self::Floats => "Floats 0..1",
            Self::UiColor => "Swift UIColor",
            Self::SwiftUi => "SwiftUI Color",
            Self::Compose => "Compose Color",
            Self::FromArgb => ".NET FromArgb",
        }
    }
}

fn argb(c: Rgba8) -> u32 {
    u32::from_be_bytes([c.a, c.r, c.g, c.b])
}

fn unit(v: f64, extra: i32) -> String {
    num(v, 3 + extra)
}

/// A float 0..1 text for `byte`, one more decimal at a time until it reads back as `byte`.
fn float_of(byte: u8, extra: i32) -> String {
    unit(f64::from(byte) / 255.0, extra)
}

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

/// The text of `c` in an extra format. Everything the parser reads ([`parse`]) round-trips; the
/// `Floats` form is for code and is not parsed back.
pub fn extra_text(c: Rgba8, f: ExtraFormat) -> String {
    let opaque = c.a == 255;
    match f {
        ExtraFormat::HexUpper => c.to_hex().to_uppercase(),
        ExtraFormat::HexBare => c.to_hex().trim_start_matches('#').to_owned(),
        ExtraFormat::ZeroX => {
            if opaque {
                format!("0x{:02x}{:02x}{:02x}", c.r, c.g, c.b)
            } else {
                format!("0x{:08x}", argb(c))
            }
        }
        ExtraFormat::Hwb => shortest(c, |dp| {
            let v = to_hsv(c);
            let w = (1.0 - v.s) * v.v;
            let b = 1.0 - v.v;
            let h = if v.s == 0.0 { 0.0 } else { v.h };
            let tail = if opaque {
                String::new()
            } else {
                format!(" / {}", alpha(c.a))
            };
            format!(
                "hwb({} {}% {}%{tail})",
                num(h, dp),
                num(w * 100.0, dp),
                num(b * 100.0, dp)
            )
        }),
        ExtraFormat::Lab => shortest(c, |dp| {
            let (l, a, b) = to_lab(c);
            lab_like("lab", c, &[num(l, 1 + dp), num(a, 1 + dp), num(b, 1 + dp)])
        }),
        ExtraFormat::Lch => shortest(c, |dp| {
            let (l, a, b) = to_lab(c);
            let ch = a.hypot(b);
            let h = if ch < 0.01 {
                0.0
            } else {
                b.atan2(a).to_degrees().rem_euclid(360.0)
            };
            lab_like("lch", c, &[num(l, 1 + dp), num(ch, 1 + dp), num(h, 1 + dp)])
        }),
        ExtraFormat::Oklab => shortest(c, |dp| {
            let (l, a, b) = crate::css4::oklab(c);
            lab_like(
                "oklab",
                c,
                &[num(l, 3 + dp), num(a, 4 + dp), num(b, 4 + dp)],
            )
        }),
        ExtraFormat::SrgbFloat => shortest(c, |dp| {
            color_fn("srgb", c, &[c.r, c.g, c.b].map(|v| float_of(v, dp)))
        }),
        ExtraFormat::DisplayP3 => shortest(c, |dp| {
            let p3 = to_p3_linear(c).map(|v| unit(gamma(v).clamp(0.0, 1.0), dp));
            color_fn("display-p3", c, &p3)
        }),
        ExtraFormat::Floats => {
            let mut parts = vec![float_of(c.r, 0), float_of(c.g, 0), float_of(c.b, 0)];
            if !opaque {
                parts.push(float_of(c.a, 0));
            }
            parts.join(", ")
        }
        ExtraFormat::UiColor => shortest(c, |dp| {
            format!(
                "UIColor(red: {}, green: {}, blue: {}, alpha: {})",
                float_of(c.r, dp),
                float_of(c.g, dp),
                float_of(c.b, dp),
                float_of(c.a, dp)
            )
        }),
        ExtraFormat::SwiftUi => shortest(c, |dp| {
            format!(
                "Color(red: {}, green: {}, blue: {}, opacity: {})",
                float_of(c.r, dp),
                float_of(c.g, dp),
                float_of(c.b, dp),
                float_of(c.a, dp)
            )
        }),
        ExtraFormat::Compose => format!("Color(0x{:08X})", argb(c)),
        ExtraFormat::FromArgb => {
            format!("Color.FromArgb({}, {}, {}, {})", c.a, c.r, c.g, c.b)
        }
    }
}

fn lab_like(name: &str, c: Rgba8, v: &[String; 3]) -> String {
    if c.a == 255 {
        format!("{name}({} {} {})", v[0], v[1], v[2])
    } else {
        format!("{name}({} {} {} / {})", v[0], v[1], v[2], alpha(c.a))
    }
}

fn color_fn(space: &str, c: Rgba8, v: &[String; 3]) -> String {
    if c.a == 255 {
        format!("color({space} {} {} {})", v[0], v[1], v[2])
    } else {
        format!("color({space} {} {} {} / {})", v[0], v[1], v[2], alpha(c.a))
    }
}

/// Fills a user template. Tokens: `%Re %Gr %Bl %Al` (0..255), `%Rf %Gf %Bf %Af` (0..1), `%Hex`
/// (`3a7bd5`), `%HEX` (`3A7BD5`), `%HexA` (`3a7bd580`, alpha last), `%Hue %Sat %Lig` (HSL, whole
/// numbers), `%Hsb` (HSB brightness), `%OkL %OkC %OkH` (OKLCH, 3 decimals) and `%%` for a percent
/// sign. An unknown `%x` stays as typed. Longer tokens win (`%Hexa` is not `%Hex` + `a`).
pub fn custom_format(c: Rgba8, template: &str) -> String {
    let hsl = to_hsl(c);
    let hsv = to_hsv(c);
    let ok = to_oklch(c);
    let whole = |v: f64| num(v, 0);
    let tokens: [(&str, String); 20] = [
        (
            "%HexA",
            format!("{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a),
        ),
        ("%Hex", format!("{:02x}{:02x}{:02x}", c.r, c.g, c.b)),
        ("%HEX", format!("{:02X}{:02X}{:02X}", c.r, c.g, c.b)),
        ("%Re", c.r.to_string()),
        ("%Gr", c.g.to_string()),
        ("%Bl", c.b.to_string()),
        ("%Al", c.a.to_string()),
        ("%Rf", float_of(c.r, 0)),
        ("%Gf", float_of(c.g, 0)),
        ("%Bf", float_of(c.b, 0)),
        ("%Af", float_of(c.a, 0)),
        ("%Hue", whole(hsl.h)),
        ("%Sat", whole(hsl.s * 100.0)),
        ("%Lig", whole(hsl.l * 100.0)),
        ("%Hsb", whole(hsv.v * 100.0)),
        ("%OkL", num(ok.l, 3)),
        ("%OkC", num(ok.c, 3)),
        ("%OkH", num(ok.h, 3)),
        ("%%", "%".to_owned()),
        ("%Ar", u32::from_be_bytes([c.a, c.r, c.g, c.b]).to_string()),
    ];
    let mut out = String::new();
    let mut rest = template;
    'outer: while !rest.is_empty() {
        if rest.starts_with('%') {
            for (tok, val) in &tokens {
                if let Some(after) = rest.strip_prefix(tok) {
                    out.push_str(val);
                    rest = after;
                    continue 'outer;
                }
            }
        }
        let mut chars = rest.chars();
        if let Some(ch) = chars.next() {
            out.push(ch);
        }
        rest = chars.as_str();
    }
    out
}
