//! The words of every toast come from the UI crate's `ToastTexts` global, so they follow the
//! language selected for the UI. Rust builds no English for a toast. One process for this file
//! (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    failure::Cause,
    history::Action,
    settings::{DockEdge, Theme},
    toast::ToastMessage,
    wording::toast_text,
};
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

/// The position of `message`'s variant in [`samples`]. The match has no wildcard, so a new
/// `ToastMessage` variant stops this file compiling until it is listed (and so tested).
#[allow(clippy::too_many_lines, reason = "one line per message variant")]
const fn position(message: &ToastMessage) -> usize {
    match message {
        ToastMessage::Pasted { .. } => 0,
        ToastMessage::PastedNotRestored { .. } => 1,
        ToastMessage::Copied => 2,
        ToastMessage::CopiedElevated { .. } => 3,
        ToastMessage::CopiedNothingFocused { .. } => 4,
        ToastMessage::CopiedExcluded { .. } => 5,
        ToastMessage::CopiedTargetGone => 6,
        ToastMessage::ClipboardBusy => 7,
        ToastMessage::TemplateBroken => 8,
        ToastMessage::Locked => 9,
        ToastMessage::Unlocked => 10,
        ToastMessage::MasterPasswordSet => 11,
        ToastMessage::MasterPasswordChanged => 12,
        ToastMessage::MasterPasswordRemoved => 13,
        ToastMessage::MasterPasswordChangedNotOpen => 14,
        ToastMessage::MasterPasswordRemovedNotOpen => 15,
        ToastMessage::UnlockFirst => 16,
        ToastMessage::LockedEditorClosed => 17,
        ToastMessage::LockedFillClosed => 18,
        ToastMessage::LockedEditorKept => 19,
        ToastMessage::Saved { .. } => 20,
        ToastMessage::NothingToSave => 21,
        ToastMessage::TooLargeToSave => 22,
        ToastMessage::NoListToSave => 23,
        ToastMessage::NothingToPasteNext => 24,
        ToastMessage::PasteNextFinished => 25,
        ToastMessage::HotkeyInUse { .. } => 26,
        ToastMessage::HotkeyInvalid { .. } => 27,
        ToastMessage::HotkeyUnavailable { .. } => 28,
        ToastMessage::PanelHidden { .. } => 29,
        ToastMessage::Failed => 30,
        ToastMessage::WindowNotShown => 31,
        ToastMessage::SettingsNotSaved => 32,
        ToastMessage::SettingsDamaged => 33,
        ToastMessage::SettingsUnreadable => 34,
        ToastMessage::ClipboardWatchFailed => 35,
        ToastMessage::ItemsTrashed { .. } => 36,
        ToastMessage::ListTrashed { .. } => 37,
        ToastMessage::BoardTrashed { .. } => 38,
        ToastMessage::Restored { .. } => 39,
        ToastMessage::ItemsMoved { .. } => 40,
        ToastMessage::DeletedForever { .. } => 41,
        ToastMessage::TrashEmptied => 42,
        ToastMessage::LibraryProblem => 43,
        ToastMessage::CrashedLastTime => 44,
        ToastMessage::BackupMade => 45,
        ToastMessage::BackupFailed(_) => 46,
        ToastMessage::BackupRestored { .. } => 47,
        ToastMessage::RestoreFailed(_) => 48,
        ToastMessage::Imported { .. } => 49,
        ToastMessage::ImportFailed(_) => 50,
        ToastMessage::Exported { .. } => 51,
        ToastMessage::ExportFailed(_) => 52,
        ToastMessage::LibraryErased => 53,
        ToastMessage::EraseFailed => 54,
        ToastMessage::FolderNotOpened => 55,
        ToastMessage::LinkNotOpened => 56,
        ToastMessage::AutostartFailed => 57,
        ToastMessage::CannotOpenLog => 58,
        ToastMessage::KeyUnavailable => 59,
        ToastMessage::ItemGone => 60,
        ToastMessage::ListGone => 61,
        ToastMessage::StorageFailed => 62,
        ToastMessage::NotSaved => 63,
        ToastMessage::NotOpened => 64,
        ToastMessage::DraftNotSaved => 65,
        ToastMessage::ItemPlaced { .. } => 66,
        ToastMessage::TaskbarEdge => 67,
        ToastMessage::UseFailed(_) => 68,
        ToastMessage::DailyBackupFailed(_) => 69,
        ToastMessage::TrashPurgeFailed => 70,
        ToastMessage::ChangeFailed(_) => 71,
        ToastMessage::ScreenshotFailed => 72,
        ToastMessage::Undone { .. } => 73,
        ToastMessage::Redone { .. } => 74,
        ToastMessage::DatabaseRebuilt { .. } => 75,
        ToastMessage::RebuildNeedsRestart => 76,
        ToastMessage::RebuildUnfinished => 77,
        ToastMessage::ImageNotPasteable => 78,
        ToastMessage::HotkeysFailed { .. } => 79,
        ToastMessage::ThemeSet(_) => 80,
        ToastMessage::PanelOnTop(_) => 81,
        ToastMessage::Pinned(_) => 82,
        ToastMessage::Duplicated => 83,
        ToastMessage::ListAdded => 84,
        ToastMessage::ListSaved => 85,
        ToastMessage::BoardSaved => 86,
        ToastMessage::RecentDeleted => 87,
        ToastMessage::RecentCleared => 88,
        ToastMessage::PickerRecentsCleared => 89,
        ToastMessage::GroupNeedsList => 90,
        ToastMessage::GroupColourFilter => 91,
        ToastMessage::GroupNothing => 92,
        ToastMessage::GroupTooDeep => 93,
        ToastMessage::TagDeleted { .. } => 94,
        ToastMessage::TagMerged { .. } => 95,
        ToastMessage::UndoStopped => 96,
        ToastMessage::HistoryStepLost => 97,
        ToastMessage::SavedAgain { .. } => 98,
        ToastMessage::DockSet(_) => 99,
        ToastMessage::NotRefreshed => 100,
        ToastMessage::IconNotAddable => 101,
        ToastMessage::IconSaved { .. } => 102,
        ToastMessage::EyedropFailed => 103,
        ToastMessage::EyedropHdr { .. } => 104,
        ToastMessage::AddedTo { .. } => 105,
        ToastMessage::LooksSecret => 106,
        ToastMessage::PasteNotAddedEditorOpen => 107,
        ToastMessage::AddedSomeFiles { .. } => 108,
        ToastMessage::FilesNotAdded => 109,
        ToastMessage::DropTo { .. } => 110,
        ToastMessage::TooManyFiles => 111,
        ToastMessage::SwitchToManual { .. } => 112,
        ToastMessage::FileMoved => 113,
        ToastMessage::FilesTooLarge => 114,
        ToastMessage::HelloTurnedOff => 115,
        ToastMessage::HelloNotTurnedOn => 116,
        ToastMessage::HelloEnrolFailed => 117,
        ToastMessage::TriggerOverlaps { .. } => 118,
        ToastMessage::TypingUnavailable => 119,
        ToastMessage::AutostartSwitchedOff => 120,
        ToastMessage::AutostartPolicy => 121,
        ToastMessage::AlreadySaved { .. } => 122,
        ToastMessage::NoColourOnClipboard => 123,
        ToastMessage::EyedropHdrCopied { .. } => 124,
        ToastMessage::EyedropRefused => 125,
        ToastMessage::EyedropEnded { .. } => 126,
        ToastMessage::EyedropKeysBusy { .. } => 127,
        ToastMessage::NameNotExact { .. } => 128,
        ToastMessage::ArchiveBusy => 129,
        ToastMessage::StoreDataReminder => 130,
        ToastMessage::HotkeyOwnAction { .. } => 131,
        ToastMessage::HotkeyRefused { .. } => 132,
        ToastMessage::BackupMadeSkipping { .. } => 133,
        ToastMessage::BackupShrunk { .. } => 134,
        ToastMessage::ExportedSkipping { .. } => 135,
        ToastMessage::LibrarySetAside { .. } => 136,
        ToastMessage::ExportedSome { .. } => 137,
        ToastMessage::ItemsTagged { .. } => 138,
        ToastMessage::CopiedPartlySent => 139,
        ToastMessage::StackRunning => 140,
        ToastMessage::FormClosed => 141,
    }
}

