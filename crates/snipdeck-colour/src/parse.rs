//! Parsing colour text: lenient for the input fields, strict for card swatches.

use crate::model::{Cmyk, Hsl, Hsv, Oklch, Rgba8, from_cmyk, from_hsl, from_hsv, from_oklch, unit_to_u8};
use crate::names::named;

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

fn function(name: &str, args: &str) -> Result<Rgba8, ParseError> {
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
            let v = Hsl { h: hue(toks[0])?, s: percent_like(toks[1])?, l: percent_like(toks[2])? };
            Ok(from_hsl(v, alpha(toks.get(3))?))
        }
        "hsb" | "hsv" if count_ok(3) => {
            let v = Hsv { h: hue(toks[0])?, s: percent_like(toks[1])?, v: percent_like(toks[2])? };
            Ok(from_hsv(v, alpha(toks.get(3))?))
        }
        "cmyk" if count_ok(4) => {
            let v = Cmyk { c: fraction(toks[0])?, m: fraction(toks[1])?, y: fraction(toks[2])?, k: fraction(toks[3])? };
            Ok(from_cmyk(v, alpha(toks.get(4))?))
        }
        "oklch" if count_ok(3) => {
            let chroma = match toks[1] {
                (v, Unit::Percent) => v / 100.0 * 0.4,
                (v, Unit::None) => v,
                (_, Unit::Angle) => return Err(ParseError::BadFunction),
            };
            let v = Oklch { l: fraction(toks[0])?, c: chroma.max(0.0), h: hue(toks[2])? };
            Ok(from_oklch(v, alpha(toks.get(3))?))
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
        6 | 8 => Ok(Rgba8 { r: byte(0), g: byte(2), b: byte(4), a: if d.len() == 8 { byte(6) } else { 255 } }),
        _ => Err(ParseError::BadHex),
    }
}

fn bare_triple(text: &str) -> Result<Rgba8, ParseError> {
    let parts: Vec<&str> = text.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
    if parts.len() != 3 {
        return Err(ParseError::BadFunction);
    }
    let mut out = [0u8; 3];
    for (slot, p) in out.iter_mut().zip(&parts) {
        *slot = p.parse::<u32>().ok().and_then(|v| u8::try_from(v).ok()).ok_or(ParseError::OutOfRange)?;
    }
    Ok(Rgba8::rgb(out[0], out[1], out[2]))
}

/// Parses user-typed colour text (lenient; see the crate docs for the accepted forms).
///
/// # Errors
/// A [`ParseError`] naming what was wrong.
pub fn parse(text: &str) -> Result<Rgba8, ParseError> {
    let t = text.trim().to_ascii_lowercase();
    if t.is_empty() {
        return Err(ParseError::Empty);
    }
    if let Some(d) = t.strip_prefix('#') {
        return hex_digits(d);
    }
    if let Some(open) = t.find('(') {
        let inner = t[open + 1..].strip_suffix(')').ok_or(ParseError::BadFunction)?;
        return function(t[..open].trim(), inner);
    }
    if t == "transparent" {
        return Ok(Rgba8 { r: 0, g: 0, b: 0, a: 0 });
    }
    if let Some(c) = named(&t) {
        return Ok(c);
    }
    if let Some(d) = t.strip_prefix("0x") {
        return if d.len() == 6 { hex_digits(d) } else { Err(ParseError::BadHex) };
    }
    if t.bytes().all(|b| b.is_ascii_digit() || b == b',' || b.is_ascii_whitespace())
        && t.contains(|c: char| c == ',' || c.is_whitespace())
    {
        return bare_triple(&t);
    }
    if t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hex_digits(&t);
    }
    Err(ParseError::UnknownName)
}

/// Strict parsing for card swatches: the whole trimmed text (at most 64 bytes) must be `#` hex with
/// 3, 4, 6 or 8 digits, or an `rgb[a]()`, `hsl[a]()` or `oklch()` function. Bare keywords and hex
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
    if !matches!(name.as_str(), "rgb" | "rgba" | "hsl" | "hsla" | "oklch") {
        return None;
    }
    parse(t).ok()
}
