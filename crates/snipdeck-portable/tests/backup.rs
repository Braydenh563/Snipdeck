//! The daily backup: once per local day, pruned safely, restorable.
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

use std::fs;
use std::path::Path;

use common::*;
use jiff::Zoned;
use jiff::civil::date;
use jiff::tz::{Offset, TimeZone};
use snipdeck_core::{Format, Formats, Item, ItemKind, Placement};
use snipdeck_portable::backup::{
    BackupOutcome, DEFAULT_KEEP, file_name, list_backups, parse_file_name, run_if_due,
};
use snipdeck_portable::{
    BoardLayout, ImportOptions, ImportReport, KeySource, MergeStrategy, import_archive_path,
};
use snipdeck_store::crypto::DataKey;
use snipdeck_store::{ItemRepo, Store};

/// A restore: the shared import path, keeping the backup's boards and skipping what exists.
fn restore(
    lib: &mut Store,
    backup: &std::path::Path,
    now: snipdeck_core::Timestamp,
    key: KeySource<'_>,
    max_boards: Option<usize>,
) -> snipdeck_portable::Result<ImportReport> {
    let options = ImportOptions {
        layout: BoardLayout::KeepBoards,
        merge: MergeStrategy::SkipDuplicates,
        key,
        max_boards,
        ..ImportOptions::default()
    };
    import_archive_path(lib, backup, now, &options)
}

fn zone(hours: i8) -> TimeZone {
    TimeZone::fixed(Offset::from_hours(hours).unwrap())
}

/// A local wall-clock time in a fixed zone.
fn local(y: i16, m: i8, d: i8, h: i8, min: i8, hours_east: i8) -> Zoned {
    date(y, m, d)
        .at(h, min, 0, 0)
        .to_zoned(zone(hours_east))
        .unwrap()
}

fn created(outcome: &BackupOutcome) -> &Path {
    match outcome {
        BackupOutcome::Created { path, .. } => path,
        BackupOutcome::NotDue { .. } => panic!("expected a backup to be created"),
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    out.sort();
    out
}

#[test]
fn file_names_round_trip_and_reject_lookalikes() {
    let d = date(2026, 10, 7);
    assert_eq!(file_name(d), "snipdeck-backup-20261007.snipdeck");
    assert_eq!(
        parse_file_name("snipdeck-backup-20261007.snipdeck"),
        Some(d)
    );
    for bad in [
        "snipdeck-backup-2026107.snipdeck",
        "snipdeck-backup-202610071.snipdeck",
        "snipdeck-backup-20261307.snipdeck",
        "snipdeck-backup-20260230.snipdeck",
        "snipdeck-backup-2026100a.snipdeck",
        "snipdeck-backup-20261007.snipdeck.part",
        "snipdeck-backup-20261007.snipdeck.bak",
        "Snipdeck-backup-20261007.snipdeck",
        "snipdeck-backup-+0261007.snipdeck",
        "snipdeck-backup-20261007",
        " snipdeck-backup-20261007.snipdeck",
        "xsnipdeck-backup-20261007.snipdeck",
        "",
    ] {
        assert_eq!(parse_file_name(bad), None, "{bad:?}");
    }
}

#[test]
fn the_first_run_writes_todays_backup_and_the_second_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let s = sample_store();
    let now = local(2026, 10, 17, 9, 0, 0);

    let first = run_if_due(&s, dir.path(), &now, DEFAULT_KEEP, None).unwrap();
    let path = created(&first).to_owned();
    assert_eq!(
        path,
        dir.path().join("backups/snipdeck-backup-20261017.snipdeck")
    );
    let bytes = fs::read(&path).unwrap();

    let later = local(2026, 10, 17, 23, 59, 0);
    let second = run_if_due(&s, dir.path(), &later, DEFAULT_KEEP, None).unwrap();
    assert_eq!(
        second,
        BackupOutcome::NotDue {
            existing: path.clone()
        }
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "the existing backup is untouched"
    );
    assert_eq!(names(dir.path()), ["snipdeck-backup-20261017.snipdeck"]);
}

