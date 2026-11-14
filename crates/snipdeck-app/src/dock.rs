//! The edge dock's state machine (SPEC 5.1 "Edge dock with reveal"): a Panel docked to a
//! screen edge collapses to a thin handle and slides out again on hover and/or click, then
//! collapses when the pointer leaves or after a paste. Pure: the caller feeds it events with
//! the moment they happened (from the injected clock, never the real one) and does what the
//! returned [`DockAction`] says. The window, the timer that sends [`DockEvent::Tick`] and the
//! frame arithmetic ([`crate::frame::handle_rect`]) are elsewhere.
//!
//! "Docked" is the caller's to decide: a Panel with no edge is floating, and the caller passes
//! it a [`DockSettings`] whose trigger is [`DockTrigger::Off`]. Off means always expanded, so
//! a collapsed state can only exist while there is a handle to find (and the Panel hotkey
//! always brings it back).

use std::time::Duration;

use snipdeck_core::Timestamp;

use crate::{
    frame::{self, Monitor, Rect},
    settings::{DockEdge, DockSettings, DockTrigger},
};

/// How often the host sends [`DockEvent::Tick`] while a delay is running.
pub const TICK: Duration = Duration::from_millis(50);

/// Where the dock is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DockState {
    /// The Panel is open.
    #[default]
    Expanded,
    /// Only the handle shows.
    Collapsed,
    /// Only the handle shows, and the pointer has not been seen off it since the Panel
    /// collapsed: resting there opens nothing (the handle appeared under the pointer, nobody
    /// reached for it). A click and the hotkey still open it.
    Parked,
    /// Collapsed, with the pointer on the handle since `since`: the hover delay is running.
    Revealing {
        /// When the pointer arrived.
        since: Timestamp,
    },
    /// Open, with the pointer off the Panel since `since`: the leave delay is running.
    Leaving {
        /// When the pointer left.
        since: Timestamp,
    },
}

/// Something that happened to the dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockEvent {
    /// The pointer entered (`true`) or left (`false`) the handle.
    HandleHover(bool),
    /// The handle was clicked.
    HandleClick,
    /// The pointer entered or left the open Panel.
    PanelHover(bool),
    /// The Panel hotkey.
    Hotkey,
    /// An item was pasted from the Panel.
    Pasted,
    /// Time passed (sent every [`TICK`] while [`DockState::needs_tick`]).
    Tick,
}

/// What the caller must do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAction {
    /// Nothing.
    None,
    /// Show the whole Panel.
    Expand,
    /// Show only the handle.
    Collapse,
}

fn elapsed_ms(now: Timestamp, since: Timestamp) -> i64 {
    now.as_millisecond().saturating_sub(since.as_millisecond())
}

impl DockState {
    /// Only the handle shows ([`Collapsed`](Self::Collapsed), [`Parked`](Self::Parked) or
    /// [`Revealing`](Self::Revealing)).
    #[must_use]
    pub const fn is_collapsed(self) -> bool {
        matches!(
            self,
            Self::Collapsed | Self::Parked | Self::Revealing { .. }
        )
    }

    /// A delay is running, so the host should send [`DockEvent::Tick`] every [`TICK`].
    #[must_use]
    pub const fn needs_tick(self) -> bool {
        matches!(self, Self::Revealing { .. } | Self::Leaving { .. })
    }

    /// The settings changed (or the Panel was docked or undocked): with the dock off a
    /// collapsed Panel opens, so that nothing stays hidden behind a trigger that no longer
    /// exists.
    pub fn apply_config(&mut self, cfg: &DockSettings) -> DockAction {
        if cfg.trigger != DockTrigger::Off {
            return DockAction::None;
        }
        let was_collapsed = self.is_collapsed();
        *self = Self::Expanded;
        if was_collapsed {
            DockAction::Expand
        } else {
            DockAction::None
        }
    }

