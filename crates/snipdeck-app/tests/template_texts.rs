//! What is wrong with a template is worded by the UI crate's `TemplateTexts` global, so the
//! editor's save error and the preview card follow the selected language (M5 of the audit).
//! One process for this file (the testing platform is process-wide).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failure is a failed test"
)]

use snipdeck_app::{
    item_editor::{EditorProblem, TemplateProblem},
    wording::{editor_problem_text, template_problem_text},
};
use snipdeck_core::template::{TemplateErrorKind as K, UnitHint};
use snipdeck_ui::PanelWindow;

#[path = "support/mod.rs"]
mod support;
use support::on_ui_thread;

/// One sample of every kind. The match has no wildcard, so a new `TemplateErrorKind` stops this
/// file compiling until it is listed (and so tested).
fn samples() -> Vec<K> {
    let all = vec![
        K::UnmatchedClose,
        K::Unterminated,
        K::EmptyPlaceholder,
        K::UnclosedSection,
        K::NestedSection,
        K::UnmatchedSectionEnd,
        K::EmptyName,
        K::NoOptions,
        K::EmptyOption,
        K::DuplicateOption { option: "a".into() },
        K::TooManyParts,
        K::DuplicateCursor,
        K::TrailingPercent,
        K::Unknown {
            found: "dat".into(),
            suggestion: Some("date"),
        },
        K::Unknown {
            found: "zzz".into(),
            suggestion: None,
        },
        K::UnexpectedArgument {
            placeholder: "date",
        },
        K::MissingArgument {
            placeholder: "datetime",
        },
        K::ConflictingPrompt { name: "N".into() },
        K::TooManyPrompts { max: 20 },
        K::BadOffset,
        K::UnknownOffsetUnit {
            found: "m".into(),
            hint: UnitHint::MonthOrMinute,
        },
        K::UnknownOffsetUnit {
            found: "x".into(),
            hint: UnitHint::AnyUnit,
        },
        K::OffsetOutOfRange {
            unit: "d",
            max: 36_500,
        },
        K::TooManyOffsets { max: 8 },
        K::UnknownFormatSpecifier { specifier: 'q' },
    ];
    for kind in &all {
        match kind {
            K::UnmatchedClose
            | K::Unterminated
            | K::EmptyPlaceholder
            | K::UnclosedSection
            | K::NestedSection
            | K::UnmatchedSectionEnd
            | K::EmptyName
            | K::NoOptions
            | K::EmptyOption
            | K::DuplicateOption { .. }
            | K::TooManyParts
            | K::DuplicateCursor
            | K::TrailingPercent
            | K::Unknown { .. }
            | K::UnexpectedArgument { .. }
            | K::MissingArgument { .. }
            | K::ConflictingPrompt { .. }
            | K::TooManyPrompts { .. }
            | K::BadOffset
            | K::UnknownOffsetUnit { .. }
            | K::OffsetOutOfRange { .. }
            | K::TooManyOffsets { .. }
            | K::UnknownFormatSpecifier { .. } => {}
        }
    }
    all
}

#[test]
fn every_template_problem_has_words_in_english_and_follows_the_translation() {
    on_ui_thread(|| {
        let ui = PanelWindow::new().expect("window");
        let english: Vec<String> = samples()
            .into_iter()
            .map(|kind| template_problem_text(&ui, &TemplateProblem::Parse(kind)))
            .collect();
        for (kind, text) in samples().iter().zip(&english) {
            assert!(!text.is_empty(), "{kind:?}");
            assert!(
                !text.ends_with('.'),
                "the sentence around it adds the stop: {text}"
            );
            if !matches!(kind, K::Unknown { .. }) {
                assert_eq!(
                    *text,
                    kind.to_string(),
                    "the English is the parser's own words"
                );
            }
        }
        assert!(
            english[13].contains("did you mean `{date}`?"),
            "{}",
            english[13]
        );
        assert_eq!(
            template_problem_text(&ui, &TemplateProblem::Sensitive),
            "a placeholder is not valid"
        );

        slint::select_bundled_translation("de").expect("de is bundled");
        for (kind, said) in samples().iter().zip(&english) {
            let text = template_problem_text(&ui, &TemplateProblem::Parse(kind.clone()));
            assert_ne!(&text, said, "{kind:?} stays English in German: {text}");
            assert!(!text.contains(&kind.to_string()), "{text}");
        }
        let wrapped = editor_problem_text(
            &ui,
            &EditorProblem::Template(TemplateProblem::Parse(K::EmptyOption)),
        );
        assert!(
            !wrapped.contains("template can’t be read") && !wrapped.contains("is empty"),
            "no English sentence is left: {wrapped}"
        );
        slint::select_bundled_translation("").expect("back to the source language");
    });
}