const VARIANTS: usize = 142;

fn s(text: &str) -> String {
    text.to_owned()
}

/// One message per variant, with the awkward arguments (no app name, a count of one, a count of
/// none) as extra entries.
#[allow(clippy::too_many_lines, reason = "one line per message variant")]
fn samples() -> Vec<ToastMessage> {
    use ToastMessage as M;
    vec![
        M::Pasted { app: s("Outlook") },
        M::PastedNotRestored { app: s("Outlook") },
        M::Copied,
        M::CopiedElevated { app: s("Outlook") },
        M::CopiedNothingFocused { app: s("Outlook") },
        M::CopiedExcluded { app: s("Outlook") },
        M::CopiedTargetGone,
        M::CopiedPartlySent,
        M::ClipboardBusy,
        M::TemplateBroken,
        M::Locked,
        M::Unlocked,
        M::MasterPasswordSet,
        M::MasterPasswordChanged,
        M::MasterPasswordRemoved,
        M::MasterPasswordChangedNotOpen,
        M::MasterPasswordRemovedNotOpen,
        M::UnlockFirst,
        M::LockedEditorClosed,
        M::LockedEditorKept,
        M::LockedFillClosed,
        M::Saved { list: s("Notes") },
        M::SavedAgain {
            list: s("Notes"),
            was_in: s("Snippets"),
        },
        M::NothingToSave,
        M::TooLargeToSave,
        M::NoListToSave,
        M::NothingToPasteNext,
        M::StackRunning,
        M::FormClosed,
        M::PasteNextFinished,
        M::HotkeyInUse {
            chord: s("Ctrl+Alt+Space"),
        },
        M::HotkeyInvalid {
            chord: s("Ctrl+Alt+Space"),
        },
        M::HotkeyUnavailable {
            chord: s("Ctrl+Alt+Space"),
        },
        M::HotkeysFailed {
            chords: s("Ctrl+Alt+Space, Ctrl+Alt+V"),
        },
        M::PanelHidden {
            chord: Some(s("Ctrl+Shift+Space")),
        },
        M::Failed,
        M::WindowNotShown,
        M::SettingsNotSaved,
        M::ItemsTrashed { count: 3 },
        M::ListTrashed { name: s("Code") },
        M::BoardTrashed { name: s("Work") },
        M::Restored { count: 2 },
        M::ItemsMoved {
            count: 2,
            list: s("Code"),
        },
        M::DeletedForever { count: 1 },
        M::TrashEmptied,
        M::LibraryProblem,
        M::CrashedLastTime,
        M::BackupMade,
        M::BackupFailed(Cause::Other),
        M::BackupRestored { items: 5 },
        M::RestoreFailed(Cause::NotArchive),
        M::DatabaseRebuilt {
            restored: 5,
            recovered: 2,
            kept: s("snipdeck.damaged-20261017-1030"),
            unreadable: false,
            missing_sensitive: 1,
        },
        M::RebuildNeedsRestart,
        M::RebuildUnfinished,
        M::ImageNotPasteable,
        M::Imported { items: 5 },
        M::ImportFailed(Cause::TooNew),
        M::Exported {
            file: s("snipdeck.zip"),
        },
        M::ExportedSkipping {
            file: s("snipdeck.zip"),
            count: 2,
            first: s("Screenshot"),
        },
        M::BackupMadeSkipping {
            count: 1,
            first: s("Screenshot"),
        },
        M::BackupShrunk { was: 120, now: 3 },
        M::ExportedSome {
            file: s("list.txt"),
            written: 12,
            total: 15,
        },
        M::ItemsTagged {
            count: 3,
            added: true,
        },
        M::ItemsTagged {
            count: 1,
            added: false,
        },
        M::TagDeleted { name: s("Work") },
        M::TagMerged {
            from: s("Work"),
            into: s("Office"),
        },
        M::ExportFailed(Cause::Locked),
        M::LibraryErased,
        M::EraseFailed,
        M::FolderNotOpened,
        M::LinkNotOpened,
        M::AutostartFailed,
        M::AutostartSwitchedOff,
        M::AutostartPolicy,
        M::ArchiveBusy,
        M::StoreDataReminder,
        M::CannotOpenLog,
        M::KeyUnavailable,
        M::ItemGone,
        M::ListGone,
        M::StorageFailed,
        M::NotSaved,
        M::NotOpened,
        M::DraftNotSaved,
        M::ItemPlaced {
            position: 2,
            total: 7,
        },
        M::Undone {
            action: Action::Delete,
        },
        M::Redone {
            action: Action::Move,
        },
        // The same messages with the other shapes of their arguments.
        M::Pasted { app: String::new() },
        M::CopiedElevated { app: String::new() },
        M::PanelHidden { chord: None },
        M::ItemsTrashed { count: 1 },
        M::Restored { count: 1 },
        M::ItemsMoved {
            count: 1,
            list: s("Code"),
        },
        M::UseFailed(Cause::DiskFull),
        M::DailyBackupFailed(Cause::InUse),
        M::TrashPurgeFailed,
        M::ChangeFailed(Cause::Other),
        M::BackupRestored { items: 0 },
        M::BackupRestored { items: 1 },
        M::Imported { items: 0 },
        M::Imported { items: 1 },
        M::TaskbarEdge,
        M::ScreenshotFailed,
        M::ThemeSet(Theme::System),
        M::ThemeSet(Theme::Light),
        M::ThemeSet(Theme::Dark),
        M::PanelOnTop(true),
        M::PanelOnTop(false),
        M::Pinned(true),
        M::Pinned(false),
        M::Duplicated,
        M::ListAdded,
        M::ListSaved,
        M::BoardSaved,
        M::RecentDeleted,
        M::RecentCleared,
        M::PickerRecentsCleared,
        M::GroupNeedsList,
        M::GroupColourFilter,
        M::GroupNothing,
        M::GroupTooDeep,
        M::TagDeleted { name: s("work") },
        M::TagMerged {
            from: s("work"),
            into: s("Work"),
        },
        M::UndoStopped,
        M::HistoryStepLost,
        M::DockSet(None),
        M::DockSet(Some(DockEdge::Left)),
        M::DockSet(Some(DockEdge::Right)),
        M::DockSet(Some(DockEdge::Top)),
        M::DockSet(Some(DockEdge::Bottom)),
        M::NotRefreshed,
        M::IconNotAddable,
        M::IconSaved {
            name: s("trash.svg"),
        },
        M::EyedropFailed,
        M::EyedropHdr { hex: s("#3a7bd5") },
        M::AddedTo {
            list: s("Notes"),
            count: 1,
        },
        M::LooksSecret,
        M::PasteNotAddedEditorOpen,
        M::AddedSomeFiles { list: s("Notes") },
        M::FilesNotAdded,
        M::DropTo { list: s("Notes") },
        M::TooManyFiles,
        M::FileMoved,
        M::FilesTooLarge,
        M::TriggerOverlaps {
            other: s("Signature"),
        },
        M::TypingUnavailable,
        M::SwitchToManual {
            list: snipdeck_core::ListId::generate(),
        },
        M::HelloTurnedOff,
        M::HelloNotTurnedOn,
        M::HelloEnrolFailed,
        M::SettingsDamaged,
        M::SettingsUnreadable,
        M::LibrarySetAside {
            folder: s("snipdeck.damaged-20261116-1000"),
            has_backup: true,
        },
        M::LibrarySetAside {
            folder: s("snipdeck.damaged-20261116-1000"),
            has_backup: false,
        },
        M::ClipboardWatchFailed,
        M::AlreadySaved { list: s("Colours") },
        M::NoColourOnClipboard,
        M::EyedropHdrCopied { hex: s("#3a7bd5") },
        M::EyedropRefused,
        M::EyedropEnded { timed_out: true },
        M::EyedropEnded { timed_out: false },
        M::EyedropKeysBusy { keys: s("+, -") },
        M::NameNotExact {
            name: s("royalblue"),
            hex: s("#4169e1"),
        },
        M::HotkeyOwnAction {
            chord: s("Ctrl+Alt+Space"),
        },
        M::HotkeyRefused {
            chord: s("Ctrl+Alt+Space"),
            code: 5,
        },
    ]
}

