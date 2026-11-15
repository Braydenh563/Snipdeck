//! Parsing, strict literals, names, formats and serde.
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
fn spec_parse_vectors() {
    let base = hex("#3a7bd5");
    for t in [
        "3A7BD5",
        "0x3a7bd5",
        "58, 123, 213",
        "58 123 213",
        "#3A7BD5",
        "  #3a7bd5\n",
        "rgb(58,123,213)",
    ] {
        assert_eq!(parse(t), Ok(base), "{t}");
    }
    assert_eq!(parse("rgb(58 123 213 / 50%)"), Ok(hex("#3a7bd580")));
    assert_eq!(parse("RGBA(58, 123, 213, 0.5)"), Ok(hex("#3a7bd580")));
    assert_eq!(parse("hsl(215deg 65% 53%)"), Ok(hex("#397ad5")));
    assert_eq!(parse("hsb(215, 73%, 84%)"), Ok(hex("#3a7bd6")));
    assert_eq!(parse("hsv(215 73% 84%)"), Ok(hex("#3a7bd6")));
    assert_eq!(parse("oklch(58.62% 0.1533 257.23)"), Ok(base));
    assert_eq!(parse("oklch(0.5862 0.1533 257.23)"), Ok(base));
    assert_eq!(parse("RoyalBlue"), Ok(hex("#4169e1")));
    assert_eq!(literal("royalblue"), None);
    assert_eq!(parse("#3ad"), Ok(hex("#33aadd")));
    assert_eq!(parse("#12345"), Err(ParseError::BadHex));
    assert_eq!(parse("rgb(300,0,0)"), Ok(hex("#ff0000")));
    assert_eq!(parse("blurple"), Err(ParseError::UnknownName));
}

#[test]
fn more_forms_and_errors() {
    assert_eq!(parse(""), Err(ParseError::Empty));
    assert_eq!(parse("   "), Err(ParseError::Empty));
    assert_eq!(parse("#abcd"), Ok(hex("#aabbccdd")));
    assert_eq!(
        parse("transparent"),
        Ok(Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 0
        })
    );
    assert_eq!(parse("rgb(100%, 0%, 0%)"), Ok(hex("#ff0000")));
    assert_eq!(parse("rgb(0 0 0 / 0)"), Ok(hex("#00000000")));
    assert_eq!(parse("hsl(0.5turn 100% 50%)"), Ok(hex("#00ffff")));
    assert_eq!(parse("hsl(180deg, 100%, 50%)"), Ok(hex("#00ffff")));
    assert_eq!(parse("hsl(200grad 100% 50%)"), Ok(hex("#00ffff")));
    assert_eq!(parse("hsl(3.14159265rad 100% 50%)"), Ok(hex("#00ffff")));
    assert_eq!(parse("hsl(-180 100% 50%)"), Ok(hex("#00ffff")));
    assert_eq!(parse("cmyk(0%, 100%, 100%, 0%)"), Ok(hex("#ff0000")));
    assert_eq!(parse("cmyk(0 1 1 0)"), Ok(hex("#ff0000")));
    assert_eq!(parse("oklch(100% 0 none)"), Ok(hex("#ffffff")));
    assert_eq!(parse("0x3a7b"), Err(ParseError::BadHex));
    assert_eq!(parse("58, 123, 300"), Err(ParseError::OutOfRange));
    assert_eq!(parse("58, 123"), Err(ParseError::BadFunction));
    assert_eq!(parse("rgb(1, 2)"), Err(ParseError::BadFunction));
    assert_eq!(parse("rgb(1, 2, 3"), Err(ParseError::BadFunction));
    assert_eq!(parse("rgb(a, b, c)"), Err(ParseError::BadFunction));
    assert_eq!(parse("rgb(1, 2, nan)"), Err(ParseError::BadFunction));
    assert_eq!(parse("foo(1, 2, 3)"), Err(ParseError::BadFunction));
    assert_eq!(parse("#12g456"), Err(ParseError::BadHex));
    assert_eq!(parse("rgb(1,2,3) rgb(4,5,6)"), Err(ParseError::BadFunction));
}

#[test]
fn strict_literals() {
    assert_eq!(literal("#ffcc00"), Some(hex("#ffcc00")));
    assert_eq!(literal(" #FC0 "), Some(hex("#ffcc00")));
    assert_eq!(literal("#ffcc0080"), Some(hex("#ffcc0080")));
    assert_eq!(literal("rgb(255, 204, 0)"), Some(hex("#ffcc00")));
    assert_eq!(literal("rgba(255 204 0 / 50%)"), Some(hex("#ffcc0080")));
    assert_eq!(literal("hsl(48, 100%, 50%)"), Some(hex("#ffcc00")));
    assert!(literal("oklch(58.62% 0.1533 257.23)").is_some());
    for no in [
        "red",
        "ffcc00",
        "0xffcc00",
        "58, 123, 213",
        "hsb(1, 2%, 3%)",
        "cmyk(0,0,0,0)",
        "#ffcc0",
        "color: #fff",
        "",
        "transparent",
        "rgb(1,2,3) and more",
    ] {
        assert_eq!(literal(no), None, "{no}");
    }
    let long = format!("#fff{}", " ".repeat(70));
    assert_eq!(literal(&format!("{long}x")), None);
    assert_eq!(literal(&format!("rgb(1,2,3{})", " ".repeat(70))), None);
}

