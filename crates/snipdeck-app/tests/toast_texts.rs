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
    toast::{LicenceProblem, ToastMessage},
    wording::{licence_problem_text, toast_text},
};
use snipdeck_core::Feature;
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

/// The position of `message`'s variant in [`samples`]. The match has no wildcard, so a new
/// `ToastMessage` variant stops this file compiling until it is listed (and so tested).
const fn position(message: &ToastMessage) -> usize {
    match message {
        ToastMessage::Pasted { .. } => 0,
        ToastMessage::Copied => 1,
        ToastMessage::CopiedElevated { .. } => 2,
        ToastMessage::CopiedNothingFocused { .. } => 3,
        ToastMessage::CopiedExcluded { .. } => 4,
        ToastMessage::CopiedTargetGone => 5,
        ToastMessage::ClipboardBusy => 6,
        ToastMessage::NeedsPro(_) => 7,
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
        ToastMessage::Saved { .. } => 18,
        ToastMessage::NothingToSave => 19,
        ToastMessage::TooLargeToSave => 20,
        ToastMessage::NoListToSave => 21,
        ToastMessage::NothingToPasteNext => 22,
        ToastMessage::PasteNextFinished => 23,
        ToastMessage::HotkeyInUse { .. } => 24,
        ToastMessage::HotkeyInvalid { .. } => 25,
        ToastMessage::HotkeyUnavailable { .. } => 26,
        ToastMessage::PanelHidden { .. } => 27,
        ToastMessage::Failed => 28,
        ToastMessage::WindowNotShown => 29,
        ToastMessage::SettingsNotSaved => 30,
        ToastMessage::ItemsTrashed { .. } => 31,
        ToastMessage::ListTrashed { .. } => 32,
        ToastMessage::BoardTrashed { .. } => 33,
        ToastMessage::Restored { .. } => 34,
        ToastMessage::ItemsMoved { .. } => 35,
        ToastMessage::DeletedForever { .. } => 36,
        ToastMessage::TrashEmptied => 37,
        ToastMessage::BoardLimit => 38,
        ToastMessage::LibraryProblem => 39,
        ToastMessage::CrashedLastTime => 40,
        ToastMessage::LicenceAccepted => 41,
        ToastMessage::LicenceRejected(_) => 42,
        ToastMessage::BackupMade => 43,
        ToastMessage::BackupFailed(_) => 44,
        ToastMessage::BackupRestored { .. } => 45,
        ToastMessage::RestoreFailed(_) => 46,
        ToastMessage::Imported { .. } => 47,
        ToastMessage::ImportFailed(_) => 48,
        ToastMessage::Exported { .. } => 49,
        ToastMessage::ExportFailed(_) => 50,
        ToastMessage::LibraryErased => 51,
        ToastMessage::EraseFailed => 52,
        ToastMessage::FolderNotOpened => 53,
        ToastMessage::MailNotOpened => 54,
        ToastMessage::AutostartFailed => 55,
        ToastMessage::CannotOpenLog => 56,
        ToastMessage::KeyUnavailable => 57,
        ToastMessage::ItemGone => 58,
        ToastMessage::ListGone => 59,
        ToastMessage::StorageFailed => 60,
        ToastMessage::NotSaved => 61,
        ToastMessage::NotOpened => 62,
        ToastMessage::DraftNotSaved => 63,
        ToastMessage::ItemPlaced { .. } => 64,
        ToastMessage::TaskbarEdge => 65,
        ToastMessage::UseFailed(_) => 66,
        ToastMessage::DailyBackupFailed(_) => 67,
        ToastMessage::TrashPurgeFailed => 68,
        ToastMessage::ChangeFailed(_) => 69,
        ToastMessage::ScreenshotFailed => 70,
        ToastMessage::Undone { .. } => 71,
        ToastMessage::Redone { .. } => 72,
        ToastMessage::DatabaseRebuilt { .. } => 73,
        ToastMessage::RebuildNeedsRestart => 74,
        ToastMessage::ImageNotPasteable => 75,
        ToastMessage::HotkeysFailed { .. } => 76,
        ToastMessage::ThemeSet(_) => 77,
        ToastMessage::PanelOnTop(_) => 78,
        ToastMessage::Pinned(_) => 79,
        ToastMessage::Duplicated => 80,
        ToastMessage::ListAdded => 81,
        ToastMessage::ListSaved => 82,
        ToastMessage::BoardSaved => 83,
        ToastMessage::RecentDeleted => 84,
        ToastMessage::RecentCleared => 85,
        ToastMessage::DockSet(_) => 86,
        ToastMessage::NotRefreshed => 87,
    }
}

