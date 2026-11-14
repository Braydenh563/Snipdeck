//! The embedded, generated data files and how they are inflated.

use thiserror::Error;

/// A generated data file is damaged. `scripts/symbols.py --check` catches this before release.
#[derive(Debug, Error)]
#[error("symbols data file {file}: {what}")]
pub struct DataError {
    pub(crate) file: &'static str,
    pub(crate) what: &'static str,
}

pub(crate) static EMOJI: &[u8] = include_bytes!("../data/emoji.tsv.z");
pub(crate) static KAOMOJI: &[u8] = include_bytes!("../data/kaomoji.tsv.z");
pub(crate) static ICONS: &[u8] = include_bytes!("../data/icons.bin.z");
pub(crate) static ART: &[u8] = include_bytes!("../data/emoji48.bin");

pub(crate) static NAMES: [(&str, &[u8]); 6] = [
    ("de", include_bytes!("../data/names-de.tsv.z")),
    ("es", include_bytes!("../data/names-es.tsv.z")),
    ("fr", include_bytes!("../data/names-fr.tsv.z")),
    ("ja", include_bytes!("../data/names-ja.tsv.z")),
    ("pt", include_bytes!("../data/names-pt.tsv.z")),
    ("zh", include_bytes!("../data/names-zh.tsv.z")),
];

/// Inflates a raw-deflate TSV file into its non-empty lines.
pub(crate) fn rows(file: &'static str, blob: &[u8]) -> Result<Vec<String>, DataError> {
    let raw = miniz_oxide::inflate::decompress_to_vec(blob).map_err(|_| DataError {
        file,
        what: "does not inflate",
    })?;
    let text = String::from_utf8(raw).map_err(|_| DataError {
        file,
        what: "is not valid UTF-8",
    })?;
    Ok(text
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect())
}

/// `seq` (lowercase hex code points joined by `-`) as the emoji string.
pub(crate) fn seq_to_string(seq: &str) -> Option<String> {
    seq.split('-')
        .map(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
        .collect()
}
