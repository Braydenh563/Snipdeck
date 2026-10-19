//! The edge dock's half of the [`Controller`] (SPEC 5.1 "Edge dock with reveal"): it feeds the
//! pure state machine of [`crate::dock`] with what the window and the hotkey report, turns its
//! answer into window placements (the handle strip or the open Panel, from [`crate::frame`]),
//! and keeps the edge each monitor was docked to. Slint-free: `binding` wires the handle's and
//! the Panel's pointer callbacks to [`Controller::dock_event`] and runs the timer that sends
//! the ticks.

use snipdeck_core::UseOutcome;
use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, Library, PanelMenu, UseResult};
use crate::{
    dock::{DockAction, DockEvent, DockState},
    frame::{self, Monitor, Rect},
    host::ClipboardText,
    settings::{DockEdge, DockSettings, DockTrigger, PanelGeometry},
};

/// The height a Panel starts with when nothing was stored for its monitor (logical pixels).
const DEFAULT_HEIGHT: u32 = 480;

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
        if matches!(result, UseResult::Done(UseOutcome::Pasted)) {
            self.dock_event(DockEvent::Pasted)
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
        let open = frame::docked_rect(
            edge,
            monitor.work_area,
            (geometry.w, geometry.h),
            monitor.scale,
        );
        let rect = if collapsed {
            frame::handle_rect(edge, monitor.work_area, open.along(edge), monitor.scale)
        } else {
            open
        };
        vec![Effect::PlaceWindow(rect)]
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
    fn open_frame(&self, monitor: &Monitor) -> Rect {
        self.stored_geometry(monitor).map_or_else(
            || {
                let scale = f64::from(monitor.scale.max(0.1));
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a small positive pixel count, rounded"
                )]
                let px = |logical: u32| (f64::from(logical) * scale).round() as u32;
                Rect {
                    x: monitor.work_area.x,
                    y: monitor.work_area.y,
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
        let base = self.open_frame(&monitor);
        let place = match edge {
            Some(edge) => {
                frame::docked_rect(edge, monitor.work_area, (base.w, base.h), monitor.scale)
            }
            None => frame::clamp(base, &monitor),
        };
        // Open before the settings change, so that undocking does not look like a collapsed
        // Panel losing its trigger.
        self.dock = DockState::Expanded;
        self.update_settings(|s| s.remember_panel_geometry(place.geometry(&monitor, edge)));
        self.pending.push(Effect::PlaceWindow(place));
    }

    /// The kebab's dock row: docks a floating Panel to the edge nearest to it, or undocks.
    pub fn dock_toggle(&mut self) {
        let Some(monitor) = self.monitor.clone() else {
            return;
        };
        if self.dock_edge().is_some() {
            self.dock_to(None);
        } else {
            let edge = frame::nearest_edge(self.open_frame(&monitor), monitor.work_area);
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
