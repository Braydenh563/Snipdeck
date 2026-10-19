//! The item editor's data and the checks that need no store: what the window shows
//! ([`ItemEditorView`]), what it hands back ([`EditorInput`]), why a field is refused
//! ([`EditorProblem`]) and how a file chosen for an Image or File item is vetted.
//!
//! Slint-free like the controller that owns the state (`controller/editor_ctl.rs`); the window is
//! bound by [`editor_binding`](crate::editor_binding). Nothing here logs: no function writes
//! item text, a path or a file name to a log or a trace.

use std::{fs, io::Read, path::Path};

use snipdeck_core::{
    Content, Feature, FileEntry, Inline, ItemKind, ItemLabel,
    template::{Template, TemplateError},
};
use snipdeck_platform::system_look::HighContrast;

use crate::main_window::NameError;

/// The most text the editor edits in place (SPEC 5.4): longer text opens read-only, and text
/// pasted past this is refused on Save. The store itself takes more (a blob over
/// [`snipdeck_core::INLINE_LIMIT`]); this is the editor's cap.
pub const EDIT_LIMIT_BYTES: usize = 16 * 1024;

/// The longest title, in characters.
pub const TITLE_MAX_CHARS: usize = 200;

/// The largest file an Image or File item takes. It is the largest entry a `.snipdeck` archive
/// reads back, so an item that is saved can always be exported and restored.
#[must_use]
pub fn max_attachment_bytes() -> u64 {
    snipdeck_portable::Limits::default().max_entry_bytes
}

/// The largest picture the editor decodes to show a preview, in pixels.
const PREVIEW_MAX_PIXELS: u64 = 16 * 1024 * 1024;

/// The kinds in the order of the window's kind switch (`kind-index`).
pub const KINDS: [ItemKind; 6] = [
    ItemKind::Text,
    ItemKind::Code,
    ItemKind::Link,
    ItemKind::Template,
    ItemKind::Image,
    ItemKind::File,
];

/// The window's `kind-index` of `kind`.
#[must_use]
pub fn kind_index(kind: ItemKind) -> usize {
    KINDS.iter().position(|k| *k == kind).unwrap_or(0)
}

/// The kind of window index `index`; a value out of range reads as Text.
#[must_use]
pub fn kind_at(index: usize) -> ItemKind {
    KINDS.get(index).copied().unwrap_or(ItemKind::Text)
}

/// Whether the kind holds text the editor shows (Text, Code, Link, Template).
#[must_use]
pub fn is_text_kind(kind: ItemKind) -> bool {
    !matches!(kind, ItemKind::Image | ItemKind::File)
}

/// Where a request to open the editor came from, which decides the list a new item goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The Panel (or the Picker): the list on its tab.
    Panel,
    /// The Main window: the list open in its sidebar.
    Main,
}

/// What the fields of the open window hold, handed to the controller on Save and for the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorInput {
    /// The kind chosen.
    pub kind: ItemKind,
    /// The title field.
    pub title: String,
    /// The text field (what the window shows: only the start of a large text).
    pub content: String,
    /// The Sensitive switch.
    pub sensitive: bool,
    /// The list chosen: an index into [`ItemEditorView::list_names`].
    pub list_index: usize,
    /// The path field of a new Image or File item.
    pub source_path: String,
    /// The colour label chosen, if any.
    pub label: Option<ItemLabel>,
}

