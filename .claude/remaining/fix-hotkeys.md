# fix-hotkeys (PLATFORM-3..11)

## Done
- P3/P4: `host::retry_failed` + `host::RetryGate` (injected time, tested); main.rs holds a
  `HOTKEY_RETRY` state, a 15 s timer (gate = 60 s), and retries at once on Settings open and on
  the new `Callbacks::on_session_resume` (wake / unlock / remote + console connect, via
  `system_look::message_means_resume`, tested). Success clears the warning. UNVERIFIED on Windows.
- P5: reserved-chord table in `chord.rs` (`Reserved`, tested). Existing saved reserved chords reset
  to the legacy chord through `Settings::sanitize`.
- P6: `apply_hotkeys` frees every id, then registers all; swap test; FakeHotkeys refuses OwnAction
  like the real table.
- P7: `HotkeyTable` returns `OwnAction`; `HotkeyFailure::OwnAction` mapped in platform.rs.
- P8 (platform side): `HotkeyTable::probe` / `HotkeyService::probe` on the hotkey thread (fake tested).
- P9 (platform side): `chord::set_layout_lookup`, `Chord::label[_with]` (German fake tested);
  `windows::hotkey::layout_char` (MapVirtualKeyExW) + `show_layout_labels`.
- P10: claim `Ticket` per command (hotkey + tray), bounded join in hotkey and hidden window.
- P11 (carrying): `HotkeyError::Os(code)` -> `HotkeyFailure::Refused(code)`.

## Next (not done)
- Call `snipdeck_platform::windows::hotkey::show_layout_labels()` at start-up (platform.rs) and use
  `Chord::label()` in key_chips / main_ctl / host `shown` (P9 app side).
- Add `HotkeyBackend::probe` and make `hotkey_health::alternatives` probe-aware (P8 app side).
- Distinct wording for `OwnAction` and "Windows error N" (P7, P11): FormTexts, ToastTexts,
  ToastMessage, .pot and the six .po files. Today OwnAction reads as "in use" and Refused as
  "unavailable".
- `HotkeyRefusal::Reserved(Reserved)` wording in settings_binding `with_hotkey` (P5 UI).
- Workflow YAML: none needed.

## Windows run list (all unverified)
- P3/P4: hold a chord in another program, start Snipdeck, close it, wait 60 s / lock-unlock / sleep-wake: warning clears.
- P6: swap two shortcuts in Settings. P7: same chord on two actions.
- P9: German layout, Ctrl+Alt+Ü shows as typed. P10: a stuck tray/hotkey thread does not hang exit.
