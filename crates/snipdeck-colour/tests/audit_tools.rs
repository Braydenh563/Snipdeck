//! Deep-audit regressions (TOOLS-1..15, 31): one precision rule for every format, schemes that
//! never go white or black, backdrop-aware contrast, and the extra parse and copy forms.
#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "test fixtures"
)]

use snipdeck_colour::*;

fn hex(h: &str) -> Rgba8 {
    Rgba8::from_hex(h).unwrap()
}

/// A fixed spread of colours: a 13-step grid on each channel plus the awkward corners.
fn sample() -> Vec<Rgba8> {
    let steps: Vec<u8> = (0..=12).map(|i| (i * 255 / 12) as u8).collect();
    let mut v = Vec::new();
    for &r in &steps {
        for &g in &steps {
            for &b in &steps {
                v.push(Rgba8::rgb(r, g, b));
            }
        }
    }
    v.extend([
        hex("#3a7bd5"),
        hex("#4169e0"),
        hex("#783cc8"),
        hex("#000fff"),
        hex("#010101"),
    ]);
    v
}

#[test]
fn every_format_round_trips_with_and_without_alpha() {
    for base in sample() {
        for a in [255u8, 128, 1] {
            let c = Rgba8 { a, ..base };
            for f in Format::ALL.into_iter().filter(|f| *f != Format::Name) {
                let t = format(c, f);
                assert_eq!(parse(&t), Ok(c), "{f:?} {t}");
            }
            for f in ExtraFormat::ALL
                .into_iter()
                .filter(|f| *f != ExtraFormat::Floats)
            {
                let t = extra_text(c, f);
                assert_eq!(parse(&t), Ok(c), "{f:?} {t}");
            }
        }
    }
}

#[test]
fn all_256_alpha_values_survive_every_format() {
    let base = Rgba8::rgb(0x33, 0x66, 0x99);
    for a in 0..=255u8 {
        let c = Rgba8 { a, ..base };
        for f in Format::ALL.into_iter().filter(|f| *f != Format::Name) {
            let t = format(c, f);
            assert_eq!(parse(&t).map(|p| p.a), Ok(a), "{f:?} {t}");
        }
    }
}

#[test]
fn hsb_and_cmyk_write_alpha() {
    let c = hex("#33669980");
    assert!(format(c, Format::Hsb).starts_with("hsba("));
    assert!(format(c, Format::Cmyk).contains(" / "));
    assert_eq!(saved_text(c, Format::Hsb), format(c, Format::Hsb));
}

#[test]
fn name_copies_only_an_exact_keyword() {
    assert_eq!(
        copy_text(hex("#4169e1"), Format::Name).as_deref(),
        Some("royalblue")
    );
    assert_eq!(copy_text(hex("#4169e0"), Format::Name), None);
    assert_eq!(copy_text(hex("#4169e180"), Format::Name), None);
}

#[test]
fn schemes_never_collapse_to_white_black_or_the_base() {
    for base in [
        hex("#ffff00"),
        hex("#000028"),
        hex("#3a7bd5"),
        hex("#ff0000"),
        hex("#ffd0d0"),
    ] {
        for s in [
            Scheme::Complementary,
            Scheme::Analogous,
            Scheme::Triadic,
            Scheme::SplitComplementary,
            Scheme::Tetradic,
        ] {
            let out = scheme(base, s);
            assert!(out.len() >= 2, "{base:?} {s:?}");
            for c in &out[1..] {
                assert_ne!(*c, base, "{base:?} {s:?}");
                assert_ne!((c.r, c.g, c.b), (255, 255, 255), "{base:?} {s:?} white");
                assert_ne!((c.r, c.g, c.b), (0, 0, 0), "{base:?} {s:?} black");
            }
        }
    }
    let y = scheme(hex("#ffff00"), Scheme::Complementary);
    assert_eq!(y[1], hex("#0000ff"));
}

#[test]
fn tints_and_shades_go_the_right_way_and_may_be_fewer() {
    let white = scheme(hex("#ffffff"), Scheme::TintsShades);
    assert_eq!(white[0], hex("#ffffff"));
    assert!(white.len() == 5, "white has shades only: {white:?}");
    assert!(white[1..].windows(2).all(|w| w[0].r > w[1].r));
    let black = scheme(hex("#000000"), Scheme::TintsShades);
    assert_eq!(black.len(), 5, "black has tints only");
    assert!(black[1..].windows(2).all(|w| w[0].r < w[1].r));
    assert_eq!(scheme(hex("#3a7bd5"), Scheme::TintsShades).len(), 9);
}

#[test]
fn greys_have_no_hue_to_rotate() {
    assert!(!has_hue(hex("#808080")));
    assert_eq!(
        scheme(hex("#808080"), Scheme::Tetradic),
        vec![hex("#808080")]
    );
    assert!(has_hue(hex("#3a7bd5")));
}

#[test]
fn live_field_rejects_words_and_numbers() {
    for t in ["bad", "ace", "fed", "100", "255", "16711680", "123456"] {
        assert!(parse_live(t).is_err(), "{t}");
    }
    assert_eq!(parse_live("#bad"), Ok(hex("#bbaadd")));
    assert_eq!(parse_live("3a7bd5"), Ok(hex("#3a7bd5")));
    assert_eq!(parse_live("royalblue"), Ok(hex("#4169e1")));
    assert_eq!(parse_live("58, 123, 213"), Ok(hex("#3a7bd5")));
}

