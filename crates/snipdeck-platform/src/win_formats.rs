//! Byte layouts of the Windows clipboard formats Snipdeck writes, built portably so they are
//! unit-tested on Linux. The Windows backend (`windows::clipboard`) only moves these bytes in
//! and out of `HGLOBAL`s. HTML lives in [`crate::cf_html`].

/// `CF_BITMAP`, `CF_METAFILEPICT`, `CF_PALETTE`, `CF_ENHMETAFILE`: GDI handles, not memory.
const GDI_HANDLE_FORMATS: [u32; 4] = [2, 3, 9, 14];
/// `CF_OWNERDISPLAY`, `CF_DSPBITMAP`, `CF_DSPMETAFILEPICT`, `CF_DSPENHMETAFILE`.
const OWNER_HANDLE_FORMATS: [u32; 4] = [0x80, 0x82, 0x83, 0x8E];
/// `CF_PRIVATEFIRST..=CF_PRIVATELAST`: owner-private handles the system does not manage, and
/// `CF_GDIOBJFIRST..=CF_GDIOBJLAST`: GDI objects.
const PRIVATE_AND_GDI_RANGE: std::ops::RangeInclusive<u32> = 0x200..=0x3FF;

/// Whether clipboard `format` holds plain `HGLOBAL` memory whose bytes can be copied and put
/// back later without the owner. The others hold handles that die with their owner.
pub fn is_memory_format(format: u32) -> bool {
    format != 0
        && !GDI_HANDLE_FORMATS.contains(&format)
        && !OWNER_HANDLE_FORMATS.contains(&format)
        && !PRIVATE_AND_GDI_RANGE.contains(&format)
}

/// The value of the `CanIncludeInClipboardHistory` and `CanUploadToCloudClipboard` markers that
/// keeps content out of history and cloud sync: a little-endian `DWORD` 0. The same bytes serve
/// for `ExcludeClipboardContentFromMonitorProcessing`, where only presence matters.
pub const MARKER_DWORD_ZERO: [u8; 4] = 0u32.to_le_bytes();

/// `DROPEFFECT_COPY` as the `Preferred DropEffect` value, so Explorer copies pasted files
/// instead of moving them out of the paste folder.
pub const DROPEFFECT_COPY: [u8; 4] = 1u32.to_le_bytes();

