//! What the toast says after a use. Variants carry the data; the UI layer owns the wording
//! (`@tr`), so translations never live in Rust.

use snipdeck_core::{Feature, UseOutcome};
use snipdeck_platform::route::{CopyReason, Route};

use crate::{failure::Cause, panel::friendly_app_name, usecase::UseError};

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

/// Why a licence key was not accepted: one reason per toast, so the user knows what to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenceProblem {
    /// Nothing was pasted.
    Empty,
    /// The text is not shaped like a Snipdeck key (wrong prefix or parts).
    NotAKey,
    /// The key is well formed but was not signed by Snipdeck (mistyped, cut short or forged).
    BadSignature,
    /// The key comes from a newer Snipdeck.
    NewerFormat,
    /// The key is genuine but not for Supporter.
    WrongTier,
    /// The key is good but could not be saved next to the library.
    NotSaved,
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
    /// "This item uses a Supporter feature."
    NeedsPro(Feature),
    /// "This template can't be read. Open it to fix it."
    TemplateBroken,
    /// "Unlock your library to use this item."
    Locked,
    /// "Library unlocked." (the master password was accepted)
    Unlocked,
    /// "Master password set."
    MasterPasswordSet,
    /// "Master password changed."
    MasterPasswordChanged,
    /// "Master password removed."
    MasterPasswordRemoved,
    /// "Master password changed. Snipdeck couldn't open the library with it just now; ..."
    MasterPasswordChangedNotOpen,
    /// "Master password removed. Snipdeck couldn't open the library just now; ..."
    MasterPasswordRemovedNotOpen,
    /// "Unlock your library first, so a copy includes your sensitive items."
    UnlockFirst,
    /// "Your library locked. The sensitive item you were editing was closed without saving."
    LockedEditorClosed,
    /// "Saved to {list}" (quick capture).
    Saved {
        /// The list's name.
        list: String,
    },
    /// "Nothing to save. Copy some text first." (quick capture with no text on the clipboard)
    NothingToSave,
    /// "That text is too large to save." (quick capture over 5 MB)
    TooLargeToSave,
    /// "There is no list to save into. Add a list first."
    NoListToSave,
    /// "Nothing to paste next. Pin a snippet, or add one to this list."
    NothingToPasteNext,
    /// "The taskbar is on that edge. Pick another edge, or move the taskbar."
    TaskbarEdge,
    /// "That was the last one. Paste next starts again from the first."
    PasteNextFinished,
    /// "{chord} is used by another app. Choose another in Settings."
    HotkeyInUse {
        /// The chord as typed in Settings, for example `Ctrl+Alt+Space`.
        chord: String,
    },
    /// "{chord} can't be used as a shortcut. Choose another in Settings."
    HotkeyInvalid {
        /// The chord as typed in Settings.
        chord: String,
    },
    /// "Shortcuts are unavailable, so {chord} is not active. Restart Snipdeck to try again."
    HotkeyUnavailable {
        /// The chord that could not be registered.
        chord: String,
    },
    /// "Snipdeck is still running. Press {chord} or click the tray icon to bring the panel back."
    /// (shown once, the first time the Panel hides itself)
    PanelHidden {
        /// The Panel hotkey, or `None` when it is switched off.
        chord: Option<String>,
    },
    /// "Couldn't use this item. Try again, or restart Snipdeck if it keeps happening." (the
    /// cause is not known)
    Failed,
    /// A use that failed for a cause the error names: "Couldn't use this item: the disk is full.
    /// Free up some space, then try again."
    UseFailed(Cause),
    /// "Couldn't open that window." (the Picker or the fill form could not be shown)
    WindowNotShown,
    /// "Couldn't save your settings. Snipdeck will try again."
    SettingsNotSaved,
    /// "Moved 3 items to the Trash" (with Undo).
    ItemsTrashed {
        /// How many.
        count: usize,
    },
    /// "Deleted list “Code”" (with Undo).
    ListTrashed {
        /// Its name.
        name: String,
    },
    /// "Deleted board “Work”" (with Undo).
    BoardTrashed {
        /// Its name.
        name: String,
    },
    /// "Restored 2 items".
    Restored {
        /// How many Trash entries came back.
        count: usize,
    },
    /// "Undid delete": the history took a step back.
    Undone {
        /// What was taken back.
        action: crate::history::Action,
    },
    /// "Redid delete": the history brought a step back.
    Redone {
        /// What was brought back.
        action: crate::history::Action,
    },
    /// "Moved 2 items to Code".
    ItemsMoved {
        /// How many.
        count: usize,
        /// The list they went to.
        list: String,
    },
    /// "Moved to position 2 of 7": says where a keyboard or menu move put the item.
    ItemPlaced {
        /// Its place in the list, from 1.
        position: usize,
        /// How many items the list has.
        total: usize,
    },
    /// "Deleted for good".
    DeletedForever {
        /// How many Trash entries went.
        count: usize,
    },
    /// "The Trash is empty".
    TrashEmptied,
    /// "The Free plan includes 5 boards. Supporter adds unlimited boards."
    BoardLimit,
    /// "Snipdeck found a problem in your library. Export a copy to be safe." (the weekly check)
    LibraryProblem,
    /// "Snipdeck closed unexpectedly last time. Open the log?" The action opens the log folder
    /// (`crash::open_log_folder`).
    CrashedLastTime,
    /// "Snipdeck Supporter is on. Thank you."
    LicenceAccepted,
    /// A licence key that was refused, and why.
    LicenceRejected(LicenceProblem),
    /// "Backed up".
    BackupMade,
    /// "Couldn't make a backup: the disk is full. ..." (a backup the user asked for)
    BackupFailed(Cause),
    /// "Couldn't make today's backup. Check the disk has room, then use Back up now in
    /// Settings." (the daily run)
    DailyBackupFailed(Cause),
    /// "Couldn't clear old items out of the Trash. ..." (the daily run)
    TrashPurgeFailed,
    /// "Restored 5 items from the backup" (or: nothing was missing).
    BackupRestored {
        /// Items added back.
        items: usize,
    },
    /// "Couldn't restore that backup: ..."
    RestoreFailed(Cause),
    /// "Database rebuilt: 5 items restored, 2 recovered. Your old database is kept as ...".
    DatabaseRebuilt {
        /// Items the backup brought back.
        restored: usize,
        /// Items read out of the damaged database that the backup did not hold.
        recovered: usize,
        /// The file name the old database was kept under.
        kept: String,
        /// The damaged database could not be read at all, so newer items were not recovered.
        unreadable: bool,
        /// Sensitive items the old library held that the new one does not.
        missing_sensitive: usize,
    },
    /// "Couldn't reopen your library. Restart Snipdeck; your files are untouched."
    RebuildNeedsRestart,
    /// "Imported 5 items".
    Imported {
        /// Items added.
        items: usize,
    },
    /// "Couldn't import that: ..."
    ImportFailed(Cause),
    /// "Saved {file} in {folder}": an export.
    Exported {
        /// The file's name.
        file: String,
    },
    /// "Couldn't export your library: ..."
    ExportFailed(Cause),
    /// "Your library was erased."
    LibraryErased,
    /// "Couldn't erase everything. Quit Snipdeck and try again."
    EraseFailed,
    /// "Couldn't open the folder."
    FolderNotOpened,
    /// "Couldn't open your email app." (Get Supporter)
    MailNotOpened,
    /// "Couldn't change the Windows start-up entry."
    AutostartFailed,
    /// "Couldn't open the log folder."
    CannotOpenLog,
    /// "Couldn't unlock the encryption key. Nothing was saved."
    KeyUnavailable,
    /// "This item was deleted. Nothing was saved."
    ItemGone,
    /// "That list was deleted. Choose another list."
    ListGone,
    /// "Couldn't write to your library. Check that the disk has room."
    StorageFailed,
    /// "Couldn't save this item. Nothing was changed."
    NotSaved,
    /// "Couldn't open this item."
    NotOpened,
    /// "Couldn't save a draft of what you typed."
    DraftNotSaved,
    /// A move, reorder or other change to the library that the store refused, for a cause the
    /// error names: "Couldn't make that change: the disk is full. ..."
    ChangeFailed(Cause),
    /// "Couldn't start the screenshot tool." (Take screenshot)
    ScreenshotFailed,
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
        (UseOutcome::Pasted | UseOutcome::PastedNotRestored, _) => Toast {
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
    let message = match error {
        UseError::Platform(_) => ToastMessage::ClipboardBusy,
        UseError::Render(snipdeck_core::template::RenderError::ProRequired(feature)) => {
            ToastMessage::NeedsPro(*feature)
        }
        UseError::Template(_) | UseError::NotText | UseError::Render(_) => {
            ToastMessage::TemplateBroken
        }
        UseError::Store(Store::EncryptionLocked) => ToastMessage::Locked,
        UseError::Store(e) => match Cause::of_store(e) {
            Cause::Other => ToastMessage::Failed,
            cause => ToastMessage::UseFailed(cause),
        },
        // A template that needs answers opens the fill form instead of reaching a toast.
        UseError::NeedsAnswers(_) => ToastMessage::Failed,
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
    fn a_store_failure_is_worded_by_its_cause_when_the_error_names_one() {
        use std::io::{Error as Io, ErrorKind};
        let store = |e| after_error(&UseError::Store(e)).message;
        assert_eq!(
            store(snipdeck_store::Error::Io(Io::from(ErrorKind::StorageFull))),
            ToastMessage::UseFailed(Cause::DiskFull)
        );
        assert_eq!(
            store(snipdeck_store::Error::Io(Io::from(
                ErrorKind::PermissionDenied
            ))),
            ToastMessage::UseFailed(Cause::InUse)
        );
        assert_eq!(
            store(snipdeck_store::Error::Corrupt("x".into())),
            ToastMessage::Failed,
            "a cause the code cannot tell apart is not invented"
        );
    }
}
