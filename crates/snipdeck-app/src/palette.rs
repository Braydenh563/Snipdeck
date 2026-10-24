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

#[cfg(test)]
mod tests {
    use super::*;

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
}
