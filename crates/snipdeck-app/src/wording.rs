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
use snipdeck_ui::{
    FormTexts, ImportTexts, MenuTexts, PanelWindow, SettingsPage, ToastTexts, TrayTexts,
};

use snipdeck_core::{ItemKind, ItemLabel, ListIcon};

use crate::{
    item_editor::{AttachmentView, EditorProblem, SaveFailure},
    settings::DockEdge,
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
            texts.invoke_too_large(name(n), size_text(window, *size).into())
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
    for<'a> FormTexts<'a>: Global<'a, C>,
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
        ToastMessage::TaskbarEdge => t.invoke_taskbar_edge(),
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
        ToastMessage::ItemPlaced { position, total } => {
            t.invoke_item_placed(count(*position), count(*total))
        }
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
/// `window` is any window of the UI, through which the `MenuTexts` global is reached.
#[must_use]
pub fn use_label_text<C>(window: &C, label: crate::panel::UseLabel) -> String
where
    for<'a> MenuTexts<'a>: Global<'a, C>,
{
    let t = MenuTexts::get(window);
    match label {
        crate::panel::UseLabel::Paste => t.invoke_item_use(),
        crate::panel::UseLabel::Copy => t.invoke_item_copy(),
    }
    .into()
}

/// The words of a row of the Panel's kebab menu. The dock row reads "Undock the panel" while the
/// Panel is `docked`, else it names the edge it would dock to (`dock_target`).
#[must_use]
pub fn app_menu_text(
    window: &PanelWindow,
    entry: crate::controller::AppMenuEntry,
    docked: bool,
    dock_target: Option<DockEdge>,
) -> String {
    use crate::controller::AppMenuEntry;
    let t = MenuTexts::get(window);
    match entry {
        AppMenuEntry::Open => t.invoke_app_open(),
        AppMenuEntry::Commands => t.invoke_app_commands(),
        AppMenuEntry::NewSnippet => t.invoke_app_new_snippet(),
        AppMenuEntry::Settings => t.invoke_app_settings(),
        AppMenuEntry::Dock if docked => t.invoke_app_undock(),
        AppMenuEntry::Dock => match dock_target {
            Some(DockEdge::Left) => t.invoke_app_dock_left(),
            Some(DockEdge::Right) => t.invoke_app_dock_right(),
            Some(DockEdge::Top) => t.invoke_app_dock_top(),
            Some(DockEdge::Bottom) => t.invoke_app_dock_bottom(),
            None => t.invoke_app_dock(),
        },
        AppMenuEntry::PauseHotkeys => t.invoke_app_pause_hotkeys(),
        AppMenuEntry::Separator => return String::new(),
        AppMenuEntry::Troubleshoot => t.invoke_app_troubleshoot(),
        AppMenuEntry::UserGuide => t.invoke_app_user_guide(),
        AppMenuEntry::Shortcuts => t.invoke_app_shortcuts(),
        AppMenuEntry::About => t.invoke_app_about(),
        AppMenuEntry::Quit => t.invoke_app_quit(),
    }
    .into()
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
        ItemMenuEntry::MoveUp { .. } => "Alt+Up",
        ItemMenuEntry::MoveDown { .. } => "Alt+Down",
        ItemMenuEntry::Pin { .. } | ItemMenuEntry::Stack { .. } | ItemMenuEntry::Separator => "",
    }
}

/// The words of a row of the Panel's item menu. `window` is the Panel, through which
/// `MenuTexts` and `ToastTexts` are reached.
#[must_use]
pub fn item_menu_label(window: &PanelWindow, entry: crate::controller::ItemMenuEntry) -> String {
    use crate::controller::ItemMenuEntry;
    let t = MenuTexts::get(window);
    let moves = ToastTexts::get(window);
    match entry {
        ItemMenuEntry::Use => t.invoke_item_use(),
        ItemMenuEntry::Copy => t.invoke_item_copy(),
        ItemMenuEntry::Edit => t.invoke_item_edit(),
        ItemMenuEntry::Duplicate => t.invoke_item_duplicate(),
        ItemMenuEntry::Pin { pinned: false } => t.invoke_item_pin(),
        ItemMenuEntry::Pin { pinned: true } => t.invoke_item_unpin(),
        ItemMenuEntry::Stack { marked: false } => t.invoke_item_stack_add(),
        ItemMenuEntry::Stack { marked: true } => t.invoke_item_stack_remove(),
        ItemMenuEntry::Trash => t.invoke_item_trash(),
        ItemMenuEntry::MoveUp { .. } => moves.invoke_menu_move_up(),
        ItemMenuEntry::MoveDown { .. } => moves.invoke_menu_move_down(),
        ItemMenuEntry::Separator => return String::new(),
    }
    .into()
}

/// The words of a board menu entry.
#[must_use]
pub fn board_menu_text<C>(window: &C, entry: &crate::controller::BoardMenuEntry) -> String
where
    for<'a> MenuTexts<'a>: Global<'a, C>,
{
    use crate::controller::BoardMenuEntry;
    let t = MenuTexts::get(window);
    match entry {
        BoardMenuEntry::Board { name, .. } => return name.clone(),
        BoardMenuEntry::Separator => return String::new(),
        BoardMenuEntry::New => t.invoke_board_new(),
        BoardMenuEntry::Rename => t.invoke_board_rename(),
        BoardMenuEntry::MoveUp { .. } => t.invoke_board_move_up(),
        BoardMenuEntry::MoveDown { .. } => t.invoke_board_move_down(),
        BoardMenuEntry::Delete { .. } => t.invoke_board_delete(),
    }
    .into()
}

