//! The spec's section 5.1 vectors for `#3A7BD5` and the extra reference values.
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "test helpers and fixtures: unwrap on known-good inputs, conventional channel names, exact float vectors"
)]

use snipdeck_colour::*;

const BASE: Rgba8 = Rgba8::rgb(0x3a, 0x7b, 0xd5);
const HALF: Rgba8 = Rgba8 { a: 128, ..BASE };

fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
}

fn hex(h: &str) -> Rgba8 {
    Rgba8::from_hex(h).unwrap()
}

fn near(got: Rgba8, want: &str) {
    let w = hex(want);
    let d = |a: u8, b: u8| a.abs_diff(b);
    assert!(
        d(got.r, w.r) <= 1 && d(got.g, w.g) <= 1 && d(got.b, w.b) <= 1 && got.a == w.a,
        "{} vs {want}",
        got.to_hex()
    );
}

#[test]
fn hex_rgb_text() {
    assert_eq!(format(BASE, Format::Hex), "#3a7bd5");
    assert_eq!(format(HALF, Format::Hex), "#3a7bd580");
    assert_eq!(format(BASE, Format::Rgb), "rgb(58, 123, 213)");
    assert_eq!(format(HALF, Format::Rgb), "rgba(58, 123, 213, 0.5)");
}

#[test]
fn hsl_hsb_cmyk() {
    let h = to_hsl(BASE);
    close(h.h, 214.839, 0.001);
    close(h.s, 0.64854, 1e-5);
    close(h.l, 0.53137, 1e-5);
    assert_eq!(format(BASE, Format::Hsl), "hsl(215, 65%, 53%)");
    assert_eq!(format(HALF, Format::Hsl), "hsla(215, 65%, 53%, 0.5)");
    let v = to_hsv(BASE);
    close(v.h, 214.839, 0.001);
    close(v.s, 0.72770, 1e-5);
    close(v.v, 0.83529, 1e-5);
    assert_eq!(format(BASE, Format::Hsb), "hsb(215, 73%, 84%)");
    let c = to_cmyk(BASE);
    close(c.c, 0.72770, 1e-5);
    close(c.m, 0.42254, 1e-5);
    close(c.y, 0.0, 1e-9);
    close(c.k, 0.16471, 1e-5);
    assert_eq!(format(BASE, Format::Cmyk), "cmyk(73%, 42%, 0%, 16%)");
}

#[test]
fn oklch_and_name() {
    let o = to_oklch(BASE);
    close(o.l, 0.586_186, 1e-5);
    close(o.c, 0.153_274, 1e-5);
    close(o.h, 257.2335, 0.005);
    assert_eq!(format(BASE, Format::Oklch), "oklch(58.62% 0.1533 257.23)");
    assert_eq!(
        format(HALF, Format::Oklch),
        "oklch(58.62% 0.1533 257.23 / 0.5)"
    );
    let n = nearest_name(BASE);
    assert_eq!(
        (n.name, n.rgb, n.exact),
        ("royalblue", hex("#4169e1"), false)
    );
    close(n.distance, 0.0515, 0.0005);
    assert_eq!(format(BASE, Format::Name), "royalblue");
}

#[test]
fn luminance_and_contrast() {
    close(relative_luminance(BASE), 0.19870, 1e-5);
    let w = contrast_ratio(BASE, Rgba8::rgb(255, 255, 255));
    close(w, 4.2220, 1e-3);
    assert_eq!(ratio_text(w), "4.22");
    let m = marks(w);
    assert_eq!(
        (m.aa, m.aa_large, m.aaa, m.aaa_large, m.ui),
        (false, true, false, false, true)
    );
    let k = contrast_ratio(BASE, Rgba8::rgb(0, 0, 0));
    close(k, 4.9739, 1e-3);
    assert_eq!(ratio_text(k), "4.97");
    let m = marks(k);
    assert_eq!(
        (m.aa, m.aa_large, m.aaa, m.aaa_large, m.ui),
        (true, true, false, true, true)
    );
}

#[test]
fn schemes() {
    let check = |s: Scheme, want: &[&str]| {
        let got = scheme(BASE, s);
        assert_eq!(got.len(), want.len() + 1, "{s:?}");
        assert_eq!(got[0], BASE);
        for (g, w) in got[1..].iter().zip(want) {
            near(*g, w);
        }
    };
    check(Scheme::Complementary, &["#a47100"]);
    check(Scheme::Analogous, &["#0088b0", "#786ad1"]);
    check(Scheme::Triadic, &["#c64e5a", "#4b9029"]);
    check(Scheme::SplitComplementary, &["#c25914", "#858000"]);
    check(Scheme::Tetradic, &["#ba508d", "#a47100", "#00926d"]);
    check(
        Scheme::TintsShades,
        &[
            "#5799f6", "#88b9ff", "#bcd8ff", "#eff6ff", "#1b5db5", "#00428f", "#002a60", "#001435",
        ],
    );
    let comp = scheme(BASE, Scheme::Complementary)[1];
    close(contrast_ratio(BASE, comp), 1.0068, 2e-3);
}

#[test]
fn extra_reference_values() {
    assert_eq!(
        format(hex("#ff0000"), Format::Oklch),
        "oklch(62.8% 0.2577 29.23)"
    );
    assert_eq!(format(hex("#ffffff"), Format::Oklch), "oklch(100% 0 0)");
    assert_eq!(format(hex("#ffffff"), Format::Hsl), "hsl(0, 0%, 100%)");
    assert_eq!(format(hex("#808080"), Format::Oklch), "oklch(59.99% 0 0)");
    assert_eq!(
        format(hex("#000000"), Format::Cmyk),
        "cmyk(0%, 0%, 0%, 100%)"
    );
    let white = Rgba8::rgb(255, 255, 255);
    let r = contrast_ratio(hex("#777777"), white);
    close(r, 4.4781, 1e-3);
    assert_eq!(ratio_text(r), "4.47");
    assert!(!marks(r).aa);
    let r = contrast_ratio(hex("#767676"), white);
    close(r, 4.5422, 1e-3);
    assert!(marks(r).aa);
    assert!(nearest_name(hex("#4169e1")).exact);
}

#[test]
fn wcag_extremes() {
    let (b, w) = (Rgba8::rgb(0, 0, 0), Rgba8::rgb(255, 255, 255));
    close(contrast_ratio(b, w), 21.0, 1e-9);
    close(contrast_ratio(w, b), 21.0, 1e-9);
    close(contrast_ratio(BASE, BASE), 1.0, 1e-12);
    assert_eq!(ratio_text(21.0), "21.00");
    assert_eq!(ratio_text(4.5), "4.50");
    assert!(marks(21.0).aaa && marks(3.0).ui && !marks(2.999_999).ui);
}

#[test]
fn alpha_is_composited_for_contrast() {
    let half_black = Rgba8 {
        r: 0,
        g: 0,
        b: 0,
        a: 128,
    };
    let r = contrast_ratio(half_black, Rgba8::rgb(255, 255, 255));
    assert!(r > 3.5 && r < 5.0, "{r}");
}

#[test]
fn oklch_reference_greys_and_gamut() {
    let o = to_oklch(hex("#ffffff"));
    close(o.l, 1.0, 1e-6);
    assert!(o.c < 1e-4);
    // An out-of-gamut request keeps lightness (+-0.01) and lands inside sRGB.
    let c = from_oklch(
        Oklch {
            l: 0.7,
            c: 0.4,
            h: 150.0,
        },
        255,
    );
    close(to_oklch(c).l, 0.7, 0.01);
}
