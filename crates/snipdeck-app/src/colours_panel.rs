//! The Colours tab's pure half: the typed-values row (labels, texts, committing and stepping a channel) and
//! the picker's pointer maths on a unit square or disc. Slint-free; `colours_binding` draws it.
//! Docs: `docs/reviews/colour-spec.md` §3.0a.

use snipdeck_colour::{
    Cmyk, Hsl, Hsv, InputModel, PickerStyle, Rgba8, from_cmyk, from_hsl, from_hsv, parse, square_at,
    square_point, to_cmyk, to_hsl, to_hsv, wheel_at, wheel_point,
};

/// One typed channel of the Values row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    /// "R", "G", "B", "H", "S", "L", "C", "M", "Y", "K", "A" or "HEX".
    pub label: &'static str,
    /// What the field shows.
    pub text: String,
}

/// Why a typed value was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// Not a number (or, for the hex field, not a colour); the label names the field.
    NotANumber(&'static str),
    /// The field does not exist.
    NoSuchChannel,
}

/// Channel descriptors of a model: label and the largest value.
fn spec(model: InputModel) -> &'static [(&'static str, f64)] {
    match model {
        InputModel::Hex => &[("HEX", 0.0)],
        InputModel::Rgb => &[("R", 255.0), ("G", 255.0), ("B", 255.0)],
        InputModel::Hsl => &[("H", 360.0), ("S", 100.0), ("L", 100.0)],
        InputModel::Hsb => &[("H", 360.0), ("S", 100.0), ("B", 100.0)],
        InputModel::Cmyk => &[("C", 100.0), ("M", 100.0), ("Y", 100.0), ("K", 100.0)],
    }
}

fn whole(v: f64) -> String {
    format!("{}", v.round())
}

fn alpha_percent(c: Rgba8) -> f64 {
    f64::from(c.a) / 255.0 * 100.0
}

/// The fields the Values row shows for `c`: the model's channels, plus A (0 to 100) when the colour is not
/// opaque. `hue` stands for the hue of a grey, so the row does not jump to red.
#[must_use]
pub fn channels(c: Rgba8, model: InputModel, hue: f32) -> Vec<Channel> {
    let hue = f64::from(hue);
    let values: Vec<f64> = match model {
        InputModel::Hex => Vec::new(),
        InputModel::Rgb => vec![f64::from(c.r), f64::from(c.g), f64::from(c.b)],
        InputModel::Hsl => {
            let v = to_hsl(c);
            vec![
                if v.s > 0.0 { v.h } else { hue },
                v.s * 100.0,
                v.l * 100.0,
            ]
        }
        InputModel::Hsb => {
            let v = to_hsv(c);
            vec![
                if v.s > 0.0 && v.v > 0.0 { v.h } else { hue },
                v.s * 100.0,
                v.v * 100.0,
            ]
        }
        InputModel::Cmyk => {
            let v = to_cmyk(c);
            vec![v.c * 100.0, v.m * 100.0, v.y * 100.0, v.k * 100.0]
        }
    };
    let mut out: Vec<Channel> = if model == InputModel::Hex {
        vec![Channel {
            label: "HEX",
            text: c.to_hex(),
        }]
    } else {
        spec(model)
            .iter()
            .zip(values)
            .map(|((label, _), v)| Channel {
                label,
                text: whole(v),
            })
            .collect()
    };
    if c.a != 255 && model != InputModel::Hex {
        out.push(Channel {
            label: "A",
            text: whole(alpha_percent(c)),
        });
    }
    out
}

