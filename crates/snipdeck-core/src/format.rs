//! Clipboard representations of an item.

use crate::{Content, Error};

keyed_enum! {
    /// The kind of a [`Format`]. Keys match SPEC §4.
    pub enum FormatKind ("format kind") {
        /// Plain UTF-8 text.
        Text = "text",
        /// An HTML fragment.
        Html = "html",
        /// A Rich Text Format document.
        Rtf = "rtf",
        /// A PNG image.
        Png = "image/png",
        /// One or more files.
        Files = "files",
    }
}

/// One clipboard representation of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Format {
    /// Plain text, UTF-8.
    Text(Content),
    /// An HTML fragment, UTF-8, without the Windows `CF_HTML` header (the platform layer adds
    /// and strips it).
    Html(Content),
    /// A Rich Text Format document.
    Rtf(Content),
    /// A PNG-encoded image; the platform layer converts bitmaps to and from PNG.
    Png(Content),
    /// Copies of files, held by Snipdeck so the item still pastes after the originals move
    /// (ADR-0002). Never empty once inside [`Formats`].
    Files(Vec<FileEntry>),
}

impl Format {
    /// This format's kind.
    pub fn kind(&self) -> FormatKind {
        match self {
            Self::Text(_) => FormatKind::Text,
            Self::Html(_) => FormatKind::Html,
            Self::Rtf(_) => FormatKind::Rtf,
            Self::Png(_) => FormatKind::Png,
            Self::Files(_) => FormatKind::Files,
        }
    }
}

/// A file held by a [`Format::Files`]: its name and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    name: String,
    content: Content,
}

impl FileEntry {
    /// Characters Windows does not allow in a file name, besides control characters.
    const FORBIDDEN: &'static [char] = &['/', '\\', '<', '>', ':', '"', '|', '?', '*'];

    /// Creates an entry. `name` must be one Windows-valid path component, so materialising
    /// the file (on paste or import) can never write outside the target folder.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidFileName`] for an empty name, `.` or `..`, a name containing a path
    /// separator, a control character or one of `<>:"|?*`, or one ending in a dot or space.
    pub fn new(name: &str, content: Content) -> Result<Self, Error> {
        let valid = !name.is_empty()
            && name != "."
            && name != ".."
            && !name.ends_with(['.', ' '])
            && !name
                .chars()
                .any(|c| c.is_control() || Self::FORBIDDEN.contains(&c));
        if !valid {
            return Err(Error::InvalidFileName(name.to_owned()));
        }
        Ok(Self {
            name: name.to_owned(),
            content,
        })
    }

    /// The file name, e.g. `invoice.pdf`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The file's bytes.
    pub fn content(&self) -> &Content {
        &self.content
    }
}

/// An item's formats: at least one, at most one per [`FormatKind`], in preference order.
///
/// The first format is the item's primary representation; the order is preserved when the
/// item is written back to the clipboard, so richer formats can come before fallbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formats(Vec<Format>);

impl Formats {
    /// Checks the invariants.
    ///
    /// # Errors
    ///
    /// [`Error::NoFormats`], [`Error::DuplicateFormat`], or [`Error::NoFiles`] for an empty
    /// [`Format::Files`].
    pub fn new(formats: Vec<Format>) -> Result<Self, Error> {
        if formats.is_empty() {
            return Err(Error::NoFormats);
        }
        for (i, format) in formats.iter().enumerate() {
            if formats[..i]
                .iter()
                .any(|earlier| earlier.kind() == format.kind())
            {
                return Err(Error::DuplicateFormat(format.kind()));
            }
            if matches!(format, Format::Files(files) if files.is_empty()) {
                return Err(Error::NoFiles);
            }
        }
        Ok(Self(formats))
    }

    /// The preferred format.
    pub fn primary(&self) -> &Format {
        // `new` guarantees at least one format.
        &self.0[0]
    }

    /// The format of `kind`, if present.
    pub fn get(&self, kind: FormatKind) -> Option<&Format> {
        self.0.iter().find(|format| format.kind() == kind)
    }

    /// The formats in preference order.
    pub fn iter(&self) -> core::slice::Iter<'_, Format> {
        self.0.iter()
    }

    /// Unwraps the formats, in preference order.
    pub fn into_vec(self) -> Vec<Format> {
        self.0
    }
}

impl<'a> IntoIterator for &'a Formats {
    type Item = &'a Format;
    type IntoIter = core::slice::Iter<'a, Format>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Inline;

    fn bytes(data: &[u8]) -> Content {
        Content::Inline(Inline::new(data.to_vec()).unwrap())
    }

    #[test]
    fn format_kind_keys_round_trip() {
        crate::key::assert_keys_round_trip(FormatKind::ALL, FormatKind::key);
        assert_eq!(FormatKind::Png.key(), "image/png");
    }

    #[test]
    fn formats_keep_preference_order() {
        let formats = Formats::new(vec![
            Format::Html(bytes(b"<b>hi</b>")),
            Format::Text(bytes(b"hi")),
        ])
        .unwrap();
        assert_eq!(formats.primary().kind(), FormatKind::Html);
        let kinds: Vec<_> = formats.iter().map(Format::kind).collect();
        assert_eq!(kinds, [FormatKind::Html, FormatKind::Text]);
        assert_eq!(
            formats.get(FormatKind::Text),
            Some(&Format::Text(bytes(b"hi")))
        );
        assert_eq!(formats.get(FormatKind::Rtf), None);
    }

    #[test]
    fn formats_must_not_be_empty() {
        assert_eq!(Formats::new(Vec::new()), Err(Error::NoFormats));
    }

    #[test]
    fn formats_reject_a_second_format_of_the_same_kind() {
        let result = Formats::new(vec![
            Format::Text(bytes(b"a")),
            Format::Png(bytes(b"png")),
            Format::Text(bytes(b"b")),
        ]);
        assert_eq!(result, Err(Error::DuplicateFormat(FormatKind::Text)));
    }

    #[test]
    fn files_format_needs_a_file() {
        assert_eq!(
            Formats::new(vec![Format::Files(Vec::new())]),
            Err(Error::NoFiles)
        );
        let file = FileEntry::new("invoice.pdf", bytes(b"%PDF")).unwrap();
        assert!(Formats::new(vec![Format::Files(vec![file])]).is_ok());
    }

    #[test]
    fn file_names_are_single_windows_valid_components() {
        for good in ["invoice.pdf", ".gitignore", "日本語.txt", "a b"] {
            assert!(FileEntry::new(good, bytes(b"")).is_ok(), "{good}");
        }
        for bad in [
            "", ".", "..", "../x", "a/b", "a\\b", "C:x", "a?", "a\nb", "trail.", "trail ",
        ] {
            assert_eq!(
                FileEntry::new(bad, bytes(b"")),
                Err(Error::InvalidFileName(bad.to_owned())),
                "{bad:?}"
            );
        }
    }
}