/// Why the editor refuses something. The words are `wording::editor_problem_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorProblem {
    /// The title is longer than [`TITLE_MAX_CHARS`].
    TitleTooLong {
        /// The limit.
        max: usize,
    },
    /// The title has a line break or another control character.
    TitleHasControl,
    /// There is no text (or only blanks).
    ContentEmpty,
    /// The text is over [`EDIT_LIMIT_BYTES`].
    ContentTooLong {
        /// The limit in KB.
        max_kb: usize,
    },
    /// A link without a scheme such as `https://`.
    LinkNeedsScheme,
    /// A link with a space or a line break in it.
    LinkHasSpace,
    /// The template does not parse; carries the parser's own explanation.
    Template(String),
    /// The text uses something Supporter (a prompted placeholder).
    NeedsPro(Feature),
    /// No file chosen.
    SourceEmpty,
    /// No file at that path.
    SourceMissing,
    /// The path is a folder or a device, not a regular file.
    SourceNotAFile,
    /// The file cannot be read (permissions, in use, gone meanwhile).
    SourceUnreadable,
    /// The file is over the limit.
    SourceTooLarge {
        /// The limit in MB.
        max_mb: u64,
    },
    /// The file is empty.
    SourceEmptyFile,
    /// An Image item needs a PNG picture.
    SourceNotPng,
    /// The file's name cannot be kept (Windows does not allow it).
    SourceBadName,
    /// A new tag's name is refused.
    Tag(NameError),
    /// Saving failed for a reason that belongs to no field.
    Save(SaveFailure),
    /// Writing the draft failed.
    DraftNotSaved,
    /// The file dialog could not be shown.
    BrowseFailed,
}

/// Why a write to the library failed, in words the user can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFailure {
    /// The encryption key is locked (a master password is set).
    Locked,
    /// The encryption key could not be made or opened.
    KeyUnavailable,
    /// The item was deleted meanwhile.
    ItemGone,
    /// The list was deleted meanwhile.
    ListGone,
    /// The disk or database failed.
    Storage,
    /// Anything else the store refused.
    Other,
}

impl SaveFailure {
    /// The failure the store error `error` stands for.
    #[must_use]
    pub fn of(error: &snipdeck_store::Error) -> Self {
        use snipdeck_store::{Error, crypto::CryptoError};
        match error {
            Error::EncryptionLocked | Error::Crypto(CryptoError::PasswordRequired) => Self::Locked,
            Error::Crypto(_) => Self::KeyUnavailable,
            Error::NotFound { entity: "list", .. } => Self::ListGone,
            Error::NotFound { .. } => Self::ItemGone,
            Error::Io(_) | Error::Sqlite(_) | Error::BlobCorrupt { .. } => Self::Storage,
            _ => Self::Other,
        }
    }
}

/// A picture to show in the editor: 8-bit RGBA.
#[derive(Clone, PartialEq, Eq)]
pub struct Preview {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Preview")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// What an existing Image or File item holds, for the editor to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentView {
    /// The kind (Image or File).
    pub kind: ItemKind,
    /// The first file's name (empty for an image).
    pub name: String,
    /// How many files (1 for an image).
    pub files: usize,
    /// The total size in bytes.
    pub bytes: u64,
    /// The picture's size, for an image.
    pub dimensions: Option<(u32, u32)>,
    /// The picture, when it is small enough to decode.
    pub preview: Option<Preview>,
}

/// Everything the item editor window shows, as plain data. The text fields are the values to
/// start from: the window keeps what the user types and sends it back as an [`EditorInput`].
#[derive(Debug, Clone, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches the editor shows, not a state"
)]
pub struct ItemEditorView {
    /// Changes with every editor that opens, so the window resets its fields for a new one and
    /// keeps what the user typed while the same one stays up.
    pub seq: u64,
    /// Changes when something asks for the open editor to come to the front.
    pub raise: u64,
    /// A new item (no Delete button, the kind can be any of the six).
    pub is_new: bool,
    /// Dark theme.
    pub dark: bool,
    /// The high-contrast colours, while a Windows high-contrast theme is on.
    pub high_contrast: Option<HighContrast>,
    /// Animations are off.
    pub reduced_motion: bool,
    /// The starting values.
    pub fields: EditorInput,
    /// The names of the lists to choose from.
    pub list_names: Vec<String>,
    /// Every tag with whether this item has it.
    pub tags: Vec<(String, bool)>,
    /// The title is refused.
    pub title_problem: Option<EditorProblem>,
    /// The text is refused.
    pub content_problem: Option<EditorProblem>,
    /// The file is refused.
    pub source_problem: Option<EditorProblem>,
    /// The Sensitive switch is refused.
    pub privacy_problem: Option<EditorProblem>,
    /// A failure that belongs to no field.
    pub banner: Option<EditorProblem>,
    /// The text is over the editing cap: shown read-only; this is its size in bytes.
    pub large: Option<u64>,
    /// What an existing Image or File holds.
    pub attachment: Option<AttachmentView>,
    /// Prompted placeholders are available (Supporter).
    pub prompted_available: bool,
    /// The app can open a file dialog.
    pub can_browse: bool,
    /// The fields hold a draft from an earlier session.
    pub restored: bool,
}

