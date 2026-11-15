//! The screen eyedropper through the [`Controller`]: the Panel steps aside, the platform's overlay
//! runs on its own thread, and the outcome is taken up on the next redraw (`poll_eyedropper`).
//! Docs: `docs/reviews/colour-spec.md` §4. Windows-only in practice; elsewhere there is none.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use snipdeck_platform::{
    eyedrop::{EyedropOutcome, EyedropTexts, Eyedropper},
    pipeline::{Clipboard, Keyboard, Timing},
};

use super::{Controller, Effect, Library};
use crate::{
    host::ClipboardText,
    toast::{Toast, ToastKind, ToastMessage},
};

/// How long the Panel has to finish hiding before the screen is captured.
const SETTLE: Duration = Duration::from_millis(150);

/// What the eyedropper's thread leaves for the controller.
pub(super) type Mailbox = Arc<Mutex<Option<EyedropOutcome>>>;
/// Wakes the window's redraw from another thread.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

impl<S, C, K, T> Controller<S, C, K, T>
where
    S: Library,
    C: Clipboard + ClipboardText,
    K: Keyboard,
    T: Timing,
{
    /// Gives the controller the machine's eyedropper (`None` where there is none) and the call
    /// that makes the window redraw when a run ends.
    pub fn set_eyedropper(&mut self, eyedropper: Option<Box<dyn Eyedropper>>, wake: Wake) {
        self.eyedropper = eyedropper;
        self.eyedrop_wake = Some(wake);
    }

    /// The words the overlay shows (the window's language).
    pub fn set_eyedrop_texts(&mut self, texts: EyedropTexts) {
        self.eyedrop_texts = texts;
    }

    /// `(there is an eyedropper, a run is going)`.
    #[must_use]
    pub fn eyedropper_state(&self) -> (bool, bool) {
        (
            self.eyedropper.is_some(),
            self.eyedropper.as_ref().is_some_and(|e| e.running()),
        )
    }

    /// The button: the Panel steps aside and the overlay starts. A second press while a run is
    /// going does nothing.
    pub fn start_eyedropper(&mut self) -> Vec<Effect> {
        let texts = self.eyedrop_texts.clone();
        let Some(dropper) = self.eyedropper.as_mut() else {
            return Vec::new();
        };
        if dropper.running() {
            return Vec::new();
        }
        let mailbox = Arc::clone(&self.eyedrop_mailbox);
        let wake = self.eyedrop_wake.clone();
        let started = dropper.start(
            texts,
            SETTLE,
            Box::new(move |outcome| {
                if let Ok(mut slot) = mailbox.lock() {
                    *slot = Some(outcome);
                }
                if let Some(wake) = wake {
                    wake();
                }
            }),
        );
        if started.is_ok() {
            self.eyedropping = true;
            // The Panel always hides first and the platform waits SETTLE (150 ms) so its pixels are
            // gone from the screenshot the eyedropper takes. Hiding even when the Panel is not on
            // screen is a no-op, and a cancel shows it again: the cost of that is a brief flash
            // when the Panel was already hidden (a palette command from the Main window).
            vec![Effect::HidePanel]
        } else {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::EyedropFailed,
            });
            Vec::new()
        }
    }

    /// Takes up the outcome of a finished run, if there is one: a colour becomes current (see
    /// `colour_picked`), a cancel brings the Panel back, a failure says so.
    pub fn poll_eyedropper(&mut self) -> Vec<Effect> {
        let outcome = self
            .eyedrop_mailbox
            .lock()
            .ok()
            .and_then(|mut slot| slot.take());
        let Some(outcome) = outcome else {
            return Vec::new();
        };
        self.eyedropping = false;
        match outcome {
            EyedropOutcome::Picked { rgb, hdr } => {
                return self
                    .colour_picked(snipdeck_colour::Rgba8::rgb(rgb[0], rgb[1], rgb[2]), hdr);
            }
            EyedropOutcome::Cancelled => {}
            EyedropOutcome::Failed(_) => {
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::EyedropFailed,
                });
            }
        }
        self.back_from_eyedropper()
    }

    /// The Panel comes back on the Colours tab, open, so the controller's state (visible, a dock
    /// open, not tucked) and the window (shown, at its open frame) agree. Unlike a
    /// [`show`](Controller::show) it reads neither the Target (the window under the pick point
    /// is not where the user was) nor the pointer (a Panel hidden before the pick comes back
    /// where it was left, not at the pick point).
    pub(super) fn back_from_eyedropper(&mut self) -> Vec<Effect> {
        let mut effects = self.reveal_panel();
        effects.push(Effect::ShowColours);
        effects
    }
}
