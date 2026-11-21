//! Snipdeck colour maths: pure Rust, no UI, no clock, no I/O.
//!
//! Colours are 8-bit sRGB with alpha ([`Rgba8`]). The crate converts to HSL, HSB, CMYK and OKLCH,
//! prints and parses the copy formats ([`format()`], [`parse`], [`literal`]), finds the nearest CSS
//! keyword ([`nearest_name`]), builds schemes ([`scheme`]), computes WCAG contrast ([`contrast_ratio`],
//! [`marks`]), keeps recents ([`ColourRecents`]), builds the text a saved colour item holds
//! ([`saved_text`], [`title_for`], [`css_variables`]) and draws and hit-tests the hand-picking square
//! and wheel ([`render_square`], [`render_wheel`]).
//!
//! # Parsing
//! [`parse`] accepts `#rgb #rgba #rrggbb #rrggbbaa`, the same without `#`, `0x` plus six digits,
//! `rgb()`/`rgba()`, `hsl()`/`hsla()`, `hsb()`/`hsv()`, `cmyk()` and `oklch()` with commas or spaces and
//! an optional `/ alpha`, three bare integers (`58, 123, 213`), the 148 CSS keywords and `transparent`.
//! Also `hsba()`, `hwb()`, `lab()`, `lch()`, `oklab()`, `color(srgb|srgb-linear|display-p3 ...)`,
//! `0xAARRGGBB`, decimal integers, four bare numbers with alpha, `Color.FromArgb(...)`,
//! `UIColor(red: ...)` and `Color(0xAARRGGBB)`; input is NFKC-folded (full-width digits).
//! [`parse_live`] is the stricter form for the live field, [`extract`] finds a colour inside pasted
//! code, and [`literal`] is the strict form used for card swatches.
//!
//! # Deviations from the spec's signatures
//! * `Format`, `Scheme`, `PickerStyle` and `InputModel` also derive `Default` (Hex, Complementary, Square,
//!   Hex) and `Hash`; `Format`/`Scheme`/`ParseError` keep every §5 derive. `ParseError` also implements
//!   `Display` and `std::error::Error`.
//! * `Format::Name.label()` returns `"Name"`; the UI supplies its own translated text.
//! * `title_for` returns an empty string for a colour with alpha below 255 (a keyword does not describe it).
//! * Settings code must itself map `"name"` read for `colour_format` to `Format::Hex`
//!   (`Format::can_be_default`).
//!
//! Every other §5 signature is exactly as specified.
#![forbid(unsafe_code)]
#![allow(
    clippy::many_single_char_names,
    reason = "r/g/b, l/a/b, h/s/v and c/f are the conventional names in colour code"
)]

mod code;
mod contrast;
mod css4;
mod format;
mod model;
mod names;
mod parse;
mod picker;
mod recents;
mod saved;
mod scheme;

pub use code::{ExtraFormat, custom_format, extra_text};
pub use contrast::{
    Marks, apca, composite, contrast_over, contrast_ratio, marks, nearest_passing, needs_backdrop,
    ratio_text, relative_luminance,
};
pub use format::{Format, copy_text, format};
pub use model::{
    Cmyk, Hsl, Hsv, Oklch, Rgba8, from_cmyk, from_hsl, from_hsv, from_oklch, to_cmyk, to_hsl,
    to_hsv, to_oklch,
};
pub use names::{Named, named, nearest_name};
pub use parse::{ParseError, decimal, extract, literal, parse, parse_live};
pub use picker::{
    InputModel, PickerStyle, render_square, render_wheel, square_at, square_point, wheel_at,
    wheel_point,
};
pub use recents::{ColourRecents, RecentColour};
pub use saved::{css_variables, saved_text, title_for};
pub use scheme::{Scheme, has_hue, scheme};
