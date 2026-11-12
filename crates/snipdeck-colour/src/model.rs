//! The colour value types and the conversions between sRGB, HSL, HSV, CMYK and OKLCH.
#![allow(
    clippy::unreadable_literal,
    reason = "Ottosson's published matrix coefficients, kept verbatim"
)]
#![allow(
    clippy::many_single_char_names,
    reason = "r/g/b, l/a/b and h/s/v are the conventional names"
)]

/// An 8-bit sRGB colour with alpha; the canonical stored value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba8 {
    /// Red, 0..=255.
    pub r: u8,
    /// Green, 0..=255.
    pub g: u8,
    /// Blue, 0..=255.
    pub b: u8,
    /// Alpha, 0 (transparent) ..= 255 (opaque).
    pub a: u8,
}

impl Rgba8 {
    /// An opaque colour (`a` = 255).
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Lower-case hex: `#3a7bd5`, or `#3a7bd580` when alpha is below 255.
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Exactly `#rrggbb` or `#rrggbbaa` (either case), as `settings.json` stores colours.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 && digits.len() != 8 {
            return None;
        }
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        let a = if digits.len() == 8 { byte(6)? } else { 255 };
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a,
        })
    }
}

/// Hue-saturation-lightness; `h` 0..360, `s` and `l` 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    /// Hue in degrees, 0..360.
    pub h: f64,
    /// Saturation, 0..1.
    pub s: f64,
    /// Lightness, 0..1.
    pub l: f64,
}

/// Hue-saturation-value (also called HSB); `h` 0..360, `s` and `v` 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    /// Hue in degrees, 0..360.
    pub h: f64,
    /// Saturation, 0..1.
    pub s: f64,
    /// Value (brightness), 0..1.
    pub v: f64,
}

/// Naive device-independent CMYK, each component 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cmyk {
    /// Cyan, 0..1.
    pub c: f64,
    /// Magenta, 0..1.
    pub m: f64,
    /// Yellow, 0..1.
    pub y: f64,
    /// Key (black), 0..1.
    pub k: f64,
}

/// OKLCH: `l` 0..1, chroma `c` (about 0..0.4), hue `h` in degrees 0..360.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    /// Perceptual lightness, 0..1.
    pub l: f64,
    /// Chroma, 0 for greys.
    pub c: f64,
    /// Hue in degrees, 0..360.
    pub h: f64,
}

/// Rounds a 0..1 value to an 8-bit channel, clamped.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=255 before the cast"
)]
pub(crate) fn unit_to_u8(x: f64) -> u8 {
    (x * 255.0).round().clamp(0.0, 255.0) as u8
}

fn channel(v: u8) -> f64 {
    f64::from(v) / 255.0
}