fn number(text: &str) -> Option<f64> {
    let t = text.trim().trim_end_matches(['%', '°']).trim();
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `c` with field `index` of `model` set to `text` (clamped to the field's range). The hex field takes any
/// colour literal. `hue` is the remembered hue, for HSL and HSB edits of a grey.
///
/// # Errors
/// [`Reject`] when the text is not a number or the field does not exist.
pub fn commit(
    c: Rgba8,
    model: InputModel,
    index: usize,
    text: &str,
    hue: f32,
) -> Result<Rgba8, Reject> {
    let shown = channels(c, model, hue);
    let Some(field) = shown.get(index) else {
        return Err(Reject::NoSuchChannel);
    };
    let label = field.label;
    if model == InputModel::Hex {
        return parse(text).map_err(|_| Reject::NotANumber(label));
    }
    let v = number(text).ok_or(Reject::NotANumber(label))?;
    if label == "A" {
        let a = (v.clamp(0.0, 100.0) / 100.0 * 255.0).round();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=255 above"
        )]
        return Ok(Rgba8 { a: a as u8, ..c });
    }
    let max = spec(model)[index].1;
    let v = v.clamp(0.0, max);
    let hue = f64::from(hue);
    Ok(match model {
        InputModel::Hex => c,
        InputModel::Rgb => {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to 0..=255 above"
            )]
            let byte = v.round() as u8;
            match index {
                0 => Rgba8 { r: byte, ..c },
                1 => Rgba8 { g: byte, ..c },
                _ => Rgba8 { b: byte, ..c },
            }
        }
        InputModel::Hsl => {
            let mut h = to_hsl(c);
            if h.s <= 0.0 {
                h.h = hue;
            }
            let h = match index {
                0 => Hsl { h: v, ..h },
                1 => Hsl { s: v / 100.0, ..h },
                _ => Hsl { l: v / 100.0, ..h },
            };
            from_hsl(h, c.a)
        }
        InputModel::Hsb => {
            let mut h = to_hsv(c);
            if h.s <= 0.0 || h.v <= 0.0 {
                h.h = hue;
            }
            let h = match index {
                0 => Hsv { h: v, ..h },
                1 => Hsv { s: v / 100.0, ..h },
                _ => Hsv { v: v / 100.0, ..h },
            };
            from_hsv(h, c.a)
        }
        InputModel::Cmyk => {
            let k = to_cmyk(c);
            let k = match index {
                0 => Cmyk { c: v / 100.0, ..k },
                1 => Cmyk { m: v / 100.0, ..k },
                2 => Cmyk { y: v / 100.0, ..k },
                _ => Cmyk { k: v / 100.0, ..k },
            };
            from_cmyk(k, c.a)
        }
    })
}

/// Field `index` moved by `delta` (Up and Down: 1, with Shift 10). The hex field does not step.
///
/// # Errors
/// [`Reject`] when the field does not exist.
pub fn step(
    c: Rgba8,
    model: InputModel,
    index: usize,
    delta: i32,
    hue: f32,
) -> Result<Rgba8, Reject> {
    let shown = channels(c, model, hue);
    let Some(field) = shown.get(index) else {
        return Err(Reject::NoSuchChannel);
    };
    if model == InputModel::Hex {
        return Ok(c);
    }
    let now = number(&field.text).ok_or(Reject::NotANumber(field.label))?;
    // A hue wraps round; the rest stop at their ends (`commit` clamps).
    let next = now + f64::from(delta);
    let next = if field.label == "H" {
        next.rem_euclid(360.0)
    } else {
        next
    };
    commit(c, model, index, &whole(next), hue)
}

/// The hue, saturation and brightness (0..360, 0..100, 0..100) of `c`, keeping `hue` when `c` has none.
#[must_use]
pub fn hsb(c: Rgba8, hue: f32) -> (f32, f32, f32) {
    let v = to_hsv(c);
    #[allow(clippy::cast_possible_truncation, reason = "0..=360 fits an f32")]
    let (h, s, b) = (v.h as f32, (v.s * 100.0) as f32, (v.v * 100.0) as f32);
    (if s > 0.0 && b > 0.0 { h } else { hue }, s, b)
}

/// The colour under a pointer at the unit position `(x, y)` of the picker surface (0..1 each). A drag past the
/// edge of the wheel stays on the rim at the same angle.
#[must_use]
pub fn pick(style: PickerStyle, x: f32, y: f32, hue: f32, bri: f32, alpha: u8) -> (Rgba8, f32, f32) {
    match style {
        PickerStyle::Square => {
            let (sat, bri) = square_at(x, y, 1.0, 1.0);
            (colour_of(hue, sat, bri, alpha), hue, bri)
        }
        PickerStyle::Wheel => {
            let (dx, dy) = (x - 0.5, y - 0.5);
            let dist = dx.hypot(dy);
            let (x, y) = if dist > 0.5 {
                (0.5 + dx / dist * 0.5, 0.5 + dy / dist * 0.5)
            } else {
                (x, y)
            };
            let (h, sat) = wheel_at(x, y, 1.0).unwrap_or((hue, 100.0));
            // At the exact centre the angle is arbitrary: keep the hue.
            let h = if sat <= 0.0 { hue } else { h };
            (colour_of(h, sat, bri, alpha), h, bri)
        }
    }
}

fn colour_of(hue: f32, sat: f32, bri: f32, alpha: u8) -> Rgba8 {
    from_hsv(
        Hsv {
            h: f64::from(hue),
            s: f64::from(sat) / 100.0,
            v: f64::from(bri) / 100.0,
        },
        alpha,
    )
}

