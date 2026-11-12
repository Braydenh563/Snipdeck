//! Saved-colour text, CSS variables, recents and the picker surfaces.
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

fn hex(h: &str) -> Rgba8 {
    Rgba8::from_hex(h).unwrap()
}

#[test]
fn saved_text_never_rounds() {
    let c = hex("#3a7bd5");
    assert_eq!(saved_text(c, Format::Hex), "#3a7bd5");
    assert_eq!(saved_text(c, Format::Rgb), "rgb(58, 123, 213)");
    assert_eq!(saved_text(c, Format::Hsl), "#3a7bd5");
    assert_eq!(saved_text(c, Format::Hsb), "#3a7bd5");
    assert_eq!(saved_text(c, Format::Cmyk), "#3a7bd5");
    assert_eq!(saved_text(c, Format::Name), "#3a7bd5");
    assert_eq!(saved_text(hex("#4169e1"), Format::Name), "royalblue");
    // 0.5 alpha is exact (128), 0.51 would not be (129 prints 0.51 and parses to 130).
    assert_eq!(
        saved_text(hex("#3a7bd580"), Format::Rgb),
        "rgba(58, 123, 213, 0.5)"
    );
    assert_eq!(saved_text(hex("#3a7bd581"), Format::Rgb), "#3a7bd581");
    for f in Format::ALL {
        assert_eq!(parse(&saved_text(c, f)), Ok(c), "{f:?}");
    }
    // Pure white and black survive every numeric format.
    assert_eq!(saved_text(hex("#ffffff"), Format::Hsl), "hsl(0, 0%, 100%)");
}

#[test]
fn saved_text_is_always_exact() {
    for r in (0..=255).step_by(17) {
        for g in (0..=255).step_by(34) {
            for b in (0..=255).step_by(51) {
                for a in [255, 128, 129, 0] {
                    let c = Rgba8 { r, g, b, a };
                    for f in Format::ALL {
                        assert_eq!(parse(&saved_text(c, f)), Ok(c));
                    }
                }
            }
        }
    }
}

#[test]
fn titles() {
    assert_eq!(title_for(hex("#ff0000")), "red");
    assert_eq!(title_for(hex("#4169e1")), "royalblue");
    assert_eq!(title_for(hex("#3a7bd5")), "");
    assert_eq!(title_for(hex("#ff000080")), "");
    assert_eq!(title_for(hex("#808080")), "gray");
}

#[test]
fn css_variable_slugs_and_dedup() {
    assert_eq!(css_variables(&[]), ":root {\n}");
    let out = css_variables(&[
        ("Brand primary · buttons on light", "#3a7bd5"),
        ("", "#ff0000"),
        ("", "rgb(255, 0, 0)"),
        ("Brand primary · buttons on light", "#111111"),
        ("brand-primary-buttons-on-light", "#222222"),
        ("  ", "oklch(58.62% 0.1533 257.23)"),
        ("!!!", "not a colour"),
        ("Ünï Café", "#010203"),
    ]);
    assert_eq!(
        out,
        ":root {\n  --brand-primary-buttons-on-light: #3a7bd5;\n  --ff0000: #ff0000;\n  --ff0000-2: rgb(255, 0, 0);\n  \
         --brand-primary-buttons-on-light-2: #111111;\n  --brand-primary-buttons-on-light-3: #222222;\n  \
         --3a7bd5: oklch(58.62% 0.1533 257.23);\n  --colour: not a colour;\n  --ünï-café: #010203;\n}"
    );
}

#[test]
fn recents_cap_dedupe_and_order() {
    let mut r = ColourRecents::default();
    for i in 0..30u8 {
        r.push(Rgba8::rgb(i, 0, 0), i64::from(i));
    }
    assert_eq!(r.list().len(), ColourRecents::MAX);
    assert_eq!(r.list()[0], Rgba8::rgb(29, 0, 0));
    r.push(Rgba8::rgb(10, 0, 0), 99);
    assert_eq!(r.list()[0], Rgba8::rgb(10, 0, 0));
    assert_eq!(r.list().len(), 24);
    let mut seen = r.list();
    seen.dedup();
    assert_eq!(seen.len(), 24);
    r.remove(Rgba8::rgb(10, 0, 0));
    assert_eq!(r.list().len(), 23);
    assert!(!r.list().contains(&Rgba8::rgb(10, 0, 0)));
    // Alpha makes a different colour.
    r.push(hex("#11223380"), 100);
    r.push(hex("#112233"), 101);
    assert_eq!(&r.list()[..2], &[hex("#112233"), hex("#11223380")]);
}

#[test]
fn recents_saved_round_trip_and_cleanup() {
    let saved = vec![
        RecentColour {
            hex: "#3a7bd5".into(),
            used_at: 30,
        },
        RecentColour {
            hex: "garbage".into(),
            used_at: 29,
        },
        RecentColour {
            hex: "#3A7BD5".into(),
            used_at: 28,
        },
        RecentColour {
            hex: "#ff000080".into(),
            used_at: 27,
        },
    ];
    let r = ColourRecents::from_saved(&saved);
    assert_eq!(r.list(), [hex("#3a7bd5"), hex("#ff000080")]);
    assert_eq!(
        r.to_saved(),
        vec![
            RecentColour {
                hex: "#3a7bd5".into(),
                used_at: 30
            },
            RecentColour {
                hex: "#ff000080".into(),
                used_at: 27
            },
        ]
    );
    let many: Vec<_> = (0..40u8)
        .map(|i| RecentColour {
            hex: Rgba8::rgb(i, 1, 1).to_hex(),
            used_at: 0,
        })
        .collect();
    assert_eq!(ColourRecents::from_saved(&many).list().len(), 24);
    let json = serde_json::to_string(&r.to_saved()).unwrap();
    assert_eq!(
        json,
        r##"[{"hex":"#3a7bd5","used_at":30},{"hex":"#ff000080","used_at":27}]"##
    );
    let back: Vec<RecentColour> = serde_json::from_str(&json).unwrap();
    assert_eq!(ColourRecents::from_saved(&back), r);
}

