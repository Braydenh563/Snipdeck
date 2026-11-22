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
        ToastMessage::PastedNotRestored { .. } => 107,
        ToastMessage::Copied => 1,
        ToastMessage::CopiedElevated { .. } => 2,
        ToastMessage::CopiedNothingFocused { .. } => 3,
        ToastMessage::CopiedExcluded { .. } => 4,
        ToastMessage::CopiedTargetGone => 5,
        ToastMessage::ClipboardBusy => 6,
        ToastMessage::TemplateBroken => 7,
        ToastMessage::Locked => 8,
        ToastMessage::Unlocked => 9,
        ToastMessage::MasterPasswordSet => 10,
        ToastMessage::MasterPasswordChanged => 11,
        ToastMessage::MasterPasswordRemoved => 12,
        ToastMessage::MasterPasswordChangedNotOpen => 13,
        ToastMessage::MasterPasswordRemovedNotOpen => 14,
        ToastMessage::UnlockFirst => 15,
        ToastMessage::LockedEditorClosed => 16,
        ToastMessage::LockedFillClosed => 106,
        ToastMessage::LockedEditorKept => 119,
        ToastMessage::Saved { .. } => 17,
        ToastMessage::NothingToSave => 18,
        ToastMessage::TooLargeToSave => 19,
        ToastMessage::NoListToSave => 20,
        ToastMessage::NothingToPasteNext => 21,
        ToastMessage::PasteNextFinished => 22,
        ToastMessage::HotkeyInUse { .. } => 23,
        ToastMessage::HotkeyInvalid { .. } => 24,
        ToastMessage::HotkeyUnavailable { .. } => 25,
        ToastMessage::PanelHidden { .. } => 26,
        ToastMessage::Failed => 27,
        ToastMessage::WindowNotShown => 28,
        ToastMessage::SettingsNotSaved => 29,
        ToastMessage::SettingsDamaged => 108,
        ToastMessage::SettingsUnreadable => 109,
        ToastMessage::ClipboardWatchFailed => 110,
        ToastMessage::ItemsTrashed { .. } => 30,
        ToastMessage::ListTrashed { .. } => 31,
        ToastMessage::BoardTrashed { .. } => 32,
        ToastMessage::Restored { .. } => 33,
        ToastMessage::ItemsMoved { .. } => 34,
        ToastMessage::DeletedForever { .. } => 35,
        ToastMessage::TrashEmptied => 36,
        ToastMessage::LibraryProblem => 37,
        ToastMessage::CrashedLastTime => 38,
        ToastMessage::BackupMade => 39,
        ToastMessage::BackupFailed(_) => 40,
        ToastMessage::BackupRestored { .. } => 41,
        ToastMessage::RestoreFailed(_) => 42,
        ToastMessage::Imported { .. } => 43,
        ToastMessage::ImportFailed(_) => 44,
        ToastMessage::Exported { .. } => 45,
        ToastMessage::ExportFailed(_) => 46,
        ToastMessage::LibraryErased => 47,
        ToastMessage::EraseFailed => 48,
        ToastMessage::FolderNotOpened => 49,
        ToastMessage::LinkNotOpened => 50,
        ToastMessage::AutostartFailed => 51,
        ToastMessage::CannotOpenLog => 52,
        ToastMessage::KeyUnavailable => 53,
        ToastMessage::ItemGone => 54,
        ToastMessage::ListGone => 55,
        ToastMessage::StorageFailed => 56,
        ToastMessage::NotSaved => 57,
        ToastMessage::NotOpened => 58,
        ToastMessage::DraftNotSaved => 59,
        ToastMessage::ItemPlaced { .. } => 60,
        ToastMessage::TaskbarEdge => 61,
        ToastMessage::UseFailed(_) => 62,
        ToastMessage::DailyBackupFailed(_) => 63,
        ToastMessage::TrashPurgeFailed => 64,
        ToastMessage::ChangeFailed(_) => 65,
        ToastMessage::ScreenshotFailed => 66,
        ToastMessage::Undone { .. } => 67,
        ToastMessage::Redone { .. } => 68,
        ToastMessage::DatabaseRebuilt { .. } => 69,
        ToastMessage::RebuildNeedsRestart => 70,
        ToastMessage::RebuildUnfinished => 71,
        ToastMessage::ImageNotPasteable => 72,
        ToastMessage::HotkeysFailed { .. } => 73,
        ToastMessage::ThemeSet(_) => 74,
        ToastMessage::PanelOnTop(_) => 75,
        ToastMessage::Pinned(_) => 76,
        ToastMessage::Duplicated => 77,
        ToastMessage::ListAdded => 78,
        ToastMessage::ListSaved => 79,
        ToastMessage::BoardSaved => 80,
        ToastMessage::RecentDeleted => 81,
        ToastMessage::RecentCleared => 82,
        ToastMessage::PickerRecentsCleared => 111,
        ToastMessage::DockSet(_) => 83,
        ToastMessage::NotRefreshed => 84,
        ToastMessage::IconNotAddable => 85,
        ToastMessage::IconSaved { .. } => 86,
        ToastMessage::EyedropFailed => 104,
        ToastMessage::EyedropHdr { .. } => 105,
        ToastMessage::AddedTo { .. } => 87,
        ToastMessage::LooksSecret => 88,
        ToastMessage::PasteNotAddedEditorOpen => 89,
        ToastMessage::AddedSomeFiles { .. } => 90,
        ToastMessage::FilesNotAdded => 91,
        ToastMessage::DropTo { .. } => 92,
        ToastMessage::TooManyFiles => 93,
        ToastMessage::SwitchToManual { .. } => 94,
        ToastMessage::FileMoved => 95,
        ToastMessage::FilesTooLarge => 96,
        ToastMessage::HelloTurnedOff => 97,
        ToastMessage::HelloNotTurnedOn => 98,
        ToastMessage::HelloEnrolFailed => 99,
        ToastMessage::TriggerOverlaps { .. } => 100,
        ToastMessage::TypingUnavailable => 101,
        ToastMessage::AutostartSwitchedOff => 102,
        ToastMessage::AutostartPolicy => 103,
        ToastMessage::AlreadySaved { .. } => 112,
        ToastMessage::NoColourOnClipboard => 113,
        ToastMessage::EyedropHdrCopied { .. } => 114,
        ToastMessage::EyedropRefused => 115,
        ToastMessage::EyedropEnded { .. } => 116,
        ToastMessage::EyedropKeysBusy { .. } => 117,
        ToastMessage::NameNotExact { .. } => 118,
        ToastMessage::ArchiveBusy => 120,
        ToastMessage::StoreDataReminder => 121,
        ToastMessage::HotkeyOwnAction { .. } => 122,
        ToastMessage::HotkeyRefused { .. } => 123,
        ToastMessage::BackupMadeSkipping { .. } => 124,
        ToastMessage::BackupShrunk { .. } => 125,
        ToastMessage::ExportedSkipping { .. } => 126,
        ToastMessage::LibrarySetAside { .. } => 127,
        ToastMessage::ExportedSome { .. } => 128,
        ToastMessage::ItemsTagged { .. } => 129,
        ToastMessage::TagDeleted { .. } => 130,
        ToastMessage::TagMerged { .. } => 131,
    }
}