/// The marker's unit position for `(hue, sat, bri)`.
#[must_use]
pub fn marker(style: PickerStyle, hue: f32, sat: f32, bri: f32) -> (f32, f32) {
    match style {
        PickerStyle::Square => square_point(sat, bri, 1.0, 1.0),
        PickerStyle::Wheel => wheel_point(hue, sat, 1.0),
    }
}

/// An arrow key on the surface: `dx`, `dy` steps (Page Up and Down are ten). Square: saturation by `dx`,
/// brightness by `dy`; wheel: hue by `dx`, saturation by `dy`.
#[must_use]
pub fn nudge(
    style: PickerStyle,
    (hue, sat, bri): (f32, f32, f32),
    dx: i32,
    dy: i32,
    alpha: u8,
) -> (Rgba8, f32) {
    #[allow(clippy::cast_precision_loss, reason = "a few steps")]
    let (dx, dy) = (dx as f32, dy as f32);
    let (h, s, b) = match style {
        PickerStyle::Square => (hue, (sat + dx).clamp(0.0, 100.0), (bri + dy).clamp(0.0, 100.0)),
        PickerStyle::Wheel => ((hue + dx).rem_euclid(360.0), (sat + dy).clamp(0.0, 100.0), bri),
    };
    (colour_of(h, s, b, alpha), h)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: Rgba8 = Rgba8::rgb(58, 123, 213);

    fn texts(c: Rgba8, m: InputModel) -> Vec<String> {
        channels(c, m, 0.0).into_iter().map(|c| c.text).collect()
    }

    #[test]
    fn each_model_lists_its_channels_and_adds_alpha_only_when_not_opaque() {
        assert_eq!(texts(BLUE, InputModel::Hex), ["#3a7bd5"]);
        assert_eq!(texts(BLUE, InputModel::Rgb), ["58", "123", "213"]);
        assert_eq!(texts(BLUE, InputModel::Hsl), ["215", "65", "53"]);
        assert_eq!(texts(BLUE, InputModel::Hsb), ["215", "73", "84"]);
        assert_eq!(texts(BLUE, InputModel::Cmyk), ["73", "42", "0", "16"]);
        let half = Rgba8 { a: 128, ..BLUE };
        let rgb = channels(half, InputModel::Rgb, 0.0);
        assert_eq!(rgb.last().map(|c| (c.label, c.text.as_str())), Some(("A", "50")));
        assert_eq!(channels(half, InputModel::Hex, 0.0).len(), 1);
    }

    #[test]
    fn a_grey_keeps_the_remembered_hue_in_the_hue_field() {
        let grey = Rgba8::rgb(128, 128, 128);
        assert_eq!(texts(grey, InputModel::Hsl)[0], "0");
        assert_eq!(channels(grey, InputModel::Hsb, 215.0)[0].text, "215");
    }

    #[test]
    fn a_typed_value_changes_one_channel_and_clamps() {
        let c = commit(BLUE, InputModel::Rgb, 0, "255", 0.0).unwrap();
        assert_eq!(c, Rgba8::rgb(255, 123, 213));
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 1, "999", 0.0).unwrap(),
            Rgba8::rgb(58, 255, 213)
        );
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 2, "-4", 0.0).unwrap(),
            Rgba8::rgb(58, 123, 0)
        );
        assert_eq!(
            commit(BLUE, InputModel::Hsb, 2, "100", 0.0).unwrap(),
            from_hsv(
                Hsv {
                    v: 1.0,
                    ..to_hsv(BLUE)
                },
                255
            )
        );
    }

    #[test]
    fn a_bad_value_is_rejected_by_field_name() {
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 1, "green", 0.0),
            Err(Reject::NotANumber("G"))
        );
        assert_eq!(
            commit(BLUE, InputModel::Hex, 0, "#12", 0.0),
            Err(Reject::NotANumber("HEX"))
        );
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 9, "1", 0.0),
            Err(Reject::NoSuchChannel)
        );
    }

    #[test]
    fn the_hex_field_takes_any_colour_literal_and_a_percent_sign_is_allowed() {
        assert_eq!(
            commit(BLUE, InputModel::Hex, 0, "rgb(1, 2, 3)", 0.0).unwrap(),
            Rgba8::rgb(1, 2, 3)
        );
        assert_eq!(
            commit(BLUE, InputModel::Hsl, 1, "50%", 0.0).unwrap(),
            from_hsl(
                Hsl {
                    s: 0.5,
                    ..to_hsl(BLUE)
                },
                255
            )
        );
    }

    #[test]
    fn alpha_is_a_percentage() {
        let half = Rgba8 { a: 255, ..BLUE };
        let translucent = commit(half, InputModel::Rgb, 3, "50", 0.0);
        assert_eq!(translucent, Err(Reject::NoSuchChannel));
        let half = Rgba8 { a: 128, ..BLUE };
        let c = commit(half, InputModel::Rgb, 3, "100", 0.0).unwrap();
        assert_eq!(c.a, 255);
    }

    #[test]
    fn stepping_moves_by_one_or_ten_and_the_hue_wraps() {
        assert_eq!(
            step(BLUE, InputModel::Rgb, 0, 1, 0.0).unwrap(),
            Rgba8::rgb(59, 123, 213)
        );
        assert_eq!(
            step(BLUE, InputModel::Rgb, 0, -10, 0.0).unwrap(),
            Rgba8::rgb(48, 123, 213)
        );
        assert_eq!(
            step(Rgba8::rgb(255, 255, 255), InputModel::Rgb, 0, 1, 0.0).unwrap(),
            Rgba8::rgb(255, 255, 255)
        );
        let red = Rgba8::rgb(255, 0, 0);
        let back = step(red, InputModel::Hsb, 0, -1, 0.0).unwrap();
        assert_eq!(channels(back, InputModel::Hsb, 0.0)[0].text, "359");
        assert_eq!(step(BLUE, InputModel::Hex, 0, 1, 0.0).unwrap(), BLUE);
    }

    #[test]
    fn the_square_picks_saturation_and_brightness_and_clamps() {
        let (c, hue, bri) = pick(PickerStyle::Square, 1.0, 0.0, 120.0, 0.0, 255);
        assert_eq!(c, Rgba8::rgb(0, 255, 0));
        assert!((hue - 120.0).abs() < f32::EPSILON && (bri - 100.0).abs() < 1e-3);
        let (c, ..) = pick(PickerStyle::Square, -3.0, 5.0, 120.0, 0.0, 255);
        assert_eq!(c, Rgba8::rgb(0, 0, 0));
        let (c, ..) = pick(PickerStyle::Square, 0.0, 0.0, 120.0, 0.0, 128);
        assert_eq!((c.r, c.a), (255, 128));
    }

    #[test]
    fn the_wheel_picks_hue_and_saturation_and_stays_on_the_rim() {
        // The top of the rim is hue 0, full saturation: red at full brightness.
        let (c, hue, _) = pick(PickerStyle::Wheel, 0.5, 0.0, 99.0, 100.0, 255);
        assert_eq!(c, Rgba8::rgb(255, 0, 0));
        assert!(hue.abs() < 1e-3 || (hue - 360.0).abs() < 1e-3);
        // Far outside to the right: hue 90, on the rim.
        let (_, hue, _) = pick(PickerStyle::Wheel, 3.0, 0.5, 0.0, 100.0, 255);
        assert!((hue - 90.0).abs() < 1e-3);
        // The centre is white at full brightness and keeps the hue.
        let (c, hue, _) = pick(PickerStyle::Wheel, 0.5, 0.5, 215.0, 100.0, 255);
        assert_eq!(c, Rgba8::rgb(255, 255, 255));
        assert!((hue - 215.0).abs() < 1e-3);
    }

    #[test]
    fn the_marker_is_where_the_pick_came_from() {
        for (x, y) in [(0.2, 0.7), (0.9, 0.1)] {
            let (c, hue, _) = pick(PickerStyle::Square, x, y, 200.0, 0.0, 255);
            let (h, s, b) = hsb(c, hue);
            let (mx, my) = marker(PickerStyle::Square, h, s, b);
            assert!((mx - x).abs() < 0.02 && (my - y).abs() < 0.02, "{mx} {my}");
        }
        let (c, hue, bri) = pick(PickerStyle::Wheel, 0.7, 0.3, 0.0, 100.0, 255);
        let (h, s, _) = hsb(c, hue);
        let (mx, my) = marker(PickerStyle::Wheel, h, s, bri);
        assert!((mx - 0.7).abs() < 0.02 && (my - 0.3).abs() < 0.02, "{mx} {my}");
    }

    #[test]
    fn arrow_keys_move_saturation_brightness_and_hue_by_steps() {
        let start = hsb(Rgba8::rgb(255, 0, 0), 0.0);
        let (c, _) = nudge(PickerStyle::Square, start, 0, -10, 255);
        assert_eq!(hsb(c, 0.0).2.round(), 90.0);
        let (_, hue) = nudge(PickerStyle::Wheel, start, -1, 0, 255);
        assert!((hue - 359.0).abs() < 1e-3);
        let (c, _) = nudge(PickerStyle::Wheel, start, 0, -10, 255);
        assert_eq!(hsb(c, 0.0).1.round(), 90.0);
    }
}
