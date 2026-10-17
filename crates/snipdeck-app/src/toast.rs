//! What the toast says after a use. Variants carry the data; the UI layer owns the wording
//! (`@tr`), so translations never live in Rust.

use snipdeck_core::{Feature, UseOutcome};
use snipdeck_platform::route::{CopyReason, Route};

use crate::{panel::friendly_app_name, usecase::UseError};

/// How the toast is styled (DESIGN.md §4 Toast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Information.
    Neutral,
    /// It worked (shown with a check icon).
    Success,
    /// It failed (shown with a warning icon).
    Danger,
}

/// The message, before translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastMessage {
    /// "Pasted into {app}".
    Pasted {
        /// A readable app name.
        app: String,
    },
    /// "Copied".
    Copied,
    /// "Copied. {app} runs as administrator, so Snipdeck can't paste into it."
    CopiedElevated {
        /// A readable app name.
        app: String,
    },
    /// "Copied. Nothing in {app} can take text right now."
    CopiedNothingFocused {
        /// A readable app name.
        app: String,
    },
    /// "Copied. Pasting into {app} is turned off."
    CopiedExcluded {
        /// A readable app name.
        app: String,
    },
    /// "Copied. The window you were using went away."
    CopiedTargetGone,
    /// "Couldn't reach the clipboard. Another app may be using it."
    ClipboardBusy,
    /// "This item uses a Pro feature."
    NeedsPro(Feature),
    /// "This template can't be read. Open it to fix it."
    TemplateBroken,
    /// "Unlock your library to use this item."
    Locked,
    /// "Fill in this template first." (The fill form arrives with M4.)
    NeedsAnswers,
    /// "{chord} is used by another app. Choose another in Settings."
    HotkeyInUse {
        /// The chord as typed in Settings, for example `Ctrl+Alt+Space`.
        chord: String,
    },
    /// "Couldn't use this item."
    Failed,
}

/// A toast: a style and a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Style.
    pub kind: ToastKind,
    /// Message.
    pub message: ToastMessage,
}

/// The toast for a finished use. `route` is what the use was routed with, which names the
/// app when a paste fell back to a copy.
#[must_use]
pub fn after_use(outcome: UseOutcome, route: &Route, pasted_into: Option<&str>) -> Toast {
    use snipdeck_core::PasteFailure::{TargetElevated, TargetUnavailable};
    let neutral = |message| Toast {
        kind: ToastKind::Neutral,
        message,
    };
    let app = |reason: &CopyReason| match reason {
        CopyReason::Elevated { app }
        | CopyReason::NothingFocused { app }
        | CopyReason::Excluded { app } => friendly_app_name(app),
        CopyReason::NoTarget => String::new(),
    };
    match (outcome, route) {
        (UseOutcome::Pasted, _) => Toast {
            kind: ToastKind::Success,
            message: ToastMessage::Pasted {
                app: pasted_into.map(friendly_app_name).unwrap_or_default(),
            },
        },
        (
            UseOutcome::Copied | UseOutcome::CopiedInstead(_),
            Route::Copy(reason @ CopyReason::Excluded { .. }),
        ) => neutral(ToastMessage::CopiedExcluded { app: app(reason) }),
        (UseOutcome::Copied, _) => Toast {
            kind: ToastKind::Success,
            message: ToastMessage::Copied,
        },
        (UseOutcome::CopiedInstead(TargetElevated), Route::Copy(reason)) => {
            neutral(ToastMessage::CopiedElevated { app: app(reason) })
        }
        (UseOutcome::CopiedInstead(_), Route::Copy(reason @ CopyReason::NothingFocused { .. })) => {
            neutral(ToastMessage::CopiedNothingFocused { app: app(reason) })
        }
        (UseOutcome::CopiedInstead(TargetUnavailable | TargetElevated), _) => {
            neutral(ToastMessage::CopiedTargetGone)
        }
    }
}

