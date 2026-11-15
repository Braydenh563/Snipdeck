//! Colour schemes: hue rotation in OKLCH and tints/shades.

use crate::model::{Oklch, Rgba8, from_oklch, to_oklch};
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

/// The scheme's colours: `c` first, then the others in the spec's order (tints k=1..4 then shades
/// k=1..4 for [`Scheme::TintsShades`]). Hue schemes rotate in OKLCH at constant lightness and chroma,
/// reducing chroma when the result leaves the sRGB gamut. Alpha is kept.
pub fn scheme(c: Rgba8, s: Scheme) -> Vec<Rgba8> {
    let base = to_oklch(c);
    let mut out = vec![c];
    if s == Scheme::TintsShades {
        let at = |l: f64| from_oklch(Oklch { l, ..base }, c.a);
        out.extend((1..=4).map(|k| at(base.l + (0.97 - base.l) * f64::from(k) / 4.0)));
        out.extend((1..=4).map(|k| at(base.l - (base.l - 0.20) * f64::from(k) / 4.0)));
        return out;
    }
    let achromatic = base.c < 0.0002;
    out.extend(s.rotations().iter().map(|d| {
        if achromatic {
            c
        } else {
            from_oklch(
                Oklch {
                    h: (base.h + d).rem_euclid(360.0),
                    ..base
                },
                c.a,
            )
        }
    }));
    out
}
