//! What an import did: the same report for every importer.

use core::fmt;

use snipdeck_core::BoardId;

/// Most warnings a report keeps; the rest are only counted in
/// [`ImportReport::warnings_dropped`], so a folder of 100 000 bad files cannot exhaust memory.
pub const MAX_WARNINGS: usize = 200;

/// Something the import did not do, or did differently from what the source said. Warnings never
/// fail an import; the caller shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// The archive's minor version is newer than this build's; unknown fields were ignored.
    NewerMinorVersion {
        /// The archive's `format_version`.
        found: String,
    },
    /// The archive holds files this version does not know; they were ignored.
    UnknownEntries {
        /// How many.
        count: usize,
    },
    /// The archive holds assets no item refers to; they were ignored.
    UnreferencedAssets {
        /// How many.
        count: usize,
    },
    /// Encrypted (sensitive) items were skipped because no key was given.
    SensitiveNeedKey {
        /// How many items.
        count: usize,
    },
    /// An encrypted item could not be decrypted with the given key (another computer's key, or
    /// the file was altered), so it was skipped.
    CannotDecrypt {
        /// The item's title.
        title: String,
    },
    /// The archive was exported without its sensitive items.
    SensitiveOmittedByExport {
        /// How many items the exporter left out.
        count: u64,
    },
    /// The import leaves more boards than the Free tier allows. Nothing was blocked; the caller
    /// decides what to tell the user.
    BoardLimit {
        /// The limit passed in the options.
        limit: usize,
        /// Boards after the import.
        total: usize,
    },
    /// A file was not valid UTF-8; invalid bytes were replaced with U+FFFD.
    LossyText {
        /// The file or item.
        name: String,
    },
    /// A file was larger than the 5 MB text limit and was skipped.
    TooLarge {
        /// The file or item.
        name: String,
        /// Its size in bytes.
        size: u64,
    },
    /// A file or entry was empty and was skipped.
    Empty {
        /// The file or item.
        name: String,
    },
    /// A file or entry could not be read and was skipped.
    Unreadable {
        /// The file or item.
        name: String,
        /// Why.
        reason: String,
    },
    /// A symbolic link was not followed.
    SymlinkSkipped {
        /// The link.
        name: String,
    },
    /// The source holds entries that are not text (images, files, ...), which are not imported.
    NonTextSkipped {
        /// How many.
        count: usize,
    },
    /// The importer stopped at its cap on items or bytes.
    Truncated {
        /// Why it stopped.
        reason: String,
    },
    /// Something the importer changed or could not carry over; see [`Note`].
    Note(Note),
}

