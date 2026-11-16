//! The screen eyedropper through the [`Controller`]: the Panel steps aside, the platform's overlay
//! runs on its own thread, and the outcome is taken up on the next redraw (`poll_eyedropper`).
//! Docs: `docs/reviews/colour-spec.md` §4. Windows-only in practice; elsewhere there is none.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use snipdeck_platform::{
    eyedrop::{
        EndReason, EyedropNote, EyedropOutcome, EyedropTexts, Eyedropper, FailReason, KeyName,
    },
    pipeline::{Clipboard, Keyboard, Timing},
};

use super::{Controller, Effect, Library};
use crate::{
    host::ClipboardText,
    toast::{Toast, ToastKind, ToastMessage},
};

/// The longest the platform waits for every Snipdeck window to be gone before it captures the
/// screen (it stops as soon as they are).
const SETTLE: Duration = Duration::from_millis(800);

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
        // The loupe reads in the default copy format and grows with the text size, both as of now.
        let format = self.settings.colour_format;
        let texts = EyedropTexts {
            readout: Arc::new(move |rgb: [u8; 3]| {
                let c = snipdeck_colour::Rgba8::rgb(rgb[0], rgb[1], rgb[2]);
                snipdeck_colour::copy_text(c, format).unwrap_or_else(|| c.to_hex())
            }),
            text_scale: crate::text_scale::current(),
            ..self.eyedrop_texts.clone()
        };
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
            // The Panel hides first and the platform waits (up to SETTLE) until every Snipdeck
            // window is gone from the screen, so none of them is in the screenshot. Hiding even when the Panel is not on
            // screen is a no-op, and a cancel shows it again: the cost of that is a brief flash
            // when the Panel was already hidden (a palette command from the Main window).
            vec![Effect::HidePanel, Effect::HideMainForEyedropper]
        } else {
            self.show_toast(Toast {
                kind: ToastKind::Danger,
                message: ToastMessage::EyedropFailed,
            });
            Vec::new()
        }
    }

    /// The contrast section's pick button: like [`start_eyedropper`](Self::start_eyedropper), but
    /// the colour that comes back is the other colour of the contrast check.
    pub fn start_eyedropper_for_other(&mut self) -> Vec<Effect> {
        let effects = self.start_eyedropper();
        self.eyedrop_for_other = self.eyedropping;
        effects
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
        let notes = self
            .eyedropper
            .as_ref()
            .map(|e| e.notes())
            .unwrap_or_default();
        let effects = match outcome {
            EyedropOutcome::Picked { rgb, hdr, also } => {
                let pick = |c: [u8; 3]| snipdeck_colour::Rgba8::rgb(c[0], c[1], c[2]);
                let also: Vec<_> = also.into_iter().map(pick).collect();
                self.colour_picked(pick(rgb), hdr, &also)
            }
            EyedropOutcome::Cancelled => {
                self.eyedrop_for_other = false;
                self.back_from_eyedropper()
            }
            EyedropOutcome::Ended(reason) => {
                self.eyedrop_for_other = false;
                crate::trace!("the eyedropper ended: {reason:?}");
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::EyedropEnded {
                        timed_out: reason == EndReason::TimedOut,
                    },
                });
                self.back_from_eyedropper()
            }
            EyedropOutcome::Failed(reason) => {
                self.eyedrop_for_other = false;
                crate::trace!("the eyedropper failed: {reason:?}");
                self.show_toast(Toast {
                    kind: ToastKind::Danger,
                    message: if reason == FailReason::CaptureRefused {
                        ToastMessage::EyedropRefused
                    } else {
                        ToastMessage::EyedropFailed
                    },
                });
                self.back_from_eyedropper()
            }
        };
        for note in notes {
            let EyedropNote::KeysNotRegistered(keys) = note;
            let keys: Vec<&str> = keys.iter().map(|k| key_label(*k)).collect();
            if !keys.is_empty() {
                self.show_toast(Toast {
                    kind: ToastKind::Neutral,
                    message: ToastMessage::EyedropKeysBusy {
                        keys: keys.join(", "),
                    },
                });
            }
        }
        effects
    }

    /// The Panel comes back on the Colours tab, open, so the controller's state (visible, a dock
    /// open, not tucked) and the window (shown, at its open frame) agree. Unlike a
    /// [`show`](Controller::show) it reads neither the Target (the window under the pick point
    /// is not where the user was) nor the pointer (a Panel hidden before the pick comes back
    /// where it was left, not at the pick point).
    pub(super) fn back_from_eyedropper(&mut self) -> Vec<Effect> {
        let mut effects = self.reveal_panel();
        effects.push(Effect::ShowColours);
        effects.push(Effect::RestoreMainAfterEyedropper);
        effects
    }
}

/// How a key of the eyedropper is written to the user.
fn key_label(key: KeyName) -> &'static str {
    match key {
        KeyName::Escape => "Esc",
        KeyName::Enter => "Enter",
        KeyName::Space => "Space",
        KeyName::Left => "Left",
        KeyName::Right => "Right",
        KeyName::Up => "Up",
        KeyName::Down => "Down",
        KeyName::Plus => "+",
        KeyName::Minus => "-",
        KeyName::Aperture => "A",
    }
}
