//! The template fill form's state (DESIGN.md §4 `TemplateField`, SPEC 5.4): one field per prompt of
//! a template, the answers typed so far, and the live preview. Pure data; the binding copies it
//! into the `TemplateFillWindow` and the controller re-runs the use with [`FillModel::answers`].

use std::collections::BTreeMap;

use snipdeck_core::{
    ItemId, Timestamp, UseMode,
    template::{
        Locale, Prompt, PromptKind, RenderContext, TOGGLE_ON, Template, TimeZone, parse_pick_date,
        transform::Transform,
    },
};

/// How a field is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text (`{field:Name}`).
    Text,
    /// One of a fixed set (`{choice:Name|a|b}`).
    Choice,
    /// Free text over several lines (`{multiline:Name|default}`).
    Multiline,
    /// A day typed as `YYYY-MM-DD` (`{pickdate:Name|FORMAT}`); left empty it is today.
    Date,
    /// A tick box (`{optional:Name}...{/optional}`): its value is [`TOGGLE_ON`] or empty.
    Toggle,
}

/// Why a field stops the form from being submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// A required field is empty (only whitespace counts as empty).
    Empty,
    /// A date field holds something that is not a real `YYYY-MM-DD` day.
    BadDate,
}

/// One field of the form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// The prompt's name: the label, and the key of its answer.
    pub label: String,
    /// How it is answered.
    pub kind: FieldKind,
    /// What is typed or chosen now: the template's default, or the first option.
    pub value: String,
    /// The options of a choice, in the order written; empty for text.
    pub options: Vec<String>,
    /// The chosen option of a choice (0 for text).
    pub choice_index: usize,
    /// A text field the template gave no default for: it must not be left empty.
    pub required: bool,
}

/// A template waiting for the user's answers.
#[derive(Debug, Clone)]
pub struct FillModel {
    item: ItemId,
    mode: UseMode,
    title: String,
    template: Template,
    fields: Vec<Field>,
    transform: Option<Transform>,
}

impl FillModel {
    /// The form for `template`, which belongs to item `item` (shown as `title`) and will be
    /// used with `mode` once the form is submitted.
    #[must_use]
    pub fn new(item: ItemId, mode: UseMode, title: String, template: Template) -> Self {
        let fields = template.prompts().into_iter().map(field_of).collect();
        Self {
            item,
            mode,
            title,
            template,
            fields,
            transform: None,
        }
    }

    /// Pastes the filled template through `transform` ("Paste as...").
    #[must_use]
    pub fn with_transform(mut self, transform: Option<Transform>) -> Self {
        self.transform = transform;
        self
    }

    /// The "Paste as..." transform to apply once the form is submitted.
    #[must_use]
    pub fn transform(&self) -> Option<Transform> {
        self.transform
    }

    /// The template item.
    #[must_use]
    pub fn item(&self) -> ItemId {
        self.item
    }

    /// How the user asked to use it.
    #[must_use]
    pub fn mode(&self) -> UseMode {
        self.mode
    }

    /// The item's name, for the form's header.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The fields, in the order the template first uses them.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// The user typed or chose `value` in field `index`. A choice takes only one of its
    /// options (anything else is ignored); an index past the end is ignored.
    pub fn set_value(&mut self, index: usize, value: &str) {
        let Some(field) = self.fields.get_mut(index) else {
            return;
        };
        match field.kind {
            FieldKind::Choice => {
                let Some(chosen) = field.options.iter().position(|o| o == value) else {
                    return;
                };
                field.choice_index = chosen;
            }
            FieldKind::Toggle => {
                let ticked = if value == TOGGLE_ON { TOGGLE_ON } else { "" };
                ticked.clone_into(&mut field.value);
                return;
            }
            FieldKind::Text | FieldKind::Multiline | FieldKind::Date => {}
        }
        value.clone_into(&mut field.value);
    }

    /// Sets every field from `values`, in field order (what the window sends on submit).
    /// Missing values leave their fields as they are.
    pub fn set_values(&mut self, values: &[String]) {
        for (index, value) in values.iter().enumerate() {
            self.set_value(index, value);
        }
    }

