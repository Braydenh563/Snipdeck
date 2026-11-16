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