/// A count as the `int` a Slint function takes.
fn int<T: TryInto<i32>>(n: T) -> i32 {
    n.try_into().unwrap_or(i32::MAX)
}

/// Why an editor refuses a name, in words.
#[must_use]
pub fn name_error_text<C>(window: &C, error: &crate::main_window::NameError) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    use crate::main_window::NameError;
    let t = FormTexts::get(window);
    match error {
        NameError::Empty => t.invoke_name_empty(),
        NameError::TooLong { max } => t.invoke_name_too_long(int(*max)),
        NameError::ControlChar => t.invoke_name_control_char(),
    }
    .into()
}

/// Why the Tag manager refuses a new tag name, in words.
#[must_use]
pub fn tag_rename_error_text<C>(window: &C, error: &crate::controller::TagRenameError) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    use crate::controller::TagRenameError;
    let t = FormTexts::get(window);
    match error {
        TagRenameError::Name(name) => name_error_text(window, name),
        TagRenameError::Exists => t.invoke_tag_exists().into(),
        TagRenameError::Failed => t.invoke_tag_rename_failed().into(),
    }
}

/// The reason a board cannot be deleted, in words.
#[must_use]
pub fn delete_hint_text<C>(window: &C, hint: crate::main_window::DeleteHint) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    match hint {
        crate::main_window::DeleteHint::LastBoard => {
            FormTexts::get(window).invoke_last_board().into()
        }
    }
}

/// A size for people: "800 bytes", "12 KB", "2.4 MB".
#[must_use]
pub fn size_text<C>(window: &C, bytes: u64) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    let t = FormTexts::get(window);
    if bytes < KB {
        t.invoke_size_bytes(int(bytes))
    } else if bytes < MB {
        t.invoke_size_kb(int(bytes / KB))
    } else {
        let tenths = bytes.saturating_mul(10) / MB;
        t.invoke_size_mb(int(tenths / 10), int(tenths % 10))
    }
    .into()
}

/// What the item editor shows as the name of an existing Image or File item.
#[must_use]
pub fn attachment_name_text<C>(window: &C, attachment: &AttachmentView) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    let t = FormTexts::get(window);
    match attachment.kind {
        ItemKind::Image => t.invoke_attachment_picture().into(),
        _ if attachment.files > 1 => t
            .invoke_attachment_and_more(attachment.name.as_str().into(), int(attachment.files - 1))
            .into(),
        _ => attachment.name.clone(),
    }
}

/// What the item editor shows under the name of an existing Image or File item.
#[must_use]
pub fn attachment_detail_text<C>(window: &C, attachment: &AttachmentView) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    let t = FormTexts::get(window);
    let size = size_text(window, attachment.bytes);
    match (attachment.kind, attachment.dimensions) {
        (ItemKind::Image, Some((w, h))) => t
            .invoke_attachment_image_dimensions(int(w), int(h), size.into())
            .into(),
        (ItemKind::Image, None) => t.invoke_attachment_image(size.into()).into(),
        _ if attachment.files > 1 => t
            .invoke_attachment_files(int(attachment.files), size.into())
            .into(),
        _ => size,
    }
}

/// Why the item editor refuses something, in words.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one match arm per problem")]
pub fn editor_problem_text<C>(window: &C, problem: &EditorProblem) -> String
where
    for<'a> FormTexts<'a>: Global<'a, C>,
{
    let t = FormTexts::get(window);
    match problem {
        EditorProblem::TitleTooLong { max } => t.invoke_title_too_long(int(*max)),
        EditorProblem::TitleHasControl => t.invoke_title_control_char(),
        EditorProblem::ContentEmpty => t.invoke_content_empty(),
        EditorProblem::ContentTooLong { max_kb } => t.invoke_content_too_long(int(*max_kb)),
        EditorProblem::LinkNeedsScheme => t.invoke_link_needs_scheme(),
        EditorProblem::LinkHasSpace => t.invoke_link_has_space(),
        EditorProblem::Template(why) => t.invoke_template_unreadable(why.as_str().into()),
        EditorProblem::NeedsPro(_) => t.invoke_needs_supporter(),
        EditorProblem::SourceEmpty => t.invoke_source_empty(),
        EditorProblem::SourceMissing => t.invoke_source_missing(),
        EditorProblem::SourceNotAFile => t.invoke_source_not_a_file(),
        EditorProblem::SourceUnreadable => t.invoke_source_unreadable(),
        EditorProblem::SourceTooLarge { max_mb } => t.invoke_source_too_large(int(*max_mb)),
        EditorProblem::SourceEmptyFile => t.invoke_source_empty_file(),
        EditorProblem::SourceNotPng => t.invoke_source_not_png(),
        EditorProblem::SourceBadName => t.invoke_source_bad_name(),
        EditorProblem::Tag(error) => return name_error_text(window, error),
        EditorProblem::Save(failure) => match failure {
            SaveFailure::Locked => t.invoke_save_locked(),
            SaveFailure::KeyUnavailable => t.invoke_save_key_unavailable(),
            SaveFailure::ItemGone => t.invoke_save_item_gone(),
            SaveFailure::ListGone => t.invoke_save_list_gone(),
            SaveFailure::Storage => t.invoke_save_storage(),
            SaveFailure::Other => t.invoke_save_other(),
        },
        EditorProblem::DraftNotSaved => t.invoke_draft_not_saved(),
        EditorProblem::BrowseFailed => t.invoke_browse_failed(),
    }
    .into()
}
