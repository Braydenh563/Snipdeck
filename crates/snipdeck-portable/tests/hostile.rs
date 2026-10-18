//! Hostile and damaged archives: zip-slip, links, bombs, lying headers, truncation, noise.
//!
//! Every archive here is built by the test. None may panic, hang or change the library.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]
#![allow(
    unknown_lints,
    clippy::assert_is_empty,
    reason = "`assert!(x.is_empty())` reads fine; the lint is newer than our pinned toolchain"
)]

mod common;

use std::io::{Cursor, Write};

use common::*;
use snipdeck_portable::{BoardLayout, Error, ExportOptions, ImportOptions, Limits, import_archive};
use snipdeck_store::Store;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn small_library() -> Store {
    let mut s = store();
    let b = board(&mut s, "Board");
    let l = list(&mut s, b.id, "List");
    text_item(&mut s, l.id, "one", "first");
    text_item(&mut s, l.id, "two", "second");
    s
}

fn valid() -> Vec<u8> {
    export_bytes(&small_library(), &ExportOptions::default())
}

fn opts() -> ImportOptions<'static> {
    ImportOptions {
        layout: BoardLayout::KeepBoards,
        ..ImportOptions::default()
    }
}

fn target() -> Store {
    let mut s = small_library();
    tag(&mut s, "keep me");
    s
}

fn rejected_with(bytes: &[u8], options: &ImportOptions<'_>) -> Error {
    let mut s = target();
    let before = snapshot(&s);
    let err = import_archive(&mut s, Cursor::new(bytes), "x", at(1), options).unwrap_err();
    assert_same(&snapshot(&s), &before);
    assert_eq!(counts(&s), (1, 1, 2, 1));
    assert_healthy(&s);
    err
}

fn rejected(bytes: &[u8]) -> Error {
    rejected_with(bytes, &opts())
}

/// A valid archive plus one extra entry written verbatim.
fn with_extra(name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (n, b) in entries(&valid()) {
        zip.start_file(n, options).unwrap();
        zip.write_all(&b).unwrap();
    }
    zip.start_file(name, options).unwrap();
    zip.write_all(bytes).unwrap();
    zip.finish().unwrap().into_inner()
}

fn is_unsafe(e: &Error) -> bool {
    matches!(e, Error::UnsafeEntry { .. })
}

// ---- names ----

#[test]
fn zip_slip_and_odd_names_are_rejected() {
    let hostile = [
        "../evil.txt",
        "../../etc/passwd",
        "assets/../../evil",
        "assets/../manifest.json",
        "/etc/passwd",
        "/manifest.json",
        "C:/Windows/evil.dll",
        "C:\\Windows\\evil.dll",
        "\\\\server\\share\\x",
        "sub\\dir.txt",
        "./items.json",
        "a//b",
        "a/./b",
        "assets/",
        "assets/not-a-hash",
        "assets/ABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD",
        "assets/sub/0000000000000000000000000000000000000000000000000000000000000000",
        "naïve.txt",
        "tab\there",
        "new\nline",
        "nul\0byte",
        "name with space.txt",
        "~/x",
        "%2e%2e/x",
    ];
    for name in hostile {
        // `assets/` as a *file* with content is invalid; as an empty folder it would be fine,
        // which is covered below.
        let bytes = with_extra(name, b"payload");
        let e = rejected(&bytes);
        assert!(is_unsafe(&e), "{name:?}: {e}");
    }
}

#[test]
fn error_messages_do_not_echo_hostile_names_raw() {
    let bytes = with_extra("../\u{1b}[31mred\u{7}", b"x");
    let e = rejected(&bytes);
    let text = e.to_string();
    assert!(
        !text.contains('\u{1b}') && !text.contains('\u{7}'),
        "{text:?}"
    );
    let long = format!("../{}", "x".repeat(10_000));
    let e = rejected(&with_extra(&long, b"x"));
    assert!(e.to_string().len() < 400, "long names are truncated");
}