/// `title` trimmed, or why it is refused.
///
/// # Errors
///
/// [`EditorProblem::TitleTooLong`] or [`EditorProblem::TitleHasControl`].
pub fn check_title(title: &str) -> Result<String, EditorProblem> {
    let title = title.trim();
    if title.chars().any(char::is_control) {
        return Err(EditorProblem::TitleHasControl);
    }
    if title.chars().count() > TITLE_MAX_CHARS {
        return Err(EditorProblem::TitleTooLong {
            max: TITLE_MAX_CHARS,
        });
    }
    Ok(title.to_owned())
}

/// `text` as a link: trimmed, with a scheme and no blanks.
///
/// # Errors
///
/// [`EditorProblem::LinkNeedsScheme`] or [`EditorProblem::LinkHasSpace`].
pub fn check_link(text: &str) -> Result<String, EditorProblem> {
    let text = text.trim();
    if text.chars().any(char::is_whitespace) {
        return Err(EditorProblem::LinkHasSpace);
    }
    let Some((scheme, rest)) = text.split_once(':') else {
        return Err(EditorProblem::LinkNeedsScheme);
    };
    let scheme_ok = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    // `mailto:` and `tel:` carry no `//`; every other scheme needs something after it.
    let rest_ok = if matches!(scheme.to_ascii_lowercase().as_str(), "mailto" | "tel") {
        !rest.is_empty()
    } else {
        rest.strip_prefix("//").is_some_and(|r| !r.is_empty())
    };
    if scheme_ok && rest_ok {
        Ok(text.to_owned())
    } else {
        Err(EditorProblem::LinkNeedsScheme)
    }
}

/// Checks template `text`: it must parse, and a Free user may not introduce prompted
/// placeholders (`introduced`: the text is new or changed, so a template that already had them
/// can still be saved with another title).
///
/// # Errors
///
/// [`EditorProblem::Template`] or [`EditorProblem::NeedsPro`].
pub fn check_template(
    text: &str,
    prompted_allowed: bool,
    introduced: bool,
) -> Result<(), EditorProblem> {
    let template = Template::parse(text)
        .map_err(|e: TemplateError| EditorProblem::Template(e.kind.to_string()))?;
    if introduced && !prompted_allowed && template.requires(Feature::PromptedFields) {
        return Err(EditorProblem::NeedsPro(Feature::PromptedFields));
    }
    Ok(())
}

/// `text` for a text kind: not blank and within the editing cap.
///
/// # Errors
///
/// [`EditorProblem::ContentEmpty`] or [`EditorProblem::ContentTooLong`].
pub fn check_text(text: &str) -> Result<(), EditorProblem> {
    if text.trim().is_empty() {
        return Err(EditorProblem::ContentEmpty);
    }
    if text.len() > EDIT_LIMIT_BYTES {
        return Err(EditorProblem::ContentTooLong {
            max_kb: EDIT_LIMIT_BYTES / 1024,
        });
    }
    Ok(())
}

/// A file read for a new Image or File item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The kind (Image or File).
    pub kind: ItemKind,
    /// The file's name.
    pub name: String,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

