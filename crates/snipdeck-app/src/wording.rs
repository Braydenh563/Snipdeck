//! The words Rust puts on screen: the toasts, the tray menu's labels and the Panel's menus.
//!
//! The Panel's own strings are `@tr` in Slint and translate with the bundled catalogs. Rust
//! cannot reach those catalogs, so what it shows goes through a UI-crate global that holds the
//! `@tr` text: `ToastTexts` (every toast, read with [`toast_text`]), `TrayTexts` ([`tray_labels`])
//! and `ImportTexts`. The Panel's item and kebab menus below are still plain English (listed in
//! `docs/HANDOVER.md`).

use slint::Global;
use snipdeck_platform::tray::TrayLabels;
use snipdeck_portable::Warning;
use snipdeck_ui::{ImportTexts, PanelWindow, SettingsPage, ToastTexts, TrayTexts};

use snipdeck_core::{ItemKind, ItemLabel, ListIcon};

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
        pause_history: texts.get_pause_history().into(),
        quit: texts.get_quit().into(),
        tooltip_paused: texts.get_tooltip_paused().into(),
    }
}

/// The name of an item colour label: shown beside the colour wherever it appears, so the colour is
/// never the only cue.
#[must_use]
pub fn item_label_name(label: ItemLabel) -> &'static str {
    match label {
        ItemLabel::Blue => "Blue",
        ItemLabel::Teal => "Teal",
        ItemLabel::Green => "Green",
        ItemLabel::Amber => "Amber",
        ItemLabel::Orange => "Orange",
        ItemLabel::Red => "Red",
        ItemLabel::Violet => "Violet",
        ItemLabel::Slate => "Slate",
    }
}

/// The spoken name of a list icon, for the icon picker.
#[must_use]
pub fn list_icon_name(icon: ListIcon) -> &'static str {
    match icon {
        ListIcon::Folder => "Folder",
        ListIcon::Star => "Star",
        ListIcon::Heart => "Heart",
        ListIcon::Briefcase => "Briefcase",
        ListIcon::House => "House",
        ListIcon::User => "Person",
        ListIcon::EnvelopeSimple => "Envelope",
        ListIcon::ChatCircle => "Chat",
        ListIcon::Phone => "Phone",
        ListIcon::CalendarBlank => "Calendar",
        ListIcon::ShoppingCart => "Shopping cart",
        ListIcon::CreditCard => "Credit card",
        ListIcon::Code => "Code",
        ListIcon::TerminalWindow => "Terminal",
        ListIcon::BookOpen => "Book",
        ListIcon::Lightbulb => "Light bulb",
        ListIcon::Flag => "Flag",
        ListIcon::BookmarkSimple => "Bookmark",
        ListIcon::Globe => "Globe",
        ListIcon::MapPin => "Map pin",
        ListIcon::Camera => "Camera",
        ListIcon::MusicNote => "Music",
        ListIcon::Wrench => "Wrench",
        ListIcon::RocketLaunch => "Rocket",
    }
}

