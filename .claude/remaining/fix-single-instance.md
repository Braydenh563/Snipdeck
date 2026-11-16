# fix-single-instance (PLATFORM-1, 2, 26, 27, 28, 29 code side)

## Done (code complete; all Windows-only behaviour is UNVERIFIED until run on Windows)
- P27 names: `instance_names.rs` (user SID in every name), owner-only DACL `D:(A;;GA;;;<sid>)S:(ML;;NW;;;ME)`.
- P28: instance id = blake3 of SID + canonical data folder (`platform::instance_id`); `--data-dir` is its own instance; `--quit` uses the same id.
- P26: second launch waits on [ack, first-process handle]; decision in `win_logic::after_handover` (Acked -> Done, FirstExited -> retry up to 3, TimedOut/NotReached -> message box, translated in all six locales).
- P2: first pid published in the shared section; second calls AllowSetForegroundWindow(pid).
- P1: WM_QUERYENDSESSION/WM_ENDSESSION on the tray window -> `TrayEvent::SessionEnding` -> `main::flush_for_session_end` (3 s limit) -> the same `flush_now` the normal exit runs.
- P29: `handover.rs` (framing, strict decode, allow-list, length caps), `cli.rs` request, `associations.rs` table (+ test). First instance acts on file (Import preview on Settings) and `snipdeck://open|panel|settings`. Requests that arrive before the Panel is bound are queued.
- Docs: docs/user-guide.md (open a file, second launch, session end).

## Next / blocked
- Blocked (other worker): packaging must register `.snipdeck` / `Snipdeck.Archive` / `snipdeck:` using `associations.rs` values; command `"<exe>" "%1"`.
- Workflow YAML: no change needed.
- Not done: tools/windows-test script for the items below.

## Windows run list
- P1: type in an item editor, sign out within a second; reopen: draft kept. `shutdown /r /t 0`; check the trace log for `session end flush: Done`.
- P2: launch from Start menu, pinned taskbar button, Win+R while Snipdeck is hidden in the tray: Panel appears with focus.
- P26: suspend the process (Process Explorer), relaunch: message box after the timeout. Quit then relaunch several times: always one instance.
- P27: `runas /user:other snipdeck.exe` while yours runs: separate, no cross-signalling. Elevated first instance, medium-integrity second: message box, no crash.
- P28: `snipdeck.exe --data-dir D:\other` while the normal one runs: two instances; `--quit --data-dir D:\other` ends only that one.
- P29: double-click a .snipdeck file (running and not running); `start snipdeck://settings`; a bogus `snipdeck://x` is refused.