fn decode(c: u8) -> f64 {
    let c = channel(c);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn encode(l: f64) -> f64 {
    let l = l.clamp(0.0, 1.0);
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// Linear-light sRGB channels of `c`.
pub(crate) fn linear(c: Rgba8) -> [f64; 3] {
    [decode(c.r), decode(c.g), decode(c.b)]
}

fn near_zero(x: f64) -> bool {
    x.abs() < 1e-12
}

fn hue_of(r: f64, g: f64, b: f64, max: f64, d: f64) -> f64 {
    let h = if (max - r).abs() < 1e-12 {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-12 {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    h.rem_euclid(360.0)
}

fn from_hue(h: f64, chroma: f64, m: f64, a: u8) -> Rgba8 {
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp {
        p if p < 1.0 => (chroma, x, 0.0),
        p if p < 2.0 => (x, chroma, 0.0),
        p if p < 3.0 => (0.0, chroma, x),
        p if p < 4.0 => (0.0, x, chroma),
        p if p < 5.0 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    Rgba8 {
        r: unit_to_u8(r + m),
        g: unit_to_u8(g + m),
        b: unit_to_u8(b + m),
        a,
    }
}

/// sRGB to HSL (alpha ignored).
pub fn to_hsl(c: Rgba8) -> Hsl {
    let (r, g, b) = (channel(c.r), channel(c.g), channel(c.b));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let l = f64::midpoint(max, min);
    if near_zero(d) {
        return Hsl { h: 0.0, s: 0.0, l };
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    Hsl {
        h: hue_of(r, g, b, max, d),
        s,
        l,
    }
}

/// HSL to sRGB with the given alpha; inputs are clamped.
pub fn from_hsl(v: Hsl, a: u8) -> Rgba8 {
    let (s, l) = (v.s.clamp(0.0, 1.0), v.l.clamp(0.0, 1.0));
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    from_hue(v.h, chroma, l - chroma / 2.0, a)
}

/// sRGB to HSV (alpha ignored).
pub fn to_hsv(c: Rgba8) -> Hsv {
    let (r, g, b) = (channel(c.r), channel(c.g), channel(c.b));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    if near_zero(d) {
        return Hsv {
            h: 0.0,
            s: 0.0,
            v: max,
        };
    }
    Hsv {
        h: hue_of(r, g, b, max, d),
        s: d / max,
        v: max,
    }
}

/// HSV to sRGB with the given alpha; inputs are clamped.
pub fn from_hsv(v: Hsv, a: u8) -> Rgba8 {
    let (s, val) = (v.s.clamp(0.0, 1.0), v.v.clamp(0.0, 1.0));
    let chroma = val * s;
    from_hue(v.h, chroma, val - chroma, a)
}

/// sRGB to naive CMYK (alpha ignored).
pub fn to_cmyk(c: Rgba8) -> Cmyk {
    let (r, g, b) = (channel(c.r), channel(c.g), channel(c.b));
    let k = 1.0 - r.max(g).max(b);
    if near_zero(1.0 - k) {
        return Cmyk {
            c: 0.0,
            m: 0.0,
            y: 0.0,
            k: 1.0,
        };
    }
    let f = |x: f64| (1.0 - x - k) / (1.0 - k);
    Cmyk {
        c: f(r),
        m: f(g),
        y: f(b),
        k,
    }
}

/// CMYK to sRGB with the given alpha; inputs are clamped.
pub fn from_cmyk(v: Cmyk, a: u8) -> Rgba8 {
    let k = v.k.clamp(0.0, 1.0);
    let f = |x: f64| unit_to_u8((1.0 - x.clamp(0.0, 1.0)) * (1.0 - k));
    Rgba8 {
        r: f(v.c),
        g: f(v.m),
        b: f(v.y),
        a,
    }
}

/// `OKLab` `(L, a, b)` of an sRGB colour (Ottosson's published matrices).
pub(crate) fn to_oklab(c: Rgba8) -> (f64, f64, f64) {
    let [r, g, b] = linear(c);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    (
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    )
}

fn oklab_to_linear(l: f64, a: f64, b: f64) -> [f64; 3] {
    let lp = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let mp = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let sp = (l - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
    [
        4.076_741_662_1 * lp - 3.307_711_591_3 * mp + 0.230_969_929_2 * sp,
        -1.268_438_004_6 * lp + 2.609_757_401_1 * mp - 0.341_319_396_5 * sp,
        -0.004_196_086_3 * lp - 0.703_418_614_7 * mp + 1.707_614_701_0 * sp,
    ]
}

/// sRGB to OKLCH (alpha ignored).
pub fn to_oklch(c: Rgba8) -> Oklch {
    let (l, a, b) = to_oklab(c);
    Oklch {
        l,
        c: a.hypot(b),
        h: b.atan2(a).to_degrees().rem_euclid(360.0),
    }
}

fn linear_of(v: Oklch, chroma: f64) -> [f64; 3] {
    let h = v.h.to_radians();
    oklab_to_linear(v.l, chroma * h.cos(), chroma * h.sin())
}

fn in_gamut(rgb: [f64; 3], low: f64, high: f64) -> bool {
    rgb.iter().all(|&x| (-low..=1.0 + high).contains(&x))
}

/// OKLCH to sRGB with the given alpha. A colour outside the sRGB gamut keeps its lightness and hue
/// and loses chroma (40 bisection steps), then channels are clamped, encoded and rounded.
pub fn from_oklch(v: Oklch, a: u8) -> Rgba8 {
    oklch_to_rgba(v, a, 1e-6, 1e-6)
}

/// [`from_oklch`] for text that was rounded to 4 decimals of chroma and 2 of lightness. Rounding can
/// push a colour that sits on the gamut edge a hair outside it, and mapping that by chroma reduction
/// would shift the other channels by tens of levels (`oklch(45.71% 0.3098 264.13)` is `#000fff`), so
/// overshoots up to 2e-4 (low end) and 2e-3 (high end) in linear light are clamped instead. Those
/// slacks are under half an 8-bit level once encoded.
pub(crate) fn from_oklch_rounded(v: Oklch, a: u8) -> Rgba8 {
    oklch_to_rgba(v, a, 2e-4, 2e-3)
}

fn oklch_to_rgba(v: Oklch, a: u8, low: f64, high: f64) -> Rgba8 {
    let v = Oklch {
        l: v.l.clamp(0.0, 1.0),
        c: v.c.max(0.0),
        h: v.h,
    };
    let mut rgb = linear_of(v, v.c);
    if !in_gamut(rgb, low, high) {
        let (mut lo, mut hi) = (0.0_f64, v.c);
        for _ in 0..40 {
            let mid = f64::midpoint(lo, hi);
            if in_gamut(linear_of(v, mid), low, high) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        rgb = linear_of(v, lo);
    }
    Rgba8 {
        r: unit_to_u8(encode(rgb[0])),
        g: unit_to_u8(encode(rgb[1])),
        b: unit_to_u8(encode(rgb[2])),
        a,
    }
}
