//! Icons: Phosphor Regular as SVG text, or rendered to PNG with resvg.

use crate::catalog::{Catalog, SymbolId};

const OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256""#;

/// Wraps stored icon markup: a bare path-data string becomes `<path d="..."/>`.
fn body(markup: &str) -> String {
    if markup.starts_with('<') {
        markup.to_owned()
    } else {
        format!(r#"<path d="{markup}"/>"#)
    }
}

impl Catalog {
    fn markup(&self, id: &SymbolId) -> Option<String> {
        if !matches!(id, SymbolId::Icon(_)) {
            return None;
        }
        self.get_symbol(id)?.icon_markup().map(body)
    }

    /// The icon as a standalone SVG with `fill="currentColor"`, as Phosphor ships it.
    #[must_use]
    pub fn icon_svg(&self, id: &SymbolId) -> Option<String> {
        Some(format!(
            r#"{OPEN} fill="currentColor">{}</svg>"#,
            self.markup(id)?
        ))
    }

    /// The icon rendered to a `px` x `px` PNG in the colour `rgba` (straight alpha), transparent
    /// background. `None` for unknown ids and sizes outside 1..=2048.
    #[must_use]
    #[allow(
        clippy::many_single_char_names,
        reason = "r, g, b, a and w are the usual names"
    )]
    pub fn icon_png(&self, id: &SymbolId, px: u32, rgba: [u8; 4]) -> Option<Vec<u8>> {
        if px == 0 || px > 2048 {
            return None;
        }
        let [r, g, b, a] = rgba;
        let svg = format!(
            r#"{OPEN} fill="rgb({r},{g},{b})" fill-opacity="{}">{}</svg>"#,
            f32::from(a) / 255.0,
            self.markup(id)?
        );
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
        #[expect(clippy::cast_precision_loss, reason = "px is at most 2048")]
        let scale = px as f32 / 256.0;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let straight = pixmap.take_demultiplied();
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, px, px);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().ok()?;
        w.write_image_data(&straight).ok()?;
        w.finish().ok()?;
        Some(out)
    }
}
