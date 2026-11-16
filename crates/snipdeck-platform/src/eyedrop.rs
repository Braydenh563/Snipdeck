//! The screen eyedropper: what the overlay does with pixels and input, apart from the Win32 calls.
//!
//! [`Eyedropper`] is what the app asks for; Windows answers with
//! `windows::eyedrop::WinEyedropper` (freeze-frame of every monitor, one overlay per monitor,
//! **unverified on Windows**) and tests with [`FakeEyedropper`]. Off Windows the app has no
//! eyedropper and hides the button.
//!
//! Everything the overlay decides lives here so it is tested on Linux CI:
//!
//! - [`Shot`] is one monitor's frozen pixels (shared with the DIB the overlay paints from, never
//!   copied); [`sample`], [`average`] and [`loupe_cells`] read them in physical screen pixels
//!   (negative origins and gaps between monitors included); a monitor that could not be captured
//!   is a shot with no pixels and reads like a gap.
//! - [`loupe_rect`] places the magnifier next to the cursor and flips it at the monitor edges.
//! - [`Session`] turns [`Input`] (mouse, touch, keys, wheel, focus loss) into a [`Step`].
//! - The words come from [`EyedropTexts`] (the host translates them); nothing the user can read
//!   is written in this crate (a test reads the source to make sure).
//! - The hint bar's fade, the key tables, the cloak wait, the HDR decision and the other small
//!   rules of the Windows glue are here too, as plain functions.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::error::Error;
use crate::route::ScreenRect;

// --- pixels and shots -------------------------------------------------------------------------

/// The bytes of a [`Shot`]: either an owned buffer or memory somebody else owns (the Windows
/// backend points it at the DIB section the overlay paints from, so a 4K monitor is held once, not
/// twice). Cloning shares the bytes.
#[derive(Clone)]
pub struct Pixels(Rc<dyn AsRef<[u8]>>);

impl Pixels {
    /// Pixels read from `source`, which stays alive as long as any clone of this value.
    #[must_use]
    pub fn new(source: impl AsRef<[u8]> + 'static) -> Self {
        Self(Rc::new(source))
    }

    /// The bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        (*self.0).as_ref()
    }
}

impl From<Vec<u8>> for Pixels {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({} bytes)", self.bytes().len())
    }
}

impl PartialEq for Pixels {
    fn eq(&self, other: &Self) -> bool {
        self.bytes() == other.bytes()
    }
}

/// One monitor, frozen: its place on the virtual screen and its pixels.
///
/// The captures are per monitor so no buffer ever spans a gap or a negative origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    /// The monitor on the virtual screen, in physical pixels.
    pub bounds: ScreenRect,
    /// Physical pixels per logical pixel on this monitor (1.5 at 150 %).
    pub scale: f32,
    /// The monitor's values are approximate: it is in HDR mode, or Windows manages its colour
    /// (advanced colour) so what GDI hands over was converted.
    pub hdr: bool,
    /// The monitor was copied. `false`: the capture failed (a protected or disconnected-in-flight
    /// display); the overlay paints it grey with a note and it has no colours to pick.
    pub captured: bool,
    /// `bounds.w * bounds.h` pixels, top-down, four bytes each: blue, green, red, then a byte
    /// that is undefined (GDI leaves the alpha alone) and never read.
    pub bgra: Pixels,
}

impl Shot {
    /// A captured monitor.
    #[must_use]
    pub fn new(bounds: ScreenRect, scale: f32, hdr: bool, bgra: impl Into<Pixels>) -> Self {
        Self {
            bounds,
            scale,
            hdr,
            captured: true,
            bgra: bgra.into(),
        }
    }

    /// A monitor that could not be captured: no pixels, so nothing on it can be picked.
    #[must_use]
    pub fn uncaptured(bounds: ScreenRect, scale: f32) -> Self {
        Self {
            bounds,
            scale,
            hdr: false,
            captured: false,
            bgra: Pixels::from(Vec::new()),
        }
    }

    /// The offset of the pixel at `(x, y)` in [`Shot::bgra`], if this shot covers it (and holds
    /// the bytes for it).
    fn pixel_offset(&self, x: i32, y: i32) -> Option<usize> {
        if !self.captured {
            return None;
        }
        let dx = i64::from(x) - i64::from(self.bounds.x);
        let dy = i64::from(y) - i64::from(self.bounds.y);
        if dx < 0 || dy < 0 || dx >= i64::from(self.bounds.w) || dy >= i64::from(self.bounds.h) {
            return None;
        }
        let offset = usize::try_from((dy * i64::from(self.bounds.w) + dx) * 4).ok()?;
        (offset.checked_add(4)? <= self.bgra.bytes().len()).then_some(offset)
    }

    /// Whether this shot has the pixel at `(x, y)`.
    #[must_use]
    pub fn covers(&self, x: i32, y: i32) -> bool {
        self.pixel_offset(x, y).is_some()
    }

    /// Whether `(x, y)` is on this monitor, captured or not.
    #[must_use]
    pub fn contains(&self, x: i32, y: i32) -> bool {
        rect_contains(self.bounds, x, y)
    }
}

/// Whether `(x, y)` is inside `r`.
pub(crate) fn rect_contains(r: ScreenRect, x: i32, y: i32) -> bool {
    let (dx, dy) = (i64::from(x) - i64::from(r.x), i64::from(y) - i64::from(r.y));
    (0..i64::from(r.w)).contains(&dx) && (0..i64::from(r.h)).contains(&dy)
}

/// The shot that has the pixel at `(x, y)`, if any: the monitor under that point.
#[must_use]
pub fn shot_at(shots: &[Shot], x: i32, y: i32) -> Option<&Shot> {
    shots.iter().find(|shot| shot.covers(x, y))
}

/// The monitor under `(x, y)` even when it could not be captured.
#[must_use]
pub fn monitor_at(shots: &[Shot], x: i32, y: i32) -> Option<&Shot> {
    shots.iter().find(|shot| shot.contains(x, y))
}

/// The colour of the pixel at `(x, y)` (physical pixels on the virtual screen) as `[r, g, b]`,
/// or `None` in a gap between monitors (or on a monitor that could not be captured).
#[must_use]
pub fn sample(shots: &[Shot], x: i32, y: i32) -> Option<[u8; 3]> {
    shots.iter().find_map(|shot| {
        let i = shot.pixel_offset(x, y)?;
        let bytes = shot.bgra.bytes();
        Some([bytes[i + 2], bytes[i + 1], bytes[i]])
    })
}

/// How many pixels the sample is averaged over: one, or the 3 x 3 or 5 x 5 around the cursor
/// (photos, gradients and anti-aliased text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Aperture {
    /// The pixel under the cursor.
    #[default]
    One,
    /// 3 x 3.
    Three,
    /// 5 x 5.
    Five,
}

impl Aperture {
    /// Pixels on a side.
    #[must_use]
    pub fn side(self) -> u32 {
        match self {
            Self::One => 1,
            Self::Three => 3,
            Self::Five => 5,
        }
    }

    /// The next one when the key is pressed: 1, 3, 5, then 1 again.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::One => Self::Three,
            Self::Three => Self::Five,
            Self::Five => Self::One,
        }
    }
}

/// The colour at `(x, y)` averaged over the `aperture` square around it: each channel's mean of
/// the pixels some shot has (the part of the square in a gap or off the screen is left out),
/// rounded to the nearest. `None` when the pixel itself is in a gap.
#[must_use]
pub fn average(shots: &[Shot], x: i32, y: i32, aperture: Aperture) -> Option<[u8; 3]> {
    let centre = sample(shots, x, y)?;
    let n = aperture.side();
    if n == 1 {
        return Some(centre);
    }
    let half = i64::from(n / 2);
    let (mut sum, mut count) = ([0_u64; 3], 0_u64);
    for row in 0..i64::from(n) {
        for col in 0..i64::from(n) {
            let at = |origin: i32, step: i64| i32::try_from(i64::from(origin) + step - half).ok();
            if let (Some(px), Some(py)) = (at(x, col), at(y, row))
                && let Some(rgb) = sample(shots, px, py)
            {
                for (total, channel) in sum.iter_mut().zip(rgb) {
                    *total += u64::from(channel);
                }
                count += 1;
            }
        }
    }
    let mean = |total: u64| u8::try_from((total + count / 2) / count.max(1)).unwrap_or(u8::MAX);
    Some([mean(sum[0]), mean(sum[1]), mean(sum[2])])
}

/// The `n` x `n` pixels around `(x, y)`, row-major, the pixel itself in the middle cell. `n`
/// must be odd; a pixel in a gap (or off the screen) is `None`.
#[must_use]
pub fn loupe_cells(shots: &[Shot], x: i32, y: i32, n: u32) -> Vec<Option<[u8; 3]>> {
    let half = i64::from(n / 2);
    let mut cells = Vec::with_capacity((n as usize).saturating_mul(n as usize));
    for row in 0..i64::from(n) {
        for col in 0..i64::from(n) {
            let at = |origin: i32, step: i64| i32::try_from(i64::from(origin) + step - half).ok();
            cells.push(match (at(x, col), at(y, row)) {
                (Some(px), Some(py)) => sample(shots, px, py),
                _ => None,
            });
        }
    }
    cells
}

// --- the magnifier's geometry -----------------------------------------------------------------

/// Logical pixels each sampled pixel is magnified to, smallest to largest. The default is 8, and
/// the smallest is 8 too: at 100 % a cell must be at least [`MIN_CELL_PX`] physical pixels or the
/// frame around the picked one has no room to show it.
pub const ZOOMS: [u32; 4] = [8, 12, 16, 24];

/// No magnified cell is smaller than this many physical pixels (at a scale of 1 or more).
pub const MIN_CELL_PX: u32 = 8;

const _: () = assert!(ZOOMS[0] >= MIN_CELL_PX);

/// The magnifier shows `CELLS` x `CELLS` sampled pixels.
pub const CELLS: u32 = 11;

/// Index of the default zoom (8) in [`ZOOMS`].
const DEFAULT_ZOOM: usize = 0;

/// Logical padding around the magnified grid (room for the frame around the picked cell).
const PAD: u32 = 4;

/// Logical height of the readout bar under the grid (swatch and two lines of text) at a text
/// scale of 1.
const READOUT: u32 = 48;

/// Logical gap between the cursor and the magnifier.
const GAP: u32 = 24;

/// The narrowest box, in logical pixels at a text scale of 1: the readout (swatch and a default
/// colour format such as `rgb(255, 255, 255)`) needs this much.
const MIN_WIDTH: u32 = 160;

/// Logical width of the touch "Pick" button at a text scale of 1.
const BUTTON_W: u32 = 64;
/// Logical height of the touch "Pick" button at a text scale of 1.
const BUTTON_H: u32 = 36;
/// Logical margin around the button inside the readout bar.
const BUTTON_MARGIN: u32 = 6;

/// A touch within this many logical pixels of the placed loupe's cursor confirms the pick.
const TAP_SLOP: u32 = 28;

/// The text scale (Windows Accessibility > Text size, up to 225 %) the loupe copes with.
const MAX_TEXT_SCALE: f32 = 3.0;

/// `logical` pixels at `scale`, rounded.
pub(crate) fn scaled(logical: u32, scale: f32) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a few hundred logical pixels times a scale of 1 to 5, rounded and never negative"
    )]
    let px = (logical as f32 * scale).round().max(0.0) as u32;
    px
}

/// `v` as a signed coordinate, saturating.
pub(crate) fn signed(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// The text scale the host passed, made safe to multiply with: 1 to 3, and 1 for NaN.
#[must_use]
pub fn clamp_text_scale(text_scale: f32) -> f32 {
    if text_scale.is_finite() {
        text_scale.clamp(1.0, MAX_TEXT_SCALE)
    } else {
        1.0
    }
}

/// The (negative) pixel height of a font of `points` at the monitor `scale` and the system
/// `text_scale` (1 pt is 4/3 px at 96 DPI).
#[cfg(any(windows, test))]
pub(crate) fn font_height_px(points: u32, scale: f32, text_scale: f32) -> i32 {
    -signed(scaled(points * 4 / 3, scale * clamp_text_scale(text_scale)))
}

/// What the loupe's size depends on besides the cursor and the monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoupeLayout {
    /// Logical pixels per sampled pixel (one of [`ZOOMS`]).
    pub zoom: u32,
    /// The system text scale (1 = 100 %): the readout bar and its minimum width grow with it.
    pub text_scale: f32,
    /// A touch "Pick" button is in the readout bar, which makes the box wider.
    pub button: bool,
}

impl LoupeLayout {
    /// The layout at `zoom`, text scale 1, no button.
    #[must_use]
    pub fn new(zoom: u32) -> Self {
        Self {
            zoom,
            text_scale: 1.0,
            button: false,
        }
    }

    /// Logical height of the readout bar.
    fn readout(&self) -> u32 {
        scaled(READOUT, clamp_text_scale(self.text_scale))
    }

    /// Logical minimum width.
    fn min_width(&self) -> u32 {
        let ts = clamp_text_scale(self.text_scale);
        let base = scaled(MIN_WIDTH, ts);
        if self.button {
            base + scaled(BUTTON_W + BUTTON_MARGIN, ts)
        } else {
            base
        }
    }
}

/// The box of the magnifier for a cursor at `cursor`, in physical pixels on the virtual screen.
///
/// The box is `CELLS * zoom + 2 * 4` logical pixels wide (never narrower than the readout needs,
/// which grows with the text scale) and the readout bar (48 at text scale 1) taller than that,
/// times the monitor's `scale`. It sits 24 logical pixels right of and below the cursor, flipped
/// to the left / above when it would leave `monitor`, and kept inside `monitor` if it does not
/// fit either way.
#[must_use]
pub fn loupe_rect(
    cursor: (i32, i32),
    monitor: ScreenRect,
    scale: f32,
    layout: LoupeLayout,
) -> ScreenRect {
    let grid = CELLS.saturating_mul(layout.zoom).saturating_add(2 * PAD);
    let w = scaled(grid.max(layout.min_width()), scale);
    let h = scaled(grid.saturating_add(layout.readout()), scale);
    let gap = signed(scaled(GAP, scale));
    let (left, top) = (i64::from(monitor.x), i64::from(monitor.y));
    let (right, bottom) = (left + i64::from(monitor.w), top + i64::from(monitor.h));
    let place = |at: i32, size: u32, low: i64, high: i64| -> i32 {
        let size = i64::from(size);
        let after = i64::from(at) + i64::from(gap);
        let start = if after + size > high {
            i64::from(at) - i64::from(gap) - size
        } else {
            after
        };
        // Inside the monitor whichever way it went (`high - size` first so that a box larger
        // than the monitor sticks to the low edge).
        let start = start.min(high - size).max(low);
        i32::try_from(start).unwrap_or(at)
    };
    ScreenRect {
        x: place(cursor.0, w, left, right),
        y: place(cursor.1, h, top, bottom),
        w,
        h,
    }
}

