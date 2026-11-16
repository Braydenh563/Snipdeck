//! List colours for the Panel's tabs and mixed-view labels.
//!
//! The nine list hues live in `design/tokens/palette.json`, the same file `snipdeck-ui` generates
//! its tokens from (DESIGN.md §2: step 600 on light, step 400 on dark). Reading that file keeps one
//! source of truth: the app never spells a hex value.

use std::{collections::HashMap, sync::LazyLock};

use snipdeck_core::ListAccent;

const PALETTE_JSON: &str = include_str!("../../../design/tokens/palette.json");

static PRIMITIVES: LazyLock<HashMap<String, [u8; 3]>> = LazyLock::new(|| {
    let value: serde_json::Value = serde_json::from_str(PALETTE_JSON).unwrap_or_default();
    value
        .get("primitives")
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(name, hex)| Some((name.clone(), parse_hex(hex.as_str()?)?)))
                .collect()
        })
        .unwrap_or_default()
});

fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// A palette primitive by name (`teal-600`).
#[must_use]
pub fn primitive(name: &str) -> Option<[u8; 3]> {
    PRIMITIVES.get(name).copied()
}

/// The colour of a list's accent: step 600 on light, step 400 on dark. A list without an accent
/// is slate.
#[must_use]
pub fn list_color(accent: Option<ListAccent>, dark: bool) -> slint::Color {
    let hue = accent.unwrap_or(ListAccent::Slate).key();
    let step = if dark { 400 } else { 600 };
    let [r, g, b] = PRIMITIVES
        .get(&format!("{hue}-{step}"))
        .copied()
        .unwrap_or([0x7C, 0x7A, 0x76]);
    slint::Color::from_rgb_u8(r, g, b)
}

/// The least contrast a list's colour needs against a sidebar row to tint its icon (WCAG 1.4.11).
pub const ICON_MIN_CONTRAST: f64 = 3.0;

/// The theme's semantic colour (`bg.app`) from the palette file.
fn theme_token(dark: bool, name: &str) -> Option<[u8; 3]> {
    let value: serde_json::Value = serde_json::from_str(PALETTE_JSON).ok()?;
    parse_hex(
        value
            .get("themes")?
            .get(if dark { "dark" } else { "light" })?
            .get(name)?
            .as_str()?,
    )
}

/// `c` when it keeps [`ICON_MIN_CONTRAST`] against every background in `bgs`, otherwise
/// transparent, which a sidebar row reads as "use the default icon colour".
fn tint_over(c: slint::Color, bgs: &[[u8; 3]]) -> slint::Color {
    let rgb = [c.red(), c.green(), c.blue()];
    if !bgs.is_empty()
        && bgs
            .iter()
            .all(|bg| crate::accent::contrast(rgb, *bg) >= ICON_MIN_CONTRAST)
    {
        c
    } else {
        slint::Color::from_argb_u8(0, 0, 0, 0)
    }
}

