//! The Colours tab's pure half: the typed-values row (labels, texts, committing and stepping a channel) and
//! the picker's pointer maths on a unit square or disc. Slint-free; `colours_binding` draws it.
//! Docs: `docs/reviews/colour-spec.md` §3.0a.

use snipdeck_colour::{
    Cmyk, Hsl, Hsv, InputModel, PickerStyle, Rgba8, Scheme, from_cmyk, from_hsl, from_hsv, parse,
    square_at, square_point, to_cmyk, to_hsl, to_hsv, wheel_at, wheel_point,
};

/// The picker's own hue, saturation and brightness (0..360, 0..100, 0..100). The 8-bit colour cannot
/// give them back (black has no saturation, a grey no hue), so the binding keeps them next to it.
pub type Hsb = (f32, f32, f32);

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

/// The typed values of the HSL or CMYK row as the user left them (percent, hue in degrees): the 8-bit
/// colour cannot give them back (L=0 is black whatever S was). Kept next to the colour they made.
pub type Memo = Vec<f64>;

/// The raw (unrounded) values of `model`'s fields for `c`: the memo when it is for this model, else
/// read from the colour (HSB from the picker's own state).
fn base(c: Rgba8, model: InputModel, st: Hsb, memo: Option<&[f64]>) -> Vec<f64> {
    let hue = f64::from(st.0);
    match model {
        InputModel::Hex => Vec::new(),
        InputModel::Rgb => vec![f64::from(c.r), f64::from(c.g), f64::from(c.b)],
        InputModel::Hsl => {
            if let Some(m) = memo.filter(|m| m.len() == 3) {
                m.to_vec()
            } else {
                let v = to_hsl(c);
                vec![if v.s > 0.0 { v.h } else { hue }, v.s * 100.0, v.l * 100.0]
            }
        }
        InputModel::Hsb => vec![hue, f64::from(st.1), f64::from(st.2)],
        InputModel::Cmyk => {
            if let Some(m) = memo.filter(|m| m.len() == 4) {
                m.to_vec()
            } else {
                let v = to_cmyk(c);
                vec![v.c * 100.0, v.m * 100.0, v.y * 100.0, v.k * 100.0]
            }
        }
    }
}

