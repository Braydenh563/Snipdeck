//! What a second launch hands to the running Snipdeck, and how it travels.
//!
//! A launch with no running instance acts on its own command line. A launch that finds one packs the
//! command line into a [`Request`], puts it in a shared [`Mailbox`] and signals the first instance
//! (`windows::single_instance`). **Everything that comes through here is untrusted input**: any
//! process of the same user can write the mailbox, and the shell hands over whatever a web page or
//! a file name says. So:
//!
//! - a request is one of a closed set ([`Request`]); a URL is matched against an allow-list
//!   ([`parse_url`]) and never interpreted; a file must be an absolute `.snipdeck` path
//!   ([`parse_file`]) and only opens the Import preview, which the user confirms;
//! - every text has a length cap, control characters are refused, nothing is passed to a shell;
//! - [`decode`] re-checks what [`encode`] wrote, byte for byte, and refuses anything else.
//!
//! The mailbox layout is plain bytes over a slice, so the framing is tested here and the Windows
//! side only maps memory.

use std::path::PathBuf;

use crate::associations::{EXTENSION, URL_SCHEME};

/// The most characters accepted in a path argument (Windows allows 32 767; no real archive path is
/// near this).
pub const MAX_PATH_CHARS: usize = 1024;
/// The most characters accepted in a URL argument (`snipdeck://settings/` is 20).
pub const MAX_URL_CHARS: usize = 64;
/// The largest frame payload: [`MAX_PATH_CHARS`] characters at four bytes each.
pub const MAX_PAYLOAD_BYTES: usize = MAX_PATH_CHARS * 4;

/// What a `snipdeck:` URL asks for. Nothing else can be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlAction {
    /// `snipdeck://open`: the main window.
    Open,
    /// `snipdeck://panel`: the Panel.
    Panel,
    /// `snipdeck://settings`: the Settings page.
    Settings,
}

impl UrlAction {
    const fn word(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Panel => "panel",
            Self::Settings => "settings",
        }
    }

    fn of_word(word: &str) -> Option<Self> {
        [Self::Open, Self::Panel, Self::Settings]
            .into_iter()
            .find(|a| a.word() == word)
    }
}

/// What a launch asks the running instance to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// A plain launch: show the app.
    Show,
    /// A `.snipdeck` file was opened: show its Import preview.
    OpenFile(PathBuf),
    /// A `snipdeck:` URL was opened.
    Url(UrlAction),
}

/// Why an argument is not a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RequestError {
    /// Over the length cap, or containing control or reserved characters.
    #[error("the argument is too long or contains characters that are not allowed")]
    Shape,
    /// A `snipdeck:` URL that is not on the allow-list.
    #[error("that snipdeck: address is not one Snipdeck understands")]
    Url,
    /// A path that is not an absolute `.snipdeck` file.
    #[error("that is not the path of a .snipdeck file")]
    File,
    /// A file or address given together with other arguments.
    #[error("a file or snipdeck: address must be the only argument")]
    NotAlone,
}

fn has_forbidden_char(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '*' | '?'))
}

/// Whether `text` starts like a URL of ours (`snipdeck:`, any case).
#[must_use]
pub fn is_url_argument(text: &str) -> bool {
    let head = text.as_bytes().get(..=URL_SCHEME.len());
    head.is_some_and(|head| {
        head[..URL_SCHEME.len()].eq_ignore_ascii_case(URL_SCHEME.as_bytes())
            && head[URL_SCHEME.len()] == b':'
    })
}

/// Reads `snipdeck://open`, `snipdeck://panel` or `snipdeck://settings` (any case, one optional
/// trailing `/`). Nothing else: no query, fragment, port, user, extra path or percent escape.
///
/// # Errors
///
/// [`RequestError::Shape`] for a long or non-ASCII text, [`RequestError::Url`] for anything not on
/// the list.
pub fn parse_url(text: &str) -> Result<UrlAction, RequestError> {
    if text.chars().count() > MAX_URL_CHARS || !text.is_ascii() || has_forbidden_char(text) {
        return Err(RequestError::Shape);
    }
    let lower = text.to_ascii_lowercase();
    let rest = lower
        .strip_prefix(URL_SCHEME)
        .and_then(|r| r.strip_prefix("://"))
        .ok_or(RequestError::Url)?;
    let word = rest.strip_suffix('/').unwrap_or(rest);
    UrlAction::of_word(word).ok_or(RequestError::Url)
}