/// The colour that tints a list's icon in the sidebar: the list colour when it reads against the
/// row at rest, hovered and selected, otherwise transparent (the default icon colour).
#[must_use]
pub fn list_icon_tint(accent: Option<ListAccent>, dark: bool) -> slint::Color {
    let bgs: Vec<[u8; 3]> = ["bg.app", "bg.hover", "bg.selected"]
        .iter()
        .filter_map(|name| theme_token(dark, name))
        .collect();
    tint_over(list_color(accent, dark), &bgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_icon_is_tinted_only_where_it_keeps_three_to_one() {
        let mut tinted = 0;
        for dark in [false, true] {
            for &accent in ListAccent::ALL {
                let tint = list_icon_tint(Some(accent), dark);
                if tint.alpha() == 0 {
                    continue;
                }
                tinted += 1;
                assert_eq!(tint, list_color(Some(accent), dark));
                for name in ["bg.app", "bg.hover", "bg.selected"] {
                    let bg = theme_token(dark, name).expect("token");
                    let rgb = [tint.red(), tint.green(), tint.blue()];
                    assert!(
                        crate::accent::contrast(rgb, bg) >= 3.0,
                        "{accent:?} {dark} {name}"
                    );
                }
            }
        }
        assert!(tinted > 0, "some colours are readable");
    }

    #[test]
    fn a_colour_too_close_to_the_row_falls_back_to_the_default_icon_colour() {
        let row = [[0xF5, 0xF3, 0xF1]];
        let pale = slint::Color::from_rgb_u8(0xF0, 0xEE, 0xEC);
        assert_eq!(tint_over(pale, &row).alpha(), 0);
        let strong = slint::Color::from_rgb_u8(0x19, 0x5A, 0xB3);
        assert_eq!(tint_over(strong, &row), strong);
        assert_eq!(tint_over(strong, &[]).alpha(), 0, "no row known: default");
    }

    #[test]
    fn light_uses_step_600_and_dark_step_400() {
        assert_eq!(
            list_color(Some(ListAccent::Teal), false),
            slint::Color::from_rgb_u8(0x04, 0x84, 0x84)
        );
        assert_eq!(
            list_color(Some(ListAccent::Teal), true),
            slint::Color::from_rgb_u8(0x59, 0xBD, 0xBD)
        );
    }

    #[test]
    fn every_accent_resolves_from_the_palette_file() {
        let fallback = slint::Color::from_rgb_u8(0x7C, 0x7A, 0x76);
        for accent in [
            ListAccent::Blue,
            ListAccent::Teal,
            ListAccent::Green,
            ListAccent::Amber,
            ListAccent::Orange,
            ListAccent::Red,
            ListAccent::Rose,
            ListAccent::Violet,
        ] {
            for dark in [false, true] {
                assert_ne!(
                    list_color(Some(accent), dark),
                    fallback,
                    "{accent:?} {dark}"
                );
            }
        }
    }

    #[test]
    fn no_accent_is_slate() {
        assert_eq!(
            list_color(None, false),
            list_color(Some(ListAccent::Slate), false)
        );
    }

    #[test]
    fn hex_parsing_rejects_junk() {
        assert_eq!(parse_hex("#048484"), Some([4, 0x84, 0x84]));
        assert_eq!(parse_hex("048484"), None);
        assert_eq!(parse_hex("#04848"), None);
        assert_eq!(parse_hex("#04848é"), None);
        assert_eq!(parse_hex("#GG8484"), None);
    }

    // --- Panel v2 tokens (docs/reviews/panel-v2-spec.md section 1/2.9): the checks `palette.py` runs, repeated from
    // the generated JSON so CI fails when a token drifts below its contrast floor.

    fn theme_colour(theme: &str, token: &str) -> [u8; 3] {
        let value: serde_json::Value = serde_json::from_str(PALETTE_JSON).unwrap();
        let hex = value["themes"][theme][token]
            .as_str()
            .unwrap_or_else(|| panic!("{theme} theme has no token `{token}`"));
        parse_hex(hex).unwrap()
    }

    fn luminance([r, g, b]: [u8; 3]) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn contrast(theme: &str, fg: &str, bg: &str) -> f64 {
        let (a, b) = (
            luminance(theme_colour(theme, fg)),
            luminance(theme_colour(theme, bg)),
        );
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn type_tile_glyphs_read_on_their_tile_in_both_themes() {
        for theme in ["light", "dark"] {
            for kind in [
                "text",
                "template",
                "code",
                "link",
                "sensitive",
                "image",
                "file",
            ] {
                let ratio = contrast(
                    theme,
                    &format!("kind.{kind}.glyph"),
                    &format!("kind.{kind}.tile"),
                );
                assert!(ratio >= 4.5, "{theme} {kind} glyph on tile {ratio:.2}");
            }
        }
    }

    #[test]
    fn search_mark_is_readable_and_stays_visible_on_every_row_fill() {
        for theme in ["light", "dark"] {
            let text = contrast(theme, "search.mark-text", "search.mark");
            assert!(text >= 4.5, "{theme} mark text {text:.2}");
            for fill in ["bg.selected", "bg.surface", "bg.hover"] {
                let ratio = contrast(theme, "search.mark", fill);
                assert!(ratio >= 1.3, "{theme} mark vs {fill} {ratio:.2}");
            }
        }
    }

    #[test]
    fn toast_action_reads_on_the_ink_fill() {
        for theme in ["light", "dark"] {
            let ratio = contrast(theme, "toast.action", "ink.default");
            assert!(ratio >= 4.5, "{theme} toast action {ratio:.2}");
        }
    }

    /// The backgrounds a control or a label can sit on.
    const SURFACES: [&str; 7] = [
        "bg.app",
        "bg.surface",
        "bg.sunken",
        "bg.hover",
        "bg.selected",
        "bg.overlay",
        "bg.overlay-hover",
    ];

    /// UX-32: every token drawn as TEXT reaches 4.5:1 on every surface. `text.tertiary` is for glyphs,
    /// dots and disabled labels (1.4.3 exempts disabled text), never for text that has to be read.
    #[test]
    fn every_text_token_reads_on_every_surface_in_both_themes() {
        for theme in ["light", "dark"] {
            for token in [
                "text.primary",
                "text.secondary",
                "accent.text",
                "danger.fg",
                "success.fg",
                "warning.fg",
                "info.fg",
            ] {
                for bg in SURFACES {
                    let ratio = contrast(theme, token, bg);
                    assert!(ratio >= 4.5, "{theme} {token} on {bg} is {ratio:.2}:1");
                }
            }
        }
    }

    /// UX-33: every token drawn as a boundary, a glyph or a focus ring reaches 3:1 on every surface it
    /// can be drawn on. The pairs in `NOT_DRAWN` do not occur: a selected row and a hovered overlay row
    /// draw their accent as `accent.text` (`Theme.accent-on-selected`) and their danger as `danger.fg`.
    #[test]
    fn every_non_text_token_keeps_three_to_one_where_it_is_drawn() {
        const NOT_DRAWN: [(&str, &str, &str); 5] = [
            ("dark", "accent.default", "bg.selected"),
            ("dark", "accent.default", "bg.overlay-hover"),
            ("dark", "danger.solid", "bg.selected"),
            ("dark", "danger.solid", "bg.overlay-hover"),
            ("dark", "border.strong", "bg.selected"),
        ];
        for theme in ["light", "dark"] {
            for token in [
                "accent.default",
                "danger.solid",
                "border.strong",
                "focus.ring",
            ] {
                for bg in SURFACES {
                    if NOT_DRAWN.contains(&(theme, token, bg)) {
                        continue;
                    }
                    let ratio = contrast(theme, token, bg);
                    assert!(ratio >= 3.0, "{theme} {token} on {bg} is {ratio:.2}:1");
                }
            }
        }
    }
}