const VARIANTS: usize = 88;

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
        M::Copied,
        M::CopiedElevated { app: s("Outlook") },
        M::CopiedNothingFocused { app: s("Outlook") },
        M::CopiedExcluded { app: s("Outlook") },
        M::CopiedTargetGone,
        M::ClipboardBusy,
        M::NeedsPro(Feature::PromptedFields),
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
        M::NeedsPro(Feature::PasteStack),
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
        M::BoardLimit,
        M::LibraryProblem,
        M::CrashedLastTime,
        M::LicenceAccepted,
        M::LicenceRejected(LicenceProblem::BadSignature),
        M::BackupMade,
        M::BackupFailed(Cause::Other),
        M::BackupRestored { items: 5 },
        M::RestoreFailed(Cause::NotArchive),
        M::DatabaseRebuilt {
            restored: 5,
            recovered: 2,
            kept: s("snipdeck.damaged-20261017-1030.db"),
            unreadable: false,
            missing_sensitive: 1,
        },
        M::RebuildNeedsRestart,
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
        M::MailNotOpened,
        M::AutostartFailed,
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
        M::DockSet(None),
        M::DockSet(Some(DockEdge::Left)),
        M::DockSet(Some(DockEdge::Right)),
        M::DockSet(Some(DockEdge::Top)),
        M::DockSet(Some(DockEdge::Bottom)),
        M::NotRefreshed,
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
            kept: s("old.db"),
            unreadable,
            missing_sensitive,
        };
        assert_eq!(
            said(rebuilt(2, false, 0)),
            "Database rebuilt: 5 items restored, 2 recovered. Your old database is kept as old.db."
        );
        assert_eq!(
            said(rebuilt(0, true, 1)),
            "Database rebuilt: 5 items restored. Newer items could not be read. Your old database is kept as old.db. 1 sensitive item was not in the backup."
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
        assert_eq!(said(ToastMessage::Pinned(true)), "Pinned");
        assert_eq!(said(ToastMessage::Pinned(false)), "Unpinned");
        assert_eq!(said(ToastMessage::Duplicated), "Duplicated");
        assert_eq!(said(ToastMessage::ListAdded), "List added");
        assert_eq!(said(ToastMessage::ListSaved), "List saved");
        assert_eq!(said(ToastMessage::BoardSaved), "Board saved");
        assert_eq!(said(ToastMessage::RecentDeleted), "Removed from Recent");
        assert_eq!(said(ToastMessage::RecentCleared), "Recent history cleared");
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
        assert_eq!(
            said(ToastMessage::NeedsPro(Feature::PasteStack)),
            "Paste next and the paste stack are Supporter features."
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
fn a_licence_problem_reads_the_same_as_a_toast_and_under_the_key_field() {
    on_ui_thread(|| {
        let ui = ui();
        for problem in [
            LicenceProblem::Empty,
            LicenceProblem::NotAKey,
            LicenceProblem::BadSignature,
            LicenceProblem::NewerFormat,
            LicenceProblem::WrongTier,
            LicenceProblem::NotSaved,
        ] {
            let words = licence_problem_text(&ui, problem);
            assert!(!words.is_empty(), "{problem:?}");
            assert_eq!(
                toast_text(&ui, &ToastMessage::LicenceRejected(problem)),
                words,
                "{problem:?}"
            );
        }
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