/// The sentence for an import warning, in the language selected for the UI: the Settings import
/// preview lists these under "Will not be imported". The wording lives in the UI crate's
/// `ImportTexts` global (Rust cannot reach the translation catalogs); `window` is the Settings
/// page, through which the global is reached.
///
/// The importers name an entry that has no title "entry N" (history imports) or "Ditto clip N";
/// those labels are internals and read as "Clip N" here.
#[must_use]
pub fn import_warning_text(window: &SettingsPage, warning: &Warning) -> String {
    let texts = ImportTexts::get(window);
    let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    let name = |name: &str| -> slint::SharedString {
        match numbered_label(name) {
            Some(n) => texts.invoke_clip_name(count(n)),
            None => name.into(),
        }
    };
    let text = match warning {
        Warning::NewerMinorVersion { found } => texts.invoke_newer_minor_version(found.into()),
        Warning::UnknownEntries { count: n } => texts.invoke_unknown_entries(count(*n)),
        Warning::UnreferencedAssets { count: n } => texts.invoke_unreferenced_assets(count(*n)),
        Warning::SensitiveNeedKey { count: n } => texts.invoke_sensitive_need_key(count(*n)),
        Warning::CannotDecrypt { title } => texts.invoke_cannot_decrypt(title.into()),
        Warning::SensitiveOmittedByExport { count: n } => {
            texts.invoke_sensitive_omitted_by_export(i32::try_from(*n).unwrap_or(i32::MAX))
        }
        Warning::BoardLimit { limit, total } => {
            texts.invoke_board_limit(count(*total), count(*limit))
        }
        Warning::LossyText { name: n } => texts.invoke_lossy_text(name(n)),
        Warning::TooLarge { name: n, size } => {
            texts.invoke_too_large(name(n), size_text(*size).into())
        }
        Warning::Empty { name: n } => texts.invoke_empty(name(n)),
        Warning::Unreadable { name: n, .. } => texts.invoke_unreadable(name(n)),
        Warning::SymlinkSkipped { name: n } => texts.invoke_symlink_skipped(name(n)),
        Warning::NonTextSkipped { count: n } => texts.invoke_non_text_skipped(count(*n)),
        Warning::Truncated { .. } => texts.invoke_truncated(),
        // Free text from an importer: already a sentence, in English.
        Warning::Note(note) => return note.clone(),
    };
    text.into()
}

/// The N in "entry N" or "Ditto clip N", the labels the importers give a clip without a title.
fn numbered_label(name: &str) -> Option<usize> {
    let digits = name
        .strip_prefix("entry ")
        .or_else(|| name.strip_prefix("Ditto clip "))?;
    digits.parse().ok()
}