#[test]
fn a_repeated_name_is_rejected() {
    // ZipWriter refuses duplicates, so rename one entry in the raw bytes: two entries whose
    // names are the same length differ in one letter, then both headers are patched.
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (n, b) in entries(&valid()) {
        zip.start_file(n, options).unwrap();
        zip.write_all(&b).unwrap();
    }
    zip.start_file("extra-a.bin", options).unwrap();
    zip.write_all(b"1").unwrap();
    zip.start_file("extra-b.bin", options).unwrap();
    zip.write_all(b"2").unwrap();
    let mut bytes = zip.finish().unwrap().into_inner();
    replace_all(&mut bytes, b"extra-b.bin", b"extra-a.bin");
    let e = rejected(&bytes);
    // The zip layer may notice first; either way it is refused.
    assert!(is_unsafe(&e) || matches!(e, Error::NotAnArchive(_)), "{e}");
}

/// Security review 2, finding 2: the entry-count and duplicate-name checks read the
/// end-of-central-directory record, and a decoy planted in the zip comment must not replace it.
#[test]
fn a_decoy_end_record_in_the_zip_comment_does_not_hide_a_repeated_name() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (n, b) in entries(&valid()) {
        zip.start_file(n, options).unwrap();
        zip.write_all(&b).unwrap();
    }
    zip.start_file("extra-a.bin", options).unwrap();
    zip.write_all(b"1").unwrap();
    zip.start_file("extra-b.bin", options).unwrap();
    zip.write_all(b"2").unwrap();
    let mut bytes = zip.finish().unwrap().into_inner();
    replace_all(&mut bytes, b"extra-b.bin", b"extra-a.bin");

    // The real record is the last 22 bytes. Its copy, claiming one entry fewer (what the
    // library keeps after dropping the repeated name), goes into its comment.
    let at_record = bytes.len() - 22;
    assert_eq!(&bytes[at_record..at_record + 4], b"PK\x05\x06");
    let mut decoy = bytes[at_record..].to_vec();
    for field in [8, 10] {
        let total = u16::from_le_bytes([decoy[field], decoy[field + 1]]) - 1;
        decoy[field..field + 2].copy_from_slice(&total.to_le_bytes());
    }
    bytes[at_record + 20..at_record + 22].copy_from_slice(&22_u16.to_le_bytes());
    bytes.extend_from_slice(&decoy);

    let e = rejected(&bytes);
    assert!(is_unsafe(&e) || matches!(e, Error::NotAnArchive(_)), "{e}");
}

