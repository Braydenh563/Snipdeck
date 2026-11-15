//! WCAG 2.x relative luminance and contrast ratio.

use crate::model::{Rgba8, linear};

/// WCAG relative luminance (alpha ignored).
pub fn relative_luminance(c: Rgba8) -> f64 {
    let [r, g, b] = linear(c);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Contrast ratio 1..=21 of `fg` over `bg`. `fg` is composited over `bg` first (in sRGB, as a browser
/// does); `bg` is treated as opaque.
pub fn contrast_ratio(fg: Rgba8, bg: Rgba8) -> f64 {
    let a = f64::from(fg.a) / 255.0;
    let mix = |f: u8, b: u8| {
        let v = f64::from(f) * a + f64::from(b) * (1.0 - a);
        // Round like the 8-bit result a browser would paint.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "value is within 0..=255"
        )]
        let byte = v.round().clamp(0.0, 255.0) as u8;
        byte
    };
    let top = Rgba8::rgb(mix(fg.r, bg.r), mix(fg.g, bg.g), mix(fg.b, bg.b));
    let (l1, l2) = (relative_luminance(top), relative_luminance(bg));
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// The WCAG pass marks for a ratio, compared unrounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent WCAG thresholds"
)]
pub struct Marks {
    /// AA normal text, 4.5.
    pub aa: bool,
    /// AA large text, 3.
    pub aa_large: bool,
    /// AAA normal text, 7.
    pub aaa: bool,
    /// AAA large text, 4.5.
    pub aaa_large: bool,
    /// Non-text UI components (1.4.11), 3.
    pub ui: bool,
}

/// The pass marks for `ratio` (never rounded up into a pass).
pub fn marks(ratio: f64) -> Marks {
    Marks {
        aa: ratio >= 4.5,
        aa_large: ratio >= 3.0,
        aaa: ratio >= 7.0,
        aaa_large: ratio >= 4.5,
        ui: ratio >= 3.0,
    }
}

/// The ratio truncated (not rounded) to two decimals: 4.478 is `"4.47"`.
pub fn ratio_text(ratio: f64) -> String {
    let t = (ratio * 100.0 + 1e-9).floor() / 100.0;
    format!("{t:.2}")
}
