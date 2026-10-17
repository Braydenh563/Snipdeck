//! The Windows `HTML Format` clipboard encoding (`CF_HTML`), built and parsed portably.
//!
//! `CF_HTML` is UTF-8 text: a header of `Key:Value` lines followed by an HTML document. The header
//! gives byte offsets, counted from the first byte of the data, of the document
//! (`StartHTML`/`EndHTML`) and of the fragment the user copied (`StartFragment`/`EndFragment`);
//! the fragment is also bracketed by `<!--StartFragment-->` / `<!--EndFragment-->` comments.
//! See "HTML Clipboard Format" on Microsoft Learn.

const START_MARKER: &str = "<!--StartFragment-->";
const END_MARKER: &str = "<!--EndFragment-->";
const PREFIX: &str = "<html>\r\n<body>\r\n<!--StartFragment-->";
const SUFFIX: &str = "<!--EndFragment-->\r\n</body>\r\n</html>";
/// The largest value the 10-digit offset fields can hold.
const MAX_OFFSET: usize = 9_999_999_999;

/// Wraps an HTML `fragment` in the `CF_HTML` header and document, NUL-terminated.
///
/// `None` if the result would not fit the 10-digit offset fields (about 10 GB).
pub fn build(fragment: &str) -> Option<Vec<u8>> {
    let header_len = header(0, 0, 0, 0).len();
    let start_html = header_len;
    let start_fragment = start_html + PREFIX.len();
    let end_fragment = start_fragment.checked_add(fragment.len())?;
    let end_html = end_fragment.checked_add(SUFFIX.len())?;
    if end_html > MAX_OFFSET {
        return None;
    }
    let header = header(start_html, end_html, start_fragment, end_fragment);
    debug_assert_eq!(header.len(), header_len);
    let mut out = Vec::with_capacity(end_html + 1);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(PREFIX.as_bytes());
    out.extend_from_slice(fragment.as_bytes());
    out.extend_from_slice(SUFFIX.as_bytes());
    // Readers treat the data as a C string; the NUL is outside EndHTML.
    out.push(0);
    Some(out)
}

fn header(
    start_html: usize,
    end_html: usize,
    start_fragment: usize,
    end_fragment: usize,
) -> String {
    format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\n\
         StartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
    )
}

/// The fragment of `CF_HTML` `data`, or `None` if it is not `CF_HTML`.
///
/// Uses the `StartFragment`/`EndFragment` offsets when they are valid and falls back to the
/// fragment comments, because some producers write offsets that are wrong. Invalid UTF-8 is
/// replaced rather than rejected.
pub fn parse(data: &[u8]) -> Option<String> {
    // The data is a C string: anything after a NUL is allocation slack.
    let data = data
        .iter()
        .position(|&b| b == 0)
        .map_or(data, |nul| &data[..nul]);
    let start = header_value(data, b"StartFragment:");
    let end = header_value(data, b"EndFragment:");
    if let (Some(start), Some(end)) = (start, end)
        && start <= end
        && end <= data.len()
    {
        return Some(String::from_utf8_lossy(&data[start..end]).into_owned());
    }
    let start = find(data, START_MARKER.as_bytes())? + START_MARKER.len();
    let end = start + find(&data[start..], END_MARKER.as_bytes())?;
    Some(String::from_utf8_lossy(&data[start..end]).into_owned())
}