/// The touch "Pick" button inside the readout bar of `loupe`: right-aligned, vertically centred.
#[must_use]
pub fn pick_button_rect(loupe: ScreenRect, scale: f32, text_scale: f32) -> ScreenRect {
    let ts = clamp_text_scale(text_scale);
    let (w, h) = (scaled(BUTTON_W, scale * ts), scaled(BUTTON_H, scale * ts));
    let margin = signed(scaled(BUTTON_MARGIN, scale));
    let bar = scaled(READOUT, scale * ts);
    let bar_top = i64::from(loupe.y) + i64::from(loupe.h) - i64::from(bar);
    let x = i64::from(loupe.x) + i64::from(loupe.w) - i64::from(w) - i64::from(margin);
    let y = bar_top + (i64::from(bar) - i64::from(h)) / 2;
    ScreenRect {
        x: i32::try_from(x).unwrap_or(loupe.x),
        y: i32::try_from(y).unwrap_or(loupe.y),
        w,
        h,
    }
}

/// One ring of the frame around the picked cell: the rectangle's outside edge and the thickness
/// inward, white or black.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRing {
    /// Left of the ring's outer edge.
    pub x: i32,
    /// Top of the ring's outer edge.
    pub y: i32,
    /// Outer width.
    pub w: i32,
    /// Outer height.
    pub h: i32,
    /// Ring thickness, inward from the outer edge.
    pub thick: i32,
    /// White (the inner ring), otherwise black.
    pub white: bool,
}

/// The frame around the picked block (`side` pixels square at `(x, y)`: one cell, or the 3 x 3 or
/// 5 x 5 that is averaged): a 1 px black ring around a 2 px white ring that touches the block,
/// both **outside** the block, so the pixel being picked is never covered. The rings lie in the
/// padding of the grid (at least 4 px at any scale).
#[must_use]
pub fn pick_frame(x: i32, y: i32, side: i32) -> [FrameRing; 2] {
    [
        FrameRing {
            x: x - 3,
            y: y - 3,
            w: side + 6,
            h: side + 6,
            thick: 1,
            white: false,
        },
        FrameRing {
            x: x - 2,
            y: y - 2,
            w: side + 4,
            h: side + 4,
            thick: 2,
            white: true,
        },
    ]
}

// --- keys ---------------------------------------------------------------------------------------

/// A key the overlay answers to. Also what [`EyedropNote::KeysNotRegistered`] names, so the host
/// can say which key another app owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyName {
    /// Esc: ends the run.
    Escape,
    /// Enter: picks (Shift+Enter collects).
    Enter,
    /// Space: picks (Shift+Space collects).
    Space,
    /// Left arrow (Shift or Ctrl: ten pixels).
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// `+` (or `=`, or the numpad plus): zoom in.
    Plus,
    /// `-` (or the numpad minus): zoom out.
    Minus,
    /// `A`: cycles the averaged aperture 1 x 1, 3 x 3, 5 x 5.
    Aperture,
}

/// Virtual-key codes the overlay maps (plain numbers so this module needs no Windows types).
#[cfg(any(windows, test))]
pub(crate) mod vk {
    /// Esc.
    pub const ESCAPE: u16 = 0x1B;
    /// Enter.
    pub const RETURN: u16 = 0x0D;
    /// Space.
    pub const SPACE: u16 = 0x20;
    /// Left arrow.
    pub const LEFT: u16 = 0x25;
    /// Up arrow.
    pub const UP: u16 = 0x26;
    /// Right arrow.
    pub const RIGHT: u16 = 0x27;
    /// Down arrow.
    pub const DOWN: u16 = 0x28;
    /// The letter A.
    pub const A: u16 = 0x41;
    /// The numpad plus.
    pub const ADD: u16 = 0x6B;
    /// The numpad minus.
    pub const SUBTRACT: u16 = 0x6D;
    /// `=` / `+`.
    pub const OEM_PLUS: u16 = 0xBB;
    /// `-`.
    pub const OEM_MINUS: u16 = 0xBD;
}

/// The key a virtual-key code stands for, if the overlay uses it.
#[cfg(any(windows, test))]
pub(crate) fn key_for_vk(code: u16) -> Option<KeyName> {
    Some(match code {
        vk::ESCAPE => KeyName::Escape,
        vk::RETURN => KeyName::Enter,
        vk::SPACE => KeyName::Space,
        vk::LEFT => KeyName::Left,
        vk::RIGHT => KeyName::Right,
        vk::UP => KeyName::Up,
        vk::DOWN => KeyName::Down,
        vk::OEM_PLUS | vk::ADD => KeyName::Plus,
        vk::OEM_MINUS | vk::SUBTRACT => KeyName::Minus,
        vk::A => KeyName::Aperture,
        _ => return None,
    })
}

/// What a key does, with the modifiers held: Esc cancels; Enter and Space pick, or collect with
/// Shift; arrows nudge one pixel, ten with Shift or Ctrl; `+` and `-` zoom (Shift is ignored: `+`
/// is Shift and `=` on most layouts); `A` cycles the aperture.
#[cfg(any(windows, test))]
pub(crate) fn key_input(key: KeyName, shift: bool, ctrl: bool) -> Input {
    let nav = |nav| Input::Key {
        nav,
        fast: shift || ctrl,
    };
    match key {
        KeyName::Escape => Input::Cancel,
        KeyName::Enter | KeyName::Space if shift => Input::Collect,
        KeyName::Enter | KeyName::Space => Input::Pick,
        KeyName::Left => nav(Nav::Left),
        KeyName::Right => nav(Nav::Right),
        KeyName::Up => nav(Nav::Up),
        KeyName::Down => nav(Nav::Down),
        KeyName::Plus => Input::Wheel(1),
        KeyName::Minus => Input::Wheel(-1),
        KeyName::Aperture => Input::CycleAperture,
    }
}

/// A key the overlay registers as a global hotkey when it could not take the foreground.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FallbackKey {
    /// The hotkey id (per window, so it cannot clash with the app's own).
    pub id: i32,
    /// The virtual-key code.
    pub vk: u16,
    /// Shift is part of the chord.
    pub shift: bool,
    /// Ctrl is part of the chord.
    pub ctrl: bool,
}

#[cfg(any(windows, test))]
const fn fallback(id: i32, vk: u16, shift: bool, ctrl: bool) -> FallbackKey {
    FallbackKey { id, vk, shift, ctrl }
}

/// Every key of the focused path, in the fallback path too: the same table drives
/// `RegisterHotKey` and [`fallback_input`], so a key cannot work in one path and not the other.
#[cfg(any(windows, test))]
pub(crate) const FALLBACK_KEYS: [FallbackKey; 23] = [
    fallback(1, vk::ESCAPE, false, false),
    fallback(2, vk::RETURN, false, false),
    fallback(3, vk::SPACE, false, false),
    fallback(4, vk::LEFT, false, false),
    fallback(5, vk::RIGHT, false, false),
    fallback(6, vk::UP, false, false),
    fallback(7, vk::DOWN, false, false),
    fallback(8, vk::LEFT, true, false),
    fallback(9, vk::RIGHT, true, false),
    fallback(10, vk::UP, true, false),
    fallback(11, vk::DOWN, true, false),
    fallback(12, vk::LEFT, false, true),
    fallback(13, vk::RIGHT, false, true),
    fallback(14, vk::UP, false, true),
    fallback(15, vk::DOWN, false, true),
    fallback(16, vk::OEM_PLUS, false, false),
    fallback(17, vk::OEM_PLUS, true, false),
    fallback(18, vk::ADD, false, false),
    fallback(19, vk::OEM_MINUS, false, false),
    fallback(20, vk::SUBTRACT, false, false),
    fallback(21, vk::A, false, false),
    fallback(22, vk::RETURN, true, false),
    fallback(23, vk::SPACE, true, false),
];

/// The input a hotkey id of [`FALLBACK_KEYS`] stands for.
#[cfg(any(windows, test))]
pub(crate) fn fallback_input(id: i32) -> Option<Input> {
    let key = FALLBACK_KEYS.iter().find(|k| k.id == id)?;
    Some(key_input(key_for_vk(key.vk)?, key.shift, key.ctrl))
}

/// The distinct keys behind `failed` hotkey ids, in table order (what a note names).
#[cfg(any(windows, test))]
pub(crate) fn keys_of_ids(failed: &[i32]) -> Vec<KeyName> {
    let mut keys = Vec::new();
    for entry in FALLBACK_KEYS.iter().filter(|k| failed.contains(&k.id)) {
        if let Some(key) = key_for_vk(entry.vk)
            && !keys.contains(&key)
        {
            keys.push(key);
        }
    }
    keys
}

/// Whether the extra info of a mouse message says a finger made it (the message is the mouse
/// message Windows synthesises for touch): `(info & 0xFFFFFF80) == 0xFF515780`.
#[cfg(any(windows, test))]
pub(crate) fn is_touch_extra_info(info: usize) -> bool {
    info & 0xFFFF_FF80 == 0xFF51_5780
}

// --- the session --------------------------------------------------------------------------------

/// A direction of an arrow key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
}

/// Something that happened to the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// The mouse is now at this physical position.
    Moved(i32, i32),
    /// An arrow key; `fast` (Shift or Ctrl held) moves ten pixels instead of one.
    Key {
        /// Which arrow.
        nav: Nav,
        /// Shift or Ctrl is held.
        fast: bool,
    },
    /// Click, Enter or Space: picks the colour under the cursor and ends the run.
    Pick,
    /// Shift+click, Shift+Enter or Shift+Space: keeps the colour under the cursor and goes on.
    Collect,
    /// Right click or Esc: ends the run (with the colours collected so far, if any).
    Cancel,
    /// The wheel turned (or `+` / `-` was pressed): positive zooms in, negative out.
    Wheel(i32),
    /// The aperture key: 1 x 1, 3 x 3, 5 x 5.
    CycleAperture,
    /// A finger went down here (physical pixels). The first touch places the loupe; a second near
    /// the loupe's cursor, or on its Pick button, confirms.
    TouchDown(i32, i32),
    /// The finger moved here.
    TouchMove(i32, i32),
    /// The finger lifted here.
    TouchUp(i32, i32),
    /// Another app took the focus.
    LostFocus,
    /// A monitor was added, removed or changed: the frozen shots no longer match the screen.
    DisplayChanged,
    /// The overlay has been open too long.
    TimedOut,
}

/// What the overlay does about an [`Input`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Draw the magnifier again (the old and the new place).
    Redraw,
    /// Put the mouse here (physical pixels), then draw again. [`Session::cursor`] already holds
    /// the new place.
    MoveCursor(i32, i32),
    /// The run is over, with this outcome.
    Finished(EyedropOutcome),
    /// Nothing changed.
    Nothing,
}

/// What a finger that is down is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grab {
    /// No finger down.
    None,
    /// On the placed cursor or the Pick button: lifting confirms.
    Confirm,
    /// On the loupe, off the button: nothing happens.
    Ignore,
    /// Placing the loupe: it follows the finger.
    Place,
}

/// The most colours one run keeps with Shift+click.
pub const MAX_COLLECTED: usize = 64;

/// The state of one eyedropper run: where the cursor is, how far the magnifier is zoomed, the
/// averaging aperture, the colours collected with Shift+click and the touch placement.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    cursor: (i32, i32),
    zoom: usize,
    /// `(left, top, right, bottom)` of every shot together; `None` without shots.
    union: Option<(i64, i64, i64, i64)>,
    aperture: Aperture,
    collected: Vec<[u8; 3]>,
    collected_hdr: bool,
    text_scale: f32,
    /// A finger placed the loupe: the Pick button shows and a second touch confirms.
    placed: bool,
    grab: Grab,
}

impl Session {
    /// A run over `shots` with the mouse at `cursor`, the default zoom (8), a single pixel
    /// sample and a text scale of 1.
    #[must_use]
    pub fn new(shots: &[Shot], cursor: (i32, i32)) -> Self {
        let union = shots
            .iter()
            .fold(None, |acc: Option<(i64, i64, i64, i64)>, shot| {
                let (l, t) = (i64::from(shot.bounds.x), i64::from(shot.bounds.y));
                let (r, b) = (l + i64::from(shot.bounds.w), t + i64::from(shot.bounds.h));
                Some(match acc {
                    None => (l, t, r, b),
                    Some((al, at, ar, ab)) => (al.min(l), at.min(t), ar.max(r), ab.max(b)),
                })
            });
        Self {
            cursor,
            zoom: DEFAULT_ZOOM,
            union,
            aperture: Aperture::One,
            collected: Vec::new(),
            collected_hdr: false,
            text_scale: 1.0,
            placed: false,
            grab: Grab::None,
        }
    }

    /// The same run with the system text scale the loupe's readout is laid out for.
    #[must_use]
    pub fn with_text_scale(mut self, text_scale: f32) -> Self {
        self.text_scale = clamp_text_scale(text_scale);
        self
    }

    /// Where the cursor is, in physical pixels.
    #[must_use]
    pub fn cursor(&self) -> (i32, i32) {
        self.cursor
    }

    /// Logical pixels per sampled pixel now (one of [`ZOOMS`]).
    #[must_use]
    pub fn zoom(&self) -> u32 {
        ZOOMS[self.zoom]
    }

    /// The aperture the colour is averaged over.
    #[must_use]
    pub fn aperture(&self) -> Aperture {
        self.aperture
    }

    /// The colours collected with Shift+click so far, oldest first.
    #[must_use]
    pub fn collected(&self) -> &[[u8; 3]] {
        &self.collected
    }

    /// The colour under the cursor (averaged over the aperture), if it is on a monitor.
    #[must_use]
    pub fn colour(&self, shots: &[Shot]) -> Option<[u8; 3]> {
        average(shots, self.cursor.0, self.cursor.1, self.aperture)
    }

    /// The loupe's layout now.
    #[must_use]
    pub fn layout(&self) -> LoupeLayout {
        LoupeLayout {
            zoom: self.zoom(),
            text_scale: self.text_scale,
            button: self.placed,
        }
    }

    /// The magnifier's box now, if the cursor is on a monitor.
    #[must_use]
    pub fn loupe(&self, shots: &[Shot]) -> Option<ScreenRect> {
        let shot = monitor_at(shots, self.cursor.0, self.cursor.1)?;
        Some(loupe_rect(
            self.cursor,
            shot.bounds,
            shot.scale,
            self.layout(),
        ))
    }

    /// The touch "Pick" button's box, once a finger has placed the loupe.
    #[must_use]
    pub fn pick_button(&self, shots: &[Shot]) -> Option<ScreenRect> {
        if !self.placed {
            return None;
        }
        let shot = monitor_at(shots, self.cursor.0, self.cursor.1)?;
        Some(pick_button_rect(
            self.loupe(shots)?,
            shot.scale,
            self.text_scale,
        ))
    }

