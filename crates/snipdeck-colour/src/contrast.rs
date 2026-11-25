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

/// `fg` composited over `bg` (both straight alpha); the result is opaque when `bg` is.
pub fn composite(fg: Rgba8, bg: Rgba8) -> Rgba8 {
    let fa = f64::from(fg.a) / 255.0;
    let ba = f64::from(bg.a) / 255.0;
    let out_a = fa + ba * (1.0 - fa);
    if out_a <= 0.0 {
        return Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }
    let mix = |f: u8, b: u8| {
        let v = (f64::from(f) * fa + f64::from(b) * ba * (1.0 - fa)) / out_a;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "value is within 0..=255"
        )]
        let byte = v.round().clamp(0.0, 255.0) as u8;
        byte
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is within 0..=255"
    )]
    let a = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgba8 {
        r: mix(fg.r, bg.r),
        g: mix(fg.g, bg.g),
        b: mix(fg.b, bg.b),
        a,
    }
}

/// Whether the ratio of `fg` and `bg` depends on a backdrop (either has alpha below 255).
pub const fn needs_backdrop(fg: Rgba8, bg: Rgba8) -> bool {
    fg.a < 255 || bg.a < 255
}

/// The contrast ratio when `bg` sits over the opaque `backdrop` (white or black in the UI) and `fg`
/// over that. With two opaque colours the backdrop does not matter.
pub fn contrast_over(fg: Rgba8, bg: Rgba8, backdrop: Rgba8) -> f64 {
    let backdrop = Rgba8 { a: 255, ..backdrop };
    contrast_ratio(fg, composite(bg, backdrop))
}

/// APCA lightness contrast `Lc` (APCA-W3 0.0.98G-4g) of text `fg` on `bg`, both treated as opaque:
/// about -108..=106, positive for dark text on a light background.
pub fn apca(fg: Rgba8, bg: Rgba8) -> f64 {
    fn y(c: Rgba8) -> f64 {
        let ch = |v: u8| (f64::from(v) / 255.0).powf(2.4);
        let y = 0.212_672_9 * ch(c.r) + 0.715_152_2 * ch(c.g) + 0.072_175 * ch(c.b);
        if y < 0.022 {
            y + (0.022 - y).powf(1.414)
        } else {
            y
        }
    }
    let (yt, yb) = (y(fg), y(bg));
    if (yb - yt).abs() < 0.000_5 {
        return 0.0;
    }
    let out = if yb > yt {
        let s = (yb.powf(0.56) - yt.powf(0.57)) * 1.14;
        if s < 0.1 { 0.0 } else { s - 0.027 }
    } else {
        let s = (yb.powf(0.65) - yt.powf(0.62)) * 1.14;
        if s > -0.1 { 0.0 } else { s + 0.027 }
    };
    out * 100.0
}

/// The colour nearest to `fg` (same hue and chroma, lightness moved the least) whose ratio over the
/// opaque `bg` reaches `target`, or `None` when no lightness can (`bg` is mid-grey and `target`
/// high). `fg` itself when it already passes.
pub fn nearest_passing(fg: Rgba8, bg: Rgba8, target: f64) -> Option<Rgba8> {
    use crate::model::{Oklch, from_oklch, to_oklch};
    let ok = |c: Rgba8| contrast_ratio(c, bg) >= target;
    if ok(Rgba8 { a: 255, ..fg }) {
        return Some(Rgba8 { a: 255, ..fg });
    }
    let base = to_oklch(fg);
    for step in 1..=200 {
        let d = f64::from(step) * 0.005;
        let mut best: Option<Rgba8> = None;
        for l in [base.l - d, base.l + d] {
            if (0.0..=1.0).contains(&l) {
                let c = from_oklch(Oklch { l, ..base }, 255);
                if ok(c) && best.is_none() {
                    best = Some(c);
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}
