//! The edge dock's half of the [`Controller`] (SPEC 5.1 "Edge dock with reveal"): it feeds the
//! pure state machine of [`crate::dock`] with what the window and the hotkey report, turns its
//! answer into window placements (the handle strip or the open Panel, from [`crate::frame`]),
//! and keeps the edge each monitor was docked to. It also runs the drag ([`crate::dock::Drag`]):
//! the app moves the window from the cursor, and the cursor alone decides a dock. Slint-free:
//! `binding` runs the timers that call [`Controller::dock_tick`] and [`Controller::drag_move`].

use snipdeck_core::UseOutcome;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, Library, PanelMenu, UseResult, neutral};
use crate::{
    dock::{
        self, DockAction, DockEvent, DockState, Drag, DragCtx, DragEnd, DragPhase, DragSizes,
        DragStep,
    },
    frame::{self, Monitor, Rect},
    host::ClipboardText,
    settings::{DockEdge, DockSettings, DockTrigger, PanelGeometry},
    toast::ToastMessage,
};

/// The height a Panel starts with when nothing was stored for its monitor (logical pixels).
pub(super) const DEFAULT_HEIGHT: u32 = 480;
/// How far from the work area's corner a Panel with no stored place opens (logical pixels).
const OPEN_INSET: u32 = 48;

/// How long after a docking a leave reported by the window is the window having moved from
/// under the pointer. Only used where the cursor cannot be read on screen.
const FRESH_LEAVE_MS: i64 = 250;

/// `logical` pixels at `scale`, for a zone around a rectangle.
fn zone(logical: u32, scale: f32) -> i64 {
    i64::from(frame::scaled(logical, scale))
}

