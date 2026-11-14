//! "Take screenshot": the Panel steps aside, the Windows snipping overlay (`ms-screenclip:`) comes
//! up, and the Panel returns when a picture reaches the clipboard, when a minute has passed, or
//! at once if the overlay could not be started.
//!
//! [`ScreenClipFlow`] is the whole timing as a pure state machine: it takes the moment and what
//! the clipboard looks like, and answers with [`Step`]s for the controller to carry out. It
//! reads no clock and no clipboard, so every rule below is tested on any platform. Nothing is
//! added to the library: the picture stays on the clipboard for the user to paste.

use std::time::Duration;

use snipdeck_core::Timestamp;

/// How long the flow waits for a picture before the Panel comes back on its own (the user
/// pressed Esc on the overlay, or the overlay never appeared).
pub const TIMEOUT: Duration = Duration::from_mins(1);

/// How often the clipboard is looked at while the flow waits: often enough that the Panel is
/// back within a blink of the snip being taken, rare enough to cost nothing.
pub const POLL: Duration = Duration::from_millis(250);

/// What the flow needs to know of the clipboard. Reading it opens nothing and copies no content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardProbe {
    /// The clipboard's change count: different from before means something new was copied.
    pub sequence: u32,
    /// The clipboard holds a picture.
    pub has_image: bool,
}

/// What the controller does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Hide the Panel, so it is not in front of the overlay.
    HidePanel,
    /// Start the snipping overlay. Report a failure with [`ScreenClipFlow::launch_failed`].
    Launch,
    /// Show the Panel again.
    ShowPanel,
    /// Tell the user the overlay could not be started.
    ToastLaunchFailed,
}

/// Where the flow is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Nothing is going on.
    Idle,
    /// The overlay was asked for; waiting for the picture.
    Waiting {
        /// When the overlay was asked for.
        since: Timestamp,
        /// The clipboard before: a picture counts only if it arrived after this.
        before: ClipboardProbe,
        /// The flow hid the Panel and owes it a return.
        hid_panel: bool,
    },
}

/// The screenshot flow. Starts idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenClipFlow {
    phase: Phase,
}

impl Default for ScreenClipFlow {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenClipFlow {
    /// A flow with nothing going on.
    #[must_use]
    pub const fn new() -> Self {
        Self { phase: Phase::Idle }
    }

    /// Whether a screenshot is under way: the clipboard should be looked at every [`POLL`].
    #[must_use]
    pub const fn waiting(&self) -> bool {
        matches!(self.phase, Phase::Waiting { .. })
    }

    /// The user asked for a screenshot at `now`. `keep_panel` is the setting "Keep Panel visible
    /// during screenshots"; `panel_shown` whether the Panel is up now; `before` the clipboard now.
    /// A second ask while one is under way does nothing: the overlay is up already.
    pub fn start(
        &mut self,
        now: Timestamp,
        keep_panel: bool,
        panel_shown: bool,
        before: ClipboardProbe,
    ) -> Vec<Step> {
        if self.waiting() {
            return Vec::new();
        }
        let hid_panel = panel_shown && !keep_panel;
        self.phase = Phase::Waiting {
            since: now,
            before,
            hid_panel,
        };
        let mut steps = Vec::new();
        if hid_panel {
            steps.push(Step::HidePanel);
        }
        steps.push(Step::Launch);
        steps
    }

    /// The overlay could not be started: the Panel returns at once, and the user is told.
    pub fn launch_failed(&mut self) -> Vec<Step> {
        let Phase::Waiting { hid_panel, .. } = self.phase else {
            return Vec::new();
        };
        self.phase = Phase::Idle;
        let mut steps = Vec::new();
        if hid_panel {
            steps.push(Step::ShowPanel);
        }
        steps.push(Step::ToastLaunchFailed);
        steps
    }

    /// Looks at the clipboard at `now`. A picture that arrived since the ask, or [`TIMEOUT`]
    /// passing, ends the wait; the Panel comes back if the flow hid it. Anything else copied in
    /// the meantime (text, say) is not the screenshot and is ignored.
    pub fn tick(&mut self, now: Timestamp, probe: ClipboardProbe) -> Vec<Step> {
        let Phase::Waiting {
            since,
            before,
            hid_panel,
        } = self.phase
        else {
            return Vec::new();
        };
        let arrived = probe.has_image && probe.sequence != before.sequence;
        let timed_out =
            Duration::try_from(now.duration_since(since)).map_or(true, |waited| waited >= TIMEOUT);
        if !arrived && !timed_out {
            return Vec::new();
        }
        self.phase = Phase::Idle;
        if hid_panel {
            vec![Step::ShowPanel]
        } else {
            Vec::new()
        }
    }
}

/// The machine the flow runs on: the overlay and the clipboard. The real one is Windows only.
pub trait ScreenClipHost {
    /// Starts the snipping overlay.
    ///
    /// # Errors
    ///
    /// A message when it could not be started (not Windows, or no Snipping Tool).
    fn launch(&mut self) -> Result<(), String>;

    /// The clipboard now.
    fn probe(&mut self) -> ClipboardProbe;
}

/// The overlay through the shell (`ms-screenclip:`) and the clipboard through the Win32 change
/// count. Off Windows it cannot start anything.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemScreenClip;

impl ScreenClipHost for SystemScreenClip {
    #[cfg(windows)]
    fn launch(&mut self) -> Result<(), String> {
        crate::help_binding::launch(std::ffi::OsStr::new("ms-screenclip:"))
    }

