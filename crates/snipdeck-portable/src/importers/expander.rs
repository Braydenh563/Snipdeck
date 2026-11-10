//! What the text-expander importers (Espanso, Beeftext, `TextExpander`, `PhraseExpress`) share:
//! the intermediate [`Snip`], a builder that turns a source's variables into Snipdeck
//! placeholders, and the step that turns snips into a board of lists.
//!
//! # Mapping
//!
//! * A group or folder becomes a list on one new board; snips with no group go to one default
//!   list. Groups that differ only in case or spacing are one list.
//! * The abbreviation becomes the item's typed trigger (`;hi`); the title is the snip's name,
//!   or the first line of the text. A snip with several abbreviations (Espanso) keeps the first
//!   that is a valid trigger and the report names the rest. An abbreviation that cannot be a
//!   trigger (spaces around or inside it, over 32 characters, or an Espanso `word` match) stays in the title after the name, in parentheses
//!   (`Greeting (my greeting)`), with a note, so it is not lost. A trigger already taken by
//!   another snip, or by the library, is dropped with a note when the item is written.
//! * Only variables with a direct Snipdeck equivalent become placeholders (`{date}`, `{time}`,
//!   `{datetime:FORMAT}`, `{clipboard}`, `{cursor}`, `{field:Name}`, `{choice:Name|a|b}`); the
//!   snip is then a template. Any other variable stays in the text as written and the report
//!   gets a note, so nothing is silently lost. A snip with no placeholder is plain text,
//!   stored exactly as the source had it.
//! * Size limits are the same as the other importers: 5 MB per snip, [`MEMORY_BUDGET`] in all,
//!   `options.limits.max_items` snips.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use snipdeck_core::ItemKind;
use snipdeck_core::Timestamp;
use snipdeck_core::Trigger;
use snipdeck_core::template::{Template, escape};
use snipdeck_store::name_key;

use crate::error::{Error, Result};
use crate::import::{ImportOptions, board_name, finish};
use crate::limits::TEXT_LIMIT;
use crate::plan::{Library, Plan, PlanBoard, PlanItem, PlanList};
use crate::report::{ImportReport, Warning};

use super::{MEMORY_BUDGET, clean_label, title_from, truncate_chars};

/// Longest title, in characters (as for the other importers).
const TITLE_CHARS: usize = 80;
/// Longest list name read from a group, in characters.
const LIST_CHARS: usize = 100;
/// Most distinct lists one import creates; later groups share the default list.
const MAX_LISTS: usize = 1_000;

/// The text of a snip, as stored.
pub(crate) struct Body {
    pub text: String,
    /// Whether `text` holds Snipdeck placeholders (and so braces are escaped).
    pub template: bool,
}

impl Body {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            template: false,
        }
    }
}

/// One snippet read from another tool.
pub(crate) struct Snip {
    pub group: Option<String>,
    pub name: Option<String>,
    /// What the source calls the typed shortcuts, in its order.
    pub abbreviations: Vec<String>,
    /// Whether the source fires them the way Snipdeck does, on the last typed character. False
    /// for Espanso `word` and `right_word` matches, which wait for a separator.
    pub typed: bool,
    pub body: Body,
}

/// Builds a [`Body`] from literal runs and placeholders.
#[derive(Default)]
pub(crate) struct Tpl {
    out: String,
    placeholders: usize,
}

impl Tpl {
    pub(crate) fn lit(&mut self, text: &str) {
        self.out.push_str(&escape(text));
    }

    /// Adds a placeholder, written as Snipdeck spells it (`{date}`).
    pub(crate) fn ph(&mut self, placeholder: &str) {
        self.out.push_str(placeholder);
        self.placeholders += 1;
    }

    /// The finished body. With no placeholder it is `original` as it was (no escaping). If the
    /// result does not parse as a template (two `{cursor}`s, say) the snip is kept as
    /// `original` text with a note.
    pub(crate) fn finish(self, original: &str, label: &str, report: &mut ImportReport) -> Body {
        if self.placeholders == 0 {
            return Body::plain(original);
        }
        if Template::parse(&self.out).is_ok() {
            Body {
                text: self.out,
                template: true,
            }
        } else {
            report.warn(Warning::Note(format!(
                "{label}: its variables do not fit a Snipdeck template together (for example two cursor marks); kept as plain text"
            )));
            Body::plain(original)
        }
    }
}

