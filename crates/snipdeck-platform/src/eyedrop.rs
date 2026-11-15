//! The screen eyedropper: what the overlay does with pixels and input, apart from the Win32 calls.
//!
//! [`Eyedropper`] is what the app asks for; Windows answers with
//! `windows::eyedrop::WinEyedropper` (freeze-frame of every monitor, one overlay per monitor,
//! **unverified on Windows**) and tests with [`FakeEyedropper`]. Off Windows the app has no
//! eyedropper and hides the button.
//!
//! Everything the overlay decides lives here so it is tested on Linux CI:
//!
//! - [`Shot`] is one monitor's frozen pixels; [`sample`] and [`loupe_cells`] read them in
//!   physical screen pixels (negative origins and gaps between monitors included).
//! - [`loupe_rect`] places the magnifier next to the cursor and flips it at the monitor edges.
//! - [`Session`] turns [`Input`] (mouse, keys, wheel, focus loss) into a [`Step`].

use std::time::Duration;

use crate::error::Error;
use crate::route::ScreenRect;

/// One monitor, frozen: its place on the virtual screen and its pixels.
///
/// The captures are per monitor so no buffer ever spans a gap or a negative origin.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    /// The monitor on the virtual screen, in physical pixels.
    pub bounds: ScreenRect,
    /// Physical pixels per logical pixel on this monitor (1.5 at 150 %).
    pub scale: f32,
    /// The monitor is in HDR (advanced colour) mode, so the values read are approximate.
    pub hdr: bool,
    /// `bounds.w * bounds.h` pixels, top-down, four bytes each: blue, green, red, then a byte
    /// that is undefined (GDI leaves the alpha alone) and never read.
    pub bgra: Vec<u8>,
}

impl Shot {
    /// The offset of the pixel at `(x, y)` in [`Shot::bgra`], if this shot covers it (and holds
    /// the bytes for it).
    fn pixel_offset(&self, x: i32, y: i32) -> Option<usize> {
        let dx = i64::from(x) - i64::from(self.bounds.x);
        let dy = i64::from(y) - i64::from(self.bounds.y);
        if dx < 0 || dy < 0 || dx >= i64::from(self.bounds.w) || dy >= i64::from(self.bounds.h) {
            return None;
        }
        let offset = usize::try_from((dy * i64::from(self.bounds.w) + dx) * 4).ok()?;
        (offset.checked_add(4)? <= self.bgra.len()).then_some(offset)
    }

    /// Whether this shot has the pixel at `(x, y)`.
    #[must_use]
    pub fn covers(&self, x: i32, y: i32) -> bool {
        self.pixel_offset(x, y).is_some()
    }
}

/// The shot that has the pixel at `(x, y)`, if any: the monitor under that point.
#[must_use]
pub fn shot_at(shots: &[Shot], x: i32, y: i32) -> Option<&Shot> {
    shots.iter().find(|shot| shot.covers(x, y))
}

