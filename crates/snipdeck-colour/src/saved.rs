//! Text stored when a colour is saved as a snippet.

use std::fmt::Write as _;

use crate::format::{Format, format};
use crate::model::Rgba8;
use crate::names::nearest_name;
use crate::parse::parse;

/// The text a saved colour item holds: `format(c, f)` when `parse` gives `c` back exactly, else
/// hex. HSB and CMYK are always hex: the card rule (`literal`) does not see `hsb()` or `cmyk()`
/// as a colour, so such an item would lose its swatch and its Paste as formats.
pub fn saved_text(c: Rgba8, f: Format) -> String {
    let text = format(c, f);
    if !matches!(f, Format::Hsb | Format::Cmyk) && parse(&text) == Ok(c) {
        text
    } else {
        c.to_hex()
    }
}

/// The default item title: the CSS keyword when `c` is opaque and exactly a keyword, else empty.
pub fn title_for(c: Rgba8) -> String {
    let n = nearest_name(c);
    if n.exact && c.a == 255 {
        n.name.to_owned()
    } else {
        String::new()
    }
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// `:root {` custom properties `}` for `(title, stored text)` pairs in order. The name is the slug of
/// the title (lower-case letters and digits, other runs become `-`), else the colour's hex digits
/// (`3a7bd5`), else `colour`; a repeated name gets `-2`, `-3`, and so on.
pub fn css_variables(items: &[(&str, &str)]) -> String {
    let mut used: Vec<String> = Vec::new();
    let mut out = String::from(":root {\n");
    for (title, value) in items {
        let value = value.trim();
        let mut base = slug(title);
        if base.is_empty() {
            base = parse(value)
                .map(|c| c.to_hex().trim_start_matches('#').to_owned())
                .unwrap_or_default();
        }
        if base.is_empty() {
            "colour".clone_into(&mut base);
        }
        let mut name = base.clone();
        let mut n = 2;
        while used.contains(&name) {
            name = format!("{base}-{n}");
            n += 1;
        }
        let _ = writeln!(out, "  --{name}: {value};");
        used.push(name);
    }
    out.push('}');
    out
}