/// `CF_UNICODETEXT` bytes: UTF-16LE with a terminating NUL.
pub fn unicode_text(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// The text of `CF_UNICODETEXT` bytes, up to the first NUL. An odd trailing byte is ignored and
/// unpaired surrogates are replaced.
pub fn from_unicode_text(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Size of the `DROPFILES` header that precedes the file list in `CF_HDROP`.
const DROPFILES_SIZE: u32 = 20;

/// `CF_HDROP` bytes: a `DROPFILES` header (`pFiles` = 20, `pt` = 0,0, `fNC` = FALSE,
/// `fWide` = TRUE) and the UTF-16 paths, each NUL-terminated, then one more NUL.
///
/// `None` if there are no paths or a path is empty or contains a NUL (it would end the list).
pub fn drop_files<P: AsRef<[u16]>>(paths: &[P]) -> Option<Vec<u8>> {
    if paths.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    out.extend_from_slice(&DROPFILES_SIZE.to_le_bytes()); // pFiles
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0i32.to_le_bytes()); // fNC
    out.extend_from_slice(&1i32.to_le_bytes()); // fWide
    for path in paths {
        let path = path.as_ref();
        if path.is_empty() || path.contains(&0) {
            return None;
        }
        out.extend(path.iter().flat_map(|unit| unit.to_le_bytes()));
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    Some(out)
}

/// The paths in `CF_HDROP` bytes: the inverse of [`drop_files`]. Reads the `DROPFILES` header's
/// `pFiles` offset and `fWide` flag, then the NUL-separated list up to the empty entry that ends
/// it (or the end of the bytes). Empty if the header is short or `pFiles` points outside the bytes.
/// A narrow (ANSI) list is read as UTF-8, lossily: Explorer always offers the wide form.
pub fn parse_drop_files(bytes: &[u8]) -> Vec<String> {
    let header = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let (Some(start), Some(wide)) = (header(0), header(16)) else {
        return Vec::new();
    };
    let Some(list) = usize::try_from(start)
        .ok()
        .filter(|&s| s >= 20)
        .and_then(|s| bytes.get(s..))
    else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    if wide != 0 {
        let units: Vec<u16> = list
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        for entry in units.split(|&unit| unit == 0) {
            if entry.is_empty() {
                break;
            }
            paths.push(String::from_utf16_lossy(entry));
        }
    } else {
        for entry in list.split(|&byte| byte == 0) {
            if entry.is_empty() {
                break;
            }
            paths.push(String::from_utf8_lossy(entry).into_owned());
        }
    }
    paths
}

/// Whether `name` can be written directly inside the paste folder. Core validates names when an
/// item is saved; this repeats core's own check (one Windows-valid path component) as the last
/// guard before touching the disk.
pub fn is_plain_file_name(name: &str) -> bool {
    snipdeck_core::Inline::new(Vec::new())
        .and_then(|empty| {
            snipdeck_core::FileEntry::new(name, snipdeck_core::Content::Inline(empty))
        })
        .is_ok()
}

/// Paste folders are `paste-<random>` inside `%TEMP%\Snipdeck`; the older `paste-<n>` names of
/// earlier versions count as ours too, so they are cleaned up.
pub const PASTE_DIR_PREFIX: &str = "paste-";

/// Whether `name` is a paste folder of ours: the prefix and then only ASCII letters or digits.
pub fn is_paste_dir_name(name: &str) -> bool {
    name.strip_prefix(PASTE_DIR_PREFIX)
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// The folder name for 16 random bytes: `paste-` and 32 hex digits, unpredictable to other users
/// and processes.
pub fn paste_dir_name(random: [u8; 16]) -> String {
    use std::fmt::Write as _;
    let mut name = String::from(PASTE_DIR_PREFIX);
    for byte in random {
        // Writing to a `String` cannot fail.
        let _ = write!(name, "{byte:02x}");
    }
    name
}

/// Creates a fresh, randomly named paste folder under `root` (which must exist). `random` supplies
/// the 16 bytes, so a test can force a name clash.
///
/// # Errors
///
/// The I/O error if the folder cannot be created, or after 16 names were all taken.
pub fn claim_paste_dir(
    root: &std::path::Path,
    mut random: impl FnMut() -> std::io::Result<[u8; 16]>,
) -> std::io::Result<std::path::PathBuf> {
    let mut last = None;
    for _ in 0..16 {
        let dir = root.join(paste_dir_name(random()?));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no free paste folder name")))
}

/// 16 bytes from the operating system's random generator.
///
/// # Errors
///
/// An I/O error if the system has no randomness.
pub fn os_random() -> std::io::Result<[u8; 16]> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| std::io::Error::other("no system randomness"))?;
    Ok(bytes)
}

/// The most recently modified paste folder of ours under `root` (ties broken by name), if any.
/// The clipboard or a snapshot about to be restored may still point into it, so a paste keeps it.
pub fn newest_paste_dir(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let ours = entry.file_name().to_str().is_some_and(is_paste_dir_name);
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !ours || !kind.is_dir() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let candidate = (modified, entry.path());
        if best.as_ref().is_none_or(|current| candidate > *current) {
            best = Some(candidate);
        }
    }
    best.map(|(_, path)| path)
}

/// Removes every paste folder of ours under `root` except `keep` (best effort: a file still open
/// stays until a later call). Returns how many folders were removed. Other names are never
/// touched.
pub fn remove_paste_dirs(root: &std::path::Path, keep: Option<&std::path::Path>) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let ours = entry.file_name().to_str().is_some_and(is_paste_dir_name);
        // `file_type` does not follow links: a link planted under our name is not entered.
        let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if ours && is_dir && Some(path.as_path()) != keep && std::fs::remove_dir_all(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

/// The most memory one `CF_DIB` conversion may take for each of its two buffers (the decoded
/// pixels and the DIB). Bigger images are offered as PNG only, so a huge screenshot, or a small
/// PNG whose header claims a huge size, cannot exhaust memory at paste time.
pub const DIB_DECODE_LIMIT: usize = 128 * 1024 * 1024;

/// Size of a `BITMAPINFOHEADER`.
const BITMAPINFOHEADER_SIZE: usize = 40;

/// Bytes of a 24-bit `CF_DIB` of `width` x `height`: the header plus rows padded to four bytes.
/// `None` on overflow or if a dimension does not fit the header's `LONG`.
pub fn dib_size(width: u32, height: u32) -> Option<usize> {
    i32::try_from(width).ok()?;
    i32::try_from(height).ok()?;
    let stride = row_stride(usize::try_from(width).ok()?)?;
    stride
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_add(BITMAPINFOHEADER_SIZE)
}

fn row_stride(width: usize) -> Option<usize> {
    Some(width.checked_mul(3)?.checked_add(3)? & !3)
}

/// `CF_DIB` bytes for a PNG: a `BITMAPINFOHEADER` and 24-bit bottom-up BGR rows padded to four
/// bytes. Transparency is composited over white, because many `CF_DIB` readers ignore or
/// misread alpha and would show transparent areas as black; alpha-aware apps read the PNG.
///
/// `None` if the PNG cannot be decoded, or if its decoded pixels or the DIB would need more than
/// [`DIB_DECODE_LIMIT`] bytes. The size is checked from the header before anything is allocated,
/// and the buffers are reserved fallibly: this runs inside `WM_RENDERFORMAT`, where an
/// allocation failure would abort the process.
pub fn png_to_dib(png_bytes: &[u8]) -> Option<Vec<u8>> {
    png_to_dib_within(png_bytes, DIB_DECODE_LIMIT)
}

fn png_to_dib_within(png_bytes: &[u8], limit: usize) -> Option<Vec<u8>> {
    // `Limits` caps the decoder's own buffers only; the output buffer is ours to check.
    let mut decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(png_bytes),
        png::Limits { bytes: limit },
    );
    // Palette, tRNS, low bit depths and 16-bit samples all become 8-bit Gray/GrayA/RGB/RGBA.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = reader.info().size();
    let decoded_size = reader.output_buffer_size()?;
    let dib_size = dib_size(width, height)?;
    if decoded_size > limit || dib_size > limit {
        return None;
    }
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(decoded_size).ok()?;
    pixels.resize(decoded_size, 0);
    let info = reader.next_frame(&mut pixels).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let width = usize::try_from(info.width).ok()?;
    let height = usize::try_from(info.height).ok()?;
    let stride = row_stride(width)?;
    let image_size = dib_size.checked_sub(BITMAPINFOHEADER_SIZE)?;
    let rows_len = info.line_size.checked_mul(height)?;
    if rows_len > pixels.len() || width.checked_mul(channels)? > info.line_size {
        return None;
    }
    let mut out = Vec::new();
    out.try_reserve_exact(dib_size).ok()?;
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&i32::try_from(width).ok()?.to_le_bytes()); // biWidth
    out.extend_from_slice(&i32::try_from(height).ok()?.to_le_bytes()); // biHeight: bottom-up
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&u32::try_from(image_size).ok()?.to_le_bytes()); // biSizeImage
    out.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    out.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    out.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
    let padding = stride - width * 3;
    for row in pixels[..rows_len].chunks_exact(info.line_size).rev() {
        for pixel in row[..width * channels].chunks_exact(channels) {
            let (r, g, b, a) = match *pixel {
                [y] => (y, y, y, 255),
                [y, a] => (y, y, y, a),
                [r, g, b] => (r, g, b, 255),
                [r, g, b, a] => (r, g, b, a),
                _ => return None,
            };
            out.extend_from_slice(&[over_white(b, a), over_white(g, a), over_white(r, a)]);
        }
        out.extend(std::iter::repeat_n(0, padding));
    }
    // The decoded pixels are content too.
    pixels.fill(0);
    std::hint::black_box(&pixels);
    Some(out)
}