fn ui() -> PanelWindow {
    PanelWindow::new().expect("window")
}

#[test]
fn every_toast_message_has_words() {
    on_ui_thread(|| {
        let ui = ui();
        let samples = samples();
        let mut seen = [false; VARIANTS];
        for message in &samples {
            seen[position(message)] = true;
            let text = toast_text(&ui, message);
            assert!(!text.trim().is_empty(), "{message:?} has no words");
            assert!(
                !text.contains("{}") && !text.contains("{0}"),
                "{message:?} left a placeholder open: {text}"
            );
        }
        assert!(seen.iter().all(|s| *s), "a message variant has no sample");
    });
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one table of sentences, a row per toast"
)]
fn what_a_toast_carries_reaches_its_sentence() {
    on_ui_thread(|| {
        let ui = ui();
        let said = |m: ToastMessage| toast_text(&ui, &m);
        assert_eq!(
            said(ToastMessage::Pasted { app: s("Outlook") }),
            "Pasted into Outlook"
        );
        assert_eq!(said(ToastMessage::Pasted { app: String::new() }), "Pasted");
        assert_eq!(
            said(ToastMessage::PastedNotRestored { app: s("Outlook") }),
            "Pasted into Outlook. Your earlier clipboard could not be put back."
        );
        assert_eq!(
            said(ToastMessage::PastedNotRestored { app: String::new() }),
            "Pasted. Your earlier clipboard could not be put back."
        );
        assert_eq!(
            said(ToastMessage::CopiedElevated { app: String::new() }),
            "Copied. That app runs as administrator, so Snipdeck can’t paste into it."
        );
        assert_eq!(
            said(ToastMessage::ItemsTrashed { count: 1 }),
            "Moved 1 item to the Trash"
        );
        assert_eq!(
            said(ToastMessage::ItemsTrashed { count: 3 }),
            "Moved 3 items to the Trash"
        );
        // MAIN-24, 25: a partial export says how many were written; a bulk tag change counts the items.
        assert_eq!(
            said(ToastMessage::ExportedSome {
                file: s("list.txt"),
                written: 12,
                total: 15
            }),
            "Saved list.txt: 12 of 15 items. Sensitive items and items with no text are left out."
        );
        for (count, added, text) in [
            (1, true, "Tag added to 1 item"),
            (3, true, "Tag added to 3 items"),
            (1, false, "Tag removed from 1 item"),
            (4, false, "Tag removed from 4 items"),
        ] {
            assert_eq!(said(ToastMessage::ItemsTagged { count, added }), text);
        }
        assert_eq!(
            said(ToastMessage::TagDeleted { name: s("Work") }),
            "Deleted the tag “Work”"
        );
        assert_eq!(
            said(ToastMessage::TagMerged {
                from: s("Work"),
                into: s("Office")
            }),
            "Merged the tag “Work” into “Office”"
        );
        let rebuilt = |recovered, unreadable, missing_sensitive| ToastMessage::DatabaseRebuilt {
            restored: 5,
            recovered,
            kept: s("old"),
            unreadable,
            missing_sensitive,
        };
        assert_eq!(
            said(rebuilt(2, false, 0)),
            "Database rebuilt: 5 items restored, 2 recovered. Your old database is kept in the folder old."
        );
        assert_eq!(
            said(rebuilt(0, true, 1)),
            "Database rebuilt: 5 items restored. Newer items could not be read. Your old database is kept in the folder old. 1 sensitive item was not in the backup."
        );
        assert_eq!(
            said(ToastMessage::Undone {
                action: Action::Delete
            }),
            "Undid delete"
        );
        assert_eq!(
            said(ToastMessage::Redone {
                action: Action::Colour
            }),
            "Redid colour change"
        );
        assert_eq!(
            said(ToastMessage::ItemsMoved {
                count: 2,
                list: s("Code")
            }),
            "Moved 2 items to Code"
        );
        assert_eq!(
            said(ToastMessage::ItemPlaced {
                position: 2,
                total: 7
            }),
            "Moved to position 2 of 7"
        );
        assert_eq!(
            said(ToastMessage::ListTrashed { name: s("Code") }),
            "Deleted list \u{201c}Code\u{201d}"
        );
        assert_eq!(
            said(ToastMessage::BackupRestored { items: 0 }),
            "Nothing to restore: your library already has everything in that backup."
        );
        assert_eq!(said(ToastMessage::Imported { items: 1 }), "Imported 1 item");
        assert_eq!(said(ToastMessage::ThemeSet(Theme::System)), "Theme: System");
        assert_eq!(said(ToastMessage::ThemeSet(Theme::Light)), "Theme: Light");
        assert_eq!(said(ToastMessage::ThemeSet(Theme::Dark)), "Theme: Dark");
        assert_eq!(said(ToastMessage::PanelOnTop(true)), "Panel stays open");
        assert_eq!(said(ToastMessage::Pinned(true)), "Item pinned");
        assert_eq!(said(ToastMessage::Pinned(false)), "Item unpinned");
        assert_eq!(said(ToastMessage::Duplicated), "Duplicated");
        assert_eq!(said(ToastMessage::ListAdded), "List added");
        assert_eq!(said(ToastMessage::ListSaved), "List saved");
        assert_eq!(
            said(ToastMessage::SwitchToManual {
                list: snipdeck_core::ListId::generate()
            }),
            "Switch this list to Manual order?"
        );
        assert_eq!(said(ToastMessage::BoardSaved), "Board saved");
        assert_eq!(said(ToastMessage::RecentDeleted), "Removed from Recent");
        assert_eq!(said(ToastMessage::RecentCleared), "Recent history cleared");
        assert_eq!(
            said(ToastMessage::PickerRecentsCleared),
            "Recently used emoji and colours cleared"
        );
        assert_eq!(
            said(ToastMessage::GroupNeedsList),
            "Open a list to group its items"
        );
        assert_eq!(
            said(ToastMessage::GroupColourFilter),
            "Clear the colour filter to group items"
        );
        assert_eq!(
            said(ToastMessage::GroupNothing),
            "Select the items to group first"
        );
        assert_eq!(
            said(ToastMessage::GroupTooDeep),
            "Groups nest three levels deep at most"
        );
        assert_eq!(
            said(ToastMessage::TagDeleted { name: s("work") }),
            "Deleted the tag “work”"
        );
        assert_eq!(
            said(ToastMessage::TagMerged {
                from: s("work"),
                into: s("Work")
            }),
            "Merged the tag “work” into “Work”"
        );
        assert_eq!(
            said(ToastMessage::SavedAgain {
                list: s("Notes"),
                was_in: s("Snippets")
            }),
            "Saved to Notes. The same text was already in Snippets."
        );
        assert_eq!(
            said(ToastMessage::UndoStopped),
            "That edit of a sensitive item cannot be undone: its text is not kept."
        );
        assert_eq!(
            said(ToastMessage::HistoryStepLost),
            "That step cannot be undone any more: what it changed was deleted for good."
        );
        assert_eq!(said(ToastMessage::FileMoved), "File moved or deleted");
        assert_eq!(
            said(ToastMessage::FilesTooLarge),
            "Some files were left out: together they are over 64 MB."
        );
        assert_eq!(
            said(ToastMessage::TriggerOverlaps {
                other: "Signature".into()
            }),
            "Saved. This trigger overlaps the trigger of “Signature”, so one of them may never fire."
        );
        assert_eq!(
            said(ToastMessage::TriggerOverlaps {
                other: String::new()
            }),
            "Saved. This trigger overlaps another trigger, so one of them may never fire.",
            "an untitled item is not named as “”"
        );
        assert_eq!(
            said(ToastMessage::TypingUnavailable),
            "Couldn’t start typing triggers. Nothing will be expanded or suggested."
        );
        assert_eq!(said(ToastMessage::DockSet(None)), "Panel floats again");
        assert_eq!(
            said(ToastMessage::DockSet(Some(DockEdge::Left))),
            "Panel docked to the left edge"
        );
        assert_eq!(
            said(ToastMessage::DockSet(Some(DockEdge::Bottom))),
            "Panel docked to the bottom edge"
        );
        assert_eq!(
            said(ToastMessage::PanelOnTop(false)),
            "Panel closes when you click away"
        );
        assert_eq!(
            said(ToastMessage::Imported { items: 0 }),
            "Nothing new to import."
        );
        let panel = said(ToastMessage::PanelHidden {
            chord: Some(s("Ctrl+Shift+Space")),
        });
        assert!(
            panel.contains("Press Ctrl+Shift+Space or click the tray icon"),
            "{panel}"
        );
        assert!(!said(ToastMessage::PanelHidden { chord: None }).contains("Press"));
        assert!(
            said(ToastMessage::HotkeyInUse {
                chord: s("Ctrl+Alt+Space")
            })
            .starts_with("Ctrl+Alt+Space")
        );
        assert_eq!(
            said(ToastMessage::HotkeysFailed {
                chords: s("Ctrl+Alt+Space, Ctrl+Alt+V")
            }),
            "These shortcuts are not working: Ctrl+Alt+Space, Ctrl+Alt+V. Settings > Shortcuts says why."
        );
    });
}