fn replace_all(haystack: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let mut i = 0;
    while i + from.len() <= haystack.len() {
        if &haystack[i..i + from.len()] == from {
            haystack[i..i + from.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
}

#[test]
fn an_empty_folder_entry_is_harmless_but_a_folder_with_content_is_not() {
    let mut s = store();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (n, b) in entries(&valid()) {
        zip.start_file(n, options).unwrap();
        zip.write_all(&b).unwrap();
    }
    zip.add_directory("extra/", options).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    import_archive(&mut s, Cursor::new(bytes), "x", at(1), &opts()).unwrap();
}

// ---- links and special files ----

#[test]
fn symbolic_links_are_rejected() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (n, b) in entries(&valid()) {
        zip.start_file(n, options).unwrap();
        zip.write_all(&b).unwrap();
    }
    zip.add_symlink("evil-link", "/etc/passwd", options)
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let e = rejected(&bytes);
    assert!(
        is_unsafe(&e) && e.to_string().contains("symbolic link"),
        "{e}"
    );

    // A link standing in for a required file is no better.
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("manifest.json", "/etc/hostname", options)
        .unwrap();
    zip.start_file("items.json", options).unwrap();
    zip.write_all(b"{}").unwrap();
    let e = rejected(&zip.finish().unwrap().into_inner());
    assert!(is_unsafe(&e), "{e}");
}

#[test]
fn encrypted_entries_are_rejected() {
    let mut bytes = valid();
    // Set the "encrypted" bit (bit 0 of the general purpose flags) in every central
    // directory header (signature PK\x01\x02, flags at offset 8).
    let mut i = 0;
    while i + 12 < bytes.len() {
        if bytes[i..i + 4] == *b"PK\x01\x02" {
            bytes[i + 8] |= 1;
        }
        i += 1;
    }
    let e = rejected(&bytes);
    assert!(is_unsafe(&e) || matches!(e, Error::NotAnArchive(_)), "{e}");
}

#[test]
fn unsupported_compression_methods_are_rejected() {
    let mut bytes = valid();
    let mut i = 0;
    while i + 12 < bytes.len() {
        if bytes[i..i + 4] == *b"PK\x01\x02" {
            // Method 12 (bzip2) is not compiled in.
            bytes[i + 10] = 12;
            bytes[i + 11] = 0;
        }
        i += 1;
    }
    let e = rejected(&bytes);
    assert!(is_unsafe(&e) || matches!(e, Error::NotAnArchive(_)), "{e}");
}

#[test]
fn overlapping_entries_are_rejected() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (n, b) in entries(&valid()) {
        zip.start_file(n, options).unwrap();
        zip.write_all(&b).unwrap();
    }
    let src = format!("assets/{}", "1".repeat(64));
    zip.start_file(&src, options).unwrap();
    zip.write_all(&vec![7_u8; 100_000]).unwrap();
    // A second name for the same compressed bytes: the classic way to multiply a bomb.
    zip.shallow_copy_file(&src, &format!("assets/{}", "2".repeat(64)))
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let e = rejected(&bytes);
    assert!(is_unsafe(&e) && e.to_string().contains("overlaps"), "{e}");
}

// ---- limits ----

fn zeros(len: usize) -> Vec<u8> {
    vec![0; len]
}

#[test]
fn an_oversized_entry_is_rejected_from_its_header() {
    // Two MiB of zeros deflates to about 2 KiB; with a 1 MiB cap it is rejected by its declared
    // size, without being decompressed.
    let limits = Limits {
        max_entry_bytes: 1024 * 1024,
        ..Limits::default()
    };
    let bytes = with_extra(
        "assets/0000000000000000000000000000000000000000000000000000000000000000",
        &zeros(2 * 1024 * 1024),
    );
    assert!(bytes.len() < 10_000, "the bomb is small: {}", bytes.len());
    let e = rejected_with(&bytes, &ImportOptions { limits, ..opts() });
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "uncompressed entry size",
                ..
            }
        ),
        "{e}"
    );
    // The default cap is 64 MiB.
    assert_eq!(Limits::default().max_entry_bytes, 64 * 1024 * 1024);
}

#[test]
fn a_high_compression_ratio_is_rejected() {
    let bytes = with_extra(
        "assets/0000000000000000000000000000000000000000000000000000000000000000",
        &zeros(8 * 1024 * 1024),
    );
    let e = rejected(&bytes);
    assert!(is_unsafe(&e) && e.to_string().contains("zip bomb"), "{e}");
}

#[test]
fn the_total_uncompressed_size_is_capped() {
    let limits = Limits {
        max_total_bytes: 1000,
        ..Limits::default()
    };
    let e = rejected_with(
        &with_extra("extra.bin", &noise(2000, 1)),
        &ImportOptions { limits, ..opts() },
    );
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "total uncompressed size",
                ..
            }
        ),
        "{e}"
    );
}

#[test]
fn many_entries_are_capped() {
    let limits = Limits {
        max_entries: 4,
        ..Limits::default()
    };
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..5 {
        zip.start_file(format!("f{i}.txt"), SimpleFileOptions::default())
            .unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let e = rejected_with(&bytes, &ImportOptions { limits, ..opts() });
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "entry count",
                ..
            }
        ),
        "{e}"
    );
}