    /// Applies one input.
    ///
    /// - `Moved`: the cursor follows the mouse (`Redraw`, or `Nothing` if it did not move); a
    ///   mouse after a touch ends the touch placement.
    /// - `Key`: moves one pixel, ten when `fast`, to a pixel some shot has: a step that would land
    ///   in a gap or off the screen is shortened until it fits, and is `Nothing` if even one pixel
    ///   does not (the cursor never goes into a gap by keyboard).
    /// - `Pick`: `Finished(Picked)` with the colour under the cursor and the colours collected
    ///   before it (`Nothing` in a gap).
    /// - `Collect`: keeps the colour under the cursor and goes on (`Nothing` in a gap, or past
    ///   [`MAX_COLLECTED`]).
    /// - `Cancel`, `LostFocus`: `Finished(Cancelled)`; `DisplayChanged`, `TimedOut`:
    ///   `Finished(Ended(..))`. With colours collected every one of these delivers them as
    ///   `Picked` (the last one collected is `rgb`) so nothing the user kept is lost.
    /// - `Wheel`: one step along [`ZOOMS`] (`Redraw`, or `Nothing` at the end of the list).
    /// - `CycleAperture`: the next aperture (`Redraw`).
    /// - `TouchDown` / `TouchMove` / `TouchUp`: the first touch places the loupe; a second near
    ///   its cursor or on its Pick button, lifted, confirms; one elsewhere places it again.
    pub fn on(&mut self, shots: &[Shot], input: Input) -> Step {
        match input {
            Input::Moved(x, y) => {
                if self.cursor == (x, y) && !self.placed {
                    Step::Nothing
                } else {
                    self.cursor = (x, y);
                    self.placed = false;
                    Step::Redraw
                }
            }
            Input::Key { nav, fast } => self.nudge(shots, nav, if fast { 10 } else { 1 }),
            Input::Pick => self.pick(shots),
            Input::Collect => self.collect(shots),
            Input::Cancel | Input::LostFocus => self.end(EyedropOutcome::Cancelled),
            Input::DisplayChanged => self.end(EyedropOutcome::Ended(EndReason::DisplayChanged)),
            Input::TimedOut => self.end(EyedropOutcome::Ended(EndReason::TimedOut)),
            Input::Wheel(delta) => {
                let next = match delta.signum() {
                    1 => (self.zoom + 1).min(ZOOMS.len() - 1),
                    -1 => self.zoom.saturating_sub(1),
                    _ => self.zoom,
                };
                if next == self.zoom {
                    Step::Nothing
                } else {
                    self.zoom = next;
                    Step::Redraw
                }
            }
            Input::CycleAperture => {
                self.aperture = self.aperture.next();
                Step::Redraw
            }
            Input::TouchDown(x, y) => self.touch_down(shots, (x, y)),
            Input::TouchMove(x, y) => {
                if self.grab == Grab::Place && self.cursor != (x, y) {
                    self.cursor = (x, y);
                    Step::Redraw
                } else {
                    Step::Nothing
                }
            }
            Input::TouchUp(..) => self.touch_up(shots),
        }
    }

    fn touch_down(&mut self, shots: &[Shot], at: (i32, i32)) -> Step {
        if self.placed {
            let on_button = self
                .pick_button(shots)
                .is_some_and(|b| rect_contains(b, at.0, at.1));
            let scale = monitor_at(shots, self.cursor.0, self.cursor.1).map_or(1.0, |s| s.scale);
            let slop = i64::from(scaled(TAP_SLOP, scale));
            let near = (i64::from(at.0) - i64::from(self.cursor.0)).abs() <= slop
                && (i64::from(at.1) - i64::from(self.cursor.1)).abs() <= slop;
            if on_button || near {
                self.grab = Grab::Confirm;
                return Step::Nothing;
            }
            if self
                .loupe(shots)
                .is_some_and(|l| rect_contains(l, at.0, at.1))
            {
                self.grab = Grab::Ignore;
                return Step::Nothing;
            }
        }
        self.grab = Grab::Place;
        self.placed = false;
        self.cursor = at;
        Step::Redraw
    }

    fn touch_up(&mut self, shots: &[Shot]) -> Step {
        let grab = std::mem::replace(&mut self.grab, Grab::None);
        match grab {
            Grab::Confirm => self.pick(shots),
            Grab::Place => {
                self.placed = true;
                Step::Redraw
            }
            Grab::Ignore | Grab::None => Step::Nothing,
        }
    }

    fn pick(&mut self, shots: &[Shot]) -> Step {
        let Some(rgb) = self.colour(shots) else {
            return Step::Nothing;
        };
        let hdr = shot_at(shots, self.cursor.0, self.cursor.1).is_some_and(|s| s.hdr);
        Step::Finished(EyedropOutcome::Picked {
            rgb,
            hdr: hdr || self.collected_hdr,
            also: std::mem::take(&mut self.collected),
        })
    }

    fn collect(&mut self, shots: &[Shot]) -> Step {
        if self.collected.len() >= MAX_COLLECTED {
            return Step::Nothing;
        }
        let Some(rgb) = self.colour(shots) else {
            return Step::Nothing;
        };
        self.collected_hdr |= shot_at(shots, self.cursor.0, self.cursor.1).is_some_and(|s| s.hdr);
        self.collected.push(rgb);
        Step::Redraw
    }

    /// The run ends without a pick: `otherwise` is the outcome unless colours were collected.
    fn end(&mut self, otherwise: EyedropOutcome) -> Step {
        match self.collected.pop() {
            Some(rgb) => Step::Finished(EyedropOutcome::Picked {
                rgb,
                hdr: self.collected_hdr,
                also: std::mem::take(&mut self.collected),
            }),
            None => Step::Finished(otherwise),
        }
    }

    fn nudge(&mut self, shots: &[Shot], nav: Nav, pixels: i32) -> Step {
        let (dx, dy) = match nav {
            Nav::Left => (-1, 0),
            Nav::Right => (1, 0),
            Nav::Up => (0, -1),
            Nav::Down => (0, 1),
        };
        for n in (1..=pixels).rev() {
            let (Some(x), Some(y)) = (
                self.cursor.0.checked_add(dx * n),
                self.cursor.1.checked_add(dy * n),
            ) else {
                continue;
            };
            let inside = self.union.is_some_and(|(l, t, r, b)| {
                (l..r).contains(&i64::from(x)) && (t..b).contains(&i64::from(y))
            });
            if inside && shot_at(shots, x, y).is_some() {
                self.cursor = (x, y);
                return Step::MoveCursor(x, y);
            }
        }
        Step::Nothing
    }
}

// --- words --------------------------------------------------------------------------------------

/// The `#rrggbb` text of a colour, lower case.
pub(crate) fn hex_text(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// The words and settings the overlay uses, all from the host so they are translated and follow
/// the user's settings. Nothing the user can read is written in this crate.
#[derive(Clone)]
pub struct EyedropTexts {
    /// The hint bar, for example "Click or Enter picks, Esc cancels, arrows move, A averages".
    pub hint: String,
    /// "HDR is on for this screen: values are approximate", shown on an HDR monitor and kept
    /// when the hint has faded.
    pub hdr_note: String,
    /// The window title screen readers hear, with `{}` where the colour's hex goes: for example
    /// "Eyedropper, {}". Without a colour (the cursor is in a gap) the text before `{}` is used,
    /// trailing punctuation cut off.
    pub title: String,
    /// The loupe's first line: the colour in the user's default copy format.
    /// The hex stays as the second line (unless this already is the hex).
    pub readout: Arc<dyn Fn([u8; 3]) -> String + Send + Sync>,
    /// The system text scale (Windows Accessibility > Text size, 1.0 = 100 %): the loupe's text
    /// and readout bar grow with it.
    pub text_scale: f32,
    /// The touch "Pick" button's label. Empty: a check mark.
    pub pick_label: String,
    /// The note painted on a monitor that could not be captured ("This screen could not be
    /// captured").
    pub unavailable: String,
}

/// The shared hex formatter, so two default texts compare equal.
fn default_readout() -> Arc<dyn Fn([u8; 3]) -> String + Send + Sync> {
    static HEX: std::sync::OnceLock<Arc<dyn Fn([u8; 3]) -> String + Send + Sync>> =
        std::sync::OnceLock::new();
    Arc::clone(HEX.get_or_init(|| Arc::new(hex_text)))
}

impl Default for EyedropTexts {
    /// No words, the hex as the readout, the title just the hex, text scale 1.
    fn default() -> Self {
        Self {
            hint: String::new(),
            hdr_note: String::new(),
            title: "{}".to_owned(),
            readout: default_readout(),
            text_scale: 1.0,
            pick_label: String::new(),
            unavailable: String::new(),
        }
    }
}

impl std::fmt::Debug for EyedropTexts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EyedropTexts")
            .field("hint", &self.hint)
            .field("hdr_note", &self.hdr_note)
            .field("title", &self.title)
            .field("text_scale", &self.text_scale)
            .field("pick_label", &self.pick_label)
            .field("unavailable", &self.unavailable)
            .finish_non_exhaustive()
    }
}

impl PartialEq for EyedropTexts {
    /// The readout formatters are equal when they are the same function object (a clone).
    fn eq(&self, other: &Self) -> bool {
        self.hint == other.hint
            && self.hdr_note == other.hdr_note
            && self.title == other.title
            && self.text_scale.to_bits() == other.text_scale.to_bits()
            && self.pick_label == other.pick_label
            && self.unavailable == other.unavailable
            && Arc::ptr_eq(&self.readout, &other.readout)
    }
}

impl Eq for EyedropTexts {}

impl EyedropTexts {
    /// The loupe's first line for `rgb`: the colour in the user's default format.
    #[must_use]
    pub fn format(&self, rgb: [u8; 3]) -> String {
        (self.readout)(rgb)
    }
}

/// The two lines of the loupe's readout: the colour in the default format (`-` in a gap) and a
/// second line with the hex (left out when the first line already is the hex), the aperture as
/// `3x3` and, once colours were collected, `+N`.
#[cfg(any(windows, test))]
pub(crate) fn readout_lines(
    texts: &EyedropTexts,
    rgb: Option<[u8; 3]>,
    aperture: Aperture,
    collected: usize,
) -> (String, String) {
    let first = rgb.map_or_else(|| "-".to_owned(), |c| texts.format(c));
    let mut second = Vec::new();
    if let Some(c) = rgb {
        let hex = hex_text(c);
        if !first.eq_ignore_ascii_case(&hex) {
            second.push(hex);
        }
    }
    let side = aperture.side();
    second.push(format!("{side}x{side}"));
    if collected > 0 {
        second.push(format!("+{collected}"));
    }
    (first, second.join("  "))
}

/// The window title the overlay carries for screen readers: the host's pattern with the hex in
/// it. Without a colour, the pattern up to `{}` with trailing punctuation and spaces cut off; a
/// pattern with nothing in front of `{}` gives the product name.
#[cfg(any(windows, test))]
pub(crate) fn window_title(texts: &EyedropTexts, rgb: Option<[u8; 3]>) -> String {
    let pattern = texts.title.as_str();
    match rgb {
        Some(rgb) if pattern.contains("{}") => pattern.replacen("{}", &hex_text(rgb), 1),
        Some(rgb) => format!("{pattern} {}", hex_text(rgb)),
        None => {
            let head = pattern.split("{}").next().unwrap_or_default();
            let head = head.trim_end_matches(|c: char| !c.is_alphanumeric());
            if head.is_empty() {
                "Snipdeck".to_owned()
            } else {
                head.to_owned()
            }
        }
    }
}

// --- the hint bar -------------------------------------------------------------------------------

/// How long the hint stays fully visible.
pub const HINT_HOLD: Duration = Duration::from_secs(2);
/// How long it takes to fade out after that.
pub const HINT_FADE: Duration = Duration::from_millis(400);
/// The bar moves to the other edge when the cursor is within this many logical pixels of it.
const HINT_NEAR: u32 = 80;
/// Logical distance of the bar from its edge of the monitor.
const HINT_EDGE: u32 = 16;

/// Opacity (0 to 255) of the hint `elapsed` after the run began: 255 for [`HINT_HOLD`], then a
/// linear fade over [`HINT_FADE`], then 0.
#[cfg(any(windows, test))]
pub(crate) fn hint_alpha(elapsed: Duration) -> u8 {
    if elapsed < HINT_HOLD {
        return 255;
    }
    let into = elapsed - HINT_HOLD;
    if into >= HINT_FADE {
        return 0;
    }
    let left = HINT_FADE - into;
    // 255 * left / fade, rounded; both are small.
    let value = (255 * left.as_millis() + HINT_FADE.as_millis() / 2) / HINT_FADE.as_millis();
    u8::try_from(value).unwrap_or(255)
}

/// Whether the hint is still changing at `elapsed` (the overlay repaints on a timer until then).
#[cfg(any(windows, test))]
pub(crate) fn hint_animating(elapsed: Duration) -> bool {
    elapsed < HINT_HOLD + HINT_FADE
}

/// What the hint bar shows `elapsed` after the run began, and how opaque: the hint (and, on an
/// HDR monitor, the note) for [`HINT_HOLD`]; then the hint fades out, while the HDR note stays
/// (it is a warning, not a hint). `None`: nothing to draw.
#[cfg(any(windows, test))]
pub(crate) fn hint_content(
    texts: &EyedropTexts,
    hdr: bool,
    elapsed: Duration,
) -> Option<(String, u8)> {
    let note = (hdr && !texts.hdr_note.is_empty()).then_some(texts.hdr_note.as_str());
    let text = if elapsed < HINT_HOLD {
        match (texts.hint.is_empty(), note) {
            (true, None) => return None,
            (true, Some(note)) => note.to_owned(),
            (false, None) => texts.hint.clone(),
            (false, Some(note)) => format!("{}\n{note}", texts.hint),
        }
    } else if let Some(note) = note {
        return Some((note.to_owned(), 255));
    } else {
        texts.hint.clone()
    };
    let alpha = if elapsed < HINT_HOLD {
        255
    } else {
        hint_alpha(elapsed)
    };
    (alpha > 0 && !text.is_empty()).then_some((text, alpha))
}

/// Which edge of the monitor the hint bar sits at.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HintSide {
    /// Top centre (the default).
    Top,
    /// Bottom centre: where it moves when the cursor comes near the top.
    Bottom,
}

/// The hint bar of `size` (physical pixels) at `side` of `monitor`, centred, 16 logical pixels
/// from the edge.
#[cfg(any(windows, test))]
pub(crate) fn hint_rect(
    monitor: ScreenRect,
    scale: f32,
    size: (u32, u32),
    side: HintSide,
) -> ScreenRect {
    let edge = i64::from(scaled(HINT_EDGE, scale));
    let x = i64::from(monitor.x) + (i64::from(monitor.w) - i64::from(size.0)) / 2;
    let y = match side {
        HintSide::Top => i64::from(monitor.y) + edge,
        HintSide::Bottom => {
            i64::from(monitor.y) + i64::from(monitor.h) - edge - i64::from(size.1)
        }
    };
    ScreenRect {
        x: i32::try_from(x).unwrap_or(monitor.x),
        y: i32::try_from(y).unwrap_or(monitor.y),
        w: size.0,
        h: size.1,
    }
}

