//! The Recent inbox: dedupe, order, retention boundaries, clear, and its separation from
//! the library. Every timestamp is fixed.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::time::Duration;

use common::*;
use snipdeck_store::{
    ItemRepo, RecentContent, RecentImage, RecentKind, RecentPayload, RecentRepo, RecentThumb,
};

const WEEK: Duration = Duration::from_hours(24 * 7);

fn texts(s: &snipdeck_store::Store) -> Vec<String> {
    s.recent().unwrap().into_iter().map(|e| e.text).collect()
}

#[test]
fn entries_come_back_newest_first_with_their_source() {
    let mut s = store();
    s.add_recent(RecentContent::text("a"), Some("notepad.exe"), at(1))
        .unwrap();
    s.add_recent(RecentContent::text("b"), None, at(2)).unwrap();
    let all = s.recent().unwrap();
    assert_eq!(texts(&s), ["b", "a"]);
    assert_eq!(all[1].source_app.as_deref(), Some("notepad.exe"));
    assert_eq!(all[0].source_app, None);
    assert_eq!(all[0].copied_at, at(2));
}

#[test]
fn recopying_bumps_the_entry_to_the_top_without_a_duplicate() {
    let mut s = store();
    s.add_recent(RecentContent::text("a"), None, at(1)).unwrap();
    s.add_recent(RecentContent::text("b"), None, at(2)).unwrap();
    s.add_recent(RecentContent::text("a"), Some("code.exe"), at(3))
        .unwrap();
    assert_eq!(texts(&s), ["a", "b"]);
    let top = &s.recent().unwrap()[0];
    assert_eq!(top.copied_at, at(3));
    assert_eq!(top.source_app.as_deref(), Some("code.exe"));
}

#[test]
fn a_bump_at_the_same_instant_still_lands_on_top() {
    let mut s = store();
    s.add_recent(RecentContent::text("a"), None, at(1)).unwrap();
    s.add_recent(RecentContent::text("b"), None, at(1)).unwrap();
    s.add_recent(RecentContent::text("a"), None, at(1)).unwrap();
    assert_eq!(texts(&s), ["a", "b"]);
}

#[test]
fn the_entry_cap_keeps_exactly_the_newest_n() {
    let mut s = store();
    for i in 0..201 {
        s.add_recent(RecentContent::text(&format!("t{i}")), None, at(i))
            .unwrap();
    }
    // 201 entries, cap 200: the oldest goes.
    assert_eq!(s.prune_recent(at(300), 200, WEEK).unwrap(), 1);
    let all = texts(&s);
    assert_eq!(all.len(), 200);
    assert_eq!(all[0], "t200");
    assert_eq!(all[199], "t1");
    // Exactly at the cap: nothing more goes.
    assert_eq!(s.prune_recent(at(300), 200, WEEK).unwrap(), 0);
}

#[test]
fn the_age_limit_keeps_an_entry_exactly_seven_days_old() {
    let mut s = store();
    let now = at(10 * 86_400);
    s.add_recent(RecentContent::text("exactly"), None, at(3 * 86_400))
        .unwrap();
    s.add_recent(RecentContent::text("just older"), None, at(3 * 86_400 - 1))
        .unwrap();
    s.add_recent(RecentContent::text("fresh"), None, at(9 * 86_400))
        .unwrap();
    assert_eq!(s.prune_recent(now, 200, WEEK).unwrap(), 1);
    assert_eq!(texts(&s), ["fresh", "exactly"]);
}

#[test]
fn delete_and_clear_remove_entries() {
    let mut s = store();
    let a = s.add_recent(RecentContent::text("a"), None, at(1)).unwrap();
    s.add_recent(RecentContent::text("b"), None, at(2)).unwrap();
    s.add_recent(RecentContent::text("c"), None, at(3)).unwrap();
    s.delete_recent(a).unwrap();
    s.delete_recent(a).unwrap(); // already gone: fine
    assert_eq!(texts(&s), ["c", "b"]);
    assert_eq!(s.clear_recent().unwrap(), 2);
    assert_eq!(s.recent().unwrap().len(), 0);
    assert_eq!(s.clear_recent().unwrap(), 0);
}