#[test]
fn more_input_forms() {
    for t in [
        "hwb(0 0% 0%)",
        "lab(54.29 80.8 69.89)",
        "lch(54.29 106.84 40.85)",
        "oklab(0.628 0.2249 0.1258)",
        "color(srgb 1 0 0)",
        "color(display-p3 0.9175 0.2003 0.1386)",
        "0xffff0000",
        "Color.FromArgb(255, 255, 0, 0)",
        "UIColor(red: 1, green: 0, blue: 0, alpha: 1)",
        "Color(0xFFFF0000)",
    ] {
        let got = parse(t).unwrap_or_else(|e| panic!("{t}: {e}"));
        let d = u32::from(got.r).abs_diff(255) + u32::from(got.g) + u32::from(got.b);
        assert!(d <= 3, "{t} -> {got:?}");
    }
    assert_eq!(parse("255,255,255,0.5"), Ok(hex("#ffffff80")));
    assert_eq!(parse("1671168"), Ok(hex("#198000")));
    assert_eq!(parse("0x80ff0000"), Ok(hex("#ff000080")));
}

#[test]
fn full_width_and_decimal_comma() {
    assert_eq!(parse("＃ｆｆ００００"), Ok(hex("#ff0000")));
    assert_eq!(parse("ｒｇｂ（２５５，０，０）"), Ok(hex("#ff0000")));
    assert_eq!(decimal("50,5"), Some(50.5));
    assert_eq!(decimal("50.5"), Some(50.5));
    assert_eq!(decimal("1,234.5"), Some(1234.5));
}

#[test]
fn extract_finds_the_colour_in_code() {
    for (t, want) in [
        ("#FF0000;", "#ff0000"),
        ("\"#ff0000\"", "#ff0000"),
        ("background: #ff0000", "#ff0000"),
        ("rgb(255,0,0);", "#ff0000"),
        ("#ff0000 #00ff00", "#ff0000"),
        ("color: hsl(120, 100%, 50%); x", "#00ff00"),
    ] {
        let (c, range) = extract(t).unwrap_or_else(|| panic!("{t}"));
        assert_eq!(c, hex(want), "{t}");
        assert!(!t[range].contains(';'), "{t}");
    }
    assert!(extract("no colour here, only words").is_none());
}

#[test]
fn contrast_uses_the_backdrop() {
    let white = hex("#ffffff");
    let clear_black = hex("#00000000");
    let over_white = contrast_over(white, clear_black, white);
    assert!((over_white - 1.0).abs() < 0.01, "{over_white}");
    let over_black = contrast_over(white, clear_black, hex("#000000"));
    assert!(over_black > 20.9, "{over_black}");
    let half = hex("#00000080");
    assert!(needs_backdrop(half, white));
    assert!(!needs_backdrop(hex("#000000"), white));
    let a = contrast_over(half, white, white);
    let b = contrast_over(half, white, hex("#000000"));
    assert!(
        (a - b).abs() < 1e-9,
        "opaque background ignores the backdrop"
    );
}

#[test]
fn nearest_passing_and_apca() {
    let bg = hex("#ffffff");
    let fg = hex("#8a8a8a");
    let p = nearest_passing(fg, bg, 4.5).unwrap();
    assert!(contrast_ratio(p, bg) >= 4.5);
    assert!(to_oklch(p).l > to_oklch(hex("#000000")).l);
    assert_eq!(
        nearest_passing(hex("#000000"), bg, 4.5),
        Some(hex("#000000"))
    );
    assert!(apca(hex("#000000"), bg) > 100.0);
    assert!(apca(bg, hex("#000000")) < -100.0);
    assert_eq!(apca(bg, bg), 0.0);
}

#[test]
fn custom_templates() {
    let c = hex("#3a7bd5");
    assert_eq!(custom_format(c, "%Re,%Gr,%Bl"), "58,123,213");
    assert_eq!(custom_format(c, "0x%HEX%%"), "0x3A7BD5%");
    assert_eq!(custom_format(c, "%Hexa"), "3a7bd5a");
    assert_eq!(custom_format(c, "%HexA"), "3a7bd5ff");
    assert_eq!(custom_format(c, "%Zz"), "%Zz");
    assert_eq!(extra_text(c, ExtraFormat::HexUpper), "#3A7BD5");
    assert_eq!(extra_text(c, ExtraFormat::Compose), "Color(0xFF3A7BD5)");
}

/// The whole sRGB cube for the seven rows (172,961 colours sampled in the audit); run on demand:
/// `cargo test -p snipdeck-colour --test audit_tools -- --ignored`.
#[test]
#[ignore = "slow: every colour of the sRGB cube"]
fn full_cube_round_trips() {
    for r in (0..=255u8).step_by(3) {
        for g in (0..=255u8).step_by(3) {
            for b in 0..=255u8 {
                let c = Rgba8::rgb(r, g, b);
                for f in Format::ALL.into_iter().filter(|f| *f != Format::Name) {
                    let t = format(c, f);
                    assert_eq!(parse(&t), Ok(c), "{f:?} {t}");
                }
            }
        }
    }
}
