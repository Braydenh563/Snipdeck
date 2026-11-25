//! The template fill form's state (DESIGN.md §4 `TemplateField`, SPEC 5.4): one field per prompt of
//! a template, the answers typed so far, and the live preview. Pure data; the binding copies it
//! into the `TemplateFillWindow` and the controller re-runs the use with [`FillModel::answers`].

use std::collections::BTreeMap;

use snipdeck_core::{
    ItemId, Timestamp, UseMode,
    template::{
        Locale, Prompt, PromptKind, RenderContext, TOGGLE_ON, Template, TimeZone, parse_number,
        parse_pick_date, transform::Transform,
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
    /// A number (`{number:Name|default}`), typed with `.` or `,` as the decimal mark.
    Number,
    /// A day typed in the locale's order or as `YYYY-MM-DD` (`{pickdate:Name|FORMAT}`); left
    /// empty it is today.
    Date,
    /// A tick box (`{optional:Name}...{/optional}`): its value is [`TOGGLE_ON`] or empty.
    Toggle,
}

/// Why a field stops the form from being submitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// A required field is empty (only whitespace counts as empty).
    Empty,
    /// A date field holds something that is not a real day in the locale's order or ISO.
    BadDate,
    /// A number field holds something that is not a number.
    BadNumber,
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
    /// Whether the answer matters now: `false` for a field inside an `{optional}` section the
    /// answers leave unticked. The form disables it and never holds the submit for it.
    pub enabled: bool,
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
    locale: Locale,
    selection: Option<String>,
}

impl FillModel {
    /// The form for `template`, which belongs to item `item` (shown as `title`) and will be
    /// used with `mode` once the form is submitted. `locale` is how dates are typed and shown.
    #[must_use]
    pub fn new(
        item: ItemId,
        mode: UseMode,
        title: String,
        template: Template,
        locale: Locale,
    ) -> Self {
        let fields = template.prompts().into_iter().map(field_of).collect();
        let mut model = Self {
            item,
            mode,
            title,
            template,
            fields,
            transform: None,
            locale,
            selection: None,
        };
        model.refresh_enabled();
        model
    }

    /// The text that was selected in the Target when the use began, for `{selection}` in the
    /// preview and the paste.
    #[must_use]
    pub fn with_selection(mut self, selection: Option<String>) -> Self {
        self.selection = selection;
        self
    }

    /// The selection captured before the form opened.
    #[must_use]
    pub fn selection(&self) -> Option<&str> {
        self.selection.as_deref()
    }

    /// How dates are typed in this form.
    #[must_use]
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// The date format as letters (`DD/MM/YYYY`), the hint of a date field.
    #[must_use]
    pub fn date_hint(&self) -> String {
        self.locale.date.hint()
    }

