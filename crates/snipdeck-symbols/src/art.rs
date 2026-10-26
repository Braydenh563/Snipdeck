//! Emoji art: the embedded pack of small palette PNGs, decoded and resampled to device pixels.
//!
//! Layout of `emoji48.bin` (little-endian): `b"SDE1"`, `u16` px, `u32` count, `count` x (`u32`
//! offset, `u32` length), then the PNGs. Entry `i` is the art of the catalog's emoji `i`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::{Catalog, SymbolId};
use crate::data::DataError;

/// The embedded art pack, or any pack with the same layout (used by tests).
#[derive(Clone, Debug)]
pub struct ArtPack {
    blob: &'static [u8],
    px: u32,
    count: usize,
}

fn le32(b: &[u8], at: usize) -> Option<usize> {
    let a: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
    usize::try_from(u32::from_le_bytes(a)).ok()
}

impl ArtPack {
    /// A pack with no art: every lookup is `None` and the UI draws the character as text.
    #[must_use]
    pub fn none() -> Self {
        Self {
            blob: &[],
            px: 0,
            count: 0,
        }
    }

    /// Reads the header and checks that the index fits inside `blob`.
    ///
    /// # Errors
    /// When the magic, header or index is wrong.
    pub fn parse(blob: &'static [u8]) -> Result<Self, DataError> {
        let err = |what| DataError {
            file: "emoji48.bin",
            what,
        };
        if blob.get(..4) != Some(b"SDE1") {
            return Err(err("bad magic"));
        }
        let px = blob
            .get(4..6)
            .map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])));
        let count = le32(blob, 6);
        let (Some(px), Some(count)) = (px, count) else {
            return Err(err("truncated header"));
        };
        if blob.len() < 10 + 8 * count {
            return Err(err("truncated index"));
        }
        Ok(Self { blob, px, count })
    }

    /// Side of the stored art in pixels (48).
    #[must_use]
    pub fn px(&self) -> u32 {
        self.px
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when there is no art at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The PNG bytes of entry `i`, `None` when `i` or its extent is outside the pack.
    #[must_use]
    pub fn png(&self, i: usize) -> Option<&'static [u8]> {
        if i >= self.count {
            return None;
        }
        let off = le32(self.blob, 10 + 8 * i)?;
        let len = le32(self.blob, 14 + 8 * i)?;
        self.blob.get(off..off.checked_add(len)?)
    }

    /// Entry `i` as straight (non-premultiplied) RGBA at `px` x `px`; `None` when missing or broken.
    #[must_use]
    pub fn rgba(&self, i: usize, px: u32) -> Option<Vec<u8>> {
        if px == 0 || px > 1024 {
            return None;
        }
        let (w, h, rgba) = decode_png(self.png(i)?)?;
        Some(resample(&rgba, w, h, px))
    }
}

/// Decodes a PNG to (width, height, straight RGBA8). Palette and RGB inputs are expanded.
fn decode_png(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let data = buf.get(..info.buffer_size())?;
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

/// Area-average resample of straight RGBA to `out` x `out` (a box filter when shrinking), with the
/// colour averaged premultiplied so transparent edges do not bleed dark fringes.
#[allow(
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel maths on image sizes of at most 1024"
)]
fn resample(src: &[u8], w: u32, h: u32, out: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    if w == out as usize && h == out as usize {
        return src.to_vec();
    }
    let out_n = out as usize;
    let sx = w as f32 / out as f32;
    let sy = h as f32 / out as f32;
    let mut dst = vec![0u8; out_n * out_n * 4];
    for oy in 0..out_n {
        let (y0, y1) = (oy as f32 * sy, (oy + 1) as f32 * sy);
        for ox in 0..out_n {
            let (x0, x1) = (ox as f32 * sx, (ox + 1) as f32 * sx);
            let (mut r, mut g, mut b, mut a, mut wsum) = (0.0f32, 0.0, 0.0, 0.0, 0.0);
            let (iy0, iy1) = (y0.floor() as usize, (y1.ceil() as usize).min(h));
            let (ix0, ix1) = (x0.floor() as usize, (x1.ceil() as usize).min(w));
            for iy in iy0..iy1 {
                let wy = (y1.min((iy + 1) as f32) - y0.max(iy as f32)).max(0.0);
                for ix in ix0..ix1 {
                    let wx = (x1.min((ix + 1) as f32) - x0.max(ix as f32)).max(0.0);
                    let wgt = wx * wy;
                    let p = &src[(iy * w + ix) * 4..(iy * w + ix) * 4 + 4];
                    let pa = f32::from(p[3]) / 255.0;
                    r += f32::from(p[0]) * pa * wgt;
                    g += f32::from(p[1]) * pa * wgt;
                    b += f32::from(p[2]) * pa * wgt;
                    a += pa * wgt;
                    wsum += wgt;
                }
            }
            let o = (oy * out_n + ox) * 4;
            if wsum > 0.0 && a > 0.0 {
                let to_u8 = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                dst[o] = to_u8(r / a);
                dst[o + 1] = to_u8(g / a);
                dst[o + 2] = to_u8(b / a);
                dst[o + 3] = to_u8(a / wsum * 255.0);
            }
        }
    }
    dst
}

impl Catalog {
    /// The art of an emoji as straight RGBA at `px` x `px`. `None` for kaomoji and icons, unknown
    /// ids, and art that is missing or fails to decode: the caller then draws the character.
    #[must_use]
    pub fn art(&self, id: &SymbolId, px: u32) -> Option<Vec<u8>> {
        if !matches!(id, SymbolId::Emoji(_)) {
            return None;
        }
        // Emoji are the first catalog entries, in the same order as the art entries.
        self.art_pack().rgba(self.index_of(id)?, px)
    }
}

type Cell = (u64, Arc<[u8]>);

/// A small least-recently-used cache of decoded cells (the spec: 256 cells, at most 2.4 MB).
pub struct ArtCache {
    capacity: usize,
    tick: u64,
    cells: HashMap<(SymbolId, u32), Cell>,
}

impl ArtCache {
    /// A cache holding at most `capacity` cells.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            tick: 0,
            cells: HashMap::new(),
        }
    }

    /// Cells currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True when nothing is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// The RGBA of `id` at `px`, decoding it on a miss and evicting the least recently used cell.
    pub fn get(&mut self, catalog: &Catalog, id: &SymbolId, px: u32) -> Option<Arc<[u8]>> {
        self.tick += 1;
        let key = (id.clone(), px);
        if let Some((stamp, rgba)) = self.cells.get_mut(&key) {
            *stamp = self.tick;
            return Some(Arc::clone(rgba));
        }
        let rgba: Arc<[u8]> = catalog.art(id, px)?.into();
        let oldest = self
            .cells
            .iter()
            .min_by_key(|(_, (t, _))| *t)
            .map(|(k, _)| k.clone());
        if self.cells.len() >= self.capacity
            && let Some(oldest) = oldest
        {
            self.cells.remove(&oldest);
        }
        self.cells.insert(key, (self.tick, Arc::clone(&rgba)));
        Some(rgba)
    }
}