#[test]
fn a_failure_toast_names_its_cause_and_ends_with_what_to_do() {
    on_ui_thread(|| {
        let ui = ui();
        let said = |m: ToastMessage| toast_text(&ui, &m);
        assert_eq!(
            said(ToastMessage::DailyBackupFailed(Cause::Other)),
            "Couldn’t make today’s backup. Use Back up now in Settings to try again; if it keeps failing, open the data folder from Settings and check that it can be written to."
        );
        // DATA-35: an unknown cause never blames the disk.
        for m in [
            ToastMessage::DailyBackupFailed(Cause::Other),
            ToastMessage::BackupFailed(Cause::Other),
            ToastMessage::ExportFailed(Cause::Other),
            ToastMessage::TrashPurgeFailed,
        ] {
            assert!(!said(m).contains("disk has room"));
        }
        assert_eq!(
            said(ToastMessage::UseFailed(Cause::DiskFull)),
            "Couldn’t use this item: the disk is full. Free up some space, then try again."
        );
        let causes = [
            Cause::Other,
            Cause::DiskFull,
            Cause::InUse,
            Cause::Locked,
            Cause::NotArchive,
            Cause::TooNew,
            Cause::NotAllowed,
        ];
        let failures: [fn(Cause) -> ToastMessage; 7] = [
            ToastMessage::UseFailed,
            ToastMessage::ChangeFailed,
            ToastMessage::BackupFailed,
            ToastMessage::DailyBackupFailed,
            ToastMessage::RestoreFailed,
            ToastMessage::ImportFailed,
            ToastMessage::ExportFailed,
        ];
        for make in failures {
            let mut texts = Vec::new();
            for cause in causes {
                let text = said(make(cause));
                assert!(text.starts_with("Couldn’t "), "{text}");
                assert!(
                    text.ends_with('.') && !text.ends_with(".."),
                    "ends with the action: {text}"
                );
                texts.push(text);
            }
            // The unknown cause has its own plain sentence, and a cause the error names is
            // never reworded as the unknown one.
            assert_ne!(texts[1], texts[0], "{texts:?}");
            assert_ne!(texts[2], texts[0], "{texts:?}");
            assert_ne!(
                texts[6], texts[0],
                "a permission is not the unknown cause: {texts:?}"
            );
            assert_ne!(
                texts[6], texts[2],
                "a permission is not a file in use: {texts:?}"
            );
        }
        assert!(
            said(ToastMessage::ImportFailed(Cause::NotArchive)).contains("isn’t a file Snipdeck")
        );
        assert!(said(ToastMessage::ImportFailed(Cause::TooNew)).contains("newer Snipdeck"));
    });
}

