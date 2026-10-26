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

use crate::settings::{DockSettings, DockTrigger};

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
    /// Only the handle shows ([`Collapsed`](Self::Collapsed) or [`Revealing`](Self::Revealing)).
    #[must_use]
    pub const fn is_collapsed(self) -> bool {
        matches!(self, Self::Collapsed | Self::Revealing { .. })
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
            (Self::Collapsed | Self::Revealing { .. }, DockEvent::Hotkey) => self.expand(),

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
            (Self::Revealing { .. }, DockEvent::HandleHover(false)) => {
                *self = Self::Collapsed;
                DockAction::None
            }
            (Self::Revealing { since }, DockEvent::Tick)
                if elapsed_ms(now, since) >= hover_wait =>
            {
                self.expand()
            }
            (Self::Collapsed | Self::Revealing { .. }, DockEvent::HandleClick)
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
        *self = Self::Collapsed;
        DockAction::Collapse
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
        assert_eq!(state, DockState::Collapsed);
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
        assert_eq!(state, DockState::Collapsed);
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
        assert_eq!(state, DockState::Collapsed);
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
        assert_eq!(leaving, DockState::Collapsed);
    }

    #[test]
    fn a_paste_collapses_an_open_panel() {
        let hover = cfg(DockTrigger::Hover);
        for start in [DockState::Expanded, DockState::Leaving { since: at(0) }] {
            let mut state = start;
            assert_eq!(state.on(Pasted, at(10), &hover), DockAction::Collapse);
            assert_eq!(state, DockState::Collapsed);
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
                                    !before.is_collapsed() && state == DockState::Collapsed,
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
}
