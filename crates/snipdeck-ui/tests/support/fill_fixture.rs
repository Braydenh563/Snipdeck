//! Sample template-fill data shared by `tests/fill.rs` (goldens) and `tests/fill_logic.rs` (behaviour);
//! `#[path]`-included, so each test binary compiles its own copy.
#![allow(dead_code, reason = "each test binary uses a subset of the fixtures")]

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use snipdeck_ui::{CalendarWords, FormField, TemplateFieldKind, TemplateFillWindow};

pub fn text(label: &str, value: &str, required: bool) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Text,
        value: value.into(),
        required,
        enabled: true,
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
        enabled: true,
        ..Default::default()
    }
}

pub fn date(label: &str) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Date,
        placeholder: "YYYY-MM-DD".into(),
        enabled: true,
        ..Default::default()
    }
}

pub fn multiline(label: &str) -> FormField {
    FormField {
        label: label.into(),
        kind: TemplateFieldKind::Multiline,
        placeholder: "Anything to add".into(),
        enabled: true,
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
    set_calendar(ui);
}

/// The words and today's date the host gives the month grid (today is 2026-10-17, fixed).
pub fn set_calendar(ui: &TemplateFillWindow) {
    let words = |list: &[&str]| {
        ModelRc::new(VecModel::from(
            list.iter()
                .map(|w| SharedString::from(*w))
                .collect::<Vec<_>>(),
        ))
    };
    let g = ui.global::<CalendarWords<'_>>();
    g.set_month_names(words(&[
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ]));
    g.set_weekday_names(words(&["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]));
    g.set_week_starts_sunday(false);
    g.set_today_year(2026);
    g.set_today_month(10);
    g.set_today_day(17);
}