/// The eight bytes every PNG starts with.
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Reads the file at `path` for an Image or File item. The path is canonicalised (so a link is
/// followed to what it names), must exist and be a regular file within the size limit, and an
/// image must be a PNG.
///
/// # Errors
///
/// A `Source…` [`EditorProblem`].
pub fn read_source(kind: ItemKind, path: &str) -> Result<Source, EditorProblem> {
    // Explorer's "Copy as path" puts quotes around the path.
    let path = path.trim().trim_matches('"').trim();
    if path.is_empty() {
        return Err(EditorProblem::SourceEmpty);
    }
    let canonical = fs::canonicalize(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => EditorProblem::SourceMissing,
        _ => EditorProblem::SourceUnreadable,
    })?;
    let meta = fs::metadata(&canonical).map_err(|_| EditorProblem::SourceUnreadable)?;
    if !meta.is_file() {
        return Err(EditorProblem::SourceNotAFile);
    }
    let limit = max_attachment_bytes();
    if meta.len() > limit {
        return Err(EditorProblem::SourceTooLarge {
            max_mb: limit / (1024 * 1024),
        });
    }
    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(EditorProblem::SourceBadName)?
        .to_owned();
    // The name must be one Snipdeck can write back out when the item is pasted.
    let empty = Inline::new(Vec::new()).map_err(|_| EditorProblem::SourceUnreadable)?;
    FileEntry::new(&name, Content::Inline(empty)).map_err(|_| EditorProblem::SourceBadName)?;
    let bytes = read_bounded(&canonical, limit)?;
    if bytes.is_empty() {
        return Err(EditorProblem::SourceEmptyFile);
    }
    if kind == ItemKind::Image && !is_png(&bytes) {
        return Err(EditorProblem::SourceNotPng);
    }
    Ok(Source { kind, name, bytes })
}

/// The whole file, but never more than `limit` bytes (a file that grows while it is read).
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, EditorProblem> {
    let file = fs::File::open(path).map_err(|_| EditorProblem::SourceUnreadable)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| EditorProblem::SourceUnreadable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(EditorProblem::SourceTooLarge {
            max_mb: limit / (1024 * 1024),
        });
    }
    Ok(bytes)
}

/// Whether `bytes` start like a PNG whose header the decoder accepts.
#[must_use]
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&PNG_SIGNATURE) && png_size(bytes).is_some()
}

/// The size of a PNG picture from its header.
#[must_use]
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let info = reader.info();
    Some((info.width, info.height))
}

