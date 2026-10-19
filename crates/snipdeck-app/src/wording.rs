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

use snipdeck_core::ItemKind;

use crate::{
    item_editor::{AttachmentView, EditorProblem, SaveFailure},
    toast::{LicenceProblem, ToastMessage},
};

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
        ToastMessage::PanelHidden { chord: Some(chord) } => format!(
            "Snipdeck is still running. Press {chord} or click the tray icon to bring the panel back."
        ),
        ToastMessage::PanelHidden { chord: None } => {
            "Snipdeck is still running. Click the tray icon to bring the panel back.".into()
        }
        ToastMessage::Failed => "Couldn't use this item.".into(),
        ToastMessage::WindowNotShown => "Couldn't open that window.".into(),
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
        ToastMessage::CannotOpenLog => "Couldn't open the log folder.".into(),
        ToastMessage::KeyUnavailable => {
            "Couldn't open the encryption key. Nothing was saved.".into()
        }
        ToastMessage::ItemGone => "This item was deleted. Nothing was saved.".into(),
        ToastMessage::ListGone => "That list was deleted. Choose another list.".into(),
        ToastMessage::StorageFailed => {
            "Couldn't write to your library. Check that the disk has room.".into()
        }
        ToastMessage::NotSaved => "Couldn't save this item. Nothing was changed.".into(),
        ToastMessage::NotOpened => "Couldn't open this item.".into(),
        ToastMessage::DraftNotSaved => "Couldn't save a draft of what you typed.".into(),
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

/// The label of the crash toast's button.
pub const OPEN_LOG_LABEL: &str = "Open log";

/// The label of `toast`'s button; empty when it has none.
#[must_use]
pub const fn toast_button_text(toast: &crate::controller::ToastView) -> &'static str {
    if toast.undo {
        UNDO_LABEL
    } else if toast.open_log {
        OPEN_LOG_LABEL
    } else {
        ""
    }
}

/// The Main window's item-use button: "Use" where a click pastes, "Copy" where it cannot.
#[must_use]
pub const fn use_label_text(label: crate::panel::UseLabel) -> &'static str {
    match label {
        crate::panel::UseLabel::Paste => "Use",
        crate::panel::UseLabel::Copy => "Copy",
    }
}

/// The kebab's dock row while the Panel is docked.
pub const UNDOCK_TEXT: &str = "Undock the panel";

/// The words of a row of the Panel's kebab menu.
#[must_use]
pub const fn app_menu_text(entry: crate::controller::AppMenuEntry) -> &'static str {
    use crate::controller::AppMenuEntry;
    match entry {
        AppMenuEntry::Open => "Open Snipdeck",
        AppMenuEntry::NewSnippet => "New snippet",
        AppMenuEntry::Settings => "Settings\u{2026}",
        AppMenuEntry::Dock => "Dock to the nearest edge",
        AppMenuEntry::PauseHotkeys => "Pause hotkeys",
        AppMenuEntry::Separator => "",
        AppMenuEntry::Troubleshoot => "Troubleshoot\u{2026}",
        AppMenuEntry::UserGuide => "User guide",
        AppMenuEntry::About => "About Snipdeck",
        AppMenuEntry::Quit => "Quit",
    }
}

