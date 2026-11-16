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
        ToastMessage::ArchiveBusy => 112,
        ToastMessage::StoreDataReminder => 113,
        ToastMessage::HotkeyOwnAction { .. } => 114,
        ToastMessage::HotkeyRefused { .. } => 115,
    }
}

const VARIANTS: usize = 116;

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
        M::ClipboardWatchFailed,
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
            "Copied. That app runs as administrator, so Snipdeck can't paste into it."
        );
        assert_eq!(
            said(ToastMessage::ItemsTrashed { count: 1 }),
            "Moved 1 item to the Trash"
        );
        assert_eq!(
            said(ToastMessage::ItemsTrashed { count: 3 }),
            "Moved 3 items to the Trash"
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
            "Couldn't start typing triggers. Nothing will be expanded or suggested."
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
            "Couldn't make today's backup. Check the disk has room, then use Back up now in Settings."
        );
        assert_eq!(
            said(ToastMessage::UseFailed(Cause::DiskFull)),
            "Couldn't use this item: the disk is full. Free up some space, then try again."
        );
        let causes = [
            Cause::Other,
            Cause::DiskFull,
            Cause::InUse,
            Cause::Locked,
            Cause::NotArchive,
            Cause::TooNew,
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
                assert!(text.starts_with("Couldn't "), "{text}");
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
        }
        assert!(
            said(ToastMessage::ImportFailed(Cause::NotArchive)).contains("isn't a file Snipdeck")
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
