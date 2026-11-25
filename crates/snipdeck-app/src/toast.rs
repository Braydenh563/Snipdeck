//! What the toast says after a use. Variants carry the data; the UI layer owns the wording
//! (`@tr`), so translations never live in Rust.

use snipdeck_core::UseOutcome;
use snipdeck_platform::route::{CopyReason, Route};

use crate::{
    failure::Cause,
    panel::friendly_app_name,
    settings::{DockEdge, Theme},
    usecase::UseError,
};

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
    /// "Pasted into {app}. Your earlier clipboard could not be put back."
    PastedNotRestored {
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
    /// "Copied. Only part of the keys went out, so some typed text may be gone."
    CopiedPartlySent,
    /// "Couldn’t reach the clipboard. Another app may be using it."
    ClipboardBusy,
    /// "This template can’t be read. Open it to fix it."
    TemplateBroken,
    /// "Unlock your library to use this item"
    Locked,
    /// "Library unlocked" (the master password was accepted)
    Unlocked,
    /// "Master password set"
    MasterPasswordSet,
    /// "Master password changed"
    MasterPasswordChanged,
    /// "Master password removed"
    MasterPasswordRemoved,
    /// "Master password changed. Snipdeck couldn't open the library with it just now; ..."
    MasterPasswordChangedNotOpen,
    /// "Master password removed. Snipdeck couldn't open the library just now; ..."
    MasterPasswordRemovedNotOpen,
    /// "Windows Hello didn’t work, so it was turned off. You can turn it on again in Settings."
    HelloTurnedOff,
    /// "Windows Hello was not turned on" (the user cancelled the prompt that turns it on)
    HelloNotTurnedOn,
    /// "Windows Hello didn't work, so it was not turned on" (the prompt that turns it on failed)
    HelloEnrolFailed,
    /// "Unlock your library first, so a copy includes your sensitive items."
    UnlockFirst,
    /// "Your library locked. The sensitive item you were editing was closed without saving."
    LockedEditorClosed,
    /// "Your library locked. What you were typing in the sensitive item is kept, and comes back
    /// when you unlock."
    LockedEditorKept,
    /// "Your library locked after a while. The form for the sensitive template you were filling in
    /// was closed."
    LockedFillClosed,
    /// "Saved to {list}" (quick capture).
    Saved {
        /// The list's name.
        list: String,
    },
    /// "No colour in the clipboard" (Ctrl+V on the Colours tab found none).
    NoColourOnClipboard,
    /// "Already saved in {list}" (a colour saved twice selects the first).
    AlreadySaved {
        /// The list's name.
        list: String,
    },
    /// "≈ royalblue, not exactly a colour name. Nothing copied" (a copy as Name of a colour that no
    /// CSS keyword is exactly).
    NameNotExact {
        /// The nearest keyword.
        name: String,
        /// That keyword's colour, as hex.
        hex: String,
    },
    /// "Saved to `list`. The same text was already in `was_in`." (a capture or a Ctrl+V of text
    /// another item holds): it is saved anyway, and Undo takes it back.
    SavedAgain {
        /// The list the copy went to.
        list: String,
        /// The list that holds the same text.
        was_in: String,
    },
    /// "Added to {list}" / "Added {count} items to {list}" (Ctrl+V on a list), with Undo and Edit.
    AddedTo {
        /// The list's name.
        list: String,
        /// How many items were made.
        count: usize,
    },
    /// "Added to {list}. Some were skipped: folders, and files that are empty, too large or
    /// unreadable." (Ctrl+V or a drop where some could not be used), with Undo and Edit.
    AddedSomeFiles {
        /// The list's name.
        list: String,
    },
    /// "Nothing was added. Folders, and files or pictures that are empty, too large or
    /// unreadable, are skipped."
    FilesNotAdded,
    /// "That is too many files to add at once"
    TooManyFiles,
    /// "Drop to add to {list}" (files dragged over a window).
    DropTo {
        /// The list's name.
        list: String,
    },
    /// "That looks like a password or key, so it was not saved. Check it, then save." (Ctrl+V
    /// with a secret on the clipboard opens the editor instead of saving)
    LooksSecret,
    /// "An item is open in the editor, so the pasted text was not added. Close it, then paste
    /// again." (Ctrl+V with a secret while an item editor is open)
    PasteNotAddedEditorOpen,
    /// "Nothing to save. Copy some text first." (quick capture with no text on the clipboard)
    NothingToSave,
    /// "That text is too large to save" (quick capture over 5 MB)
    TooLargeToSave,
    /// "There is no list to save into. Add a list first."
    NoListToSave,
    /// "Nothing to paste next. Pin an item, or add one to this list."
    NothingToPasteNext,
    /// "A stack is running. End it before you mark more snippets."
    StackRunning,
    /// "The form was closed without pasting"
    FormClosed,
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
    /// "{chord} is already used by another Snipdeck shortcut. Choose another in Settings."
    HotkeyOwnAction {
        /// The chord two Snipdeck shortcuts were given.
        chord: String,
    },
    /// "Windows refused {chord} (Windows error {code}). Choose another in Settings."
    HotkeyRefused {
        /// The chord Windows refused.
        chord: String,
        /// The Windows error code.
        code: u32,
    },
    /// "Shortcuts are unavailable, so {chord} is not active. Restart Snipdeck to try again."
    HotkeyUnavailable {
        /// The chord that could not be registered.
        chord: String,
    },
    /// "These shortcuts are not working: {chords}. Settings > Shortcuts says why." (more than one
    /// hotkey could not be registered)
    HotkeysFailed {
        /// The chords, joined with ", ".
        chords: String,
    },
    /// "Snipdeck is still running. Press {chord} or click the tray icon to bring the panel back."
    /// (shown once, the first time the Panel hides itself)
    PanelHidden {
        /// The Panel hotkey, or `None` when it is switched off.
        chord: Option<String>,
    },
    /// "Couldn’t use this item. Try again, or restart Snipdeck if it keeps happening." (the
    /// cause is not known)
    Failed,
    /// A use that failed for a cause the error names: "Couldn't use this item: the disk is full.
    /// Free up some space, then try again."
    UseFailed(Cause),
    /// "Couldn't open that window" (the Picker or the fill form could not be shown)
    WindowNotShown,
    /// "Couldn’t save your settings. Snipdeck will try again."
    SettingsNotSaved,
    /// "Your settings file was damaged. Snipdeck kept what it could read; the original is saved
    /// as settings.json.bad."
    SettingsDamaged,
    /// "Couldn't read your settings file, so Snipdeck started with the defaults. Changes to
    /// settings won't be saved until you fix or remove the file."
    SettingsUnreadable,
    /// "Your library couldn't be opened, so it was moved aside to {folder}." With a backup in
    /// the folder it adds that Settings > Rebuild from backup restores the newest one.
    LibrarySetAside {
        /// The folder the library was moved into (a name inside the data folder).
        folder: String,
        /// Whether the data folder holds a backup to rebuild from.
        has_backup: bool,
    },
    /// "Snipdeck can’t watch the clipboard, so Recent stays empty. Restart Snipdeck to try again."
    ClipboardWatchFailed,
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
    /// "Open a list to group its items": Ctrl+G in a search or a view of several lists.
    GroupNeedsList,
    /// "Clear the colour filter to group items": the rows show no group headers under it.
    GroupColourFilter,
    /// "Select the items to group first".
    GroupNothing,
    /// "Groups nest three levels deep at most": a nest or a new group inside a group would go
    /// deeper.
    GroupTooDeep,
    /// "Deleted tag work": the items keep their other tags.
    TagDeleted {
        /// The tag's name.
        name: String,
    },
    /// "Merged tag work into Work": the items of the first carry the second now.
    TagMerged {
        /// The tag that went.
        from: String,
        /// The tag its items moved to.
        into: String,
    },
    /// "That edit of a sensitive item cannot be undone": Undo stopped at an edit whose text is
    /// not kept.
    UndoStopped,
    /// "That step cannot be undone any more: what it changed is gone for good."
    HistoryStepLost,
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
    /// "Snipdeck found a problem in your library. Export a copy to be safe." (the weekly check)
    LibraryProblem,
    /// "Snipdeck ran into a problem. Open the log?" The action opens the log folder
    /// (`crash::open_log_folder`).
    CrashedLastTime,
    /// "Backed up".
    BackupMade,
    /// "Backed up, but {count} items could not be included, such as {first}." (an attachment
    /// could not be read; the rest of the backup is whole)
    BackupMadeSkipping {
        /// How many items were left out.
        count: usize,
        /// The title of the first one.
        first: String,
    },
    /// "This backup holds {now} items, the one before held {was}. The older backups were kept."
    BackupShrunk {
        /// Items in the previous backup.
        was: usize,
        /// Items in this one.
        now: usize,
    },
    /// "Another backup or export is still running. Try again when it has finished."
    ArchiveBusy,
    /// "The Store version keeps your library inside the app, so uninstalling removes it. Back up
    /// now also saves a copy in Documents > Snipdeck backups." Shown once (`store_data`).
    StoreDataReminder,
    /// "Couldn't make a backup: the disk is full. ..." (a backup the user asked for)
    BackupFailed(Cause),
    /// "Couldn't make today's backup. Use Back up now in Settings to try again; ..." (the daily
    /// run)
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
    /// "Database rebuilt: 5 items restored, 2 recovered. Your old database is kept in the folder ...".
    DatabaseRebuilt {
        /// Items the backup brought back.
        restored: usize,
        /// Items read out of the damaged database that the backup did not hold.
        recovered: usize,
        /// The name of the folder the old database was kept in.
        kept: String,
        /// The damaged database could not be read at all, so newer items were not recovered.
        unreadable: bool,
        /// Sensitive items the old library held that the new one does not.
        missing_sensitive: usize,
    },
    /// "Couldn't reopen your library. Restart Snipdeck; your files are untouched."
    RebuildNeedsRestart,
    /// "Your library was rebuilt, but the cleanup failed. Snipdeck will finish it the next time it starts."
    RebuildUnfinished,
    /// "Images can’t be pasted here. Save the picture as a PNG and add it as an Image item." (Ctrl+V
    /// in the item editor with a picture on the clipboard)
    ImageNotPasteable,
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
    /// "Saved {file}, but {count} items could not be included, such as {first}."
    ExportedSkipping {
        /// The file's name.
        file: String,
        /// How many items were left out.
        count: usize,
        /// The title of the first one.
        first: String,
    },
    /// "Saved {file}: 12 of 15 items. Sensitive items and items with no text are left out."
    ExportedSome {
        /// The file's name.
        file: String,
        /// Items written.
        written: usize,
        /// Items in the list.
        total: usize,
    },
    /// "Tag added to 3 items" or "Tag removed from 3 items" (a bulk tag toggle).
    ItemsTagged {
        /// How many items changed.
        count: usize,
        /// Whether the tag was added (otherwise removed).
        added: bool,
    },
    /// "Couldn't export your library: ..."
    ExportFailed(Cause),
    /// "Your library was erased"
    LibraryErased,
    /// "Couldn’t erase everything. Quit Snipdeck and try again."
    EraseFailed,
    /// "Couldn't open the folder"
    FolderNotOpened,
    /// "Couldn't open the link" (a web page that did not open)
    LinkNotOpened,
    /// "Couldn't change the Windows start-up entry"
    AutostartFailed,
    /// "Windows has Snipdeck switched off under Settings > Apps > Startup. Switch it on there."
    /// (the Store build; only the user can switch the startup task back on)
    AutostartSwitchedOff,
    /// "Your organisation's policy keeps Snipdeck from starting with Windows"
    AutostartPolicy,
    /// "Couldn't open the log folder"
    CannotOpenLog,
    /// "Saved. This trigger overlaps the trigger of “{other}”, so one of them may never fire."
    TriggerOverlaps {
        /// The title of the other item whose trigger overlaps.
        other: String,
    },
    /// "Couldn’t start typing triggers. Nothing will be expanded or suggested."
    TypingUnavailable,
    /// "Couldn't unlock the encryption key. Nothing was saved."
    KeyUnavailable,
    /// "This item was deleted. Nothing was saved."
    ItemGone,
    /// "That list was deleted. Choose another list."
    ListGone,
    /// "Couldn’t write to your library. Check that the disk has room."
    StorageFailed,
    /// "Couldn’t save this item. Nothing was changed."
    NotSaved,
    /// "Couldn't open this item"
    NotOpened,
    /// "Couldn't save a draft of what you typed"
    DraftNotSaved,
    /// A move, reorder or other change to the library that the store refused, for a cause the
    /// error names: "Couldn't make that change: the disk is full. ..."
    ChangeFailed(Cause),
    /// "Couldn't start the screenshot tool" (Take screenshot)
    ScreenshotFailed,
    /// "Theme: Dark": the theme was switched from a menu, the palette or Settings.
    ThemeSet(Theme),
    /// "Panel stays open" (`true`: on top) or "Panel closes when you click away": the Panel's pin
    /// was switched from its button or the palette.
    PanelOnTop(bool),
    /// "Pinned" (`true`) or "Unpinned": the pin of an item, in the Panel or the Main window.
    Pinned(bool),
    /// "Duplicated".
    Duplicated,
    /// "List added": the Panel's `+` tab.
    ListAdded,
    /// "List saved": the list editor.
    ListSaved,
    /// "Switch this list to Manual order?": a drag or Alt+Up / Alt+Down in a list that sorts itself.
    /// The toast's button switches list `list` to manual order.
    SwitchToManual {
        /// The list that was dragged in.
        list: snipdeck_core::ListId,
    },
    /// "Board saved": the board editor.
    BoardSaved,
    /// "Removed from Recent".
    RecentDeleted,
    /// "Recent cleared".
    RecentCleared,
    /// "Recently used emoji and colours cleared".
    PickerRecentsCleared,
    /// "File moved or deleted": a Recent entry's files are gone from where they were copied.
    FileMoved,
    /// "Some files were left out: together they are over 64 MB": a files entry passes the total cap.
    FilesTooLarge,
    /// "Panel docked to the left edge" (and the others), or `None`: "Panel floats again".
    DockSet(Option<DockEdge>),
    /// "Couldn't refresh the view": the change worked, but the list on screen could not be read
    /// again and may be out of date.
    NotRefreshed,
    /// "Icons can't be added to a list yet" (Symbols: Add to list on an icon)
    IconNotAddable,
    /// "Saved {name}." (Symbols: Save as... wrote the file)
    IconSaved {
        /// The file name written, without its folder.
        name: String,
    },
    /// "Couldn't pick a colour from the screen" (the eyedropper failed)
    EyedropFailed,
    /// "HDR is on for this screen: {hex} is approximate." (a colour picked on an HDR monitor)
    EyedropHdr {
        /// The picked colour, `#rrggbb`.
        hex: String,
    },
    /// "Copied {hex}. HDR is on for this screen, so it is approximate." (one toast for the copy
    /// and the HDR note)
    EyedropHdrCopied {
        /// The picked colour, `#rrggbb`.
        hex: String,
    },
    /// "Windows won't let Snipdeck read this screen." (the capture was refused)
    EyedropRefused,
    /// "The screen changed, so the pick ended." / "The pick timed out." (not the user's doing)
    EyedropEnded {
        /// Whether it was the time limit (else the display changed).
        timed_out: bool,
    },
    /// "Another app keeps {keys}: use the mouse for those" (fallback hotkeys refused)
    EyedropKeysBusy {
        /// The keys, as written for the user.
        keys: String,
    },
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
    use snipdeck_core::PasteFailure::{PartlySent, TargetElevated, TargetUnavailable};
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
        // The paste went through; the clipboard the user had before it is gone. Say so.
        (UseOutcome::PastedNotRestored, _) => neutral(ToastMessage::PastedNotRestored {
            app: pasted_into.map(friendly_app_name).unwrap_or_default(),
        }),
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
        (UseOutcome::CopiedInstead(PartlySent), _) => neutral(ToastMessage::CopiedPartlySent),
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
        UseError::Template(_) | UseError::NotText | UseError::Render(_) => {
            ToastMessage::TemplateBroken
        }
        UseError::Store(Store::EncryptionLocked) => ToastMessage::Locked,
        UseError::Store(e) => match Cause::of_store(e) {
            Cause::Other => ToastMessage::Failed,
            cause => ToastMessage::UseFailed(cause),
        },
        // A template that needs answers opens the fill form instead of reaching a toast.
        // No random numbers for `{uuid}` is a plain failure.
        UseError::NeedsAnswers(_) | UseError::NoRandomness => ToastMessage::Failed,
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
    fn a_paste_that_could_not_restore_the_clipboard_says_so() {
        let restored = after_use(UseOutcome::Pasted, &PASTE, Some("NOTEPAD.EXE"));
        let lost = after_use(UseOutcome::PastedNotRestored, &PASTE, Some("NOTEPAD.EXE"));
        assert_ne!(lost, restored);
        assert_eq!(
            lost.message,
            ToastMessage::PastedNotRestored {
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
                app: "File Explorer".into()
            }
        );
        let toast = after_use(
            UseOutcome::CopiedInstead(PasteFailure::TargetUnavailable),
            &PASTE,
            Some("a.exe"),
        );
        assert_eq!(toast.message, ToastMessage::CopiedTargetGone);
        let toast = after_use(
            UseOutcome::CopiedInstead(PasteFailure::PartlySent),
            &PASTE,
            Some("a.exe"),
        );
        assert_eq!(toast.message, ToastMessage::CopiedPartlySent);
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
            ToastMessage::UseFailed(Cause::NotAllowed)
        );
        assert_eq!(
            store(snipdeck_store::Error::Corrupt("x".into())),
            ToastMessage::Failed,
            "a cause the code cannot tell apart is not invented"
        );
    }
}