/// The fields the Values row shows for `c`: the model's channels, plus A (0 to 100) when the colour is not
/// opaque. `st` stands for the hue of a grey, so the row does not jump to red; `memo` is what was last typed
/// or stepped in the HSL or CMYK row, while it still makes `c`.
#[must_use]
pub fn channels(c: Rgba8, model: InputModel, st: Hsb, memo: Option<&[f64]>) -> Vec<Channel> {
    let mut values = base(c, model, st, memo);
    // A hue that rounds to 360 reads 0, as the format rows do.
    if matches!(model, InputModel::Hsl | InputModel::Hsb) && values[0].round() >= 360.0 {
        values[0] = 0.0;
    }
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
/// colour literal. `st` is the picker's own hue, saturation and brightness: HSB edits start from
/// it, and an HSL edit of a grey keeps its hue. Returns the new colour and the new `st` (a typed hue
/// is kept even where the colour has none). The third part is the HSL or CMYK row's values to keep next
/// to the colour (`memo` is what it held).
///
/// # Errors
/// [`Reject`] when the text is not a number or the field does not exist.
pub fn commit(
    c: Rgba8,
    model: InputModel,
    index: usize,
    text: &str,
    st: Hsb,
    memo: Option<&[f64]>,
) -> Result<(Rgba8, Hsb, Option<Memo>), Reject> {
    let shown = channels(c, model, st, memo);
    let Some(field) = shown.get(index) else {
        return Err(Reject::NoSuchChannel);
    };
    let label = field.label;
    if model == InputModel::Hex {
        let c = parse(text).map_err(|_| Reject::NotANumber(label))?;
        return Ok((c, hsb(c, st.0), None));
    }
    let v = number(text).ok_or(Reject::NotANumber(label))?;
    if label == "A" {
        let a = (v.clamp(0.0, 100.0) / 100.0 * 255.0).round();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped to 0..=255 above"
        )]
        return Ok((Rgba8 { a: a as u8, ..c }, st, memo.map(<[f64]>::to_vec)));
    }
    let max = spec(model)[index].1;
    let v = v.clamp(0.0, max);
    let hue = f64::from(st.0);
    let made = match model {
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
            let mut now = base(c, model, st, memo);
            now[index] = v;
            let made = from_hsl(
                Hsl {
                    h: now[0],
                    s: now[1] / 100.0,
                    l: now[2] / 100.0,
                },
                c.a,
            );
            // A typed hue stays where the user put it, even on a grey; any other edit keeps the hue.
            #[allow(clippy::cast_possible_truncation, reason = "0..=360 fits an f32")]
            let keep = if index == 0 { v as f32 } else { st.0 };
            return Ok((made, hsb(made, keep), Some(now)));
        }
        InputModel::Hsb => {
            let h = Hsv {
                h: hue,
                s: f64::from(st.1) / 100.0,
                v: f64::from(st.2) / 100.0,
            };
            let h = match index {
                0 => Hsv { h: v, ..h },
                1 => Hsv { s: v / 100.0, ..h },
                _ => Hsv { v: v / 100.0, ..h },
            };
            #[allow(
                clippy::cast_possible_truncation,
                reason = "0..=360 and 0..=100 fit an f32"
            )]
            let next = (h.h as f32, (h.s * 100.0) as f32, (h.v * 100.0) as f32);
            return Ok((from_hsv(h, c.a), next, None));
        }
        InputModel::Cmyk => {
            let mut now = base(c, model, st, memo);
            now[index] = v;
            let made = from_cmyk(
                Cmyk {
                    c: now[0] / 100.0,
                    m: now[1] / 100.0,
                    y: now[2] / 100.0,
                    k: now[3] / 100.0,
                },
                c.a,
            );
            return Ok((made, hsb(made, st.0), Some(now)));
        }
    };
    Ok((made, hsb(made, st.0), None))
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
    st: Hsb,
    memo: Option<&[f64]>,
) -> Result<(Rgba8, Hsb, Option<Memo>), Reject> {
    let shown = channels(c, model, st, memo);
    let Some(field) = shown.get(index) else {
        return Err(Reject::NoSuchChannel);
    };
    if model == InputModel::Hex {
        return Ok((c, st, None));
    }
    let now = number(&field.text).ok_or(Reject::NotANumber(field.label))?;
    // A hue wraps round; the rest stop at their ends (`commit` clamps).
    let next = now + f64::from(delta);
    let next = if field.label == "H" {
        next.rem_euclid(360.0)
    } else {
        next
    };
    commit(c, model, index, &whole(next), st, memo)
}

/// `c` laid over opaque white, as a browser would paint it (the contrast ratio treats its
/// background as opaque, so a translucent one is flattened first).
#[must_use]
pub fn over_white(c: Rgba8) -> Rgba8 {
    let a = u16::from(c.a);
    let mix = |v: u8| {
        // Rounded like the 8-bit result a browser paints; at most 255 by construction.
        u8::try_from((u16::from(v) * a + 255 * (255 - a) + 127) / 255).unwrap_or(255)
    };
    Rgba8::rgb(mix(c.r), mix(c.g), mix(c.b))
}

/// A slider value held to `0..=max`; NaN (a broken pointer event) reads as 0.
pub fn slider(v: f32, max: f32) -> f32 {
    if v.is_nan() { 0.0 } else { v.clamp(0.0, max) }
}

/// The hue, saturation and brightness (0..360, 0..100, 0..100) of `c`, keeping `hue` when `c` has none.
#[must_use]
pub fn hsb(c: Rgba8, hue: f32) -> (f32, f32, f32) {
    let v = to_hsv(c);
    #[allow(clippy::cast_possible_truncation, reason = "0..=360 fits an f32")]
    let (hh, sat, bri) = (v.h as f32, (v.s * 100.0) as f32, (v.v * 100.0) as f32);
    (if sat > 0.0 && bri > 0.0 { hh } else { hue }, sat, bri)
}