#[test]
fn a_new_local_day_means_a_new_backup() {
    let dir = tempfile::tempdir().unwrap();
    let s = sample_store();
    run_if_due(&s, dir.path(), &local(2026, 10, 17, 23, 59, 0), 7, None).unwrap();
    let next = run_if_due(&s, dir.path(), &local(2026, 10, 18, 0, 0, 0), 7, None).unwrap();
    assert!(created(&next).ends_with("snipdeck-backup-20261018.snipdeck"));
    assert_eq!(names(dir.path()).len(), 2);
}

#[test]
fn the_day_is_the_local_day_not_the_utc_day() {
    let dir = tempfile::tempdir().unwrap();
    let s = sample_store();
    // 02:00 UTC on the 18th is 21:00 on the 17th in UTC-5 and 11:00 on the 18th in UTC+9.
    let instant = date(2026, 10, 18)
        .at(2, 0, 0, 0)
        .to_zoned(TimeZone::UTC)
        .unwrap();
    let west = instant.with_time_zone(zone(-5));
    let east = instant.with_time_zone(zone(9));
    let a = run_if_due(&s, dir.path(), &west, 7, None).unwrap();
    assert!(created(&a).ends_with("snipdeck-backup-20261017.snipdeck"));
    let b = run_if_due(&s, dir.path(), &east, 7, None).unwrap();
    assert!(created(&b).ends_with("snipdeck-backup-20261018.snipdeck"));
}

#[test]
fn only_the_newest_keep_backups_survive() {
    let dir = tempfile::tempdir().unwrap();
    let s = sample_store();
    for day in 1..=10 {
        let out = run_if_due(&s, dir.path(), &local(2026, 10, day, 12, 0, 0), 3, None).unwrap();
        let BackupOutcome::Created {
            pruned,
            prune_failed,
            ..
        } = out
        else {
            panic!()
        };
        assert_eq!(prune_failed.len(), 0);
        assert_eq!(pruned.len(), usize::from(day > 3), "day {day}");
    }
    assert_eq!(
        names(dir.path()),
        [
            "snipdeck-backup-20261008.snipdeck",
            "snipdeck-backup-20261009.snipdeck",
            "snipdeck-backup-20261010.snipdeck",
        ]
    );
}

#[test]
fn keep_is_at_least_one_and_the_new_backup_is_never_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let s = sample_store();
    run_if_due(&s, dir.path(), &local(2026, 10, 20, 12, 0, 0), 7, None).unwrap();
    // A clock that went backwards: today is older than an existing backup.
    let out = run_if_due(&s, dir.path(), &local(2026, 10, 10, 12, 0, 0), 0, None).unwrap();
    assert!(created(&out).exists());
    assert!(names(dir.path()).contains(&"snipdeck-backup-20261010.snipdeck".to_owned()));
}

#[test]
fn files_it_did_not_create_are_never_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    fs::create_dir_all(&backups).unwrap();
    let foreign = [
        "notes.txt",
        "snipdeck-backup-2026101.snipdeck",
        "snipdeck-backup-20261017.snipdeck.bak",
        "snipdeck-backup-20269999.snipdeck",
        "Snipdeck-backup-20250101.snipdeck",
        "my-snipdeck-backup-20250101.snipdeck",
        "snipdeck-backup-20250101.snipdeck.old",
        "snipdeck-backup-.snipdeck",
        "snipdeck.db",
    ];
    for name in foreign {
        fs::write(backups.join(name), name).unwrap();
    }
    // A folder that has a backup's name, with a file inside.
    fs::create_dir(backups.join("snipdeck-backup-20250102.snipdeck")).unwrap();
    fs::write(
        backups.join("snipdeck-backup-20250102.snipdeck/inside"),
        "x",
    )
    .unwrap();
    #[cfg(unix)]
    {
        let target = dir.path().join("precious.txt");
        fs::write(&target, "precious").unwrap();
        std::os::unix::fs::symlink(&target, backups.join("snipdeck-backup-20250103.snipdeck"))
            .unwrap();
    }
    // Real old backups, which are the only things that may go.
    let s = sample_store();
    for day in 1..=4 {
        run_if_due(&s, dir.path(), &local(2026, 9, day, 12, 0, 0), 7, None).unwrap();
    }

    let out = run_if_due(&s, dir.path(), &local(2026, 10, 1, 12, 0, 0), 2, None).unwrap();
    let BackupOutcome::Created { pruned, .. } = out else {
        panic!()
    };
    assert_eq!(
        pruned.len(),
        3,
        "of four old backups, all but the newest go"
    );

    for name in foreign {
        assert!(backups.join(name).exists(), "{name} was deleted");
    }
    assert!(
        backups
            .join("snipdeck-backup-20250102.snipdeck/inside")
            .exists()
    );
    #[cfg(unix)]
    {
        assert!(dir.path().join("precious.txt").exists());
        assert!(
            backups
                .join("snipdeck-backup-20250103.snipdeck")
                .symlink_metadata()
                .is_ok()
        );
    }
    let real: Vec<_> = list_backups(dir.path())
        .unwrap()
        .iter()
        .map(|b| b.date)
        .collect();
    assert_eq!(real, [date(2026, 10, 1), date(2026, 9, 4)]);
}

