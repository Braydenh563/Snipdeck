//! Whether an item can be pasted into the current Target, and how.

use snipdeck_core::PasteFailure;

use crate::keys::{PasteKey, paste_key_for};

/// What the platform could see of the Target's keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// A system caret is visible (`GetGUIThreadInfo`).
    Caret,
    /// A control has focus but no system caret was seen. Custom-drawn editors look like this;
    /// pasting is still attempted.
    Control,
    /// UI Automation found a focused editable element (the fallback for apps such as `OneNote`
    /// and Windows Terminal, where `GetGUIThreadInfo` reports nothing).
    Automation,
    /// Nothing focusable could be found.
    None,
}

/// The window that had focus before the user clicked Snipdeck.
#[derive(Clone, PartialEq, Eq)]
pub struct Target {
    /// Executable file name, for example `notepad.exe`.
    pub app: String,
    /// Window title, shown as "Pastes into App · title".
    pub title: String,
    /// The Target's integrity level is above ours, so Windows drops injected input (UIPI).
    pub elevated: bool,
    /// What focus was found inside the Target.
    pub focus: Focus,
}

// The title may be private (a document name, a page title): never print it.
impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("app", &self.app)
            .field("title_len", &self.title.len())
            .field("elevated", &self.elevated)
            .field("focus", &self.focus)
            .finish()
    }
}

/// User policy that affects routing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    /// Executable names (case-insensitive) that never receive an auto-paste (SPEC §5.8).
    pub no_paste_apps: Vec<String>,
}

/// Why an item is copied rather than pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyReason {
    /// There is no Target window.
    NoTarget,
    /// The Target runs elevated.
    Elevated {
        /// The Target's executable name, for the toast.
        app: String,
    },
    /// The Target has nothing that can take text.
    NothingFocused {
        /// The Target's executable name, for the toast.
        app: String,
    },
    /// The user excluded the Target from auto-paste.
    Excluded {
        /// The Target's executable name, for the toast.
        app: String,
    },
}

/// What a use does with the current Target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// Inject `key` into the Target.
    Paste {
        /// The Target's paste chord.
        key: PasteKey,
    },
    /// Leave the item on the clipboard.
    Copy(CopyReason),
}

impl CopyReason {
    /// The failure this reason reports when a paste was asked for, or `None` when copying is
    /// the expected result (no Target, SPEC §5.1a).
    pub fn failure(&self) -> Option<PasteFailure> {
        match self {
            Self::NoTarget => None,
            Self::Elevated { .. } => Some(PasteFailure::TargetElevated),
            Self::NothingFocused { .. } | Self::Excluded { .. } => {
                Some(PasteFailure::TargetUnavailable)
            }
        }
    }
}

/// Decides how to use an item given the Target (`None` when there is none).
pub fn decide(target: Option<&Target>, policy: &Policy) -> Route {
    let Some(target) = target else {
        return Route::Copy(CopyReason::NoTarget);
    };
    let app = || target.app.clone();
    if policy
        .no_paste_apps
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(&target.app))
    {
        return Route::Copy(CopyReason::Excluded { app: app() });
    }
    if target.elevated {
        return Route::Copy(CopyReason::Elevated { app: app() });
    }
    if target.focus == Focus::None {
        return Route::Copy(CopyReason::NothingFocused { app: app() });
    }
    Route::Paste {
        key: paste_key_for(&target.app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(app: &str, elevated: bool, focus: Focus) -> Target {
        Target {
            app: app.into(),
            title: "Untitled".into(),
            elevated,
            focus,
        }
    }

    #[test]
    fn target_debug_hides_the_title() {
        let mut t = target("a.exe", false, Focus::Caret);
        t.title = "Salary review.docx".into();
        assert!(!format!("{t:?}").contains("Salary"));
    }

    #[test]
    fn no_target_copies_and_is_not_a_failure() {
        let route = decide(None, &Policy::default());
        assert_eq!(route, Route::Copy(CopyReason::NoTarget));
        let Route::Copy(reason) = route else {
            unreachable!()
        };
        assert_eq!(reason.failure(), None);
    }

    #[test]
    fn elevated_target_falls_back_to_copy_with_uipi_failure() {
        let t = target("taskmgr.exe", true, Focus::Caret);
        let Route::Copy(reason) = decide(Some(&t), &Policy::default()) else {
            panic!()
        };
        assert_eq!(
            reason,
            CopyReason::Elevated {
                app: "taskmgr.exe".into()
            }
        );
        assert_eq!(reason.failure(), Some(PasteFailure::TargetElevated));
    }

    #[test]
    fn focus_none_copies_but_a_control_or_automation_focus_pastes() {
        let policy = Policy::default();
        assert!(matches!(
            decide(Some(&target("a.exe", false, Focus::None)), &policy),
            Route::Copy(CopyReason::NothingFocused { .. })
        ));
        for focus in [Focus::Caret, Focus::Control, Focus::Automation] {
            assert!(matches!(
                decide(Some(&target("a.exe", false, focus)), &policy),
                Route::Paste { .. }
            ));
        }
    }

    #[test]
    fn exclusion_is_case_insensitive_and_beats_everything_else() {
        let policy = Policy {
            no_paste_apps: vec!["NOTEPAD.EXE".into()],
        };
        let t = target("notepad.exe", false, Focus::Caret);
        assert_eq!(
            decide(Some(&t), &policy),
            Route::Copy(CopyReason::Excluded {
                app: "notepad.exe".into()
            })
        );
    }

    #[test]
    fn terminals_get_their_own_paste_key() {
        let t = target("MinTTY.exe", false, Focus::Automation);
        assert_eq!(
            decide(Some(&t), &Policy::default()),
            Route::Paste {
                key: PasteKey::ShiftInsert
            }
        );
    }
}