/// The words of a row of the Panel's item menu.
#[must_use]
pub const fn item_menu_text(entry: crate::controller::ItemMenuEntry) -> &'static str {
    use crate::controller::ItemMenuEntry;
    match entry {
        ItemMenuEntry::Use => "Use",
        ItemMenuEntry::Copy => "Copy",
        ItemMenuEntry::Edit => "Edit\u{2026}",
        ItemMenuEntry::Pin { pinned: false } => "Pin",
        ItemMenuEntry::Pin { pinned: true } => "Unpin",
        ItemMenuEntry::Trash => "Move to Trash",
        ItemMenuEntry::Separator => "",
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

/// Why the Tag manager refuses a new tag name, in words.
#[must_use]
pub fn tag_rename_error_text(error: &crate::controller::TagRenameError) -> String {
    use crate::controller::TagRenameError;
    match error {
        TagRenameError::Name(name) => name_error_text(name),
        TagRenameError::Exists => "A tag with that name already exists.".into(),
        TagRenameError::Failed => "Couldn't rename the tag.".into(),
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

/// A size for people: "800 bytes", "12 KB", "2.4 MB".
#[must_use]
pub fn size_text(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes < KB {
        format!("{bytes} bytes")
    } else if bytes < MB {
        format!("{} KB", bytes / KB)
    } else {
        let tenths = bytes.saturating_mul(10) / MB;
        format!("{}.{} MB", tenths / 10, tenths % 10)
    }
}

/// What the item editor shows as the name of an existing Image or File item.
#[must_use]
pub fn attachment_name_text(attachment: &AttachmentView) -> String {
    match attachment.kind {
        ItemKind::Image => "Picture".to_owned(),
        _ if attachment.files > 1 => {
            format!("{} and {} more", attachment.name, attachment.files - 1)
        }
        _ => attachment.name.clone(),
    }
}

/// What the item editor shows under the name of an existing Image or File item.
#[must_use]
pub fn attachment_detail_text(attachment: &AttachmentView) -> String {
    let size = size_text(attachment.bytes);
    match (attachment.kind, attachment.dimensions) {
        (ItemKind::Image, Some((w, h))) => format!("PNG, {w} \u{d7} {h}, {size}"),
        (ItemKind::Image, None) => format!("PNG, {size}"),
        _ if attachment.files > 1 => format!("{} files, {size} in all", attachment.files),
        _ => size,
    }
}

/// Why the item editor refuses something, in words.
#[must_use]
pub fn editor_problem_text(problem: &EditorProblem) -> String {
    match problem {
        EditorProblem::TitleTooLong { max } => {
            format!("A title can have at most {max} characters.")
        }
        EditorProblem::TitleHasControl => {
            "A title can't contain line breaks or control characters.".into()
        }
        EditorProblem::ContentEmpty => "Add some text, or cancel.".into(),
        EditorProblem::ContentTooLong { max_kb } => format!(
            "This text is over {max_kb} KB, the most that can be edited here. Shorten it, or keep it in a file."
        ),
        EditorProblem::LinkNeedsScheme => "A link needs a scheme such as https://".into(),
        EditorProblem::LinkHasSpace => "A link can't contain spaces or line breaks.".into(),
        EditorProblem::Template(why) => format!("This template can't be read: {why}."),
        EditorProblem::NeedsPro(_) => {
            "Fields that ask for a value are a Pro feature. Remove them to save.".into()
        }
        EditorProblem::SourceEmpty => "Choose a file, or cancel.".into(),
        EditorProblem::SourceMissing => "There is no file at that path.".into(),
        EditorProblem::SourceNotAFile => "That path is a folder, not a file.".into(),
        EditorProblem::SourceUnreadable => {
            "Snipdeck can't read that file. It may be in use or protected.".into()
        }
        EditorProblem::SourceTooLarge { max_mb } => {
            format!("That file is over {max_mb} MB, the most an item can hold.")
        }
        EditorProblem::SourceEmptyFile => "That file is empty.".into(),
        EditorProblem::SourceNotPng => {
            "An Image item holds a PNG picture. Choose a PNG file, or add this one as a File."
                .into()
        }
        EditorProblem::SourceBadName => {
            "Snipdeck can't keep a file with that name. Rename the file and try again.".into()
        }
        EditorProblem::Tag(error) => name_error_text(error),
        EditorProblem::Save(failure) => match failure {
            SaveFailure::Locked => "Unlock your library to save this item.".into(),
            SaveFailure::KeyUnavailable => {
                "Snipdeck couldn't open the encryption key, so nothing was saved.".into()
            }
            SaveFailure::ItemGone => "This item was deleted, so nothing was saved.".into(),
            SaveFailure::ListGone => "That list was deleted. Choose another list.".into(),
            SaveFailure::Storage => {
                "Snipdeck couldn't write to your library. Check that the disk has room.".into()
            }
            SaveFailure::Other => "Snipdeck couldn't save this item. Nothing was changed.".into(),
        },
        EditorProblem::DraftNotSaved => "Snipdeck couldn't save a draft of what you typed.".into(),
        EditorProblem::BrowseFailed => {
            "The file dialog couldn't open. Type the path instead.".into()
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
            ToastMessage::PanelHidden { chord: None },
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
            ToastMessage::CannotOpenLog,
            ToastMessage::KeyUnavailable,
            ToastMessage::ItemGone,
            ToastMessage::ListGone,
            ToastMessage::StorageFailed,
            ToastMessage::NotSaved,
            ToastMessage::NotOpened,
            ToastMessage::DraftNotSaved,
        ];
        for message in all {
            assert!(!toast_text(&message).is_empty(), "{message:?}");
        }
        assert!(
            toast_text(&ToastMessage::PanelHidden {
                chord: Some("Ctrl+Shift+Space".into())
            })
            .contains("Press Ctrl+Shift+Space or click the tray icon")
        );
        assert!(!toast_text(&ToastMessage::PanelHidden { chord: None }).contains("Press"));
        assert!(
            toast_text(&ToastMessage::HotkeyInUse {
                chord: "Ctrl+Alt+Space".into()
            })
            .starts_with("Ctrl+Alt+Space")
        );
    }
}