/// The colour under a pointer at the unit position `(x, y)` of the picker surface (0..1 each), with
/// the hue, saturation and brightness it was made from. A drag past the edge of the wheel stays on the
/// rim at the same angle. `st` is what the picker held: the square keeps its hue, the wheel its
/// brightness.
#[must_use]
pub fn pick_hsb(style: PickerStyle, x: f32, y: f32, st: Hsb, alpha: u8) -> (Rgba8, Hsb) {
    let (hue, _, bri) = st;
    let next = match style {
        PickerStyle::Square => {
            let (sat, bri) = square_at(x, y, 1.0, 1.0);
            (hue, sat, bri)
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
            (if sat <= 0.0 { hue } else { h }, sat, bri)
        }
    };
    (colour_of(next.0, next.1, next.2, alpha), next)
}

/// [`pick_hsb`] as `(colour, hue, brightness)`.
#[must_use]
pub fn pick(
    style: PickerStyle,
    x: f32,
    y: f32,
    hue: f32,
    bri: f32,
    alpha: u8,
) -> (Rgba8, f32, f32) {
    let (c, st) = pick_hsb(style, x, y, (hue, 0.0, bri), alpha);
    (c, st.0, st.2)
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
) -> (Rgba8, Hsb) {
    #[allow(clippy::cast_precision_loss, reason = "a few steps")]
    let (dx, dy) = (dx as f32, dy as f32);
    let (h, s, b) = match style {
        PickerStyle::Square => (
            hue,
            (sat + dx).clamp(0.0, 100.0),
            (bri + dy).clamp(0.0, 100.0),
        ),
        PickerStyle::Wheel => (
            (hue + dx).rem_euclid(360.0),
            (sat + dy).clamp(0.0, 100.0),
            bri,
        ),
    };
    (colour_of(h, s, b, alpha), (h, s, b))
}

/// Page Up and Page Down on the surface: brightness by `delta` (10) in the square and in the wheel.
#[must_use]
pub fn page(hsb: Hsb, delta: i32, alpha: u8) -> (Rgba8, Hsb) {
    let (hue, sat, bri) = hsb;
    #[allow(clippy::cast_precision_loss, reason = "a few steps")]
    let bri = (bri + delta as f32).clamp(0.0, 100.0);
    (colour_of(hue, sat, bri, alpha), (hue, sat, bri))
}

