//! How backups are kept (DATA-6, 14, 15, 16, 17): a manual backup is its own file, a damaged
//! file does not block the day, daily, weekly and monthly copies are kept, a backup that lost
//! most of the library never pushes a good one out, and every new backup is read back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests unwrap freely; a failure is the test failing"
)]

mod common;

use std::fs;
use std::path::Path;

use common::*;
use jiff::Zoned;
use jiff::civil::date;
use jiff::tz::{Offset, TimeZone};
use snipdeck_portable::backup::{
    BackupOutcome, apply_retention, list_backups, run_if_due, run_manual,
};
use snipdeck_store::{BoardRepo, ItemRepo, ListRepo, TrashRepo, Trashable};

fn local(y: i16, m: i8, d: i8, h: i8, min: i8) -> Zoned {
    date(y, m, d)
        .at(h, min, 0, 0)
        .to_zoned(TimeZone::fixed(Offset::from_hours(2).unwrap()))
        .unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    out.sort();
    out
}

#[test]
fn a_manual_backup_is_its_own_file_and_leaves_the_daily_one_alone() {
    let s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    let first = run_if_due(&s, dir.path(), &local(2026, 11, 16, 8, 0), 7, None).unwrap();
    let BackupOutcome::Created { path: daily, .. } = first else {
        panic!("a backup is made")
    };
    let before = fs::read(&daily).unwrap();

    let manual = run_manual(&s, dir.path(), &local(2026, 11, 16, 17, 45), None).unwrap();
    let BackupOutcome::Created { path, .. } = manual else {
        panic!("a manual backup is made")
    };
    assert!(
        path.ends_with("snipdeck-backup-20261116-174500.snipdeck"),
        "{path:?}"
    );
    assert_eq!(
        fs::read(&daily).unwrap(),
        before,
        "the daily file is untouched"
    );
    let listed = list_backups(dir.path()).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed.iter().filter(|b| b.manual).count(), 1);
    assert!(listed[0].manual, "newest first");
}

#[test]
fn two_manual_backups_in_the_same_minute_are_two_files() {
    let s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    let at = |sec: i8| {
        date(2026, 11, 16)
            .at(17, 45, sec, 0)
            .to_zoned(TimeZone::fixed(Offset::from_hours(2).unwrap()))
            .unwrap()
    };
    let BackupOutcome::Created { path: first, .. } =
        run_manual(&s, dir.path(), &at(5), None).unwrap()
    else {
        panic!("a manual backup is made")
    };
    let kept = fs::read(&first).unwrap();
    let BackupOutcome::Created { path: second, .. } =
        run_manual(&s, dir.path(), &at(40), None).unwrap()
    else {
        panic!("a second manual backup is made")
    };
    assert_ne!(
        first, second,
        "the second press does not reuse the first file"
    );
    assert_eq!(
        fs::read(&first).unwrap(),
        kept,
        "the first copy is untouched"
    );
    assert_eq!(
        names(dir.path()),
        [
            "snipdeck-backup-20261116-174505.snipdeck",
            "snipdeck-backup-20261116-174540.snipdeck"
        ]
    );
    let listed = list_backups(dir.path()).unwrap();
    assert!(
        listed[0]
            .path
            .ends_with("snipdeck-backup-20261116-174540.snipdeck"),
        "newest first"
    );
}

#[test]
fn manual_backups_are_kept_ten_deep_and_never_count_against_the_daily_ones() {
    let s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    run_if_due(&s, dir.path(), &local(2026, 11, 1, 8, 0), 1, None).unwrap();
    for minute in 0..12 {
        run_manual(&s, dir.path(), &local(2026, 11, 1, 9, minute), None).unwrap();
    }
    let listed = list_backups(dir.path()).unwrap();
    assert_eq!(listed.iter().filter(|b| b.manual).count(), 10);
    assert_eq!(listed.iter().filter(|b| !b.manual).count(), 1);
}

#[test]
fn an_empty_or_cut_off_file_with_todays_name_does_not_block_todays_backup() {
    let s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    let today = dir.path().join("snipdeck-backup-20261116.snipdeck");
    fs::write(&today, b"").unwrap();
    let out = run_if_due(&s, dir.path(), &local(2026, 11, 16, 10, 0), 7, None).unwrap();
    assert!(matches!(out, BackupOutcome::Created { .. }), "{out:?}");
    assert!(fs::metadata(&today).unwrap().len() > 100);
    // A good file is left alone.
    let again = run_if_due(&s, dir.path(), &local(2026, 11, 16, 11, 0), 7, None).unwrap();
    assert!(matches!(again, BackupOutcome::NotDue { .. }));
}