/// The colour of the pixel at `(x, y)` (physical pixels on the virtual screen) as `[r, g, b]`,
/// or `None` in a gap between monitors.
#[must_use]
pub fn sample(shots: &[Shot], x: i32, y: i32) -> Option<[u8; 3]> {
    shots.iter().find_map(|shot| {
        let i = shot.pixel_offset(x, y)?;
        Some([shot.bgra[i + 2], shot.bgra[i + 1], shot.bgra[i]])
    })
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

/// Logical pixels each sampled pixel is magnified to, smallest to largest. The default is 8.
pub const ZOOMS: [u32; 5] = [4, 8, 12, 16, 24];

/// The magnifier shows `CELLS` x `CELLS` sampled pixels.
pub const CELLS: u32 = 11;

/// Index of the default zoom (8) in [`ZOOMS`].
const DEFAULT_ZOOM: usize = 1;

/// Logical padding around the magnified grid.
const PAD: u32 = 4;

/// Logical height of the readout bar under the grid (swatch and `#rrggbb`).
const READOUT: u32 = 40;

/// Logical gap between the cursor and the magnifier.
const GAP: u32 = 24;

/// The narrowest box, in logical pixels: the readout (swatch and `#rrggbb`) needs this much, and
/// at the smallest zoom the grid alone (`11 * 4 + 8 = 52`) is narrower. It equals the width at
/// the default zoom, so only the zoom 4 box is widened.
const MIN_WIDTH: u32 = 96;

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

/// The box of the magnifier for a cursor at `cursor`, in physical pixels on the virtual screen.
///
/// The box is `CELLS * zoom + 2 * 4` logical pixels wide (never narrower than the readout needs)
/// and 40 taller than that for the readout, times the monitor's `scale`. It sits 24 logical
/// pixels right of and below the cursor, flipped to the left / above when it would leave
/// `monitor`, and kept inside `monitor` if it does not fit either way.
#[must_use]
pub fn loupe_rect(cursor: (i32, i32), monitor: ScreenRect, scale: f32, zoom: u32) -> ScreenRect {
    let grid = CELLS.saturating_mul(zoom).saturating_add(2 * PAD);
    let w = scaled(grid.max(MIN_WIDTH), scale);
    let h = scaled(grid.saturating_add(READOUT), scale);
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
    /// An arrow key; `shift` moves ten pixels instead of one.
    Key {
        /// Which arrow.
        nav: Nav,
        /// Shift is held.
        shift: bool,
    },
    /// Click, Enter or Space.
    Pick,
    /// Right click or Esc.
    Cancel,
    /// The wheel turned: positive (away from the user) zooms in, negative out.
    Wheel(i32),
    /// Another app took the focus.
    LostFocus,
    /// A monitor was added, removed or changed: the frozen shots no longer match the screen.
    DisplayChanged,
    /// The overlay has been open too long.
    TimedOut,
}

/// What the overlay does about an [`Input`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Draw the magnifier again (the old and the new place).
    Redraw,
    /// Put the mouse here (physical pixels), then draw again. [`Session::cursor`] already holds
    /// the new place.
    MoveCursor(i32, i32),
    /// The user chose this colour: end the run.
    Picked([u8; 3]),
    /// End the run without a colour.
    Cancelled,
    /// Nothing changed.
    Nothing,
}

/// The state of one eyedropper run: where the cursor is and how far the magnifier is zoomed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    cursor: (i32, i32),
    zoom: usize,
    /// `(left, top, right, bottom)` of every shot together; `None` without shots.
    union: Option<(i64, i64, i64, i64)>,
}