/// `channel` with straight alpha `alpha` composited over white, rounded.
fn over_white(channel: u8, alpha: u8) -> u8 {
    let (c, a) = (u32::from(channel), u32::from(alpha));
    let blended = (c * a + 255 * (255 - a) + 127) / 255;
    u8::try_from(blended).unwrap_or(u8::MAX)
}

/// A PNG for `CF_DIB` / `CF_DIBV5` bytes: the inverse of [`png_to_dib`], for a screenshot or a
/// picture copied from another program. Reads a `BITMAPINFOHEADER` or its V4/V5 successors with
/// 16, 24 or 32 bits per pixel, `BI_RGB` or `BI_BITFIELDS`, bottom-up or top-down. The PNG has
/// alpha only when a bit mask names it (a 32-bit `BI_RGB` pixel's fourth byte is unused, not
/// transparency). Palette and compressed DIBs are refused.
///
/// `None` if the bytes are not such a DIB, are shorter than the header says, or the decoded
/// pixels would need more than [`DIB_DECODE_LIMIT`] bytes. Sizes are checked from the header
/// before anything is allocated.
pub fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    dib_to_png_within(dib, DIB_DECODE_LIMIT)
}

/// A bit mask's channel: where it sits and how many bits it has.
#[derive(Clone, Copy)]
struct Mask {
    shift: u32,
    bits: u32,
}

impl Mask {
    fn new(mask: u32) -> Option<Self> {
        let bits = mask.count_ones();
        // One contiguous run of at most 16 bits.
        (bits > 0 && bits <= 16 && (mask >> mask.trailing_zeros()).count_ones() == bits).then_some(
            Self {
                shift: mask.trailing_zeros(),
                bits,
            },
        )
    }

