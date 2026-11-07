//! Importing from Espanso.
//!
//! Reads Espanso's YAML match files: one `.yml` / `.yaml` file, or a folder (Espanso's config
//! folder, whose `match` subfolder is read, or a `match` folder itself), searched recursively
//! up to a fixed depth and file count. Files whose name starts with `_` are disabled in
//! Espanso and are skipped with a note; symbolic links are not followed. A file that is not
//! valid YAML, or has no `matches` list, is skipped with a warning and the others are still
//! read. Each file becomes a list named after its path under the folder (`work / mail`).
//!
//! # Mapping
//!
//! A match is read from `trigger` or `triggers` (the abbreviations, shown in the title; the
//! optional `label` is the title) and its text from `replace`, `form`, `markdown` or `html`.
//! Matches with only `regex` triggers, or with no text (`image_path`, nothing), are skipped
//! with a note. `markdown` and `html` text is kept as written.
//!
//! `{{name}}` references map where Snipdeck has a direct equivalent, looked up in the match's
//! `vars` and then the file's `global_vars`:
//!
//! | Espanso | Snipdeck |
//! |---|---|
//! | `date` var with `format` | `{datetime:FORMAT}` if the format is one Snipdeck can parse |
//! | `clipboard` var | `{clipboard}` |
//! | `echo` var | its text, inline |
//! | `choice` var with `values` | `{choice:name\|a\|b}` (Supporter) |
//! | `$\|$` | `{cursor}` |
//! | `form` text `[[field]]` | `{field:field}` / `{field:field\|default}`, or `{choice:...}` for a `choice` / `list` field with `values` (Supporter) |
//!
//! Anything else (`shell`, `script`, `random`, an unknown name, a date format Snipdeck does not
//! know) is kept in the text as written and the report gets a note. A match whose placeholders
//! do not fit one Snipdeck template (a second `$|$`) is kept as plain text, also with a note.
//!
//! # Hostile input
//!
//! Files over the JSON size limit (`Limits::max_json_bytes`) are skipped. YAML is read through
//! `serde_yaml_ng`, whose parser limits nesting depth; aliases are expanded as the parser
//! reads, so the expansion is charged against a budget of four times the file size (plus a
//! floor of 1 MiB) and a billion-laughs file fails early instead of growing.
//!
//! Written from Espanso's public documentation (v2 match syntax); not tested against a real
//! Espanso install.

use std::cell::Cell;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use snipdeck_core::Timestamp;
use snipdeck_core::template::Template;

use crate::error::{Error, Result};
use crate::import::ImportOptions;
use crate::plan::Library;
use crate::report::{ImportReport, Warning};

use super::expander::{Body, Snip, Tpl, import_snips, placeholder_name, read_capped};
use super::{MEMORY_BUDGET, clean_label, decode_text, note_lossy};

/// Most files read from a folder.
const MAX_FILES: usize = 1_000;
/// Deepest folder level searched below the start.
const MAX_DEPTH: usize = 8;

/// Imports Espanso match files from `path` (a file or a folder) into a new board named
/// `Imported from Espanso` (unless the options say otherwise), one list per file.
///
/// # Errors
///
/// [`Error::File`] if `path` cannot be read, and storage errors. Unreadable or malformed files,
/// and matches that cannot be imported, are skipped with a warning.
pub fn import_espanso<L: Library>(
    lib: &mut L,
    path: &Path,
    now: Timestamp,
    options: &ImportOptions<'_>,
) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    let meta = fs::symlink_metadata(path).map_err(|source| Error::File {
        path: path.to_owned(),
        source,
    })?;
    let (root, files) = if meta.is_dir() {
        let match_dir = path.join("match");
        let root = if match_dir.is_dir() {
            match_dir
        } else {
            path.to_owned()
        };
        let files = find_files(&root, &mut report);
        (root, files)
    } else {
        let parent = path.parent().unwrap_or(Path::new("")).to_owned();
        (parent, vec![path.to_owned()])
    };

    let mut snips = Vec::new();
    // Bytes still allowed across all files, so a folder of many big files stays bounded.
    let mut remaining = MEMORY_BUDGET.min(options.limits.max_total_bytes);
    for file in files {
        let label = list_label(&root, &file);
        if snips.len() >= options.limits.max_items {
            truncated(&mut report);
            break;
        }
        if remaining == 0 {
            report.warn(Warning::Truncated {
                reason: "reached the import memory limit".to_owned(),
            });
            break;
        }
        read_file(
            &file,
            &label,
            options,
            &mut remaining,
            &mut snips,
            &mut report,
        );
    }
    import_snips(
        lib,
        snips,
        ("Imported from Espanso", "Espanso"),
        now,
        options,
        report,
    )
}

