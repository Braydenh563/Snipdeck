//! Parsing colour text: lenient for the input fields, strict for card swatches.

use crate::css4;
use crate::model::{
    Cmyk, Hsl, Hsv, Oklch, Rgba8, from_cmyk, from_hsl, from_hsv, from_oklch_rounded, unit_to_u8,
};
use crate::names::named;
use unicode_normalization::UnicodeNormalization as _;

/// Why [`parse`] rejected a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Nothing but whitespace.
    Empty,
    /// Letters that are not a CSS keyword.
    UnknownName,
    /// A hex form with the wrong number of digits or a non-hex digit.
    BadHex,
    /// A malformed function: unknown name, wrong argument count, a bad number or unit, no `)`.
    BadFunction,
    /// Three bare integers (`58, 123, 300`) outside 0..=255. Function arguments are clamped instead.
    OutOfRange,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty colour",
            Self::UnknownName => "unknown colour name",
            Self::BadHex => "bad hex colour",
            Self::BadFunction => "bad colour function",
            Self::OutOfRange => "colour value out of range",
        })
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Copy, PartialEq)]
enum Unit {
    None,
    Percent,
    /// An angle, already converted to degrees.
    Angle,
}

/// One argument: `none`, a number, a percentage or an angle with a unit.
fn token(t: &str) -> Result<(f64, Unit), ParseError> {
    let bad = ParseError::BadFunction;
    if t == "none" {
        return Ok((0.0, Unit::None));
    }
    let (body, unit, scale) = if let Some(b) = t.strip_suffix('%') {
        (b, Unit::Percent, 1.0)
    } else if let Some(b) = t.strip_suffix("deg") {
        (b, Unit::Angle, 1.0)
    } else if let Some(b) = t.strip_suffix("grad") {
        (b, Unit::Angle, 0.9)
    } else if let Some(b) = t.strip_suffix("turn") {
        (b, Unit::Angle, 360.0)
    } else if let Some(b) = t.strip_suffix("rad") {
        (b, Unit::Angle, 180.0 / std::f64::consts::PI)
    } else {
        (t, Unit::None, 1.0)
    };
    let v: f64 = body.parse().map_err(|_| bad)?;
    if !v.is_finite() {
        return Err(bad);
    }
    Ok((v * scale, unit))
}

/// A 0..1 fraction from a percentage or a plain 0..1 number, clamped.
fn fraction((v, u): (f64, Unit)) -> Result<f64, ParseError> {
    match u {
        Unit::Percent => Ok((v / 100.0).clamp(0.0, 1.0)),
        Unit::None => Ok(v.clamp(0.0, 1.0)),
        Unit::Angle => Err(ParseError::BadFunction),
    }
}

/// A percentage where a plain number also means percent (HSL/HSB legacy form), as 0..1.
fn percent_like((v, u): (f64, Unit)) -> Result<f64, ParseError> {
    match u {
        Unit::Percent | Unit::None => Ok((v / 100.0).clamp(0.0, 1.0)),
        Unit::Angle => Err(ParseError::BadFunction),
    }
}

fn hue((v, u): (f64, Unit)) -> Result<f64, ParseError> {
    match u {
        Unit::Percent => Err(ParseError::BadFunction),
        _ => Ok(v.rem_euclid(360.0)),
    }
}

fn alpha(t: Option<&(f64, Unit)>) -> Result<u8, ParseError> {
    t.map_or(Ok(255), |&t| fraction(t).map(unit_to_u8))
}

fn rgb_channel((v, u): (f64, Unit)) -> Result<u8, ParseError> {
    match u {
        Unit::Percent => Ok(unit_to_u8((v / 100.0).clamp(0.0, 1.0))),
        Unit::None => Ok(unit_to_u8((v / 255.0).clamp(0.0, 1.0))),
        Unit::Angle => Err(ParseError::BadFunction),
    }
}