/// Whether `path` is rooted on a drive (`C:\` or `C:/`) or a UNC share (`\\server\share`), and not
/// a device or verbatim path (`\\.\`, `\\?\`).
fn is_absolute_windows_path(path: &str) -> bool {
    let b = path.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'\\' | b'/') {
        return true;
    }
    if let Some(rest) = path.strip_prefix(r"\\") {
        let mut parts = rest.split('\\');
        let server = parts.next().unwrap_or_default();
        let share = parts.next().unwrap_or_default();
        let file = parts.next().unwrap_or_default();
        return !matches!(server, "" | "." | "?") && !share.is_empty() && !file.is_empty();
    }
    // Off Windows (development) a Unix absolute path stands in.
    cfg!(not(windows)) && path.starts_with('/')
}

/// Reads the path of a `.snipdeck` file: absolute, within the cap, with a real name before the
/// extension.
///
/// # Errors
///
/// [`RequestError::Shape`] or [`RequestError::File`].
pub fn parse_file(text: &str) -> Result<PathBuf, RequestError> {
    if text.chars().count() > MAX_PATH_CHARS || has_forbidden_char(text) {
        return Err(RequestError::Shape);
    }
    // The extension is compared on the last component, ignoring case (NTFS does).
    let name = text.rsplit(['\\', '/']).next().unwrap_or_default();
    let stem_len = name.len().saturating_sub(EXTENSION.len());
    let has_extension = name.len() > EXTENSION.len()
        && name.is_char_boundary(stem_len)
        && name[stem_len..].eq_ignore_ascii_case(EXTENSION);
    if !has_extension || !is_absolute_windows_path(text) {
        return Err(RequestError::File);
    }
    Ok(PathBuf::from(text))
}

/// Turns one command-line argument into a request: a `snipdeck:` URL, or a `.snipdeck` path.
///
/// # Errors
///
/// See [`RequestError`].
pub fn classify_argument(text: &str) -> Result<Request, RequestError> {
    if is_url_argument(text) {
        parse_url(text).map(Request::Url)
    } else {
        parse_file(text).map(Request::OpenFile)
    }
}

/// Whether a command line (after the program name) breaks the rule that a request travels alone.
///
/// A file or `snipdeck:` address comes from the shell or a web page, which must not be able to add
/// `--screenshot`, `--data-dir` or any other flag. So when the first argument is not a flag, or any
/// argument is a `snipdeck:` address, the line must be that one argument.
///
/// # Errors
///
/// [`RequestError::NotAlone`].
pub fn check_alone(args: &[&str]) -> Result<(), RequestError> {
    let first_is_request = args.first().is_some_and(|a| !a.starts_with('-') && !a.is_empty());
    let has_url = args.iter().any(|a| is_url_argument(a));
    if (first_is_request || has_url) && args.len() > 1 {
        return Err(RequestError::NotAlone);
    }
    Ok(())
}

// ---- Wire format -----------------------------------------------------------------------------

const MAGIC: [u8; 3] = *b"SDH";
const VERSION: u8 = 1;
const KIND_SHOW: u8 = 0;
const KIND_FILE: u8 = 1;
const KIND_URL: u8 = 2;
const HEADER_LEN: usize = 3 + 1 + 1 + 2;

/// Why bytes are not a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Too short, wrong magic or version, a length that does not match, or trailing bytes.
    #[error("not a hand-over frame")]
    Frame,
    /// A well-formed frame whose content fails the same checks as a command-line argument.
    #[error("the hand-over content is not allowed")]
    Content,
}