#[test]
fn entries_are_not_library_items_and_are_not_searchable() {
    let mut s = store();
    s.add_recent(RecentContent::text("needle haystack"), None, at(1))
        .unwrap();
    assert_eq!(s.all_items().unwrap().len(), 0);
}

#[test]
fn reading_leaves_the_revisions_alone_and_writing_moves_the_recent_one() {
    let mut s = store();
    s.add_recent(RecentContent::text("a"), None, at(1)).unwrap();
    let before = s.recent_revision();
    s.recent().unwrap();
    assert_eq!(s.recent_revision(), before);
    s.add_recent(RecentContent::text("b"), None, at(2)).unwrap();
    assert!(s.recent_revision() > before);
}

#[test]
fn entries_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = snipdeck_store::Store::open(dir.path()).unwrap();
        s.add_recent(RecentContent::text("kept"), Some("x.exe"), at(5))
            .unwrap();
    }
    let s = snipdeck_store::Store::open(dir.path()).unwrap();
    assert_eq!(texts(&s), ["kept"]);
}

// ---- Encryption at rest ---------------------------------------------------------------------

mod at_rest {
    use super::*;
    use rusqlite::Connection;
    use snipdeck_store::crypto::{KdfParams, KeyRing, MemoryKeyProvider};
    use snipdeck_store::{Error, Store};

    fn provider() -> MemoryKeyProvider {
        MemoryKeyProvider::new().unwrap()
    }

