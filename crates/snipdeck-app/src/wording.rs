//! The words Rust puts on screen: the English text of the toasts, and the tray menu's labels.
//!
//! The Panel's own strings are `@tr` in Slint and translate with the bundled catalogs. Toast
//! text is built in Rust from [`ToastMessage`] variants, and Rust cannot reach those catalogs,
//! so it is literal English until the UI crate exposes a translated `ToastTexts` global (listed
//! in `docs/HANDOVER.md`). The tray menu already does: its words are the UI crate's `TrayTexts`
//! global, read with [`tray_labels`].

use slint::Global;
use snipdeck_platform::tray::TrayLabels;
use snipdeck_ui::{PanelWindow, TrayTexts};

use crate::toast::{LicenceProblem, ToastMessage};

/// The tray menu's words in the language selected for the UI now (English until a bundled
/// translation is selected). `window` is any window of the UI: Slint globals are reached
/// through a component.
#[must_use]
pub fn tray_labels(window: &PanelWindow) -> TrayLabels {
    let texts = TrayTexts::get(window);
    TrayLabels {
        open_panel: texts.get_open_panel().into(),
        open_snipdeck: texts.get_open_snipdeck().into(),
        pause_hotkeys: texts.get_pause_hotkeys().into(),
        quit: texts.get_quit().into(),
    }
}

/// The words for `message`.
#[must_use]
pub fn toast_text(message: &ToastMessage) -> String {
    let app = |name: &str| {
        if name.is_empty() {
            "That app".to_owned()
        } else {
            name.to_owned()
        }
    };
    match message {
        ToastMessage::Pasted { app } if app.is_empty() => "Pasted".into(),
        ToastMessage::Pasted { app } => format!("Pasted into {app}"),
        ToastMessage::Copied => "Copied".into(),
        ToastMessage::CopiedElevated { app: name } => format!(
            "Copied. {} runs as administrator, so Snipdeck can't paste into it.",
            app(name)
        ),
        ToastMessage::CopiedNothingFocused { app: name } => {
            format!("Copied. Nothing in {} can take text right now.", app(name))
        }
        ToastMessage::CopiedExcluded { app: name } => {
            format!("Copied. Pasting into {} is turned off.", app(name))
        }
        ToastMessage::CopiedTargetGone => "Copied. The window you were using went away.".into(),
        ToastMessage::ClipboardBusy => {
            "Couldn't reach the clipboard. Another app may be using it.".into()
        }
        ToastMessage::NeedsPro(_) => "This item uses a Pro feature.".into(),
        ToastMessage::TemplateBroken => "This template can't be read. Open it to fix it.".into(),
        ToastMessage::Locked => "Unlock your library to use this item.".into(),
        ToastMessage::Saved { list } => format!("Saved to {list}"),
        ToastMessage::NothingToSave => "Nothing to save. Copy some text first.".into(),
        ToastMessage::TooLargeToSave => "That text is too large to save.".into(),
        ToastMessage::NoListToSave => "There is no list to save into. Add a list first.".into(),
        ToastMessage::NothingToPasteNext => {
            "Nothing to paste next. Pin a snippet, or add one to this list.".into()
        }
        ToastMessage::PasteNextFinished => {
            "That was the last one. Paste next starts again from the first.".into()
        }
        ToastMessage::HotkeyInUse { chord } => {
            format!("{chord} is used by another app. Choose another in Settings.")
        }
        ToastMessage::Failed => "Couldn't use this item.".into(),
        ToastMessage::SettingsNotSaved => {
            "Couldn't save your settings. Snipdeck will try again.".into()
        }
        ToastMessage::ItemsTrashed { count: 1 } => "Moved 1 item to the Trash".into(),
        ToastMessage::ItemsTrashed { count } => format!("Moved {count} items to the Trash"),
        ToastMessage::ListTrashed { name } => format!("Deleted list \u{201c}{name}\u{201d}"),
        ToastMessage::BoardTrashed { name } => format!("Deleted board \u{201c}{name}\u{201d}"),
        ToastMessage::Restored { count: 1 } => "Restored 1 item".into(),
        ToastMessage::Restored { count } => format!("Restored {count} items"),
        ToastMessage::ItemsMoved { count: 1, list } => format!("Moved 1 item to {list}"),
        ToastMessage::ItemsMoved { count, list } => format!("Moved {count} items to {list}"),
        ToastMessage::DeletedForever { .. } => "Deleted for good".into(),
        ToastMessage::TrashEmptied => "The Trash is empty".into(),
        ToastMessage::BoardLimit => {
            "The Free plan includes 3 boards. Upgrade to Pro for more.".into()
        }
        ToastMessage::LibraryProblem => {
            "Snipdeck found a problem in your library. Export a copy to be safe.".into()
        }
        ToastMessage::CrashedLastTime => {
            "Snipdeck closed unexpectedly last time. Open the log?".into()
        }
        ToastMessage::LicenceAccepted => "Snipdeck Pro is on. Thank you.".into(),
        ToastMessage::LicenceRejected(problem) => licence_problem_text(*problem).into(),
        ToastMessage::BackupMade => "Backed up".into(),
        ToastMessage::BackupFailed => "Couldn't make a backup.".into(),
        ToastMessage::BackupRestored { items: 0 } => {
            "Nothing to restore: your library already has everything in that backup.".into()
        }
        ToastMessage::BackupRestored { items: 1 } => "Restored 1 item from the backup".into(),
        ToastMessage::BackupRestored { items } => format!("Restored {items} items from the backup"),
        ToastMessage::RestoreFailed => "Couldn't restore that backup.".into(),
        ToastMessage::Exported { file } => format!("Saved {file} in your Documents folder"),
        ToastMessage::ExportFailed => "Couldn't export your library.".into(),
        ToastMessage::LibraryErased => "Your library was erased.".into(),
        ToastMessage::EraseFailed => {
            "Couldn't erase everything. Quit Snipdeck and try again.".into()
        }
        ToastMessage::FolderNotOpened => "Couldn't open the folder.".into(),
        ToastMessage::AutostartFailed => "Couldn't change the Windows start-up entry.".into(),
    }
}