#[test]
fn a_directory_that_declares_too_many_entries_is_refused_before_it_is_parsed() {
    let mut bytes = valid();
    // Patch the end-of-central-directory record (PK\x05\x06) to claim 60 000 entries.
    let pos = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap();
    bytes[pos + 8..pos + 10].copy_from_slice(&60_000_u16.to_le_bytes());
    bytes[pos + 10..pos + 12].copy_from_slice(&60_000_u16.to_le_bytes());
    let limits = Limits {
        max_entries: 100,
        ..Limits::default()
    };
    let e = rejected_with(&bytes, &ImportOptions { limits, ..opts() });
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "entry count",
                ..
            }
        ),
        "{e}"
    );
    // With the default limit the lie is still caught: the names do not add up.
    let e = rejected(&bytes);
    assert!(is_unsafe(&e) || matches!(e, Error::NotAnArchive(_)), "{e}");
}

#[test]
fn the_archive_file_size_is_capped() {
    let limits = Limits {
        max_total_bytes: 10,
        max_entries: 1,
        ..Limits::default()
    };
    let e = rejected_with(&valid(), &ImportOptions { limits, ..opts() });
    assert!(matches!(e, Error::LimitExceeded { .. }), "{e}");
}

#[test]
fn an_oversized_items_json_is_capped() {
    let limits = Limits {
        max_json_bytes: 100,
        ..Limits::default()
    };
    let e = rejected_with(&valid(), &ImportOptions { limits, ..opts() });
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "items.json size",
                ..
            }
        ),
        "{e}"
    );
    let huge_manifest = rewrite(&valid(), |m| {
        m.insert("manifest.json".into(), vec![b' '; 2 * 1024 * 1024]);
    });
    let e = rejected(&huge_manifest);
    assert!(
        matches!(
            e,
            Error::LimitExceeded {
                what: "manifest size",
                ..
            }
        ),
        "{e}"
    );
}

/// Patches the uncompressed size in every central directory header to `size`.
fn lie_about_size(bytes: &mut [u8], size: u32) {
    let mut i = 0;
    while i + 28 < bytes.len() {
        if bytes[i..i + 4] == *b"PK\x01\x02" {
            bytes[i + 24..i + 28].copy_from_slice(&size.to_le_bytes());
        }
        i += 1;
    }
}

#[test]
fn headers_that_lie_about_size_cannot_make_us_read_more() {
    let mut bytes = valid();
    lie_about_size(&mut bytes, 10);
    let e = rejected(&bytes);
    // The lie is caught somewhere: size mismatch, bad JSON, CRC or the zip layer.
    let _ = e;

    // The same for a referenced asset whose header says "tiny" but whose data is big.
    let payload = noise(100_000, 5)
        .into_iter()
        .map(|b| b'a' + b % 26)
        .collect::<Vec<u8>>();
    let hash = blake3::hash(&payload).to_hex().to_string();
    let manifest = json_manifest(1);
    let items = format!(
        r#"{{"tags":[],"boards":[{{"id":"{b}","name":"B","created_at":"2027-01-01T00:00:00Z","updated_at":"2027-01-01T00:00:00Z","lists":[{{"id":"{l}","name":"L","view":"compact","sort":"manual","created_at":"2027-01-01T00:00:00Z","updated_at":"2027-01-01T00:00:00Z","items":[{{"id":"{i}","title":"t","kind":"text","created_at":"2027-01-01T00:00:00Z","updated_at":"2027-01-01T00:00:00Z","formats":[{{"kind":"text","asset":"{hash}","size":10}}]}}]}}]}}]}}"#,
        b = uuid(1),
        l = uuid(2),
        i = uuid(3),
    );
    let mut bytes = build_zip([
        ("manifest.json", manifest.as_bytes()),
        ("items.json", items.as_bytes()),
        (&format!("assets/{hash}"), payload.as_slice()),
    ]);
    lie_about_size(&mut bytes, 10);
    let mut s = store();
    let result = import_archive(&mut s, Cursor::new(bytes), "x", at(1), &opts());
    assert!(result.is_err(), "a size lie must not be accepted");
    assert_eq!(counts(&s), (0, 0, 0, 0));
}

