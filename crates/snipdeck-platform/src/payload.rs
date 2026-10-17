//! Clipboard content with its bytes in hand.
//!
//! `snipdeck_core::Formats` may hold blob references that only the store can read, and the
//! platform layer has no store (it depends on core alone). The app resolves an item's formats
//! into a [`Payload`] and hands that over; the capture path (M5) produces one from the clipboard.

/// A file to place on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePayload {
    /// A single valid Windows path component (core checks this when the item is saved).
    pub name: String,
    /// The file's bytes.
    pub bytes: Vec<u8>,
}

/// One clipboard representation, bytes included.
#[derive(Clone, PartialEq, Eq)]
pub enum Data {
    /// Plain text.
    Text(String),
    /// An HTML fragment without the Windows `CF_HTML` header (the backend adds and strips it).
    Html(String),
    /// A Rich Text Format document.
    Rtf(Vec<u8>),
    /// A PNG image (the backend converts to and from `CF_DIB`).
    Png(Vec<u8>),
    /// Files, written to a temporary folder when pasted.
    Files(Vec<FilePayload>),
}

// Content may be sensitive: show shapes, never bytes.
impl std::fmt::Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text(s) => write!(f, "Text({} bytes)", s.len()),
            Self::Html(s) => write!(f, "Html({} bytes)", s.len()),
            Self::Rtf(b) => write!(f, "Rtf({} bytes)", b.len()),
            Self::Png(b) => write!(f, "Png({} bytes)", b.len()),
            Self::Files(files) => write!(f, "Files({})", files.len()),
        }
    }
}

/// What to write to the clipboard: one or more representations of the same content, the first
/// being the primary one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload(Vec<Data>);

impl Payload {
    /// Wraps `data`; `None` if it is empty.
    pub fn new(data: Vec<Data>) -> Option<Self> {
        (!data.is_empty()).then_some(Self(data))
    }

    /// The representations, primary first.
    pub fn data(&self) -> &[Data] {
        &self.0
    }

    #[must_use]
    /// This payload reduced to its text, or unchanged when it has none (an image or a file).
    pub fn plain_text(&self) -> Self {
        self.0
            .iter()
            .find(|data| matches!(data, Data::Text(_)))
            .map_or_else(|| self.clone(), |text| Self(vec![text.clone()]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_payload_is_rejected() {
        assert_eq!(Payload::new(vec![]), None);
    }

    #[test]
    fn plain_text_keeps_only_the_text() {
        let payload =
            Payload::new(vec![Data::Html("<b>a</b>".into()), Data::Text("a".into())]).unwrap();
        assert_eq!(payload.plain_text().data(), [Data::Text("a".into())]);
    }

    #[test]
    fn plain_text_of_an_image_is_the_image() {
        let payload = Payload::new(vec![Data::Png(vec![1, 2, 3])]).unwrap();
        assert_eq!(payload.plain_text(), payload);
    }

    #[test]
    fn debug_never_prints_content() {
        let shown = format!("{:?}", Data::Text("hunter2".into()));
        assert!(!shown.contains("hunter2"));
    }
}