    /// The first field that stops the form, and why: a required field left empty, or a
    /// date that is not a real day (an empty date is fine: it means today).
    #[must_use]
    pub fn first_missing(&self) -> Option<(usize, Missing)> {
        self.fields.iter().enumerate().find_map(|(i, f)| {
            let value = f.value.trim();
            if f.required && value.is_empty() {
                Some((i, Missing::Empty))
            } else if f.kind == FieldKind::Date
                && !value.is_empty()
                && parse_pick_date(value).is_none()
            {
                Some((i, Missing::BadDate))
            } else {
                None
            }
        })
    }

    /// The answers by prompt name, for [`crate::usecase::UseRequest::answers`].
    #[must_use]
    pub fn answers(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .map(|f| (f.label.clone(), f.value.clone()))
            .collect()
    }

    /// The template as it would be pasted with the answers so far (a field not answered yet
    /// is empty in it). Each `{uuid}` shows as [`preview_uuid`], which the window replaces with
    /// a translated "(new ID)": the real one is drawn when the paste happens.
    #[must_use]
    pub fn preview(&self, now: Timestamp, tz: &TimeZone, locale: Locale) -> String {
        let answers = self.answers();
        let uuids = vec![PREVIEW_UUID_BYTES; self.template.uuid_count()];
        let ctx = RenderContext::new(now, &answers)
            .with_tz(tz.clone())
            .with_locale(locale)
            .with_uuids(&uuids);
        self.template.render(&ctx).text
    }
}

const PREVIEW_UUID_BYTES: [u8; 16] = [0xA5; 16];

/// The text a `{uuid}` has in [`FillModel::preview`]; the window swaps it for a translated label.
#[must_use]
pub fn preview_uuid() -> String {
    uuid::Builder::from_random_bytes(PREVIEW_UUID_BYTES)
        .into_uuid()
        .to_string()
}

fn field_of(prompt: Prompt) -> Field {
    match prompt.kind {
        PromptKind::Field { default } => Field {
            label: prompt.name,
            kind: FieldKind::Text,
            required: default.is_none(),
            value: default.unwrap_or_default(),
            options: Vec::new(),
            choice_index: 0,
        },
        PromptKind::Choice { options } => Field {
            label: prompt.name,
            kind: FieldKind::Choice,
            value: options.first().cloned().unwrap_or_default(),
            options,
            choice_index: 0,
            required: false,
        },
        PromptKind::Multiline { default } => Field {
            label: prompt.name,
            kind: FieldKind::Multiline,
            required: default.is_none(),
            value: default.unwrap_or_default(),
            options: Vec::new(),
            choice_index: 0,
        },
        PromptKind::Date => blank(prompt.name, FieldKind::Date),
        PromptKind::Toggle => blank(prompt.name, FieldKind::Toggle),
    }
}