const VARIANTS: usize = 132;

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
        M::NothingToSave,
        M::TooLargeToSave,
        M::NoListToSave,
        M::NothingToPasteNext,
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

/// DESIGN.md "Names": a toast of one clause has no full stop; one of several sentences ends
/// every sentence with one, the last included. Nothing ends in an exclamation mark.
#[test]
fn toast_punctuation_follows_the_one_clause_rule() {
    on_ui_thread(|| {
        let ui = ui();
        let mut bad: Vec<String> = Vec::new();
        for message in samples() {
            let text = toast_text(&ui, &message);
            let text = text.trim();
            let sentences = text.matches(". ").count() + text.matches("? ").count();
            let ends_stop = text.ends_with('.');
            if text.ends_with('!')
                || (sentences == 0 && ends_stop)
                || (sentences > 0 && !ends_stop && !text.ends_with('?'))
            {
                bad.push(format!("{message:?}: {text}"));
            }
        }
        assert_eq!(bad.len(), 0, "{bad:#?}");
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
            "Library rebuilt: 5 items restored, 2 recovered. Your old library is kept in the folder old."
        );
        assert_eq!(
            said(rebuilt(0, true, 1)),
            "Library rebuilt: 5 items restored. Newer items could not be read. Your old library is kept in the folder old. 1 sensitive item was not in the backup."
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
            "Nothing to restore: your library already has everything in that backup"
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
        assert_eq!(said(ToastMessage::RecentCleared), "Recent cleared");
        assert_eq!(
            said(ToastMessage::PickerRecentsCleared),
            "Recently used emoji and colours cleared"
        );
        assert_eq!(said(ToastMessage::FileMoved), "File moved or deleted");
        assert_eq!(
            said(ToastMessage::FilesTooLarge),
            "Some files were left out: together they are over 64 MB"
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
            "Nothing new to import"
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