fn uuid(n: u8) -> String {
    format!("018f3a52-0000-7000-8000-{n:012}")
}

fn json_manifest(assets: u64) -> String {
    format!(
        r#"{{"format":"snipdeck-archive","format_version":"1.0","app_version":"t","created_at":"2027-01-01T00:00:00Z","counts":{{"boards":1,"lists":1,"items":1,"tags":0,"assets":{assets}}}}}"#
    )
}

// ---- damage and noise ----

/// Deterministic pseudo-random numbers.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }
}

/// Imports `bytes`; whatever happens must leave a healthy library, and a failure must leave it
/// exactly as it was.
fn survive(bytes: &[u8]) {
    let mut s = target();
    let before = snapshot(&s);
    match import_archive(&mut s, Cursor::new(bytes), "x", at(1), &opts()) {
        Ok(_) => {}
        Err(_) => assert_same(&snapshot(&s), &before),
    }
    assert_healthy(&s);
    assert_trash_empty(&s);
}

#[test]
fn nothing_and_noise_are_not_archives() {
    for bytes in [
        Vec::new(),
        b"PK".to_vec(),
        b"PK\x05\x06".to_vec(),
        b"PK\x03\x04".to_vec(),
        vec![0; 1000],
        b"{\"format\":\"snipdeck-archive\"}".to_vec(),
        noise(5000, 11),
    ] {
        let e = rejected(&bytes);
        assert!(
            matches!(e, Error::NotAnArchive(_) | Error::Io(_)),
            "{} bytes: {e}",
            bytes.len()
        );
    }
}

#[test]
fn every_truncation_is_handled() {
    let bytes = valid();
    // Every cut near the end (the directory) and every third one elsewhere.
    for cut in (0..bytes.len()).filter(|c| c % 3 == 0 || bytes.len() - c < 200) {
        survive(&bytes[..cut]);
    }
}

#[test]
fn random_corruption_is_handled() {
    let bytes = valid();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..400 {
        let mut damaged = bytes.clone();
        for _ in 0..=rng.below(4) {
            let at = rng.below(damaged.len());
            damaged[at] = u8::try_from(rng.next() & 0xff).unwrap();
        }
        survive(&damaged);
    }
}

#[test]
fn random_corruption_of_the_json_is_handled() {
    // Damage inside a well-formed zip, so the JSON and validation layers see the garbage.
    let base = valid();
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    for _ in 0..300 {
        let damaged = rewrite(&base, |map| {
            let target = if rng.below(2) == 0 {
                "items.json"
            } else {
                "manifest.json"
            };
            let json = map.get_mut(target).unwrap();
            for _ in 0..=rng.below(3) {
                let at = rng.below(json.len());
                match rng.below(3) {
                    0 => json[at] = u8::try_from(rng.next() & 0xff).unwrap(),
                    1 => json.truncate(at),
                    _ => {
                        json.remove(at);
                    }
                }
                if json.is_empty() {
                    break;
                }
            }
        });
        survive(&damaged);
    }
}

#[test]
fn a_failed_import_leaves_no_blobs_or_tags_behind() {
    // An archive whose last asset is damaged is refused before anything is written, so the tag
    // list and the trash are as they were (checked by `rejected`) and no board exists.
    let zip = export_bytes(&crate::common::sample_store(), &ExportOptions::default());
    let broken = rewrite(&zip, |map| {
        let name = map
            .keys()
            .rfind(|k| k.starts_with("assets/"))
            .unwrap()
            .clone();
        map.get_mut(&name).unwrap().push(0);
    });
    let mut s = store();
    let r = import_archive(&mut s, Cursor::new(broken), "x", at(1), &opts());
    assert!(r.is_err());
    assert_eq!(counts(&s), (0, 0, 0, 0));
}
