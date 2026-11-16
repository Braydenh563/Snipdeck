# fix-hotkeys (PLATFORM-3..11)

## Done (all pieces; Windows behaviour unverified until the owner runs the list below)
- P3/P4: `host::retry_failed` + `host::RetryGate` (injected time, tested). main.rs keeps a
  `HOTKEY_RETRY` state, a 15 s timer (gate 1 min), and retries at once on Settings open and on
  `Callbacks::on_session_resume` (wake / unlock / remote + console connect, via
  `system_look::message_means_resume`, tested). Success clears the warning.
- P5: reserved-chord table in `chord.rs` (tested). `HotkeyRefusal::Reserved(Reserved)` in
  `with_hotkey` (tested) with four worded messages (FormTexts). Saved reserved chords reset to the
  legacy chord through `Settings::sanitize`.
- P6: `apply_hotkeys` frees every id, then registers all; swap test; FakeHotkeys refuses a chord
  another id holds, like the real table.
- P7: table returns `OwnAction`; `HotkeyFailure::OwnAction` with its own Settings row text and
  toast (`ToastMessage::HotkeyOwnAction`); tests.
- P8: `HotkeyBackend::probe` (platform thread via `HotkeyService::probe`, fake tested);
  `hotkey_health::alternatives(hotkeys, is_free)` skips chords other programs hold;
  `SettingsHost::free_alternatives` (SystemHost probes, cached until hotkeys are applied) used by
  Settings and the welcome tour.
- P9: `chord::set_layout_lookup`, `Chord::label[_with]` (German fake tested);
  `windows::hotkey::layout_char` (MapVirtualKeyExW) installed by `show_layout_labels()` in
  platform.rs `connect`; `key_chips`, `commands::chord_text`, `main_ctl` panel hotkey and
  `HotkeyProblem.chord` show the layout label.
- P10: claim `Ticket` per command (hotkey + tray), bounded join in hotkey and hidden_window.
- P11: `HotkeyError::Os(code)` -> `HotkeyFailure::Refused(code)` -> "Windows refused {} (Windows
  error {})." in Settings and the toast (`ToastMessage::HotkeyRefused`; tested).
- Strings: 8 new msgids in the .pot and six .po files; `i18n.py check --strict` and
  `test_i18n.py` pass.
- Workflow YAML: none needed.

## Next / blocked
- Nothing open. Note: 15 s retry timer and the probe use the real clock/OS only in the thin shell.

## Windows run list (all unverified)
- P3/P4: hold a chord in another program, start Snipdeck, close that program: warning clears within
  about 1 min, or at once after lock/unlock, sleep/wake, remote connect, or opening Settings.
- P5: try Ctrl+V, F5, Alt+F, Win+L in Settings: each is refused with its reason.
- P6: swap two shortcuts in Settings. P7: give two actions the same chord (own-action wording).
- P8: with another program holding Ctrl+Shift+G, it is not offered as an alternative.
- P9: German layout: Ctrl+Alt+Ü shows as typed in Settings, the tour and toasts.
- P10: a stuck tray/hotkey thread does not hang exit; a timed-out command does not run later.
- P11: force a Windows error (hard to do): the toast/row shows the error number.
