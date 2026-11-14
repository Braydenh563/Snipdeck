//! "Take screenshot" through the [`Controller`]: carries out the steps of
//! [`ScreenClipFlow`](crate::screen_clip::ScreenClipFlow) (hide the Panel, start the snipping
//! overlay, show the Panel again, say it failed) with the controller's clock, settings and
//! machine. The timing itself is in `screen_clip.rs`.

use snipdeck_platform::pipeline::{Clipboard, Keyboard, Timing};

use super::{Controller, Effect, Library};
use crate::{
    host::ClipboardText,
    screen_clip::{ClipboardProbe, ScreenClipHost, Step},
    toast::{Toast, ToastKind, ToastMessage},
};

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Gives the controller the machine "Take screenshot" runs on (the snipping overlay and the
    /// clipboard's change count). Without one the action only says it failed.
    pub fn set_screen_clip_host(&mut self, host: Box<dyn ScreenClipHost>) {
        self.screen_clip_host = Some(host);
    }

    /// "Take screenshot" (the Panel menu, the tray, the command palette): the Panel steps aside
    /// unless "Keep Panel visible during screenshots" is on, and the snipping overlay starts.
    /// The Panel returns by [`screen_clip_tick`](Self::screen_clip_tick).
    pub fn take_screenshot(&mut self) -> Vec<Effect> {
        let before = self.screen_clip_probe();
        let steps = self.screen_clip.start(
            self.clock.now(),
            self.settings.screenshot_keep_panel,
            self.visible,
            before,
        );
        let mut effects = Vec::new();
        for step in steps {
            if step == Step::Launch {
                let started = self.screen_clip_host.as_mut().map_or_else(
                    || Err("no screenshot host".to_owned()),
                    |host| host.launch(),
                );
                if let Err(problem) = started {
                    crate::trace!("cannot start the snipping overlay: {problem}");
                    let after = self.screen_clip.launch_failed();
                    effects.extend(self.screen_clip_steps(&after));
                }
            } else {
                effects.extend(self.screen_clip_steps(&[step]));
            }
        }
        effects
    }

    /// Whether a screenshot is under way: the host calls
    /// [`screen_clip_tick`](Self::screen_clip_tick) every [`POLL`](crate::screen_clip::POLL).
    #[must_use]
    pub const fn screen_clip_waiting(&self) -> bool {
        self.screen_clip.waiting()
    }

    /// Looks at the clipboard: a picture on it, or a minute gone, brings the Panel back.
    pub fn screen_clip_tick(&mut self) -> Vec<Effect> {
        if !self.screen_clip.waiting() {
            return Vec::new();
        }
        let probe = self.screen_clip_probe();
        let steps = self.screen_clip.tick(self.clock.now(), probe);
        self.screen_clip_steps(&steps)
    }

    fn screen_clip_probe(&mut self) -> ClipboardProbe {
        self.screen_clip_host.as_mut().map_or(
            ClipboardProbe {
                sequence: 0,
                has_image: false,
            },
            |host| host.probe(),
        )
    }

    fn screen_clip_steps(&mut self, steps: &[Step]) -> Vec<Effect> {
        let mut effects = Vec::new();
        for step in steps {
            match step {
                Step::HidePanel => effects.extend(self.hide()),
                Step::ShowPanel => effects.extend(self.show()),
                Step::ToastLaunchFailed => self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::ScreenshotFailed,
                }),
                // `take_screenshot` starts the overlay itself: it must see the result.
                Step::Launch => {}
            }
        }
        effects
    }
}