    /// Takes `event`, which happened at `now`, and says what to do.
    pub fn on(&mut self, event: DockEvent, now: Timestamp, cfg: &DockSettings) -> DockAction {
        if cfg.trigger == DockTrigger::Off {
            return self.apply_config(cfg);
        }
        let hover_wait = i64::from(cfg.hover_delay_ms);
        let leave_wait = i64::from(cfg.leave_delay_ms);
        match (*self, event) {
            // The hotkey and a paste work in every state: they are the way back and the way
            // out. A collapsed Panel opens on the hotkey; an open one closes.
            (Self::Expanded | Self::Leaving { .. }, DockEvent::Hotkey | DockEvent::Pasted) => {
                self.collapse()
            }
            (Self::Collapsed | Self::Parked | Self::Revealing { .. }, DockEvent::Hotkey) => {
                self.expand()
            }
            // The pointer is off the handle. A Panel that just collapsed under it listens from
            // now on: resting on the handle is asking for the Panel again. One that was about to
            // open does not.
            (Self::Parked | Self::Revealing { .. }, DockEvent::HandleHover(false)) => {
                *self = Self::Collapsed;
                DockAction::None
            }

            (Self::Expanded, DockEvent::PanelHover(false)) => {
                if leave_wait == 0 {
                    self.collapse()
                } else {
                    *self = Self::Leaving { since: now };
                    DockAction::None
                }
            }
            (Self::Leaving { .. }, DockEvent::PanelHover(true)) => {
                *self = Self::Expanded;
                DockAction::None
            }
            (Self::Leaving { since }, DockEvent::Tick) if elapsed_ms(now, since) >= leave_wait => {
                self.collapse()
            }

            (Self::Collapsed, DockEvent::HandleHover(true)) if cfg.trigger.hover() => {
                if hover_wait == 0 {
                    self.expand()
                } else {
                    *self = Self::Revealing { since: now };
                    DockAction::None
                }
            }
            (Self::Revealing { since }, DockEvent::Tick)
                if elapsed_ms(now, since) >= hover_wait =>
            {
                self.expand()
            }
            (Self::Collapsed | Self::Parked | Self::Revealing { .. }, DockEvent::HandleClick)
                if cfg.trigger.click() =>
            {
                self.expand()
            }
            _ => DockAction::None,
        }
    }

    fn expand(&mut self) -> DockAction {
        *self = Self::Expanded;
        DockAction::Expand
    }

    fn collapse(&mut self) -> DockAction {
        *self = Self::Parked;
        DockAction::Collapse
    }
}

/// How close to an edge of the work area the cursor must be for a drag to offer the dock
/// there, in logical pixels. Only the cursor counts: the window's own sides never dock it.
pub const EDGE_ZONE: u32 = 4;
/// How far from that edge the cursor must go for the offer to end (logical pixels): wider
/// than [`EDGE_ZONE`], so the offer does not flicker at the zone's border.
pub const EDGE_EXIT: u32 = 12;
/// How long the cursor must stay at the edge before the dock is offered: crossing from one
/// monitor to the next, or brushing an edge, docks nothing.
pub const DOCK_ARM: Duration = Duration::from_millis(200);
/// How far across the edge (logical pixels) a docked Panel must be pulled to leave it; less
/// than that slides it along the edge.
pub const UNDOCK_PULL: u32 = 24;
/// How often the host moves the window to the cursor while a drag is under way.
pub const DRAG_TICK: Duration = Duration::from_millis(16);
/// How far past each end of the tucked tab, along the screen edge, the pointer already counts
/// as on it (logical pixels). Only along the edge: in front of the tab is another window (its
/// scrollbar, on a right edge), and resting there must not open the Panel.
pub const REVEAL_ZONE: u32 = 12;
/// How far around the open Panel the pointer still counts as on it (logical pixels). Not less
/// than [`REVEAL_ZONE`]: a pointer that opened the Panel from beside the tab must not be one
/// that has left it.
pub const KEEP_ZONE: u32 = 16;

/// Where a drag of the Panel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPhase {
    /// The window follows the cursor; released now, it floats where it is.
    Float,
    /// The cursor has stayed at this edge: the window is shown docked there; released now, it
    /// docks.
    Preview(DockEdge),
    /// A docked Panel being slid along its edge; released now, it stays docked.
    Slide(DockEdge),
}

/// The Panel's sizes on a monitor (physical pixels): docking and undocking only ever choose
/// between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragSizes {
    /// While it floats.
    pub floating: (u32, u32),
    /// Docked to the left or the right edge.
    pub side: (u32, u32),
    /// Docked to the top or the bottom edge.
    pub flat: (u32, u32),
}

impl DragSizes {
    /// The size docked to `edge`.
    #[must_use]
    pub fn docked_on(&self, edge: DockEdge) -> (u32, u32) {
        if edge.vertical() {
            self.side
        } else {
            self.flat
        }
    }
}

/// What a drag step needs to know about the monitor it is on. For [`DragPhase::Slide`] that is
/// the monitor the Panel is docked on; otherwise the one under the cursor.
#[derive(Debug, Clone, Copy)]
pub struct DragCtx<'a> {
    /// The monitor.
    pub monitor: &'a Monitor,
    /// The Panel's sizes there.
    pub sizes: DragSizes,
    /// "Docked panel uses the full edge".
    pub full_edge: bool,
    /// The moment of the step, from the injected clock.
    pub now: Timestamp,
}

