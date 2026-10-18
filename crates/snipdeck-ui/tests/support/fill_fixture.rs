//! Sample template-fill data shared by `tests/fill.rs` (goldens) and `tests/fill_logic.rs` (behaviour);
//! `#[path]`-included, so each test binary compiles its own copy.
#![allow(dead_code, reason = "each test binary uses a subset of the fixtures")]

use slint::{ModelRc, SharedString, VecModel};
use snipdeck_ui::{FormField, TemplateFieldKind, TemplateFillWindow};

pub fn text(label: &str, value: &str, required: bool) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Text,
        value: value.into(),
        required,
        ..Default::default()
    }
}

pub fn choice(label: &str, options: &[&str], index: i32) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Choice,
        options: ModelRc::new(VecModel::from(
            options
                .iter()
                .map(|o| SharedString::from(*o))
                .collect::<Vec<_>>(),
        )),
        choice_index: index,
        ..Default::default()
    }
}

pub fn date(label: &str) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Date,
        placeholder: "YYYY-MM-DD".into(),
        ..Default::default()
    }
}

pub fn multiline(label: &str) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Multiline,
        placeholder: "Anything to add".into(),
        ..Default::default()
    }
}

/// Text (required, with a default), text (optional), choice, date, multi-line.
pub fn fields() -> Vec<FormField> {
    vec![
        text("Name", "Ada", true),
        FormField {
            placeholder: "Optional".into(),
            ..text("Company", "", false)
        },
        choice("Tone", &["Friendly", "Formal", "Brief"], 0),
        date("Due date"),
        multiline("Notes"),
    ]
}

pub const PREVIEW: &str =
    "Hello Ada,\n\nA gentle reminder that the invoice is due.\n\nThanks,\nSnipdeck";

pub fn set_fields(ui: &TemplateFillWindow, fields: Vec<FormField>) {
    ui.set_fields(ModelRc::new(VecModel::from(fields)));
}

/// The usual populated form.
pub fn populate(ui: &TemplateFillWindow) {
    ui.set_template_title("Invoice reminder".into());
    set_fields(ui, fields());
    ui.set_preview(PREVIEW.into());
}