#[test]
fn every_new_backup_is_read_back_and_the_summary_says_what_it_holds() {
    let s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    let out = run_if_due(&s, dir.path(), &local(2026, 11, 16, 10, 0), 7, None).unwrap();
    let BackupOutcome::Created {
        report, checked, ..
    } = out
    else {
        panic!()
    };
    assert_eq!(checked.items, report.items);
    assert_eq!(
        (checked.boards, checked.lists),
        (report.boards, report.lists)
    );
}

#[test]
fn daily_weekly_and_monthly_copies_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    // A file for every day of 2026 up to the 16th of November (a Monday).
    let mut day = date(2026, 1, 1);
    while day <= date(2026, 11, 16) {
        let name = snipdeck_portable::backup::file_name(day);
        fs::write(dir.path().join(name), b"x").unwrap();
        day = day.tomorrow().unwrap();
    }
    // One in the future (a clock that was once wrong) stays and takes no slot.
    fs::write(dir.path().join("snipdeck-backup-20271231.snipdeck"), b"x").unwrap();
    fs::write(dir.path().join("notes.txt"), b"mine").unwrap();

    let removed = apply_retention(dir.path(), 7, date(2026, 11, 16));
    assert!(removed.failed.is_empty());
    let kept: Vec<String> = names(dir.path())
        .into_iter()
        .filter_map(|n| {
            n.strip_prefix("snipdeck-backup-")
                .and_then(|n| n.strip_suffix(".snipdeck"))
                .map(str::to_owned)
        })
        .collect();
    let expected = [
        // seven dailies
        "20261110", "20261111", "20261112", "20261113", "20261114", "20261115", "20261116",
        // the last four full weeks before the current one: 46 (covered by the dailies), 45, 44, 43
        "20261108", "20261101", "20261025", // the last six months before this one
        "20261031", "20260930", "20260831", "20260731", "20260630", "20260531",
        // not ours to touch
        "20271231",
    ];
    let mut expected: Vec<String> = expected.iter().map(|s| (*s).to_owned()).collect();
    expected.sort();
    assert_eq!(kept, expected);
    assert!(names(dir.path()).contains(&"notes.txt".to_owned()));
}

#[test]
fn lowering_the_number_kept_removes_files_at_once() {
    let dir = tempfile::tempdir().unwrap();
    for d in 10..=16 {
        let name = snipdeck_portable::backup::file_name(date(2026, 11, d));
        fs::write(dir.path().join(name), b"x").unwrap();
    }
    let removed = apply_retention(dir.path(), 2, date(2026, 11, 16));
    // 2 daily, week 46 (Nov 9..15) is the week before this Monday: its newest is Nov 15, which
    // is already a daily. Nothing older exists.
    assert_eq!(removed.removed.len(), 5);
    assert_eq!(list_backups(dir.path()).unwrap().len(), 2);
}

#[test]
fn a_backup_that_lost_most_of_the_library_does_not_push_a_good_one_out() {
    let mut s = sample_store();
    let dir = tempfile::tempdir().unwrap();
    let first = run_if_due(&s, dir.path(), &local(2026, 11, 15, 8, 0), 1, None).unwrap();
    let BackupOutcome::Created {
        path: good, report, ..
    } = first
    else {
        panic!()
    };
    assert!(report.items >= 6);
    // The library is emptied (a zero-length database, a mistake).
    for board in s.boards().unwrap() {
        for list in s.lists(board.id).unwrap() {
            for item in s.items(list.id).unwrap() {
                s.trash(Trashable::Item(item.id), at(100)).unwrap();
            }
        }
    }
    let second = run_if_due(&s, dir.path(), &local(2026, 11, 16, 8, 0), 1, None).unwrap();
    let BackupOutcome::Created {
        suspect, pruned, ..
    } = second
    else {
        panic!()
    };
    let shrunk = suspect.expect("the loss is noticed");
    assert_eq!(shrunk.now, 0);
    assert_eq!(shrunk.was, report.items);
    assert!(pruned.is_empty());
    assert!(good.exists(), "the good copy stays");
}