#[test]
fn stale_partial_files_are_cleaned_but_only_exact_ones() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    fs::create_dir_all(&backups).unwrap();
    fs::write(
        backups.join("snipdeck-backup-20261001.snipdeck.part"),
        "half",
    )
    .unwrap();
    fs::write(backups.join("something.part"), "not ours").unwrap();
    let s = sample_store();
    let out = run_if_due(&s, dir.path(), &local(2026, 10, 2, 12, 0, 0), 7, None).unwrap();
    let BackupOutcome::Created { pruned, .. } = out else {
        panic!()
    };
    assert_eq!(pruned.len(), 1);
    assert_eq!(
        names(dir.path()),
        ["snipdeck-backup-20261002.snipdeck", "something.part"]
    );
}

#[test]
fn a_failed_backup_leaves_no_partial_file_and_does_not_prune() {
    let dir = tempfile::tempdir().unwrap();
    let s = sample_store();
    run_if_due(&s, dir.path(), &local(2026, 10, 1, 12, 0, 0), 1, None).unwrap();
    // A limit the library exceeds makes the export fail halfway.
    let tiny = snipdeck_portable::Limits {
        max_total_bytes: 10,
        ..snipdeck_portable::Limits::default()
    };
    let options = snipdeck_portable::ExportOptions {
        limits: tiny,
        ..snipdeck_portable::ExportOptions::default()
    };
    let target = dir.path().join("backups/snipdeck-backup-20261002.snipdeck");
    let err = snipdeck_portable::export_to_path(&s, at(1), &options, &target).unwrap_err();
    assert!(
        matches!(err, snipdeck_portable::Error::LimitExceeded { .. }),
        "{err}"
    );
    assert_eq!(names(dir.path()), ["snipdeck-backup-20261001.snipdeck"]);

    // The data folder is unusable: a file where the backups folder should be.
    let blocked = tempfile::tempdir().unwrap();
    fs::write(blocked.path().join("backups"), "a file").unwrap();
    let err = run_if_due(&s, blocked.path(), &local(2026, 10, 3, 12, 0, 0), 7, None).unwrap_err();
    assert!(
        matches!(err, snipdeck_portable::Error::File { .. }),
        "{err}"
    );
}

fn vault() -> Store {
    let mut s = store();
    let b = board(&mut s, "Vault");
    let l = list(&mut s, b.id, "L");
    text_item(&mut s, l.id, "public", "public text");
    let mut secret = Item::new(l.id, "Card".to_owned(), ItemKind::Text, at(1));
    secret.sensitive = true;
    s.insert_item(
        &secret,
        &Formats::new(vec![Format::Text(inline("BACKUP-SECRET-5512"))]).unwrap(),
        Placement::Last,
    )
    .unwrap();
    s
}

fn has(bytes: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    bytes.windows(n.len()).any(|w| w == n)
        || entries(bytes)
            .values()
            .any(|v| v.windows(n.len()).any(|w| w == n))
}

