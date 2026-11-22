//! Colour schemes: HSL hue rotation and OKLCH tints and shades.

use crate::model::{Hsl, Oklch, Rgba8, from_hsl, from_oklch, to_hsl, to_oklch};
use serde::{Deserialize, Serialize};

/// A colour scheme kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scheme {
    /// The opposite hue.
    #[default]
    Complementary,
    /// Hues 30 degrees either side.
    Analogous,
    /// Three hues 120 degrees apart.
    Triadic,
    /// The two hues next to the complement.
    SplitComplementary,
    /// Four hues 90 degrees apart (square).
    Tetradic,
    /// Four tints then four shades.
    TintsShades,
}

impl Scheme {
    /// Every scheme in display order.
    pub const ALL: [Self; 6] = [
        Self::Complementary,
        Self::Analogous,
        Self::Triadic,
        Self::SplitComplementary,
        Self::Tetradic,
        Self::TintsShades,
    ];

    fn rotations(self) -> &'static [f64] {
        match self {
            Self::Complementary => &[180.0],
            Self::Analogous => &[-30.0, 30.0],
            Self::Triadic => &[120.0, 240.0],
            Self::SplitComplementary => &[150.0, 210.0],
            Self::Tetradic => &[90.0, 180.0, 270.0],
            Self::TintsShades => &[],
        }
    }
}

/// Whether `c` has a hue to rotate. Greys, white and black do not: every hue scheme of them would
/// be the same colour repeated.
pub fn has_hue(c: Rgba8) -> bool {
    to_oklch(c).c >= 0.02
}

/// The scheme's colours: `c` first, then the others in the spec's order (tints k=1..4 then shades
/// k=1..4 for [`Scheme::TintsShades`]). Hue schemes rotate the HSL hue and keep saturation and
/// lightness (as `PowerToys` and Adobe do), so a pale or dark colour gives a pale or dark partner and
/// never white or black. A colour without a hue ([`has_hue`]) gives only itself for a hue scheme.
/// Tints exist only while there is room above the base lightness and shades only below it, so a
/// near-white colour has no tints and a near-black one no shades (fewer swatches, never wrong
/// ones). Alpha is kept.
pub fn scheme(c: Rgba8, s: Scheme) -> Vec<Rgba8> {
    let mut out = vec![c];
    if s == Scheme::TintsShades {
        let base = to_oklch(c);
        let at = |l: f64| from_oklch(Oklch { l, ..base }, c.a);
        if base.l < LIGHT_END - 0.04 {
            out.extend((1..=4).map(|k| at(base.l + (LIGHT_END - base.l) * f64::from(k) / 4.0)));
        }
        if base.l > DARK_END + 0.04 {
            out.extend((1..=4).map(|k| at(base.l - (base.l - DARK_END) * f64::from(k) / 4.0)));
        }
        return out;
    }
    if !has_hue(c) {
        return out;
    }
    let hsl = to_hsl(c);
    out.extend(s.rotations().iter().map(|d| {
        from_hsl(
            Hsl {
                h: (hsl.h + d).rem_euclid(360.0),
                ..hsl
            },
            c.a,
        )
    }));
    out
}

const LIGHT_END: f64 = 0.97;
const DARK_END: f64 = 0.20;
