# tray-icons remaining work (V1-PLAN step 9)

Done
- A. `scripts/make_logo.py` emits `assets/logo/snipdeck.ico` (16, 20, 24, 32, 48, 64, 128, 256; PNG entries; size-specific marks per L2) and `tray-16/20/24/32.png` (light card, dark hairline, blue tab; no tile). `crates/snipdeck-app/tests/app_icon.rs` parses both.
- B. `crates/snipdeck-app/build.rs` embeds the icon as resource 1 + version info through `winresource` (Windows targets only).
- C. The tray already loaded resource 1 with an `IDI_APPLICATION` fallback (`windows/tray.rs` `load_icon`); the exe had no resource, so it always fell back. Nothing more to change there.
- D. Panel re-show: `native_window::raise` after every `ShowPanel` (z-order, no activation); `SNIPDECK_LOG` traces the show result. Tray menu words come from `TrayTexts` (`@tr`, six locales) through `Tray::set_labels`.

Next
- Owner runs `tools/windows-test/tests/m3-app.json` (tray steps) on Windows and sends `app-*.err.txt` from a `SNIPDECK_LOG=1` run: look for `effect ShowPanel`, `show panel: Ok(()), visible now true`, `raise panel 0x...: true`.
- `--hidden` start never attaches the panel style (`attach_when_created` only runs for a visible start): attach on the first `ShowPanel`.
- Pick the UI language at start (`slint::select_bundled_translation`) and call `wording::tray_labels` again when it changes; today nothing selects a language, so the tray shows English.

Blocked
- No Windows machine and no `x86_64-pc-windows-gnu` target here: `windows/tray.rs`, `windows/window_style.rs` and the winresource build are not compiled in this session.
- `cargo deny` is not installed here.