/// A name usable inside `{field:...}` / `{choice:...}`: no braces or bars, no control
/// characters, trimmed and short. `None` when nothing is left.
pub(crate) fn placeholder_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '{' | '}' | '|'))
        .collect();
    let cleaned = truncate_chars(cleaned.trim(), 40);
    let cleaned = cleaned.trim().to_owned();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Reads `path` whole, refusing files over the JSON size limit (and never more than
/// [`MEMORY_BUDGET`]).
pub(crate) fn read_capped(
    path: &Path,
    what: &'static str,
    options: &ImportOptions<'_>,
) -> Result<Vec<u8>> {
    let cap = options.limits.max_json_bytes.min(MEMORY_BUDGET);
    let file_error = |source| Error::File {
        path: path.to_owned(),
        source,
    };
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(file_error)?
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(file_error)?;
    if bytes.len() as u64 > cap {
        return Err(Error::LimitExceeded { what, limit: cap });
    }
    Ok(bytes)
}

/// The trigger for a snip: its first abbreviation that is a valid [`Trigger`]. Notes go to
/// `report`: one for each other abbreviation that was not kept.
fn trigger_of(snip: &Snip, label: &str, report: &mut ImportReport) -> Option<Trigger> {
    let mut chosen: Option<Trigger> = None;
    let mut unused = Vec::new();
    for raw in &snip.abbreviations {
        if raw.is_empty() {
            continue;
        }
        // Taken as written: a space around it is part of the source's trigger, so it is not one.
        match Trigger::new(raw) {
            Ok(t) if chosen.is_none() && snip.typed => chosen = Some(t),
            _ => unused.push(clean_label(raw, 40)),
        }
    }
    if !unused.is_empty() {
        let why = if chosen.is_some() {
            "Snipdeck keeps one trigger per snippet"
        } else if !snip.typed {
            "it fires after a separator, which a Snipdeck trigger does not wait for"
        } else {
            "a trigger has no spaces and at most 32 characters"
        };
        report.warn(Warning::Note(format!(
            "{label}: `{}` was not kept as a trigger ({why})",
            unused.join("`, `")
        )));
    }
    chosen
}

/// The name, or the first line of the text; an abbreviation that did not become a trigger is
/// appended to the name in parentheses.
fn title(snip: &Snip, trigger: Option<&Trigger>) -> String {
    let name = snip
        .name
        .as_deref()
        .map(|n| clean_label(n, TITLE_CHARS))
        .unwrap_or_default();
    let lost = if trigger.is_some() {
        String::new()
    } else {
        clean_label(&snip.abbreviations.join(", "), 40)
    };
    match (name.is_empty(), lost.is_empty()) {
        (false, false) if name != lost => {
            let suffix = format!(" ({lost})");
            let room = TITLE_CHARS.saturating_sub(suffix.chars().count());
            format!("{}{suffix}", truncate_chars(&name, room))
        }
        (false, _) => name,
        (true, false) => lost,
        (true, true) => title_from(&snip.body.text),
    }
}

/// Turns `snips` into one new board and writes it. `report` carries what the caller already
/// noted.
pub(crate) fn import_snips<L: Library>(
    lib: &mut L,
    snips: Vec<Snip>,
    defaults: (&str, &str),
    now: Timestamp,
    options: &ImportOptions<'_>,
    mut report: ImportReport,
) -> Result<ImportReport> {
    let (default_board, default_list) = defaults;
    let mut budget = MEMORY_BUDGET;
    let mut lists: Vec<PlanList> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut added = 0usize;
    for (index, snip) in snips.into_iter().enumerate() {
        let label = format!("snippet {}", index + 1);
        if added >= options.limits.max_items {
            report.warn(Warning::Truncated {
                reason: "too many snippets".to_owned(),
            });
            break;
        }
        let size = snip.body.text.len() as u64;
        if size > TEXT_LIMIT {
            report.items_skipped += 1;
            report.warn(Warning::TooLarge { name: label, size });
            continue;
        }
        if size > budget {
            report.warn(Warning::Truncated {
                reason: "reached the import memory limit".to_owned(),
            });
            break;
        }
        budget -= size;
        if snip.body.text.trim().is_empty() {
            report.items_skipped += 1;
            report.warn(Warning::Empty { name: label });
            continue;
        }
        let group = snip
            .group
            .as_deref()
            .map(|g| clean_label(g, LIST_CHARS))
            .filter(|g| !g.is_empty())
            .unwrap_or_else(|| default_list.to_owned());
        let mut key = name_key(&group);
        if !by_key.contains_key(&key) && by_key.len() >= MAX_LISTS {
            key = name_key(default_list);
        }
        let slot = if let Some(slot) = by_key.get(&key) {
            *slot
        } else {
            let name = if key == name_key(&group) {
                group
            } else {
                default_list.to_owned()
            };
            lists.push(PlanList::new(board_name(&name, default_list)?, now));
            by_key.insert(key, lists.len() - 1);
            lists.len() - 1
        };
        let trigger = trigger_of(&snip, &label, &mut report);
        let mut item = PlanItem::text(title(&snip, trigger.as_ref()), snip.body.text, now);
        item.trigger = trigger;
        if snip.body.template {
            item.kind = ItemKind::Template;
        }
        lists[slot].items.push(item);
        added += 1;
    }

    let mut plan = Plan::default();
    if !lists.is_empty() {
        plan.boards.push(PlanBoard {
            name: board_name(
                options.board_name.as_deref().unwrap_or(default_board),
                default_board,
            )?,
            created_at: now,
            updated_at: now,
            lists,
        });
    }
    finish(lib, plan, options, report)
}