/// What an import left out or changed, as data: the UI words each in the selected language;
/// [`fmt::Display`] is the English sentence, for logs and tests. `label` and `what` name the
/// snippet (or file) the note is about; `tokens` is a list of source variables as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    /// Variables with no Snipdeck equivalent were left as written.
    UnmappedVariables {
        /// The snippet.
        label: String,
        /// The variables, as written in the source, separated by commas.
        tokens: String,
    },
    /// Espanso variables (shell, script, random or unknown) have no Snipdeck equivalent and were
    /// left as written.
    UnmappedEspansoVariables {
        /// The snippet.
        label: String,
        /// The variables, as written in the source, separated by commas.
        tokens: String,
    },
    /// A snippet's variables cannot be one Snipdeck template; it is kept as plain text.
    VariablesDoNotFit {
        /// The snippet.
        label: String,
    },
    /// A snippet keeps one trigger; the other abbreviations were not kept.
    TriggerOnlyOne {
        /// The snippet.
        label: String,
        /// The abbreviations that were not kept, separated by commas.
        triggers: String,
    },
    /// Abbreviations that fire after a separator cannot become triggers.
    TriggerNeedsSeparator {
        /// The snippet.
        label: String,
        /// The abbreviations, separated by commas.
        triggers: String,
    },
    /// Abbreviations that are not a valid trigger (spaces, or over 32 characters).
    TriggerBadShape {
        /// The snippet.
        label: String,
        /// The abbreviations, separated by commas.
        triggers: String,
    },
    /// An Espanso file whose name starts with `_` is disabled there, so it was skipped.
    DisabledFile {
        /// The file.
        name: String,
    },
    /// An Espanso entry is not a match and was skipped.
    NotAMatch {
        /// The entry.
        what: String,
    },
    /// An Espanso match has only a regex trigger, which Snipdeck cannot use; it was skipped.
    RegexTriggerOnly {
        /// The entry.
        what: String,
    },
    /// An Espanso match has no trigger and was skipped.
    NoTrigger {
        /// The entry.
        what: String,
    },
    /// Markdown or HTML text is kept as written.
    MarkupKept {
        /// The entry.
        what: String,
    },
    /// An Espanso match has no text (an image or other content) and was skipped.
    NoText {
        /// The entry.
        what: String,
    },
    /// A date format Snipdeck does not know was left as written.
    DateFormat {
        /// The entry.
        what: String,
        /// The format.
        format: String,
    },
    /// A form field has no equivalent and was left as written.
    FormField {
        /// The entry.
        what: String,
        /// The field.
        name: String,
    },
    /// Some table rows had more columns than are read.
    ColumnsCut {
        /// The file.
        file: String,
    },
    /// The trigger is already used by another snippet; the snippet came in without it.
    TriggerTaken {
        /// The snippet's title.
        title: String,
        /// The trigger.
        trigger: String,
    },
    /// The trigger starts like others, so the shorter fires first.
    TriggerOverlap {
        /// The snippet's title.
        title: String,
        /// The trigger.
        trigger: String,
        /// The others, separated by commas.
        others: String,
    },
    /// Titles or source apps over the limit were cut.
    TitleCut {
        /// How many items.
        count: usize,
        /// The title limit.
        title_chars: usize,
        /// The source-app limit.
        source_chars: usize,
    },
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmappedVariables { label, tokens } => {
                write!(
                    f,
                    "{label}: no Snipdeck equivalent for {tokens}; left as written"
                )
            }
            Self::UnmappedEspansoVariables { label, tokens } => write!(
                f,
                "{label}: no Snipdeck equivalent for {tokens} (shell, script, random or unknown); left as written"
            ),
            Self::VariablesDoNotFit { label } => write!(
                f,
                "{label}: its variables do not fit a Snipdeck template together (for example two cursor marks); kept as plain text"
            ),
            Self::TriggerOnlyOne { label, triggers } => write!(
                f,
                "{label}: `{triggers}` was not kept as a trigger (Snipdeck keeps one trigger per snippet)"
            ),
            Self::TriggerNeedsSeparator { label, triggers } => write!(
                f,
                "{label}: `{triggers}` was not kept as a trigger (it fires after a separator, which a Snipdeck trigger does not wait for)"
            ),
            Self::TriggerBadShape { label, triggers } => write!(
                f,
                "{label}: `{triggers}` was not kept as a trigger (a trigger has no spaces and at most 32 characters)"
            ),
            Self::DisabledFile { name } => write!(
                f,
                "{name} is disabled in Espanso (its name starts with _) and was skipped"
            ),
            Self::NotAMatch { what } => write!(f, "{what}: is not a match; skipped"),
            Self::RegexTriggerOnly { what } => write!(
                f,
                "{what}: has only a regex trigger, which Snipdeck cannot use; skipped"
            ),
            Self::NoTrigger { what } => write!(f, "{what}: has no trigger; skipped"),
            Self::MarkupKept { what } => {
                write!(f, "{what}: markdown/html text is kept as written")
            }
            Self::NoText { what } => {
                write!(f, "{what}: has no text (image or other content); skipped")
            }
            Self::DateFormat { what, format } => write!(
                f,
                "{what}: the date format {format} is not one Snipdeck knows; left as written"
            ),
            Self::FormField { what, name } => write!(
                f,
                "{what}: the form field [[{name}]] has no Snipdeck equivalent; left as written"
            ),
            Self::ColumnsCut { file } => write!(
                f,
                "{file}: some rows have more than 256 columns; the rest were ignored"
            ),
            Self::TriggerTaken { title, trigger } => write!(
                f,
                "`{title}`: the trigger `{trigger}` is already used by another snippet; imported without it"
            ),
            Self::TriggerOverlap {
                title,
                trigger,
                others,
            } => write!(
                f,
                "`{title}`: the trigger `{trigger}` and `{others}` start alike, so the shorter one fires first"
            ),
            Self::TitleCut {
                count,
                title_chars,
                source_chars,
            } => write!(
                f,
                "{count} item(s) had a title or source app longer than {title_chars} or {source_chars} characters; it was cut"
            ),
        }
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NewerMinorVersion { found } => write!(
                f,
                "the archive uses format version {found}, newer than this Snipdeck knows; \
                 unknown fields were ignored"
            ),
            Self::UnknownEntries { count } => {
                write!(f, "{count} unknown file(s) in the archive were ignored")
            }
            Self::UnreferencedAssets { count } => {
                write!(f, "{count} asset(s) no item refers to were ignored")
            }
            Self::SensitiveNeedKey { count } => write!(
                f,
                "{count} sensitive item(s) are encrypted and were skipped because no key is available"
            ),
            Self::CannotDecrypt { title } => write!(
                f,
                "`{title}` could not be decrypted with this computer's key and was skipped"
            ),
            Self::SensitiveOmittedByExport { count } => write!(
                f,
                "{count} sensitive item(s) were left out when this file was exported"
            ),
            Self::BoardLimit { limit, total } => write!(
                f,
                "you now have {total} boards; the Free plan includes {limit}"
            ),
            Self::LossyText { name } => write!(
                f,
                "`{name}` is not valid UTF-8; unreadable bytes were replaced with \u{fffd}"
            ),
            Self::TooLarge { name, size } => write!(
                f,
                "`{name}` is {size} bytes, over the 5 MB text limit, and was skipped"
            ),
            Self::Empty { name } => write!(f, "`{name}` is empty and was skipped"),
            Self::Unreadable { name, reason } => {
                write!(f, "`{name}` could not be read ({reason}) and was skipped")
            }
            Self::SymlinkSkipped { name } => {
                write!(f, "`{name}` is a symbolic link and was not followed")
            }
            Self::NonTextSkipped { count } => {
                write!(
                    f,
                    "{count} entry(ies) without text (images, files) were skipped"
                )
            }
            Self::Truncated { reason } => write!(f, "the import stopped early: {reason}"),
            Self::Note(note) => note.fmt(f),
        }
    }
}

/// The outcome of an import, the same for every importer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Boards created.
    pub boards_added: usize,
    /// Lists created.
    pub lists_added: usize,
    /// Items created.
    pub items_added: usize,
    /// Items in the source that were not imported (duplicates, unreadable, undecryptable, ...).
    pub items_skipped: usize,
    /// How many of `items_skipped` were already in the library (content-identical).
    pub duplicates_skipped: usize,
    /// Tags created (existing tags with the same name are reused).
    pub tags_added: usize,
    /// The boards created, in order, so the UI can open the first one.
    pub boards: Vec<BoardId>,
    /// Up to [`MAX_WARNINGS`] warnings.
    pub warnings: Vec<Warning>,
    /// Warnings beyond [`MAX_WARNINGS`], which were counted but not kept.
    pub warnings_dropped: usize,
}

impl ImportReport {
    /// Records a warning, or only counts it once [`MAX_WARNINGS`] are kept.
    pub fn warn(&mut self, warning: Warning) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(warning);
        } else {
            self.warnings_dropped += 1;
        }
    }

    /// Whether anything was left out or changed that the user should hear about.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty() || self.warnings_dropped > 0
    }
}
