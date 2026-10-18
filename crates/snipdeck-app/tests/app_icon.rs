//! The committed icon assets (`scripts/make_logo.py`): the `.ico` the exe embeds and the tray PNGs.
//! Parses the files as Windows would, so a broken regeneration fails here and not on the owner's PC.

use std::path::PathBuf;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn asset(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/logo")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    usize::try_from(u32::from_le_bytes([
        bytes[at],
        bytes[at + 1],
        bytes[at + 2],
        bytes[at + 3],
    ]))
    .expect("fits")
}

/// Width and height from a PNG's IHDR (big-endian, unlike the .ico).
fn png_size(png: &[u8]) -> (usize, usize) {
    assert_eq!(png[..8], PNG_SIGNATURE, "not a PNG");
    assert_eq!(&png[12..16], b"IHDR");
    let be = |at: usize| {
        usize::try_from(u32::from_be_bytes([
            png[at],
            png[at + 1],
            png[at + 2],
            png[at + 3],
        ]))
        .expect("fits")
    };
    (be(16), be(20))
}

#[test]
fn the_ico_holds_every_size_the_exe_and_the_tray_need_as_png_entries() {
    let ico = asset("snipdeck.ico");
    assert_eq!(u16_at(&ico, 0), 0, "reserved");
    assert_eq!(u16_at(&ico, 2), 1, "type 1 = icon");
    let count = u16_at(&ico, 4);
    let mut sizes = Vec::new();
    for i in 0..count {
        let entry = 6 + 16 * i;
        // A stored 0 means 256.
        let size = |byte: u8| if byte == 0 { 256 } else { usize::from(byte) };
        let (w, h) = (size(ico[entry]), size(ico[entry + 1]));
        assert_eq!(w, h, "entry {i} is square");
        assert_eq!(
            u16_at(&ico, entry + 6),
            32,
            "entry {i} is 32 bits per pixel"
        );
        let (len, offset) = (u32_at(&ico, entry + 8), u32_at(&ico, entry + 12));
        let png = &ico[offset..offset + len];
        assert_eq!(
            png_size(png),
            (w, h),
            "entry {i}: the PNG is as large as its header says"
        );
        sizes.push(w);
    }
    assert_eq!(sizes, [16, 20, 24, 32, 48, 64, 128, 256]);
}

#[test]
fn the_tray_pngs_are_square_at_the_sizes_windows_asks_for() {
    for size in [16, 20, 24, 32] {
        let png = asset(&format!("tray-{size}.png"));
        assert_eq!(png_size(&png), (size, size), "tray-{size}.png");
    }
}
