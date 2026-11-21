//! The panic hook end to end: a panic on another thread leaves one report in the data folder.
//! One test per file because the hook is process-wide.
#![allow(
    clippy::expect_used,
    reason = "a test file: a failed setup is a failed test"
)]

use snipdeck_app::crash;
use snipdeck_core::Timestamp;

/// A fixed "now" for the hook (it takes a function, not the clock).
fn frozen() -> Timestamp {
    "2026-10-18T09:30:45Z".parse().expect("timestamp")
}

#[test]
fn a_panic_in_a_thread_writes_one_report_without_library_content() {
    let dir = tempfile::tempdir().expect("temp dir");
    crash::install(frozen);
    crash::set_data_dir(dir.path());

    let joined = std::thread::Builder::new()
        .name("worker".into())
        .spawn(|| {
            let secret = "my-password-snippet";
            let _ = secret.len();
            panic!("something broke");
        })
        .expect("spawn")
        .join();
    assert!(joined.is_err(), "the thread panicked");

    let names = crash::report_names(dir.path());
    assert_eq!(names.len(), 1, "{names:?}");
    // The hook uses the zone of the machine; the date part is the frozen day in any zone near UTC.
    assert!(names[0].starts_with("crash-2026-10-"), "{}", names[0]);
    let text = std::fs::read_to_string(crash::logs_dir(dir.path()).join(&names[0])).expect("read");
    assert!(text.contains("message: something broke"), "{text}");
    assert!(text.contains("thread: worker"));
    assert!(text.contains("location: "));
    assert!(
        text.contains("crash_hook.rs"),
        "the location names this file: {text}"
    );
    assert!(text.contains("backtrace:"));
    assert!(!text.contains("my-password-snippet"));
}
