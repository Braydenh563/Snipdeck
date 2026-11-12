//! The hand-picking surfaces: saturation/brightness square and HSB hue wheel (pixels and pointer maths),
//! plus the two settings enums that remember the user's choices.
//!
//! Units: hue in degrees 0..360, saturation and brightness 0..100. Pixel `(i, j)` is sampled at its
//! centre `(i + 0.5, j + 0.5)`, the same coordinates the pointer functions take.

#![allow(
    clippy::many_single_char_names,
    reason = "r/g/b and w/h are the conventional names"
)]

use crate::model::{Hsv, from_hsv};
use serde::{Deserialize, Serialize};

/// Which surface the picker shows (`colour_picker_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PickerStyle {
    /// Saturation by brightness gradient.
    #[default]
    Square,
    /// HSB hue wheel.
    Wheel,
}

/// Which channel set the Values row edits (`colour_input_model`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputModel {
    /// One hex field.
    #[default]
    Hex,
    /// R G B.
    Rgb,
    /// H S L.
    Hsl,
    /// H S B.
    Hsb,
    /// C M Y K.
    Cmyk,
}

fn px(hue: f32, sat: f32, bri: f32) -> [u8; 3] {
    let c = from_hsv(
        Hsv {
            h: f64::from(hue),
            s: f64::from(sat) / 100.0,
            v: f64::from(bri) / 100.0,
        },
        255,
    );
    [c.r, c.g, c.b]
}

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
fn fl(v: u32) -> f32 {
    v as f32
}

/// RGBA8 `w` x `h` buffer: x is saturation 0..100, y is brightness 100 (top) to 0, at `hue`.
pub fn render_square(hue: f32, w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for j in 0..h {
        let (_, bri) = square_at(0.0, fl(j) + 0.5, fl(w), fl(h));
        for i in 0..w {
            let (sat, _) = square_at(fl(i) + 0.5, 0.0, fl(w), fl(h));
            let [r, g, b] = px(hue, sat, bri);
            out.extend_from_slice(&[r, g, b, 255]);
        }
    }
    out
}

/// RGBA8 `d` x `d` buffer of the HSB wheel at `brightness` (0..100): hue round the rim, 0 degrees at the
/// top going clockwise, saturation 0 at the centre to 100 at the rim. Outside the disc it is
/// transparent; the rim has a 1 px anti-aliased edge (alpha only).
pub fn render_wheel(brightness: f32, d: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(d as usize * d as usize * 4);
    let (c, r) = (fl(d) / 2.0, fl(d) / 2.0);
    for j in 0..d {
        for i in 0..d {
            let (dx, dy) = (fl(i) + 0.5 - c, fl(j) + 0.5 - c);
            let dist = dx.hypot(dy);
            let cover = (r - dist + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let hue = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
            let [red, green, blue] = px(hue, (dist / r * 100.0).min(100.0), brightness);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "cover is 0..=1"
            )]
            let alpha = (cover * 255.0).round() as u8;
            out.extend_from_slice(&[red, green, blue, alpha]);
        }
    }
    out
}

/// The `(hue, saturation)` under a pointer at `(x, y)` on a wheel of diameter `d`; `None` outside the disc.
pub fn wheel_at(x: f32, y: f32, d: f32) -> Option<(f32, f32)> {
    let r = d / 2.0;
    let (dx, dy) = (x - r, y - r);
    let dist = dx.hypot(dy);
    if r <= 0.0 || dist > r * (1.0 + 1e-5) {
        return None;
    }
    Some((
        dx.atan2(-dy).to_degrees().rem_euclid(360.0),
        (dist / r * 100.0).min(100.0),
    ))
}

/// The pointer position of `(hue, sat)` on a wheel of diameter `d`; the inverse of [`wheel_at`].
pub fn wheel_point(hue: f32, sat: f32, d: f32) -> (f32, f32) {
    let r = d / 2.0;
    let (rad, dist) = (hue.to_radians(), r * sat / 100.0);
    (r + dist * rad.sin(), r - dist * rad.cos())
}

/// The `(saturation, brightness)` under a pointer at `(x, y)` on a `w` x `h` square, clamped to the square.
pub fn square_at(x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
    let unit = |v: f32, size: f32| {
        if size > 0.0 {
            (v / size).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    (unit(x, w) * 100.0, (1.0 - unit(y, h)) * 100.0)
}

/// The pointer position of `(sat, bri)` on a `w` x `h` square; the inverse of [`square_at`].
pub fn square_point(sat: f32, bri: f32, w: f32, h: f32) -> (f32, f32) {
    (
        sat.clamp(0.0, 100.0) / 100.0 * w,
        (1.0 - bri.clamp(0.0, 100.0) / 100.0) * h,
    )
}