/// What the host does to the window after a drag step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragStep {
    /// Nothing: the window is where it should be.
    None,
    /// Move the window's top-left here; its size stays.
    Move(i32, i32),
    /// Place the window at this frame (a size may change: the preview, or leaving an edge).
    Place(Rect),
}

/// How a drag ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragEnd {
    /// The Panel floats at this frame (inside the work area).
    Floating(Rect),
    /// The Panel is docked to the edge, at this frame.
    Docked(DockEdge, Rect),
}

/// A drag of the Panel by its header, driven by the cursor on screen. The app moves the window
/// itself, so it knows where the cursor is and when the button comes up; the dock is decided by
/// the cursor alone ([`EDGE_ZONE`], [`DOCK_ARM`]). All positions are physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    phase: DragPhase,
    /// The cursor's offset inside the window that follows it.
    grab: (i32, i32),
    /// The width of the window `grab` was measured on.
    grab_w: u32,
    /// 1000 logical pixels at the scale of the monitor `grab` was measured on (0 until the
    /// first step says what monitor that is).
    grab_unit: i64,
    /// Where the cursor was pressed.
    press: (i32, i32),
    /// Where the cursor was last seen.
    cursor: (i32, i32),
    /// The edge the cursor is at (within [`EDGE_ZONE`] of it: the nearest when it came there,
    /// kept while it stays in that zone), and since when.
    candidate: Option<(DockEdge, Timestamp)>,
    /// The frame last asked for.
    frame: Rect,
}

fn px(logical: u32, scale: f32) -> i64 {
    let scale = if scale.is_finite() && scale > 0.0 {
        f64::from(scale)
    } else {
        1.0
    };
    #[allow(clippy::cast_possible_truncation, reason = "a few pixels, rounded")]
    let pixels = (f64::from(logical) * scale).round() as i64;
    pixels
}

fn sub(a: (i32, i32), b: (i32, i32)) -> (i32, i32) {
    (a.0.saturating_sub(b.0), a.1.saturating_sub(b.1))
}

impl Drag {
    /// The press: the cursor on screen, the window's frame, and the edge it is docked to.
    #[must_use]
    pub fn begin(cursor: (i32, i32), window: Rect, docked: Option<DockEdge>) -> Self {
        Self {
            phase: docked.map_or(DragPhase::Float, DragPhase::Slide),
            grab: sub(cursor, (window.x, window.y)),
            grab_w: window.w,
            grab_unit: 0,
            press: cursor,
            cursor,
            candidate: None,
            frame: window,
        }
    }

    /// Where the drag is.
    #[must_use]
    pub const fn phase(&self) -> DragPhase {
        self.phase
    }

    /// The edge the Panel docks to if released now, while that is being shown.
    #[must_use]
    pub const fn preview(&self) -> Option<DockEdge> {
        match self.phase {
            DragPhase::Preview(edge) => Some(edge),
            _ => None,
        }
    }

    /// The frame last asked for.
    #[must_use]
    pub const fn frame(&self) -> Rect {
        self.frame
    }

    /// Where the cursor was last seen.
    #[must_use]
    pub const fn cursor(&self) -> (i32, i32) {
        self.cursor
    }

    /// The window follows the cursor at its floating size from now on.
    fn float(&mut self, ctx: &DragCtx<'_>) -> DragStep {
        // The cursor keeps its place across the header when the width changes under it.
        // Measured from the window the header was pressed on: a preview in between (another
        // width) must not shift it.
        let (old_w, new_w) = (
            i64::from(self.grab_w.max(1)),
            i64::from(ctx.sizes.floating.0),
        );
        let across = i64::from(self.grab.0) * new_w / old_w;
        // And its place down the header when the monitor's scale does: the header is that
        // much taller or shorter there.
        let unit = px(1000, ctx.monitor.scale);
        let old_unit = if self.grab_unit > 0 {
            self.grab_unit
        } else {
            unit
        };
        let down = i64::from(self.grab.1) * unit / old_unit;
        let limit = |v: i64, len: u32| {
            i32::try_from(v.clamp(0, i64::from(len.saturating_sub(1)))).unwrap_or(0)
        };
        self.grab = (
            limit(across, ctx.sizes.floating.0),
            limit(down, ctx.sizes.floating.1),
        );
        self.grab_w = ctx.sizes.floating.0;
        self.grab_unit = unit;
        self.phase = DragPhase::Float;
        self.candidate = None;
        let (x, y) = sub(self.cursor, self.grab);
        self.frame = Rect {
            x,
            y,
            w: ctx.sizes.floating.0,
            h: ctx.sizes.floating.1,
        };
        DragStep::Place(self.frame)
    }