/// `Color.FromArgb(a, r, g, b)` / `(r, g, b)`, `UIColor(red: 0.1, green: ...)`, `Color(red: ...)` and
/// `Color(0xAARRGGBB)` as code writes them. `None` when `name` is none of these.
fn code_function(name: &str, args: &str) -> Option<Result<Rgba8, ParseError>> {
    let bad = Err(ParseError::BadFunction);
    match name {
        "color.fromargb" | "system.drawing.color.fromargb" => {
            let n: Vec<u8> = args
                .split(',')
                .map(|p| p.trim().parse::<u8>().ok())
                .collect::<Option<_>>()
                .unwrap_or_default();
            Some(match n[..] {
                [r, g, b] => Ok(Rgba8::rgb(r, g, b)),
                [a, r, g, b] => Ok(Rgba8 { r, g, b, a }),
                _ => bad,
            })
        }
        "uicolor" | "color" | "nscolor" if args.contains(':') => {
            let (mut c, mut got) = ([0.0, 0.0, 0.0, 1.0], [false; 3]);
            for part in args.split(',') {
                let (k, v) = part.split_once(':')?;
                let Ok(v) = v.trim().parse::<f64>() else {
                    return Some(bad);
                };
                match k.trim() {
                    "red" | "srgbred" => (c[0], got[0]) = (v, true),
                    "green" | "srgbgreen" => (c[1], got[1]) = (v, true),
                    "blue" | "srgbblue" => (c[2], got[2]) = (v, true),
                    "alpha" | "opacity" => c[3] = v,
                    _ => return Some(bad),
                }
            }
            Some(if got == [true; 3] {
                Ok(Rgba8 {
                    r: unit_to_u8(c[0]),
                    g: unit_to_u8(c[1]),
                    b: unit_to_u8(c[2]),
                    a: unit_to_u8(c[3]),
                })
            } else {
                bad
            })
        }
        "color" if args.trim().starts_with("0x") => {
            Some(argb_digits(args.trim().trim_start_matches("0x")))
        }
        _ => None,
    }
}

/// The CSS `color(<space> r g b [/ a])` function for `srgb`, `srgb-linear` and `display-p3`.
fn color_function(args: &str) -> Result<Rgba8, ParseError> {
    let mut words = args
        .split(|c: char| c == '/' || c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty());
    let space = words.next().ok_or(ParseError::BadFunction)?;
    let toks = words.map(token).collect::<Result<Vec<_>, _>>()?;
    if toks.len() != 3 && toks.len() != 4 {
        return Err(ParseError::BadFunction);
    }
    let a = alpha(toks.get(3))?;
    let mut ch = [0.0; 3];
    for (slot, &t) in ch.iter_mut().zip(&toks) {
        *slot = fraction(t)?;
    }
    Ok(match space {
        "srgb" => Rgba8 {
            r: unit_to_u8(ch[0]),
            g: unit_to_u8(ch[1]),
            b: unit_to_u8(ch[2]),
            a,
        },
        "srgb-linear" => crate::model::from_linear(ch, a),
        "display-p3" => css4::from_p3_linear(ch.map(css4::degamma), a),
        _ => return Err(ParseError::BadFunction),
    })
}

