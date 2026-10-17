use std::borrow::Cow;

use snipdeck_core::Content;

use crate::Result;

/// The content-addressed blob folder (`blobs/` in the data folder, ADR-0002).
///
/// This is the only place that decides between inline and blob storage, using
/// [`INLINE_LIMIT`](snipdeck_core::INLINE_LIMIT), and the only place that hashes (BLAKE3).
pub trait BlobStore {
    /// Stores `bytes`: returns [`Content::Inline`] when they fit within the inline limit,
    /// otherwise writes them once under their hash (identical bytes share one file) and returns
    /// [`Content::Blob`].
    fn put(&mut self, bytes: Vec<u8>) -> Result<Content>;

    /// Reads `content`'s bytes, borrowing them when they are inline.
    ///
    /// Fails with [`Error::NotFound`](crate::Error::NotFound) if a blob file is missing.
    fn read<'a>(&self, content: &'a Content) -> Result<Cow<'a, [u8]>>;
}
