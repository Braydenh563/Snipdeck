//! UX-25: the windows learn which archive job runs, so the button that started it shows it is working.

use super::*;
use crate::archive_job::ArchiveOp;
use crate::controller::archive_ctl::busy_op_of;

#[test]
fn every_archive_step_has_a_busy_number_and_idle_is_zero() {
    let w = build(Opts::default());
    assert_eq!(w.ctl.archive_busy_op(), 0);
    assert!(!w.ctl.archive_busy());
    assert_eq!(busy_op_of(ArchiveOp::BackupNow), 1);
    assert_eq!(busy_op_of(ArchiveOp::DailyBackup), 1);
    assert_eq!(busy_op_of(ArchiveOp::Export), 2);
    assert_eq!(busy_op_of(ArchiveOp::Salvage), 3);
    assert_eq!(busy_op_of(ArchiveOp::Decode), 3);
}