    /// Recomputes which fields matter after an answer changed.
    fn refresh_enabled(&mut self) {
        let active = self.template.active_prompts(&self.answers());
        for (field, on) in self.fields.iter_mut().zip(active) {
            field.enabled = on;
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
                self.refresh_enabled();
                return;
            }
            FieldKind::Text | FieldKind::Multiline | FieldKind::Number | FieldKind::Date => {}
        }
        value.clone_into(&mut field.value);
        self.refresh_enabled();
    }

    /// The day field `index` holds, as (year, month, day), if it is a date field with a real
    /// day typed in: the month grid marks it.
    #[must_use]
    pub fn date_parts(&self, index: usize) -> Option<(i32, i32, i32)> {
        let field = self
            .fields
            .get(index)
            .filter(|f| f.kind == FieldKind::Date)?;
        let date = parse_pick_date(field.value.trim(), self.locale.date)?;
        Some((
            i32::from(date.year()),
            i32::from(date.month()),
            i32::from(date.day()),
        ))
    }

    /// A day was picked in the month grid of date field `index`: the field now holds it in the
    /// locale's date order, which is returned. `None` (nothing changes) for a field that is not
    /// a date or a day that does not exist.
    pub fn pick_date(&mut self, index: usize, year: i32, month: i32, day: i32) -> Option<String> {
        if self.fields.get(index)?.kind != FieldKind::Date {
            return None;
        }
        let date = jiff::civil::Date::new(
            i16::try_from(year).ok()?,
            i8::try_from(month).ok()?,
            i8::try_from(day).ok()?,
        )
        .ok()?;
        let text = self
            .locale
            .date
            .write(date.year(), date.month(), date.day());
        self.set_value(index, &text);
        Some(text)
    }

    /// Sets every field from `values`, in field order (what the window sends on submit).
    /// Missing values leave their fields as they are.
    pub fn set_values(&mut self, values: &[String]) {
        for (index, value) in values.iter().enumerate() {
            self.set_value(index, value);
        }
    }

    /// Every field that stops the form, and why, in field order. A field the answers switch
    /// off (inside an unticked `{optional}`) never stops it.
    #[must_use]
    pub fn missing(&self) -> Vec<(usize, Missing)> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.enabled)
            .filter_map(|(i, f)| {
                let value = f.value.trim();
                let why = if f.required && value.is_empty() {
                    Missing::Empty
                } else if f.kind == FieldKind::Date
                    && !value.is_empty()
                    && parse_pick_date(value, self.locale.date).is_none()
                {
                    Missing::BadDate
                } else if f.kind == FieldKind::Number
                    && !value.is_empty()
                    && parse_number(value).is_none()
                {
                    Missing::BadNumber
                } else {
                    return None;
                };
                Some((i, why))
            })
            .collect()
    }

    /// The first field that stops the form, and why (see [`missing`](Self::missing)).
    #[must_use]
    pub fn first_missing(&self) -> Option<(usize, Missing)> {
        self.missing().into_iter().next()
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
    pub fn preview(&self, now: Timestamp, tz: &TimeZone) -> String {
        let answers = self.answers();
        let uuids = vec![PREVIEW_UUID_BYTES; self.template.uuid_count()];
        let mut ctx = RenderContext::new(now, &answers)
            .with_tz(tz.clone())
            .with_locale(self.locale)
            .with_uuids(&uuids);
        if let Some(selection) = &self.selection {
            ctx = ctx.with_selection(selection);
        }
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
    let text_like = |kind, default: Option<String>| Field {
        label: prompt.name.clone(),
        kind,
        required: default.is_none(),
        value: default.unwrap_or_default(),
        options: Vec::new(),
        choice_index: 0,
        enabled: true,
    };
    match prompt.kind {
        PromptKind::Field { default } => text_like(FieldKind::Text, default),
        PromptKind::Multiline { default } => text_like(FieldKind::Multiline, default),
        PromptKind::Number { default } => text_like(FieldKind::Number, default),
        PromptKind::Choice { options, default } => Field {
            label: prompt.name,
            kind: FieldKind::Choice,
            value: options.get(default).cloned().unwrap_or_default(),
            options,
            choice_index: default,
            required: false,
            enabled: true,
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
        enabled: true,
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
            Locale::for_tag("en"),
        )
    }

    fn form_in(src: &str, tag: &str) -> FillModel {
        FillModel::new(
            ItemId::generate(),
            UseMode::Paste,
            "Reminder".into(),
            Template::parse(src).expect("template"),
            Locale::for_tag(tag),
        )
    }

    fn preview(m: &FillModel) -> String {
        m.preview(now(), &TimeZone::UTC)
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
    fn a_whitespace_only_answer_in_the_last_of_sixteen_fields_is_missing() {
        let src: String = (1..=16).fold(String::new(), |mut acc, i| {
            use std::fmt::Write;
            let _ = write!(acc, "{{field:Field{i}}} ");
            acc
        });
        let mut m = form(&src);
        let values: Vec<String> = (0..16)
            .map(|i| if i == 15 { "   ".into() } else { "x".into() })
            .collect();
        m.set_values(&values);
        assert_eq!(m.first_missing(), Some((15, Missing::Empty)));
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

    #[test]
    fn library_13_an_empty_default_makes_any_text_field_optional() {
        let m = form("{field:CC|} {multiline:Note|} {number:Qty|} {pickdate:Due|%F}");
        assert!(m.fields().iter().all(|f| !f.required), "{:?}", m.fields());
        assert_eq!(m.first_missing(), None);
        assert_eq!(preview(&m), "   2026-10-18");
        // Without the bar they are still required.
        let m = form("{field:CC} {multiline:Note} {number:Qty}");
        assert_eq!(m.missing().len(), 3);
    }

    #[test]
    fn library_14_a_required_field_in_an_unticked_section_does_not_stop_the_form() {
        let mut m = form("Hi {field:Name}{optional:Why}, because {field:Reason}{/optional}.");
        assert_eq!(m.fields().len(), 3);
        assert_eq!(
            m.fields().iter().map(|f| f.enabled).collect::<Vec<_>>(),
            [true, true, false],
            "the Reason field is off while Why is unticked"
        );
        m.set_value(0, "Ada");
        assert_eq!(m.first_missing(), None, "Reason is off, so not missing");
        m.set_value(1, TOGGLE_ON);
        assert!(m.fields()[2].enabled);
        assert_eq!(m.first_missing(), Some((2, Missing::Empty)));
        m.set_value(2, "tea");
        assert_eq!(m.first_missing(), None);
        assert_eq!(preview(&m), "Hi Ada, because tea.");
        // Unticked again: the answer stays typed but is off, and nothing is missing.
        m.set_value(2, "");
        m.set_value(1, "");
        assert!(!m.fields()[2].enabled);
        assert_eq!(m.first_missing(), None);
    }

    #[test]
    fn library_14_every_missing_field_is_listed_not_only_the_first() {
        let m = form("{field:A} {field:B|x} {field:C}");
        assert_eq!(m.missing(), [(0, Missing::Empty), (2, Missing::Empty)]);
    }

    #[test]
    fn library_18_a_date_is_typed_in_the_locales_order_with_its_hint() {
        let mut de = form_in("{pickdate:Due|%F}", "de");
        assert_eq!(de.date_hint(), "DD.MM.YYYY");
        de.set_value(0, "17.10.2026");
        assert_eq!(de.first_missing(), None);
        assert_eq!(preview(&de), "2026-10-17");
        de.set_value(0, "30.02.2026");
        assert_eq!(de.first_missing(), Some((0, Missing::BadDate)));
        // ISO is accepted in every locale.
        de.set_value(0, "2026-10-17");
        assert_eq!(de.first_missing(), None);

        let mut us = form_in("{pickdate:Due|%F}", "en-US");
        assert_eq!(us.date_hint(), "MM/DD/YYYY");
        us.set_value(0, "10/17/2026");
        assert_eq!(preview(&us), "2026-10-17");
        us.set_value(0, "17/10/2026");
        assert_eq!(us.first_missing(), Some((0, Missing::BadDate)));
    }

    #[test]
    fn library_20_prompt_names_that_differ_in_case_or_spaces_are_one_field() {
        let mut m = form("{field:Name} and {field: name } and {field:NAME}");
        assert_eq!(m.fields().len(), 1);
        m.set_value(0, "Ada");
        assert_eq!(preview(&m), "Ada and Ada and Ada");
    }

    #[test]
    fn library_21_sixty_four_prompts_make_sixty_four_fields() {
        let src: String = (1..=64)
            .map(|i| ["{field:F", &i.to_string(), "} "].concat())
            .collect();
        let mut m = form(&src);
        assert_eq!(m.fields().len(), 64);
        let values: Vec<String> = (0..64)
            .map(|i| if i == 63 { String::new() } else { "x".into() })
            .collect();
        m.set_values(&values);
        assert_eq!(m.first_missing(), Some((63, Missing::Empty)));
    }

    #[test]
    fn library_22_a_starred_choice_starts_there_and_none_is_selectable() {
        let mut m = form("{choice:Size|S|*M|L} {choice:Tone||warm}");
        let f = m.fields();
        assert_eq!((f[0].value.as_str(), f[0].choice_index), ("M", 1));
        assert_eq!((f[1].value.as_str(), f[1].choice_index), ("", 0));
        assert_eq!(f[1].options, ["", "warm"]);
        m.set_value(1, "warm");
        assert_eq!(m.fields()[1].choice_index, 1);
        m.set_value(1, "");
        assert_eq!(m.fields()[1].choice_index, 0, "the none option is a choice");
        assert_eq!(preview(&m), "M ");
    }

    #[test]
    fn library_23_a_number_field_accepts_either_decimal_mark_and_rejects_text() {
        let mut m = form("{number:Qty|2} x {number:Price}");
        assert_eq!(m.fields()[0].kind, FieldKind::Number);
        assert_eq!(m.fields()[0].value, "2");
        assert!(!m.fields()[0].required && m.fields()[1].required);
        m.set_value(1, "3,5");
        assert_eq!(m.first_missing(), None);
        m.set_value(1, "three");
        assert_eq!(m.first_missing(), Some((1, Missing::BadNumber)));
    }

    #[test]
    fn library_23_the_selection_shows_in_the_preview() {
        let m = form("Re: {selection|upper}").with_selection(Some("topic".into()));
        assert_eq!(preview(&m), "Re: TOPIC");
        assert_eq!(m.selection(), Some("topic"));
    }

    #[test]
    fn library_18_a_picked_day_is_written_in_the_locales_order_and_marked_again() {
        let mut de = form_in("{pickdate:Due|%F}", "de");
        assert_eq!(de.date_parts(0), None);
        assert_eq!(de.pick_date(0, 2026, 10, 7).as_deref(), Some("07.10.2026"));
        assert_eq!(de.fields()[0].value, "07.10.2026");
        assert_eq!(de.date_parts(0), Some((2026, 10, 7)));
        assert_eq!(de.pick_date(0, 2026, 2, 30), None, "not a day");
        assert_eq!(de.fields()[0].value, "07.10.2026");
        let mut us = form_in("{pickdate:Due|%F} {field:Name}", "en-US");
        assert_eq!(us.pick_date(0, 2026, 10, 7).as_deref(), Some("10/07/2026"));
        assert_eq!(
            us.pick_date(1, 2026, 10, 7),
            None,
            "a text field is not a date"
        );
        assert_eq!(us.date_parts(1), None);
        // What is typed and not a day marks nothing.
        us.set_value(0, "31/31/2026");
        assert_eq!(us.date_parts(0), None);
    }
}