/// `AARRGGBB` (8 digits) or `RRGGBB` (6): the form of `0x` literals in code.
fn argb_digits(d: &str) -> Result<Rgba8, ParseError> {
    match d.len() {
        6 => hex_digits(d),
        8 => {
            let c = hex_digits(d)?;
            // `hex_digits` read RRGGBBAA; the digits were AARRGGBB.
            Ok(Rgba8 {
                r: c.g,
                g: c.b,
                b: c.a,
                a: c.r,
            })
        }
        _ => Err(ParseError::BadHex),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "one match arm per colour function; each is short"
)]
fn function(name: &str, args: &str) -> Result<Rgba8, ParseError> {
    if let Some(r) = code_function(name, args) {
        return r;
    }
    if name == "color" {
        return color_function(args);
    }
    if args.contains(['(', ')']) {
        return Err(ParseError::BadFunction);
    }
    let toks = args
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(token)
        .collect::<Result<Vec<_>, _>>()?;
    let count_ok = |n: usize| toks.len() == n || toks.len() == n + 1;
    match name {
        "rgb" | "rgba" if count_ok(3) => Ok(Rgba8 {
            r: rgb_channel(toks[0])?,
            g: rgb_channel(toks[1])?,
            b: rgb_channel(toks[2])?,
            a: alpha(toks.get(3))?,
        }),
        "hsl" | "hsla" if count_ok(3) => {
            let v = Hsl {
                h: hue(toks[0])?,
                s: percent_like(toks[1])?,
                l: percent_like(toks[2])?,
            };
            Ok(from_hsl(v, alpha(toks.get(3))?))
        }
        "hsb" | "hsv" | "hsba" | "hsva" if count_ok(3) => {
            let v = Hsv {
                h: hue(toks[0])?,
                s: percent_like(toks[1])?,
                v: percent_like(toks[2])?,
            };
            Ok(from_hsv(v, alpha(toks.get(3))?))
        }
        "cmyk" if count_ok(4) => {
            // Bare numbers are one set: fractions 0..1, unless one is above 1, which can only be
            // a percentage (design tools write `cmyk(50, 20, 0, 10)`). `%` is always percent.
            let bare_percent = toks[..4].iter().any(|&(v, u)| u == Unit::None && v > 1.0);
            let part = |t: (f64, Unit)| match t {
                (v, Unit::None) if bare_percent => percent_like((v, Unit::None)),
                t => fraction(t),
            };
            let v = Cmyk {
                c: part(toks[0])?,
                m: part(toks[1])?,
                y: part(toks[2])?,
                k: part(toks[3])?,
            };
            Ok(from_cmyk(v, alpha(toks.get(4))?))
        }
        "hwb" if count_ok(3) => {
            let (w, b) = (percent_like(toks[1])?, percent_like(toks[2])?);
            let sum = w + b;
            let (w, b) = if sum > 1.0 {
                (w / sum, b / sum)
            } else {
                (w, b)
            };
            let pure = from_hsl(
                Hsl {
                    h: hue(toks[0])?,
                    s: 1.0,
                    l: 0.5,
                },
                255,
            );
            let mix = |ch: u8| unit_to_u8(f64::from(ch) / 255.0 * (1.0 - w - b) + w);
            Ok(Rgba8 {
                r: mix(pure.r),
                g: mix(pure.g),
                b: mix(pure.b),
                a: alpha(toks.get(3))?,
            })
        }
        "lab" if count_ok(3) => {
            let l = match toks[0] {
                (v, Unit::Percent | Unit::None) => v.clamp(0.0, 100.0),
                _ => return Err(ParseError::BadFunction),
            };
            let ab = |t: (f64, Unit)| match t {
                (v, Unit::Percent) => Ok(v / 100.0 * 125.0),
                (v, Unit::None) => Ok(v),
                _ => Err(ParseError::BadFunction),
            };
            Ok(css4::from_lab(
                l,
                ab(toks[1])?,
                ab(toks[2])?,
                alpha(toks.get(3))?,
            ))
        }
        "lch" if count_ok(3) => {
            let l = match toks[0] {
                (v, Unit::Percent | Unit::None) => v.clamp(0.0, 100.0),
                _ => return Err(ParseError::BadFunction),
            };
            let chroma = match toks[1] {
                (v, Unit::Percent) => v / 100.0 * 150.0,
                (v, Unit::None) => v,
                _ => return Err(ParseError::BadFunction),
            }
            .max(0.0);
            let h = hue(toks[2])?.to_radians();
            Ok(css4::from_lab(
                l,
                chroma * h.cos(),
                chroma * h.sin(),
                alpha(toks.get(3))?,
            ))
        }
        "oklab" if count_ok(3) => {
            let ab = |t: (f64, Unit)| match t {
                (v, Unit::Percent) => Ok(v / 100.0 * 0.4),
                (v, Unit::None) => Ok(v),
                _ => Err(ParseError::BadFunction),
            };
            Ok(css4::from_oklab(
                fraction(toks[0])?,
                ab(toks[1])?,
                ab(toks[2])?,
                alpha(toks.get(3))?,
            ))
        }
        "oklch" if count_ok(3) => {
            let chroma = match toks[1] {
                (v, Unit::Percent) => v / 100.0 * 0.4,
                (v, Unit::None) => v,
                (_, Unit::Angle) => return Err(ParseError::BadFunction),
            };
            let v = Oklch {
                l: fraction(toks[0])?,
                c: chroma.max(0.0),
                h: hue(toks[2])?,
            };
            Ok(from_oklch_rounded(v, alpha(toks.get(3))?))
        }
        _ => Err(ParseError::BadFunction),
    }
}