fn truncated(report: &mut ImportReport) {
    report.warn(Warning::Truncated {
        reason: "too many snippets".to_owned(),
    });
}

fn list_label(root: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let parts: Vec<String> = rel
        .with_extension("")
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    clean_label(&parts.join(" / "), 100)
}

/// The `.yml` / `.yaml` files under `dir`, sorted, without links.
fn find_files(dir: &Path, report: &mut ImportReport) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(dir.to_owned(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            report.warn(Warning::Unreadable {
                name: current.display().to_string(),
                reason: "the folder cannot be read".to_owned(),
            });
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                report.warn(Warning::SymlinkSkipped { name });
            } else if kind.is_dir() {
                if depth < MAX_DEPTH && !name.starts_with('.') {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                let is_yaml = Path::new(&name).extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("yml") || e.eq_ignore_ascii_case("yaml")
                });
                if !is_yaml {
                    continue;
                }
                if name.starts_with('_') {
                    report.warn(Warning::Note(format!(
                        "{name} is disabled in Espanso (its name starts with _) and was skipped"
                    )));
                } else {
                    found.push(entry.path());
                }
            }
        }
    }
    found.sort();
    if found.len() > MAX_FILES {
        found.truncate(MAX_FILES);
        report.warn(Warning::Truncated {
            reason: format!("only the first {MAX_FILES} files were read"),
        });
    }
    found
}

fn read_file(
    path: &Path,
    label: &str,
    options: &ImportOptions<'_>,
    remaining: &mut u64,
    snips: &mut Vec<Snip>,
    report: &mut ImportReport,
) {
    let name = path
        .file_name()
        .map_or_else(|| label.to_owned(), |n| n.to_string_lossy().into_owned());
    let cap = options.limits.max_json_bytes;
    let size = fs::metadata(path).map_or(0, |m| m.len());
    if size > cap {
        report.items_skipped += 1;
        report.warn(Warning::TooLarge { name, size });
        return;
    }
    if size > *remaining {
        *remaining = 0;
        report.warn(Warning::Truncated {
            reason: "reached the import memory limit".to_owned(),
        });
        return;
    }
    let bytes = match read_capped(path, "an Espanso file", options) {
        Ok(b) => b,
        Err(Error::LimitExceeded { limit, .. }) => {
            report.items_skipped += 1;
            report.warn(Warning::TooLarge {
                name,
                size: limit.saturating_add(1),
            });
            return;
        }
        Err(e) => {
            report.warn(Warning::Unreadable {
                name,
                reason: match e {
                    Error::File { source, .. } => source.kind().to_string(),
                    _ => "it cannot be read".to_owned(),
                },
            });
            return;
        }
    };
    *remaining = remaining.saturating_sub(bytes.len() as u64);
    let (text, how) = decode_text(&bytes);
    note_lossy(how, &name, report);
    let budget = Cell::new(bytes.len().saturating_mul(4).max(1 << 20));
    drop(bytes);
    let root = match parse_yaml(&text, &budget) {
        Ok(node) => node,
        Err(reason) => {
            report.warn(Warning::Unreadable { name, reason });
            return;
        }
    };
    let Some(matches) = root.get("matches").and_then(Node::as_seq) else {
        report.warn(Warning::Unreadable {
            name,
            reason: "no `matches` list".to_owned(),
        });
        return;
    };
    let globals = root
        .get("global_vars")
        .and_then(Node::as_seq)
        .unwrap_or(&[]);
    for (index, m) in matches.iter().enumerate() {
        if snips.len() >= options.limits.max_items {
            truncated(report);
            return;
        }
        let what = format!("{name}, match {}", index + 1);
        match read_match(m, globals, &what, report) {
            Some(mut snip) => {
                snip.group = Some(label.to_owned());
                snips.push(snip);
            }
            None => report.items_skipped += 1,
        }
    }
}

// ----------------------------------------------------------------- the YAML tree

/// A YAML value, scalars as text.
enum Node {
    Null,
    Scalar(String),
    Seq(Vec<Node>),
    Map(Vec<(Node, Node)>),
}

