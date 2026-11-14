//! Importing a clipboard history that the caller has already read, such as the current session
//! of the Windows clipboard history (Win+V).
//!
//! The source is passed in memory as [`HistoryEntry`] values, so this module has no platform
//! code and is tested everywhere. Reading the history itself is `snipdeck-platform`'s job.

use snipdeck_core::Timestamp;

use crate::error::Result;
use crate::import::{ImportOptions, board_name, finish};
use crate::limits::TEXT_LIMIT;
use crate::plan::{Library, Plan, PlanBoard, PlanList};
use crate::report::{ImportReport, Warning};

use super::{MEMORY_BUDGET, title_from};

/// One text entry of a clipboard history, newest or oldest first as the caller lists them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// The entry's text.
    pub text: String,
    /// When the entry was copied, if the source says.
    pub timestamp: Option<Timestamp>,
}

/// Imports `entries` as one new board (named by `options.board_name`, else `default_board_name`,
/// else "Windows clipboard history") with one list and one item per entry.
///
/// * Order is kept as given. Blank entries are skipped with a [`Warning::Empty`].
/// * An entry over 5 MB is skipped with a [`Warning::TooLarge`], like a file in
///   [`import_folder`](crate::import_folder). Reaching [`MEMORY_BUDGET`] or
///   `options.limits.max_items` stops the read with a [`Warning::Truncated`].
/// * The item title is the first non-blank line. An entry's timestamp, when present, is the
///   item's creation time; otherwise `now` is.
///
/// # Errors
///
/// Storage errors, and a board name that cannot be used.
pub fn import_entries<L: Library>(
    lib: &mut L,
    entries: &[HistoryEntry],
    default_board_name: &str,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let board = board_name(
        options.board_name.as_deref().unwrap_or(default_board_name),
        "Windows clipboard history",
    )?;
    let mut budget = MEMORY_BUDGET;
    let mut items = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let label = format!("entry {}", index + 1);
        if items.len() >= options.limits.max_items {
            report.warn(Warning::Truncated {
                reason: "too many entries".to_owned(),
            });
            break;
        }
        let size = entry.text.len() as u64;
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
        if entry.text.trim().is_empty() {
            report.items_skipped += 1;
            report.warn(Warning::Empty { name: label });
            continue;
        }
        let at = entry.timestamp.unwrap_or(now);
        items.push(crate::plan::PlanItem::text(
            title_from(&entry.text),
            entry.text.clone(),
            at,
        ));
    }

    let mut plan = Plan::default();
    if !items.is_empty() {
        let mut list = PlanList::new(board.clone(), now);
        list.items = items;
        plan.boards.push(PlanBoard {
            name: board,
            created_at: now,
            updated_at: now,
            lists: vec![list],
        });
    }
    finish(lib, plan, options, report)
}