/// How far `cursor` is from `r` (0 inside), in physical pixels.
#[cfg(any(windows, test))]
fn distance_to(r: ScreenRect, cursor: (i32, i32)) -> i64 {
    let gap = |c: i32, origin: i32, size: u32| -> i64 {
        let (c, lo) = (i64::from(c), i64::from(origin));
        let hi = lo + i64::from(size);
        if c < lo {
            lo - c
        } else if c >= hi {
            c - hi + 1
        } else {
            0
        }
    };
    gap(cursor.0, r.x, r.w).max(gap(cursor.1, r.y, r.h))
}

/// The side the hint bar should be at: the top, unless the cursor is within 80 logical pixels of
/// it, then the bottom; when it is that close to both, the one it is farther from.
#[cfg(any(windows, test))]
pub(crate) fn hint_side(
    cursor: (i32, i32),
    monitor: ScreenRect,
    scale: f32,
    size: (u32, u32),
) -> HintSide {
    let near = i64::from(scaled(HINT_NEAR, scale));
    let top = distance_to(hint_rect(monitor, scale, size, HintSide::Top), cursor);
    if top > near {
        return HintSide::Top;
    }
    let bottom = distance_to(hint_rect(monitor, scale, size, HintSide::Bottom), cursor);
    if bottom > near || bottom > top {
        HintSide::Bottom
    } else {
        HintSide::Top
    }
}

// --- small rules of the Windows glue --------------------------------------------------------------

/// What the dying `extern "system"` callbacks use: runs `f`, and answers `fallback` instead of
/// letting a panic unwind into Windows.
#[cfg(any(windows, test))]
pub(crate) fn guarded<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// The monitors that were captured, in order (`None` for one that failed), or why nothing was:
/// one failed monitor (a protected or disconnected-in-flight display) must not end the run, only
/// all of them failing does.
#[cfg(any(windows, test))]
pub(crate) fn keep_captured<T>(results: Vec<Result<T, Error>>) -> Result<Vec<Option<T>>, FailReason> {
    if results.is_empty() {
        return Err(FailReason::NoMonitor);
    }
    let kept: Vec<Option<T>> = results.into_iter().map(Result::ok).collect();
    if kept.iter().all(Option::is_none) {
        return Err(FailReason::CaptureRefused);
    }
    Ok(kept)
}

/// `DISPLAYCONFIG_ADVANCED_COLOR_MODE` of standard dynamic range.
#[cfg(any(windows, test))]
pub(crate) const COLOR_MODE_SDR: u32 = 0;

/// Is a monitor with these `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO` bits in HDR mode? Bit 1 is
/// `advancedColorEnabled`, bit 2 `wideColorEnforced`: advanced colour that is on only because
/// wide colour is enforced is not HDR. The old struct cannot tell automatic colour management
/// (ACM) from HDR.
#[cfg(any(windows, test))]
pub(crate) fn legacy_is_hdr(value: u32) -> bool {
    const ENABLED: u32 = 0b10;
    const WIDE_COLOR_ENFORCED: u32 = 0b100;
    value & ENABLED != 0 && value & WIDE_COLOR_ENFORCED == 0
}

/// Whether the values read from this monitor are approximate: `activeColorMode` of
/// `DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO_2` (Windows 11 24H2) other than SDR (HDR, or wide
/// colour gamut with automatic colour management on an SDR panel, where GDI converts too); on
/// Windows without that query, the old bits.
#[cfg(any(windows, test))]
pub(crate) fn colour_is_approximate(active_mode: Option<u32>, legacy_bits: Option<u32>) -> bool {
    match (active_mode, legacy_bits) {
        (Some(mode), _) => mode != COLOR_MODE_SDR,
        (None, Some(bits)) => legacy_is_hdr(bits),
        (None, None) => false,
    }
}

/// How a bounded wait ended.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waited {
    /// The condition held.
    Ready,
    /// The cap ran out first.
    TimedOut,
}

/// Polls `ready` every `step` until it holds or `cap` has passed on `now` (the clock and the
/// sleep are injected so the wait is tested without waiting). `ready` is tried at least once.
#[cfg(any(windows, test))]
pub(crate) fn wait_until(
    cap: Duration,
    step: Duration,
    mut now: impl FnMut() -> Duration,
    mut sleep: impl FnMut(Duration),
    mut ready: impl FnMut() -> bool,
) -> Waited {
    let start = now();
    loop {
        if ready() {
            return Waited::Ready;
        }
        if now().saturating_sub(start) >= cap {
            return Waited::TimedOut;
        }
        sleep(step);
    }
}

/// What the wait for the hidden Snipdeck window looks at: one top-level window of this process.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowState {
    /// `IsWindowVisible`.
    pub visible: bool,
    /// `DWMWA_CLOAKED` is non-zero (hidden by the app, the shell or DWM itself).
    pub cloaked: bool,
    /// `IsIconic`.
    pub minimized: bool,
}

/// Is no window of the process on screen any more: each is hidden, cloaked or minimised.
#[cfg(any(windows, test))]
pub(crate) fn cloak_settled(windows: &[WindowState]) -> bool {
    windows
        .iter()
        .all(|w| !w.visible || w.cloaked || w.minimized)
}

// --- how a run ends ---------------------------------------------------------------------------

/// Why a run ended on its own (the user did not choose it): the host says so in a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// A monitor was added, removed or changed, so the frozen shots no longer match the screen.
    DisplayChanged,
    /// The overlay was open for 120 seconds.
    TimedOut,
}

/// Why a run could not start or went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailReason {
    /// Every monitor refused to be captured (a secure desktop, a locked session, protected
    /// content on all screens).
    CaptureRefused,
    /// Windows listed no monitor.
    NoMonitor,
    /// Anything else: the call name and code, never screen content. For the log, not the user.
    Other(String),
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EyedropOutcome {
    /// The user picked a colour.
    Picked {
        /// Red, green, blue of the final pick (the last one collected when Esc or a right click
        /// ended a run that had collected colours).
        rgb: [u8; 3],
        /// A monitor it came from is in HDR mode or colour managed: the value is approximate.
        hdr: bool,
        /// Colours the user kept with Shift+click before the final pick, oldest first.
        also: Vec<[u8; 3]>,
    },
    /// The user cancelled, or left the overlay (focus went to another app).
    Cancelled,
    /// The run ended by itself: tell the user why.
    Ended(EndReason),
    /// The screen could not be captured, or the overlay failed.
    Failed(FailReason),
}

/// Something worth telling the user that is not how the run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EyedropNote {
    /// The overlay could not take the keyboard, so it asked Windows for its keys as global
    /// hotkeys, and these were refused (another app owns them): they do not work in this run.
    KeysNotRegistered(Vec<KeyName>),
}

/// Called once, on the eyedropper's own thread, when the run ends.
pub type EyedropDone = Box<dyn FnOnce(EyedropOutcome) + Send>;

/// Lets the user pick a colour from anywhere on the screen.
pub trait Eyedropper {
    /// Waits for the Snipdeck window to be hidden (cloaked), at most `settle`, captures every
    /// monitor, shows the overlay and returns at once; the run goes on in the background.
    ///
    /// `done` is called exactly once, on the eyedropper's thread, with how the run ended, unless
    /// this returns an error, in which case it is dropped uncalled.
    ///
    /// # Errors
    ///
    /// A run is already going, or its thread could not start.
    fn start(
        &mut self,
        texts: EyedropTexts,
        settle: Duration,
        done: EyedropDone,
    ) -> Result<(), Error>;

    /// A run has started and its `done` has not been called yet.
    fn running(&self) -> bool;

    /// What the latest run has to say besides how it ended (see [`EyedropNote`]), complete once
    /// its `done` has been called; emptied when the next run starts.
    fn notes(&self) -> Vec<EyedropNote>;
}

/// An eyedropper that answers from a script and remembers how it was started (for tests and the
/// dev build). Clones share the script.
///
/// A scripted outcome ([`FakeEyedropper::push`]) is handed to `done` inside `start`, on the
/// caller's thread. With nothing scripted the run stays going until
/// [`FakeEyedropper::complete`], so a test can look at the state in between.
#[cfg(any(test, feature = "testing"))]
#[derive(Clone, Default)]
pub struct FakeEyedropper {
    state: std::sync::Arc<std::sync::Mutex<FakeState>>,
}

#[cfg(any(test, feature = "testing"))]
#[derive(Default)]
struct FakeState {
    script: std::collections::VecDeque<Result<EyedropOutcome, Error>>,
    starts: Vec<(EyedropTexts, Duration)>,
    pending: Option<EyedropDone>,
    notes: Vec<EyedropNote>,
}

#[cfg(any(test, feature = "testing"))]
impl std::fmt::Debug for FakeEyedropper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.lock();
        f.debug_struct("FakeEyedropper")
            .field("starts", &state.as_ref().map(|s| s.starts.len()).ok())
            .field("running", &state.as_ref().map(|s| s.pending.is_some()).ok())
            .finish()
    }
}

#[cfg(any(test, feature = "testing"))]
impl FakeEyedropper {
    /// An eyedropper whose next run ends with `outcome`.
    #[must_use]
    pub fn answering(outcome: EyedropOutcome) -> Self {
        let fake = Self::default();
        fake.push(outcome);
        fake
    }

    /// Adds an outcome to the end of the script.
    pub fn push(&self, outcome: EyedropOutcome) {
        if let Ok(mut state) = self.state.lock() {
            state.script.push_back(Ok(outcome));
        }
    }

    /// Adds a refusal to the end of the script: that `start` returns `error` and drops `done`.
    pub fn push_start_error(&self, error: Error) {
        if let Ok(mut state) = self.state.lock() {
            state.script.push_back(Err(error));
        }
    }

    /// The texts and settle time of every `start` so far, oldest first (refused ones too).
    #[must_use]
    pub fn starts(&self) -> Vec<(EyedropTexts, Duration)> {
        self.state
            .lock()
            .map(|s| s.starts.clone())
            .unwrap_or_default()
    }

    /// Sets the notes [`Eyedropper::notes`] returns (until set again).
    pub fn set_notes(&self, notes: Vec<EyedropNote>) {
        if let Ok(mut state) = self.state.lock() {
            state.notes = notes;
        }
    }

    /// Ends the run that is going (one started with nothing scripted) with `outcome`. `false` if
    /// none is going.
    pub fn complete(&self, outcome: EyedropOutcome) -> bool {
        let pending = self.state.lock().ok().and_then(|mut s| s.pending.take());
        pending.map(|done| done(outcome)).is_some()
    }
}

#[cfg(any(test, feature = "testing"))]
impl Eyedropper for FakeEyedropper {
    fn start(
        &mut self,
        texts: EyedropTexts,
        settle: Duration,
        done: EyedropDone,
    ) -> Result<(), Error> {
        let scripted = {
            let Ok(mut state) = self.state.lock() else {
                return Err(Error::Unavailable("the fake eyedropper is poisoned"));
            };
            state.starts.push((texts, settle));
            if state.pending.is_some() {
                return Err(Error::Unavailable("an eyedropper run is already going"));
            }
            match state.script.pop_front() {
                Some(Err(error)) => return Err(error),
                Some(Ok(outcome)) => Some(outcome),
                None => {
                    state.pending = Some(done);
                    return Ok(());
                }
            }
        };
        // Outside the lock: `done` may call back into this fake.
        if let Some(outcome) = scripted {
            done(outcome);
        }
        Ok(())
    }

    fn running(&self) -> bool {
        self.state.lock().is_ok_and(|s| s.pending.is_some())
    }