/// The toast for a use that did not happen.
#[must_use]
pub fn after_error(error: &UseError) -> Toast {
    use snipdeck_store::Error as Store;
    if matches!(error, UseError::NeedsAnswers(_)) {
        return Toast {
            kind: ToastKind::Neutral,
            message: ToastMessage::NeedsAnswers,
        };
    }
    let message = match error {
        UseError::Platform(_) => ToastMessage::ClipboardBusy,
        UseError::Render(snipdeck_core::template::RenderError::ProRequired(feature)) => {
            ToastMessage::NeedsPro(*feature)
        }
        UseError::Template(_) | UseError::NotText | UseError::Render(_) => {
            ToastMessage::TemplateBroken
        }
        UseError::Store(Store::EncryptionLocked) => ToastMessage::Locked,
        UseError::Store(_) | UseError::NeedsAnswers(_) => ToastMessage::Failed,
    };
    Toast {
        kind: ToastKind::Danger,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snipdeck_core::PasteFailure;
    use snipdeck_platform::keys::PasteKey;

    const PASTE: Route = Route::Paste {
        key: PasteKey::CtrlV,
    };

    #[test]
    fn a_paste_names_the_app() {
        let toast = after_use(UseOutcome::Pasted, &PASTE, Some("NOTEPAD.EXE"));
        assert_eq!(toast.kind, ToastKind::Success);
        assert_eq!(
            toast.message,
            ToastMessage::Pasted {
                app: "Notepad".into()
            }
        );
    }

    #[test]
    fn a_plain_copy_is_a_success() {
        let route = Route::Copy(CopyReason::NoTarget);
        let toast = after_use(UseOutcome::Copied, &route, None);
        assert_eq!(
            (toast.kind, toast.message),
            (ToastKind::Success, ToastMessage::Copied)
        );
    }

    #[test]
    fn an_excluded_app_says_pasting_is_off() {
        let route = Route::Copy(CopyReason::Excluded {
            app: "keepass.exe".into(),
        });
        let toast = after_use(UseOutcome::Copied, &route, None);
        assert_eq!(toast.kind, ToastKind::Neutral);
        assert_eq!(
            toast.message,
            ToastMessage::CopiedExcluded {
                app: "Keepass".into()
            }
        );
    }

    #[test]
    fn a_paste_into_an_excluded_app_says_pasting_is_off_too() {
        let route = Route::Copy(CopyReason::Excluded {
            app: "keepass.exe".into(),
        });
        let toast = after_use(
            UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable),
            &route,
            None,
        );
        assert_eq!(
            toast.message,
            ToastMessage::CopiedExcluded {
                app: "Keepass".into()
            }
        );
    }

    #[test]
    fn an_elevated_target_explains_why_it_was_only_copied() {
        let route = Route::Copy(CopyReason::Elevated {
            app: "regedit.exe".into(),
        });
        let toast = after_use(
            UseOutcome::CopiedInstead(PasteFailure::TargetElevated),
            &route,
            None,
        );
        assert_eq!(toast.kind, ToastKind::Neutral);
        assert_eq!(
            toast.message,
            ToastMessage::CopiedElevated {
                app: "Regedit".into()
            }
        );
    }

    #[test]
    fn nothing_focused_and_a_vanished_target_have_their_own_messages() {
        let route = Route::Copy(CopyReason::NothingFocused {
            app: "explorer.exe".into(),
        });
        let toast = after_use(
            UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable),
            &route,
            None,
        );
        assert_eq!(
            toast.message,
            ToastMessage::CopiedNothingFocused {
                app: "Explorer".into()
            }
        );
        let toast = after_use(
            UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable),
            &PASTE,
            Some("a.exe"),
        );
        assert_eq!(toast.message, ToastMessage::CopiedTargetGone);
    }

    #[test]
    fn errors_are_danger_toasts_with_a_specific_cause() {
        use snipdeck_platform::pipeline::Error as Platform;
        let busy = after_error(&UseError::Platform(Platform::Clipboard("x".into())));
        assert_eq!(
            (busy.kind, busy.message),
            (ToastKind::Danger, ToastMessage::ClipboardBusy)
        );
        let locked = after_error(&UseError::Store(snipdeck_store::Error::EncryptionLocked));
        assert_eq!(locked.message, ToastMessage::Locked);
        assert_eq!(
            after_error(&UseError::NotText).message,
            ToastMessage::TemplateBroken
        );
    }

    #[test]
    fn a_template_that_needs_answers_is_information_not_an_error() {
        let toast = after_error(&UseError::NeedsAnswers(Vec::new()));
        assert_eq!(
            (toast.kind, toast.message),
            (ToastKind::Neutral, ToastMessage::NeedsAnswers)
        );
    }
}