    /// The channel of `pixel` scaled to 0..=255.
    fn get(self, pixel: u32) -> u8 {
        let max = (1u32 << self.bits) - 1;
        let value = (pixel >> self.shift) & max;
        u8::try_from((value * 255 + max / 2) / max).unwrap_or(u8::MAX)
    }
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn dib_to_png_within(dib: &[u8], limit: usize) -> Option<Vec<u8>> {
    let header_size = usize::try_from(le_u32(dib, 0)?).ok()?;
    if !matches!(header_size, 40 | 108 | 124) {
        return None;
    }
    let width = i32::from_le_bytes(dib.get(4..8)?.try_into().ok()?);
    let height = i32::from_le_bytes(dib.get(8..12)?.try_into().ok()?);
    let planes = u16::from_le_bytes(dib.get(12..14)?.try_into().ok()?);
    let bit_count = usize::from(u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?));
    let compression = le_u32(dib, 16)?;
    let colors_used = usize::try_from(le_u32(dib, 32)?).ok()?;
    if planes != 1 || width <= 0 || height == 0 || height == i32::MIN {
        return None;
    }
    let top_down = height < 0;
    let width = usize::try_from(width).ok()?;
    let rows = usize::try_from(height.unsigned_abs()).ok()?;
    let (default_masks, has_masks) = match (compression, bit_count) {
        (0, 24) => ([0; 4], false),
        (0, 32) => ([0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0], false),
        (0, 16) => ([0x7C00, 0x03E0, 0x001F, 0], false),
        // BI_BITFIELDS: three masks follow a 40-byte header; V4/V5 headers hold four.
        (3, 16 | 32) => ([0; 4], true),
        _ => return None,
    };
    let mut masks = default_masks;
    let mut pixel_start = header_size;
    if has_masks {
        let count = if header_size == 40 { 3 } else { 4 };
        for (i, mask) in masks.iter_mut().enumerate().take(count) {
            *mask = le_u32(dib, 40 + 4 * i)?;
        }
        if header_size == 40 {
            pixel_start += 12;
        }
    }
    // A colour table before the pixels (rare for 16+ bits) is skipped.
    pixel_start = pixel_start.checked_add(colors_used.checked_mul(4)?)?;
    let bytes_per_pixel = bit_count / 8;
    let stride = width.checked_mul(bit_count)?.checked_add(31)? / 32 * 4;
    let has_alpha = bit_count != 24 && masks[3] != 0;
    let channels = if has_alpha { 4 } else { 3 };
    let out_len = width.checked_mul(rows)?.checked_mul(channels)?;
    let dib_len = stride.checked_mul(rows)?;
    if out_len > limit || dib_len > limit {
        return None;
    }
    let pixel_end = pixel_start.checked_add(dib_len)?;
    let data = dib.get(pixel_start..pixel_end)?;
    let (r, g, b) = if bit_count == 24 {
        (None, None, None)
    } else {
        (
            Some(Mask::new(masks[0])?),
            Some(Mask::new(masks[1])?),
            Some(Mask::new(masks[2])?),
        )
    };
    let a = if has_alpha {
        Some(Mask::new(masks[3])?)
    } else {
        None
    };
    let mut pixels = Vec::new();
    pixels.try_reserve_exact(out_len).ok()?;
    for y in 0..rows {
        let src_row = if top_down { y } else { rows - 1 - y };
        let row = &data[src_row * stride..][..stride];
        for pixel in row.chunks_exact(bytes_per_pixel).take(width) {
            if let (Some(r), Some(g), Some(b)) = (r, g, b) {
                let mut raw = [0u8; 4];
                raw[..bytes_per_pixel].copy_from_slice(pixel);
                let value = u32::from_le_bytes(raw);
                pixels.extend_from_slice(&[r.get(value), g.get(value), b.get(value)]);
                if let Some(a) = a {
                    pixels.push(a.get(value));
                }
            } else {
                pixels.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(
        &mut out,
        u32::try_from(width).ok()?,
        u32::try_from(rows).ok()?,
    );
    encoder.set_color(if has_alpha {
        png::ColorType::Rgba
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&pixels).ok()?;
    writer.finish().ok()?;
    // The decoded pixels are content too.
    pixels.fill(0);
    std::hint::black_box(&pixels);
    Some(out)
}

/// What a restore needs to know about the clipboard, read while Snipdeck holds it open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreCheck {
    /// The transient write being restored is the latest one Snipdeck made.
    pub same_write: bool,
    /// No `WM_DESTROYCLIPBOARD` arrived since that write: nobody emptied the clipboard.
    pub never_emptied: bool,
    /// `GetClipboardOwner()` is still Snipdeck's owner window.
    pub still_owner: bool,
    /// `GetClipboardSequenceNumber()` now.
    pub sequence: u32,
    /// The sequence number Snipdeck last recorded as its own (see [`OwnerLedger`]).
    pub own_sequence: u32,
}

impl RestoreCheck {
    /// Whether the clipboard still holds exactly what Snipdeck put there, so the snapshot may
    /// replace it. Anything else means newer content, which is never overwritten.
    ///
    /// Replacing content requires `EmptyClipboard`, which makes the caller the owner and sends
    /// the old owner `WM_DESTROYCLIPBOARD`, so ownership catches every replacement. The
    /// sequence number additionally catches a format added without emptying.
    pub fn unchanged(&self) -> bool {
        self.same_write
            && self.never_emptied
            && self.still_owner
            && self.sequence == self.own_sequence
    }
}

/// The clipboard owner's bookkeeping: which write is current, whether someone emptied the
/// clipboard since, and which sequence number is Snipdeck's own. The Windows backend feeds it
/// the events in the order they happen on its single thread.
///
/// Sequence numbers recorded as ours: after a write (read after `CloseClipboard`), after each
/// delayed render (read inside `WM_RENDERFORMAT`, while the requester holds the clipboard open),
/// and once after a render, at the first `WM_CLIPBOARDUPDATE` showing exactly one more, in case
/// the requester's `CloseClipboard` also bumps the number (undocumented either way). An update
/// showing the same number (a stale notification, or Windows does not bump on close) keeps
/// waiting; one showing a bigger jump means someone else changed the clipboard, and is never
/// taken as ours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OwnerLedger {
    generation: u64,
    emptied: bool,
    own_sequence: u32,
    awaiting_close_bump: bool,
}

impl OwnerLedger {
    /// Our `EmptyClipboard` succeeded for a new write (or a restore): returns its generation.
    /// Every earlier write's generation stops matching, even if this write then fails.
    pub fn begin_write(&mut self) -> u64 {
        self.generation += 1;
        self.emptied = false;
        self.awaiting_close_bump = false;
        self.generation
    }

    /// The write was closed with the clipboard at `sequence`.
    pub fn wrote(&mut self, sequence: u32) {
        self.own_sequence = sequence;
    }

    /// We answered `WM_RENDERFORMAT`; the clipboard is now at `sequence`.
    pub fn rendered(&mut self, sequence: u32) {
        if !self.emptied {
            self.own_sequence = sequence;
            self.awaiting_close_bump = true;
        }
    }

    /// `WM_DESTROYCLIPBOARD`: someone (possibly we) emptied the clipboard.
    pub fn destroyed(&mut self) {
        self.emptied = true;
        self.awaiting_close_bump = false;
    }

    /// Whether a `WM_CLIPBOARDUPDATE` is worth reading the owner and sequence number for.
    pub fn wants_update(&self) -> bool {
        self.awaiting_close_bump && !self.emptied
    }

    /// `WM_CLIPBOARDUPDATE`, with whether we are still the owner and the sequence number now.
    pub fn clipboard_updated(&mut self, still_owner: bool, sequence: u32) {
        if !self.wants_update() || sequence == self.own_sequence {
            return;
        }
        if still_owner && sequence == self.own_sequence.wrapping_add(1) {
            self.own_sequence = sequence;
        }
        self.awaiting_close_bump = false;
    }

    /// The generation of the latest write.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The restore decision for the write `generation`, given what the clipboard shows now.
    pub fn check(&self, generation: u64, still_owner: bool, sequence: u32) -> RestoreCheck {
        RestoreCheck {
            same_write: self.generation == generation,
            never_emptied: !self.emptied,
            still_owner,
            sequence,
            own_sequence: self.own_sequence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_formats_are_not_memory() {
        for format in [
            0, 2, 3, 9, 14, 0x80, 0x82, 0x83, 0x8E, 0x200, 0x2FF, 0x300, 0x3FF,
        ] {
            assert!(!is_memory_format(format), "{format:#x}");
        }
        // CF_TEXT, CF_DIB, CF_UNICODETEXT, CF_HDROP, CF_LOCALE, CF_DIBV5, CF_DSPTEXT, registered.
        for format in [1, 8, 13, 15, 16, 17, 0x81, 0x400, 0xC000, 0xFFFF] {
            assert!(is_memory_format(format), "{format:#x}");
        }
    }

    #[test]
    fn unicode_text_is_nul_terminated_utf16le() {
        assert_eq!(unicode_text("Aé"), [0x41, 0, 0xE9, 0, 0, 0]);
        assert_eq!(unicode_text("😀").len(), 6);
        assert_eq!(unicode_text(""), [0, 0]);
    }

    #[test]
    fn unicode_text_round_trips_and_stops_at_nul() {
        let mut bytes = unicode_text("héllo 😀\r\nworld");
        bytes.extend_from_slice(&[0x41, 0, 7]); // GlobalSize slack after the NUL, odd length
        assert_eq!(from_unicode_text(&bytes), "héllo 😀\r\nworld");
        assert_eq!(from_unicode_text(&[0x41, 0, 0x42]), "A");
        assert_eq!(from_unicode_text(&[]), "");
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn drop_files_has_the_dropfiles_header_and_a_double_nul_list() {
        let bytes = drop_files(&[wide(r"C:\a.txt"), wide(r"C:\b")]).unwrap();
        assert_eq!(bytes[..4], 20u32.to_le_bytes());
        assert_eq!(bytes[4..16], [0; 12]);
        assert_eq!(bytes[16..20], 1u32.to_le_bytes());
        let list: Vec<u16> = bytes[20..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        let mut expected = wide(r"C:\a.txt");
        expected.push(0);
        expected.extend(wide(r"C:\b"));
        expected.extend([0, 0]);
        assert_eq!(list, expected);
    }

    #[test]
    fn drop_files_rejects_empty_lists_and_bad_paths() {
        assert_eq!(drop_files::<Vec<u16>>(&[]), None);
        assert_eq!(drop_files(&[wide("")]), None);
        assert_eq!(drop_files(&[vec![0x41, 0, 0x42]]), None);
    }

    #[test]
    fn only_single_components_are_plain_file_names() {
        for good in ["a.txt", "Résumé (final).docx", "x", ".hidden"] {
            assert!(is_plain_file_name(good), "{good}");
        }
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            r"a\b",
            "C:x",
            "a?",
            "a\0b",
            "trailing.",
            "NUL.txt",
        ] {
            assert!(!is_plain_file_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn only_our_paste_folder_names_are_recognised() {
        for ours in [
            "paste-0",
            "paste-42",
            "paste-9f86d081884c7d659a2feaa0c55ad015",
        ] {
            assert!(is_paste_dir_name(ours), "{ours}");
        }
        for other in [
            "paste-",
            "paste--1",
            "paste-1.a",
            "Paste-1",
            "other",
            "paste",
        ] {
            assert!(!is_paste_dir_name(other), "{other}");
        }
        assert_eq!(paste_dir_name([0xab; 16]).len(), "paste-".len() + 32);
        assert!(is_paste_dir_name(&paste_dir_name([0; 16])));
    }

    #[test]
    fn a_new_paste_folder_has_a_random_name_and_survives_a_name_clash() {
        let root = tempfile::tempdir().unwrap();
        let first = claim_paste_dir(root.path(), || Ok([1; 16])).unwrap();
        let mut calls = 0;
        let second = claim_paste_dir(root.path(), || {
            calls += 1;
            Ok([if calls == 1 { 1 } else { 2 }; 16])
        })
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(calls, 2, "the clash was retried with a new name");
        let a = claim_paste_dir(root.path(), os_random).unwrap();
        let b = claim_paste_dir(root.path(), os_random).unwrap();
        assert_ne!(a, b);
        let always_taken = claim_paste_dir(root.path(), || Ok([1; 16]));
        assert!(
            always_taken.is_err(),
            "gives up rather than looping forever"
        );
    }

    #[test]
    fn cleanup_removes_every_paste_folder_but_the_kept_one_and_nothing_else() {
        let root = tempfile::tempdir().unwrap();
        let old = claim_paste_dir(root.path(), || Ok([1; 16])).unwrap();
        std::fs::write(old.join("secret.txt"), b"x").unwrap();
        let legacy = root.path().join("paste-7");
        std::fs::create_dir(&legacy).unwrap();
        let keep = claim_paste_dir(root.path(), || Ok([2; 16])).unwrap();
        let other = root.path().join("keep-me");
        std::fs::create_dir(&other).unwrap();
        let file = root.path().join("paste-file");
        std::fs::write(&file, b"not a folder").unwrap();

        assert_eq!(remove_paste_dirs(root.path(), Some(&keep)), 2);
        assert!(!old.exists() && !legacy.exists());
        assert!(keep.is_dir() && other.is_dir() && file.is_file());

        assert_eq!(
            remove_paste_dirs(root.path(), None),
            1,
            "the next-start cleanup"
        );
        assert!(!keep.exists() && other.is_dir());
        assert_eq!(remove_paste_dirs(&root.path().join("missing"), None), 0);
    }

    #[test]
    fn the_newest_paste_folder_is_the_one_a_paste_keeps() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(newest_paste_dir(root.path()), None);
        let old = claim_paste_dir(root.path(), || Ok([1; 16])).unwrap();
        let new = claim_paste_dir(root.path(), || Ok([2; 16])).unwrap();
        let other = root.path().join("newer-but-not-ours");
        std::fs::create_dir(&other).unwrap();
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let set = |dir: &std::path::Path, secs: u64| {
            std::fs::File::open(dir)
                .unwrap()
                .set_modified(t + std::time::Duration::from_secs(secs))
                .unwrap();
        };
        set(&old, 0);
        set(&new, 10);
        set(&other, 20);
        assert_eq!(newest_paste_dir(root.path()), Some(new.clone()));
        // The paste sequence: keep the newest, drop the rest, then claim a fresh one.
        let keep = newest_paste_dir(root.path());
        assert_eq!(remove_paste_dirs(root.path(), keep.as_deref()), 1);
        assert!(new.is_dir() && !old.exists() && other.is_dir());
    }

    fn encode_png(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn dib_header_describes_a_bottom_up_24_bit_bitmap() {
        let png = encode_png(3, 2, png::ColorType::Rgb, &[0; 18]);
        let dib = png_to_dib(&png).unwrap();
        assert_eq!(u32_at(&dib, 0), 40);
        assert_eq!(u32_at(&dib, 4), 3);
        assert_eq!(u32_at(&dib, 8), 2); // positive height: bottom-up
        assert_eq!(dib[12..14], 1u16.to_le_bytes());
        assert_eq!(dib[14..16], 24u16.to_le_bytes());
        assert_eq!(u32_at(&dib, 16), 0); // BI_RGB
        // 3 px * 3 bytes = 9, padded to 12 per row.
        assert_eq!(u32_at(&dib, 20), 24);
        assert_eq!(dib.len(), 40 + 24);
    }

    #[test]
    fn dib_rows_are_bottom_up_bgr_and_padded() {
        // Row 0 (top): red, green. Row 1 (bottom): blue, white.
        let rgb = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
        let dib = png_to_dib(&encode_png(2, 2, png::ColorType::Rgb, &rgb)).unwrap();
        let pixels = &dib[40..];
        // First stored row is the bottom one: blue then white, then 2 bytes of padding.
        assert_eq!(pixels[..8], [255, 0, 0, 255, 255, 255, 0, 0]);
        assert_eq!(pixels[8..16], [0, 0, 255, 0, 255, 0, 0, 0]);
    }

    #[test]
    fn dib_composites_transparency_over_white() {
        // Transparent black, opaque black, half-transparent red.
        let rgba = [0, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 128];
        let dib = png_to_dib(&encode_png(3, 1, png::ColorType::Rgba, &rgba)).unwrap();
        assert_eq!(dib[40..49], [255, 255, 255, 0, 0, 0, 127, 127, 255]);
    }

    #[test]
    fn dib_accepts_gray_images() {
        let dib = png_to_dib(&encode_png(1, 1, png::ColorType::Grayscale, &[200])).unwrap();
        assert_eq!(dib[40..43], [200, 200, 200]);
        let dib = png_to_dib(&encode_png(1, 1, png::ColorType::GrayscaleAlpha, &[0, 0])).unwrap();
        assert_eq!(dib[40..43], [255, 255, 255]);
    }

    #[test]
    fn invalid_png_has_no_dib() {
        assert_eq!(png_to_dib(b"not a png"), None);
        assert_eq!(png_to_dib(&[]), None);
    }

    fn decode_png(bytes: &[u8]) -> (u32, u32, png::ColorType, Vec<u8>) {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info.width, info.height, info.color_type, buf)
    }

    #[test]
    fn dib_round_trips_through_png_for_1x1_odd_and_even_widths() {
        let cases: [(u32, u32); 4] = [(1, 1), (3, 2), (4, 3), (5, 1)];
        for (w, h) in cases {
            let rgb: Vec<u8> = (0..w * h * 3).map(|i| (i * 7 + 3) as u8).collect();
            let png = encode_png(w, h, png::ColorType::Rgb, &rgb);
            let back = dib_to_png(&png_to_dib(&png).unwrap()).unwrap();
            let (bw, bh, color, pixels) = decode_png(&back);
            assert_eq!((bw, bh, color), (w, h, png::ColorType::Rgb), "{w}x{h}");
            assert_eq!(pixels, rgb, "{w}x{h}");
        }
    }

    #[test]
    fn dib_round_trip_of_alpha_is_composited_over_white() {
        // png_to_dib drops alpha over white, so that is what comes back; opaque pixels are exact.
        let rgba = [10, 20, 30, 255, 0, 0, 0, 0, 255, 0, 0, 128];
        let png = encode_png(3, 1, png::ColorType::Rgba, &rgba);
        let (w, h, _, pixels) = decode_png(&dib_to_png(&png_to_dib(&png).unwrap()).unwrap());
        assert_eq!((w, h), (3, 1));
        assert_eq!(pixels, [10, 20, 30, 255, 255, 255, 255, 127, 127]);
    }

    #[test]
    fn dib_to_png_reads_top_down_and_bitfields() {
        // 2x1, 32 bit, BI_BITFIELDS in a 40-byte header, masks after it, top-down (-1 height).
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&2i32.to_le_bytes());
        dib.extend_from_slice(&(-1i32).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&3u32.to_le_bytes());
        dib.extend_from_slice(&[0; 20]);
        for mask in [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF] {
            dib.extend_from_slice(&mask.to_le_bytes());
        }
        // Pixels as B, G, R, X: red then blue.
        dib.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        let (w, h, color, pixels) = decode_png(&dib_to_png(&dib).unwrap());
        assert_eq!((w, h, color), (2, 1, png::ColorType::Rgb));
        assert_eq!(pixels, [255, 0, 0, 0, 0, 255]);
    }

    #[test]
    fn dib_to_png_keeps_alpha_when_a_mask_names_it() {
        // 1x1 BITMAPV5HEADER (124 bytes) 32 bit, BI_BITFIELDS with an alpha mask.
        let mut dib = vec![0u8; 124];
        dib[0..4].copy_from_slice(&124u32.to_le_bytes());
        dib[4..8].copy_from_slice(&1i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1i32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&32u16.to_le_bytes());
        dib[16..20].copy_from_slice(&3u32.to_le_bytes());
        for (at, mask) in [
            (40, 0x00FF_0000u32),
            (44, 0x0000_FF00),
            (48, 0x0000_00FF),
            (52, 0xFF00_0000),
        ] {
            dib[at..at + 4].copy_from_slice(&mask.to_le_bytes());
        }
        dib.extend_from_slice(&[30, 20, 10, 128]); // B G R A
        let (_, _, color, pixels) = decode_png(&dib_to_png(&dib).unwrap());
        assert_eq!(color, png::ColorType::Rgba);
        assert_eq!(pixels, [10, 20, 30, 128]);
    }

    #[test]
    fn dib_to_png_rejects_truncated_and_oversized() {
        let png = encode_png(4, 4, png::ColorType::Rgb, &[9; 48]);
        let dib = png_to_dib(&png).unwrap();
        assert!(dib_to_png(&dib[..dib.len() - 1]).is_none());
        assert!(dib_to_png(&dib[..30]).is_none());
        assert!(dib_to_png(&[]).is_none());
        // The header claims 30000 x 30000 pixels: refused from the header, before any allocation.
        let mut huge = dib[..40].to_vec();
        huge[4..8].copy_from_slice(&30_000i32.to_le_bytes());
        huge[8..12].copy_from_slice(&30_000i32.to_le_bytes());
        assert!(dib_to_png(&huge).is_none());
        // A small image over a small limit.
        assert!(dib_to_png_within(&dib, 10).is_none());
        assert!(dib_to_png_within(&dib, DIB_DECODE_LIMIT).is_some());
    }

    #[test]
    fn parse_drop_files_round_trips_drop_files() {
        let names = [
            "C:\\a\\b.txt",
            "D:\\Ünï cödé\\日本語.png",
            "E:\\x y\\😀.bin",
        ];
        let wide: Vec<Vec<u16>> = names.iter().map(|n| n.encode_utf16().collect()).collect();
        let bytes = drop_files(&wide).unwrap();
        assert_eq!(parse_drop_files(&bytes), names);
    }

    #[test]
    fn parse_drop_files_survives_bad_input() {
        assert!(parse_drop_files(&[]).is_empty());
        assert!(parse_drop_files(&[1, 2, 3]).is_empty());
        // pFiles pointing past the end.
        let mut bytes = drop_files(&[[0x41u16]]).unwrap();
        bytes[0..4].copy_from_slice(&999u32.to_le_bytes());
        assert!(parse_drop_files(&bytes).is_empty());
        // A list without its final terminator still yields what is complete.
        let mut bytes = drop_files(&[[0x41u16], [0x42u16]]).unwrap();
        bytes.truncate(bytes.len() - 2);
        assert_eq!(parse_drop_files(&bytes), ["A", "B"]);
        // The narrow (ANSI) form.
        let mut narrow = drop_files(&[[0x41u16]]).unwrap()[..20].to_vec();
        narrow[16..20].copy_from_slice(&0i32.to_le_bytes());
        narrow.extend_from_slice(b"C:\\x.txt\0\0");
        assert_eq!(parse_drop_files(&narrow), ["C:\\x.txt"]);
    }

    #[test]
    fn over_white_endpoints() {
        assert_eq!(over_white(0, 0), 255);
        assert_eq!(over_white(0, 255), 0);
        assert_eq!(over_white(77, 255), 77);
        assert_eq!(over_white(255, 128), 255);
    }

    fn untouched() -> RestoreCheck {
        RestoreCheck {
            same_write: true,
            never_emptied: true,
            still_owner: true,
            sequence: 7,
            own_sequence: 7,
        }
    }

    #[test]
    fn restore_only_over_our_own_unchanged_content() {
        assert!(untouched().unchanged());
        let newer_write = RestoreCheck {
            same_write: false,
            ..untouched()
        };
        let emptied = RestoreCheck {
            never_emptied: false,
            ..untouched()
        };
        let other_owner = RestoreCheck {
            still_owner: false,
            ..untouched()
        };
        let format_added = RestoreCheck {
            sequence: 8,
            ..untouched()
        };
        for changed in [newer_write, emptied, other_owner, format_added] {
            assert!(!changed.unchanged(), "{changed:?}");
        }
    }

    /// CRC-32 (IEEE), for hand-built PNG chunks.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
        out
    }

    /// A PNG whose header claims `width` x `height` RGBA, followed by a tiny, truncated IDAT.
    fn png_claiming(width: u32, height: u32) -> Vec<u8> {
        let mut ihdr = width.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, deflate, no filter, no interlace
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend(chunk(*b"IHDR", &ihdr));
        out.extend(chunk(*b"IDAT", &[0x78, 0x9C, 0x01, 0x00]));
        out.extend(chunk(*b"IEND", &[]));
        out
    }

    #[test]
    fn a_huge_header_is_refused_before_anything_is_allocated() {
        // 60000 x 60000 RGBA would be 14.4 GB; this must return, not abort.
        assert_eq!(png_to_dib(&png_claiming(60_000, 60_000)), None);
        assert_eq!(
            png_to_dib(&png_claiming(u32::MAX >> 1, u32::MAX >> 1)),
            None
        );
    }

    #[test]
    fn the_huge_header_reaches_the_size_check() {
        // Otherwise the test above would pass only because the PNG is malformed.
        let png = png_claiming(60_000, 60_000);
        let reader = png::Decoder::new(std::io::Cursor::new(&png[..]))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().size(), (60_000, 60_000));
        assert!(reader.output_buffer_size().unwrap() > DIB_DECODE_LIMIT);
    }

    #[test]
    fn the_limit_is_inclusive() {
        // 4 x 4 RGBA: 64 decoded bytes (+ filter bytes per the decoder's buffer size); the DIB
        // is 40 + 4 rows * 12 bytes = 88 bytes, the larger of the two.
        let png = encode_png(4, 4, png::ColorType::Rgba, &[9; 64]);
        assert_eq!(dib_size(4, 4), Some(88));
        assert!(png_to_dib_within(&png, 88).is_some());
        assert_eq!(png_to_dib_within(&png, 87), None);
    }

    #[test]
    fn dib_size_rejects_dimensions_the_header_cannot_hold() {
        assert_eq!(dib_size(1, 1), Some(44));
        assert_eq!(dib_size(u32::MAX, 1), None);
        assert_eq!(dib_size(1, 1 << 31), None);
    }

    fn written(ledger: &mut OwnerLedger, sequence: u32) -> u64 {
        ledger.destroyed(); // our own EmptyClipboard notifies us first
        let generation = ledger.begin_write();
        ledger.wrote(sequence);
        generation
    }

    #[test]
    fn a_write_untouched_since_is_restorable() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        assert!(ledger.check(generation, true, 10).unchanged());
    }

    #[test]
    fn a_render_and_a_bump_on_close_are_both_ours() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        ledger.rendered(11);
        assert!(ledger.check(generation, true, 11).unchanged());
        assert!(ledger.wants_update());
        ledger.clipboard_updated(true, 12);
        assert!(ledger.check(generation, true, 12).unchanged());
        // Taken once: a further bump is someone else's.
        ledger.clipboard_updated(true, 13);
        assert!(!ledger.check(generation, true, 13).unchanged());
    }

    #[test]
    fn a_stale_update_after_a_render_keeps_waiting_for_the_close() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        ledger.rendered(11);
        // The update posted for our own write's close arrives late and shows the render's number.
        ledger.clipboard_updated(true, 11);
        assert!(ledger.wants_update());
        ledger.clipboard_updated(true, 12);
        assert!(ledger.check(generation, true, 12).unchanged());
    }

    #[test]
    fn a_jump_after_a_render_is_not_taken_as_ours() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        ledger.rendered(11);
        ledger.clipboard_updated(true, 14);
        assert!(!ledger.wants_update());
        assert!(!ledger.check(generation, true, 14).unchanged());
    }