/// The bytes for `request`. A path that cannot be sent (too long, not UTF-8, not a `.snipdeck`
/// path) becomes [`Request::Show`]: the launch still raises the app.
#[must_use]
pub fn encode(request: &Request) -> Vec<u8> {
    let (kind, payload): (u8, String) = match request {
        Request::Show => (KIND_SHOW, String::new()),
        Request::Url(action) => (KIND_URL, action.word().to_owned()),
        Request::OpenFile(path) => match path.to_str().filter(|p| parse_file(p).is_ok()) {
            Some(text) => (KIND_FILE, text.to_owned()),
            None => (KIND_SHOW, String::new()),
        },
    };
    let len = u16::try_from(payload.len()).unwrap_or(0);
    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    frame.extend_from_slice(&MAGIC);
    frame.push(VERSION);
    frame.push(kind);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(payload.as_bytes());
    frame
}

/// Reads a frame written by [`encode`], and nothing else.
///
/// # Errors
///
/// See [`DecodeError`].
pub fn decode(bytes: &[u8]) -> Result<Request, DecodeError> {
    let header = bytes.get(..HEADER_LEN).ok_or(DecodeError::Frame)?;
    if header[..3] != MAGIC || header[3] != VERSION {
        return Err(DecodeError::Frame);
    }
    let len = usize::from(u16::from_le_bytes([header[5], header[6]]));
    if len > MAX_PAYLOAD_BYTES || bytes.len() != HEADER_LEN + len {
        return Err(DecodeError::Frame);
    }
    let payload = std::str::from_utf8(&bytes[HEADER_LEN..]).map_err(|_| DecodeError::Content)?;
    match header[4] {
        KIND_SHOW if len == 0 => Ok(Request::Show),
        KIND_FILE => parse_file(payload)
            .map(Request::OpenFile)
            .map_err(|_| DecodeError::Content),
        KIND_URL => UrlAction::of_word(payload)
            .map(Request::Url)
            .ok_or(DecodeError::Content),
        _ => Err(DecodeError::Frame),
    }
}

// ---- The shared mailbox ----------------------------------------------------------------------

/// The size of the shared memory section the mailbox lives in.
pub const SECTION_BYTES: usize = 8192;
const PID_AT: usize = 0;
const LEN_AT: usize = 8;
const FRAME_AT: usize = 16;

/// The first instance's process id and one pending request, laid out over a byte slice (the shared
/// section on Windows; zeroed memory is a valid empty mailbox).
///
/// One writer at a time (a lock the second launches take) and one reader (the first instance).
#[derive(Debug)]
pub struct Mailbox<'a>(&'a mut [u8]);

impl<'a> Mailbox<'a> {
    /// A mailbox over `bytes`, which must be [`SECTION_BYTES`] long.
    #[must_use]
    pub fn over(bytes: &'a mut [u8]) -> Option<Self> {
        (bytes.len() == SECTION_BYTES).then_some(Self(bytes))
    }

    /// The first instance records its process id.
    pub fn set_owner(&mut self, pid: u32) {
        self.0[PID_AT..PID_AT + 4].copy_from_slice(&pid.to_le_bytes());
    }

    /// The first instance's process id, if it has recorded one yet.
    #[must_use]
    pub fn owner(&self) -> Option<u32> {
        let pid = u32::from_le_bytes([
            self.0[PID_AT],
            self.0[PID_AT + 1],
            self.0[PID_AT + 2],
            self.0[PID_AT + 3],
        ]);
        (pid != 0).then_some(pid)
    }

    /// Puts `request` in the box, replacing any earlier one.
    pub fn put(&mut self, request: &Request) {
        let frame = encode(request);
        let len = u16::try_from(frame.len()).unwrap_or(0);
        self.0[FRAME_AT..FRAME_AT + frame.len()].copy_from_slice(&frame);
        self.0[LEN_AT..LEN_AT + 2].copy_from_slice(&len.to_le_bytes());
    }