/// Why a licence key was refused, in words (the toast and the line under the key field).
#[must_use]
pub const fn licence_problem_text(problem: LicenceProblem) -> &'static str {
    match problem {
        LicenceProblem::Empty => "Paste your licence key first.",
        LicenceProblem::NotAKey => "That is not a Snipdeck licence key. A key starts with SNIP1.",
        LicenceProblem::BadSignature => {
            "That licence key is not valid. Check that you copied all of it."
        }
        LicenceProblem::NewerFormat => "That key needs a newer Snipdeck. Update and try again.",
        LicenceProblem::WrongTier => "That key is not for Snipdeck Pro.",
        LicenceProblem::NotSaved => {
            "The key is valid but couldn't be saved, so Pro will be off next time."
        }
    }
}

/// The names of the interface languages Settings offers, each in its own language, after
/// "Follow Windows". The tags are in [`LANGUAGE_TAGS`], in the same order.
pub const LANGUAGE_NAMES: [&str; 7] = [
    "English",
    "Espa\u{f1}ol",
    "Deutsch",
    "Fran\u{e7}ais",
    "Portugu\u{ea}s (Brasil)",
    "\u{65e5}\u{672c}\u{8a9e}",
    "\u{7b80}\u{4f53}\u{4e2d}\u{6587}",
];

/// The locale tags of [`LANGUAGE_NAMES`] (`settings.language`).
pub const LANGUAGE_TAGS: [&str; 7] = ["en", "es", "de", "fr", "pt-BR", "ja", "zh-Hans"];

/// The label of a toast's undo button.
pub const UNDO_LABEL: &str = "Undo";

/// The Main window's item-use button: "Use" where a click pastes, "Copy" where it cannot.
#[must_use]
pub const fn use_label_text(label: crate::panel::UseLabel) -> &'static str {
    match label {
        crate::panel::UseLabel::Paste => "Use",
        crate::panel::UseLabel::Copy => "Copy",
    }
}

