# fix-misc remaining (PLATFORM-15, 16, 19, 20, 21, 22, 35, 40 and the P33 app side)

## Done (code, unverified on Windows)
- P15: `crates/snipdeck-app/snipdeck.manifest` embedded by `build.rs`; `installer_check.py` rule and test.
- P16: `dll_search.rs` doc says what is true.
- P19/P20: `dwm_look.rs` (portable gates, tested), `windows/dwm.rs` (RtlGetVersion), title theme follows the app theme, round corners on the frameless windows. Decision in `.claude/now.md`.
- P21: accent from AccentColorMenu with a ColorizationColor fallback; text size follows through a registry watch.
- P22: `--help` and docs say `start /wait`; no `.com` stub (decision recorded).
- P35: `windows_subsystem` no longer depends on `debug_assertions`; feature `console` gives a console build.
- P40: `paint_diag` pushes `PaintRecord`s to a writer thread; formatting in portable `paint_record.rs`.
- P33: `store_data.rs`, `store_data_reminded` setting, `StoreDataReminder` toast (.pot + 6 .po), Back up now also exports to `Documents\Snipdeck backups` in the packaged build.

## Next / blocked
- Workflow edits are not made here. Give them to the coordinator:

P16, release job (assert the MSVC target; add after the build step):
```yaml
      - name: Assert the MSVC target
        shell: bash
        run: |
          rustc -vV | grep -q 'host: x86_64-pc-windows-msvc' || { echo "release must build for the MSVC target"; exit 1; }
```
P35: no workflow change is needed (every profile is now the GUI subsystem). For a console build: `cargo build -p snipdeck-app --locked --features console`.

- `python3 scripts/test_installer_check.py` has failures on the integration branch itself (`rust_mutex()` cannot find APP_ID in platform.rs after the single-instance move). Not caused here; the single-instance owner should fix it.
- `cargo clippy` for the Windows target fails on `windows/eyedrop.rs:588` (`used_underscore_binding` on `_bitmap`); pre-existing.

## Windows run list (owner)
1. P15: `mt.exe -inputresource:snipdeck.exe;#1 -out:m.xml` shows the manifest; a themed message box; open a file under a path over 260 characters.
2. P19: dark and light app theme against dark and light Windows: the title bar follows the app theme (Main, Settings windows).
3. P20: Windows 10 22H2 against Windows 11: Panel, Picker, fill form and toast corners, shadow, tray, hotkeys, autostart.
4. P21: the accent matches the Windows Settings swatch; the Text size slider in Windows changes Snipdeck without a restart.
5. P22: `start /wait snipdeck.exe --check-db` then `echo %ERRORLEVEL%`.
6. P35: a dev (debug) build starts with no console window.
7. P40: `SNIPDECK_DIAG=render` logs lines with `hook_at=` and the window still paints normally.
8. P16/installer/MSIX: a dummy `version.dll` in the current directory is not loaded; file dialogs still work with a shell extension pack.
9. P33: in the Store build the reminder toast shows once; Back up now writes to `Documents\Snipdeck backups`; the Setup build shows neither.