/// A field that starts empty and may stay so: a date (today) or an unticked box.
fn blank(label: String, kind: FieldKind) -> Field {
    Field {
        label,
        kind,
        value: String::new(),
        options: Vec::new(),
        choice_index: 0,
        required: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-18T09:00:00Z".parse().expect("timestamp")
    }

    fn form(src: &str) -> FillModel {
        FillModel::new(
            ItemId::generate(),
            UseMode::Paste,
            "Reminder".into(),
            Template::parse(src).expect("template"),
        )
    }

    fn preview(m: &FillModel) -> String {
        m.preview(now(), &TimeZone::UTC, Locale::for_tag("en"))
    }

    #[test]
    fn every_uuid_in_the_preview_is_the_marker_not_a_nil_id() {
        let m = form("a {uuid} b {uuid}");
        let marker = preview_uuid();
        assert_ne!(marker, uuid::Uuid::nil().to_string());
        assert_eq!(preview(&m), format!("a {marker} b {marker}"));
    }

    #[test]
    fn each_prompt_becomes_a_field_with_its_default() {
        let m = form("Hi {field:Name}, {field:Greeting|Dear}, pick {choice:Tone|warm|firm}.");
        let f = m.fields();
        assert_eq!(f.len(), 3);
        assert_eq!(
            (f[0].label.as_str(), f[0].kind, f[0].required),
            ("Name", FieldKind::Text, true)
        );
        assert_eq!(f[0].value, "");
        assert_eq!((f[1].value.as_str(), f[1].required), ("Dear", false));
        assert_eq!(f[2].kind, FieldKind::Choice);
        assert_eq!(
            (f[2].options.as_slice(), f[2].value.as_str()),
            (&["warm".to_owned(), "firm".to_owned()][..], "warm")
        );
        assert_eq!(m.title(), "Reminder");
    }

    #[test]
    fn a_template_without_prompts_has_no_fields() {
        assert_eq!(form("Just text {date}").fields(), Vec::new());
    }

    #[test]
    fn typing_fills_the_answers_and_the_preview_follows() {
        let mut m = form("Hi {field:Name}, see you {choice:When|today|tomorrow}.");
        assert_eq!(preview(&m), "Hi , see you today.");
        m.set_value(0, "Ada");
        m.set_value(1, "tomorrow");
        assert_eq!(preview(&m), "Hi Ada, see you tomorrow.");
        let answers = m.answers();
        assert_eq!(answers["Name"], "Ada");
        assert_eq!(answers["When"], "tomorrow");
        assert_eq!(m.fields()[1].choice_index, 1);
    }

    #[test]
    fn a_choice_takes_only_its_options_and_a_bad_index_is_ignored() {
        let mut m = form("{choice:When|today|tomorrow}");
        m.set_value(0, "never");
        assert_eq!(m.fields()[0].value, "today");
        m.set_value(7, "x");
        assert_eq!(m.fields().len(), 1);
    }

    #[test]
    fn submit_values_set_every_field_in_order() {
        let mut m = form("{field:A} {field:B}");
        m.set_values(&["1".into(), "2".into()]);
        assert_eq!(m.answers().into_values().collect::<Vec<_>>(), ["1", "2"]);
        // Extra values are ignored, fewer leave the rest alone.
        m.set_values(&["9".into(), "8".into(), "7".into()]);
        m.set_values(&["5".into()]);
        assert_eq!(m.answers().into_values().collect::<Vec<_>>(), ["5", "8"]);
    }

    #[test]
    fn a_required_field_left_blank_is_missing() {
        let mut m = form("{field:Name} {field:Nick|friend}");
        assert_eq!(m.first_missing(), Some((0, Missing::Empty)));
        m.set_value(0, "   ");
        assert_eq!(
            m.first_missing(),
            Some((0, Missing::Empty)),
            "whitespace is not an answer"
        );
        m.set_value(0, "Ada");
        assert_eq!(m.first_missing(), None);
        // A field with a default may be cleared on purpose.
        m.set_value(1, "");
        assert_eq!(m.first_missing(), None);
        assert_eq!(m.answers()["Nick"], "");
    }

    #[test]
    fn multiline_answers_keep_their_newlines() {
        let mut m = form("Note:\n{multiline:Body}");
        assert_eq!(m.fields()[0].kind, FieldKind::Multiline);
        assert_eq!(m.first_missing(), Some((0, Missing::Empty)));
        m.set_value(0, "line one\nline two");
        assert_eq!(m.first_missing(), None);
        assert_eq!(preview(&m), "Note:\nline one\nline two");
    }

    #[test]
    fn a_multiline_default_fills_the_box_and_is_not_required() {
        let m = form("{multiline:Body|Hi}");
        assert_eq!(
            (m.fields()[0].value.as_str(), m.fields()[0].required),
            ("Hi", false)
        );
    }

    #[test]
    fn an_empty_date_is_today_and_not_missing() {
        let m = form("{pickdate:Due|%F}");
        assert_eq!(m.fields()[0].kind, FieldKind::Date);
        assert_eq!(m.first_missing(), None);
        assert_eq!(preview(&m), "2026-10-18");
    }

    #[test]
    fn a_bad_date_is_missing_with_a_reason() {
        let mut m = form("{pickdate:Due|%F}");
        m.set_value(0, "2026-02-30");
        assert_eq!(m.first_missing(), Some((0, Missing::BadDate)));
        m.set_value(0, "2026-03-01");
        assert_eq!(m.first_missing(), None);
        assert_eq!(preview(&m), "2026-03-01");
    }

    #[test]
    fn a_toggle_takes_only_ticked_or_empty_and_drives_its_section() {
        let mut m = form("Hi{optional:PS}, P.S. thanks{/optional}.");
        assert_eq!(m.fields()[0].kind, FieldKind::Toggle);
        assert_eq!(m.first_missing(), None);
        assert_eq!(preview(&m), "Hi.");
        m.set_value(0, TOGGLE_ON);
        assert_eq!(preview(&m), "Hi, P.S. thanks.");
        m.set_value(0, "yes please");
        assert_eq!(m.fields()[0].value, "");
        assert_eq!(preview(&m), "Hi.");
    }
}