fn px(buf: &[u8], d: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * d + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

fn near_px(got: [u8; 4], want: [u8; 3]) {
    for k in 0..3 {
        assert!(got[k].abs_diff(want[k]) <= 3, "{got:?} vs {want:?}");
    }
    assert_eq!(got[3], 255);
}

#[test]
fn square_pixels() {
    let (w, h) = (101, 100);
    let buf = render_square(0.0, w, h);
    assert_eq!(buf.len(), (w * h * 4) as usize);
    near_px(px(&buf, w, 0, 0), [255, 255, 255]);
    near_px(px(&buf, w, w - 1, 0), [255, 0, 0]);
    near_px(px(&buf, w, 0, h - 1), [0, 0, 0]);
    near_px(px(&buf, w, w - 1, h - 1), [0, 0, 0]);
    assert!(buf.chunks(4).all(|p| p[3] == 255));
    // A non-red hue: the top-right is that hue at full saturation and brightness.
    let green = render_square(120.0, w, h);
    near_px(px(&green, w, w - 1, 0), [0, 255, 0]);
}

#[test]
fn wheel_pixels() {
    let d = 201;
    let buf = render_wheel(100.0, d);
    assert_eq!(buf.len(), (d * d * 4) as usize);
    near_px(px(&buf, d, d / 2, d / 2), [255, 255, 255]);
    near_px(px(&buf, d, d / 2, 0), [255, 0, 0]);
    // Clockwise from the top: 90 degrees is the right rim (yellow-green hue 90 = (128, 255, 0)).
    let right = px(&buf, d, d - 1, d / 2);
    assert!(
        right[0].abs_diff(128) <= 6 && right[1] >= 252 && right[2] <= 3,
        "{right:?}"
    );
    // Corners are outside the disc: fully transparent.
    assert_eq!(px(&buf, d, 0, 0)[3], 0);
    assert_eq!(px(&buf, d, d - 1, d - 1)[3], 0);
    // The rim has an anti-aliased edge: some alpha strictly between 0 and 255.
    let mid = buf.chunks(4).filter(|p| p[3] > 0 && p[3] < 255).count();
    assert!(mid >= 2, "{mid}");
    // Brightness scales the whole wheel.
    let dark = render_wheel(0.0, d);
    near_px(px(&dark, d, d / 2, d / 2), [0, 0, 0]);
}

#[test]
fn wheel_maths_round_trips() {
    let d = 160.0_f32;
    for hi in 0..36 {
        for si in 1..=20 {
            let (h, s) = (hi as f32 * 10.0 + 0.5, si as f32 * 5.0);
            let (x, y) = wheel_point(h, s, d);
            let (h2, s2) = wheel_at(x, y, d).unwrap_or_else(|| panic!("outside at {h} {s}"));
            let dh = (h - h2).abs().min(360.0 - (h - h2).abs());
            assert!(dh < 0.01 && (s - s2).abs() < 0.01, "{h} {s} -> {h2} {s2}");
        }
    }
    // Centre has saturation 0, the top is hue 0, right is 90, bottom 180, left 270.
    assert_eq!(wheel_at(80.0, 80.0, d).unwrap().1, 0.0);
    let (h, _) = wheel_at(80.0, 0.5, d).unwrap();
    assert!(!(1.0..=359.0).contains(&h));
    assert!((wheel_at(159.5, 80.0, d).unwrap().0 - 90.0).abs() < 0.5);
    assert!((wheel_at(80.0, 159.5, d).unwrap().0 - 180.0).abs() < 0.5);
    assert!((wheel_at(0.5, 80.0, d).unwrap().0 - 270.0).abs() < 0.5);
    assert_eq!(wheel_at(0.0, 0.0, d), None);
    assert_eq!(wheel_at(-5.0, 80.0, d), None);
    assert_eq!(wheel_at(1.0, 1.0, 0.0), None);
}

#[test]
fn square_maths() {
    assert_eq!(square_at(0.0, 0.0, 200.0, 100.0), (0.0, 100.0));
    assert_eq!(square_at(200.0, 100.0, 200.0, 100.0), (100.0, 0.0));
    assert_eq!(square_at(50.0, 25.0, 200.0, 100.0), (25.0, 75.0));
    assert_eq!(square_at(-10.0, 500.0, 200.0, 100.0), (0.0, 0.0));
    assert_eq!(square_at(999.0, -3.0, 200.0, 100.0), (100.0, 100.0));
    assert_eq!(square_at(1.0, 1.0, 0.0, 0.0), (0.0, 100.0));
    assert_eq!(square_point(25.0, 75.0, 200.0, 100.0), (50.0, 25.0));
    assert_eq!(square_point(150.0, -5.0, 200.0, 100.0), (200.0, 100.0));
    for s in 0..=10 {
        for b in 0..=10 {
            let (s, b) = (s as f32 * 10.0, b as f32 * 10.0);
            let (x, y) = square_point(s, b, 240.0, 160.0);
            let (s2, b2) = square_at(x, y, 240.0, 160.0);
            assert!((s - s2).abs() < 0.01 && (b - b2).abs() < 0.01);
        }
    }
}
