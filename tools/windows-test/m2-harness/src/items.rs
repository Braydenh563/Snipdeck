//! The test items. Their content is made up here; nothing is read from the clipboard.

use snipdeck_platform::payload::{Data, FilePayload, Payload};

/// Which test item a hotkey uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// Plain text.
    Text,
    /// HTML with a different plain-text fallback.
    Html,
    /// A 64x64 RGBA PNG with a transparent corner.
    Png,
    /// Two small text files.
    Files,
    /// A long multi-line text with non-ASCII characters.
    Unicode,
}

impl Item {
    /// Short name for the console.
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Html => "html",
            Self::Png => "png",
            Self::Files => "files",
            Self::Unicode => "unicode",
        }
    }

    /// The payload for this item.
    pub fn payload(self) -> Payload {
        let data = match self {
            Self::Text => vec![Data::Text("Hello from Snipdeck M2".into())],
            // The text differs from the HTML's own text so plain paste is recognisable.
            Self::Html => vec![
                Data::Html(
                    "<h3 style=\"color:#c00\">Snipdeck M2 HTML</h3>\
                     <p>Formatted: <b>bold</b>, <i>italic</i> and \
                     <a href=\"https://example.com/\">a link</a>.</p>"
                        .into(),
                ),
                Data::Text("Plain text of the HTML item".into()),
            ],
            Self::Png => vec![Data::Png(test_png())],
            Self::Files => vec![Data::Files(vec![
                FilePayload {
                    name: "m2-note-1.txt".into(),
                    bytes: b"M2 file one\r\n".to_vec(),
                },
                FilePayload {
                    name: "m2-note-2.txt".into(),
                    bytes: b"M2 file two\r\n".to_vec(),
                },
            ])],
            Self::Unicode => vec![Data::Text(long_unicode())],
        };
        Payload::new(data).expect("every item has data")
    }
}

/// The text another app "copies" while a paste is in flight (`--clobber`).
pub fn clobber_payload() -> Payload {
    Payload::new(vec![Data::Text("Written by another app mid-paste".into())])
        .expect("one text representation")
}

fn long_unicode() -> String {
    let mut text = String::from("BEGIN-M2-TEXT\n");
    for n in 1..=40 {
        text.push_str(&format!(
            "Line {n:02}: Gr\u{fc}\u{df}e, na\u{ef}ve caf\u{e9}, \u{4f60}\u{597d}, \
             \u{3053}\u{3093}\u{306b}\u{3061}\u{306f}, \u{c548}\u{b155}, \u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}, \
             \u{2713} \u{20ac} \u{2030}, e\u{301}, \u{1f600}\u{1f680}, \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\n"
        ));
    }
    text.push_str("\n\tTabbed line after a blank one\nEND-M2-TEXT");
    text
}

/// 64x64 RGBA: four colour quadrants, a black frame, and a fully transparent 20x20 top-left
/// corner whose (invisible) colour is magenta, so a viewer that ignores alpha shows it.
fn test_png() -> Vec<u8> {
    const SIZE: u32 = 64;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let rgba = if x < 20 && y < 20 {
                [255, 0, 255, 0]
            } else if x == 0 || y == 0 || x == SIZE - 1 || y == SIZE - 1 {
                [0, 0, 0, 255]
            } else {
                match (x < SIZE / 2, y < SIZE / 2) {
                    (true, true) => [220, 40, 40, 255],
                    (false, true) => [40, 180, 60, 255],
                    (true, false) => [40, 80, 220, 255],
                    (false, false) => [240, 200, 30, 255],
                }
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, SIZE, SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("PNG header");
    writer.write_image_data(&pixels).expect("PNG data");
    writer.finish().expect("PNG end");
    out
}
