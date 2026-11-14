//! Logic tests for the symbols crate (icon).
#![allow(clippy::unwrap_used, reason = "test helper outside a #[test] fn")]
use snipdeck_symbols::{Catalog, SymbolId};

fn trash() -> SymbolId {
    SymbolId::Icon("trash".into())
}

fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    (info.width, info.height, buf[..info.buffer_size()].to_vec())
}

#[test]
fn icon_svg_keeps_current_color() {
    let c = Catalog::get();
    let svg = c.icon_svg(&trash()).unwrap();
    assert!(svg.starts_with(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" fill="currentColor">"#
    ));
    assert!(
        svg.contains(r#"<path d="M"#),
        "path data is rebuilt into a <path>"
    );
    assert!(svg.ends_with("</svg>"));
    assert!(c.icon_svg(&SymbolId::Emoji("1f600".into())).is_none());
    assert!(c.icon_svg(&SymbolId::Icon("no-such-icon".into())).is_none());
    // Every icon yields well-formed svg that resvg can parse.
    for s in c
        .symbols()
        .iter()
        .filter(|s| s.kind == snipdeck_symbols::Kind::Icon)
    {
        let svg = c.icon_svg(&s.id).unwrap();
        resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
            .unwrap_or_else(|e| panic!("{:?}: {e}", s.id));
    }
}

#[test]
fn icon_png_is_the_requested_size_and_colour() {
    let c = Catalog::get();
    for px in [24u32, 64, 128] {
        let (w, h, rgba) = decode(&c.icon_png(&trash(), px, [0x1f, 0x6f, 0xeb, 255]).unwrap());
        assert_eq!((w, h), (px, px));
        let opaque: Vec<_> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] == 255)
            .collect();
        assert!(!opaque.is_empty(), "{px}: some pixels are fully covered");
        assert!(
            opaque.iter().all(|p| p[..3] == [0x1f, 0x6f, 0xeb]),
            "{px}: exactly the colour"
        );
        assert_eq!(rgba[3], 0, "{px}: corner is transparent");
        // Anti-aliased edges keep the hue (straight alpha), within rounding.
        for p in rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 128) {
            assert!(
                p[0].abs_diff(0x1f) < 12 && p[2].abs_diff(0xeb) < 12,
                "{px}: edge {p:?}"
            );
        }
    }
}

#[test]
fn icon_png_honours_alpha_and_rejects_bad_requests() {
    let c = Catalog::get();
    let (_, _, rgba) = decode(&c.icon_png(&trash(), 64, [255, 0, 0, 128]).unwrap());
    let max_a = rgba.as_chunks::<4>().0.iter().map(|p| p[3]).max().unwrap();
    assert!(
        (126..=130).contains(&max_a),
        "half-transparent colour, got max alpha {max_a}"
    );
    assert!(c.icon_png(&trash(), 0, [0, 0, 0, 255]).is_none());
    assert!(c.icon_png(&trash(), 5000, [0, 0, 0, 255]).is_none());
    assert!(
        c.icon_png(&SymbolId::Emoji("1f600".into()), 64, [0, 0, 0, 255])
            .is_none()
    );
}