    fn ring(p: &MemoryKeyProvider) -> KeyRing<'_> {
        KeyRing::new(p).with_kdf_params(KdfParams::insecure_for_tests(64, 1, 1).unwrap())
    }

    /// A password computed at run time, so none is written in the source.
    fn pw(label: &str) -> String {
        let key = blake3::derive_key("snipdeck recent test password", label.as_bytes());
        blake3::Hash::from_bytes(key).to_hex().as_str()[..24].to_owned()
    }

    /// A marker no other test, table or schema text contains, built at run time.
    fn marker(label: &str) -> String {
        let key = blake3::derive_key("snipdeck recent test marker", label.as_bytes());
        format!(
            "MK-{}",
            &blake3::Hash::from_bytes(key).to_hex().as_str()[..20]
        )
    }

    /// Whether any file of the library holds `needle`. The `-shm` file (an index of the WAL) and
    /// `snipdeck.lock` (the single-instance lock) never hold row text, and Windows keeps a lock on
    /// both while the store is open; the other files are opened with every share mode so the open
    /// store does not block the read.
    fn holds(dir: &std::path::Path, needle: &str) -> bool {
        use std::io::Read;
        std::fs::read_dir(dir).unwrap().any(|e| {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if !path.is_file() || name.ends_with("-shm") || name == "snipdeck.lock" {
                return false;
            }
            let mut open = std::fs::OpenOptions::new();
            open.read(true);
            #[cfg(windows)]
            std::os::windows::fs::OpenOptionsExt::share_mode(&mut open, 0x7);
            let mut bytes = Vec::new();
            open.open(&path).unwrap().read_to_end(&mut bytes).unwrap();
            bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
        })
    }

    fn raw(dir: &std::path::Path) -> Connection {
        Connection::open(dir.join("snipdeck.db")).unwrap()
    }

    #[test]
    fn an_unlocked_entry_round_trips() {
        let p = provider();
        let mut s = store();
        s.create_key_ring(&ring(&p), None).unwrap();
        s.add_recent(RecentContent::text("hello"), Some("code.exe"), at(1))
            .unwrap();
        s.add_recent(RecentContent::text("hello"), Some("notepad.exe"), at(2))
            .unwrap();
        s.add_recent(RecentContent::text("world"), None, at(3))
            .unwrap();
        let all = s.recent().unwrap();
        assert_eq!(
            all.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            ["world", "hello"]
        );
        assert_eq!(all[1].source_app.as_deref(), Some("notepad.exe"));
        assert_eq!(all[1].copied_at, at(2));
    }

    #[test]
    fn the_text_is_not_in_the_database_files_but_is_without_a_key_ring() {
        let p = provider();
        let secret = marker("sealed");
        let plain = marker("plain");
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = Store::open(dir.path()).unwrap();
            s.create_key_ring(&ring(&p), None).unwrap();
            s.add_recent(RecentContent::text(&secret), Some(&marker("app")), at(1))
                .unwrap();
            // Still open: the write-ahead log is part of what is searched.
            assert!(!holds(dir.path(), &secret), "before close");
            assert!(!holds(dir.path(), &marker("app")), "source app");
        }
        assert!(!holds(dir.path(), &secret), "after close");
        let c = raw(dir.path());
        let (encrypted, hash_is_plain): (i64, bool) = c
            .query_row(
                "SELECT encrypted, hash = ?1 FROM recent_entries",
                [blake3::hash(secret.as_bytes()).as_bytes().as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (encrypted, hash_is_plain),
            (1, false),
            "keyed hash, not BLAKE3 of the text"
        );
        drop(c);

        // The probe can see plaintext when nothing encrypts it.
        let other = tempfile::tempdir().unwrap();
        {
            let mut s = Store::open(other.path()).unwrap();
            s.add_recent(RecentContent::text(&plain), None, at(1))
                .unwrap();
        }
        assert!(holds(other.path(), &plain), "the probe must see plaintext");
    }

    #[test]
    fn a_locked_library_refuses_reads_and_writes_but_still_clears() {
        let p = provider();
        let mut s = store();
        s.create_key_ring(&ring(&p), None).unwrap();
        s.add_recent(RecentContent::text("a"), None, at(1)).unwrap();
        s.add_recent(RecentContent::text("b"), None, at(2)).unwrap();
        s.lock_key_ring();
        assert!(matches!(s.recent(), Err(Error::EncryptionLocked)));
        assert!(matches!(
            s.add_recent(RecentContent::text("c"), None, at(3)),
            Err(Error::EncryptionLocked)
        ));
        // Retention and clearing need no key.
        assert_eq!(
            s.prune_recent(at(10), 1, Duration::from_hours(24)).unwrap(),
            1
        );
        s.unlock_key_ring(&ring(&p), None).unwrap();
        assert_eq!(texts(&s), ["b"]);
        s.lock_key_ring();
        assert_eq!(s.clear_recent().unwrap(), 1);
        s.unlock_key_ring(&ring(&p), None).unwrap();
        assert_eq!(texts(&s), Vec::<String>::new());
    }

    #[test]
    fn with_a_master_password_a_restart_stays_locked_until_the_password_is_given() {
        let p = provider();
        let word = pw("master");
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = Store::open(dir.path()).unwrap();
            s.create_key_ring(&ring(&p), Some(&word)).unwrap();
            s.add_recent(RecentContent::text("kept"), None, at(1))
                .unwrap();
        }
        let mut s = Store::open(dir.path()).unwrap();
        assert!(matches!(s.recent(), Err(Error::EncryptionLocked)));
        assert!(s.unlock_key_ring(&ring(&p), None).is_err());
        assert!(matches!(s.recent(), Err(Error::EncryptionLocked)));
        s.unlock_key_ring(&ring(&p), Some(&word)).unwrap();
        assert_eq!(texts(&s), ["kept"]);
    }

    #[test]
    fn a_ciphertext_moved_to_another_row_does_not_read() {
        let p = provider();
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.create_key_ring(&ring(&p), None).unwrap();
        let first = s
            .add_recent(RecentContent::text("aaa"), None, at(1))
            .unwrap();
        let second = s
            .add_recent(RecentContent::text("bbb"), None, at(2))
            .unwrap();
        let conn = raw(dir.path());
        let text_of = |id: i64| -> Vec<u8> {
            conn.query_row("SELECT text FROM recent_entries WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
        };
        let (ta, tb) = (text_of(first), text_of(second));
        conn.execute(
            "UPDATE recent_entries SET text = ?1 WHERE id = ?2",
            rusqlite::params![tb, first],
        )
        .unwrap();
        conn.execute(
            "UPDATE recent_entries SET text = ?1 WHERE id = ?2",
            rusqlite::params![ta, second],
        )
        .unwrap();
        // Neither swapped row opens (each is skipped, so no text from the wrong row appears).
        assert_eq!(s.recent().unwrap().len(), 0);
    }

    fn insert_damaged_plain_row(conn: &Connection, tag: u8, copied_at: i64) {
        conn.execute(
            "INSERT INTO recent_entries (hash, text, source_app, copied_at, encrypted)
             VALUES (?1, ?2, NULL, ?3, 0)",
            rusqlite::params![vec![tag; 32], vec![0xFF_u8, 0xFE, 0xFD], copied_at],
        )
        .unwrap();
    }

    #[test]
    fn a_row_that_cannot_be_sealed_is_deleted_and_does_not_block_the_unlock() {
        let p = provider();
        let kept = marker("kept");
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = Store::open(dir.path()).unwrap();
            s.add_recent(RecentContent::text(&kept), None, at(1))
                .unwrap();
            insert_damaged_plain_row(&raw(dir.path()), 7, 5);
            // Unreadable while plain, but the readable rows still list.
            assert_eq!(texts(&s), std::slice::from_ref(&kept));
        }
        let mut s = Store::open(dir.path()).unwrap();
        s.create_key_ring(&ring(&p), None).unwrap();
        assert!(!s.has_plain_recent().unwrap());
        assert_eq!(texts(&s), std::slice::from_ref(&kept));
        let rows: i64 = raw(dir.path())
            .query_row("SELECT count(*) FROM recent_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "the damaged row was deleted");
        drop(s);
        let mut s = Store::open(dir.path()).unwrap();
        s.unlock_key_ring(&ring(&p), None).unwrap();
        assert_eq!(texts(&s), [kept]);
    }

    #[test]
    fn a_sealed_row_that_will_not_open_is_skipped_not_fatal() {
        let p = provider();
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(dir.path()).unwrap();
        s.create_key_ring(&ring(&p), None).unwrap();
        s.add_recent(RecentContent::text("first"), None, at(1))
            .unwrap();
        let bad = s
            .add_recent(RecentContent::text("second"), None, at(2))
            .unwrap();
        raw(dir.path())
            .execute(
                "UPDATE recent_entries SET text = ?1 WHERE id = ?2",
                rusqlite::params![vec![1_u8; 40], bad],
            )
            .unwrap();
        assert_eq!(texts(&s), ["first"]);
    }

    /// An old database: the version-4 schema with the plaintext Recent table.
    fn old_db(dir: &std::path::Path, rows: &[(&str, Option<&str>, i64)]) {
        drop(Store::open(dir).unwrap());
        let c = raw(dir);
        c.execute_batch(
            "DROP TABLE recent_entries;
             CREATE TABLE recent_entries (
                id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
                hash BLOB NOT NULL UNIQUE CHECK (length(hash) = 32),
                text TEXT NOT NULL CHECK (length(text) > 0),
                source_app TEXT,
                copied_at INTEGER NOT NULL
             ) STRICT;
             CREATE INDEX recent_entries_newest ON recent_entries (copied_at DESC, id DESC);
             DROP INDEX items_group;
             ALTER TABLE items DROP COLUMN group_id;
             DROP TABLE item_groups;
             ALTER TABLE key_ring DROP COLUMN previous;
             PRAGMA user_version = 4;",
        )
        .unwrap();
        for (text, app, secs) in rows {
            c.execute(
                "INSERT INTO recent_entries (hash, text, source_app, copied_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    blake3::hash(text.as_bytes()).as_bytes().as_slice(),
                    text,
                    app,
                    1_800_000_000_i64 * 1_000_000_000 + secs * 1_000_000_000
                ],
            )
            .unwrap();
        }
    }

    #[test]
    fn old_plaintext_rows_survive_the_migration_and_are_encrypted_once_a_key_is_there() {
        let p = provider();
        let (m1, m2) = (marker("old1"), marker("old2"));
        let dir = tempfile::tempdir().unwrap();
        old_db(dir.path(), &[(&m1, Some("a.exe"), 1), (&m2, None, 2)]);
        assert!(holds(dir.path(), &m1), "the old file holds plaintext");

        // Opened with no key: the rows are kept and readable, flagged plain.
        {
            let s = Store::open(dir.path()).unwrap();
            assert_eq!(s.schema_version().unwrap(), 8);
            assert_eq!(texts(&s), [m2.clone(), m1.clone()]);
            assert!(s.has_plain_recent().unwrap());
        }
        // The key arrives (first unlocked open): everything is encrypted, nothing is lost.
        {
            let mut s = Store::open(dir.path()).unwrap();
            s.create_key_ring(&ring(&p), None).unwrap();
            assert!(!s.has_plain_recent().unwrap());
            let all = s.recent().unwrap();
            assert_eq!(all.len(), 2);
            assert_eq!(all[1].source_app.as_deref(), Some("a.exe"));
            assert_eq!(all[1].copied_at, at(1));
        }
        assert!(!holds(dir.path(), &m1), "plaintext gone from every file");
        assert!(!holds(dir.path(), &m2));
        // Idempotent: another open and unlock changes nothing.
        let mut s = Store::open(dir.path()).unwrap();
        s.unlock_key_ring(&ring(&p), None).unwrap();
        assert_eq!(texts(&s), [m2, m1]);
    }

    #[test]
    fn a_migration_backup_does_not_keep_the_old_plaintext() {
        let m = marker("backup");
        let dir = tempfile::tempdir().unwrap();
        old_db(dir.path(), &[(&m, None, 1)]);
        drop(Store::open(dir.path()).unwrap());
        assert!(dir.path().join("snipdeck.db.pre-v4.bak").exists());
        let bak = std::fs::read(dir.path().join("snipdeck.db.pre-v4.bak")).unwrap();
        assert!(!bak.windows(m.len()).any(|w| w == m.as_bytes()));
    }

    #[test]
    fn without_a_key_ring_entries_are_kept_plain_and_flagged() {
        // No key ring yet (no key protection on this machine, or not set up): still works.
        let mut s = store();
        s.add_recent(RecentContent::text("x"), None, at(1)).unwrap();
        assert!(s.has_plain_recent().unwrap());
        assert_eq!(texts(&s), ["x"]);
    }

    fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(files_under(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    /// A stand-in for PNG bytes (the store never decodes them), over the inline limit.
    fn picture(marker: &str) -> Vec<u8> {
        let mut bytes = marker.as_bytes().to_vec();
        bytes.resize(100 * 1024, 7);
        bytes
    }

    fn image<'a>(png: &'a [u8], thumb: &'a RecentThumb, digest: u8) -> RecentContent<'a> {
        RecentContent::Image(RecentImage {
            png,
            width: 4,
            height: 2,
            pixel_digest: [digest; 32],
            thumb: Some(thumb),
            label: "Image 4 \u{d7} 2",
        })
    }

    fn thumb() -> RecentThumb {
        RecentThumb {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 255],
        }
    }

    fn keyed(dir: &std::path::Path) -> Store {
        let p = provider();
        let mut s = Store::open(dir).unwrap();
        s.create_key_ring(&ring(&p), None).unwrap();
        s
    }

    #[test]
    fn v7_migration_keeps_v6_text_rows() {
        let dir = tempfile::tempdir().unwrap();
        let m = marker("v6row");
        {
            let mut s = Store::open(dir.path()).unwrap();
            s.add_recent(RecentContent::text(&m), Some("a.exe"), at(1))
                .unwrap();
        }
        // Put the file back the way version 6 left it: no rich columns.
        raw(dir.path())
            .execute_batch(
                "DROP INDEX items_group;
                 ALTER TABLE items DROP COLUMN group_id;
                 DROP TABLE item_groups;
                 ALTER TABLE recent_entries DROP COLUMN kind;
                 ALTER TABLE recent_entries DROP COLUMN rich;
                 ALTER TABLE recent_entries DROP COLUMN blob;
                 ALTER TABLE recent_entries DROP COLUMN blob_len;
                 PRAGMA user_version = 6;",
            )
            .unwrap();
        let s = Store::open(dir.path()).unwrap();
        assert_eq!(s.schema_version().unwrap(), 8);
        let all = s.recent().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].text, m);
        assert_eq!(all[0].kind, RecentKind::Text);
        assert_eq!(all[0].source_app.as_deref(), Some("a.exe"));
        assert_eq!(
            s.recent_payload(all[0].id).unwrap(),
            Some(RecentPayload::Text {
                html: None,
                rtf: None
            })
        );
    }

    #[test]
    fn an_image_blob_on_disk_is_ciphertext() {
        let dir = tempfile::tempdir().unwrap();
        let m = marker("picture");
        let png = picture(&m);
        let th = thumb();
        let mut s = keyed(dir.path());
        let id = s.add_recent(image(&png, &th, 1), None, at(1)).unwrap();
        let blobs = files_under(&dir.path().join("blobs"));
        assert_eq!(blobs.len(), 1, "one blob file");
        for file in files_under(dir.path()) {
            if file.ends_with("snipdeck.lock") || file.to_string_lossy().ends_with("-shm") {
                continue;
            }
            let bytes = std::fs::read(&file).unwrap();
            assert!(
                !bytes.windows(m.len()).any(|w| w == m.as_bytes()),
                "{} holds the picture",
                file.display()
            );
        }
        assert_eq!(
            s.recent_payload(id).unwrap(),
            Some(RecentPayload::Image(png))
        );
        let entry = &s.recent().unwrap()[0];
        assert_eq!(entry.kind, RecentKind::Image);
        assert_eq!(entry.image_size, Some((4, 2)));
        assert_eq!(entry.thumb.as_ref(), Some(&th));
    }

    #[test]
    fn pruning_an_image_row_releases_its_blob() {
        let dir = tempfile::tempdir().unwrap();
        let png = picture(&marker("pruned"));
        let th = thumb();
        let mut s = keyed(dir.path());
        s.add_recent(image(&png, &th, 2), None, at(1)).unwrap();
        assert_eq!(files_under(&dir.path().join("blobs")).len(), 1);
        // Past the entry cap: the oldest goes with its blob.
        s.add_recent(RecentContent::text("newer"), None, at(2))
            .unwrap();
        assert_eq!(s.prune_recent(at(3), 1, WEEK).unwrap(), 1);
        assert!(files_under(&dir.path().join("blobs")).is_empty());
        // Age, delete and clear release too.
        s.add_recent(image(&png, &th, 3), None, at(4)).unwrap();
        assert_eq!(
            s.prune_recent(at(4) + Duration::from_hours(24 * 8), 10, WEEK)
                .unwrap(),
            2
        );
        assert!(files_under(&dir.path().join("blobs")).is_empty());
        let id = s.add_recent(image(&png, &th, 4), None, at(5)).unwrap();
        s.delete_recent(id).unwrap();
        assert!(files_under(&dir.path().join("blobs")).is_empty());
        s.add_recent(image(&png, &th, 5), None, at(6)).unwrap();
        s.clear_recent().unwrap();
        assert!(files_under(&dir.path().join("blobs")).is_empty());
    }

    #[test]
    fn gc_keeps_recent_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let png = picture(&marker("kept"));
        let th = thumb();
        let mut s = keyed(dir.path());
        let id = s.add_recent(image(&png, &th, 6), None, at(1)).unwrap();
        let report = s.gc_blobs(Duration::ZERO).unwrap();
        assert_eq!((report.blobs, report.failed), (0, 0));
        assert_eq!(
            s.recent_payload(id).unwrap(),
            Some(RecentPayload::Image(png))
        );
    }

    #[test]
    fn no_key_ring_skips_images() {
        let mut s = store();
        let th = thumb();
        assert!(!s.recent_seals_rich());
        let err = s.add_recent(image(&[1, 2, 3], &th, 7), None, at(1));
        assert!(matches!(err, Err(Error::Invalid(_))), "{err:?}");
        let paths = vec!["C:\\a.txt".to_owned()];
        let err = s.add_recent(
            RecentContent::Files {
                paths: &paths,
                label: "1 file: a.txt",
            },
            None,
            at(1),
        );
        assert!(matches!(err, Err(Error::Invalid(_))), "{err:?}");
        assert!(s.recent().unwrap().is_empty());
        // Text still works, without its formatted flavours.
        s.add_recent(
            RecentContent::Text {
                text: "t",
                html: Some("<b>t</b>"),
                rtf: None,
            },
            None,
            at(2),
        )
        .unwrap();
        let id = s.recent().unwrap()[0].id;
        assert_eq!(
            s.recent_payload(id).unwrap(),
            Some(RecentPayload::Text {
                html: None,
                rtf: None
            })
        );
    }

    #[test]
    fn a_small_image_and_files_and_text_flavours_round_trip_sealed() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = keyed(dir.path());
        let th = thumb();
        let small = s.add_recent(image(&[9; 100], &th, 8), None, at(1)).unwrap();
        assert!(files_under(&dir.path().join("blobs")).is_empty(), "inline");
        assert_eq!(
            s.recent_payload(small).unwrap(),
            Some(RecentPayload::Image(vec![9; 100]))
        );
        let paths = vec!["C:\\b.txt".to_owned(), "C:\\a.txt".to_owned()];
        let files = s
            .add_recent(
                RecentContent::Files {
                    paths: &paths,
                    label: "2 files: b.txt, a.txt",
                },
                None,
                at(2),
            )
            .unwrap();
        assert_eq!(
            s.recent_payload(files).unwrap(),
            Some(RecentPayload::Files(paths.clone()))
        );
        let rich = s
            .add_recent(
                RecentContent::Text {
                    text: "hello",
                    html: Some("<b>hello</b>"),
                    rtf: Some(b"{\\rtf1 hello}"),
                },
                None,
                at(3),
            )
            .unwrap();
        assert_eq!(
            s.recent_payload(rich).unwrap(),
            Some(RecentPayload::Text {
                html: Some("<b>hello</b>".to_owned()),
                rtf: Some(b"{\\rtf1 hello}".to_vec())
            })
        );
        assert_eq!(s.recent_payload(9999).unwrap(), None);
        let all = s.recent().unwrap();
        assert_eq!(all[1].files, paths);
        assert_eq!(all[1].kind, RecentKind::Files);
    }

    #[test]
    fn rich_payloads_are_sealed_in_the_database_file() {
        let dir = tempfile::tempdir().unwrap();
        let (html, path) = (marker("html"), marker("path"));
        {
            let mut s = keyed(dir.path());
            s.add_recent(
                RecentContent::Text {
                    text: "plain",
                    html: Some(&html),
                    rtf: None,
                },
                None,
                at(1),
            )
            .unwrap();
            let paths = vec![path.clone()];
            s.add_recent(
                RecentContent::Files {
                    paths: &paths,
                    label: "1 file",
                },
                None,
                at(2),
            )
            .unwrap();
        }
        assert!(!holds(dir.path(), &html));
        assert!(!holds(dir.path(), &path));
    }

    #[test]
    fn a_failed_write_releases_the_pictures_blob() {
        let dir = tempfile::tempdir().unwrap();
        let png = picture(&marker("failing"));
        let th = thumb();
        let mut s = keyed(dir.path());
        // Any insert now fails, after the picture was already put in the blob folder.
        raw(dir.path())
            .execute_batch(
                "CREATE TRIGGER no_inserts BEFORE INSERT ON recent_entries
                 BEGIN SELECT RAISE(ABORT, 'refused'); END;",
            )
            .unwrap();
        assert!(s.add_recent(image(&png, &th, 9), None, at(1)).is_err());
        assert!(files_under(&dir.path().join("blobs")).is_empty());
        assert!(s.recent().unwrap().is_empty());
    }

    #[test]
    fn the_entry_cap_releases_the_oldest_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let th = thumb();
        let mut s = keyed(dir.path());
        for i in 0..201_u8 {
            let mut png = picture(&marker("cap"));
            png[0] = i;
            png[1] = u8::from(i > 127);
            s.add_recent(image(&png, &th, i), None, at(i64::from(i)))
                .unwrap();
        }
        assert_eq!(files_under(&dir.path().join("blobs")).len(), 201);
        assert_eq!(s.prune_recent(at(300), 200, WEEK).unwrap(), 1);
        assert_eq!(files_under(&dir.path().join("blobs")).len(), 200);
        assert_eq!(s.recent().unwrap().len(), 200);
    }
}