    fn docked_at(&self, edge: DockEdge, ctx: &DragCtx<'_>) -> Rect {
        let size = ctx.sizes.docked_on(edge);
        // The cursor keeps its place across the header at the docked width too: on a top or
        // bottom edge a narrower dock would otherwise be shown beside the cursor, not under it.
        let across = i64::from(self.grab.0) * i64::from(size.0) / i64::from(self.grab_w.max(1));
        let across = i32::try_from(across).unwrap_or(self.grab.0);
        let (x, y) = sub(self.cursor, (across, self.grab.1));
        frame::docked_rect(
            edge,
            ctx.monitor.work_area,
            size,
            ctx.monitor.scale,
            Some(if edge.vertical() { y } else { x }),
            ctx.full_edge,
        )
    }

    fn place(&mut self, rect: Rect) -> DragStep {
        if rect == self.frame {
            return DragStep::None;
        }
        self.frame = rect;
        DragStep::Place(rect)
    }

    /// The cursor is at `cursor` (or time passed with it still: the host calls this every
    /// [`DRAG_TICK`]). Says where the window goes.
    pub fn moved(&mut self, cursor: (i32, i32), ctx: &DragCtx<'_>) -> DragStep {
        self.cursor = cursor;
        let (area, scale) = (ctx.monitor.work_area, ctx.monitor.scale);
        if self.grab_unit == 0 {
            self.grab_unit = px(1000, scale);
        }
        match self.phase {
            DragPhase::Slide(edge) => {
                let pulled = sub(cursor, self.press);
                // Away from the edge it leaves. Pushed into the edge it has nowhere to go (and
                // letting go of the edge there would dock it again a moment later), unless
                // the cursor does: past an edge another monitor begins at, it is on its way
                // there.
                let away = match edge {
                    DockEdge::Left => pulled.0,
                    DockEdge::Right => pulled.0.saturating_neg(),
                    DockEdge::Top => pulled.1,
                    DockEdge::Bottom => pulled.1.saturating_neg(),
                };
                let past = frame::edge_gap(cursor, area, edge).saturating_neg();
                if i64::from(away).max(past) > px(UNDOCK_PULL, scale) {
                    return self.float(ctx);
                }
                let rect = self.docked_at(edge, ctx);
                self.place(rect)
            }
            DragPhase::Preview(edge) => {
                if frame::edge_gap(cursor, area, edge) > px(EDGE_EXIT, scale) {
                    return self.float(ctx);
                }
                let rect = self.docked_at(edge, ctx);
                self.place(rect)
            }
            DragPhase::Float => {
                // A press that has not moved is a click, not a push to the edge.
                let pushed = cursor != self.press;
                if pushed && self.rescaled_under(ctx) {
                    // Onto a monitor of another scale: Windows resizes the window there, so
                    // the offset the cursor holds it by is measured again. Another size at
                    // the same scale waits for the drop: a cursor wobbling over the seam must
                    // not resize the window each time it crosses.
                    return self.float(ctx);
                }
                let zone = px(EDGE_ZONE, scale);
                // The edge the cursor came to is kept while it stays in that edge's zone, even
                // where another edge is nearer (a hand wobbles in a corner); out of it, the
                // nearest edge is the one, and its wait starts.
                let held = self.candidate.filter(|(edge, _)| {
                    pushed
                        && Some(*edge) != ctx.monitor.taskbar
                        && frame::edge_gap(cursor, area, *edge) <= zone
                });
                self.candidate = held.or_else(|| {
                    pushed
                        .then(|| frame::edge_at(cursor, area, ctx.monitor.taskbar, zone))
                        .flatten()
                        .map(|edge| (edge, ctx.now))
                });
                let armed = self.candidate.filter(|(_, since)| {
                    elapsed_ms(ctx.now, *since)
                        >= i64::try_from(DOCK_ARM.as_millis()).unwrap_or(i64::MAX)
                });
                if let Some((edge, _)) = armed {
                    self.phase = DragPhase::Preview(edge);
                    let rect = self.docked_at(edge, ctx);
                    self.frame = rect;
                    return DragStep::Place(rect);
                }
                let (x, y) = sub(cursor, self.grab);
                if (x, y) == (self.frame.x, self.frame.y) {
                    return DragStep::None;
                }
                self.frame = Rect { x, y, ..self.frame };
                DragStep::Move(x, y)
            }
        }
    }