fn hex_digits(d: &str) -> Result<Rgba8, ParseError> {
    if !d.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ParseError::BadHex);
    }
    let nib = |i: usize| u8::from_str_radix(&d[i..=i], 16).unwrap_or(0);
    let byte = |i: usize| nib(i) * 16 + nib(i + 1);
    match d.len() {
        3 | 4 => Ok(Rgba8 {
            r: nib(0) * 17,
            g: nib(1) * 17,
            b: nib(2) * 17,
            a: if d.len() == 4 { nib(3) * 17 } else { 255 },
        }),
        6 | 8 => Ok(Rgba8 {
            r: byte(0),
            g: byte(2),
            b: byte(4),
            a: if d.len() == 8 { byte(6) } else { 255 },
        }),
        _ => Err(ParseError::BadHex),
    }
}

fn bare_triple(text: &str) -> Result<Rgba8, ParseError> {
    let parts: Vec<&str> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    if parts.len() != 3 && parts.len() != 4 {
        return Err(ParseError::BadFunction);
    }
    let a = match parts.get(3) {
        Some(p) => alpha(Some(&token(p)?))?,
        None => 255,
    };
    let mut out = [0u8; 3];
    for (slot, p) in out.iter_mut().zip(&parts) {
        *slot = p
            .parse::<u32>()
            .ok()
            .and_then(|v| u8::try_from(v).ok())
            .ok_or(ParseError::OutOfRange)?;
    }
    Ok(Rgba8 {
        r: out[0],
        g: out[1],
        b: out[2],
        a,
    })
}

/// Parses user-typed colour text (lenient; see the crate docs for the accepted forms).
///
/// # Errors
/// A [`ParseError`] naming what was wrong.
pub fn parse(text: &str) -> Result<Rgba8, ParseError> {
    let t = fold(text);
    if t.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(d) = t.strip_prefix('#') {
        return hex_digits(d);
    }
    if let Some(open) = t.find('(') {
        let inner = t[open + 1..]
            .strip_suffix(')')
            .ok_or(ParseError::BadFunction)?;
        return function(t[..open].trim(), inner);
    }
    if t == "transparent" {
        return Ok(Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
    }
    if let Some(c) = named(&t) {
        return Ok(c);
    }
    if let Some(d) = t.strip_prefix("0x") {
        return argb_digits(d);
    }
    if t.bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b',' | b'.' | b'%') || b.is_ascii_whitespace())
        && t.contains(|c: char| c == ',' || c.is_whitespace())
    {
        return bare_triple(&t);
    }
    if t.bytes().all(|b| b.is_ascii_digit()) {
        // A decimal integer as .NET and Android print it: 7 digits are 0xRRGGBB (at most 16777215),
        // 9 or 10 digits are 0xAARRGGBB. Other lengths are hex digits, tried below.
        if let (7 | 9 | 10, Ok(n)) = (t.len(), t.parse::<u32>()) {
            if t.len() == 7 && n <= 0x00ff_ffff {
                return argb_digits(&format!("{n:06x}"));
            }
            if t.len() >= 9 {
                return argb_digits(&format!("{n:08x}"));
            }
        }
    }
    if t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hex_digits(&t);
    }
    Err(ParseError::UnknownName)
}

/// NFKC-fold, trim and lower-case colour text, so full-width `＃ｆｆ００００` and a no-break space
/// from an IME or a web page parse like their ASCII forms.
fn fold(text: &str) -> String {
    text.nfkc().collect::<String>().trim().to_lowercase()
}

