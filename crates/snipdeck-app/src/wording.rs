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

use crate::toast::ToastMessage;

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