/// The words of a row of the Panel's kebab menu.
#[must_use]
pub const fn app_menu_text(entry: crate::controller::AppMenuEntry) -> &'static str {
    use crate::controller::AppMenuEntry;
    match entry {
        AppMenuEntry::Open => "Open Snipdeck",
        AppMenuEntry::NewSnippet => "New snippet",
        AppMenuEntry::Settings => "Settings\u{2026}",
        AppMenuEntry::PauseHotkeys => "Pause hotkeys",
        AppMenuEntry::Separator => "",
        AppMenuEntry::Troubleshoot => "Troubleshoot\u{2026}",
        AppMenuEntry::UserGuide => "User guide",
        AppMenuEntry::About => "About Snipdeck",
        AppMenuEntry::Quit => "Quit",
    }
}

/// The words of a board menu entry.
#[must_use]
pub fn board_menu_text(entry: &crate::controller::BoardMenuEntry) -> String {
    use crate::controller::BoardMenuEntry;
    match entry {
        BoardMenuEntry::Board { name, .. } => name.clone(),
        BoardMenuEntry::Separator => String::new(),
        BoardMenuEntry::New => "New board\u{2026}".into(),
        BoardMenuEntry::Rename => "Rename board\u{2026}".into(),
        BoardMenuEntry::MoveUp { .. } => "Move board up".into(),
        BoardMenuEntry::MoveDown { .. } => "Move board down".into(),
        BoardMenuEntry::Delete => "Delete board\u{2026}".into(),
    }
}

/// Why an editor refuses a name, in words.
#[must_use]
pub fn name_error_text(error: &crate::main_window::NameError) -> String {
    use crate::main_window::NameError;
    match error {
        NameError::Empty => "Give it a name.".into(),
        NameError::TooLong { max } => format!("Names can have at most {max} characters."),
        NameError::ControlChar => "Names can't contain line breaks or control characters.".into(),
    }
}

/// The reason a board cannot be deleted, in words.
#[must_use]
pub fn delete_hint_text(hint: crate::main_window::DeleteHint) -> String {
    match hint {
        crate::main_window::DeleteHint::LastBoard => {
            "This is your only board. Add another before deleting it.".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paste_names_the_app_and_an_anonymous_one_still_reads_well() {
        assert_eq!(
            toast_text(&ToastMessage::Pasted {
                app: "Outlook".into()
            }),
            "Pasted into Outlook"
        );
        assert_eq!(
            toast_text(&ToastMessage::Pasted { app: String::new() }),
            "Pasted"
        );
        assert_eq!(
            toast_text(&ToastMessage::CopiedElevated { app: String::new() }),
            "Copied. That app runs as administrator, so Snipdeck can't paste into it."
        );
    }

    #[test]
    fn every_message_has_text() {
        use snipdeck_core::Feature;
        let all = [
            ToastMessage::Copied,
            ToastMessage::CopiedTargetGone,
            ToastMessage::ClipboardBusy,
            ToastMessage::NeedsPro(Feature::PromptedFields),
            ToastMessage::TemplateBroken,
            ToastMessage::Locked,
            ToastMessage::Saved {
                list: "Notes".into(),
            },
            ToastMessage::NothingToSave,
            ToastMessage::TooLargeToSave,
            ToastMessage::NoListToSave,
            ToastMessage::NothingToPasteNext,
            ToastMessage::PasteNextFinished,
            ToastMessage::HotkeyInUse {
                chord: "Ctrl+Alt+Space".into(),
            },
            ToastMessage::Failed,
            ToastMessage::SettingsNotSaved,
            ToastMessage::ItemsTrashed { count: 1 },
            ToastMessage::ItemsTrashed { count: 3 },
            ToastMessage::ListTrashed {
                name: "Code".into(),
            },
            ToastMessage::BoardTrashed {
                name: "Work".into(),
            },
            ToastMessage::Restored { count: 2 },
            ToastMessage::ItemsMoved {
                count: 2,
                list: "Code".into(),
            },
            ToastMessage::DeletedForever { count: 1 },
            ToastMessage::TrashEmptied,
            ToastMessage::BoardLimit,
            ToastMessage::LibraryProblem,
            ToastMessage::CrashedLastTime,
        ];
        for message in all {
            assert!(!toast_text(&message).is_empty(), "{message:?}");
        }
        assert!(
            toast_text(&ToastMessage::HotkeyInUse {
                chord: "Ctrl+Alt+Space".into()
            })
            .starts_with("Ctrl+Alt+Space")
        );
    }
}
