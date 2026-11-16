# fix-audit-platform: consolidated remaining work (PLATFORM-1..41)

Everything Windows-only is **unverified**: nothing here ran on Windows. Verified on Linux: fmt, native clippy, `cargo check --workspace --all-targets --target x86_64-pc-windows-gnu`, clippy of snipdeck-platform on that target, portable tests, i18n strict, installer/MSIX python tests.

Owner actions: (1) apply the workflow YAML below (agents may not edit `.github/workflows`); (2) run each section's Windows list.
Not done by design: P22 ships no `snipdeck.com` console stub (needs a second binary in Setup, MSIX and the release workflow; `start /wait` documented in `--help` and docs instead, per the audit's alternative). P33 Store-data rule is documented in docs/packaging.md and the reminder + Documents backup copy are built. P20 keeps the Inno/MSIX minimum version (decision in `.claude/now.md`).

Each section below is one worker's file, kept verbatim.

---

<!-- fix-single-instance -->
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

---

<!-- fix-tray-autostart -->
# fix-tray-autostart

## Done
- P12 three-state autostart (Off / On / SwitchedOffInWindows via StartupApproved\Run), stale Run path rewrite at start-up, enable() clears the Windows-disabled record.
- P14 async StartupTask (request_startup_task, cached status); DisabledByUser and DisabledByPolicy map to their own toast and Settings text.
- P17 file dialogs on a dedicated STA thread with the owner handle; answers return to the UI thread via ui_thread::back_on_ui. All app callers converted (settings import/export, editor Browse, icon Save as, test fakes).
- P18 DONTADDTORECENT flags built in one place (portable, tested; cfg(windows) test asserts equality with the windows crate constants).
- P23 AddRetry back-off state machine (injected ticks), shared by start-up and TaskbarCreated.
- P24 icon reload plus NIM_MODIFY on WM_DPICHANGED and TaskbarCreated.
- P25 NIN_KEYSELECT debounce, and a Settings toggle "Double-click opens the Main window" (setting `tray_double_click_opens`, default on; off makes a single click act at once). Strings in .pot and all six .po (i18n check --strict passes).
- Verified natively: snipdeck-app lib (1295), settings_window, editor_controller, main_window tests pass; platform Windows-target check passed earlier.

## Next / blocked
- Clippy and scripts/gate.sh --changed run last (see report).
- Workspace Windows check is impossible here (no mingw gcc); app cfg(windows) code is unverified by compile.

## Windows run list (all unverified)
- Tray: 10 sign-ins with autostart on; restart explorer.exe 5x; scale 100% to 200%; Win+B then Enter/Space opens the Panel once; with "Double-click opens" off a single click acts at once; with it on a double click opens Main and a single click still works after the double-click time.
- Autostart: disable in Task Manager, reopen Settings (not plain "on"); move the portable folder and start it (Run value repoints); toggling on in Snipdeck clears the Windows-disabled record.
- MSIX: consent prompt; disable in Settings > Apps > Startup then toggle (toast explains); sign in again, app starts hidden.
- Dialogs: clipboard auto-clear on time with Export open; hotkey works with dialog open; drag the dialog over the owner (no unpainted areas); exported file not in shell:recent.

---

<!-- fix-hotkeys -->
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

---

<!-- fix-misc -->
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

---

<!-- fix-packaging -->
# fix-packaging (branch fix-packaging)

## Done (code and docs in this branch)
- P13 StartupApproved\Run removed at uninstall (Inno `uninsdeletevalue`, launcher `Get-RegistryLeftovers`); one list in `packaging/registration.json`, checked against the .iss and the launcher by `scripts/installer_check.py`.
- P29 packaging side: `.snipdeck` + `snipdeck:` in the .iss (HKCU, uninsdeletekey) and in AppxManifest (FileTypeAssociation, Protocol, AppExecutionAlias), from the one table. App hand-over code is another worker's.
- P30 every [CustomMessages] key in all six languages, checked.
- P31 data folder recorded at `HKCU\Software\Snipdeck\DataDir` (`snipdeck-platform` data_record, portable test); erased at "erase my data"; DelTree skipped on empty env var; SafeToErase guard.
- P32 downgrade warning in InitializeSetup (silent run answers No); relaunch with `--hidden` if it was running.
- P33 decided and documented in docs/packaging.md ("Where the Store build keeps your data"); no code.
- P34 full target-size icon set (`make_msix_assets.py`), makepri step in docs and checked.
- P37 launcher `Stop-AppProcesses`: `--quit`, wait for mutex, kill as fallback, match by full exe path (selftest cases added).
- P38 `spikes/s2-ime/--cjk` removed; gate step `dash-names`.
- P39 `scripts/check_release_pin.py` (+ tests, in gate) and the release-recipe docs.
- P36/P41 manifest `--arch`, Inno `/DArm64=1` (code); YAML below is for the owner (I may not edit .github/workflows).

## Next / blocked
- Owner applies the YAML below to `.github/workflows`.
- App side (not mine): one-time "export before uninstall" reminder in the packaged build and a default backup folder in Documents (UI strings need .pot + six .po).

## YAML: P36 toolchain pin and Windows GUI smoke step (CI, windows-latest job)
```yaml
      - uses: dtolnay/rust-toolchain@master
        with:
          toolchain: 1.99.1        # same version as rust-toolchain.toml; bump both together
          components: clippy, rustfmt
      - name: Build release exe
        run: cargo build --release --locked -p snipdeck-app
      - name: GUI smoke (start hidden, wait for mutex, quit, clean log)
        shell: pwsh
        run: |
          $exe = "target/release/snipdeck.exe"
          $data = Join-Path $env:RUNNER_TEMP "smoke-data"
          $p = Start-Process $exe -ArgumentList @("--hidden","--data-dir",$data) -PassThru
          $m = $null; $ok = $false
          for ($i = 0; $i -lt 60 -and -not $ok; $i++) {
            $ok = [System.Threading.Mutex]::TryOpenExisting("Snipdeck-main", [ref]$m)
            if (-not $ok) { Start-Sleep -Milliseconds 500 }
          }
          if (-not $ok) { throw "main mutex never appeared" }
          $q = Start-Process $exe -ArgumentList @("--quit","--data-dir",$data) -Wait -PassThru
          if ($q.ExitCode -ne 0) { throw "--quit exited $($q.ExitCode)" }
          if (-not $p.WaitForExit(15000)) { $p.Kill(); throw "app did not exit after --quit" }
          if ($p.ExitCode -ne 0) { throw "app exit code $($p.ExitCode)" }
          $log = Get-ChildItem $data -Filter *.log -Recurse -ErrorAction SilentlyContinue
          if ($log -and (Select-String -Path $log.FullName -Pattern "ERROR|panicked" -Quiet)) { throw "log has errors" }
```
(Mutex name must match `Snipdeck-main`; installer_check keeps it in step with the Rust code. Unverified on a runner.)

## YAML: P41 aarch64 check job and ARM64 release artifacts
```yaml
  check-arm64:
    runs-on: windows-11-arm        # or windows-latest with the target added and cargo check only
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master
        with: { toolchain: 1.99.1, targets: aarch64-pc-windows-msvc }
      - run: cargo check --workspace --locked --target aarch64-pc-windows-msvc
```
Release matrix (release.yml), per arch `x64` -> `x86_64-pc-windows-msvc`, `arm64` -> `aarch64-pc-windows-msvc`:
```yaml
    strategy:
      matrix:
        include:
          - { arch: x64,   target: x86_64-pc-windows-msvc,  iscc_define: "" }
          - { arch: arm64, target: aarch64-pc-windows-msvc, iscc_define: "/DArm64=1" }
    steps:
      - run: cargo build --release --locked -p snipdeck-app --target ${{ matrix.target }}
      - run: python scripts/msix_manifest.py --arch ${{ matrix.arch }} ...   # fill manifest, see packaging/msix/README.md
      - run: iscc ${{ matrix.iscc_define }} /DAppVersion=${{ env.VERSION }} packaging\inno\snipdeck.iss
      # zip name: snipdeck-v<version>-windows-${{ matrix.arch }}.zip ; Setup name gets the arch too
```
Unverified: whether `windows-11-arm` runners are available to the repo; cross-compiling `aarch64` from x64 needs the MSVC ARM64 build tools (preinstalled on windows-latest).

## YAML: P34 makepri in the MSIX job (after staging, before pack)
```yaml
      - name: Build resources.pri
        shell: pwsh
        run: |
          $sdk = (Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\makepri.exe" | Sort-Object FullName | Select-Object -Last 1).FullName
          & $sdk createconfig /cf dist\priconfig.xml /dq en-US /o
          & $sdk new /pr dist\msix-stage /cf dist\priconfig.xml /mn dist\msix-stage\AppxManifest.xml /of dist\msix-stage\resources.pri /o
```

## YAML: P39 pin step (release.yml, after the upload job, needs contents: write)
```yaml
  pin:
    needs: release
    runs-on: ubuntu-latest
    permissions: { contents: write }
    steps:
      - uses: actions/checkout@v4
        with: { ref: main }
      - run: gh release download "$TAG" --pattern "snipdeck-*-windows-*.zip" --dir zips
        env: { GH_TOKEN: "${{ github.token }}", TAG: "${{ github.ref_name }}" }
      - run: |
          python3 scripts/check_release_pin.py add --tag "$TAG" $(for z in zips/*.zip; do printf -- '--zip %s ' "$z"; done)
          python3 scripts/check_release_pin.py check --tag "$TAG" --arch x64 --arch arm64 $(for z in zips/*.zip; do printf -- '--zip %s ' "$z"; done)
          git config user.name "github-actions"; git config user.email "actions@users.noreply.github.com"
          git add tools/launcher/release-sha256.txt && git commit -m "Pin $TAG" && git push
        env: { TAG: "${{ github.ref_name }}" }
```
(Drop `--arch arm64` until the ARM64 zip ships. If branch protection blocks the push, open a PR instead.)

## Windows run list (all unverified until the owner runs them)
- P13: install, enable Start with Windows, toggle it off/on in Task Manager > Startup, uninstall; `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run` has no Snipdeck value. Launcher: `Snipdeck.bat uninstall` same.
- P29: Setup: double-click a `.snipdeck`, open `snipdeck:` link; uninstall removes `HKCU\Software\Classes\.snipdeck`, `Snipdeck.Archive`, `snipdeck`. MSIX: association, scheme, `snipdeck` alias.
- P30: Setup in de/es/fr/ja/pt-BR: no English sentence.
- P31: start with `--data-dir D:\X`, `reg query HKCU\Software\Snipdeck` shows DataDir; uninstall, answer Yes: both folders gone; with USERPROFILE/TEMP unset nothing at drive root.
- P32: older Setup over newer: warning, silent run refuses; upgrade while hidden in tray: returns hidden.
- P33: Store data location and what survives uninstall.
- P34: `makepri new` succeeds, icons sharp at 100/150/200 percent.
- P36/P41: CI YAML above; ARM64 device: exe starts, hotkey, paste, tray.
- P37: launcher `update`/`uninstall` with the app running: quits via `--quit`, no stray kill; `selftest.ps1` new cases pass.

## Review round B (branch fix-review-b): Windows run list, all unverified
- Argument injection: from a command prompt run `snipdeck.exe snipdeck://open --data-dir C:\x` and `snipdeck.exe --hidden snipdeck://panel`: the app must print/show "cannot be combined with other options" and start nothing. `snipdeck.exe snipdeck://settings` and double-clicking a `.snipdeck` file still work; check the Setup and MSIX registered open commands pass only `"%1"` (no extra flag).
- Session end: with the app open and a settings change within the last second (e.g. toggle a switch, then immediately Sign out / Restart): after sign-in the setting survived; the log has no repeated "flush skipped" lines beyond a few.
- Store build (MSIX): `Back up now` and wait for the daily backup (or delete `backup-state` and restart): `Documents\Snipdeck backups` holds `snipdeck-backup-YYYYMMDD.snipdeck` for both; only one toast shows; the folder never holds more files than "Backups kept"; break the write (read-only folder) and the toast still says the backup was made; uninstall the MSIX and the Documents copies remain.