    /// The button came up: where the Panel ends. A floating Panel is pulled inside the work
    /// area of the monitor it was dropped on, at its floating size there.
    #[must_use]
    pub fn released(mut self, ctx: &DragCtx<'_>) -> DragEnd {
        match self.phase {
            DragPhase::Preview(edge) | DragPhase::Slide(edge) => DragEnd::Docked(edge, self.frame),
            DragPhase::Float => {
                if self.resized_under(ctx) {
                    let _ = self.float(ctx);
                }
                DragEnd::Floating(frame::clamp(self.frame, ctx.monitor))
            }
        }
    }

    /// The floating window is not what `ctx` says it is there: another size, or a monitor of
    /// another scale than the one the grab was measured on.
    fn resized_under(&self, ctx: &DragCtx<'_>) -> bool {
        (self.frame.w, self.frame.h) != ctx.sizes.floating || self.rescaled_under(ctx)
    }

    /// The monitor in `ctx` has another scale than the one the grab was measured on.
    fn rescaled_under(&self, ctx: &DragCtx<'_>) -> bool {
        self.grab_unit > 0 && self.grab_unit != px(1000, ctx.monitor.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frozen start; every test moves from it by a number of milliseconds.
    const BASE: i64 = 1_760_000_000_000;

    fn at(ms: i64) -> Timestamp {
        Timestamp::from_millisecond(BASE + ms).expect("a valid moment")
    }

    fn cfg(trigger: DockTrigger) -> DockSettings {
        DockSettings {
            trigger,
            hover_delay_ms: 150,
            leave_delay_ms: 400,
            full_edge: false,
            ..DockSettings::default()
        }
    }

    use DockEvent::{HandleClick, HandleHover, Hotkey, PanelHover, Pasted, Tick};

    #[test]
    fn off_is_always_expanded_and_every_event_does_nothing() {
        let off = cfg(DockTrigger::Off);
        for event in [
            HandleHover(true),
            HandleHover(false),
            HandleClick,
            PanelHover(true),
            PanelHover(false),
            Hotkey,
            Pasted,
            Tick,
        ] {
            let mut state = DockState::Expanded;
            assert_eq!(state.on(event, at(0), &off), DockAction::None, "{event:?}");
            assert_eq!(state, DockState::Expanded, "{event:?}");
        }
    }

    #[test]
    fn switching_the_dock_off_opens_a_collapsed_panel() {
        for start in [DockState::Collapsed, DockState::Revealing { since: at(0) }] {
            let mut state = start;
            assert_eq!(
                state.apply_config(&cfg(DockTrigger::Off)),
                DockAction::Expand
            );
            assert_eq!(state, DockState::Expanded);
        }
        let mut open = DockState::Leaving { since: at(0) };
        assert_eq!(open.apply_config(&cfg(DockTrigger::Off)), DockAction::None);
        assert_eq!(open, DockState::Expanded, "no delay left running");
        let mut state = DockState::Collapsed;
        assert_eq!(
            state.apply_config(&cfg(DockTrigger::Hover)),
            DockAction::None
        );
        assert_eq!(
            state,
            DockState::Collapsed,
            "an on dock leaves the state alone"
        );
    }

    #[test]
    fn a_stale_event_with_the_dock_off_still_opens_a_collapsed_panel() {
        let mut state = DockState::Collapsed;
        assert_eq!(
            state.on(HandleClick, at(0), &cfg(DockTrigger::Off)),
            DockAction::Expand
        );
    }

    #[test]
    fn resting_on_the_handle_opens_it_after_the_hover_delay() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Collapsed;
        assert_eq!(
            state.on(HandleHover(true), at(1000), &hover),
            DockAction::None
        );
        assert_eq!(state, DockState::Revealing { since: at(1000) });
        assert!(state.needs_tick() && state.is_collapsed());
        assert_eq!(state.on(Tick, at(1100), &hover), DockAction::None);
        assert_eq!(state.on(Tick, at(1149), &hover), DockAction::None);
        assert_eq!(state.on(Tick, at(1150), &hover), DockAction::Expand);
        assert_eq!(state, DockState::Expanded);
    }

    #[test]
    fn leaving_the_handle_before_the_delay_cancels_the_reveal() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Collapsed;
        state.on(HandleHover(true), at(0), &hover);
        assert_eq!(
            state.on(HandleHover(false), at(100), &hover),
            DockAction::None
        );
        assert_eq!(state, DockState::Collapsed);
        assert_eq!(
            state.on(Tick, at(900), &hover),
            DockAction::None,
            "nothing is running"
        );
    }