    /// Takes the pending request out, leaving the box empty. `None` when nothing was pending; a
    /// frame that fails [`decode`] counts as [`Request::Show`] (someone launched the app).
    pub fn take(&mut self) -> Option<Request> {
        let len = usize::from(u16::from_le_bytes([self.0[LEN_AT], self.0[LEN_AT + 1]]));
        if len == 0 {
            return None;
        }
        let end = (FRAME_AT + len).min(SECTION_BYTES);
        let request = decode(&self.0[FRAME_AT..end]).unwrap_or(Request::Show);
        self.0[LEN_AT..LEN_AT + 2].fill(0);
        self.0[FRAME_AT..end].fill(0);
        Some(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r"C:\Users\Ann\Documents\Snipdeck\backup.snipdeck";

    #[test]
    fn the_url_allow_list_is_exactly_three_words() {
        assert_eq!(parse_url("snipdeck://open"), Ok(UrlAction::Open));
        assert_eq!(parse_url("snipdeck://panel/"), Ok(UrlAction::Panel));
        assert_eq!(parse_url("SnipDeck://SETTINGS"), Ok(UrlAction::Settings));
        for bad in [
            "snipdeck://",
            "snipdeck://open//",
            "snipdeck://open?x=1",
            "snipdeck://open#a",
            "snipdeck://open/x",
            "snipdeck://user@open",
            "snipdeck://open:80",
            "snipdeck://%6fpen",
            "snipdeck://opens",
            "snipdeck:open",
            "snipdeck:/open",
            "http://open",
            "xsnipdeck://open",
            "snipdeck://open ",
            "snipdeck://open\n",
            "snipdeck://\u{43e}pen",
        ] {
            assert!(parse_url(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            parse_url(&format!("snipdeck://{}", "a".repeat(80))),
            Err(RequestError::Shape)
        );
    }

    #[test]
    fn a_file_must_be_an_absolute_snipdeck_path() {
        assert_eq!(parse_file(FILE), Ok(PathBuf::from(FILE)));
        assert!(parse_file(r"C:/x/Backup.SNIPDECK").is_ok());
        assert!(parse_file(r"\\server\share\a.snipdeck").is_ok());
        for bad in [
            "a.snipdeck",
            r"..\a.snipdeck",
            r"C:a.snipdeck",
            r"C:\x\.snipdeck",
            r"C:\x\a.snipdecks",
            r"C:\x\a.zip",
            r"C:\x\a.snipdeck.exe",
            r"\\?\C:\x\a.snipdeck",
            r"\\.\C:\x\a.snipdeck",
            r"\\server\a.snipdeck",
            "C:\\x\\a\0.snipdeck",
            "C:\\x\\a\n.snipdeck",
            "C:\\x\\\"a.snipdeck",
            r"C:\x\a|b.snipdeck",
            "",
        ] {
            assert!(parse_file(bad).is_err(), "{bad:?}");
        }
        let long = format!(r"C:\{}.snipdeck", "a".repeat(MAX_PATH_CHARS));
        assert_eq!(parse_file(&long), Err(RequestError::Shape));
    }

    #[test]
    fn an_argument_is_a_url_or_a_file() {
        assert_eq!(
            classify_argument("snipdeck://panel"),
            Ok(Request::Url(UrlAction::Panel))
        );
        assert_eq!(
            classify_argument(FILE),
            Ok(Request::OpenFile(PathBuf::from(FILE)))
        );
        assert_eq!(classify_argument("snipdeck://evil"), Err(RequestError::Url));
        assert_eq!(classify_argument("notes.txt"), Err(RequestError::File));
        assert!(!is_url_argument("snip"));
        assert!(!is_url_argument("\u{e9}"));
    }

    #[test]
    fn a_request_is_accepted_only_alone() {
        assert_eq!(check_alone(&[]), Ok(()));
        assert_eq!(check_alone(&["snipdeck://open"]), Ok(()));
        assert_eq!(check_alone(&["--hidden", "--dark"]), Ok(()));
        for line in [
            &["snipdeck://open", "--screenshot", "x.png"][..],
            &["--data-dir", "C:\\x", "snipdeck://open"],
            &["snipdeck://open", "--hidden"],
            &["--hidden", "snipdeck://open"],
            &["snipdeck://open", "snipdeck://panel"],
            &["C:\\a.snipdeck", "--data-dir", "C:\\x"],
        ] {
            assert_eq!(check_alone(line), Err(RequestError::NotAlone), "{line:?}");
        }
    }

    #[test]
    fn frames_round_trip() {
        for request in [
            Request::Show,
            Request::Url(UrlAction::Settings),
            Request::OpenFile(PathBuf::from(FILE)),
        ] {
            assert_eq!(decode(&encode(&request)), Ok(request));
        }
    }

    #[test]
    fn a_path_that_cannot_be_sent_becomes_a_plain_show() {
        let long = PathBuf::from(format!(r"C:\{}.snipdeck", "a".repeat(2000)));
        assert_eq!(decode(&encode(&Request::OpenFile(long))), Ok(Request::Show));
        assert_eq!(
            decode(&encode(&Request::OpenFile(PathBuf::from("rel.snipdeck")))),
            Ok(Request::Show)
        );
    }

    #[test]
    fn decode_refuses_everything_encode_does_not_write() {
        let good = encode(&Request::OpenFile(PathBuf::from(FILE)));
        // Truncated at every length, and with a byte added.
        for n in 0..good.len() {
            assert_eq!(decode(&good[..n]), Err(DecodeError::Frame), "cut at {n}");
        }
        let mut longer = good.clone();
        longer.push(0);
        assert_eq!(decode(&longer), Err(DecodeError::Frame));
        // A flipped magic, version or kind.
        for at in 0..5 {
            let mut bad = good.clone();
            bad[at] ^= 0x40;
            assert!(decode(&bad).is_err(), "byte {at}");
        }
        // Valid frame, hostile content.
        let payload = b"C:\\x\\a.exe";
        let mut frame = vec![b'S', b'D', b'H', 1, KIND_FILE];
        frame.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(payload);
        assert_eq!(decode(&frame), Err(DecodeError::Content));
        let mut frame = vec![b'S', b'D', b'H', 1, KIND_URL, 4, 0];
        frame.extend_from_slice(b"evil");
        assert_eq!(decode(&frame), Err(DecodeError::Content));
        // A Show with a payload is not a Show.
        let frame = [b'S', b'D', b'H', 1, KIND_SHOW, 1, 0, b'x'];
        assert!(decode(&frame).is_err());
        // Invalid UTF-8.
        let frame = [b'S', b'D', b'H', 1, KIND_FILE, 1, 0, 0xFF];
        assert_eq!(decode(&frame), Err(DecodeError::Content));
        // An oversized declared length.
        let big = u16::try_from(MAX_PAYLOAD_BYTES + 1).unwrap().to_le_bytes();
        assert_eq!(
            decode(&[b'S', b'D', b'H', 1, KIND_FILE, big[0], big[1]]),
            Err(DecodeError::Frame)
        );
    }

    #[test]
    fn the_mailbox_carries_the_owner_and_one_request() {
        let mut bytes = vec![0_u8; SECTION_BYTES];
        let mut mailbox = Mailbox::over(&mut bytes).expect("right size");
        assert_eq!(mailbox.owner(), None);
        assert_eq!(mailbox.take(), None);
        mailbox.set_owner(4242);
        assert_eq!(mailbox.owner(), Some(4242));
        mailbox.put(&Request::OpenFile(PathBuf::from(FILE)));
        assert_eq!(mailbox.take(), Some(Request::OpenFile(PathBuf::from(FILE))));
        assert_eq!(mailbox.take(), None, "taking empties the box");
        assert_eq!(mailbox.owner(), Some(4242), "and keeps the owner");
        assert!(
            bytes[FRAME_AT..].iter().all(|b| *b == 0),
            "nothing left over"
        );
    }

    #[test]
    fn a_damaged_mailbox_still_counts_as_a_launch() {
        let mut bytes = vec![0_u8; SECTION_BYTES];
        bytes[LEN_AT] = 40;
        bytes[FRAME_AT..FRAME_AT + 40].fill(0xAA);
        let mut mailbox = Mailbox::over(&mut bytes).expect("right size");
        assert_eq!(mailbox.take(), Some(Request::Show));
        // A length past the section is clamped, not trusted.
        bytes[LEN_AT..LEN_AT + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        let mut mailbox = Mailbox::over(&mut bytes).expect("right size");
        assert_eq!(mailbox.take(), Some(Request::Show));
        assert!(Mailbox::over(&mut [0_u8; 10]).is_none());
    }
}
