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
        match started {
            Ok(()) => vec![Effect::HidePanel],
            Err(_) => {
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::EyedropFailed,
                });
                Vec::new()
            }
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
        match outcome {
            None => Vec::new(),
            Some(EyedropOutcome::Picked { rgb, hdr }) => {
                self.colour_picked(snipdeck_colour::Rgba8::rgb(rgb[0], rgb[1], rgb[2]), hdr)
            }
            Some(EyedropOutcome::Cancelled) => vec![Effect::ShowPanel, Effect::ShowColours],
            Some(EyedropOutcome::Failed(_)) => {
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: ToastMessage::EyedropFailed,
                });
                vec![Effect::ShowPanel, Effect::ShowColours]
            }
        }
    }
}