#[test]
fn hex_helpers() {
    assert_eq!(
        Rgba8::from_hex("#3a7bd5"),
        Some(Rgba8::rgb(0x3a, 0x7b, 0xd5))
    );
    assert_eq!(Rgba8::from_hex("#3A7BD580").unwrap().a, 0x80);
    for bad in ["3a7bd5", "#3a7bd", "#3a7bd5f", "#fff", "#gggggg", "", "#"] {
        assert_eq!(Rgba8::from_hex(bad), None, "{bad}");
    }
}

#[test]
fn keyword_table() {
    for (name, expect) in [
        ("red", "#ff0000"),
        ("REBECCAPURPLE", "#663399"),
        ("grey", "#808080"),
    ] {
        assert_eq!(named(name), Some(hex(expect)));
    }
    assert_eq!(named("transparent"), None);
    assert_eq!(named("nope"), None);
    // Ties go to the alphabetically first keyword.
    assert_eq!(nearest_name(hex("#00ffff")).name, "aqua");
    assert_eq!(nearest_name(hex("#ff00ff")).name, "fuchsia");
    assert_eq!(nearest_name(hex("#808080")).name, "gray");
    assert_eq!(nearest_name(hex("#a9a9a9")).name, "darkgray");
    let n = nearest_name(hex("#4169e1"));
    assert!(n.exact && n.distance == 0.0);
    // Alpha is ignored.
    assert!(nearest_name(hex("#4169e180")).exact);
}

#[test]
fn every_keyword_is_its_own_nearest() {
    let mut count = 0;
    let mut last = "";
    for name in ALL_NAMES {
        assert!(last < name, "table not sorted at {name}");
        last = name;
        let c = named(name).unwrap();
        let n = nearest_name(c);
        assert!(n.exact, "{name}");
        assert_eq!(n.rgb, c);
        count += 1;
    }
    assert_eq!(count, 148);
}

// The names in the same order as the crate's table, recorded independently to catch drift.
const ALL_NAMES: [&str; 148] = [
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

#[test]
fn format_metadata_and_serde() {
    assert_eq!(Format::ALL.len(), 7);
    assert!(Format::ALL[..6].iter().all(|f| f.can_be_default()));
    assert!(!Format::Name.can_be_default());
    let labels: Vec<_> = Format::ALL[..6].iter().map(|f| f.label()).collect();
    assert_eq!(labels, ["HEX", "RGB", "HSL", "HSB", "CMYK", "OKLCH"]);
    assert_eq!(serde_json::to_string(&Format::Oklch).unwrap(), "\"oklch\"");
    assert_eq!(
        serde_json::from_str::<Format>("\"hsb\"").unwrap(),
        Format::Hsb
    );
    assert!(serde_json::from_str::<Format>("\"nope\"").is_err());
    assert_eq!(
        serde_json::to_string(&Scheme::SplitComplementary).unwrap(),
        "\"split-complementary\""
    );
    assert_eq!(
        serde_json::from_str::<Scheme>("\"tints-shades\"").unwrap(),
        Scheme::TintsShades
    );
    assert_eq!(
        (Format::default(), Scheme::default()),
        (Format::Hex, Scheme::Complementary)
    );
    assert_eq!(Scheme::ALL.len(), 6);
    assert_eq!(
        serde_json::to_string(&PickerStyle::Wheel).unwrap(),
        "\"wheel\""
    );
    assert_eq!(
        serde_json::from_str::<InputModel>("\"cmyk\"").unwrap(),
        InputModel::Cmyk
    );
    assert_eq!(
        (PickerStyle::default(), InputModel::default()),
        (PickerStyle::Square, InputModel::Hex)
    );
}

#[test]
fn conversions_round_trip_on_a_grid() {
    let mut worst_oklch = 0u8;
    for r in (0..=255).step_by(15) {
        for g in (0..=255).step_by(15) {
            for b in (0..=255).step_by(15) {
                let c = Rgba8::rgb(r, g, b);
                assert_eq!(parse(&format(c, Format::Hex)), Ok(c));
                assert_eq!(parse(&format(c, Format::Rgb)), Ok(c));
                assert_eq!(from_hsl(to_hsl(c), 255), c);
                assert_eq!(from_hsv(to_hsv(c), 255), c);
                assert_eq!(from_cmyk(to_cmyk(c), 255), c);
                assert_eq!(from_oklch(to_oklch(c), 255), c);
                let back = parse(&format(c, Format::Oklch)).unwrap();
                let d = back
                    .r
                    .abs_diff(r)
                    .max(back.g.abs_diff(g))
                    .max(back.b.abs_diff(b));
                worst_oklch = worst_oklch.max(d);
            }
        }
    }
    assert!(worst_oklch <= 1, "oklch text drifted by {worst_oklch}");
}

#[test]
fn alpha_round_trips_in_hex_and_nearly_in_rgb() {
    for a in 0..=255u8 {
        let c = Rgba8 {
            r: 10,
            g: 200,
            b: 99,
            a,
        };
        assert_eq!(parse(&format(c, Format::Hex)), Ok(c));
        // rgba() prints alpha to 2 dp, so only some alphas survive; none drift by more than 1.
        let back = parse(&format(c, Format::Rgb)).unwrap();
        assert!(back.a.abs_diff(a) <= 1, "{a}");
    }
}
