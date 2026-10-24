//! The user's accent colour (DESIGN.md §2, "User accent choices").
//!
//! Blue is the shipped palette and needs no override. Each preset takes the same steps blue does
//! from `design/tokens/palette.json`; "Windows accent" derives its steps from the system colour
//! and nudges them until the contrast rules hold in both themes.

use serde::{Deserialize, Serialize};

use crate::palette::primitive;

/// An sRGB colour.
pub type Rgb = [u8; 3];

/// Which accent the user picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccentChoice {
    /// The Index Blue default.
    #[default]
    Blue,
    /// Teal.
    Teal,
    /// Green.
    Green,
    /// Amber.
    Amber,
    /// Rose.
    Rose,
    /// Violet.
    Violet,
    /// The colour Windows uses for its own accent.
    Windows,
}

impl AccentChoice {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [Self; 7] = [
        Self::Blue,
        Self::Teal,
        Self::Green,
        Self::Amber,
        Self::Rose,
        Self::Violet,
        Self::Windows,
    ];

    fn hue(self) -> Option<&'static str> {
        match self {
            Self::Teal => Some("teal"),
            Self::Green => Some("green"),
            Self::Amber => Some("amber"),
            Self::Rose => Some("rose"),
            Self::Violet => Some("violet"),
            Self::Blue | Self::Windows => None,
        }
    }
}

/// The accent tokens of one theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Steps {
    /// `accent.default`.
    pub default: Rgb,
    /// `accent.hover`.
    pub hover: Rgb,
    /// `accent.pressed`.
    pub pressed: Rgb,
    /// `accent.text`.
    pub text: Rgb,
    /// `accent.subtle` (also the selected row).
    pub subtle: Rgb,
    /// `focus.ring`.
    pub focus: Rgb,
}

/// An accent for both themes. `None` from [`resolve`] means the built-in blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    /// Steps on the light theme.
    pub light: Steps,
    /// Steps on the dark theme.
    pub dark: Steps,
}

/// Light surface and the text drawn on an accent fill.
pub const LIGHT_SURFACE: Rgb = [0xFB, 0xFA, 0xF7];
/// Dark surface.
pub const DARK_SURFACE: Rgb = [0x17, 0x16, 0x13];
/// Text on an accent fill (`text.on-accent`).
pub const ON_ACCENT: Rgb = LIGHT_SURFACE;

/// The accent to draw, or `None` to keep the built-in blue. `system` is the Windows accent, when known.
#[must_use]
pub fn resolve(choice: AccentChoice, system: Option<Rgb>) -> Option<Accent> {
    if choice == AccentChoice::Windows {
        return system.map(from_system);
    }
    let hue = choice.hue()?;
    let step = |n: u16| primitive(&format!("{hue}-{n}"));
    Some(Accent {
        light: Steps {
            default: step(600)?,
            hover: step(700)?,
            pressed: step(800)?,
            text: step(700)?,
            subtle: step(100)?,
            focus: step(600)?,
        },
        dark: Steps {
            default: step(600)?,
            hover: step(700)?,
            pressed: step(800)?,
            text: step(300)?,
            subtle: step(900)?,
            focus: step(400)?,
        },
    })
}

/// WCAG relative luminance.
#[must_use]
pub fn luminance(c: Rgb) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG contrast ratio, 1 to 21.
#[must_use]
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `a` moved `t` (0 to 1) of the way to `b`.
fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let m = |i: usize| {
        let v = f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to a byte"
        )]
        let byte = v.round().clamp(0.0, 255.0) as u8;
        byte
    };
    [m(0), m(1), m(2)]
}

/// Moves `c` toward `to` in small steps until it reaches `ratio` against every colour in `against`.
fn nudge(c: Rgb, to: Rgb, against: &[Rgb], ratio: f64) -> Rgb {
    (0..=100)
        .map(|i| mix(c, to, f64::from(i) / 100.0))
        .find(|candidate| against.iter().all(|bg| contrast(*candidate, *bg) >= ratio))
        .unwrap_or(to)
}