/// Where a drag step happens: the monitor and the Panel's sizes there (physical pixels).
struct DragPlace {
    monitor: Monitor,
    sizes: DragSizes,
}

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// The edge the Panel is docked to on the monitor it is on, if it is docked.
    #[must_use]
    pub fn dock_edge(&self) -> Option<DockEdge> {
        self.stored_geometry(self.monitor.as_ref()?)?.edge
    }

    /// The dock settings in force: a floating Panel has no handle, so for it the trigger is off.
    fn dock_config(&self) -> DockSettings {
        let mut config = self.settings.dock;
        if self.dock_edge().is_none() {
            config.trigger = DockTrigger::Off;
        }
        config
    }

    /// The Panel is docked and may collapse (an edge is set and the trigger is not off).
    #[must_use]
    pub fn dock_active(&self) -> bool {
        self.dock_config().trigger != DockTrigger::Off
    }

    /// Only the handle shows now.
    #[must_use]
    pub fn dock_collapsed(&self) -> bool {
        self.dock.is_collapsed()
    }

    /// A hover or leave delay is running.
    #[must_use]
    pub fn dock_needs_tick(&self) -> bool {
        self.dock.needs_tick()
    }

    pub(super) fn stored_geometry(&self, monitor: &Monitor) -> Option<&PanelGeometry> {
        self.settings
            .panel_geometry
            .iter()
            .find(|g| g.monitor_id == monitor.id)
    }

    /// Something happened to the handle or the open Panel (or the hotkey, a paste or time):
    /// runs the state machine and returns the placement it asks for.
    pub fn dock_event(&mut self, event: DockEvent) -> Vec<Effect> {
        let config = self.dock_config();
        let now = self.clock.now();
        let action = self.dock.on(event, now, &config);
        self.dock_act(action)
    }

    /// The window reported the pointer on or off the handle or the Panel. Only used where
    /// the cursor cannot be read on screen: where it can, [`dock_tick`](Self::dock_tick) looks
    /// for itself, and a window's own enter and leave (which a click-through or a covered
    /// strip can miss) would only disagree with it.
    pub fn dock_hover(&mut self, event: DockEvent) -> Vec<Effect> {
        if self.target.cursor().is_some() {
            return Vec::new();
        }
        if event == DockEvent::PanelHover(true) {
            self.dock_fresh = false;
        } else if event == DockEvent::PanelHover(false) && self.dock_fresh {
            // A window that went to its edge from under the pointer reports that leave at once:
            // it is the docking, not the pointer. A later one is the pointer going, from a
            // Panel that stayed under it (so no enter was ever reported).
            let since = self.clock.now().as_millisecond() - self.docked_at.as_millisecond();
            if since < FRESH_LEAVE_MS {
                return Vec::new();
            }
            self.dock_fresh = false;
        }
        // A window may report a leave when the pointer only moved onto its own menu: under an
        // open menu a leave is not believed.
        if event == DockEvent::PanelHover(false) && self.menu != PanelMenu::None {
            return Vec::new();
        }
        self.dock_event(event)
    }

    /// A full-screen app is in front and the Panel has not the keyboard: the docked Panel is
    /// out of its way, not above it and not sliding out under a pointer at the screen edge.
    pub(super) fn dock_covered(&self) -> bool {
        self.fullscreen && !self.panel_focused
    }

    /// The Panel is docked, may tuck away and is on screen: the host calls
    /// [`dock_tick`](Self::dock_tick) every [`dock::TICK`] while this is true.
    #[must_use]
    pub fn dock_live(&self) -> bool {
        self.visible && self.dock_active()
    }

    /// The open docked Panel, its tucked tab and its edge, on its monitor.
    fn dock_rects(&self) -> Option<(Rect, Rect, DockEdge)> {
        let monitor = self.monitor.as_ref()?;
        let geometry = self.stored_geometry(monitor)?;
        let edge = geometry.edge?;
        let open = self.dock_open(geometry, edge, monitor);
        let tab = frame::handle_rect(edge, monitor.work_area, open.along(edge), monitor.scale);
        Some((open, tab, edge))
    }

    /// The size the Panel docks at on `edge` of `monitor`, from what `geometry` remembers there:
    /// the size it was given on that kind of edge, else its floating size, never a full-height
    /// one the user did not choose ([`frame::dock_size`]). `along` is where it will sit, `None`
    /// for a dock that was stored without one.
    pub(super) fn dock_size(
        &self,
        geometry: &PanelGeometry,
        edge: DockEdge,
        monitor: &Monitor,
        along: Option<i32>,
    ) -> (u32, u32) {
        frame::dock_size(
            edge,
            geometry.docked_size(edge),
            (geometry.w, geometry.h),
            monitor.work_area,
            monitor.scale,
            DEFAULT_HEIGHT,
            along,
            self.settings.dock.full_edge,
        )
    }

    /// The open Panel's frame on `monitor`, docked to `edge` as `geometry` remembers it.
    pub(super) fn dock_open(
        &self,
        geometry: &PanelGeometry,
        edge: DockEdge,
        monitor: &Monitor,
    ) -> Rect {
        frame::docked_rect(
            edge,
            monitor.work_area,
            self.dock_size(geometry, edge, monitor, geometry.along),
            monitor.scale,
            geometry.along,
            self.settings.dock.full_edge,
        )
    }

    /// The Panel is in use, so it stays out whatever the pointer does: it has the keyboard, or
    /// it is being dragged or resized. An open menu is not that by itself: it is drawn inside
    /// the Panel, so the pointer on it is on the Panel, and one left open with the pointer and
    /// the keyboard both gone must not keep the Panel out for ever.
    fn dock_held(&mut self) -> bool {
        // A resize lasts as long as its button is down; where that cannot be read it holds
        // nothing.
        if self.resizing && self.target.primary_down() != Some(true) {
            self.resizing = false;
        }
        self.panel_focused || self.drag.is_some() || self.resizing
    }

    /// The Panel is on `monitor` now (the system moved its window there), or `monitor` is not
    /// what it was (its work area changed): the dock follows what is remembered for it. A
    /// Panel that is not docked there opens floating where it was left.
    pub(super) fn dock_on_monitor(&mut self, monitor: &Monitor) -> Vec<Effect> {
        self.monitor = Some(monitor.clone());
        let config = self.dock_config();
        let _ = self.dock.apply_config(&config);
        if self.dock_edge().is_some() {
            self.dock_place(self.dock.is_collapsed())
        } else {
            self.placement(monitor)
                .map(Effect::PlaceWindow)
                .into_iter()
                .collect()
        }
    }

    /// The work area of the Panel's monitor changed since the dock's timer last looked (the
    /// taskbar moved or changed size; no window is told): the dock is placed again. Only a
    /// change seen by the same reader counts, so two readers that disagree cannot make it
    /// place for ever.
    fn dock_area_refresh(&mut self) -> Vec<Effect> {
        if self.drag.is_some() {
            return Vec::new();
        }
        let Some((open, tab, _)) = self.dock_rects() else {
            return Vec::new();
        };
        let on = if self.dock.is_collapsed() { tab } else { open };
        let centre = |start: i32, len: u32| start.saturating_add_unsigned(len / 2);
        let Some(now) = self
            .target
            .monitor_at(centre(on.x, on.w), centre(on.y, on.h))
        else {
            return Vec::new();
        };
        let before = self.dock_area_seen.replace((now.id.clone(), now.work_area));
        let Some(own) = self.monitor.clone().filter(|m| m.id == now.id) else {
            return Vec::new();
        };
        let changed = before.is_some_and(|(id, area)| id == now.id && area != now.work_area);
        if !changed || own.work_area == now.work_area {
            return Vec::new();
        }
        self.dock_on_monitor(&Monitor {
            work_area: now.work_area,
            taskbar: now.taskbar,
            ..own
        })
    }

    /// The Panel's window gained or lost the keyboard. A docked Panel that has it stays out.
    pub fn set_panel_focused(&mut self, focused: bool) {
        self.panel_focused = focused;
    }

    /// Time passed (the host calls this every [`dock::TICK`] while
    /// [`dock_live`](Self::dock_live)): reads where the pointer is, when it can be read, and
    /// runs the hover and leave delays. The tucked tab reveals when the pointer is on it, or at
    /// the screen edge within [`dock::REVEAL_ZONE`] of its ends ([`frame::on_tab`]), once it
    /// has been off it since the Panel tucked; the open Panel stays while the pointer is within
    /// [`dock::KEEP_ZONE`] of it or it is in use.
    pub fn dock_tick(&mut self) -> Vec<Effect> {
        if !self.dock_live() {
            return Vec::new();
        }
        let mut effects = self.dock_area_refresh();
        let scale = self.monitor.as_ref().map_or(1.0, |m| m.scale);
        let held = self.dock_held();
        self.fullscreen = self.target.fullscreen_in_front();
        if self.dock_covered() {
            // A full-screen app is in front: the pointer at its edge is the app's, and a Panel
            // left open over it goes to its tab (which is under the app until that ends).
            let event = if self.dock.is_collapsed() {
                DockEvent::HandleHover(false)
            } else {
                DockEvent::PanelHover(held)
            };
            effects.extend(self.dock_event(event));
        } else if let (Some(cursor), Some((open, tab, edge))) =
            (self.target.cursor(), self.dock_rects())
        {
            let event = if self.dock.is_collapsed() {
                DockEvent::HandleHover(frame::on_tab(
                    cursor,
                    tab,
                    edge,
                    zone(dock::REVEAL_ZONE, scale),
                ))
            } else {
                let on = frame::near(cursor, open, zone(dock::KEEP_ZONE, scale));
                if on {
                    self.dock_fresh = false;
                }
                DockEvent::PanelHover(held || on || self.dock_fresh)
            };
            effects.extend(self.dock_event(event));
        } else if (held || self.dock_fresh) && !self.dock.is_collapsed() {
            effects.extend(self.dock_event(DockEvent::PanelHover(true)));
        }
        effects.extend(self.dock_event(DockEvent::Tick));
        effects
    }

    /// Tucks the open docked Panel away (the close button, Esc, the Panel hotkey) and gives
    /// the keyboard back to the Target when the Panel had it. The window stays: the tab is the
    /// way back.
    pub(super) fn dock_tuck(&mut self) -> Vec<Effect> {
        if self.dock.is_collapsed() {
            return Vec::new();
        }
        self.drag = None;
        self.dock_fresh = false;
        self.dock = DockState::Parked;
        self.panel_end_modes();
        self.panel_revealed = None;
        if std::mem::take(&mut self.panel_focused) {
            self.target.refocus();
        }
        self.dock_act(DockAction::Collapse)
    }

    /// A use from the Panel ended with `result`: a paste collapses a docked Panel (a copy
    /// does not: the user is about to paste it by hand).
    pub fn dock_after_use(&mut self, result: &UseResult) -> Vec<Effect> {
        if matches!(
            result,
            UseResult::Done(UseOutcome::Pasted | UseOutcome::PastedNotRestored)
        ) {
            self.dock_event(DockEvent::Pasted)
        } else {
            Vec::new()
        }
    }

    /// A use from the Panel ended with `result`: the Panel gets out of the way. A docked one
    /// collapses after a paste ([`dock_after_use`](Self::dock_after_use)); a floating one that
    /// is not pinned hides after a paste or a copy when "After a paste: hide the Panel" is on.
    /// A pinned Panel stays, and so does one whose use did not happen.
    pub fn after_use(&mut self, result: &UseResult) -> Vec<Effect> {
        if self.dock_active() {
            return self.dock_after_use(result);
        }
        if matches!(result, UseResult::Done(_))
            && self.visible
            && self.settings.panel_hide_after_use
            && !self.on_top
        {
            self.hide()
        } else {
            Vec::new()
        }
    }

    fn dock_act(&mut self, action: DockAction) -> Vec<Effect> {
        match action {
            DockAction::None => Vec::new(),
            DockAction::Expand => self.dock_place(false),
            DockAction::Collapse => {
                // A menu cannot stay open on a strip six pixels wide.
                self.menu = PanelMenu::None;
                self.dock_fresh = false;
                if self.target.cursor().is_none() {
                    // Nothing will say that the pointer is off the tab: the window reports only
                    // its own enter and leave, so its next enter has to count.
                    self.dock = DockState::Collapsed;
                }
                self.dock_place(true)
            }
        }
    }

    /// The window frame for the open Panel, or for its handle when `collapsed`.
    fn dock_place(&self, collapsed: bool) -> Vec<Effect> {
        let Some(monitor) = &self.monitor else {
            return Vec::new();
        };
        let Some(geometry) = self.stored_geometry(monitor) else {
            return Vec::new();
        };
        let Some(edge) = geometry.edge else {
            return Vec::new();
        };
        let open = self.dock_open(geometry, edge, monitor);
        let rect = if collapsed {
            frame::handle_rect(edge, monitor.work_area, open.along(edge), monitor.scale)
        } else {
            open
        };
        vec![Effect::PlaceWindow(rect)]
    }

    /// Where a floating Panel is about to open: "Panel opens" at the mouse or at the text cursor
    /// (the caret, else the mouse). `None` for "where you left it", a docked Panel, or when
    /// neither can be read.
    pub(super) fn open_anchor_now(&self) -> Option<crate::placement::Anchor> {
        use crate::{placement::Anchor, settings::PanelOpenAt};
        if self.settings.panel_open_at == PanelOpenAt::Last || self.dock_edge().is_some() {
            return None;
        }
        let caret = (self.settings.panel_open_at == PanelOpenAt::Caret)
            .then(|| self.target.caret())
            .flatten();
        if let Some(c) = caret {
            return Some(Anchor::Caret(Rect {
                x: c.x,
                y: c.y,
                w: c.w,
                h: c.h,
            }));
        }
        let (x, y) = self.target.cursor()?;
        Some(Anchor::Point(x, y))
    }

    /// The frame the Panel takes at `anchor`, on the monitor under it (else `fallback`): its own
    /// size, [`placement::place`](crate::placement::place) for the position.
    pub(super) fn frame_at(
        &self,
        anchor: crate::placement::Anchor,
        fallback: &Monitor,
    ) -> (Monitor, Rect) {
        let (x, y) = anchor.point();
        let monitor = self
            .target
            .monitor_at(x, y)
            .unwrap_or_else(|| fallback.clone());
        // The size it has on the monitor it was last on: the one under the anchor may be a
        // monitor it never was on.
        let sized = self.open_frame(fallback);
        let valid = |s: f32| if s.is_finite() && s > 0.0 { s } else { 1.0 };
        let scale = valid(monitor.scale);
        // That size is in the old monitor's physical pixels: the same logical size on the new
        // monitor is a different number of them.
        let ratio = f64::from(scale) / f64::from(valid(fallback.scale));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a window length in pixels, rounded and never negative"
        )]
        let convert = |px: u32| (f64::from(px) * ratio).round() as u32;
        let size = Rect {
            w: convert(sized.w),
            h: convert(sized.h),
            ..sized
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few pixels, rounded up and never negative"
        )]
        let gap = (crate::placement::GAP * scale).ceil() as u32;
        let frame = crate::placement::place(
            (size.w, size.h),
            anchor,
            monitor.work_area,
            gap,
            self.settings.picker_side,
        );
        (monitor, frame)
    }

    /// Settings or the edge changed: with the dock off a collapsed Panel opens at once.
    pub(super) fn dock_resync(&mut self) {
        let config = self.dock_config();
        if self.dock.apply_config(&config) == DockAction::Expand {
            let placed = self.dock_place(false);
            self.pending.extend(placed);
        }
    }

    /// The floating Panel's frame on `monitor`: what was stored, else a default.
    pub(super) fn open_frame(&self, monitor: &Monitor) -> Rect {
        self.stored_geometry(monitor).map_or_else(
            || {
                // Off every edge, so that it reads as floating.
                let inset = i32::try_from(frame::scaled(OPEN_INSET, monitor.scale)).unwrap_or(0);
                Rect {
                    x: monitor.work_area.x.saturating_add(inset),
                    y: monitor.work_area.y.saturating_add(inset),
                    w: frame::scaled(self.settings.panel_width, monitor.scale),
                    h: frame::scaled(DEFAULT_HEIGHT, monitor.scale),
                }
            },
            Rect::of,
        )
    }

    /// What is remembered for `monitor`, or a floating Panel at the default place there.
    fn geometry_on(&self, monitor: &Monitor) -> PanelGeometry {
        self.stored_geometry(monitor).cloned().unwrap_or_else(|| {
            frame::clamp(self.open_frame(monitor), monitor).geometry(monitor, None)
        })
    }

    /// The Panel's floating and docked sizes on `monitor`, for a drag. A monitor it never was
    /// on gets the sizes of the one it is on, at that monitor's scale (the same logical size).
    fn sizes_on(&self, monitor: &Monitor) -> DragSizes {
        let (geometry, ratio) = if let Some(g) = self.stored_geometry(monitor) {
            (g.clone(), 1.0)
        } else if let Some(from) = self.monitor.as_ref().filter(|m| m.id != monitor.id) {
            let valid = |s: f32| if s.is_finite() && s > 0.0 { s } else { 1.0 };
            (
                self.geometry_on(from),
                f64::from(valid(monitor.scale)) / f64::from(valid(from.scale)),
            )
        } else {
            (self.geometry_on(monitor), 1.0)
        };
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a window length in pixels, rounded and never negative"
        )]
        let convert = |(w, h): (u32, u32)| {
            (
                (f64::from(w) * ratio).round() as u32,
                (f64::from(h) * ratio).round() as u32,
            )
        };
        let here = PanelGeometry {
            w: convert((geometry.w, geometry.h)).0,
            h: convert((geometry.w, geometry.h)).1,
            docked: geometry.docked.map(convert),
            docked_flat: geometry.docked_flat.map(convert),
            ..geometry
        };
        // The drag chooses where the Panel sits along the edge, so a side dock's height is the
        // user's own; a top or bottom dock is still never as tall as the screen.
        DragSizes {
            floating: (here.w, here.h),
            side: self.dock_size(&here, DockEdge::Left, monitor, Some(0)),
            flat: self.dock_size(&here, DockEdge::Top, monitor, Some(0)),
        }
    }

    /// Docks the Panel to `edge` on its monitor, or lets it float again (`None`): the menu and
    /// Settings rows. The edge is remembered for that monitor; the Panel keeps its docked size
    /// there (its floating size the first time) and goes back to its floating frame when it
    /// leaves. The placement is queued for [`take_pending`](Self::take_pending). Does nothing
    /// before the Panel's monitor is known.
    pub fn dock_to(&mut self, edge: Option<DockEdge>) {
        let Some(monitor) = self.monitor.clone() else {
            return;
        };
        if edge.is_some() && edge == monitor.taskbar {
            // The taskbar owns that edge: a Panel put there would sit under it (O32).
            self.show_toast(neutral(ToastMessage::TaskbarEdge));
            return;
        }
        self.drag = None;
        let stored = self.stored_geometry(&monitor).cloned();
        let mut geometry = self.geometry_on(&monitor);
        let floating = frame::clamp(Rect::of(&geometry), &monitor);
        let place = if let Some(edge) = edge {
            // Where it was on that edge before, else level with where it floats.
            let along = geometry
                .along_for(edge)
                .unwrap_or_else(|| floating.along(edge).start);
            let size = self.dock_size(
                &PanelGeometry {
                    w: floating.w,
                    h: floating.h,
                    ..geometry.clone()
                },
                edge,
                &monitor,
                Some(along),
            );
            let dock = |full_edge| {
                frame::docked_rect(
                    edge,
                    monitor.work_area,
                    size,
                    monitor.scale,
                    Some(along),
                    full_edge,
                )
            };
            // Remembered at its own height even when the full edge is on, so that turning that
            // off gives the height back.
            let own = dock(false);
            geometry.set_docked_size(edge, (own.w, own.h));
            geometry.along = Some(own.along(edge).start);
            dock(self.settings.dock.full_edge)
        } else {
            geometry.along = None;
            floating
        };
        geometry.edge = edge;
        // Open before the settings change, so that undocking does not look like a collapsed
        // Panel losing its trigger. Docked from a menu or from Settings the pointer is not on
        // the Panel: it stays out until the pointer has been.
        self.dock = DockState::Expanded;
        self.dock_fresh = edge.is_some();
        self.docked_at = self.clock.now();
        let chosen = edge.is_some() && edge != stored.as_ref().and_then(|g| g.edge);
        self.update_settings(|s| {
            if chosen {
                s.dock.docked_by_choice();
            }
            s.remember_panel_geometry(geometry.remembering(stored.as_ref()));
        });
        self.pending.push(Effect::PlaceWindow(place));
        self.show_toast(neutral(ToastMessage::DockSet(edge)));
    }

    /// "Docked panel uses the full edge" was switched: the setting changes and an open docked
    /// Panel is placed again at its new height (queued for [`take_pending`](Self::take_pending)).
    pub fn set_dock_full_edge(&mut self, on: bool) {
        self.update_settings(|s| s.dock.full_edge = on);
        if self.dock_edge().is_some() && !self.dock_collapsed() {
            let placed = self.dock_place(false);
            self.pending.extend(placed);
        }
    }

    // ---- The drag ------------------------------------------------------------------------------

    /// A drag of the Panel is under way.
    #[must_use]
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The edge the Panel shows itself docked to: its own, or while it is dragged the one the
    /// drag has it on (none once it is pulled off, the offered one at an edge).
    pub(super) fn shown_edge(&self) -> Option<DockEdge> {
        match self.drag.map(|d| d.phase()) {
            Some(DragPhase::Float) => None,
            Some(DragPhase::Preview(edge) | DragPhase::Slide(edge)) => Some(edge),
            None => self.dock_edge(),
        }
    }

    /// What a drag step needs for the cursor at `cursor`: the monitor (the Panel's own while
    /// it slides along its edge, else the one under the cursor) and the sizes there.
    fn drag_ctx(&self, drag: &Drag, cursor: (i32, i32)) -> Option<DragPlace> {
        let own = self.monitor.clone();
        let monitor = if matches!(drag.phase(), DragPhase::Slide(_)) {
            own
        } else {
            self.target.monitor_at(cursor.0, cursor.1).or(own)
        }?;
        let sizes = self.sizes_on(&monitor);
        Some(DragPlace { monitor, sizes })
    }

    /// Pointer down on the Panel's header: a drag starts. `window` is the Panel's frame now and
    /// `pointer` where the host saw the press on screen, used when the cursor cannot be read.
    /// From here the host calls [`drag_move`](Self::drag_move) every [`dock::DRAG_TICK`] and on
    /// each pointer move, and [`drag_end`](Self::drag_end) when the button comes up.
    pub fn drag_begin(&mut self, pointer: (i32, i32), window: Rect) {
        if self.dock_collapsed() || self.monitor.is_none() {
            return;
        }
        let cursor = self.target.cursor().unwrap_or(pointer);
        self.drag = Some(Drag::begin(cursor, window, self.dock_edge()));
        self.drag_button_seen = false;
        self.dock_fresh = false;
    }

    /// The host can tell that the button a drag was started with is up again, although the
    /// window never said so: the drag is over. "Up" is believed only once the button was seen
    /// down in this drag: under a window of higher privilege Windows reports every button as
    /// up, and a drag must not end on its first step there.
    #[must_use]
    pub fn drag_let_go(&mut self) -> bool {
        if self.drag.is_none() {
            return false;
        }
        match self.target.primary_down() {
            Some(true) => {
                self.drag_button_seen = true;
                false
            }
            Some(false) => self.drag_button_seen,
            None => false,
        }
    }

    /// The drag goes on: the cursor is read (else `pointer` is used) and the window follows
    /// it. It docks nothing by itself: at an edge the Panel is shown docked there, and
    /// [`drag_end`](Self::drag_end) decides.
    pub fn drag_move(&mut self, pointer: Option<(i32, i32)>) -> Vec<Effect> {
        let Some(mut drag) = self.drag else {
            return Vec::new();
        };
        let Some(cursor) = self.target.cursor().or(pointer) else {
            return Vec::new();
        };
        let Some(DragPlace { monitor, sizes }) = self.drag_ctx(&drag, cursor) else {
            return Vec::new();
        };
        let step = drag.moved(
            cursor,
            &DragCtx {
                monitor: &monitor,
                sizes,
                full_edge: self.settings.dock.full_edge,
                now: self.clock.now(),
            },
        );
        self.drag = Some(drag);
        match step {
            DragStep::None => Vec::new(),
            DragStep::Move(x, y) => vec![Effect::MoveWindow(x, y)],
            DragStep::Place(rect) => vec![Effect::PlaceWindow(rect)],
        }
    }

    /// The button came up: the Panel docks to the edge it was shown on, or floats where it
    /// was dropped. Its floating frame and its docked size are remembered apart, so neither
    /// gesture changes the other's size.
    pub fn drag_end(&mut self) -> Vec<Effect> {
        let Some(drag) = self.drag.take() else {
            return Vec::new();
        };
        let Some(DragPlace { monitor, sizes }) = self.drag_ctx(&drag, drag.cursor()) else {
            return Vec::new();
        };
        let floating = sizes.floating;
        let end = drag.released(&DragCtx {
            monitor: &monitor,
            sizes,
            full_edge: self.settings.dock.full_edge,
            now: self.clock.now(),
        });
        let left = self.monitor.clone().filter(|m| m.id != monitor.id);
        let was_docked = self.dock_edge();
        let stored = self.stored_geometry(&monitor).cloned();
        let mut geometry = stored.clone().unwrap_or_else(|| {
            // A monitor it never was on: it floats where it was dropped.
            frame::clamp(
                Rect {
                    w: floating.0,
                    h: floating.1,
                    ..drag.frame()
                },
                &monitor,
            )
            .geometry(&monitor, None)
        });
        let place = match end {
            DragEnd::Floating(rect) => {
                (geometry.x, geometry.y, geometry.w, geometry.h) = (rect.x, rect.y, rect.w, rect.h);
                geometry.edge = None;
                geometry.along = None;
                rect
            }
            DragEnd::Docked(edge, rect) => {
                geometry.edge = Some(edge);
                // The full edge stretches the window, not the size the Panel keeps.
                let own = frame::docked_rect(
                    edge,
                    monitor.work_area,
                    sizes.docked_on(edge),
                    monitor.scale,
                    Some(drag.frame().along(edge).start),
                    false,
                );
                geometry.set_docked_size(edge, (own.w, own.h));
                geometry.along = Some(if self.settings.dock.full_edge && edge.vertical() {
                    stored
                        .as_ref()
                        .and_then(|g| g.along_for(edge))
                        .unwrap_or_else(|| own.along(edge).start)
                } else {
                    own.along(edge).start
                });
                rect
            }
        };
        let now_docked = geometry.edge;
        self.dock = DockState::Expanded;
        self.update_settings(|s| {
            if now_docked.is_some() && (now_docked != was_docked || left.is_some()) {
                s.dock.docked_by_choice();
            }
            // Dragged to another monitor, it is no longer docked on the one it left.
            if let Some(old) = left.as_ref()
                && let Some(g) = s.panel_geometry.iter_mut().find(|g| g.monitor_id == old.id)
                && g.edge.is_some()
            {
                let undocked = PanelGeometry {
                    edge: None,
                    along: None,
                    ..g.clone()
                }
                .remembering(Some(g));
                *g = undocked;
            }
            s.remember_panel_geometry(geometry.remembering(stored.as_ref()));
        });
        self.monitor = Some(monitor);
        // Only a dock made or left is news: a floating Panel dropped on another monitor floats
        // as it did.
        if now_docked != was_docked {
            self.show_toast(neutral(ToastMessage::DockSet(now_docked)));
        }
        vec![Effect::PlaceWindow(place)]
    }

    /// The drag ended without a release the app saw (the window went away): the Panel goes back
    /// to where it is remembered.
    pub fn drag_cancel(&mut self) -> Vec<Effect> {
        if self.drag.take().is_none() {
            return Vec::new();
        }
        self.monitor
            .as_ref()
            .and_then(|m| self.placement(m))
            .map(Effect::PlaceWindow)
            .into_iter()
            .collect()
    }

    /// "Reset size and position": forgets what was remembered for the Panel's current monitor (and
    /// only that one, edge included) and places the Panel at the default frame there. Nothing is
    /// done before the Panel's monitor is known. No Undo toast: it is a plain reset.
    pub fn reset_panel_geometry(&mut self) -> Vec<Effect> {
        let Some(monitor) = self.monitor.clone() else {
            return Vec::new();
        };
        self.dock = DockState::Expanded;
        // A gesture in flight ends here: it must not dock or resize the placement.
        self.drag = None;
        self.update_settings(|s| s.panel_geometry.retain(|g| g.monitor_id != monitor.id));
        let place = frame::clamp(self.open_frame(&monitor), &monitor);
        vec![Effect::PlaceWindow(place)]
    }

    /// The edge the kebab's dock row would dock a floating Panel to: the nearest one that is
    /// not the taskbar's. `None` while docked or before the monitor is known.
    #[must_use]
    pub fn dock_target(&self) -> Option<DockEdge> {
        let monitor = self.monitor.as_ref()?;
        if self.dock_edge().is_some() {
            return None;
        }
        Some(frame::nearest_dock_edge(
            self.open_frame(monitor),
            monitor.work_area,
            monitor.taskbar,
        ))
    }

    /// The kebab's dock row: docks a floating Panel to [`Self::dock_target`], or undocks.
    pub fn dock_toggle(&mut self) {
        if self.dock_edge().is_some() {
            self.dock_to(None);
        } else if let Some(edge) = self.dock_target() {
            self.dock_to(Some(edge));
        }
    }

    /// Placements queued by a change that did not come from the Panel window (the Settings
    /// page docking the Panel, or switching the dock off). The Panel's binding applies them
    /// whenever it redraws.
    pub fn take_pending(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.pending)
    }
}