/// The words for `message`, in the language selected for the UI. The sentences live in the UI
/// crate's `ToastTexts` global (Rust cannot reach the translation catalogs); `window` is any
/// window of the UI, through which the global is reached.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one match arm per toast message")]
pub fn toast_text<C>(window: &C, message: &ToastMessage) -> String
where
    for<'a> ToastTexts<'a>: Global<'a, C>,
{
    let t = ToastTexts::get(window);
    let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    let text = match message {
        ToastMessage::Pasted { app } => t.invoke_pasted(app.into()),
        ToastMessage::Copied => t.invoke_copied(),
        ToastMessage::CopiedElevated { app } => t.invoke_copied_elevated(app.into()),
        ToastMessage::CopiedNothingFocused { app } => t.invoke_copied_nothing_focused(app.into()),
        ToastMessage::CopiedExcluded { app } => t.invoke_copied_excluded(app.into()),
        ToastMessage::CopiedTargetGone => t.invoke_copied_target_gone(),
        ToastMessage::ClipboardBusy => t.invoke_clipboard_busy(),
        ToastMessage::NeedsPro(_) => t.invoke_needs_pro(),
        ToastMessage::TemplateBroken => t.invoke_template_broken(),
        ToastMessage::Locked => t.invoke_locked(),
        ToastMessage::Unlocked => t.invoke_unlocked(),
        ToastMessage::MasterPasswordSet => t.invoke_master_password_set(),
        ToastMessage::MasterPasswordChanged => t.invoke_master_password_changed(),
        ToastMessage::MasterPasswordRemoved => t.invoke_master_password_removed(),
        ToastMessage::MasterPasswordChangedNotOpen => t.invoke_master_password_changed_not_open(),
        ToastMessage::MasterPasswordRemovedNotOpen => t.invoke_master_password_removed_not_open(),
        ToastMessage::LockedEditorClosed => t.invoke_locked_editor_closed(),
        ToastMessage::UnlockFirst => t.invoke_unlock_first(),
        ToastMessage::Saved { list } => t.invoke_saved(list.into()),
        ToastMessage::NothingToSave => t.invoke_nothing_to_save(),
        ToastMessage::TooLargeToSave => t.invoke_too_large_to_save(),
        ToastMessage::NoListToSave => t.invoke_no_list_to_save(),
        ToastMessage::NothingToPasteNext => t.invoke_nothing_to_paste_next(),
        ToastMessage::PasteNextFinished => t.invoke_paste_next_finished(),
        ToastMessage::HotkeyInUse { chord } => t.invoke_hotkey_in_use(chord.into()),
        ToastMessage::HotkeyInvalid { chord } => t.invoke_hotkey_invalid(chord.into()),
        ToastMessage::HotkeyUnavailable { chord } => t.invoke_hotkey_unavailable(chord.into()),
        ToastMessage::PanelHidden { chord } => {
            t.invoke_panel_hidden(chord.as_deref().unwrap_or_default().into())
        }
        ToastMessage::Failed => t.invoke_failed(),
        ToastMessage::WindowNotShown => t.invoke_window_not_shown(),
        ToastMessage::SettingsNotSaved => t.invoke_settings_not_saved(),
        ToastMessage::ItemsTrashed { count: n } => t.invoke_items_trashed(count(*n)),
        ToastMessage::ListTrashed { name } => t.invoke_list_trashed(name.into()),
        ToastMessage::BoardTrashed { name } => t.invoke_board_trashed(name.into()),
        ToastMessage::Restored { count: n } => t.invoke_restored(count(*n)),
        ToastMessage::ItemsMoved { count: n, list } => t.invoke_items_moved(count(*n), list.into()),
        ToastMessage::DeletedForever { .. } => t.invoke_deleted_forever(),
        ToastMessage::TrashEmptied => t.invoke_trash_emptied(),
        ToastMessage::BoardLimit => t.invoke_board_limit(),
        ToastMessage::LibraryProblem => t.invoke_library_problem(),
        ToastMessage::CrashedLastTime => t.invoke_crashed_last_time(),
        ToastMessage::LicenceAccepted => t.invoke_licence_accepted(),
        ToastMessage::LicenceRejected(problem) => return licence_problem_text(window, *problem),
        ToastMessage::BackupMade => t.invoke_backup_made(),
        ToastMessage::BackupFailed => t.invoke_backup_failed(),
        ToastMessage::BackupRestored { items } => t.invoke_backup_restored(count(*items)),
        ToastMessage::RestoreFailed => t.invoke_restore_failed(),
        ToastMessage::Imported { items } => t.invoke_imported(count(*items)),
        ToastMessage::ImportFailed => t.invoke_import_failed(),
        ToastMessage::Exported { file } => t.invoke_exported(file.into()),
        ToastMessage::ExportFailed => t.invoke_export_failed(),
        ToastMessage::LibraryErased => t.invoke_library_erased(),
        ToastMessage::EraseFailed => t.invoke_erase_failed(),
        ToastMessage::FolderNotOpened => t.invoke_folder_not_opened(),
        ToastMessage::MailNotOpened => t.invoke_mail_not_opened(),
        ToastMessage::AutostartFailed => t.invoke_autostart_failed(),
        ToastMessage::CannotOpenLog => t.invoke_cannot_open_log(),
        ToastMessage::KeyUnavailable => t.invoke_key_unavailable(),
        ToastMessage::ItemGone => t.invoke_item_gone(),
        ToastMessage::ListGone => t.invoke_list_gone(),
        ToastMessage::StorageFailed => t.invoke_storage_failed(),
        ToastMessage::NotSaved => t.invoke_not_saved(),
        ToastMessage::NotOpened => t.invoke_not_opened(),
        ToastMessage::DraftNotSaved => t.invoke_draft_not_saved(),
    };
    text.into()
}