impl Session {
    /// A run over `shots` with the mouse at `cursor` and the default zoom (8).
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
        }
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

    /// The colour under the cursor, if it is on a monitor.
    #[must_use]
    pub fn colour(&self, shots: &[Shot]) -> Option<[u8; 3]> {
        sample(shots, self.cursor.0, self.cursor.1)
    }

    /// Applies one input.
    ///
    /// - `Moved`: the cursor follows the mouse (`Redraw`, or `Nothing` if it did not move).
    /// - `Key`: moves one pixel, Shift ten, to a pixel some shot has: a step that would land in
    ///   a gap or off the screen is shortened until it fits, and is `Nothing` if even one pixel
    ///   does not (the cursor never goes into a gap by keyboard).
    /// - `Pick`: `Picked` with the colour under the cursor (`Nothing` in a gap).
    /// - `Cancel`, `LostFocus`, `DisplayChanged`, `TimedOut`: `Cancelled`.
    /// - `Wheel`: one step along [`ZOOMS`] (`Redraw`, or `Nothing` at the end of the list).
    pub fn on(&mut self, shots: &[Shot], input: Input) -> Step {
        match input {
            Input::Moved(x, y) => {
                if self.cursor == (x, y) {
                    Step::Nothing
                } else {
                    self.cursor = (x, y);
                    Step::Redraw
                }
            }
            Input::Key { nav, shift } => self.nudge(shots, nav, if shift { 10 } else { 1 }),
            Input::Pick => self.colour(shots).map_or(Step::Nothing, Step::Picked),
            Input::Cancel | Input::LostFocus | Input::DisplayChanged | Input::TimedOut => {
                Step::Cancelled
            }
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

/// The `#rrggbb` text of a colour, lower case.
#[cfg(any(windows, test))]
pub(crate) fn hex_text(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// The window title the overlay carries for screen readers: "Eyedropper, #3a7bd5".
#[cfg(any(windows, test))]
pub(crate) fn window_title(rgb: Option<[u8; 3]>) -> String {
    rgb.map_or_else(
        || "Eyedropper".to_owned(),
        |rgb| format!("Eyedropper, {}", hex_text(rgb)),
    )
}

/// The words the overlay shows, translated by the app.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EyedropTexts {
    /// "Click or Enter picks · Esc cancels · arrows move".
    pub hint: String,
    /// "HDR is on for this screen: values are approximate", shown on an HDR monitor.
    pub hdr_note: String,
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EyedropOutcome {
    /// The user picked a colour.
    Picked {
        /// Red, green, blue.
        rgb: [u8; 3],
        /// The monitor it came from is in HDR mode: the value is approximate.
        hdr: bool,
    },
    /// The user cancelled, or focus or the display changed, or the overlay timed out.
    Cancelled,
    /// The screen could not be captured (a secure desktop, a locked session). Carries the call
    /// name and code, never screen content.
    Failed(String),
}

/// Called once, on the eyedropper's own thread, when the run ends.
pub type EyedropDone = Box<dyn FnOnce(EyedropOutcome) + Send>;

/// Lets the user pick a colour from anywhere on the screen.
pub trait Eyedropper {
    /// Waits `settle` (for the Panel to finish hiding), captures every monitor, shows the
    /// overlay and returns at once; the run goes on in the background.
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
        Shot {
            bounds,
            scale,
            hdr: false,
            bgra,
        }
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
        short.bgra.truncate(4 * 4 * 4 - 3);
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
        // Zoom 8: (11 * 8 + 8) = 96 wide, 96 + 40 = 136 tall.
        let r = loupe_rect((100, 100), m, 1.0, 8);
        assert_eq!((r.w, r.h), (96, 136));
        // Zoom 24 at 150 %: 272 * 1.5 = 408 wide, (272 + 40) * 1.5 = 468 tall.
        let r = loupe_rect((100, 100), m, 1.5, 24);
        assert_eq!((r.w, r.h), (408, 468));
        // Zoom 4: the grid is 52 wide but the readout needs 96; the height stays 52 + 40.
        let r = loupe_rect((100, 100), m, 1.0, 4);
        assert_eq!((r.w, r.h), (96, 92));
    }

    #[test]
    fn the_loupe_sits_right_of_and_below_the_cursor_by_24_logical_pixels() {
        let m = rect(0, 0, 1920, 1080);
        let r = loupe_rect((100, 200), m, 1.0, 8);
        assert_eq!((r.x, r.y), (124, 224));
        let r = loupe_rect((100, 200), m, 1.5, 8);
        assert_eq!((r.x, r.y), (136, 236), "24 * 1.5 = 36");
    }

    #[test]
    fn the_loupe_flips_left_at_the_right_edge_and_up_at_the_bottom_edge() {
        for scale in [1.0_f32, 1.5] {
            let m = rect(0, 0, 800, 600);
            let probe = loupe_rect((0, 0), m, scale, 8);
            let (w, h) = (
                i32::try_from(probe.w).unwrap(),
                i32::try_from(probe.h).unwrap(),
            );
            let gap = if scale > 1.0 { 36 } else { 24 };

            // Right edge: the box would end past x = 800.
            let r = loupe_rect((790, 100), m, scale, 8);
            assert_eq!(r.x, 790 - gap - w, "flipped left at scale {scale}");
            assert_eq!(r.y, 100 + gap, "same row");
            // Bottom edge.
            let r = loupe_rect((100, 590), m, scale, 8);
            assert_eq!(r.y, 590 - gap - h, "flipped up at scale {scale}");
            assert_eq!(r.x, 100 + gap, "same column");
            // Both.
            let r = loupe_rect((790, 590), m, scale, 8);
            assert_eq!((r.x, r.y), (790 - gap - w, 590 - gap - h));
            // Exactly touching the right / bottom edge does not flip.
            let r = loupe_rect((800 - gap - w, 100), m, scale, 8);
            assert_eq!(r.x, 800 - w, "ends on the edge, stays right of the cursor");
        }
    }

    #[test]
    fn the_loupe_flips_at_the_left_and_top_edges_of_a_monitor_with_a_negative_origin() {
        // A monitor left of and above the main one. Cursor at its bottom-right corner: both flips.
        let m = rect(-1920, -1080, 1920, 1080);
        let r = loupe_rect((-5, -5), m, 1.5, 12);
        // 12 * 11 + 8 = 140 logical -> 210 wide; +40 = 180 -> 270 tall; gap 36.
        assert_eq!((r.w, r.h), (210, 270));
        assert_eq!((r.x, r.y), (-5 - 36 - 210, -5 - 36 - 270));
        // Cursor at its top-left corner: right and below, inside the monitor.
        let r = loupe_rect((-1920, -1080), m, 1.0, 12);
        assert_eq!((r.x, r.y), (-1920 + 24, -1080 + 24));
    }

    #[test]
    fn the_loupe_stays_inside_a_monitor_even_when_neither_side_fits() {
        // 120 x 100 monitor; the zoom 24 box is far larger than it.
        let m = rect(50, 60, 120, 100);
        let r = loupe_rect((110, 110), m, 1.0, 24);
        assert_eq!((r.x, r.y), (50, 60), "sticks to the top left");
        // A box that fits only on one side lands fully inside.
        let m = rect(0, 0, 200, 300);
        let r = loupe_rect((150, 50), m, 1.0, 8);
        assert_eq!(r.x, 150 - 24 - 96, "flipped");
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
        let key = |nav, shift| Input::Key { nav, shift };
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
        let key = |nav, shift| Input::Key { nav, shift };
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
                    shift: true
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
            assert_eq!(s.on(&shots, Input::Key { nav, shift: true }), Step::Nothing);
        }
    }

    #[test]
    fn pick_returns_the_colour_under_the_cursor_and_nothing_in_a_gap() {
        let shots = two_with_gap();
        let mut s = session(&shots, (7, 8));
        assert_eq!(s.on(&shots, Input::Pick), Step::Picked([7, 8, 2]));
        let _ = s.on(&shots, Input::Moved(-150, 9));
        assert_eq!(s.on(&shots, Input::Pick), Step::Picked([50, 9, 1]));
        let _ = s.on(&shots, Input::Moved(-50, 9));
        assert_eq!(s.on(&shots, Input::Pick), Step::Nothing);
    }

    #[test]
    fn cancel_and_everything_that_takes_the_screen_away_cancel() {
        let shots = two_with_gap();
        for input in [
            Input::Cancel,
            Input::LostFocus,
            Input::DisplayChanged,
            Input::TimedOut,
        ] {
            let mut s = session(&shots, (1, 1));
            assert_eq!(s.on(&shots, input), Step::Cancelled, "{input:?}");
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
        assert_eq!(down, [24, 16, 12, 8, 4]);
        assert_eq!(s.zoom(), ZOOMS[0]);
        assert_eq!(s.on(&shots, Input::Wheel(-1)), Step::Nothing, "stays at 4");
        assert_eq!(s.on(&shots, Input::Wheel(0)), Step::Nothing);
        // Any size of delta is one step (high-resolution wheels send small ones).
        assert_eq!(s.on(&shots, Input::Wheel(7)), Step::Redraw);
        assert_eq!(s.zoom(), 8);
    }

    #[test]
    fn the_title_and_hex_text_are_lower_case() {
        assert_eq!(hex_text([0x3a, 0x7b, 0xd5]), "#3a7bd5");
        assert_eq!(hex_text([0, 0, 0]), "#000000");
        assert_eq!(
            window_title(Some([0x3a, 0x7b, 0xd5])),
            "Eyedropper, #3a7bd5"
        );
        assert_eq!(window_title(None), "Eyedropper");
    }

    // FakeEyedropper -----------------------------------------------------------------------

    fn texts(tag: &str) -> EyedropTexts {
        EyedropTexts {
            hint: format!("hint {tag}"),
            hdr_note: format!("hdr {tag}"),
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
        });
        fake.push(EyedropOutcome::Cancelled);
        fake.push(EyedropOutcome::Failed("BitBlt failed".to_owned()));
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
                    hdr: true
                },
                EyedropOutcome::Cancelled,
                EyedropOutcome::Failed("BitBlt failed".to_owned()),
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