/// The non-negative decimal value of header field `key` (e.g. `b"StartFragment:"`).
fn header_value(data: &[u8], key: &[u8]) -> Option<usize> {
    // The header ends where the document starts; only look at lines before the first `<`.
    let header_end = data.iter().position(|&b| b == b'<').unwrap_or(data.len());
    data[..header_end]
        .split(|&b| b == b'\n' || b == b'\r')
        .find_map(|line| line.strip_prefix(key))
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.trim().parse().ok())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(data: &[u8], key: &str) -> usize {
        header_value(data, key.as_bytes()).unwrap()
    }

    #[test]
    fn offsets_point_at_the_document_and_the_fragment() {
        let data = build("<b>héllo</b>").unwrap();
        let (sh, eh) = (field(&data, "StartHTML:"), field(&data, "EndHTML:"));
        let (sf, ef) = (field(&data, "StartFragment:"), field(&data, "EndFragment:"));
        assert!(data[sh..].starts_with(b"<html>"));
        assert!(data[..eh].ends_with(b"</html>"));
        assert_eq!(&data[sf..ef], "<b>héllo</b>".as_bytes());
        assert!(data[..sf].ends_with(START_MARKER.as_bytes()));
        assert!(data[ef..].starts_with(END_MARKER.as_bytes()));
        assert_eq!(data.last(), Some(&0));
        assert_eq!(eh, data.len() - 1);
    }

    #[test]
    fn offsets_are_ten_digits_zero_padded() {
        let data = build("x").unwrap();
        let text = String::from_utf8_lossy(&data);
        assert!(text.starts_with("Version:0.9\r\nStartHTML:0000000"));
        for key in ["StartHTML:", "EndHTML:", "StartFragment:", "EndFragment:"] {
            let line = text.lines().find(|l| l.starts_with(key)).unwrap();
            assert_eq!(line.len(), key.len() + 10, "{line}");
        }
    }

    #[test]
    fn offsets_count_bytes_not_characters() {
        let data = build("日本語").unwrap();
        let (sf, ef) = (field(&data, "StartFragment:"), field(&data, "EndFragment:"));
        assert_eq!(ef - sf, "日本語".len());
    }

    #[test]
    fn parse_round_trips_build() {
        for fragment in ["", "<p>a</p>", "<i>ü 😀</i>\r\n<p>x</p>"] {
            assert_eq!(parse(&build(fragment).unwrap()).as_deref(), Some(fragment));
        }
    }

    #[test]
    fn parse_reads_a_browser_style_document() {
        let html = "<html><body>\r\n<!--StartFragment--><a href=\"x\">link</a><!--EndFragment-->\r\n</body></html>";
        let header_len = "Version:1.0\r\nStartHTML:000000000\r\nEndHTML:000000000\r\nStartFragment:000000000\r\nEndFragment:000000000\r\nSourceURL:https://example.com/\r\n".len();
        let sf = header_len + html.find("<a").unwrap();
        let ef = header_len + html.find("<!--End").unwrap();
        let data = format!(
            "Version:1.0\r\nStartHTML:{header_len:09}\r\nEndHTML:{:09}\r\nStartFragment:{sf:09}\r\nEndFragment:{ef:09}\r\nSourceURL:https://example.com/\r\n{html}",
            header_len + html.len()
        );
        assert_eq!(
            parse(data.as_bytes()).as_deref(),
            Some("<a href=\"x\">link</a>")
        );
    }

    #[test]
    fn parse_ignores_trailing_nul_and_slack() {
        let mut data = build("<b>a</b>").unwrap();
        data.extend_from_slice(&[0, 0, 7, 7]);
        assert_eq!(parse(&data).as_deref(), Some("<b>a</b>"));
    }

    #[test]
    fn bad_offsets_fall_back_to_the_comments() {
        let data = "Version:0.9\r\nStartHTML:-1\r\nEndHTML:-1\r\nStartFragment:0000009999\r\nEndFragment:0000000001\r\n<html><body><!--StartFragment-->ok<!--EndFragment--></body></html>";
        assert_eq!(parse(data.as_bytes()).as_deref(), Some("ok"));
    }

    #[test]
    fn non_cf_html_is_none() {
        assert_eq!(parse(b"just text"), None);
        assert_eq!(parse(b""), None);
        assert_eq!(parse(b"<!--StartFragment-->unterminated"), None);
    }

    #[test]
    fn header_values_inside_the_document_are_not_read() {
        // A fragment containing "StartFragment:" text must not be mistaken for the header.
        let data = build("StartFragment:0000000000").unwrap();
        assert_eq!(parse(&data).as_deref(), Some("StartFragment:0000000000"));
    }
}