/// A number typed with `.` or a decimal comma (`50,5` in de, es, fr, pt-BR), after NFKC folding.
pub fn decimal(text: &str) -> Option<f64> {
    let t = fold(text);
    let t = if t.matches(',').count() == 1 && !t.contains('.') {
        t.replace(',', ".")
    } else {
        t.replace(',', "")
    };
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// [`parse`] for the live input field, where every keystroke is parsed. It refuses what ordinary
/// words and numbers would turn into by accident: 3 and 4 digit hex needs the `#` (`bad`, `ace`,
/// `fed` are words), and a bare run of digits is never a colour (`100`, `255`, `16711680`). Six or
/// eight hex digits with at least one letter, `0x`, `#`, functions and keywords are accepted.
///
/// # Errors
/// A [`ParseError`] naming what was wrong.
pub fn parse_live(text: &str) -> Result<Rgba8, ParseError> {
    let t = fold(text);
    let bare = !t.is_empty() && t.bytes().all(|b| b.is_ascii_hexdigit());
    if bare && (!matches!(t.len(), 6 | 8) || t.bytes().all(|b| b.is_ascii_digit())) {
        return Err(ParseError::BadHex);
    }
    parse(&t)
}

const FUNCS: [&str; 15] = [
    "rgba", "rgb", "hsla", "hsl", "hsba", "hsb", "hsv", "hwb", "lab", "lch", "oklab", "oklch",
    "cmyk", "color", "uicolor",
];

/// Finds the first colour literal in pasted text (`background: #ff0000;`, `"rgb(1, 2, 3)"`) and
/// returns it with the byte range of `text` that was used. A text that is a colour as a whole is
/// returned whole (so keywords and bare hex still work); otherwise `#` hex, `0x` hex and functions
/// are scanned left to right.
pub fn extract(text: &str) -> Option<(Rgba8, std::ops::Range<usize>)> {
    if let Ok(c) = parse(text) {
        let start = text.len() - text.trim_start().len();
        return Some((c, start..text.trim_end().len()));
    }
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &lower[i..];
        let prev_word = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'.');
        if !prev_word {
            if rest.starts_with('#') {
                let n = rest.as_bytes()[1..]
                    .iter()
                    .take_while(|b| b.is_ascii_hexdigit())
                    .count();
                let word_goes_on = bytes.get(i + 1 + n).is_some_and(u8::is_ascii_alphanumeric);
                if matches!(n, 3 | 4 | 6 | 8)
                    && !word_goes_on
                    && let Ok(c) = hex_digits(&rest[1..=n])
                {
                    return Some((c, i..i + 1 + n));
                }
            } else if let Some(d) = rest.strip_prefix("0x") {
                let n = d.bytes().take_while(u8::is_ascii_hexdigit).count();
                if matches!(n, 6 | 8)
                    && let Ok(c) = argb_digits(&d[..n])
                {
                    return Some((c, i..i + 2 + n));
                }
            } else if FUNCS.iter().any(|f| rest.starts_with(f))
                && let Some(close) = rest.find(')')
                && let Ok(c) = parse(&rest[..=close])
            {
                return Some((c, i..i + close + 1));
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Strict parsing for card swatches: the whole trimmed text (at most 64 bytes) must be `#` hex with
/// 3, 4, 6 or 8 digits, or an `rgb[a]()`, `hsl[a]()`, `hsb[a]()`, `hwb()`, `cmyk()`, `lab()`, `lch()`,
/// `oklab()` or `oklch()` function. Bare keywords and hex
/// without `#` give `None`.
pub fn literal(text: &str) -> Option<Rgba8> {
    let t = text.trim();
    if t.len() > 64 {
        return None;
    }
    if t.starts_with('#') {
        return parse(t).ok();
    }
    let open = t.find('(')?;
    let name = t[..open].trim().to_ascii_lowercase();
    if !matches!(
        name.as_str(),
        "rgb"
            | "rgba"
            | "hsl"
            | "hsla"
            | "hsb"
            | "hsba"
            | "hwb"
            | "cmyk"
            | "lab"
            | "lch"
            | "oklab"
            | "oklch"
    ) {
        return None;
    }
    parse(t).ok()
}
