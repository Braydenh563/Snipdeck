//! Performance guards (PERF audit): counters, not wall time. A single-item write patches the
//! library snapshot and never rebuilds the search index.

use super::*;

#[test]
fn pinning_and_unpinning_patch_the_snapshot_and_never_rebuild_the_index() {
    let mut w = world();
    let _ = w.ctl.view();
    let rebuilds = w.ctl.index_cache.rebuilds();
    w.ctl.pin_row(0);
    let _ = w.ctl.view();
    w.ctl.pin_row(0);
    let _ = w.ctl.view();
    assert_eq!(w.ctl.index_cache.rebuilds(), rebuilds, "no rebuild for a pin");
}

#[test]
fn a_paste_counts_one_use_without_a_rebuild() {
    let mut w = world();
    let _ = w.ctl.view();
    let rebuilds = w.ctl.index_cache.rebuilds();
    let _ = w.ctl.activate_row(0, snipdeck_core::UseMode::Paste);
    let _ = w.ctl.view();
    assert_eq!(w.ctl.index_cache.rebuilds(), rebuilds);
}

#[test]
fn opening_main_reads_the_library_through_the_shared_cache() {
    let mut w = world();
    let _ = w.ctl.view();
    let rebuilds = w.ctl.index_cache.rebuilds();
    let before = w.ctl.index_cache.snapshot(w.ctl.store()).expect("snapshot");
    w.ctl.main_open(crate::main_window::Section::All);
    let screen = w.ctl.main_screen();
    assert!(screen.counts.all > 0, "Main loaded the library");
    assert_eq!(w.ctl.index_cache.rebuilds(), rebuilds, "Main built no index of its own");
    let after = w.ctl.index_cache.snapshot(w.ctl.store()).expect("snapshot");
    assert!(std::rc::Rc::ptr_eq(&before, &after), "one snapshot for both windows");
    // A pin patches the snapshot once for both windows and rebuilds nothing.
    w.ctl.pin_row(0);
    let _ = w.ctl.main_screen();
    let _ = w.ctl.view();
    assert_eq!(w.ctl.index_cache.rebuilds(), rebuilds);
}

#[test]
fn a_view_with_nothing_changed_builds_no_rows() {
    let w = world();
    let _ = w.ctl.view();
    let built = w.ctl.model.rows_built();
    for _ in 0..5 {
        let _ = w.ctl.view();
    }
    assert_eq!(w.ctl.model.rows_built(), built, "redraws reuse the rows");
}