/// The picture of PNG `bytes` as RGBA, or `None` when it is not a PNG, is too big to be worth
/// decoding on the UI thread, or fails to decode.
#[must_use]
pub fn decode_preview(bytes: &[u8]) -> Option<Preview> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if u64::from(width) * u64::from(height) > PREVIEW_MAX_PIXELS {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let data = buffer.get(..frame.buffer_size())?;
    let mut rgba = Vec::with_capacity(data.len() / frame.color_type.samples() * 4);
    match frame.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => {
            for px in data.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for &g in data {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for px in data.as_chunks::<2>().0 {
                rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]);
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(Preview {
        width: frame.width,
        height: frame.height,
        rgba,
    })
}

/// The start of `text` that fits `max` bytes, cut on a character boundary.
#[must_use]
pub fn prefix_of(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("header");
        writer.write_image_data(data).expect("data");
        writer.finish().expect("finish");
        out
    }

    #[test]
    fn kinds_map_to_the_window_order_both_ways() {
        for (i, kind) in KINDS.iter().enumerate() {
            assert_eq!(kind_index(*kind), i);
            assert_eq!(kind_at(i), *kind);
        }
        assert_eq!(kind_at(99), ItemKind::Text, "out of range reads as Text");
        assert!(is_text_kind(ItemKind::Template));
        assert!(!is_text_kind(ItemKind::Image));
        assert!(!is_text_kind(ItemKind::File));
    }

    #[test]
    fn titles_are_trimmed_and_checked() {
        assert_eq!(check_title("  Thanks  ").as_deref(), Ok("Thanks"));
        assert_eq!(check_title("").as_deref(), Ok(""), "a title is optional");
        assert_eq!(check_title("a\nb"), Err(EditorProblem::TitleHasControl));
        assert_eq!(
            check_title(&"x".repeat(TITLE_MAX_CHARS + 1)),
            Err(EditorProblem::TitleTooLong {
                max: TITLE_MAX_CHARS
            })
        );
        assert!(
            check_title(&"é".repeat(TITLE_MAX_CHARS)).is_ok(),
            "chars, not bytes"
        );
    }

    #[test]
    fn links_need_a_scheme_and_no_blanks() {
        for good in [
            "https://example.com/a?b=1",
            " http://x.io ",
            "ftp://host/file",
            "mailto:me@example.com",
            "tel:+4930123",
            "obsidian://open?vault=a",
        ] {
            assert!(check_link(good).is_ok(), "{good:?}");
        }
        assert_eq!(check_link(" https://a.b ").as_deref(), Ok("https://a.b"));
        for bad in [
            "example.com",
            "figma.com/file/x",
            "https://",
            "://x",
            "1http://x",
            "mailto:",
            ":",
            "",
        ] {
            assert_eq!(
                check_link(bad),
                Err(EditorProblem::LinkNeedsScheme),
                "{bad:?}"
            );
        }
        assert_eq!(check_link("https://a b"), Err(EditorProblem::LinkHasSpace));
        assert_eq!(check_link("https://a\nb"), Err(EditorProblem::LinkHasSpace));
    }

    #[test]
    fn templates_must_parse_and_prompts_are_pro() {
        assert!(check_template("Dear {field:Name}, {date}", true, true).is_ok());
        assert!(check_template("Today is {date}", false, true).is_ok());
        assert_eq!(
            check_template("Dear {field:Name}", false, true),
            Err(EditorProblem::NeedsPro(Feature::PromptedFields))
        );
        assert_eq!(
            check_template("Pick {choice:Size|S|M}", false, true),
            Err(EditorProblem::NeedsPro(Feature::PromptedFields))
        );
        assert!(
            check_template("Dear {field:Name}", false, false).is_ok(),
            "a template that already had it can be saved again"
        );
        assert!(matches!(
            check_template("Broken {nope}", true, true),
            Err(EditorProblem::Template(_))
        ));
        assert!(matches!(
            check_template("Open { here", true, false),
            Err(EditorProblem::Template(_))
        ));
    }

    #[test]
    fn text_must_not_be_blank_or_over_the_cap() {
        assert_eq!(check_text(" \n\t"), Err(EditorProblem::ContentEmpty));
        assert!(check_text(&"a".repeat(EDIT_LIMIT_BYTES)).is_ok());
        assert_eq!(
            check_text(&"a".repeat(EDIT_LIMIT_BYTES + 1)),
            Err(EditorProblem::ContentTooLong { max_kb: 16 })
        );
    }

    #[test]
    fn a_prefix_never_cuts_a_character() {
        assert_eq!(prefix_of("abc", 10), "abc");
        assert_eq!(prefix_of("abcdef", 3), "abc");
        // "é" is two bytes: cutting at 1 backs up to 0.
        assert_eq!(prefix_of("éé", 1), "");
        assert_eq!(prefix_of("éé", 3), "é");
        assert_eq!(prefix_of("日本語", 4), "日");
    }

    #[test]
    fn a_source_must_exist_be_a_regular_file_and_fit() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("note.txt");
        fs::write(&file, b"hello").expect("write");
        let ok = read_source(ItemKind::File, file.to_str().expect("utf8")).expect("source");
        assert_eq!(
            (ok.name.as_str(), ok.bytes.as_slice()),
            ("note.txt", &b"hello"[..])
        );
        // Quotes, as Explorer's "Copy as path" makes them.
        let quoted = format!("  \"{}\"  ", file.display());
        assert!(read_source(ItemKind::File, &quoted).is_ok());
        assert_eq!(
            read_source(ItemKind::File, "  "),
            Err(EditorProblem::SourceEmpty)
        );
        assert_eq!(
            read_source(
                ItemKind::File,
                dir.path().join("nope").to_str().expect("utf8")
            ),
            Err(EditorProblem::SourceMissing)
        );
        assert_eq!(
            read_source(ItemKind::File, dir.path().to_str().expect("utf8")),
            Err(EditorProblem::SourceNotAFile)
        );
        let empty = dir.path().join("empty.bin");
        fs::write(&empty, b"").expect("write");
        assert_eq!(
            read_source(ItemKind::File, empty.to_str().expect("utf8")),
            Err(EditorProblem::SourceEmptyFile)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_followed_and_a_dangling_one_is_missing() {
        let dir = tempfile::tempdir().expect("dir");
        let target = dir.path().join("real.txt");
        fs::write(&target, b"x").expect("write");
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let got = read_source(ItemKind::File, link.to_str().expect("utf8")).expect("source");
        assert_eq!(got.name, "real.txt", "the name of what the link names");
        let dangling = dir.path().join("dangling");
        std::os::unix::fs::symlink(dir.path().join("gone"), &dangling).expect("symlink");
        assert_eq!(
            read_source(ItemKind::File, dangling.to_str().expect("utf8")),
            Err(EditorProblem::SourceMissing)
        );
    }

    #[test]
    fn an_image_source_must_be_a_png() {
        let dir = tempfile::tempdir().expect("dir");
        let png = dir.path().join("p.png");
        fs::write(
            &png,
            png_bytes(2, 1, png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6]),
        )
        .expect("write");
        assert!(read_source(ItemKind::Image, png.to_str().expect("utf8")).is_ok());
        let fake = dir.path().join("fake.png");
        fs::write(&fake, b"GIF89a not a png").expect("write");
        assert_eq!(
            read_source(ItemKind::Image, fake.to_str().expect("utf8")),
            Err(EditorProblem::SourceNotPng)
        );
        // The same file is a fine File item.
        assert!(read_source(ItemKind::File, fake.to_str().expect("utf8")).is_ok());
        // A PNG signature with nothing after it is not a picture either.
        fs::write(&fake, PNG_SIGNATURE).expect("write");
        assert_eq!(
            read_source(ItemKind::Image, fake.to_str().expect("utf8")),
            Err(EditorProblem::SourceNotPng)
        );
    }

    #[test]
    fn a_file_name_windows_would_refuse_is_not_kept() {
        let dir = tempfile::tempdir().expect("dir");
        let file = dir.path().join("trailing.");
        if fs::write(&file, b"x").is_ok() && file.exists() {
            assert_eq!(
                read_source(ItemKind::File, file.to_str().expect("utf8")),
                Err(EditorProblem::SourceBadName)
            );
        }
    }

    #[test]
    fn previews_come_out_as_rgba_whatever_the_colour_type() {
        let rgb = png_bytes(2, 1, png::ColorType::Rgb, &[10, 20, 30, 40, 50, 60]);
        let p = decode_preview(&rgb).expect("preview");
        assert_eq!((p.width, p.height), (2, 1));
        assert_eq!(p.rgba, [10, 20, 30, 255, 40, 50, 60, 255]);
        let rgba = png_bytes(1, 1, png::ColorType::Rgba, &[1, 2, 3, 4]);
        assert_eq!(decode_preview(&rgba).expect("preview").rgba, [1, 2, 3, 4]);
        let gray = png_bytes(2, 1, png::ColorType::Grayscale, &[7, 9]);
        assert_eq!(
            decode_preview(&gray).expect("preview").rgba,
            [7, 7, 7, 255, 9, 9, 9, 255]
        );
        let gray_alpha = png_bytes(1, 1, png::ColorType::GrayscaleAlpha, &[5, 200]);
        assert_eq!(
            decode_preview(&gray_alpha).expect("preview").rgba,
            [5, 5, 5, 200]
        );
        assert_eq!(png_size(&rgb), Some((2, 1)));
        assert!(decode_preview(b"not a png").is_none());
        assert!(png_size(b"not a png").is_none());
    }

    #[test]
    fn store_errors_become_specific_failures() {
        use snipdeck_store::{Error, crypto::CryptoError};
        let cases = [
            (Error::EncryptionLocked, SaveFailure::Locked),
            (
                Error::Crypto(CryptoError::PasswordRequired),
                SaveFailure::Locked,
            ),
            (
                Error::Crypto(CryptoError::Random),
                SaveFailure::KeyUnavailable,
            ),
            (
                Error::NotFound {
                    entity: "list",
                    id: "x".into(),
                },
                SaveFailure::ListGone,
            ),
            (
                Error::NotFound {
                    entity: "item",
                    id: "x".into(),
                },
                SaveFailure::ItemGone,
            ),
            (
                Error::Io(std::io::Error::other("disk")),
                SaveFailure::Storage,
            ),
            (Error::Invalid("x".into()), SaveFailure::Other),
        ];
        for (error, failure) in cases {
            assert_eq!(SaveFailure::of(&error), failure, "{error}");
        }
    }
}