#[test]
fn sensitive_items_are_encrypted_in_backups_or_left_out() {
    let s = vault();
    let key = DataKey::generate().unwrap();

    let with_key = tempfile::tempdir().unwrap();
    let out = run_if_due(
        &s,
        with_key.path(),
        &local(2026, 10, 1, 1, 0, 0),
        7,
        Some(&key),
    )
    .unwrap();
    let bytes = fs::read(created(&out)).unwrap();
    assert!(!has(&bytes, "BACKUP-SECRET-5512"));
    let BackupOutcome::Created { report, .. } = out else {
        panic!()
    };
    assert_eq!((report.items, report.sensitive_encrypted), (2, 1));

    let without = tempfile::tempdir().unwrap();
    let out = run_if_due(&s, without.path(), &local(2026, 10, 1, 1, 0, 0), 7, None).unwrap();
    let bytes = fs::read(created(&out)).unwrap();
    assert!(!has(&bytes, "BACKUP-SECRET-5512"));
    let BackupOutcome::Created { report, .. } = out else {
        panic!()
    };
    assert_eq!((report.items, report.sensitive_omitted), (1, 1));
}

#[test]
fn restore_is_an_import_that_recreates_boards_and_skips_what_exists() {
    let dir = tempfile::tempdir().unwrap();
    let key = DataKey::generate().unwrap();
    let original = vault();
    let out = run_if_due(
        &original,
        dir.path(),
        &local(2026, 10, 1, 1, 0, 0),
        7,
        Some(&key),
    )
    .unwrap();
    let path = created(&out).to_owned();

    // Into an empty library: everything comes back under its own board name.
    let mut fresh = store();
    let report = restore(&mut fresh, &path, at(5), KeySource::Given(&key), None).unwrap();
    assert_eq!(report.items_added, 2);
    assert_same(&snapshot(&fresh), &snapshot(&original));

    // Over an intact library: nothing is added, no board is created.
    let mut intact = vault();
    let report = restore(&mut intact, &path, at(5), KeySource::Given(&key), None).unwrap();
    assert_eq!(
        (
            report.items_added,
            report.items_skipped,
            report.boards_added
        ),
        (0, 2, 0)
    );
    assert_eq!(counts(&intact), counts(&original));

    // Without the key the sensitive item is skipped with a warning, the rest restored.
    let mut keyless = store();
    let report = restore(&mut keyless, &path, at(5), KeySource::None, None).unwrap();
    assert_eq!((report.items_added, report.items_skipped), (1, 1));
    assert!(report.has_warnings());
    assert_eq!(keyless.all_items().unwrap().len(), 1);
}

/// PR 4 review 6: a restore obeys the board limit like any other import.
#[test]
fn restore_refuses_to_go_past_the_board_limit_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let key = DataKey::generate().unwrap();
    let out = run_if_due(
        &vault(),
        dir.path(),
        &local(2026, 10, 1, 1, 0, 0),
        7,
        Some(&key),
    )
    .unwrap();
    let path = created(&out).to_owned();

    // A library already at its limit of one board: the backup's board would be a second.
    let mut full = store();
    board(&mut full, "Mine");
    let before = counts(&full);
    let err = restore(&mut full, &path, at(5), KeySource::Given(&key), Some(1)).unwrap_err();
    assert!(
        matches!(
            err,
            snipdeck_portable::Error::Core(snipdeck_core::Error::ProRequired(
                snipdeck_core::Feature::UnlimitedBoards
            ))
        ),
        "{err}"
    );
    assert_eq!(counts(&full), before, "nothing was written");

    // Within the limit it restores as usual.
    let mut room = store();
    let report = restore(&mut room, &path, at(5), KeySource::Given(&key), Some(1)).unwrap();
    assert_eq!(report.boards_added, 1);
}

#[test]
fn listing_backups_is_newest_first_and_empty_without_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(list_backups(dir.path()).unwrap().len(), 0);
    let s = sample_store();
    for day in [3, 1, 2] {
        run_if_due(&s, dir.path(), &local(2026, 10, day, 12, 0, 0), 7, None).unwrap();
    }
    let listed = list_backups(dir.path()).unwrap();
    let days: Vec<_> = listed.iter().map(|b| b.date.day()).collect();
    assert_eq!(days, [3, 2, 1]);
    assert!(listed.iter().all(|b| b.bytes > 0));
}