    #[test]
    fn a_zero_hover_delay_opens_at_once() {
        let mut state = DockState::Collapsed;
        let now = DockSettings {
            hover_delay_ms: 0,
            ..cfg(DockTrigger::Hover)
        };
        assert_eq!(state.on(HandleHover(true), at(0), &now), DockAction::Expand);
        assert_eq!(state, DockState::Expanded);
    }

    #[test]
    fn the_hover_delay_follows_the_setting() {
        let slow = DockSettings {
            hover_delay_ms: 1000,
            ..cfg(DockTrigger::Both)
        };
        let mut state = DockState::Collapsed;
        state.on(HandleHover(true), at(0), &slow);
        assert_eq!(state.on(Tick, at(999), &slow), DockAction::None);
        assert_eq!(state.on(Tick, at(1000), &slow), DockAction::Expand);
    }

    #[test]
    fn a_click_opens_the_handle_when_the_trigger_has_click() {
        for trigger in [DockTrigger::Click, DockTrigger::Both] {
            let mut state = DockState::Collapsed;
            assert_eq!(
                state.on(HandleClick, at(0), &cfg(trigger)),
                DockAction::Expand,
                "{trigger:?}"
            );
            assert_eq!(state, DockState::Expanded);
        }
    }

    #[test]
    fn a_click_during_the_hover_delay_opens_at_once() {
        let both = cfg(DockTrigger::Both);
        let mut state = DockState::Collapsed;
        state.on(HandleHover(true), at(0), &both);
        assert_eq!(state.on(HandleClick, at(40), &both), DockAction::Expand);
        assert_eq!(state, DockState::Expanded);
    }

