# maintenance agent: V1-PLAN step 6 (maintenance + crash log)

## Done
- crash.rs: panic hook, report file, retention 10, new-report detection; tests (unit + tests/crash_hook.rs).
- maintenance.rs: scheduler on the injected Clock, maintenance.json state, Jobs trait + ControllerJobs, run(); tests.
- Controller::report_maintenance (one method), ToastMessage::{LibraryProblem, CrashedLastTime} + wording.
- main.rs: hook installed first, data dir told to the hook, timers (2 s then 6 h), start-up purge removed.

## Next
- Nothing in my scope.

## Blocked / for the Panel-binding owner
- The toast action ("Open the log?") needs a button in the Panel binding: wire it to crash::open_log_folder().
- main.rs nudges a redraw with Binding::hotkey_problems(&[]) after a run; replace with a real Binding method.
- Backup date uses TimeZone::system(), which is UTC until jiff's tz-system feature is enabled.