const BLACK: Rgb = [0, 0, 0];
const WHITE: Rgb = [255, 255, 255];

/// Steps for the Windows accent colour, nudged to meet the contrast rules in both themes.
fn from_system(system: Rgb) -> Accent {
    // Light: the fill carries `text.on-accent` at 4.5:1, so it also clears 3:1 as a graphic.
    let default = nudge(system, BLACK, &[ON_ACCENT], 4.5);
    let hover = mix(default, BLACK, 0.18);
    let pressed = mix(default, BLACK, 0.36);
    let light_subtle = mix(LIGHT_SURFACE, default, 0.10);
    let light_text = nudge(hover, BLACK, &[LIGHT_SURFACE, light_subtle], 4.5);
    // Dark: text and ring lighten until they read on the dark surface and the tinted fill.
    // The dark fill must still show against the dark surface (3:1) and carry `text.on-accent` (3:1).
    let dark_default = nudge(default, WHITE, &[DARK_SURFACE], 3.0);
    let dark_subtle = mix(DARK_SURFACE, dark_default, 0.35);
    let dark_hover = mix(dark_default, BLACK, 0.18);
    let dark_pressed = mix(dark_default, BLACK, 0.36);
    let dark_text = nudge(system, WHITE, &[DARK_SURFACE, dark_subtle], 4.5);
    let dark_focus = nudge(system, WHITE, &[DARK_SURFACE], 3.0);
    Accent {
        light: Steps {
            default,
            hover,
            pressed,
            text: light_text,
            subtle: light_subtle,
            focus: default,
        },
        dark: Steps {
            default: dark_default,
            hover: dark_hover,
            pressed: dark_pressed,
            text: dark_text,
            subtle: dark_subtle,
            focus: dark_focus,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(steps: &Steps, surface: Rgb, label: &str) {
        assert!(
            contrast(steps.default, surface) >= 3.0,
            "{label}: fill vs surface"
        );
        assert!(
            contrast(steps.focus, surface) >= 3.0,
            "{label}: focus ring vs surface"
        );
        assert!(
            contrast(ON_ACCENT, steps.default) >= 3.0,
            "{label}: text on accent"
        );
        assert!(
            contrast(steps.text, surface) >= 4.5,
            "{label}: accent text vs surface"
        );
        assert!(
            contrast(steps.text, steps.subtle) >= 4.5,
            "{label}: accent text on selected"
        );
    }

    #[test]
    fn blue_keeps_the_built_in_palette() {
        assert_eq!(resolve(AccentChoice::Blue, Some([1, 2, 3])), None);
        assert_eq!(resolve(AccentChoice::Windows, None), None);
    }

    #[test]
    fn every_preset_meets_contrast_in_both_themes() {
        for choice in AccentChoice::ALL {
            let Some(accent) = resolve(choice, Some([0x00, 0x78, 0xD4])) else {
                continue;
            };
            check(&accent.light, LIGHT_SURFACE, &format!("{choice:?} light"));
            check(&accent.dark, DARK_SURFACE, &format!("{choice:?} dark"));
        }
    }

    #[test]
    fn presets_use_the_palette_steps() {
        let teal = resolve(AccentChoice::Teal, None).expect("teal");
        assert_eq!(teal.light.default, primitive("teal-600").expect("teal-600"));
        assert_eq!(teal.dark.subtle, primitive("teal-900").expect("teal-900"));
        assert_eq!(teal.dark.focus, primitive("teal-400").expect("teal-400"));
    }

    #[test]
    fn any_windows_accent_is_nudged_until_it_passes() {
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let accent = resolve(AccentChoice::Windows, Some([r, g, b])).expect("accent");
                    let label = format!("windows {r},{g},{b}");
                    check(&accent.light, LIGHT_SURFACE, &format!("{label} light"));
                    check(&accent.dark, DARK_SURFACE, &format!("{label} dark"));
                }
            }
        }
    }

    #[test]
    fn the_choice_is_stored_as_a_word() {
        assert_eq!(
            serde_json::to_string(&AccentChoice::Windows).expect("json"),
            "\"windows\""
        );
    }
}
