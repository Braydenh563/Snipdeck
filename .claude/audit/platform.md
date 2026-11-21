# Deep audit: platform

Audit head: 169ad183 (tip of origin/claude/compassionate-pascal-pod8ux), read in a Linux container. Nothing here was run on Windows: every row is `code` unless marked `run`.

## Coverage

| file | status | how |
|---|---|---|
| `Snipdeck.bat` | checked | code |
| `Test Snipdeck on Windows.bat` | checked | code |
| `crates/snipdeck-app/src/controller/tests/tray.rs` | not reached (test file; time went to the Windows code) | code |
| `crates/snipdeck-app/src/hotkey_health.rs` | checked | code |
| `crates/snipdeck-app/src/native_window.rs` | checked | code |
| `crates/snipdeck-app/src/platform.rs` | checked | code |
| `crates/snipdeck-app/src/popup_window.rs` | checked (first 60 lines and outline; no unsafe in it) | code |
| `crates/snipdeck-app/src/shortcuts.rs` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/Cargo.toml` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/src/chord.rs` | checked (the parts the Windows code calls; rest outlined) | code |
| `crates/snipdeck-platform/src/command_line.rs` | checked (the parts the Windows code calls; rest outlined) | code |
| `crates/snipdeck-platform/src/error.rs` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/src/file_dialog.rs` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/src/instance_names.rs` | checked (the parts the Windows code calls; rest outlined) | code |
| `crates/snipdeck-platform/src/keys.rs` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/src/lib.rs` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/src/startup_task.rs` | checked | code |
| `crates/snipdeck-platform/src/system_look.rs` | checked (the parts the Windows code calls; rest outlined) | code |
| `crates/snipdeck-platform/src/testing.rs` | checked (outline: items and signatures only) | code |
| `crates/snipdeck-platform/src/tray.rs` | checked (the parts the Windows code calls; rest outlined) | code |
| `crates/snipdeck-platform/src/win_logic.rs` | checked (the parts the Windows code calls; rest outlined) | code |
| `crates/snipdeck-platform/src/windows/autostart.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/console.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/dll_search.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/dpi.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/file_dialog.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/hidden_window.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/hotkey.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/message_box.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/mod.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/package.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/paint_diag.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/shell_open.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/single_instance.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/system_look.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/tray.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/update_region.rs` | checked | code |
| `crates/snipdeck-platform/src/windows/window_style.rs` | checked | code |
| `packaging/inno/snipdeck.iss` | checked | code |
| `packaging/msix/.gitignore` | checked | code |
| `packaging/msix/AppxManifest.xml` | checked | code |
| `packaging/msix/README.md` | checked (header and outline, grep for PRI and target sizes) | code |
| `run-snipdeck-demo.bat` | checked | code |
| `scripts/installer_check.py` | checked (header and outline, grep for PRI and target sizes) | code |
| `scripts/make_msix_assets.py` | checked | code |
| `scripts/msix_manifest.py` | checked (header and outline, grep for PRI and target sizes) | code |
| `scripts/test_installer_check.py` | not reached (tests of the two scripts) | code |
| `scripts/test_msix_manifest.py` | not reached (tests of the two scripts) | code |
| `spikes/s1-noactivate/Cargo.lock` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s1-noactivate/Cargo.toml` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s1-noactivate/src/main.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s1-noactivate/src/platform/mod.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s1-noactivate/src/platform/stub.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s1-noactivate/src/platform/windows.rs` | checked (every unsafe site listed and skimmed; not shipped code) | code |
| `spikes/s2-ime/--cjk` | checked (file header: an ImageMagick PostScript dump, see PLATFORM-38) | code |
| `spikes/s2-ime/.gitignore` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/Cargo.lock` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/Cargo.toml` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/build.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/fonts/IBMPlexSans-Medium.ttf` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/fonts/IBMPlexSans-Regular.ttf` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/fonts/OFL.txt` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/src/main.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s2-ime/ui/app.slint` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/Cargo.lock` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/Cargo.toml` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/build.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/editor.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/gen.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/harness.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/list.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/main.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/search.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/src/viewer.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s3-longtext/ui/bench.slint` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/.gitignore` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/Cargo.lock` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/Cargo.toml` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/build.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/src/appearance.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/src/main.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/src/platform/stub.rs` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `spikes/s4-mica/src/platform/windows.rs` | checked (every unsafe site listed and skimmed; not shipped code) | code |
| `spikes/s4-mica/ui/panel.slint` | not reached (spike code, not shipped; no FFI outside the two platform/windows.rs files) | code |
| `tools/launcher/README.md` | not reached (build, cli, lib, selftest, README: time) | code |
| `tools/launcher/build.ps1` | not reached (build, cli, lib, selftest, README: time) | code |
| `tools/launcher/cli.ps1` | not reached (build, cli, lib, selftest, README: time) | code |
| `tools/launcher/commands/Back up my library.bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Build from source (developers only).bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Check my setup (doctor).bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Help (all commands).bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Open the logs.bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Reinstall Snipdeck (keep my data).bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Restore a backup.bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Start without rebuilding (after a pull).bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Uninstall Snipdeck (keep my data).bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Uninstall Snipdeck and erase my data.bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/commands/Update Snipdeck.bat` | checked (one read in full; the rest are the same one-line wrapper by name) | code |
| `tools/launcher/core.ps1` | checked (start-up, download, pin, uninstall, autostart and process-stop paths; rest not reached) | code |
| `tools/launcher/data.ps1` | checked (start-up, download, pin, uninstall, autostart and process-stop paths; rest not reached) | code |
| `tools/launcher/lib.ps1` | not reached (build, cli, lib, selftest, README: time) | code |
| `tools/launcher/release-sha256.txt` | checked | code |
| `tools/launcher/release.ps1` | checked (start-up, download, pin, uninstall, autostart and process-stop paths; rest not reached) | code |
| `tools/launcher/selftest.ps1` | not reached (build, cli, lib, selftest, README: time) | code |
| `tools/launcher/snipdeck.ps1` | checked (start-up, download, pin, uninstall, autostart and process-stop paths; rest not reached) | code |
| `tools/launcher/tests/accessdenied.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/corrupt.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/exelocked.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/expect.txt` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/filelock.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/lockmismatch.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/longpath.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/msvc.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/network.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/no-toolchain.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/nospace.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/ok.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/oom.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/release.json` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/revoke.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/launcher/tests/toolchain-old.log` | not reached (log fixtures for the launcher selftest) | code |
| `tools/windows-test/README.md` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/assets/colour-test.html` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/lib.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/mock/primitives.mock.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/primitives.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/qc.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/s1.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/s2.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/s4.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/auto/tests/selftest.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/diag-render.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/flow.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/m2-harness/Cargo.lock` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/m2-harness/Cargo.toml` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/m2-harness/src/app.rs` | checked (every unsafe site listed and skimmed; not shipped code) | code |
| `tools/windows-test/m2-harness/src/items.rs` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/m2-harness/src/main.rs` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/m2-harness/src/sys.rs` | checked | code |
| `tools/windows-test/run-tests.ps1` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/run-tests.sh` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/colour.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/m2-platform.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/m3-app.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/quick-check.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/s1-noactivate.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/s2-ime.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `tools/windows-test/tests/s4-mica.json` | not reached (owner-run Windows harness scripts and JSON plans: time) | code |
| `crates/snipdeck-ui/tests/audit_final1.rs` | not reached (UI tests, no platform code; left to the UI agents) | code |
| `crates/snipdeck-ui/tests/audit_final1_keys.rs` | not reached (UI tests, no platform code; left to the UI agents) | code |
| `crates/snipdeck-ui/tests/audit_final1_tree.rs` | not reached (UI tests, no platform code; left to the UI agents) | code |
| `crates/snipdeck-ui/tests/button_logic.rs` | not reached (UI tests, no platform code; left to the UI agents) | code |
| `crates/snipdeck-ui/tests/buttons.rs` | not reached (UI tests, no platform code; left to the UI agents) | code |

