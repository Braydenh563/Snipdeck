//! Clipboard representations of an item.

use std::collections::HashSet;

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

    /// The longest name Windows accepts, in UTF-16 code units (the unit NTFS counts).
    const MAX_UTF16_UNITS: usize = 255;

    /// Characters that make a name display as something other than what it is: bidirectional
    /// controls (a name `a<RLO>gpj.exe` shows as `aexe.jpg`) and line or paragraph
    /// separators. Joiners (U+200C, U+200D) are legitimate and stay allowed.
    const DECEPTIVE: &'static [char] = &[
        '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
        '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{061c}', '\u{2028}', '\u{2029}',
    ];

    /// Creates an entry. `name` must be one Windows-valid path component, so materialising
    /// the file (on paste or import) can never write outside the target folder, onto a
    /// device, or under a name that hides what it is.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidFileName`] for:
    ///
    /// * an empty name, `.` or `..`;
    /// * a path separator, a control character, or one of `<>:"|?*`;
    /// * a name ending in a dot or a space;
    /// * a reserved device name (`CON`, `PRN`, `AUX`, `NUL`, `COM0`-`COM9`, `LPT0`-`LPT9`,
    ///   `CONIN$`, `CONOUT$`), case-insensitively and with any extension (`nul.txt`);
    /// * more than 255 UTF-16 code units;
    /// * a bidirectional control or a line/paragraph separator (see the source for the list).
    ///
    /// Names that Windows treats as equal are checked per item by [`Formats::new`].
    pub fn new(name: &str, content: Content) -> Result<Self, Error> {
        let valid = !name.is_empty()
            && name != "."
            && name != ".."
            && !name.ends_with(['.', ' '])
            && !name.chars().any(|c| {
                c.is_control() || Self::FORBIDDEN.contains(&c) || Self::DECEPTIVE.contains(&c)
            })
            && name.encode_utf16().count() <= Self::MAX_UTF16_UNITS
            && !is_reserved_device_name(name);
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

/// Whether Windows treats `name` as a device: the part before the first dot, ignoring
/// trailing spaces and letter case, is `CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, or
/// `COM`/`LPT` followed by one digit (including the superscript digits Windows also reserves).
fn is_reserved_device_name(name: &str) -> bool {
    const DEVICES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    if DEVICES
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
    {
        return true;
    }
    let mut chars = stem.chars();
    let (Some(a), Some(b), Some(c), Some(digit), None) = (
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
        chars.next(),
    ) else {
        return false;
    };
    let prefix: String = [a, b, c].iter().collect();
    (prefix.eq_ignore_ascii_case("COM") || prefix.eq_ignore_ascii_case("LPT"))
        && (digit.is_ascii_digit() || matches!(digit, '\u{b9}' | '\u{b2}' | '\u{b3}'))
}

/// The key under which Windows treats two names as the same file: letter case is ignored
/// (deliberately a little stricter than NTFS, so `ß` and `ss` also collide).
fn name_key(name: &str) -> String {
    name.to_uppercase().to_lowercase()
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
    /// [`Error::NoFormats`], [`Error::DuplicateFormat`], [`Error::NoFiles`] for an empty
    /// [`Format::Files`], or [`Error::DuplicateFileName`] if two files in the [`Format::Files`]
    /// have names Windows treats as equal (they differ at most in letter case): writing both
    /// to a folder would overwrite one with the other.
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
            if let Format::Files(files) = format {
                if files.is_empty() {
                    return Err(Error::NoFiles);
                }
                let mut seen = HashSet::with_capacity(files.len());
                for file in files {
                    if !seen.insert(name_key(file.name())) {
                        return Err(Error::DuplicateFileName(file.name().to_owned()));
                    }
                }
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
        for good in [
            "invoice.pdf",
            ".gitignore",
            "日本語.txt",
            "a b",
            "e\u{301}.txt",
            "CONSOLE.txt",
            "console",
            "COM10.txt",
            "COM.txt",
            "LPT.log",
            "NULL",
            "AUXILIARY",
            "a.con",
            "x.nul.txt",
            "COM¹x",
        ] {
            assert!(FileEntry::new(good, bytes(b"")).is_ok(), "{good}");
        }
        for bad in [
            "", ".", "..", "../x", "a/b", "a\\b", "C:x", "a?", "a\nb", "trail.", "trail ", "a<b",
            "a>b", "a\"b", "a|b", "a*b", "a\tb", "a\u{7f}b", "a\u{85}b",
        ] {
            assert_eq!(
                FileEntry::new(bad, bytes(b"")),
                Err(Error::InvalidFileName(bad.to_owned())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn reserved_windows_device_names_are_rejected_with_or_without_an_extension() {
        let mut names: Vec<String> = [
            "CON",
            "PRN",
            "AUX",
            "NUL",
            "CONIN$",
            "CONOUT$",
            "con",
            "Prn",
            "aUx",
            "nUl",
            "CON.txt",
            "nul.txt",
            "Aux.tar.gz",
            "CON .txt",
            "COM\u{b9}",
            "LPT\u{b2}.txt",
            "COM\u{b3}",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for n in 0..=9 {
            names.push(format!("COM{n}"));
            names.push(format!("lpt{n}"));
            names.push(format!("COM{n}.txt"));
            names.push(format!("LPT{n}.log"));
        }
        for bad in names {
            assert_eq!(
                FileEntry::new(&bad, bytes(b"")),
                Err(Error::InvalidFileName(bad.clone())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn names_are_limited_to_255_utf16_code_units() {
        assert!(FileEntry::new(&"a".repeat(255), bytes(b"")).is_ok());
        let over = "a".repeat(256);
        assert_eq!(
            FileEntry::new(&over, bytes(b"")),
            Err(Error::InvalidFileName(over.clone()))
        );
        // 128 astral characters are 128 chars but 256 UTF-16 code units; 127 plus one BMP
        // character make exactly 255.
        assert!(FileEntry::new(&"😀".repeat(128), bytes(b"")).is_err());
        assert!(FileEntry::new(&format!("{}x", "😀".repeat(127)), bytes(b"")).is_ok());
        // Windows counts UTF-16 units, not bytes: 255 two-byte characters (510 bytes) pass.
        assert!(FileEntry::new(&"é".repeat(255), bytes(b"")).is_ok());
        // The extension counts.
        assert!(FileEntry::new(&format!("{}.txt", "a".repeat(252)), bytes(b"")).is_err());
    }

    #[test]
    fn bidi_and_format_controls_are_rejected() {
        // `a\u{202e}gpj.exe` displays as `aexe.jpg`.
        for bad in [
            "a\u{202e}gpj.exe",
            "a\u{202a}b",
            "a\u{202b}b",
            "a\u{202c}b",
            "a\u{202d}b",
            "a\u{2066}b",
            "a\u{2067}b",
            "a\u{2068}b",
            "a\u{2069}b",
            "a\u{200e}b",
            "a\u{200f}b",
            "a\u{61c}b",
            "a\u{2028}b",
            "a\u{2029}b",
        ] {
            assert_eq!(
                FileEntry::new(bad, bytes(b"")),
                Err(Error::InvalidFileName(bad.to_owned())),
                "{bad:?}"
            );
        }
        // Joiners are legitimate in names (emoji sequences, Indic scripts).
        assert!(FileEntry::new("👨\u{200d}👩.png", bytes(b"")).is_ok());
    }

    fn file(name: &str) -> FileEntry {
        FileEntry::new(name, bytes(b"x")).unwrap()
    }

    #[test]
    fn files_in_one_format_need_distinct_names_ignoring_case() {
        for (a, b) in [
            ("a.txt", "a.txt"),
            ("a.txt", "A.TXT"),
            ("Report.PDF", "report.pdf"),
            ("É.txt", "é.txt"),
            ("straße", "STRASSE"),
        ] {
            assert_eq!(
                Formats::new(vec![Format::Files(vec![file("other"), file(a), file(b)])]),
                Err(Error::DuplicateFileName(b.to_owned())),
                "{a} / {b}"
            );
        }
        let distinct = Format::Files(vec![file("a.txt"), file("b.txt"), file("a.txt.bak")]);
        assert!(Formats::new(vec![distinct]).is_ok());
    }

    #[test]
    fn the_same_file_name_in_different_formats_is_fine() {
        let formats = Formats::new(vec![
            Format::Files(vec![file("a.txt")]),
            Format::Text(bytes(b"a.txt")),
        ]);
        assert!(formats.is_ok());
    }
}
