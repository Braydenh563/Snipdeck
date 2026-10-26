//! The edge dock's half of the [`Controller`] (SPEC 5.1 "Edge dock with reveal"): it feeds the
//! pure state machine of [`crate::dock`] with what the window and the hotkey report, turns its
//! answer into window placements (the handle strip or the open Panel, from [`crate::frame`]),
//! and keeps the edge each monitor was docked to. Slint-free: `binding` wires the handle's and
//! the Panel's pointer callbacks to [`Controller::dock_event`] and runs the timer that sends
//! the ticks.

use snipdeck_core::UseOutcome;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, Library, PanelMenu, UseResult, neutral};
use crate::{
    dock::{DockAction, DockEvent, DockState},
    frame::{self, Monitor, Rect},
    host::ClipboardText,
    settings::{DockEdge, DockSettings, DockTrigger, PanelGeometry},
    toast::ToastMessage,
};

/// The height a Panel starts with when nothing was stored for its monitor (logical pixels).
pub(super) const DEFAULT_HEIGHT: u32 = 480;

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

    /// A hover or leave delay is running: the host sends [`DockEvent::Tick`] every
    /// [`crate::dock::TICK`] while this is true.
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
        // The pointer moving onto an open menu (or the window edge under it) is not leaving:
        // a menu in use keeps the Panel open.
        if event == DockEvent::PanelHover(false) && self.menu != PanelMenu::None {
            return Vec::new();
        }
        let config = self.dock_config();
        let now = self.clock.now();
        let action = self.dock.on(event, now, &config);
        self.dock_act(action)
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
        // A frame stored as full height (a side dock's, or one an older version saved for a
        // top or bottom dock) is not the height of a top or bottom dock.
        let size = frame::dock_size(
            edge,
            (geometry.w, geometry.h),
            geometry.pre_dock,
            monitor.work_area,
            monitor.scale,
            DEFAULT_HEIGHT,
        );
        let open = frame::docked_rect(edge, monitor.work_area, size, monitor.scale);
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

    /// Where the Panel is on `monitor` when it is open: what was stored, else a default.
    pub(super) fn open_frame(&self, monitor: &Monitor) -> Rect {
        self.stored_geometry(monitor).map_or_else(
            || {
                let scale = f64::from(monitor.scale.max(0.1));
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a small positive pixel count, rounded"
                )]
                let px = |logical: u32| (f64::from(logical) * scale).round() as u32;
                // Off every edge: flush with the corner it would be in snap range of the
                // dock, and a drag from there would end up docked and stretched.
                let inset = i32::try_from(px(crate::frame::DOCK_SNAP * 3)).unwrap_or(0);
                Rect {
                    x: monitor.work_area.x.saturating_add(inset),
                    y: monitor.work_area.y.saturating_add(inset),
                    w: px(self.settings.panel_width),
                    h: px(DEFAULT_HEIGHT),
                }
            },
            Rect::of,
        )
    }

    /// Docks the Panel to `edge` on its monitor, or lets it float again (`None`). The edge is
    /// remembered for that monitor and the Panel is placed flush with it; the placement is
    /// queued for [`take_pending`](Self::take_pending). Does nothing before the Panel's
    /// monitor is known.
    pub fn dock_to(&mut self, edge: Option<DockEdge>) {
        let Some(monitor) = self.monitor.clone() else {
            return;
        };
        if edge.is_some() && edge == monitor.taskbar {
            // The taskbar owns that edge: a Panel put there would sit under it (O32).
            self.show_toast(neutral(ToastMessage::TaskbarEdge));
            return;
        }
        let mut base = self.open_frame(&monitor);
        let stored = self.stored_geometry(&monitor);
        let earlier = stored.filter(|g| g.edge.is_some()).and_then(|g| g.pre_dock);
        // The size to give back when the Panel leaves its edge: the one it had when it was
        // docked, kept while it stays docked (a docked frame is as tall as the screen).
        let pre_dock = edge.map(|_| earlier.unwrap_or((base.w, base.h)));
        if let (None, Some((w, h))) = (edge, earlier) {
            base = Rect { w, h, ..base };
        }
        let place = match edge {
            Some(edge) => {
                let size = frame::dock_size(
                    edge,
                    (base.w, base.h),
                    earlier,
                    monitor.work_area,
                    monitor.scale,
                    DEFAULT_HEIGHT,
                );
                frame::docked_rect(edge, monitor.work_area, size, monitor.scale)
            }
            None => frame::clamp(base, &monitor),
        };
        // Open before the settings change, so that undocking does not look like a collapsed
        // Panel losing its trigger.
        self.dock = DockState::Expanded;
        self.update_settings(|s| {
            s.remember_panel_geometry(PanelGeometry {
                pre_dock,
                ..place.geometry(&monitor, edge)
            });
        });
        self.pending.push(Effect::PlaceWindow(place));
        self.show_toast(neutral(ToastMessage::DockSet(edge)));
    }

    /// "Reset size and position": forgets what was remembered for the Panel's current monitor (and
    /// only that one, edge included) and places the Panel at the default frame there. Nothing is
    /// done before the Panel's monitor is known. No Undo toast: it is a plain reset.
    pub fn reset_panel_geometry(&mut self) -> Vec<Effect> {
        let Some(monitor) = self.monitor.clone() else {
            return Vec::new();
        };
        self.dock = DockState::Expanded;
        // A gesture in flight ends here: its flags must not dock or resize the placement.
        self.drag_started = false;
        self.dragging = false;
        self.dock_preview = None;
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

    /// The Panel stopped moving after a drag (`frame_moved` went quiet): a drag released near
    /// an allowed edge ([`frame::dock_preview`]) docks there. Nothing happens after a move that
    /// was not a drag (a restore, a placement), so a Panel kept near an edge stays floating.
    pub fn frame_settled(&mut self, monitor: &Monitor, frame: Rect) -> Vec<Effect> {
        if !std::mem::take(&mut self.dragging) {
            return Vec::new();
        }
        self.monitor = Some(monitor.clone());
        let preview = self.dock_preview.take();
        if preview.is_some()
            && self.dock_edge().is_none()
            && preview
                == frame::dock_preview(frame, monitor.work_area, monitor.taskbar, monitor.scale)
        {
            self.dock_to(preview);
            return self.take_pending();
        }
        Vec::new()
    }

    /// Placements queued by a change that did not come from the Panel window (the Settings
    /// page docking the Panel, or switching the dock off). The Panel's binding applies them
    /// whenever it redraws.
    pub fn take_pending(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.pending)
    }
}
