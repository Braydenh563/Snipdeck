//! Property tests over all colours.
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "test helpers and fixtures: unwrap on known-good inputs, conventional channel names, exact float vectors"
)]

use proptest::prelude::*;
use snipdeck_colour::*;

fn any_rgba() -> impl Strategy<Value = Rgba8> {
    (any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b, a)| Rgba8 {
        r,
        g,
        b,
        a,
    })
}

fn opaque() -> impl Strategy<Value = Rgba8> {
    (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| Rgba8::rgb(r, g, b))
}

proptest! {
    #[test]
    fn hex_round_trips(c in any_rgba()) {
        prop_assert_eq!(parse(&format(c, Format::Hex)), Ok(c));
        prop_assert_eq!(Rgba8::from_hex(&c.to_hex()), Some(c));
    }

    #[test]
    fn rgb_round_trips_when_opaque(c in opaque()) {
        prop_assert_eq!(parse(&format(c, Format::Rgb)), Ok(c));
    }

    #[test]
    fn oklch_text_within_one(c in any_rgba()) {
        let back = parse(&format(c, Format::Oklch)).unwrap();
        prop_assert!(back.r.abs_diff(c.r) <= 1 && back.g.abs_diff(c.g) <= 1 && back.b.abs_diff(c.b) <= 1);
        prop_assert!(back.a.abs_diff(c.a) <= 1);
    }

    #[test]
    fn saved_text_is_lossless(c in any_rgba(), i in 0usize..7) {
        prop_assert_eq!(parse(&saved_text(c, Format::ALL[i])), Ok(c));
    }

    #[test]
    fn every_format_text_parses_back_close(c in opaque()) {
        // Lossy integers still land within a few steps (HSL/HSB/CMYK are integer percentages).
        for f in [Format::Hsl, Format::Hsb, Format::Cmyk] {
            let back = parse(&format(c, f)).unwrap();
            prop_assert!(back.r.abs_diff(c.r) <= 4 && back.g.abs_diff(c.g) <= 4 && back.b.abs_diff(c.b) <= 4, "{:?}", f);
        }
    }

    #[test]
    fn contrast_symmetric_and_bounded(a in opaque(), b in opaque()) {
        let r = contrast_ratio(a, b);
        prop_assert!((r - contrast_ratio(b, a)).abs() < 1e-12);
        prop_assert!((1.0..=21.0 + 1e-9).contains(&r));
        prop_assert!(ratio_text(r).len() >= 4);
    }

    #[test]
    fn schemes_are_valid_and_start_with_base(c in any_rgba()) {
        for s in Scheme::ALL {
            let out = scheme(c, s);
            prop_assert_eq!(out[0], c);
            prop_assert!(out.len() >= 2);
            prop_assert!(out.iter().all(|x| x.a == c.a));
        }
    }

    #[test]
    fn oklch_conversion_round_trips(c in opaque()) {
        prop_assert_eq!(from_oklch(to_oklch(c), 255), c);
    }

    #[test]
    fn from_oklch_never_panics(l in -0.5f64..1.5, ch in -0.1f64..0.8, h in -720.0f64..720.0) {
        let c = from_oklch(Oklch { l, c: ch, h }, 255);
        prop_assert_eq!(c.a, 255);
    }

    #[test]
    fn recents_never_exceed_24_or_duplicate(cs in prop::collection::vec(any_rgba(), 0..80)) {
        let mut r = ColourRecents::default();
        for (i, c) in cs.iter().enumerate() {
            r.push(*c, i64::try_from(i).unwrap());
        }
        let list = r.list();
        prop_assert!(list.len() <= ColourRecents::MAX);
        let mut sorted: Vec<_> = list.iter().map(|c| c.to_hex()).collect();
        sorted.sort();
        sorted.dedup();
        prop_assert_eq!(sorted.len(), list.len());
        prop_assert_eq!(ColourRecents::from_saved(&r.to_saved()), r);
    }

    #[test]
    fn parse_never_panics(s in ".{0,80}") {
        let _ = parse(&s);
        let _ = literal(&s);
    }

    #[test]
    fn nearest_name_of_keyword_values(c in opaque()) {
        let n = nearest_name(c);
        prop_assert_eq!(n.exact, n.rgb == c);
        prop_assert!(n.distance >= 0.0);
    }
}