/// Why a licence key was refused, in words (the toast and the line under the key field), in the
/// language selected for the UI.
#[must_use]
pub fn licence_problem_text<C>(window: &C, problem: LicenceProblem) -> String
where
    for<'a> ToastTexts<'a>: Global<'a, C>,
{
    let t = ToastTexts::get(window);
    match problem {
        LicenceProblem::Empty => t.invoke_licence_empty(),
        LicenceProblem::NotAKey => t.invoke_licence_not_a_key(),
        LicenceProblem::BadSignature => t.invoke_licence_bad_signature(),
        LicenceProblem::NewerFormat => t.invoke_licence_newer_format(),
        LicenceProblem::WrongTier => t.invoke_licence_wrong_tier(),
        LicenceProblem::NotSaved => t.invoke_licence_not_saved(),
    }
    .into()
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

/// Why an import could not be previewed, in words for the page, in the language selected for the
/// UI (the sentences are in the `ImportTexts` global; `window` is the Settings page).
#[must_use]
pub fn import_problem_text(
    window: &SettingsPage,
    problem: crate::controller::ImportProblem,
) -> String {
    use crate::controller::{ImportKind, ImportProblem};
    let texts = ImportTexts::get(window);
    match problem {
        ImportProblem::Unreadable => texts.invoke_problem_unreadable(),
        ImportProblem::NotThisKind(ImportKind::Snipdeck) => texts.invoke_problem_not_snipdeck(),
        ImportProblem::NotThisKind(ImportKind::Ditto) => texts.invoke_problem_not_ditto(),
        ImportProblem::NotThisKind(ImportKind::CopyQ) => texts.invoke_problem_not_copyq(),
        ImportProblem::NotThisKind(ImportKind::Folder) => texts.invoke_problem_not_folder(),
        ImportProblem::TooNew => texts.invoke_problem_too_new(),
        ImportProblem::HistoryOff => texts.invoke_problem_history_off(),
        ImportProblem::HistoryUnavailable => texts.invoke_problem_history_unavailable(),
        ImportProblem::HistoryEmpty => texts.invoke_problem_history_empty(),
        ImportProblem::Failed => texts.invoke_problem_failed(),
    }
    .into()
}

/// The name the import preview gives the Windows clipboard history, in the UI's language.
#[must_use]
pub fn history_source_name(window: &SettingsPage) -> String {
    ImportTexts::get(window).invoke_history_name().into()
}

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
        AppMenuEntry::Commands => "All commands\u{2026}",
        AppMenuEntry::NewSnippet => "New snippet",
        AppMenuEntry::Settings => "Settings\u{2026}",
        AppMenuEntry::Dock => "Dock to the nearest edge",
        AppMenuEntry::PauseHotkeys => "Pause hotkeys",
        AppMenuEntry::Separator => "",
        AppMenuEntry::Troubleshoot => "Troubleshoot\u{2026}",
        AppMenuEntry::UserGuide => "User guide",
        AppMenuEntry::Shortcuts => "Keyboard shortcuts",
        AppMenuEntry::About => "About Snipdeck",
        AppMenuEntry::Quit => "Quit",
    }
}

/// The shortcut shown beside a row of the Panel's kebab menu; empty where the row has none.
#[must_use]
pub const fn app_menu_shortcut(entry: crate::controller::AppMenuEntry) -> &'static str {
    use crate::controller::AppMenuEntry;
    match entry {
        AppMenuEntry::Commands => "Ctrl+K",
        AppMenuEntry::UserGuide => "F1",
        AppMenuEntry::Shortcuts => "Ctrl+/",
        _ => "",
    }
}

/// The shortcut shown beside a row of the Panel's item menu; empty where the row has none.
#[must_use]
pub const fn item_menu_shortcut(entry: crate::controller::ItemMenuEntry) -> &'static str {
    use crate::controller::ItemMenuEntry;
    match entry {
        ItemMenuEntry::Use => "Enter",
        ItemMenuEntry::Copy => "Shift+Enter",
        ItemMenuEntry::Edit => "Ctrl+E",
        ItemMenuEntry::Duplicate => "Ctrl+D",
        ItemMenuEntry::Trash => "Del",
        ItemMenuEntry::Pin { .. } | ItemMenuEntry::Stack { .. } | ItemMenuEntry::Separator => "",
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
        ItemMenuEntry::Duplicate => "Duplicate",
        ItemMenuEntry::Pin { pinned: false } => "Pin",
        ItemMenuEntry::Pin { pinned: true } => "Unpin",
        ItemMenuEntry::Stack { marked: false } => "Add to paste stack",
        ItemMenuEntry::Stack { marked: true } => "Remove from paste stack",
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
            "Fields that ask for a value are a Supporter feature. Remove them to save.".into()
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