    #[cfg(not(windows))]
    fn launch(&mut self) -> Result<(), String> {
        Err("the snipping overlay is only on Windows".to_owned())
    }

    #[cfg(windows)]
    fn probe(&mut self) -> ClipboardProbe {
        let (sequence, has_image) = snipdeck_platform::windows::screen::clipboard_picture_state();
        ClipboardProbe {
            sequence,
            has_image,
        }
    }

    #[cfg(not(windows))]
    fn probe(&mut self) -> ClipboardProbe {
        ClipboardProbe {
            sequence: 0,
            has_image: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> Timestamp {
        let start: Timestamp = "2026-10-17T10:30:00Z".parse().expect("a moment");
        Timestamp::from_second(start.as_second() + seconds).expect("a moment")
    }

    fn clipboard(sequence: u32, has_image: bool) -> ClipboardProbe {
        ClipboardProbe {
            sequence,
            has_image,
        }
    }

    /// A flow started at second 0 with the Panel up and the clipboard at sequence 7.
    fn started(keep_panel: bool) -> (ScreenClipFlow, Vec<Step>) {
        let mut flow = ScreenClipFlow::new();
        let steps = flow.start(at(0), keep_panel, true, clipboard(7, false));
        (flow, steps)
    }

    #[test]
    fn the_panel_steps_aside_before_the_overlay_starts() {
        let (flow, steps) = started(false);
        assert_eq!(steps, [Step::HidePanel, Step::Launch]);
        assert!(flow.waiting());
    }

    #[test]
    fn with_the_setting_on_the_panel_stays_and_the_overlay_starts() {
        let (mut flow, steps) = started(true);
        assert_eq!(steps, [Step::Launch]);
        // Nothing was hidden, so nothing comes back.
        assert_eq!(flow.tick(at(2), clipboard(8, true)), []);
        assert!(!flow.waiting());
    }

    #[test]
    fn a_hidden_panel_has_nothing_to_hide_and_nothing_to_show() {
        let mut flow = ScreenClipFlow::new();
        assert_eq!(
            flow.start(at(0), false, false, clipboard(7, false)),
            [Step::Launch]
        );
        assert_eq!(flow.tick(at(1), clipboard(8, true)), []);
    }

    #[test]
    fn a_picture_on_the_clipboard_brings_the_panel_back() {
        let (mut flow, _) = started(false);
        assert_eq!(flow.tick(at(3), clipboard(8, true)), [Step::ShowPanel]);
        assert!(!flow.waiting());
    }

    #[test]
    fn nothing_happens_while_the_clipboard_is_unchanged() {
        let (mut flow, _) = started(false);
        assert_eq!(flow.tick(at(1), clipboard(7, false)), []);
        // A picture that was already there before the ask is not the screenshot.
        assert_eq!(flow.tick(at(2), clipboard(7, true)), []);
        assert!(flow.waiting());
    }

    #[test]
    fn text_copied_meanwhile_is_not_the_screenshot() {
        let (mut flow, _) = started(false);
        assert_eq!(flow.tick(at(5), clipboard(8, false)), []);
        assert!(flow.waiting());
        assert_eq!(flow.tick(at(9), clipboard(9, true)), [Step::ShowPanel]);
    }

    #[test]
    fn the_panel_returns_by_itself_after_a_minute() {
        let (mut flow, _) = started(false);
        assert_eq!(flow.tick(at(59), clipboard(7, false)), []);
        assert!(flow.waiting());
        assert_eq!(flow.tick(at(60), clipboard(7, false)), [Step::ShowPanel]);
        assert!(!flow.waiting());
    }

    #[test]
    fn a_clock_that_went_backwards_ends_the_wait_instead_of_hanging() {
        let (mut flow, _) = started(false);
        assert_eq!(flow.tick(at(-30), clipboard(7, false)), [Step::ShowPanel]);
    }

    #[test]
    fn a_failed_launch_brings_the_panel_back_at_once_with_a_toast() {
        let (mut flow, _) = started(false);
        assert_eq!(
            flow.launch_failed(),
            [Step::ShowPanel, Step::ToastLaunchFailed]
        );
        assert!(!flow.waiting());
    }

    #[test]
    fn a_failed_launch_with_the_panel_kept_only_says_so() {
        let (mut flow, _) = started(true);
        assert_eq!(flow.launch_failed(), [Step::ToastLaunchFailed]);
    }

    #[test]
    fn asking_twice_starts_one_overlay() {
        let (mut flow, _) = started(false);
        assert_eq!(flow.start(at(2), false, false, clipboard(7, false)), []);
        // The wait still runs from the first ask.
        assert_eq!(flow.tick(at(60), clipboard(7, false)), [Step::ShowPanel]);
    }

    #[test]
    fn an_idle_flow_ignores_ticks_and_failures() {
        let mut flow = ScreenClipFlow::new();
        assert_eq!(flow.tick(at(100), clipboard(1, true)), []);
        assert_eq!(flow.launch_failed(), []);
        assert!(!flow.waiting());
    }

    #[test]
    fn a_second_screenshot_can_follow_the_first() {
        let (mut flow, _) = started(false);
        flow.tick(at(3), clipboard(8, true));
        assert_eq!(
            flow.start(at(10), false, true, clipboard(8, true)),
            [Step::HidePanel, Step::Launch]
        );
        // The old picture does not end the new wait.
        assert_eq!(flow.tick(at(11), clipboard(8, true)), []);
        assert_eq!(flow.tick(at(12), clipboard(9, true)), [Step::ShowPanel]);
    }
}