/// The scheme's colours as the chips show them: [`snipdeck_colour::scheme`] gives the base first, then
/// the tints and the shades; Tints & shades read light to dark (tint 4 .. 1, base, shade 1 .. 4).
#[must_use]
pub fn scheme_layout(which: Scheme, mut colours: Vec<Rgba8>) -> Vec<Rgba8> {
    if which == Scheme::TintsShades && colours.len() == 9 {
        colours[..5].reverse();
    }
    colours
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_translucent_colour_is_flattened_over_white() {
        assert_eq!(over_white(Rgba8::rgb(1, 2, 3)), Rgba8::rgb(1, 2, 3));
        let nearly_clear = Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 0x10,
        };
        assert_eq!(over_white(nearly_clear), Rgba8::rgb(255, 255, 255));
        assert_eq!(
            over_white(Rgba8 {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            }),
            Rgba8::rgb(255, 255, 255)
        );
        assert_eq!(
            over_white(Rgba8 {
                r: 0,
                g: 0,
                b: 0,
                a: 128
            }),
            Rgba8::rgb(127, 127, 127)
        );
    }

    #[test]
    fn a_slider_value_is_held_to_its_range_and_nan_is_zero() {
        assert_eq!(slider(-5.0, 359.999), 0.0);
        assert_eq!(slider(500.0, 359.999), 359.999);
        assert_eq!(slider(120.0, 359.999), 120.0);
        assert_eq!(slider(f32::NAN, 100.0), 0.0);
    }

    use super::*;

    fn st(c: Rgba8) -> Hsb {
        hsb(c, 0.0)
    }

    const BLUE: Rgba8 = Rgba8::rgb(58, 123, 213);

    fn texts(c: Rgba8, m: InputModel) -> Vec<String> {
        channels(c, m, st(c), None)
            .into_iter()
            .map(|c| c.text)
            .collect()
    }

    #[test]
    fn each_model_lists_its_channels_and_adds_alpha_only_when_not_opaque() {
        assert_eq!(texts(BLUE, InputModel::Hex), ["#3a7bd5"]);
        assert_eq!(texts(BLUE, InputModel::Rgb), ["58", "123", "213"]);
        assert_eq!(texts(BLUE, InputModel::Hsl), ["215", "65", "53"]);
        assert_eq!(texts(BLUE, InputModel::Hsb), ["215", "73", "84"]);
        assert_eq!(texts(BLUE, InputModel::Cmyk), ["73", "42", "0", "16"]);
        let half = Rgba8 { a: 128, ..BLUE };
        let rgb = channels(half, InputModel::Rgb, st(half), None);
        assert_eq!(
            rgb.last().map(|c| (c.label, c.text.as_str())),
            Some(("A", "50"))
        );
        assert_eq!(channels(half, InputModel::Hex, st(half), None).len(), 1);
    }

    #[test]
    fn a_grey_keeps_the_remembered_hue_in_the_hue_field() {
        let grey = Rgba8::rgb(128, 128, 128);
        assert_eq!(texts(grey, InputModel::Hsl)[0], "0");
        assert_eq!(
            channels(grey, InputModel::Hsb, (215.0, 0.0, 50.0), None)[0].text,
            "215"
        );
    }

    #[test]
    fn a_typed_value_changes_one_channel_and_clamps() {
        let c = commit(BLUE, InputModel::Rgb, 0, "255", st(BLUE), None)
            .map(|r| r.0)
            .unwrap();
        assert_eq!(c, Rgba8::rgb(255, 123, 213));
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 1, "999", st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
            Rgba8::rgb(58, 255, 213)
        );
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 2, "-4", st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
            Rgba8::rgb(58, 123, 0)
        );
        assert_eq!(
            commit(BLUE, InputModel::Hsb, 2, "100", st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
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
            commit(BLUE, InputModel::Rgb, 1, "green", st(BLUE), None).map(|r| r.0),
            Err(Reject::NotANumber("G"))
        );
        assert_eq!(
            commit(BLUE, InputModel::Hex, 0, "#12", st(BLUE), None).map(|r| r.0),
            Err(Reject::NotANumber("HEX"))
        );
        assert_eq!(
            commit(BLUE, InputModel::Rgb, 9, "1", st(BLUE), None).map(|r| r.0),
            Err(Reject::NoSuchChannel)
        );
    }

    #[test]
    fn the_hex_field_takes_any_colour_literal_and_a_percent_sign_is_allowed() {
        assert_eq!(
            commit(BLUE, InputModel::Hex, 0, "rgb(1, 2, 3)", st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
            Rgba8::rgb(1, 2, 3)
        );
        assert_eq!(
            commit(BLUE, InputModel::Hsl, 1, "50%", st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
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
        let translucent = commit(half, InputModel::Rgb, 3, "50", st(half), None).map(|r| r.0);
        assert_eq!(translucent, Err(Reject::NoSuchChannel));
        let half = Rgba8 { a: 128, ..BLUE };
        let c = commit(half, InputModel::Rgb, 3, "100", st(half), None)
            .map(|r| r.0)
            .unwrap();
        assert_eq!(c.a, 255);
    }

    #[test]
    fn stepping_moves_by_one_or_ten_and_the_hue_wraps() {
        assert_eq!(
            step(BLUE, InputModel::Rgb, 0, 1, st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
            Rgba8::rgb(59, 123, 213)
        );
        assert_eq!(
            step(BLUE, InputModel::Rgb, 0, -10, st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
            Rgba8::rgb(48, 123, 213)
        );
        let white = Rgba8::rgb(255, 255, 255);
        assert_eq!(
            step(white, InputModel::Rgb, 0, 1, st(white), None)
                .map(|r| r.0)
                .unwrap(),
            Rgba8::rgb(255, 255, 255)
        );
        let red = Rgba8::rgb(255, 0, 0);
        let back = step(red, InputModel::Hsb, 0, -1, st(red), None)
            .map(|r| r.0)
            .unwrap();
        assert_eq!(
            channels(back, InputModel::Hsb, st(back), None)[0].text,
            "359"
        );
        assert_eq!(
            step(BLUE, InputModel::Hex, 0, 1, st(BLUE), None)
                .map(|r| r.0)
                .unwrap(),
            BLUE
        );
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
        assert!(
            (mx - 0.7).abs() < 0.02 && (my - 0.3).abs() < 0.02,
            "{mx} {my}"
        );
    }

    #[test]
    fn a_hue_that_rounds_to_360_reads_0() {
        let c = Rgba8::rgb(255, 0, 2);
        for model in [InputModel::Hsl, InputModel::Hsb] {
            assert_eq!(channels(c, model, st(c), None)[0].text, "0", "{model:?}");
        }
    }

    #[test]
    fn arrow_keys_move_saturation_brightness_and_hue_by_steps() {
        let start = hsb(Rgba8::rgb(255, 0, 0), 0.0);
        let (c, _) = nudge(PickerStyle::Square, start, 0, -10, 255);
        assert!((hsb(c, 0.0).2 - 90.0).abs() < 0.5);
        let (_, moved) = nudge(PickerStyle::Wheel, start, -1, 0, 255);
        assert!((moved.0 - 359.0).abs() < 1e-3);
        let (c, _) = nudge(PickerStyle::Wheel, start, 0, -10, 255);
        assert!((hsb(c, 0.0).1 - 90.0).abs() < 0.5);
    }

    #[test]
    fn dragging_the_square_keeps_the_hue_and_reports_the_saturation() {
        let (c, st) = pick_hsb(PickerStyle::Square, 0.5, 0.25, (200.0, 10.0, 10.0), 255);
        assert!((st.0 - 200.0).abs() < 1e-3);
        assert!(
            st.1 > 40.0 && st.1 < 60.0 && st.2 > 70.0 && st.2 < 80.0,
            "{st:?}"
        );
        assert_eq!(c, colour_of(st.0, st.1, st.2, 255));
    }

    #[test]
    fn a_typed_hue_sticks_on_a_grey_and_a_step_moves_from_it() {
        let grey = Rgba8::rgb(128, 128, 128);
        for model in [InputModel::Hsl, InputModel::Hsb] {
            let (c, next, memo) = commit(grey, model, 0, "120", st(grey), None).unwrap();
            assert_eq!(c, grey, "{model:?}");
            assert_eq!(
                channels(c, model, next, memo.as_deref())[0].text,
                "120",
                "{model:?}"
            );
            let (c, up, memo) = step(c, model, 0, 1, next, memo.as_deref()).unwrap();
            assert_eq!(
                channels(c, model, up, memo.as_deref())[0].text,
                "121",
                "{model:?}"
            );
        }
        // Low chroma: the 8-bit colour cannot hold one degree, the field still advances.
        let low = Rgba8::rgb(58, 60, 62);
        let (c, a, _) = step(low, InputModel::Hsb, 0, 1, st(low), None).unwrap();
        let (c, b, _) = step(c, InputModel::Hsb, 0, 1, a, None).unwrap();
        assert!(b.0 - st(low).0 > 1.5, "{} -> {}", st(low).0, b.0);
        assert_eq!(
            channels(c, InputModel::Hsb, b, None)[0].text,
            whole(f64::from(b.0))
        );
    }

    #[test]
    fn brightness_to_zero_and_back_keeps_the_colour() {
        let start = st(BLUE);
        let (black, state, _) = commit(BLUE, InputModel::Hsb, 2, "0", start, None).unwrap();
        assert_eq!(black, Rgba8::rgb(0, 0, 0));
        assert!(
            (state.1 - start.1).abs() < 1e-3,
            "saturation kept at 0 brightness"
        );
        let (restored, ..) =
            commit(black, InputModel::Hsb, 2, &start.2.to_string(), state, None).unwrap();
        assert_eq!(restored, BLUE);
    }

    #[test]
    fn page_up_and_down_step_brightness_by_ten_in_the_square_and_the_wheel() {
        let start = (215.0, 73.0, 50.0);
        for _ in [PickerStyle::Square, PickerStyle::Wheel] {
            let (_, up) = page(start, 10, 255);
            assert!((up.2 - 60.0).abs() < 1e-3 && (up.0 - 215.0).abs() < 1e-3);
            assert!((up.1 - 73.0).abs() < 1e-3, "saturation stays");
            let (_, down) = page(start, -10, 255);
            assert!((down.2 - 40.0).abs() < 1e-3);
        }
        assert!(
            (page((0.0, 50.0, 95.0), 10, 255).1.2 - 100.0).abs() < 1e-3,
            "stops at 100"
        );
    }

    #[test]
    fn tints_and_shades_lay_out_light_to_dark_and_other_schemes_keep_their_order() {
        let tints = snipdeck_colour::scheme(BLUE, Scheme::TintsShades);
        let shown = scheme_layout(Scheme::TintsShades, tints.clone());
        assert_eq!(shown.len(), 9);
        assert_eq!(shown[4], BLUE, "the base sits in the middle");
        let (t1, t4) = (tints[1], tints[4]);
        let (s1, s4) = (tints[5], tints[8]);
        assert_eq!(
            (shown[0], shown[3], shown[5], shown[8]),
            (t4, t1, s1, s4),
            "tints 4 to 1, base, shades 1 to 4"
        );
        let luma = |c: Rgba8| u32::from(c.r) + u32::from(c.g) + u32::from(c.b);
        assert!(
            shown.windows(2).all(|w| luma(w[0]) >= luma(w[1])),
            "light to dark"
        );
        let pair = snipdeck_colour::scheme(BLUE, Scheme::Complementary);
        assert_eq!(scheme_layout(Scheme::Complementary, pair.clone()), pair);
    }

    /// Steps `field` of `model` `n` times the way the window does: the memo carried from one to the next.
    fn stepped(c: Rgba8, model: InputModel, field: usize, n: i32) -> (Rgba8, Vec<String>) {
        let (mut c, mut st, mut memo) = (c, hsb(c, 0.0), None::<Memo>);
        for _ in 0..n {
            (c, st, memo) = step(c, model, field, 1, st, memo.as_deref()).unwrap();
        }
        let texts = channels(c, model, st, memo.as_deref())
            .into_iter()
            .map(|c| c.text)
            .collect();
        (c, texts)
    }

    #[test]
    fn hsl_and_cmyk_fields_advance_where_the_8_bit_colour_cannot_hold_the_step() {
        // HSL hue of a near-grey: the colour barely changes, the field keeps counting.
        let (_, t) = stepped(Rgba8::rgb(58, 60, 62), InputModel::Hsl, 0, 3);
        assert_eq!(t[0], "213");
        // HSL saturation of a dark red.
        let (_, t) = stepped(Rgba8::rgb(30, 20, 20), InputModel::Hsl, 1, 3);
        assert_eq!(t[1], "23");
        // CMYK cyan of a dark blue-grey (K about 90).
        let (_, t) = stepped(Rgba8::rgb(13, 20, 25), InputModel::Cmyk, 0, 3);
        assert_eq!(t[0], "51");
    }

    #[test]
    fn typing_lightness_or_black_to_the_end_keeps_the_other_channels() {
        let (black, _, memo) = commit(BLUE, InputModel::Hsl, 2, "0", st(BLUE), None).unwrap();
        assert_eq!(black, Rgba8::rgb(0, 0, 0));
        let (again, ..) =
            commit(black, InputModel::Hsl, 2, "53", st(black), memo.as_deref()).unwrap();
        assert!(
            near(again, BLUE),
            "L back gives the colour, not a grey: {again:?}"
        );
        let (black, _, memo) = commit(BLUE, InputModel::Cmyk, 3, "100", st(BLUE), None).unwrap();
        assert_eq!(black, Rgba8::rgb(0, 0, 0));
        let (again, ..) =
            commit(black, InputModel::Cmyk, 3, "16", st(black), memo.as_deref()).unwrap();
        assert!(near(again, BLUE), "{again:?}");
    }

    /// Within one step of 8 bits per channel (a typed whole percent is not the exact value).
    fn near(a: Rgba8, b: Rgba8) -> bool {
        a.r.abs_diff(b.r) <= 1 && a.g.abs_diff(b.g) <= 1 && a.b.abs_diff(b.b) <= 1
    }
}