    #[test]
    fn a_click_does_nothing_with_a_hover_only_trigger() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Collapsed;
        assert_eq!(state.on(HandleClick, at(0), &hover), DockAction::None);
        assert_eq!(state, DockState::Collapsed);
    }

    #[test]
    fn hovering_does_nothing_with_a_click_only_trigger() {
        let click = cfg(DockTrigger::Click);
        let mut state = DockState::Collapsed;
        assert_eq!(state.on(HandleHover(true), at(0), &click), DockAction::None);
        assert_eq!(state, DockState::Collapsed, "no hover delay starts");
        assert_eq!(state.on(Tick, at(5000), &click), DockAction::None);
    }

    #[test]
    fn both_opens_on_either() {
        let both = cfg(DockTrigger::Both);
        let mut by_hover = DockState::Collapsed;
        by_hover.on(HandleHover(true), at(0), &both);
        assert_eq!(by_hover.on(Tick, at(150), &both), DockAction::Expand);
        let mut by_click = DockState::Collapsed;
        assert_eq!(by_click.on(HandleClick, at(0), &both), DockAction::Expand);
    }

    #[test]
    fn leaving_the_open_panel_collapses_it_after_the_leave_delay() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Expanded;
        assert_eq!(state.on(PanelHover(false), at(0), &hover), DockAction::None);
        assert_eq!(state, DockState::Leaving { since: at(0) });
        assert!(state.needs_tick() && !state.is_collapsed());
        assert_eq!(state.on(Tick, at(399), &hover), DockAction::None);
        assert_eq!(state.on(Tick, at(400), &hover), DockAction::Collapse);
        assert_eq!(state, DockState::Parked);
        assert!(!state.needs_tick());
    }

    #[test]
    fn coming_back_before_the_leave_delay_keeps_the_panel_open() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Expanded;
        state.on(PanelHover(false), at(0), &hover);
        assert_eq!(
            state.on(PanelHover(true), at(300), &hover),
            DockAction::None
        );
        assert_eq!(state, DockState::Expanded);
        assert_eq!(
            state.on(Tick, at(2000), &hover),
            DockAction::None,
            "nothing is running"
        );
    }

    #[test]
    fn a_second_leave_report_does_not_restart_the_delay() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Expanded;
        state.on(PanelHover(false), at(0), &hover);
        state.on(PanelHover(false), at(300), &hover);
        assert_eq!(state.on(Tick, at(400), &hover), DockAction::Collapse);
    }

    #[test]
    fn a_zero_leave_delay_collapses_at_once() {
        let now = DockSettings {
            leave_delay_ms: 0,
            ..cfg(DockTrigger::Click)
        };
        let mut state = DockState::Expanded;
        assert_eq!(
            state.on(PanelHover(false), at(0), &now),
            DockAction::Collapse
        );
        assert_eq!(state, DockState::Parked);
    }

    #[test]
    fn the_leave_delay_applies_to_a_click_trigger_too() {
        let click = cfg(DockTrigger::Click);
        let mut state = DockState::Expanded;
        state.on(PanelHover(false), at(0), &click);
        assert_eq!(state.on(Tick, at(400), &click), DockAction::Collapse);
    }

    #[test]
    fn the_hotkey_toggles_at_once() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Expanded;
        assert_eq!(state.on(Hotkey, at(0), &hover), DockAction::Collapse);
        assert_eq!(state, DockState::Parked);
        assert_eq!(state.on(Hotkey, at(10), &hover), DockAction::Expand);
        assert_eq!(state, DockState::Expanded);
    }

    #[test]
    fn the_hotkey_works_with_a_click_trigger_and_in_the_middle_of_a_delay() {
        let click = cfg(DockTrigger::Click);
        let mut state = DockState::Collapsed;
        assert_eq!(state.on(Hotkey, at(0), &click), DockAction::Expand);

        let hover = cfg(DockTrigger::Hover);
        let mut revealing = DockState::Revealing { since: at(0) };
        assert_eq!(revealing.on(Hotkey, at(50), &hover), DockAction::Expand);
        let mut leaving = DockState::Leaving { since: at(0) };
        assert_eq!(leaving.on(Hotkey, at(50), &hover), DockAction::Collapse);
        assert_eq!(leaving, DockState::Parked);
    }

    #[test]
    fn a_paste_collapses_an_open_panel() {
        let hover = cfg(DockTrigger::Hover);
        for start in [DockState::Expanded, DockState::Leaving { since: at(0) }] {
            let mut state = start;
            assert_eq!(state.on(Pasted, at(10), &hover), DockAction::Collapse);
            assert_eq!(state, DockState::Parked);
        }
    }

    #[test]
    fn a_paste_while_collapsed_does_nothing() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Collapsed;
        assert_eq!(state.on(Pasted, at(0), &hover), DockAction::None);
        let mut revealing = DockState::Revealing { since: at(0) };
        assert_eq!(revealing.on(Pasted, at(0), &hover), DockAction::None);
        assert_eq!(revealing, DockState::Revealing { since: at(0) });
    }

    #[test]
    fn pointer_reports_for_the_other_part_are_ignored() {
        let both = cfg(DockTrigger::Both);
        let mut collapsed = DockState::Collapsed;
        assert_eq!(
            collapsed.on(PanelHover(false), at(0), &both),
            DockAction::None
        );
        assert_eq!(
            collapsed.on(PanelHover(true), at(0), &both),
            DockAction::None
        );
        assert_eq!(
            collapsed,
            DockState::Collapsed,
            "the window shrinking is not a leave"
        );
        let mut open = DockState::Expanded;
        assert_eq!(open.on(HandleHover(true), at(0), &both), DockAction::None);
        assert_eq!(open.on(HandleClick, at(0), &both), DockAction::None);
        assert_eq!(open.on(Tick, at(9999), &both), DockAction::None);
        assert_eq!(open, DockState::Expanded);
    }

    #[test]
    fn a_clock_that_goes_backwards_waits_rather_than_acting() {
        let hover = cfg(DockTrigger::Hover);
        let mut state = DockState::Revealing { since: at(1000) };
        assert_eq!(state.on(Tick, at(0), &hover), DockAction::None);
    }

    #[test]
    fn a_panel_that_just_collapsed_ignores_a_pointer_that_never_left_the_handle() {
        let cfg = cfg(DockTrigger::Both);
        for close in [Hotkey, Pasted] {
            let mut state = DockState::Expanded;
            assert_eq!(state.on(close, at(0), &cfg), DockAction::Collapse);
            assert_eq!(state, DockState::Parked);
            // The handle appeared under the pointer: that is not a hover.
            assert_eq!(state.on(HandleHover(true), at(50), &cfg), DockAction::None);
            assert_eq!(state.on(Tick, at(5_000), &cfg), DockAction::None);
            assert!(state.is_collapsed() && !state.needs_tick(), "{close:?}");
            // Off it and back on: now it is one.
            assert_eq!(
                state.on(HandleHover(false), at(5_100), &cfg),
                DockAction::None
            );
            assert_eq!(state, DockState::Collapsed);
            assert_eq!(
                state.on(HandleHover(true), at(5_200), &cfg),
                DockAction::None
            );
            assert_eq!(state.on(Tick, at(5_350), &cfg), DockAction::Expand);
        }
        // Leaving the open Panel collapses it the same way.
        let mut left = DockState::Expanded;
        left.on(PanelHover(false), at(0), &cfg);
        assert_eq!(left.on(Tick, at(400), &cfg), DockAction::Collapse);
        assert_eq!(left, DockState::Parked);
        // A click and the hotkey need no leaving first.
        for open in [HandleClick, Hotkey] {
            let mut state = DockState::Parked;
            assert_eq!(
                state.on(open, at(100), &cfg),
                DockAction::Expand,
                "{open:?}"
            );
        }
    }

    #[test]
    fn the_open_panel_is_kept_wherever_the_tab_would_reveal_it() {
        // Else a pointer between the two zones would open and close the Panel for ever.
        const { assert!(KEEP_ZONE >= REVEAL_ZONE) };
    }

    /// Every way of feeding up to five events, 100 ms apart, to a dock in each trigger mode.
    fn each_sequence(len: usize, mut check: impl FnMut(&[DockEvent])) {
        const EVENTS: [DockEvent; 8] = [
            HandleHover(true),
            HandleHover(false),
            HandleClick,
            PanelHover(true),
            PanelHover(false),
            Hotkey,
            Pasted,
            Tick,
        ];
        let mut counters = vec![0usize; len];
        loop {
            let sequence: Vec<DockEvent> = counters.iter().map(|&i| EVENTS[i]).collect();
            check(&sequence);
            let mut place = 0;
            loop {
                if place == len {
                    return;
                }
                counters[place] += 1;
                if counters[place] < EVENTS.len() {
                    break;
                }
                counters[place] = 0;
                place += 1;
            }
        }
    }

    #[test]
    fn there_is_never_a_hidden_panel_without_a_way_back() {
        let triggers = DockTrigger::ALL;
        for trigger in triggers {
            let cfg = cfg(trigger);
            for len in 1..=5 {
                each_sequence(len, |events| {
                    let mut state = DockState::Expanded;
                    for (step, &event) in events.iter().enumerate() {
                        let before = state;
                        let now = at(i64::try_from(step).expect("small") * 100);
                        let action = state.on(event, now, &cfg);
                        let story = format!(
                            "{trigger:?} {events:?} at step {step}: {before:?} -> {state:?}"
                        );

                        // A collapsed Panel exists only while the dock is on, so a handle exists.
                        assert!(
                            !state.is_collapsed() || trigger != DockTrigger::Off,
                            "{story}"
                        );
                        // The action says exactly what changed.
                        match action {
                            DockAction::Expand => {
                                assert!(
                                    before.is_collapsed() && state == DockState::Expanded,
                                    "{story}"
                                );
                            }
                            DockAction::Collapse => {
                                assert!(
                                    !before.is_collapsed() && state == DockState::Parked,
                                    "{story}"
                                );
                            }
                            DockAction::None => {
                                assert_eq!(before.is_collapsed(), state.is_collapsed(), "{story}");
                            }
                        }
                        // A running delay needs the dock on, or nothing would ever end it.
                        assert!(
                            !state.needs_tick() || trigger != DockTrigger::Off,
                            "{story}"
                        );
                        // The hotkey always brings a collapsed Panel back.
                        if state.is_collapsed() {
                            let mut probe = state;
                            assert_eq!(probe.on(Hotkey, now, &cfg), DockAction::Expand, "{story}");
                            assert_eq!(probe, DockState::Expanded, "{story}");
                        }
                        // With the dock off nothing ever collapses.
                        if trigger == DockTrigger::Off {
                            assert_eq!(state, DockState::Expanded, "{story}");
                            assert_eq!(action, DockAction::None, "{story}");
                        }
                    }
                });
            }
        }
    }

    #[test]
    fn a_collapsed_panel_can_always_be_opened_by_the_pointer_or_the_hotkey() {
        // Hover: rest on the handle. Click: click it. Both: either. Whatever came before.
        for trigger in [DockTrigger::Hover, DockTrigger::Click, DockTrigger::Both] {
            let cfg = cfg(trigger);
            for len in 1..=4 {
                each_sequence(len, |events| {
                    let mut state = DockState::Expanded;
                    for (step, &event) in events.iter().enumerate() {
                        state.on(event, at(i64::try_from(step).expect("small") * 100), &cfg);
                    }
                    if !state.is_collapsed() {
                        return;
                    }
                    let mut probe = state;
                    // Back to a plain collapsed handle first, then use the trigger.
                    probe.on(HandleHover(false), at(1000), &cfg);
                    let opened = if trigger.hover() {
                        probe.on(HandleHover(true), at(1100), &cfg);
                        probe.on(Tick, at(1250), &cfg)
                    } else {
                        probe.on(HandleClick, at(1100), &cfg)
                    };
                    assert_eq!(opened, DockAction::Expand, "{trigger:?} {events:?}");
                });
            }
        }
    }

    include!("dock_drag_tests.rs");
}
