//! Importing a folder of `.txt` / `.md` files, one item per file.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use snipdeck_core::Timestamp;

use crate::error::{Error, Result};
use crate::import::{ImportOptions, board_name, finish};
use crate::limits::TEXT_LIMIT;
use crate::plan::{Library, Plan, PlanBoard, PlanItem, PlanList};
use crate::report::{ImportReport, Warning};

use super::{MEMORY_BUDGET, clean_label, decode_text, note_lossy, title_from};

/// Imports every `.txt` and `.md` file (any letter case) directly in `folder` as one item each,
/// into a new board named after the folder.
///
/// * The item title is the file name without its extension.
/// * Files directly in `folder` form a list named after the folder; each immediate subfolder
///   becomes its own list of its files. Deeper folders are not visited.
/// * Text is read as UTF-8 (a BOM is removed). UTF-16 files with a BOM are converted. Other bytes
///   are replaced lossily and a [`Warning::LossyText`] says so. Files with NUL bytes are skipped
///   as binary.
/// * A file over 5 MB (SPEC §5.4) is skipped with a warning, as is an empty one. Names starting
///   with a dot, symbolic links and anything that is not a regular file are not read.
/// * Order is the file name, ignoring case.
///
/// # Errors
///
/// [`Error::File`] if `folder` cannot be listed, and storage errors. Individual unreadable files
/// are warnings.
pub fn import_folder<L: Library>(
    lib: &mut L,
    folder: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let mut state = Reader {
        budget: MEMORY_BUDGET,
        files_left: options.limits.max_items,
        stopped: false,
    };

    let folder_label = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let board = board_name(
        options.board_name.as_deref().unwrap_or(&folder_label),
        "Imported notes",
    )?;

    let (files, dirs) = list(folder, &mut report)?;
    let mut lists = Vec::new();
    let root = read_list(&mut state, &files, board.as_str(), now, &mut report)?;
    lists.extend(root);
    for dir in dirs {
        if state.stopped {
            break;
        }
        let label = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (files, _) = list(&dir, &mut report)?;
        let list_name = board_name(&label, "Notes")?;
        lists.extend(read_list(
            &mut state,
            &files,
            list_name.as_str(),
            now,
            &mut report,
        )?);
    }

    let mut plan = Plan::default();
    if !lists.is_empty() {
        plan.boards.push(PlanBoard {
            name: board,
            created_at: now,
            updated_at: now,
            lists,
        });
    }
    finish(lib, plan, options, report)
}

struct Reader {
    budget: u64,
    files_left: usize,
    stopped: bool,
}

/// The importable files and the subfolders directly inside `dir`, each sorted by name.
fn list(dir: &Path, report: &mut ImportReport) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let entries = fs::read_dir(dir).map_err(|source| Error::File {
        path: dir.to_owned(),
        source,
    })?;
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    for entry in entries {
        let entry = entry.map_err(|source| Error::File {
            path: dir.to_owned(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // `file_type` does not follow symbolic links.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_symlink() {
            if has_text_extension(&path) {
                report.items_skipped += 1;
                report.warn(Warning::SymlinkSkipped { name });
            }
        } else if kind.is_file() {
            if has_text_extension(&path) {
                files.push(path);
            }
        } else if kind.is_dir() {
            dirs.push(path);
        }
    }
    let key = |p: &PathBuf| {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (name.to_lowercase(), name)
    };
    files.sort_by_key(key);
    dirs.sort_by_key(key);
    Ok((files, dirs))
}

fn has_text_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("txt") || e.eq_ignore_ascii_case("md"))
}

fn read_list(
    state: &mut Reader,
    files: &[PathBuf],
    list_name: &str,
    now: Timestamp,
    report: &mut ImportReport,
) -> Result<Option<PlanList>> {
    let mut items = Vec::new();
    for path in files {
        if state.stopped {
            break;
        }
        if state.files_left == 0 {
            state.stopped = true;
            report.warn(Warning::Truncated {
                reason: "too many files".to_owned(),
            });
            break;
        }
        state.files_left -= 1;
        if let Some(item) = read_file(state, path, now, report) {
            items.push(item);
        } else {
            report.items_skipped += 1;
        }
    }
    if items.is_empty() {
        return Ok(None);
    }
    let mut list = PlanList::new(board_name(list_name, "Notes")?, now);
    list.items = items;
    Ok(Some(list))
}

/// Reads one file into an item, or warns and returns `None`.
fn read_file(
    state: &mut Reader,
    path: &Path,
    now: Timestamp,
    report: &mut ImportReport,
) -> Option<PlanItem> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut unreadable = |reason: String| {
        report.warn(Warning::Unreadable {
            name: name.clone(),
            reason,
        });
    };

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            unreadable(e.to_string());
            return None;
        }
    };
    // Re-check on the open handle: the path may have changed since it was listed.
    let size = match file.metadata() {
        Ok(m) if m.is_file() => m.len(),
        Ok(_) => {
            unreadable("not a regular file".to_owned());
            return None;
        }
        Err(e) => {
            unreadable(e.to_string());
            return None;
        }
    };
    if size > TEXT_LIMIT {
        report.warn(Warning::TooLarge { name, size });
        return None;
    }
    if size > state.budget {
        state.stopped = true;
        report.warn(Warning::Truncated {
            reason: "reached the import memory limit".to_owned(),
        });
        return None;
    }

    // Read at most one byte over the limit: the file may have grown since `metadata`.
    let mut bytes = Vec::new();
    if let Err(e) = file.take(TEXT_LIMIT + 1).read_to_end(&mut bytes) {
        unreadable(e.to_string());
        return None;
    }
    if bytes.len() as u64 > TEXT_LIMIT {
        report.warn(Warning::TooLarge {
            name,
            size: bytes.len() as u64,
        });
        return None;
    }
    state.budget = state.budget.saturating_sub(bytes.len() as u64);

    let is_utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    if !is_utf16 && bytes.contains(&0) {
        unreadable("it looks like a binary file".to_owned());
        return None;
    }
    let (text, how) = decode_text(&bytes);
    if text.trim().is_empty() {
        report.warn(Warning::Empty { name });
        return None;
    }
    note_lossy(how, &name, report);

    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut title = clean_label(&stem, 200);
    if title.is_empty() {
        title = title_from(&text);
    }
    Some(PlanItem::text(title, text, now))
}