impl Node {
    fn get(&self, key: &str) -> Option<&Node> {
        let Self::Map(pairs) = self else { return None };
        pairs
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::Scalar(s) => Some(s),
            _ => None,
        }
    }

    fn as_seq(&self) -> Option<&[Node]> {
        match self {
            Self::Seq(items) => Some(items),
            _ => None,
        }
    }

    fn str_of(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    /// Strings of a sequence (non-string items dropped), or the single string.
    fn strings(&self) -> Vec<&str> {
        match self {
            Self::Scalar(s) => vec![s],
            Self::Seq(items) => items.iter().filter_map(Node::as_str).collect(),
            _ => Vec::new(),
        }
    }
}

/// Builds a [`Node`] and charges every node and scalar byte against `budget`.
#[derive(Clone, Copy)]
struct Seed<'a> {
    budget: &'a Cell<usize>,
}

impl Seed<'_> {
    fn charge<E: de::Error>(self, cost: usize) -> std::result::Result<(), E> {
        let left = self.budget.get();
        if cost > left {
            self.budget.set(0);
            return Err(E::custom(
                "the file expands to more than four times its size",
            ));
        }
        self.budget.set(left - cost);
        Ok(())
    }
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Node;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Node, D::Error> {
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a YAML value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Node, E> {
        self.charge(16)?;
        Ok(Node::Scalar(v.to_string()))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Node, E> {
        self.charge(32)?;
        Ok(Node::Scalar(v.to_string()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Node, E> {
        self.charge(32)?;
        Ok(Node::Scalar(v.to_string()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Node, E> {
        self.charge(32)?;
        Ok(Node::Scalar(v.to_string()))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Node, E> {
        self.charge(16 + v.len())?;
        Ok(Node::Scalar(v.to_owned()))
    }

    fn visit_unit<E: de::Error>(self) -> std::result::Result<Node, E> {
        self.charge(16)?;
        Ok(Node::Null)
    }

    fn visit_none<E: de::Error>(self) -> std::result::Result<Node, E> {
        self.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> std::result::Result<Node, D::Error> {
        self.deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Node, A::Error> {
        self.charge(16)?;
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self)? {
            items.push(item);
        }
        Ok(Node::Seq(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Node, A::Error> {
        self.charge(16)?;
        let mut pairs = Vec::new();
        while let Some(key) = map.next_key_seed(self)? {
            pairs.push((key, map.next_value_seed(self)?));
        }
        Ok(Node::Map(pairs))
    }
}

/// Parses one YAML document; the reason is safe to show (no content quoted).
fn parse_yaml(text: &str, budget: &Cell<usize>) -> std::result::Result<Node, String> {
    let de = serde_yaml_ng::Deserializer::from_str(text);
    Seed { budget }.deserialize(de).map_err(|e| {
        if budget.get() == 0 {
            "it expands to far more than its size (alias bomb?)".to_owned()
        } else {
            format!(
                "it is not valid YAML ({})",
                e.to_string().lines().next().unwrap_or("")
            )
        }
    })
}

// ----------------------------------------------------------------- one match

/// What a `{{name}}` stands for.
enum Var {
    Placeholder(String),
    Literal(String),
}

fn read_match(m: &Node, globals: &[Node], what: &str, report: &mut ImportReport) -> Option<Snip> {
    let note = |report: &mut ImportReport, msg: &str| {
        report.warn(Warning::Note(format!("{what}: {msg}")));
    };
    if !matches!(m, Node::Map(_)) {
        note(report, "is not a match; skipped");
        return None;
    }
    let mut triggers: Vec<&str> = m.get("trigger").map(Node::strings).unwrap_or_default();
    triggers.extend(m.get("triggers").map(Node::strings).unwrap_or_default());
    triggers.retain(|t| !t.trim().is_empty());
    if triggers.is_empty() {
        if m.get("regex").is_some() || m.get("regexes").is_some() {
            note(
                report,
                "has only a regex trigger, which Snipdeck cannot use; skipped",
            );
        } else {
            note(report, "has no trigger; skipped");
        }
        return None;
    }
    let label = m.str_of("label").map(str::to_owned);
    let abbreviation = Some(triggers.join(", "));
    let vars: Vec<&Node> = m
        .get("vars")
        .and_then(Node::as_seq)
        .unwrap_or(&[])
        .iter()
        .chain(globals)
        .collect();

    let body = if let Some(text) = m.str_of("replace") {
        convert(text, &vars, what, report)
    } else if let Some(form) = m.str_of("form") {
        convert_form(form, m.get("form_fields"), what, report)
    } else if let Some(text) = m.str_of("markdown").or_else(|| m.str_of("html")) {
        note(report, "markdown/html text is kept as written");
        Body::plain(text)
    } else {
        note(report, "has no text (image or other content); skipped");
        return None;
    };
    Some(Snip {
        group: None,
        name: label,
        abbreviation,
        body,
    })
}

fn var_for(name: &str, vars: &[&Node], what: &str, report: &mut ImportReport) -> Option<Var> {
    let var = vars.iter().find(|v| v.str_of("name") == Some(name))?;
    let params = var.get("params");
    let param = |key: &str| params.and_then(|p| p.str_of(key));
    match var.str_of("type")? {
        "clipboard" => Some(Var::Placeholder("{clipboard}".to_owned())),
        "echo" => param("echo").map(|t| Var::Literal(t.to_owned())),
        "date" => {
            let format = param("format")?;
            let placeholder = format!("{{datetime:{format}}}");
            // A brace in the format would close the placeholder early and leave live ones.
            if !format.contains(['{', '}']) && Template::parse(&placeholder).is_ok() {
                Some(Var::Placeholder(placeholder))
            } else {
                report.warn(Warning::Note(format!(
                    "{what}: the date format {format} is not one Snipdeck knows; left as written"
                )));
                None
            }
        }
        "choice" => {
            let values: Vec<&str> = params?.get("values")?.strings();
            choice(name, &values).map(Var::Placeholder)
        }
        _ => None,
    }
}

fn choice(name: &str, values: &[&str]) -> Option<String> {
    let name = placeholder_name(name)?;
    let mut options: Vec<String> = Vec::new();
    for v in values {
        let v = placeholder_name(v)?;
        if !options.contains(&v) {
            options.push(v);
        }
    }
    (options.len() >= 2).then(|| format!("{{choice:{name}|{}}}", options.join("|")))
}

/// `replace` text: `{{name}}` references and `$|$`.
fn convert(src: &str, vars: &[&Node], what: &str, report: &mut ImportReport) -> Body {
    let mut tpl = Tpl::default();
    let mut rest = src;
    let mut unmapped: Vec<String> = Vec::new();
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        literal(&mut tpl, &rest[..start]);
        match var_for(name, vars, what, report) {
            Some(Var::Placeholder(p)) => tpl.ph(&p),
            Some(Var::Literal(t)) => tpl.lit(&t),
            None => {
                tpl.lit(&rest[start..start + 2 + end + 2]);
                if !unmapped.iter().any(|u| u == name) {
                    unmapped.push(name.to_owned());
                }
            }
        }
        rest = &after[end + 2..];
    }
    literal(&mut tpl, rest);
    if !unmapped.is_empty() {
        report.warn(Warning::Note(format!(
            "{what}: no Snipdeck equivalent for {{{{{}}}}} (shell, script, random or unknown); left as written",
            unmapped.join("}}, {{")
        )));
    }
    tpl.finish(src, what, report)
}

/// A literal run, with Espanso's cursor mark turned into `{cursor}`.
fn literal(tpl: &mut Tpl, text: &str) {
    let mut parts = text.split("$|$");
    if let Some(first) = parts.next() {
        tpl.lit(first);
    }
    for part in parts {
        tpl.ph("{cursor}");
        tpl.lit(part);
    }
}

/// `form` text: `[[field]]` references.
fn convert_form(src: &str, fields: Option<&Node>, what: &str, report: &mut ImportReport) -> Body {
    let mut tpl = Tpl::default();
    let mut rest = src;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let name = after[..end].trim();
        let field = fields.and_then(|f| f.get(name));
        let values: Vec<&str> = field
            .and_then(|f| f.get("values"))
            .map(Node::strings)
            .unwrap_or_default();
        let default = field.and_then(|f| f.str_of("default"));
        let placeholder = if values.is_empty() {
            placeholder_name(name).map(|n| match default.and_then(placeholder_name) {
                Some(d) => format!("{{field:{n}|{d}}}"),
                None => format!("{{field:{n}}}"),
            })
        } else {
            choice(name, &values)
        };
        tpl.lit(&rest[..start]);
        if let Some(p) = placeholder {
            tpl.ph(&p);
        } else {
            tpl.lit(&rest[start..start + 2 + end + 2]);
            report.warn(Warning::Note(format!(
                "{what}: the form field [[{name}]] has no Snipdeck equivalent; left as written"
            )));
        }
        rest = &after[end + 2..];
    }
    tpl.lit(rest);
    tpl.finish(src, what, report)
}