Also read outside the map, for context: `crates/snipdeck-app/src/{host.rs,settings_binding.rs,main.rs,build.rs,window_kind.rs,help_binding.rs}` (parts) and `.github/workflows/{ci.yml,release-build.yml,release.yml}` (Windows jobs).

## Build check

- `cargo check -p snipdeck-platform --all-targets --target x86_64-pc-windows-gnu --locked`: not reached (lock held). Queued on /tmp/target-panel/.audit.lock from 04:24 to 05:08 in three attempts (30 minute background wait, then 300 s and 560 s bounded waits); another agent's gate.sh and test runs held the lock throughout and cargo never started. No Windows-target compile result exists from this audit.
- `cargo clippy` for the same target: not reached (same lock; not started)

## Findings

All verified by `code` (reading), none by `run`. Paths: `P/` is `crates/snipdeck-platform/src/`, `A/` is `crates/snipdeck-app/src/`. Effort: S under an hour, M half a day, L more.

| id | sev | lens | evidence | finding | class-level fix | effort | a Windows run must confirm |
|---|---|---|---|---|---|---|---|
| PLATFORM-1 | medium | session end | no `WM_QUERYENDSESSION` / `WM_ENDSESSION` anywhere in `crates/` (grep empty); cleanup only after the event loop, `A/main.rs:554-565` | At shutdown, sign-out or a Restart Manager close, Windows ends the process without the event loop returning, so `flush_draft`, `flush_settings` and `clear_pending_clipboard_now` never run. A draft typed in the last half second and unsaved settings are lost. | Handle `WM_QUERYENDSESSION` / `WM_ENDSESSION` in the hidden tray window (it already has a wndproc and already maps `WM_CLOSE` to Quit): run the same flush synchronously on `WM_ENDSESSION` with `wParam` true, through one shared "flush now" entry point. | M | Type in an editor, sign out within a second (and again with `shutdown /r /t 0`); after sign-in the draft and the last setting change must be there. Check the trace log for the flush lines. |
| PLATFORM-2 | medium | single instance | `P/windows/single_instance.rs` (no `AllowSetForegroundWindow`, grep empty); `A/native_window.rs:671` `bring_to_foreground` | The second launch only sets the "raise" event. The first process has no foreground right at that moment, so its `SetForegroundWindow` is likely refused and the taskbar button only flashes. | In the second process call `AllowSetForegroundWindow(pid of first)` (or `ASFW_ANY`) before `SetEvent`; publish the first pid through a small shared section. | S | With Snipdeck hidden in the tray and another app focused, start Snipdeck from the Start menu: the Main window must come to the front with keyboard focus, not flash. Repeat from a pinned taskbar button and from `Win+R`. |
| PLATFORM-3 | medium | hotkeys | `A/host.rs:568` `register_all` is called at start-up and on settings changes only; `A/hotkey_health.rs` | A hotkey that fails at start-up (another app holds it, or logon order) is never retried. When the other app exits the hotkey stays dead until the user reopens Settings or restarts. | Keep failed ids in the hotkey service and retry on a slow timer and on session unlock, resume and Settings open; clear the health warning when one succeeds. | M | Hold `Ctrl+Alt+V` in another tool (PowerToys Keyboard Manager or AutoHotkey), start Snipdeck, see the warning, quit the other tool: the hotkey must start working without a restart. |
| PLATFORM-4 | low | hotkeys | `P/windows/system_look.rs` handles only `PBT_APMSUSPEND` and `WTS_SESSION_LOCK`; no `PBT_APMRESUME*`, `WTS_SESSION_UNLOCK`, `WTS_REMOTE_CONNECT` (grep empty) | Nothing checks the hotkeys are still alive after sleep, lock, RDP reconnect or fast user switch. Registrations normally survive, so this is a missing health check, not a known break. | On resume, unlock and remote connect, re-run registration for ids not in the table (same code as PLATFORM-3). | S | Hotkeys fire after: sleep and wake, lock and unlock, RDP disconnect and reconnect to the same session, switch user and back. |
| PLATFORM-5 | medium | hotkeys | `P/chord.rs:392` `Chord::validate` | A global hotkey may be a bare function key (all but F12) or a chord every app uses (`Ctrl+V`, `Ctrl+C`, `Ctrl+X`, `Alt+letter`). `Ctrl+V` as a global hotkey also catches the paste Snipdeck itself injects. No reserved list. | One reserved-chord table in `chord.rs` (clipboard chords, bare F keys, `Alt+F4`, `Alt+Tab`, `Win` chords the shell owns), refused in `validate` with its own error and wording. | S | In Settings try to record `Ctrl+V`, `F5`, `Alt+F4`, `Win+L`: each must be refused with a reason. If `Ctrl+V` is accepted, paste a snippet and watch for a loop. |
| PLATFORM-6 | low | hotkeys | `A/host.rs:568-586`; `P/windows/hotkey.rs:277-317` `do_register` | Applying a set in which two actions swap chords fails for both: each id is registered while the other still holds the chord, the rollback puts both old chords back and both are reported `InUse`. The Settings page changes one action at a time, so this needs a path that applies several at once (settings restore or import, reset). | In `apply_hotkeys`, unregister every id whose chord changes first, then register all. Add the swap case to the `FakeHotkeys` tests. | S | Restore or import settings whose two hotkeys are swapped against the current ones: both must register with no warning. |
| PLATFORM-7 | low | hotkeys, wording | `P/windows/hotkey.rs:57-76`, `:360-366` | Every `ERROR_HOTKEY_ALREADY_REGISTERED` is worded as "another application", also when Snipdeck's own other action holds the chord. | Check the service's own table first and return a distinct "used by Snipdeck's other action" error. | S | Give two actions the same chord by editing the settings file; the warning must name Snipdeck's own action. |
| PLATFORM-8 | low | hotkeys | `A/hotkey_health.rs:29-50` `ALTERNATIVE_POOL` | The alternatives offered after a conflict are filtered against Snipdeck's own chords only, never probed with the system, so an offered chord can be taken too. | Probe each candidate with a register and unregister on the hotkey thread before offering it. | S | With the default and the first two pool chords held by AutoHotkey, the offer must skip them. |
| PLATFORM-9 | low | hotkeys, layouts | `P/keys.rs`, `P/chord.rs` (`to_win32`): punctuation is `VK_OEM_*` shown with US names | On non-US layouts the label in Settings names the US character for the key, not what is printed on the user's key. | Label OEM keys through `MapVirtualKeyExW(MAPVK_VK_TO_CHAR)` for the current layout, at display time only. | M | On German and French layouts record a chord with a punctuation key: the label must match the key cap. |
| PLATFORM-10 | low | threads | `P/windows/hotkey.rs:157-175`, `P/windows/tray.rs:119-130` (`recv_timeout(REPLY_TIMEOUT)`); joins at `hotkey.rs:124,194` and in tray `Drop` | When a call times out (5 s) or the post fails, the caller is told "stopped" but the command stays queued and runs later, so the app's view and the thread's table can differ. `Drop` joins the worker from the UI thread with no bound. | Give each command a cancel flag checked before it runs; make shutdown a bounded join that falls through. | M | Hard to force: suspend the hotkey thread in a debugger for 6 s during a settings change, resume, and compare Settings with what really fires. |
| PLATFORM-11 | low | hotkeys, feedback | `P/windows/hotkey.rs:70-80` | Any other OS error becomes a plain "unavailable" for the user; the code is only in the trace. | Carry the code into the warning text ("Windows error 5") or the doctor report. | S | Read the warning text when a registration fails for a reason other than a conflict. |
| PLATFORM-12 | medium | autostart | `P/windows/autostart.rs:140-150` `is_enabled` (value exists, nothing else); doc at `:7` | The toggle reads "on" when Windows has the entry disabled (Task Manager or Settings, `StartupApproved\Run`) and when the stored path points at an exe that is gone (portable folder moved, old install). Nothing re-syncs the path at start-up. | Read the value and compare it with the current exe command line; read `StartupApproved\Run` and show a third state "switched off in Windows"; rewrite a stale path at start-up when the setting is on. | M | Turn autostart on, disable it in Task Manager, reopen Settings: must not show plain "on". Move the portable folder, start it: the Run value must point at the new path. |
| PLATFORM-13 | low | uninstall leftovers | `packaging/inno/snipdeck.iss:104` removes the Run value only | Setup's uninstall leaves `HKCU\...\Explorer\StartupApproved\Run\Snipdeck` behind (the launcher's uninstall removes it). | Add the `StartupApproved\Run` value with `uninsdeletevalue`, one shared list of leftovers for Setup and the launcher. | S | After uninstall, `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run` shows no Snipdeck value. |
| PLATFORM-14 | medium | autostart, MSIX | `P/startup_task.rs`, `P/windows/package.rs` (`set_startup_task`, `startup_task_enabled` join a worker) | Packaged builds block the UI thread on a WinRT round trip each time. `RequestEnableAsync` from an MTA worker and reading the activation kind off the UI thread are unproven, and when the user disabled the task in Windows the request cannot turn it back on; a failure only shows as a visible start, with no message of its own. | Make the call async with a result callback; map `DisabledByUser` and `DisabledByPolicy` to their own Settings text ("turn it on in Windows Settings, Apps, Startup"). | M | In the MSIX build: toggle on (consent prompt appears, toggle follows the answer), disable in Settings, Apps, Startup, toggle again (must explain, not silently stay off), sign in again and check the app starts hidden. |
| PLATFORM-15 | medium | manifest | `crates/snipdeck-app/build.rs` (winresource, no `set_manifest`; grep empty) | The exe has no application manifest: no Common Controls v6 (old-style message box buttons), no `longPathAware` (import, export and backup paths over 260 characters fail), no `supportedOS`, no declared `dpiAwareness` (it relies on a run-time call), no UTF-8 code page. | Embed one manifest: comctl32 v6, `longPathAware`, `supportedOS` for 10/11, `dpiAwareness` PerMonitorV2, `asInvoker`. Add an installer_check rule that the built exe has it. | S | `mt.exe -inputresource:snipdeck.exe;#1 -out:m.xml` shows the manifest; the fatal-error message box has themed buttons; exporting to a path longer than 260 characters works (with the system long-path policy on). |
| PLATFORM-16 | low | DLL search | `P/windows/dll_search.rs:22` vs its module doc; `crates/snipdeck-app/build.rs:26-31` | The doc promises protection against a DLL planted beside the exe, but `LOAD_LIBRARY_SEARCH_APPLICATION_DIR` is kept, so the app folder is still searched. The link flag is MSVC only. Restricting the search can also stop third-party shell extensions that file dialogs load. | Make the doc say what is true (current directory and PATH are removed). Assert the MSVC target in release CI. | S | Put a dummy `version.dll` beside the exe and start: record whether it loads. Open the file dialogs on a machine with OneDrive and a shell extension pack: they must open and browse. |
| PLATFORM-17 | medium | UI thread | `P/windows/file_dialog.rs:47-66,72,144,196` run modally on the caller; callers are Slint callbacks | While an Open, Save or folder dialog is up, the UI thread sits in the dialog's own loop inside a Slint callback. Slint timers (clipboard auto-clear, maintenance), hotkey events and repaint of the owner window are expected to stall until it closes. | Run the dialogs on a dedicated STA thread with the owner handle and deliver the result through `invoke_from_event_loop`. | M | Copy a snippet with auto-clear on, open Export and leave it past the clear time: the clipboard must be cleared on time. Press a global hotkey with the dialog open. Drag the dialog over the owner window and look for unpainted areas. |
| PLATFORM-18 | low | recent files | `P/windows/file_dialog.rs:104-111,156-160` | No `FOS_DONTADDTORECENT` / `OFN_DONTADDTORECENT`: backup and export file names land in Windows Recent and jump lists. | Add the flag in the one place the flags are built. | S | After an export, the file is not in `shell:recent`. |
| PLATFORM-19 | low | theme | no `DWMWA_USE_IMMERSIVE_DARK_MODE` or `set_theme` outside `paint_diag.rs` | Title bars of the regular windows follow the Windows app mode, not Snipdeck's own Light or Dark setting, so a dark app can sit under a light title bar. | Set the window theme from the app theme in the one place windows are attached (`native_window.rs`). | S | System light, Snipdeck dark (and the reverse): the title bar of Main, Settings and the editors matches the window body. |
| PLATFORM-20 | low | version gates | no `RtlGetVersion` or build check in `crates/`; no `DWMWA_WINDOW_CORNER_PREFERENCE` outside diagnostics; `snipdeck.iss:48` allows 10.0.18362 | Nothing in the code differs between Windows 10 and 11, and only Windows 11 build 26300 has been run. Corners and shadow of the frameless Panel, Picker and toast are left to the system default on both. | State the corner preference explicitly on 11 (build 22000 and up) and test the minimum the installer admits, or raise `MinVersion` to what has been run. | M | On Windows 10 22H2 and Windows 11: Panel, Picker, toast edges and shadow; tray icon and menu; hotkeys; autostart. |
| PLATFORM-21 | low | system look | `P/windows/system_look.rs:130-141` (`ColorizationColor`), `:70-82` (`TextScaleFactor`) | The accent is read from the DWM colorization value, which differs from the accent colour shown in Settings (blend and alpha). The change notice for text scale relies on an undocumented `WM_SETTINGCHANGE` string. | Read `HKCU\...\Explorer\Accent\AccentColorMenu` or `UISettings`, fall back to the current value. | S | Pick a strong accent in Settings: the app accent matches the colour swatch. Move the Text size slider: the app follows without a restart. |
| PLATFORM-22 | low | console | `P/windows/console.rs:18` | CLI output of the GUI-subsystem exe lands after the shell prompt has already returned, and `cmd` does not wait for the exit code. | Document `start /wait snipdeck.exe ...` in the help text, or ship a small console stub (`snipdeck.com`). | M | In `cmd` and PowerShell run `snipdeck.exe --check-db` and `echo %ERRORLEVEL%`: note where the text lands and the code. |
| PLATFORM-23 | medium | tray | `P/windows/tray.rs:263` (`NIM_ADD` once), `:424-431` (one attempt on `TaskbarCreated`) | If the shell refuses the icon (Explorer still starting at sign-in, or slow after a restart) there is no timed retry. An autostart with `--hidden` then leaves the app with no icon and no window, reachable only by hotkey. | Retry `NIM_ADD` with a short back-off while "wanted but not added"; one state machine for start-up and `TaskbarCreated`. | S | Sign in with autostart on a slow or cold machine ten times: icon present each time. Kill and restart `explorer.exe` five times: icon returns each time, menu still works. |
| PLATFORM-24 | low | tray, DPI | `P/windows/tray.rs:451-454` `load_icon` at `dpi::small_icon_size()`, no `WM_DPICHANGED` handling | The tray icon is loaded once at the start-up DPI and never reloaded when the scale changes, so it is blurred or cropped until restart. | Reload and `NIM_MODIFY` on `WM_DPICHANGED` and on `TaskbarCreated`. | S | Change the display scale from 100 to 200 percent without signing out: the icon stays sharp. |
| PLATFORM-25 | low | tray input | `P/windows/tray.rs:13` and the click decode in `P/tray.rs` | `NIN_KEYSELECT` can arrive twice for one Enter press (a toggle would open and close), the single click waits for the double-click time before acting, and the order of click and double-click messages is assumed. | Debounce `NIN_KEYSELECT`; make the double-click action optional so a single click can act at once. | S | Focus the icon with `Win+B`, press Enter and then Space: the Panel opens once. Time a mouse click to Panel open. |
| PLATFORM-26 | medium | single instance | `A/main.rs:224` `Start::AlreadyRunning => return Ok(())` | A second launch exits silently when the first instance is hung, elevated, or already quitting (the second signals a process that is on its way out and both end). The user double-clicks and nothing happens. | After signalling, wait for an acknowledgement event from the first instance; on timeout show a message box with what to do. | M | Start Snipdeck, suspend it (Resource Monitor), start it again: a message must appear. Choose Quit and start again at once, several times: one instance must end up running. |
| PLATFORM-27 | low | single instance | `P/windows/single_instance.rs:130-147` (`None` security attributes, names in `Local\`) | The mutex and events have the default DACL and guessable names: any process in the session can create them first and make Snipdeck exit silently, or set the quit event. A second user in the same session (run as) collides on the names. | Add the user SID to the names and an explicit DACL for that user only. | S | `runas /user:other snipdeck.exe` while it runs: it must start its own instance or say why not. |
| PLATFORM-28 | low | single instance | `A/platform.rs:609` `APP_ID = "main"` | The instance name does not depend on the data folder, so `--data-dir X` while the normal instance runs just raises the normal one. | Derive the id from the canonical data folder (hash) or refuse with a message. | S | Run the app, then `snipdeck.exe --data-dir D:\other`: note what happens; it must not be silent. |
| PLATFORM-29 | low | missing | `P/windows/single_instance.rs` (two signals only); `snipdeck.iss`, `AppxManifest.xml` (no associations, no protocol) | The second launch cannot hand over arguments, and no file type or URL scheme is registered (a backup file cannot be opened by double-click). No AppExecutionAlias in the MSIX, so `snipdeck` is not on the command line there. | If wanted: a named pipe or `WM_COPYDATA` hand-over, then the association in both packages from one table. | L | Nothing until built. |
| PLATFORM-30 | low | installer, i18n | `packaging/inno/snipdeck.iss:67-77` | The custom messages (still running, erase question, desktop icon task) exist in English only, so the five translated Setup languages show mixed text. Chinese Simplified has no Setup language at all (stated in the file). | Add a per-language line for each custom message, checked by `installer_check.py` (every custom message has every language). | S | Run Setup on German Windows: no English sentence in the wizard or the uninstall question. |
| PLATFORM-31 | medium | uninstall | `packaging/inno/snipdeck.iss:195-203` | "Erase my data" deletes `{localappdata}\Snipdeck` only: a data folder set with `--data-dir` stays, although the user asked for everything to go. The two `DelTree` paths are built from `GetEnv`, so an empty variable yields a path at the drive root. | Record the data folder in the registry at first run and erase that; skip `DelTree` when the variable is empty. | M | Install, use a custom data folder, uninstall with erase: list what remains under `%LOCALAPPDATA%`, `%TEMP%`, `%USERPROFILE%` and the custom folder. |
| PLATFORM-32 | low | upgrade | `packaging/inno/snipdeck.iss:58-60,107,172` | No guard against installing an older version over a newer one (a newer database schema then meets an older exe). After an interactive upgrade the app relaunches visible even when it had been running hidden. | Compare versions in `InitializeSetup` and warn; relaunch with `--hidden` when the old instance was running. | S | Install a newer build, then run an older Setup: it must warn. Upgrade while hidden in the tray: note whether a window appears. |
| PLATFORM-33 | medium | MSIX data | `packaging/msix/AppxManifest.xml` (packaged, `runFullTrust`), `A/main.rs:905-907` data folder from the environment | In the Store package, writes under `%LOCALAPPDATA%` are redirected into the package, so a Store uninstall deletes the library and backups with no question, and moving from Setup to Store (or back) depends on merged-view behaviour that has not been run. | Decide and document one rule; in the packaged build keep backups in Documents by default or remind to export. | M | Install the Setup build, add items, install the MSIX: are the items there, and where is the file on disk? Uninstall the MSIX: what remains? |
| PLATFORM-34 | low | MSIX assets | `scripts/make_msix_assets.py:21-30`; no `makepri` or `resources.pri` in `packaging/msix/README.md` or `scripts/msix_manifest.py` | Only one unplated target size (24) is made; Windows asks for 16, 24, 32, 48 and 256. No step builds `resources.pri`, without which the scale and target-size files are not picked up. | Generate the full target-size set and add the `makepri` step to the packaging recipe, checked by `installer_check.py`. | S | Install the MSIX: taskbar and Start icons are sharp at 100, 150 and 200 percent and have no coloured plate on the taskbar. |
| PLATFORM-35 | medium | dev build | `.github/workflows/release-build.yml:3,41` (debug profile); `A/main.rs:9` (`windows_subsystem` only without debug assertions) | The dev zip and dev Setup the owner tests are console-subsystem builds: a console window opens with every start (also on autostart), and closing it kills the app with no flush. Findings from such runs (focus, foreground, start-up look) may not hold for the release exe. | Decouple the subsystem from `debug_assertions` (a cargo feature or a `cfg` set by the workflow) so dev builds are GUI subsystem too. | S | Start the dev build from Explorer: no console window. Compare focus behaviour of the Panel between the dev and the release exe once. |
| PLATFORM-36 | low | CI | `.github/workflows/release.yml:209`, `release-build.yml:36` (`stable`, unpinned); Windows smoke is `--check-db` only | Release builds use whatever stable is current on the day, and no job ever starts the Windows GUI exe. A crash at window creation or in tray or hotkey start-up ships unseen. | Pin the toolchain to the version CI tested; add a smoke step on the Windows runner: start `--hidden`, wait for the mutex, `--quit`, assert exit code 0 and a clean log. | S | The smoke step itself is the check; its first run shows whether a hosted runner session allows a tray icon and `RegisterHotKey`. |
| PLATFORM-37 | medium | launcher | `tools/launcher/core.ps1` `Stop-AppProcesses`, `tools/launcher/release.ps1:248-258,415-431` | Update, reinstall and uninstall stop the app with `CloseMainWindow` and then `Stop-Process -Force` after 3 s. An app hidden in the tray has no main window, so it is force-killed with no flush, and any process named `snipdeck` is matched. | Call `snipdeck.exe --quit` first (the app already has the quit event), wait for the mutex to go, and only then fall back to kill; match by full exe path. | S | With the app hidden and an unsaved draft, run "Update Snipdeck": the draft survives and the log shows a normal quit. |
| PLATFORM-38 | low | repo hygiene | `spikes/s2-ime/--cjk` (tracked, 264 KB, header `%!PS-Adobe-3.0`, creator ImageMagick, title `(--cjk)`, 1200x850) | An image written by a mistyped ImageMagick command is committed under a name that starts with two dashes, which breaks naive `rm`, `cat` and some tools, and its content has not been looked at. | Delete it; add a gate check that no tracked file name starts with `-`. | S | Nothing. |
| PLATFORM-39 | low | launcher trust | `tools/launcher/release-sha256.txt` (no pins), `tools/launcher/release.ps1:94-128` | With no pin committed, every download is checked only against the hash file from the same release, behind a y/N prompt that users will learn to accept. | Make committing the pin a step of the release recipe, checked by the release workflow. | S | After the next release, `Snipdeck.bat update` on a clean machine shows no unpinned prompt. |
| PLATFORM-40 | low | diagnostics | `P/windows/paint_diag.rs` (hook procedures format strings and write the log) | The paint diagnostics do file I/O inside window hook procedures. Diagnostic mode only, but the timing there changes what it measures. | Push records to a channel and write them from another thread. | S | Nothing until the diagnostics are next used. |
| PLATFORM-41 | low | missing | `.github/workflows/release.yml` (x64 only) | No native ARM64 build. The x64 exe runs emulated on ARM64 Windows; untested there. | Add `aarch64-pc-windows-msvc` to the Windows check job first. | M | On an ARM64 device: start, hotkey, paste, tray. |

## No finding (looked, sound as read)

- `unsafe` and FFI in `P/windows/*`: handler pointer lifetime in `hidden_window.rs`, owned and borrowed handles in `single_instance.rs`, COM guard drop order in `file_dialog.rs`, the subclass procedure in `window_style.rs`, registry key guard in `autostart.rs`. Callbacks are wrapped in `catch_unwind`. No handle leak or use after free found by reading.
- `AppMutex=Snipdeck-main` in the installer matches the `Local\Snipdeck-main` name the app creates.
- `shell_open::open` receives only constant URLs and the app's own folders.
- `hotkey::do_register` keeps an unchanged chord in place and puts the old chord back on failure; switching an action off is handled in `settings_binding::apply_hotkeys`.
- Spike and harness FFI (`spikes/*/platform/windows.rs`, `m2-harness/src/sys.rs`): not shipped; token and process handles are closed; callbacks have no `catch_unwind`, which aborts rather than being undefined.
- Launcher unzip checks entry names (`Test-ZipEntries`) before `Expand-Archive`.

## Could not be verified here; a Windows run must check

Everything above is from reading. On top of the per-finding checks:

1. The build and clippy on the MSVC target, which is what ships; the container only has the GNU target (see "Build check").
2. Thread affinity at run time: tray and hotkey threads each own their window or queue; run start, pause hotkeys, resume, quit under Application Verifier (Handles, Locks).
3. Panic paths: force a panic inside a tray callback (debug switch) and confirm the app logs it and survives or exits cleanly, with the icon removed.
4. Explorer restart, DPI change, sleep and wake, lock and unlock, RDP reconnect, user switch: icon, menu, hotkeys and the Panel position after each.
5. `WS_EX_NOACTIVATE` Panel and the style subclass on Windows 10 (only Windows 11 26300 has been run).
6. Setup: install, upgrade over a running instance, silent install (`/VERYSILENT`), uninstall with and without erase; leftovers in the registry (`Run`, `StartupApproved`, `Uninstall`) and on disk.
7. MSIX: pack, sign with a test certificate, install; StartupTask consent, data location, uninstall result, logos at three scales.
8. File dialogs with the restricted DLL search path on a machine with cloud-storage shell extensions.
9. Narrator, IME and elevated paste targets were left out: they are recorded in `docs/HANDOVER.md` or covered by other audit files.