    #[test]
    fn a_foreign_empty_clipboard_refuses_the_restore() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        ledger.rendered(11);
        ledger.destroyed();
        assert!(!ledger.wants_update());
        ledger.clipboard_updated(false, 12);
        ledger.rendered(13); // cannot happen without ownership, but must not re-arm
        assert!(!ledger.check(generation, false, 12).unchanged());
        assert!(!ledger.check(generation, true, 11).unchanged());
    }

    #[test]
    fn an_update_without_ownership_is_not_ours() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        ledger.rendered(11);
        ledger.clipboard_updated(false, 12);
        assert!(!ledger.check(generation, true, 12).unchanged());
    }

    #[test]
    fn a_refused_write_invalidates_the_previous_one() {
        let mut ledger = OwnerLedger::default();
        let first = written(&mut ledger, 10);
        // The next write emptied the clipboard, then Windows refused every format.
        ledger.destroyed();
        let second = ledger.begin_write();
        assert_ne!(first, second);
        assert!(!ledger.check(first, true, 10).unchanged());
    }

    #[test]
    fn a_restore_invalidates_the_write_it_restored() {
        let mut ledger = OwnerLedger::default();
        let generation = written(&mut ledger, 10);
        assert!(ledger.check(generation, true, 10).unchanged());
        written(&mut ledger, 12); // the restore is itself a write
        assert!(!ledger.check(generation, true, 12).unchanged());
    }
}