#[test]
fn toast_words_follow_the_selected_translation() {
    on_ui_thread(|| {
        let ui = ui();
        let pasted = ToastMessage::Pasted { app: s("Outlook") };
        let copied = ToastMessage::Copied;
        assert_eq!(toast_text(&ui, &copied), "Copied");
        slint::select_bundled_translation("de").expect("de is bundled");
        assert_eq!(toast_text(&ui, &copied), "Kopiert");
        assert_eq!(toast_text(&ui, &pasted), "In Outlook eingefügt");
        slint::select_bundled_translation("").expect("back to the source language");
        assert_eq!(toast_text(&ui, &pasted), "Pasted into Outlook");
    });
}

#[test]
fn a_start_up_failure_names_its_reason_and_the_way_back() {
    on_ui_thread(|| {
        let ui = ui();
        let text = snipdeck_app::wording::startup_failure_text(&ui, "C:\\Data: database is locked");
        assert!(text.contains("C:\\Data: database is locked"), "{text}");
        assert!(text.contains("backups folder"), "{text}");
        assert!(!text.contains("{}"), "{text}");
    });
}

/// TOAST-1 and TOAST-2: the button of every kind of toast is worded in the language shown.
#[test]
fn the_toast_button_follows_the_selected_translation() {
    use snipdeck_app::controller::{ToastPhase, ToastView};
    use snipdeck_app::toast::ToastKind;
    on_ui_thread(|| {
        let ui = ui();
        let view = |message: ToastMessage, undo: bool, open_log: bool| ToastView {
            seq: 1,
            kind: ToastKind::Neutral,
            message,
            phase: ToastPhase::Hold,
            undo,
            edit: false,
            purge: false,
            open_log,
        };
        let undo = view(ToastMessage::Copied, true, false);
        let log = view(ToastMessage::CrashedLastTime, false, true);
        let switch = view(
            ToastMessage::SwitchToManual {
                list: snipdeck_core::ListId::generate(),
            },
            false,
            false,
        );
        let none = view(ToastMessage::Copied, false, false);
        let words = |ui: &PanelWindow| {
            [&undo, &log, &switch, &none].map(|v| snipdeck_app::wording::toast_button_text(ui, v))
        };
        assert_eq!(words(&ui), ["Undo", "Open log", "Switch", ""]);
        slint::select_bundled_translation("de").expect("de is bundled");
        let german = words(&ui);
        slint::select_bundled_translation("").expect("back to the source language");
        assert_ne!(german[0], "Undo");
        assert_ne!(german[1], "Open log");
        assert_ne!(german[2], "Switch");
        assert_eq!(german[3], "");
    });
}