    fn notes(&self) -> Vec<EyedropNote> {
        self.state
            .lock()
            .map(|s| s.notes.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> ScreenRect {
        ScreenRect { x, y, w, h }
    }

    /// A shot whose pixel at local `(lx, ly)` is `colour(lx, ly)`.
    fn shot(bounds: ScreenRect, scale: f32, colour: impl Fn(u32, u32) -> [u8; 3]) -> Shot {
        let mut bgra = Vec::new();
        for ly in 0..bounds.h {
            for lx in 0..bounds.w {
                let [r, g, b] = colour(lx, ly);
                bgra.extend([b, g, r, 0x7f]); // an alpha byte that must never matter
            }
        }
        Shot::new(bounds, scale, false, bgra)
    }

    /// Left monitor at a negative origin, a 100 px gap, then the main monitor at the origin:
    /// `x` in -200..-100 and 0..100, `y` in 0..50.
    fn two_with_gap() -> Vec<Shot> {
        vec![
            shot(rect(-200, 0, 100, 50), 1.0, |x, y| {
                [u8::try_from(x).unwrap(), u8::try_from(y).unwrap(), 1]
            }),
            shot(rect(0, 0, 100, 50), 1.5, |x, y| {
                [u8::try_from(x).unwrap(), u8::try_from(y).unwrap(), 2]
            }),
        ]
    }

    // sample -------------------------------------------------------------------------------

    #[test]
    fn a_pixel_is_read_from_the_shot_that_covers_it_as_rgb() {
        let shots = two_with_gap();
        assert_eq!(sample(&shots, 5, 7), Some([5, 7, 2]), "main monitor");
        assert_eq!(sample(&shots, 99, 49), Some([99, 49, 2]), "its last pixel");
        assert_eq!(sample(&shots, -200, 0), Some([0, 0, 1]), "negative origin");
        assert_eq!(sample(&shots, -101, 3), Some([99, 3, 1]), "its right edge");
    }

    #[test]
    fn the_gap_and_the_outside_have_no_colour() {
        let shots = two_with_gap();
        for (x, y) in [
            (-100, 10),
            (-1, 10),
            (-50, 10),
            (100, 10),
            (-201, 10),
            (5, 50),
            (5, -1),
        ] {
            assert_eq!(sample(&shots, x, y), None, "({x}, {y})");
        }
        assert_eq!(sample(&[], 0, 0), None);
    }

    #[test]
    fn a_shot_with_too_few_bytes_counts_as_a_gap_not_a_panic() {
        let mut short = shot(rect(0, 0, 4, 4), 1.0, |_, _| [1, 2, 3]);
        short.bgra = Pixels::from(short.bgra.bytes()[..4 * 4 * 4 - 3].to_vec());
        assert_eq!(sample(std::slice::from_ref(&short), 3, 3), None);
        assert_eq!(sample(&[short], 3, 2), Some([1, 2, 3]));
    }

    #[test]
    fn shot_at_names_the_monitor_under_a_point() {
        let shots = two_with_gap();
        assert_eq!(
            shot_at(&shots, -150, 5).map(|s| s.scale.to_bits()),
            Some(1.0_f32.to_bits())
        );
        assert_eq!(
            shot_at(&shots, 50, 5).map(|s| s.scale.to_bits()),
            Some(1.5_f32.to_bits())
        );
        assert!(shot_at(&shots, -50, 5).is_none());
    }

    // loupe_cells --------------------------------------------------------------------------

    #[test]
    fn the_loupe_cells_are_row_major_with_the_cursor_pixel_in_the_middle() {
        let shots = two_with_gap();
        let cells = loupe_cells(&shots, 50, 25, 3);
        assert_eq!(cells.len(), 9);
        assert_eq!(cells[0], Some([49, 24, 2]), "top left");
        assert_eq!(cells[1], Some([50, 24, 2]), "top middle");
        assert_eq!(cells[3], Some([49, 25, 2]), "middle left");
        assert_eq!(cells[4], sample(&shots, 50, 25), "centre");
        assert_eq!(cells[8], Some([51, 26, 2]), "bottom right");
        assert_eq!(loupe_cells(&shots, 50, 25, CELLS).len(), 121);
    }

    #[test]
    fn loupe_cells_in_a_gap_or_off_the_edge_are_none() {
        let shots = two_with_gap();
        // Cursor on the left edge of the main monitor: the column to its left is the gap.
        let cells = loupe_cells(&shots, 0, 0, 3);
        let row = |r: usize| &cells[r * 3..r * 3 + 3];
        assert_eq!(row(0), [None, None, None], "above the top edge");
        assert_eq!(row(1), [None, Some([0, 0, 2]), Some([1, 0, 2])]);
        assert_eq!(row(2), [None, Some([0, 1, 2]), Some([1, 1, 2])]);
        // And no shots at all: nothing, but the right count.
        assert_eq!(loupe_cells(&[], 0, 0, 5), vec![None; 25]);
        // Extreme coordinates do not overflow.
        assert_eq!(loupe_cells(&shots, i32::MAX, i32::MIN, 3), vec![None; 9]);
    }

    // loupe_rect ---------------------------------------------------------------------------

    #[test]
    fn the_loupe_box_is_the_grid_plus_padding_and_the_readout_times_the_scale() {
        let m = rect(0, 0, 1920, 1080);
        // Zoom 8: the grid is 11 * 8 + 8 = 96 but the readout needs 160 wide; 96 + 48 = 144 tall.
        let r = loupe_rect((100, 100), m, 1.0, LoupeLayout::new(8));
        assert_eq!((r.w, r.h), (160, 144));
        // Zoom 24 at 150 %: 272 * 1.5 = 408 wide, (272 + 48) * 1.5 = 480 tall.
        let r = loupe_rect((100, 100), m, 1.5, LoupeLayout::new(24));
        assert_eq!((r.w, r.h), (408, 480));
    }

    #[test]
    fn the_readout_bar_and_its_minimum_width_grow_with_the_text_scale() {
        let m = rect(0, 0, 1920, 1080);
        let at = |text_scale| {
            loupe_rect(
                (100, 100),
                m,
                1.0,
                LoupeLayout {
                    zoom: 8,
                    text_scale,
                    button: false,
                },
            )
        };
        assert_eq!((at(1.0).w, at(1.0).h), (160, 144));
        // 225 %: the bar is 48 * 2.25 = 108 tall, the minimum width 160 * 2.25 = 360.
        assert_eq!((at(2.25).w, at(2.25).h), (360, 96 + 108));
        // Below 1 and NaN count as 1; above 3 stops at 3.
        assert_eq!(at(0.5), at(1.0));
        assert_eq!(at(f32::NAN), at(1.0));
        assert_eq!(at(10.0), at(3.0));
    }

    #[test]
    fn the_touch_button_widens_the_box_and_sits_inside_the_readout_bar() {
        let m = rect(0, 0, 1920, 1080);
        for (scale, ts) in [(1.0_f32, 1.0_f32), (1.5, 1.0), (1.0, 2.0), (2.0, 1.5)] {
            let plain = loupe_rect((100, 100), m, scale, LoupeLayout::new(8));
            let layout = LoupeLayout {
                zoom: 8,
                text_scale: ts,
                button: true,
            };
            let with = loupe_rect((100, 100), m, scale, layout);
            assert!(with.w > plain.w, "wider with the button at {scale}/{ts}");
            let b = pick_button_rect(with, scale, ts);
            let bar_top = i64::from(with.y) + i64::from(with.h) - i64::from(scaled(48, scale * ts));
            assert!(i64::from(b.y) >= bar_top, "inside the bar, below its top");
            assert!(i64::from(b.y) + i64::from(b.h) <= i64::from(with.y) + i64::from(with.h));
            assert!(b.x >= with.x);
            assert!(i64::from(b.x) + i64::from(b.w) <= i64::from(with.x) + i64::from(with.w));
            // A finger target: at least 36 logical pixels tall and 64 wide.
            assert!(b.h >= scaled(36, scale) && b.w >= scaled(64, scale));
        }
    }

    #[test]
    fn the_smallest_magnified_cell_is_eight_physical_pixels_and_the_frame_fits_the_padding() {
        assert_eq!(ZOOMS, [8, 12, 16, 24]);
        for zoom in ZOOMS {
            assert!(scaled(zoom, 1.0) >= MIN_CELL_PX, "zoom {zoom}");
        }
        // The loupe pads the grid by 4 logical pixels (>= 4 physical at any scale >= 1); the
        // frame's two rings need 3.
        for scale in [1.0_f32, 1.25, 1.5, 2.0, 3.0] {
            assert!(scaled(PAD, scale) >= 3, "scale {scale}");
        }
    }

    #[test]
    fn the_loupe_sits_right_of_and_below_the_cursor_by_24_logical_pixels() {
        let m = rect(0, 0, 1920, 1080);
        let r = loupe_rect((100, 200), m, 1.0, LoupeLayout::new(8));
        assert_eq!((r.x, r.y), (124, 224));
        let r = loupe_rect((100, 200), m, 1.5, LoupeLayout::new(8));
        assert_eq!((r.x, r.y), (136, 236), "24 * 1.5 = 36");
    }

    #[test]
    fn the_loupe_flips_left_at_the_right_edge_and_up_at_the_bottom_edge() {
        for scale in [1.0_f32, 1.5] {
            let m = rect(0, 0, 800, 600);
            let probe = loupe_rect((0, 0), m, scale, LoupeLayout::new(8));
            let (w, h) = (
                i32::try_from(probe.w).unwrap(),
                i32::try_from(probe.h).unwrap(),
            );
            let gap = if scale > 1.0 { 36 } else { 24 };

            // Right edge: the box would end past x = 800.
            let r = loupe_rect((790, 100), m, scale, LoupeLayout::new(8));
            assert_eq!(r.x, 790 - gap - w, "flipped left at scale {scale}");
            assert_eq!(r.y, 100 + gap, "same row");
            // Bottom edge.
            let r = loupe_rect((100, 590), m, scale, LoupeLayout::new(8));
            assert_eq!(r.y, 590 - gap - h, "flipped up at scale {scale}");
            assert_eq!(r.x, 100 + gap, "same column");
            // Both.
            let r = loupe_rect((790, 590), m, scale, LoupeLayout::new(8));
            assert_eq!((r.x, r.y), (790 - gap - w, 590 - gap - h));
            // Exactly touching the right / bottom edge does not flip.
            let r = loupe_rect((800 - gap - w, 100), m, scale, LoupeLayout::new(8));
            assert_eq!(r.x, 800 - w, "ends on the edge, stays right of the cursor");
        }
    }

    #[test]
    fn the_loupe_flips_at_the_left_and_top_edges_of_a_monitor_with_a_negative_origin() {
        // A monitor left of and above the main one. Cursor at its bottom-right corner: both flips.
        let m = rect(-1920, -1080, 1920, 1080);
        let r = loupe_rect((-5, -5), m, 1.5, LoupeLayout::new(12));
        // 12 * 11 + 8 = 140 logical, the readout needs 160 -> 240 wide; 140 + 48 = 188 -> 282
        // tall; gap 36.
        assert_eq!((r.w, r.h), (240, 282));
        assert_eq!((r.x, r.y), (-5 - 36 - 240, -5 - 36 - 282));
        // Cursor at its top-left corner: right and below, inside the monitor.
        let r = loupe_rect((-1920, -1080), m, 1.0, LoupeLayout::new(12));
        assert_eq!((r.x, r.y), (-1920 + 24, -1080 + 24));
    }

    #[test]
    fn the_loupe_stays_inside_a_monitor_even_when_neither_side_fits() {
        // 120 x 100 monitor; the zoom 24 box is far larger than it.
        let m = rect(50, 60, 120, 100);
        let r = loupe_rect((110, 110), m, 1.0, LoupeLayout::new(24));
        assert_eq!((r.x, r.y), (50, 60), "sticks to the top left");
        // A box that fits only on one side lands fully inside.
        let m = rect(0, 0, 400, 300);
        let r = loupe_rect((300, 50), m, 1.0, LoupeLayout::new(8));
        assert_eq!(r.x, 300 - 24 - 160, "flipped");
        assert!(r.x >= 0);
    }

    // Session ------------------------------------------------------------------------------

    fn session(shots: &[Shot], at: (i32, i32)) -> Session {
        Session::new(shots, at)
    }

    #[test]
    fn a_new_session_starts_at_the_cursor_with_zoom_eight() {
        let shots = two_with_gap();
        let s = session(&shots, (10, 20));
        assert_eq!(s.cursor(), (10, 20));
        assert_eq!(s.zoom(), 8);
        assert_eq!(s.colour(&shots), Some([10, 20, 2]));
    }

    #[test]
    fn moved_follows_the_mouse_and_a_repeat_changes_nothing() {
        let shots = two_with_gap();
        let mut s = session(&shots, (10, 20));
        assert_eq!(s.on(&shots, Input::Moved(30, 40)), Step::Redraw);
        assert_eq!(s.cursor(), (30, 40));
        assert_eq!(s.on(&shots, Input::Moved(30, 40)), Step::Nothing);
        // The mouse itself may be anywhere, even a place with no pixels: it is only a position.
        assert_eq!(s.on(&shots, Input::Moved(-50, 40)), Step::Redraw);
        assert_eq!(s.colour(&shots), None);
    }

    #[test]
    fn arrows_nudge_one_pixel_and_shift_arrows_ten() {
        let shots = two_with_gap();
        let mut s = session(&shots, (50, 25));
        let key = |nav, shift| Input::Key { nav, fast: shift };
        assert_eq!(
            s.on(&shots, key(Nav::Right, false)),
            Step::MoveCursor(51, 25)
        );
        assert_eq!(
            s.on(&shots, key(Nav::Left, false)),
            Step::MoveCursor(50, 25)
        );
        assert_eq!(
            s.on(&shots, key(Nav::Down, false)),
            Step::MoveCursor(50, 26)
        );
        assert_eq!(s.on(&shots, key(Nav::Up, false)), Step::MoveCursor(50, 25));
        assert_eq!(
            s.on(&shots, key(Nav::Right, true)),
            Step::MoveCursor(60, 25)
        );
        assert_eq!(s.on(&shots, key(Nav::Down, true)), Step::MoveCursor(60, 35));
        assert_eq!(s.on(&shots, key(Nav::Left, true)), Step::MoveCursor(50, 35));
        assert_eq!(s.on(&shots, key(Nav::Up, true)), Step::MoveCursor(50, 25));
        assert_eq!(
            s.cursor(),
            (50, 25),
            "the session holds the new place itself"
        );
    }

    #[test]
    fn a_nudge_is_shortened_at_the_edge_and_never_lands_in_a_gap_or_off_the_screen() {
        let shots = two_with_gap();
        let key = |nav, shift| Input::Key { nav, fast: shift };
        // Shift+Right 4 px from the right edge of the main monitor: stops on its last pixel.
        let mut s = session(&shots, (95, 25));
        assert_eq!(
            s.on(&shots, key(Nav::Right, true)),
            Step::MoveCursor(99, 25)
        );
        assert_eq!(
            s.on(&shots, key(Nav::Right, false)),
            Step::Nothing,
            "no pixel beyond"
        );
        assert_eq!(s.cursor(), (99, 25));
        // Left from the main monitor's left edge: the gap is right there.
        let mut s = session(&shots, (0, 25));
        assert_eq!(s.on(&shots, key(Nav::Left, false)), Step::Nothing);
        assert_eq!(s.on(&shots, key(Nav::Left, true)), Step::Nothing);
        // Shift+Left 3 px from the gap lands on the first pixel: shortened to the edge.
        let mut s = session(&shots, (3, 25));
        assert_eq!(s.on(&shots, key(Nav::Left, true)), Step::MoveCursor(0, 25));
        // Top and bottom edges.
        let mut s = session(&shots, (50, 0));
        assert_eq!(s.on(&shots, key(Nav::Up, true)), Step::Nothing);
        let mut s = session(&shots, (50, 45));
        assert_eq!(s.on(&shots, key(Nav::Down, true)), Step::MoveCursor(50, 49));
        // Right edge of the left monitor, with the gap beyond it.
        let mut s = session(&shots, (-101, 25));
        assert_eq!(s.on(&shots, key(Nav::Right, false)), Step::Nothing);
        // The nudge never invents a pixel: whatever it returned has a colour.
        let mut s = session(&shots, (97, 3));
        for _ in 0..20 {
            let _ = s.on(&shots, key(Nav::Right, true));
            assert!(s.colour(&shots).is_some());
        }
    }

    #[test]
    fn arrows_with_no_screen_do_nothing() {
        let mut s = session(&[], (0, 0));
        assert_eq!(
            s.on(
                &[],
                Input::Key {
                    nav: Nav::Left,
                    fast: true
                }
            ),
            Step::Nothing
        );
        assert_eq!(s.on(&[], Input::Pick), Step::Nothing);
    }

    #[test]
    fn a_nudge_does_not_overflow_at_the_extremes() {
        let shots = two_with_gap();
        let mut s = session(&shots, (i32::MAX, i32::MIN));
        for nav in [Nav::Left, Nav::Right, Nav::Up, Nav::Down] {
            assert_eq!(s.on(&shots, Input::Key { nav, fast: true }), Step::Nothing);
        }
    }

    fn picked(rgb: [u8; 3]) -> Step {
        Step::Finished(EyedropOutcome::Picked {
            rgb,
            hdr: false,
            also: Vec::new(),
        })
    }

    #[test]
    fn pick_returns_the_colour_under_the_cursor_and_nothing_in_a_gap() {
        let shots = two_with_gap();
        let mut s = session(&shots, (7, 8));
        assert_eq!(s.on(&shots, Input::Pick), picked([7, 8, 2]));
        let _ = s.on(&shots, Input::Moved(-150, 9));
        assert_eq!(s.on(&shots, Input::Pick), picked([50, 9, 1]));
        let _ = s.on(&shots, Input::Moved(-50, 9));
        assert_eq!(s.on(&shots, Input::Pick), Step::Nothing);
    }

    #[test]
    fn cancel_and_losing_the_focus_cancel_and_the_screen_changing_or_the_timeout_end_the_run() {
        let shots = two_with_gap();
        let expect = [
            (Input::Cancel, EyedropOutcome::Cancelled),
            (Input::LostFocus, EyedropOutcome::Cancelled),
            (
                Input::DisplayChanged,
                EyedropOutcome::Ended(EndReason::DisplayChanged),
            ),
            (
                Input::TimedOut,
                EyedropOutcome::Ended(EndReason::TimedOut),
            ),
        ];
        for (input, outcome) in expect {
            let mut s = session(&shots, (1, 1));
            assert_eq!(s.on(&shots, input), Step::Finished(outcome), "{input:?}");
        }
    }

    #[test]
    fn the_wheel_steps_through_the_zooms_and_stops_at_both_ends() {
        let shots = two_with_gap();
        let mut s = session(&shots, (1, 1));
        let mut seen = vec![s.zoom()];
        loop {
            match s.on(&shots, Input::Wheel(120)) {
                Step::Redraw => seen.push(s.zoom()),
                Step::Nothing => break,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(seen, [8, 12, 16, 24], "up from the default");
        assert_eq!(
            s.on(&shots, Input::Wheel(120)),
            Step::Nothing,
            "stays at 24"
        );
        let mut down = vec![s.zoom()];
        while s.on(&shots, Input::Wheel(-120)) == Step::Redraw {
            down.push(s.zoom());
        }
        assert_eq!(down, [24, 16, 12, 8]);
        assert_eq!(s.zoom(), ZOOMS[0]);
        assert_eq!(s.on(&shots, Input::Wheel(-1)), Step::Nothing, "stays at 8");
        assert_eq!(s.on(&shots, Input::Wheel(0)), Step::Nothing);
        // Any size of delta is one step (high-resolution wheels send small ones).
        assert_eq!(s.on(&shots, Input::Wheel(7)), Step::Redraw);
        assert_eq!(s.zoom(), 12);
    }

    // words --------------------------------------------------------------------------------

    fn with_title(title: &str) -> EyedropTexts {
        EyedropTexts {
            title: title.to_owned(),
            ..EyedropTexts::default()
        }
    }

    #[test]
    fn hex_text_is_lower_case() {
        assert_eq!(hex_text([0x3a, 0x7b, 0xd5]), "#3a7bd5");
        assert_eq!(hex_text([0, 0, 0]), "#000000");
    }

    #[test]
    fn the_window_title_is_the_hosts_pattern_with_the_hex_in_it() {
        let rgb = Some([0x3a, 0x7b, 0xd5]);
        let t = with_title("Pipette, {}");
        assert_eq!(window_title(&t, rgb), "Pipette, #3a7bd5");
        assert_eq!(window_title(&t, None), "Pipette", "no colour: the words only");
        // Other scripts and punctuation: whatever precedes `{}` minus trailing punctuation.
        let t = with_title("\u{30b9}\u{30dd}\u{30a4}\u{30c8}\u{ff1a}{}");
        assert_eq!(window_title(&t, rgb), "\u{30b9}\u{30dd}\u{30a4}\u{30c8}\u{ff1a}#3a7bd5");
        assert_eq!(window_title(&t, None), "\u{30b9}\u{30dd}\u{30a4}\u{30c8}");
        // The hex may come first; a pattern without `{}` gets the hex after it.
        assert_eq!(window_title(&with_title("{} (picker)"), rgb), "#3a7bd5 (picker)");
        assert_eq!(window_title(&with_title("Pipette"), rgb), "Pipette #3a7bd5");
        assert_eq!(window_title(&with_title("Pipette"), None), "Pipette");
        // Nothing but the hex: never an empty title.
        assert_eq!(window_title(&with_title("{}"), rgb), "#3a7bd5");
        assert_eq!(window_title(&with_title("{}"), None), "Snipdeck");
    }

    /// The string literals of one line of code, without a trailing `//` comment.
    fn string_literals(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let (mut current, mut inside, mut escaped) = (String::new(), false, false);
        let mut prev = ' ';
        for c in line.chars() {
            if inside {
                if escaped {
                    escaped = false;
                    current.push(c);
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    out.push(std::mem::take(&mut current));
                    inside = false;
                } else {
                    current.push(c);
                }
            } else if c == '/' && prev == '/' {
                break;
            } else if c == '"' && prev != '\'' {
                inside = true;
            }
            prev = c;
        }
        out
    }

    /// Two or more words of three or more letters: something a person would read.
    fn looks_like_prose(literal: &str) -> bool {
        literal
            .split_whitespace()
            .filter(|w| w.chars().filter(char::is_ascii_alphabetic).count() >= 3)
            .count()
            >= 2
    }

    #[test]
    fn no_user_visible_english_literal_is_left_in_the_eyedropper_code() {
        // Diagnostics for the log (an `Error`, an `expect` reason) are English on purpose; any
        // other prose in a string would reach the screen untranslated.
        let internal = ["Unavailable(", "Pixels(", "reason =", "Error::Os", "FailReason::Other("];
        for (name, source) in [
            ("eyedrop.rs", include_str!("eyedrop.rs")),
            ("windows/eyedrop.rs", include_str!("windows/eyedrop.rs")),
        ] {
            let code = source
                .split("\n#[cfg(test)]\nmod tests")
                .next()
                .unwrap_or_default();
            for (n, line) in code.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for literal in string_literals(line) {
                    assert!(
                        !looks_like_prose(&literal) || internal.iter().any(|m| line.contains(m)),
                        "{name}:{}: user-visible text {literal:?} belongs in EyedropTexts",
                        n + 1
                    );
                }
            }
        }
    }

    #[test]
    fn the_prose_check_itself_sees_prose_and_lets_names_through() {
        assert!(looks_like_prose("Pick a colour {}"));
        assert!(looks_like_prose("Click to pick"));
        assert!(!looks_like_prose("Segoe UI"));
        assert!(!looks_like_prose("SnipdeckEyedropOverlay"));
        assert!(!looks_like_prose("{}"));
        assert!(!looks_like_prose("#{:02x}{:02x}{:02x}"));
        assert_eq!(
            string_literals(r#"let a = "x y"; // "ignored" b"#),
            vec!["x y".to_owned()]
        );
        assert_eq!(string_literals(r#"f("a\"b", '"')"#), vec!["a\"b".to_owned()]);
    }

    #[test]
    fn the_readout_is_the_hosts_default_format_with_the_hex_on_the_second_line() {
        let rgb = Some([58, 123, 213]);
        let t = EyedropTexts {
            readout: Arc::new(|c| format!("rgb({}, {}, {})", c[0], c[1], c[2])),
            ..EyedropTexts::default()
        };
        assert_eq!(
            readout_lines(&t, rgb, Aperture::One, 0),
            ("rgb(58, 123, 213)".to_owned(), "#3a7bd5  1x1".to_owned())
        );
        assert_eq!(
            readout_lines(&t, rgb, Aperture::Five, 2),
            ("rgb(58, 123, 213)".to_owned(), "#3a7bd5  5x5  +2".to_owned())
        );
        // In a gap: a dash and the aperture.
        assert_eq!(
            readout_lines(&t, None, Aperture::Three, 0),
            ("-".to_owned(), "3x3".to_owned())
        );
        // The default format is the hex already: it is not written twice.
        let hex = EyedropTexts::default();
        assert_eq!(
            readout_lines(&hex, rgb, Aperture::Three, 0),
            ("#3a7bd5".to_owned(), "3x3".to_owned())
        );
        let upper = EyedropTexts {
            readout: Arc::new(|c| hex_text(c).to_uppercase()),
            ..EyedropTexts::default()
        };
        assert_eq!(readout_lines(&upper, rgb, Aperture::One, 0).1, "1x1");
    }

    #[test]
    fn texts_compare_by_what_they_say_and_default_to_the_hex_at_scale_one() {
        let a = texts("x");
        assert_eq!(a, texts("x"), "two builds of the default formatter agree");
        assert_eq!(a.clone(), a);
        assert_ne!(a, texts("y"));
        let scaled = EyedropTexts {
            text_scale: 1.5,
            ..texts("x")
        };
        assert_ne!(a, scaled);
        let other = EyedropTexts {
            readout: Arc::new(|_| "?".to_owned()),
            ..texts("x")
        };
        assert_ne!(a, other);
        let d = EyedropTexts::default();
        assert_eq!(d.format([1, 2, 3]), "#010203");
        assert!((d.text_scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_text_scale_is_clamped_and_scales_the_font() {
        assert!((clamp_text_scale(1.0) - 1.0).abs() < f32::EPSILON);
        assert!((clamp_text_scale(0.5) - 1.0).abs() < f32::EPSILON);
        assert!((clamp_text_scale(2.25) - 2.25).abs() < f32::EPSILON);
        assert!((clamp_text_scale(9.0) - 3.0).abs() < f32::EPSILON);
        assert!((clamp_text_scale(f32::NAN) - 1.0).abs() < f32::EPSILON);
        assert!((clamp_text_scale(f32::INFINITY) - 1.0).abs() < f32::EPSILON);
        // 12 pt = 16 px; the monitor scale and the text scale multiply.
        assert_eq!(font_height_px(12, 1.0, 1.0), -16);
        assert_eq!(font_height_px(12, 1.5, 1.0), -24);
        assert_eq!(font_height_px(12, 1.0, 2.0), -32);
        assert_eq!(font_height_px(12, 1.5, 2.0), -48);
        assert_eq!(font_height_px(12, 1.0, 0.5), -16, "never below the 100 % size");
    }

    // pixels, apertures and failed monitors ------------------------------------------------

    #[test]
    fn the_aperture_cycles_one_three_five_and_back() {
        let mut a = Aperture::default();
        assert_eq!((a, a.side()), (Aperture::One, 1));
        a = a.next();
        assert_eq!((a, a.side()), (Aperture::Three, 3));
        a = a.next();
        assert_eq!((a, a.side()), (Aperture::Five, 5));
        assert_eq!(a.next(), Aperture::One);
    }

    /// A 10 x 10 monitor whose pixel at `(x, y)` is `[x * 10, y * 10, 4]`.
    fn ramp() -> Vec<Shot> {
        vec![shot(rect(0, 0, 10, 10), 1.0, |x, y| {
            [
                u8::try_from(x * 10).unwrap(),
                u8::try_from(y * 10).unwrap(),
                4,
            ]
        })]
    }

    #[test]
    fn an_averaged_sample_is_the_mean_of_the_square_around_the_pixel() {
        let shots = ramp();
        assert_eq!(average(&shots, 5, 5, Aperture::One), Some([50, 50, 4]));
        // The ramp is linear, so the mean of a centred square is its centre.
        assert_eq!(average(&shots, 5, 5, Aperture::Three), Some([50, 50, 4]));
        assert_eq!(average(&shots, 5, 5, Aperture::Five), Some([50, 50, 4]));
        // At the corner only the 2 x 2 that exist count: x 0 and 10 twice each -> 5.
        assert_eq!(average(&shots, 0, 0, Aperture::Three), Some([5, 5, 4]));
        // Rounded to nearest: five of nine pixels are 10, the rest 0: 50 / 9 = 5.56 -> 6.
        let checker = vec![shot(rect(0, 0, 3, 3), 1.0, |x, y| {
            if (x + y) % 2 == 0 { [10, 0, 0] } else { [0, 0, 0] }
        })];
        assert_eq!(average(&checker, 1, 1, Aperture::Three), Some([6, 0, 0]));
    }

    #[test]
    fn an_averaged_sample_leaves_gaps_out_and_is_none_when_the_pixel_itself_is_in_one() {
        let shots = two_with_gap();
        // (0, 0) on the main monitor: the column to its left is the gap, the row above is off
        // the screen; the pixels [0..=1] x [0..=1] remain: x 0,0,1,1 -> 0.5 -> 1; y likewise.
        assert_eq!(average(&shots, 0, 0, Aperture::Three), Some([1, 1, 2]));
        assert_eq!(average(&shots, -50, 10, Aperture::Three), None, "gap");
        assert_eq!(average(&shots, -50, 10, Aperture::One), None);
        assert_eq!(average(&[], 0, 0, Aperture::Five), None);
    }

    #[test]
    fn a_monitor_that_could_not_be_captured_reads_like_a_gap_but_is_still_a_monitor() {
        let failed = Shot::uncaptured(rect(0, 0, 10, 10), 1.5);
        let shots = vec![failed, shot(rect(10, 0, 10, 10), 1.0, |_, _| [9, 9, 9])];
        assert_eq!(sample(&shots, 5, 5), None);
        assert!(!shots[0].covers(5, 5));
        assert!(shots[0].contains(5, 5));
        assert!(shot_at(&shots, 5, 5).is_none());
        assert_eq!(
            monitor_at(&shots, 5, 5).map(|s| s.captured),
            Some(false),
            "the overlay still needs its bounds"
        );
        assert_eq!(sample(&shots, 15, 5), Some([9, 9, 9]));
        let cells = loupe_cells(&shots, 10, 5, 3);
        assert_eq!(cells[3], None, "left of the boundary is the failed monitor");
        assert_eq!(cells[4], Some([9, 9, 9]));
        // Nothing can be picked or collected on it, and the cursor cannot be nudged onto it.
        let mut s = session(&shots, (5, 5));
        assert_eq!(s.on(&shots, Input::Pick), Step::Nothing);
        assert_eq!(s.on(&shots, Input::Collect), Step::Nothing);
        let mut s = session(&shots, (10, 5));
        assert_eq!(
            s.on(&shots, Input::Key { nav: Nav::Left, fast: false }),
            Step::Nothing
        );
        assert!(s.loupe(&shots).is_some());
    }

    #[test]
    fn one_failed_monitor_is_skipped_and_only_all_failing_is_a_failure() {
        let bad = || Error::Os {
            op: "BitBlt",
            code: 5,
        };
        assert_eq!(
            keep_captured(vec![Ok(1), Err(bad()), Ok(3)]),
            Ok(vec![Some(1), None, Some(3)])
        );
        assert_eq!(keep_captured(vec![Err::<u8, _>(bad())]), Err(FailReason::CaptureRefused));
        assert_eq!(
            keep_captured(vec![Err::<u8, _>(bad()), Err(bad())]),
            Err(FailReason::CaptureRefused)
        );
        assert_eq!(keep_captured::<u8>(Vec::new()), Err(FailReason::NoMonitor));
    }

    #[test]
    fn pixels_can_be_shared_memory_and_compare_by_content() {
        struct Dib(Vec<u8>);
        impl AsRef<[u8]> for Dib {
            fn as_ref(&self) -> &[u8] {
                &self.0
            }
        }
        let owner = Pixels::new(Dib(vec![1, 2, 3, 4]));
        let shared = owner.clone();
        assert_eq!(shared.bytes(), &[1, 2, 3, 4]);
        assert_eq!(owner, Pixels::from(vec![1, 2, 3, 4]));
        assert_eq!(format!("{owner:?}"), "Pixels(4 bytes)");
        // A shot reads straight from it.
        let s = Shot::new(rect(0, 0, 1, 1), 1.0, false, owner);
        assert_eq!(sample(&[s], 0, 0), Some([3, 2, 1]));
    }

    // the frame ------------------------------------------------------------------------------

    #[test]
    fn the_frame_lies_around_the_picked_block_and_never_over_it() {
        for side in [8, 24, 40, 64] {
            for (x, y) in [(10, 10), (0, 0), (100, 37)] {
                let [black, white] = pick_frame(x, y, side);
                assert!(white.white && !black.white);
                // What a ring paints is its rectangle minus the part inside `thick` of the edge:
                // that inner part is where the next ring, or the block, sits.
                let inner = |r: FrameRing| {
                    (
                        r.x + r.thick,
                        r.y + r.thick,
                        r.x + r.w - r.thick,
                        r.y + r.h - r.thick,
                    )
                };
                assert_eq!(inner(white), (x, y, x + side, y + side), "touches the block");
                assert_eq!(
                    inner(black),
                    (white.x, white.y, white.x + white.w, white.y + white.h),
                    "touches the white ring"
                );
                assert_eq!((black.x, black.y), (x - 3, y - 3), "three pixels beyond");
            }
        }
    }

    // keys -------------------------------------------------------------------------------------

    #[test]
    fn keys_map_to_inputs_with_their_modifiers() {
        assert_eq!(key_input(KeyName::Escape, false, false), Input::Cancel);
        assert_eq!(key_input(KeyName::Enter, false, false), Input::Pick);
        assert_eq!(key_input(KeyName::Space, false, false), Input::Pick);
        assert_eq!(key_input(KeyName::Enter, true, false), Input::Collect);
        assert_eq!(key_input(KeyName::Space, true, false), Input::Collect);
        assert_eq!(key_input(KeyName::Plus, false, false), Input::Wheel(1));
        assert_eq!(key_input(KeyName::Plus, true, false), Input::Wheel(1));
        assert_eq!(key_input(KeyName::Minus, false, false), Input::Wheel(-1));
        assert_eq!(
            key_input(KeyName::Aperture, false, false),
            Input::CycleAperture
        );
        let key = |nav, fast| Input::Key { nav, fast };
        assert_eq!(key_input(KeyName::Left, false, false), key(Nav::Left, false));
        assert_eq!(key_input(KeyName::Right, true, false), key(Nav::Right, true));
        assert_eq!(key_input(KeyName::Up, false, true), key(Nav::Up, true));
        assert_eq!(key_input(KeyName::Down, true, true), key(Nav::Down, true));
    }

    #[test]
    fn virtual_keys_map_to_names() {
        let cases = [
            (0x1B, KeyName::Escape),
            (0x0D, KeyName::Enter),
            (0x20, KeyName::Space),
            (0x25, KeyName::Left),
            (0x26, KeyName::Up),
            (0x27, KeyName::Right),
            (0x28, KeyName::Down),
            (0xBB, KeyName::Plus),
            (0x6B, KeyName::Plus),
            (0xBD, KeyName::Minus),
            (0x6D, KeyName::Minus),
            (0x41, KeyName::Aperture),
        ];
        for (code, name) in cases {
            assert_eq!(key_for_vk(code), Some(name), "{code:#x}");
        }
        assert_eq!(key_for_vk(0x42), None);
        assert_eq!(key_for_vk(0), None);
    }

    #[test]
    fn plus_and_minus_zoom_and_a_cycles_the_aperture_in_a_session() {
        let shots = ramp();
        let mut s = session(&shots, (5, 5));
        assert_eq!(s.on(&shots, key_input(KeyName::Plus, false, false)), Step::Redraw);
        assert_eq!(s.zoom(), 12);
        assert_eq!(s.on(&shots, key_input(KeyName::Minus, false, false)), Step::Redraw);
        assert_eq!(s.zoom(), 8);
        assert_eq!(s.on(&shots, key_input(KeyName::Minus, false, false)), Step::Nothing);
        assert_eq!(s.aperture(), Aperture::One);
        assert_eq!(s.on(&shots, key_input(KeyName::Aperture, false, false)), Step::Redraw);
        assert_eq!(s.aperture(), Aperture::Three);
    }

    #[test]
    fn ctrl_arrows_move_ten_pixels_like_shift_arrows() {
        let shots = vec![shot(rect(0, 0, 100, 50), 1.0, |_, _| [1, 2, 3])];
        let mut s = session(&shots, (50, 25));
        let ctrl_right = key_input(KeyName::Right, false, true);
        assert_eq!(s.on(&shots, ctrl_right), Step::MoveCursor(60, 25));
        let shift_down = key_input(KeyName::Down, true, false);
        assert_eq!(s.on(&shots, shift_down), Step::MoveCursor(60, 35));
    }

    #[test]
    fn the_hotkey_fallback_has_every_key_with_unique_ids_and_the_same_meaning() {
        let mut ids: Vec<i32> = FALLBACK_KEYS.iter().map(|k| k.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FALLBACK_KEYS.len(), "ids are unique");
        assert!(ids.iter().all(|id| *id > 0));
        for k in FALLBACK_KEYS {
            let key = key_for_vk(k.vk).unwrap_or_else(|| panic!("vk {:#x}", k.vk));
            assert_eq!(fallback_input(k.id), Some(key_input(key, k.shift, k.ctrl)));
        }
        // Every key of the focused path can be registered as a hotkey.
        for name in [
            KeyName::Escape,
            KeyName::Enter,
            KeyName::Space,
            KeyName::Left,
            KeyName::Right,
            KeyName::Up,
            KeyName::Down,
            KeyName::Plus,
            KeyName::Minus,
            KeyName::Aperture,
        ] {
            assert!(
                FALLBACK_KEYS.iter().any(|k| key_for_vk(k.vk) == Some(name)),
                "{name:?} has no hotkey"
            );
        }
        // The ten-pixel chords, Shift+Enter, plus on both keys.
        let fast = |nav| Some(Input::Key { nav, fast: true });
        assert_eq!(fallback_input(8), fast(Nav::Left));
        assert_eq!(fallback_input(13), fast(Nav::Right));
        assert_eq!(fallback_input(14), fast(Nav::Up));
        assert_eq!(fallback_input(22), Some(Input::Collect));
        assert_eq!(fallback_input(16), Some(Input::Wheel(1)));
        assert_eq!(fallback_input(17), Some(Input::Wheel(1)));
        assert_eq!(fallback_input(18), Some(Input::Wheel(1)));
        assert_eq!(fallback_input(19), Some(Input::Wheel(-1)));
        assert_eq!(fallback_input(20), Some(Input::Wheel(-1)));
        assert_eq!(fallback_input(21), Some(Input::CycleAperture));
        assert_eq!(fallback_input(0), None);
        assert_eq!(fallback_input(99), None);
    }

    #[test]
    fn refused_hotkey_ids_become_distinct_key_names_in_table_order() {
        assert_eq!(keys_of_ids(&[]), Vec::<KeyName>::new());
        assert_eq!(
            keys_of_ids(&[12, 4, 8, 2]),
            [KeyName::Enter, KeyName::Left],
            "Left three times is one key"
        );
        assert_eq!(keys_of_ids(&[21, 16, 18]), [KeyName::Plus, KeyName::Aperture]);
        assert_eq!(keys_of_ids(&[500]), Vec::<KeyName>::new());
    }

    // collecting colours ---------------------------------------------------------------------

    #[test]
    fn shift_click_keeps_the_colour_and_the_run_goes_on_until_a_plain_pick() {
        let shots = two_with_gap();
        let mut s = session(&shots, (7, 8));
        assert_eq!(s.on(&shots, Input::Collect), Step::Redraw);
        let _ = s.on(&shots, Input::Moved(9, 9));
        assert_eq!(s.on(&shots, Input::Collect), Step::Redraw);
        assert_eq!(s.collected(), [[7, 8, 2], [9, 9, 2]]);
        // In a gap nothing is kept.
        let _ = s.on(&shots, Input::Moved(-50, 9));
        assert_eq!(s.on(&shots, Input::Collect), Step::Nothing);
        assert_eq!(s.collected().len(), 2);
        // The plain pick ends it with the others in `also`, oldest first.
        let _ = s.on(&shots, Input::Moved(20, 21));
        assert_eq!(
            s.on(&shots, Input::Pick),
            Step::Finished(EyedropOutcome::Picked {
                rgb: [20, 21, 2],
                hdr: false,
                also: vec![[7, 8, 2], [9, 9, 2]],
            })
        );
    }

    #[test]
    fn every_other_way_to_end_delivers_the_collected_colours_the_last_as_the_pick() {
        let shots = two_with_gap();
        for input in [
            Input::Cancel,
            Input::LostFocus,
            Input::DisplayChanged,
            Input::TimedOut,
        ] {
            let mut s = session(&shots, (7, 8));
            let _ = s.on(&shots, Input::Collect);
            let _ = s.on(&shots, Input::Moved(9, 9));
            let _ = s.on(&shots, Input::Collect);
            assert_eq!(
                s.on(&shots, input),
                Step::Finished(EyedropOutcome::Picked {
                    rgb: [9, 9, 2],
                    hdr: false,
                    also: vec![[7, 8, 2]],
                }),
                "{input:?}"
            );
        }
    }

    #[test]
    fn a_collected_colour_from_an_hdr_monitor_marks_the_result_approximate() {
        let shots = vec![
            Shot::new(rect(0, 0, 10, 10), 1.0, true, vec![0; 400]),
            shot(rect(10, 0, 10, 10), 1.0, |_, _| [1, 1, 1]),
        ];
        let mut s = session(&shots, (2, 2));
        let _ = s.on(&shots, Input::Collect);
        let _ = s.on(&shots, Input::Moved(12, 2));
        match s.on(&shots, Input::Pick) {
            Step::Finished(EyedropOutcome::Picked { hdr, also, .. }) => {
                assert!(hdr);
                assert_eq!(also.len(), 1);
            }
            other => panic!("{other:?}"),
        }
        // A pick on the HDR monitor itself is approximate too.
        let mut s = session(&shots, (2, 2));
        assert!(matches!(
            s.on(&shots, Input::Pick),
            Step::Finished(EyedropOutcome::Picked { hdr: true, .. })
        ));
    }

    #[test]
    fn a_run_keeps_at_most_max_collected_colours() {
        let shots = two_with_gap();
        let mut s = session(&shots, (7, 8));
        for _ in 0..MAX_COLLECTED {
            assert_eq!(s.on(&shots, Input::Collect), Step::Redraw);
        }
        assert_eq!(s.on(&shots, Input::Collect), Step::Nothing);
        assert_eq!(s.collected().len(), MAX_COLLECTED);
    }

    #[test]
    fn the_aperture_key_changes_what_a_pick_returns() {
        let shots = vec![shot(rect(0, 0, 3, 3), 1.0, |x, y| {
            if (x + y) % 2 == 0 { [10, 0, 0] } else { [0, 0, 0] }
        })];
        let mut s = session(&shots, (1, 1));
        assert_eq!(s.colour(&shots), Some([10, 0, 0]));
        assert_eq!(s.on(&shots, Input::CycleAperture), Step::Redraw);
        assert_eq!((s.aperture(), s.colour(&shots)), (Aperture::Three, Some([6, 0, 0])));
        assert_eq!(s.on(&shots, Input::Pick), picked([6, 0, 0]));
    }

    // touch ------------------------------------------------------------------------------------

    fn big() -> Vec<Shot> {
        vec![shot(rect(0, 0, 1000, 800), 1.0, |x, y| {
            [
                u8::try_from(x % 250).unwrap(),
                u8::try_from(y % 250).unwrap(),
                7,
            ]
        })]
    }

    #[test]
    fn the_first_touch_places_the_loupe_and_a_second_near_it_confirms() {
        let shots = big();
        let mut s = session(&shots, (10, 10));
        assert_eq!(s.on(&shots, Input::TouchDown(300, 300)), Step::Redraw);
        assert_eq!(s.cursor(), (300, 300), "the loupe follows the finger down");
        assert_eq!(s.pick_button(&shots), None, "no button while the finger is down");
        assert_eq!(s.on(&shots, Input::TouchMove(310, 305)), Step::Redraw);
        assert_eq!(s.on(&shots, Input::TouchUp(310, 305)), Step::Redraw);
        assert_eq!(s.cursor(), (310, 305));
        assert!(s.pick_button(&shots).is_some(), "placed: the Pick button shows");
        // The loupe grew for the button.
        assert!(s.layout().button);
        // Second tap near the cursor: nothing moves, lifting picks.
        assert_eq!(s.on(&shots, Input::TouchDown(315, 300)), Step::Nothing);
        assert_eq!(s.on(&shots, Input::TouchMove(400, 400)), Step::Nothing, "no drag");
        assert_eq!(s.cursor(), (310, 305));
        assert_eq!(s.on(&shots, Input::TouchUp(315, 300)), picked([60, 55, 7]));
    }

    #[test]
    fn the_pick_button_confirms_a_placed_loupe() {
        let shots = big();
        let mut s = session(&shots, (10, 10));
        let _ = s.on(&shots, Input::TouchDown(300, 300));
        let _ = s.on(&shots, Input::TouchUp(300, 300));
        let b = s.pick_button(&shots).unwrap();
        let centre = (
            b.x + i32::try_from(b.w / 2).unwrap(),
            b.y + i32::try_from(b.h / 2).unwrap(),
        );
        assert_eq!(s.on(&shots, Input::TouchDown(centre.0, centre.1)), Step::Nothing);
        assert_eq!(s.cursor(), (300, 300), "the loupe did not move to the button");
        assert_eq!(s.on(&shots, Input::TouchUp(centre.0, centre.1)), picked([50, 50, 7]));
    }

    #[test]
    fn a_touch_away_from_the_loupe_places_it_again_and_one_on_the_loupe_does_nothing() {
        let shots = big();
        let mut s = session(&shots, (10, 10));
        let _ = s.on(&shots, Input::TouchDown(300, 300));
        let _ = s.on(&shots, Input::TouchUp(300, 300));
        // Elsewhere: re-placed, and no Pick button until it is lifted.
        assert_eq!(s.on(&shots, Input::TouchDown(700, 500)), Step::Redraw);
        assert_eq!(s.cursor(), (700, 500));
        assert_eq!(s.pick_button(&shots), None);
        assert_eq!(s.on(&shots, Input::TouchUp(700, 500)), Step::Redraw);
        assert!(s.pick_button(&shots).is_some());
        // On the loupe's grid (not the button, not near the cursor): ignored.
        let l = s.loupe(&shots).unwrap();
        let on_grid = (l.x + 100, l.y + 60);
        assert_eq!(s.on(&shots, Input::TouchDown(on_grid.0, on_grid.1)), Step::Nothing);
        assert_eq!(s.on(&shots, Input::TouchUp(on_grid.0, on_grid.1)), Step::Nothing);
        assert_eq!(s.cursor(), (700, 500));
        assert!(s.pick_button(&shots).is_some());
        // A mouse moving afterwards ends the touch placement.
        assert_eq!(s.on(&shots, Input::Moved(5, 5)), Step::Redraw);
        assert_eq!(s.pick_button(&shots), None);
    }

    #[test]
    fn a_stray_touch_up_does_nothing() {
        let shots = big();
        let mut s = session(&shots, (10, 10));
        assert_eq!(s.on(&shots, Input::TouchUp(10, 10)), Step::Nothing);
        assert_eq!(s.on(&shots, Input::TouchMove(20, 20)), Step::Nothing);
        assert_eq!(s.cursor(), (10, 10));
    }

    #[test]
    fn touch_made_mouse_messages_are_told_from_the_mouse_by_their_extra_info() {
        assert!(is_touch_extra_info(0xFF51_5780));
        assert!(is_touch_extra_info(0xFF51_57FF), "the low bits are a touch id");
        assert!(!is_touch_extra_info(0xFF51_5700), "a pen is precise: it acts as a mouse");
        assert!(!is_touch_extra_info(0xFF51_5680));
        assert!(!is_touch_extra_info(0));
    }

    // the hint bar -----------------------------------------------------------------------------

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_hint_is_opaque_for_two_seconds_then_fades_out_over_four_hundred_milliseconds() {
        assert_eq!(hint_alpha(Duration::ZERO), 255);
        assert_eq!(hint_alpha(ms(1999)), 255);
        assert_eq!(hint_alpha(ms(2000)), 255);
        assert_eq!(hint_alpha(ms(2200)), 128);
        assert!(hint_alpha(ms(2100)) > hint_alpha(ms(2300)));
        assert_eq!(hint_alpha(ms(2399)), 1);
        assert_eq!(hint_alpha(ms(2400)), 0);
        assert_eq!(hint_alpha(Duration::from_secs(60)), 0);
        assert!(hint_animating(ms(2399)));
        assert!(!hint_animating(ms(2400)));
        assert!(hint_animating(Duration::ZERO));
    }

    #[test]
    fn the_hint_text_fades_but_the_hdr_note_stays() {
        let t = EyedropTexts {
            hint: "H".to_owned(),
            hdr_note: "N".to_owned(),
            ..EyedropTexts::default()
        };
        assert_eq!(hint_content(&t, false, Duration::ZERO), Some(("H".to_owned(), 255)));
        assert_eq!(hint_content(&t, true, ms(1500)), Some(("H\nN".to_owned(), 255)));
        assert_eq!(hint_content(&t, false, ms(2200)), Some(("H".to_owned(), 128)));
        assert_eq!(hint_content(&t, false, ms(3000)), None);
        assert_eq!(hint_content(&t, true, ms(3000)), Some(("N".to_owned(), 255)));
        assert_eq!(hint_content(&t, true, Duration::from_secs(100)), Some(("N".to_owned(), 255)));
        // No hint text, no note: nothing to draw; a note alone shows.
        let none = EyedropTexts::default();
        assert_eq!(hint_content(&none, true, Duration::ZERO), None);
        let note_only = EyedropTexts {
            hdr_note: "N".to_owned(),
            ..EyedropTexts::default()
        };
        assert_eq!(hint_content(&note_only, true, Duration::ZERO), Some(("N".to_owned(), 255)));
        assert_eq!(hint_content(&note_only, false, Duration::ZERO), None);
    }

    #[test]
    fn the_hint_moves_to_the_bottom_when_the_cursor_is_within_eighty_pixels_of_it() {
        let m = rect(0, 0, 1920, 1080);
        let size = (400, 40);
        // Top bar: x 760..1160, y 16..56.
        assert_eq!(hint_rect(m, 1.0, size, HintSide::Top), rect(760, 16, 400, 40));
        assert_eq!(hint_rect(m, 1.0, size, HintSide::Bottom), rect(760, 1024, 400, 40));
        let side = |c| hint_side(c, m, 1.0, size);
        assert_eq!(side((900, 30)), HintSide::Bottom, "on the bar");
        assert_eq!(side((900, 135)), HintSide::Bottom, "80 below it");
        assert_eq!(side((900, 136)), HintSide::Top, "81 below it");
        assert_eq!(side((680, 30)), HintSide::Bottom, "80 to its left");
        assert_eq!(side((679, 30)), HintSide::Top, "81 to its left");
        assert_eq!(side((900, 700)), HintSide::Top);
        // 80 logical pixels: 160 at 200 %.
        let size2 = (800, 80);
        let side2 = |c| hint_side(c, m, 2.0, size2);
        assert_eq!(side2((900, 32 + 80 + 160 - 1)), HintSide::Bottom);
        assert_eq!(side2((900, 32 + 80 + 160)), HintSide::Top);
    }

    #[test]
    fn the_hint_picks_the_farther_edge_when_the_cursor_is_near_both() {
        let m = rect(0, 0, 300, 200);
        let size = (200, 40);
        assert_eq!(hint_side((150, 100), m, 1.0, size), HintSide::Top);
        assert_eq!(hint_side((150, 90), m, 1.0, size), HintSide::Bottom);
    }

    // small rules of the glue ------------------------------------------------------------------

    #[test]
    fn a_panic_in_a_callback_body_becomes_the_fallback() {
        assert!(!guarded(false, || -> bool { panic!("inside a callback") }));
        assert!(guarded(false, || true));
        assert_eq!(guarded(7, || 8), 8);
    }

    #[test]
    fn advanced_colour_is_approximate_for_any_active_mode_but_sdr() {
        // `activeColorMode` of GET_ADVANCED_COLOR_INFO_2: 0 SDR, 1 wide colour gamut, 2 HDR.
        assert!(!colour_is_approximate(Some(COLOR_MODE_SDR), None));
        assert!(colour_is_approximate(Some(1), None), "ACM on an SDR panel");
        assert!(colour_is_approximate(Some(2), None));
        // The new answer wins over the old bits.
        assert!(!colour_is_approximate(Some(0), Some(0b011)));
        assert!(colour_is_approximate(Some(1), Some(0b000)));
        // Without the new query (older Windows) the old bits decide.
        assert!(colour_is_approximate(None, Some(0b011)));
        assert!(!colour_is_approximate(None, Some(0b111)));
        assert!(!colour_is_approximate(None, Some(0b001)));
        assert!(!colour_is_approximate(None, None));
    }

    #[test]
    fn the_old_advanced_colour_bits_are_hdr_only_when_enabled_and_not_just_wide_colour_enforced() {
        assert!(!legacy_is_hdr(0b000));
        assert!(!legacy_is_hdr(0b001)); // supported, off
        assert!(legacy_is_hdr(0b011)); // supported and enabled
        assert!(!legacy_is_hdr(0b111)); // enabled only because wide colour is enforced: SDR
        assert!(!legacy_is_hdr(0b100));
    }

    #[test]
    fn the_wait_polls_until_ready_and_stops_at_the_cap() {
        use std::cell::Cell;
        let clock = Cell::new(Duration::ZERO);
        let polls = Cell::new(0_u32);
        let sleep = |d: Duration| clock.set(clock.get() + d);
        let outcome = wait_until(ms(500), ms(10), || clock.get(), sleep, || {
            polls.set(polls.get() + 1);
            polls.get() >= 4
        });
        assert_eq!(outcome, Waited::Ready);
        assert_eq!((polls.get(), clock.get()), (4, ms(30)), "three sleeps");

        // Never ready: the cap ends it, exactly at the cap on this clock.
        let clock = Cell::new(Duration::ZERO);
        let outcome = wait_until(
            ms(500),
            ms(10),
            || clock.get(),
            |d| clock.set(clock.get() + d),
            || false,
        );
        assert_eq!((outcome, clock.get()), (Waited::TimedOut, ms(500)));

        // A zero cap still tries once and never sleeps.
        let clock = Cell::new(Duration::ZERO);
        let slept = Cell::new(false);
        let ready = wait_until(Duration::ZERO, ms(10), || clock.get(), |_| slept.set(true), || true);
        let late = wait_until(Duration::ZERO, ms(10), || clock.get(), |_| slept.set(true), || false);
        assert_eq!((ready, late, slept.get()), (Waited::Ready, Waited::TimedOut, false));
    }

    #[test]
    fn the_snipdeck_window_is_gone_when_every_window_is_hidden_cloaked_or_minimised() {
        let w = |visible, cloaked, minimized| WindowState {
            visible,
            cloaked,
            minimized,
        };
        assert!(cloak_settled(&[]), "no window at all");
        assert!(!cloak_settled(&[w(true, false, false)]), "still on screen");
        assert!(cloak_settled(&[w(true, true, false)]), "cloaked");
        assert!(cloak_settled(&[w(true, false, true)]), "minimised");
        assert!(cloak_settled(&[w(false, false, false)]), "hidden");
        assert!(cloak_settled(&[w(false, false, false), w(true, true, false)]));
        assert!(!cloak_settled(&[w(true, true, false), w(true, false, false)]));
    }

    #[test]
    fn notes_come_back_from_the_fake_and_stay_until_set_again() {
        let fake = FakeEyedropper::default();
        assert!(fake.notes().is_empty());
        fake.set_notes(vec![EyedropNote::KeysNotRegistered(vec![
            KeyName::Escape,
            KeyName::Aperture,
        ])]);
        let mut host = fake.clone();
        host.start(texts("n"), Duration::ZERO, recorder().1).unwrap();
        assert_eq!(
            host.notes(),
            [EyedropNote::KeysNotRegistered(vec![
                KeyName::Escape,
                KeyName::Aperture
            ])]
        );
    }

    // FakeEyedropper -----------------------------------------------------------------------

    fn texts(tag: &str) -> EyedropTexts {
        EyedropTexts {
            hint: format!("hint {tag}"),
            hdr_note: format!("hdr {tag}"),
            ..EyedropTexts::default()
        }
    }

    /// A `done` that records every outcome it gets.
    fn recorder() -> (Arc<Mutex<Vec<EyedropOutcome>>>, EyedropDone) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        (seen, Box::new(move |o| sink.lock().unwrap().push(o)))
    }

    #[test]
    fn scripted_outcomes_reach_done_once_each_inside_start() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Picked {
            rgb: [1, 2, 3],
            hdr: true,
            also: vec![[4, 5, 6]],
        });
        fake.push(EyedropOutcome::Cancelled);
        fake.push(EyedropOutcome::Failed(FailReason::Other(
            "BitBlt failed".to_owned(),
        )));
        let mut host = fake.clone();
        let (seen, done) = recorder();
        let calls = Arc::new(AtomicUsize::new(0));
        for tag in ["a", "b", "c"] {
            let count = Arc::clone(&calls);
            let sink = Arc::clone(&seen);
            host.start(
                texts(tag),
                Duration::from_millis(150),
                Box::new(move |o| {
                    count.fetch_add(1, Ordering::SeqCst);
                    sink.lock().unwrap().push(o);
                }),
            )
            .unwrap();
            assert!(!host.running(), "a scripted run is over when start returns");
        }
        drop(done);
        assert_eq!(calls.load(Ordering::SeqCst), 3, "once per start");
        assert_eq!(
            *seen.lock().unwrap(),
            [
                EyedropOutcome::Picked {
                    rgb: [1, 2, 3],
                    hdr: true,
                    also: vec![[4, 5, 6]],
                },
                EyedropOutcome::Cancelled,
                EyedropOutcome::Failed(FailReason::Other("BitBlt failed".to_owned())),
            ]
        );
    }

    #[test]
    fn the_fake_records_every_start_with_its_texts_and_settle_time() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Cancelled);
        let mut host = fake.clone();
        assert_eq!(fake.starts().len(), 0);
        host.start(texts("one"), Duration::from_millis(150), recorder().1)
            .unwrap();
        host.start(texts("two"), Duration::ZERO, recorder().1)
            .unwrap();
        assert_eq!(
            fake.starts(),
            [
                (texts("one"), Duration::from_millis(150)),
                (texts("two"), Duration::ZERO)
            ]
        );
    }

    #[test]
    fn with_nothing_scripted_the_run_goes_on_until_completed_and_a_second_start_is_refused() {
        let fake = FakeEyedropper::default();
        let mut host = fake.clone();
        let (seen, done) = recorder();
        host.start(texts("x"), Duration::ZERO, done).unwrap();
        assert!(host.running());
        assert_eq!(seen.lock().unwrap().len(), 0, "done has not been called");

        let (other, other_done) = recorder();
        assert_eq!(
            host.start(texts("y"), Duration::ZERO, other_done),
            Err(Error::Unavailable("an eyedropper run is already going"))
        );
        assert!(
            other.lock().unwrap().is_empty(),
            "the refused done is never called"
        );
        assert_eq!(fake.starts().len(), 2, "the refused start is recorded too");

        assert!(fake.complete(EyedropOutcome::Cancelled));
        assert!(!host.running());
        assert_eq!(*seen.lock().unwrap(), [EyedropOutcome::Cancelled]);
        assert!(
            !fake.complete(EyedropOutcome::Cancelled),
            "nothing left to complete"
        );
        assert_eq!(seen.lock().unwrap().len(), 1, "done ran once");
    }

    #[test]
    fn a_scripted_start_error_is_returned_and_done_is_dropped_uncalled() {
        let fake = FakeEyedropper::default();
        fake.push_start_error(Error::Unavailable("no screen"));
        let mut host = fake.clone();
        let (seen, done) = recorder();
        assert_eq!(
            host.start(texts("x"), Duration::ZERO, done),
            Err(Error::Unavailable("no screen"))
        );
        assert_eq!(seen.lock().unwrap().len(), 0);
        assert!(!host.running());
        // The script moves on: the next start runs.
        let (_seen, done) = recorder();
        host.start(texts("y"), Duration::ZERO, done).unwrap();
        assert!(host.running());
    }

    #[test]
    fn done_may_start_the_next_run_from_inside_the_callback() {
        let fake = FakeEyedropper::answering(EyedropOutcome::Cancelled);
        let mut host = fake.clone();
        let mut again = fake.clone();
        let restarted = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&restarted);
        host.start(
            texts("a"),
            Duration::ZERO,
            Box::new(move |_| {
                // No deadlock on the fake's lock, and the run is already over.
                assert!(!again.running());
                again
                    .start(texts("b"), Duration::ZERO, recorder().1)
                    .unwrap();
                count.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap();
        assert_eq!(restarted.load(Ordering::SeqCst), 1);
        assert!(
            fake.running(),
            "the second run, with nothing scripted, is going"
        );
    }
}
