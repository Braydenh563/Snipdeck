# Snipdeck launcher

`Snipdeck.bat` (repo root) runs `tools/launcher/snipdeck.ps1` (Windows PowerShell 5.1 or 7). Run `Snipdeck.bat help` for the commands:
`run` (default), `demo`, `update`, `build`, `selftest`, `doctor`, `backup`, `restore`, `clean`, `uninstall`, `logs`, `help`. Options: `-Yes`, `--whatif`, `--rebuild` (build only).

## Run (users)

`Snipdeck.bat` with no arguments starts `app\snipdeck.exe` next to the `.bat`. It never checks for Rust, Visual Studio, git or crates.io.

1. `app\snipdeck.exe` exists: start it (the app itself handles a second start and keeps the library in `%LOCALAPPDATA%\Snipdeck`).
2. It does not: ask GitHub for the latest release (`https://api.github.com/repos/Braydenh563/Snipdeck/releases/latest`), pick the asset named
   `snipdeck-<tag>-windows-x64.zip` and its `.sha256` sidecar (written by `.github/workflows/release-build.yml`), download both, and check the
   zip's SHA-256 against the sidecar and, when there is one, against the line for that asset in `release-sha256.txt`.
3. On a match, unpack with `Expand-Archive` to `app.new\`, swap it in as `app\`, and start it. On any mismatch or error the download is deleted and nothing is run.

`Snipdeck.bat update` does step 2 and 3 even when `app\` exists (it refuses while the app is running; quit it from the tray first), then starts it. The library and settings are not in `app\`, so they are kept.

`release-sha256.txt` format, one line per released version: `<sha256>  <asset-name>` (`#` comments allowed). A release with no line is checked against its published `.sha256` only, and the launcher says so in one line. After publishing a release, add its line (the hash is in the release's `.sha256` file) and commit.

Messages: no network ("needs the internet once ... Connect and run Snipdeck.bat again"), no release yet (HTTP 404), GitHub rate limit (403/429), hash mismatch (refuses, deletes the download), unreadable or unsafe zip, folder cannot be replaced.

## Uninstall and reinstall

`Snipdeck.bat uninstall` removes the `app\` folder (and an unfinished `app.new\`/`app.old\`, leftover download folders in Temp, `dist\`), the build folder if asked, the start-with-Windows entry and a Start-menu shortcut. It offers a backup first. Your library in `%LOCALAPPDATA%\Snipdeck` is **kept** unless you add `--data` and type `DELETE` (with `-Yes`, `--data` erases without asking). Reinstalling is just running `Snipdeck.bat` again: it downloads the app again and finds the kept library. It is safe to repeat.

## Build (developers)

`Snipdeck.bat build` is the source build with all its checks (Rust, MSVC target, Visual C++ tools, git, crates.io), the fast-start copy in `dist\` and the known-failure repairs. It prints "Developer build from source. Users do not need this: run Snipdeck.bat without arguments." There is no launcher command for `git pull`: pull yourself, then `Snipdeck.bat build`.

- Logs: `%LOCALAPPDATA%\Snipdeck\logs` (newest 10). Backups: `Documents\Snipdeck-backups` (newest 10 per kind).
- Debug build only (a release build needs the production licence key). Known cargo failures and their fixes are in `lib.ps1` (`Get-FailureTable`); sample logs and expected matches are in `tests/`.
- Files: `lib.ps1` pure helpers, `cli.ps1` arguments, `core.ps1` console/logging/processes/checks, `release.ps1` download/verify/unpack/run/update, `build.ps1` stamp/build/cache/launch, `data.ps1` backup/restore/uninstall/doctor, `selftest.ps1` (`snipdeck.ps1 selftest`).
- Tests without a network: `tests/release.json` (a saved GitHub "latest release" response) feeds the asset-name resolver check; the pin-file parser and the SHA-256 compare run on text and a temp file.

## Status: written, never run

This code has NOT been executed by anyone: not on Windows, and not on PowerShell 7 either (the build and the release-download sessions could not run `pwsh`).
Only the failure-pattern table was checked (with Python `re`, against `tests/*.log`). The release path has never contacted GitHub: no release exists yet.
First thing to do: `powershell -NoProfile -ExecutionPolicy Bypass -File tools\launcher\snipdeck.ps1 selftest` and fix whatever fails, then a real run on Windows.

Unfinished or untested: selftest has no temp-repo stamp test, no fake-cargo build-loop test, no backup/restore round trip, no Remove-KnownPath guard test, and no test of the download/unpack/swap steps (they need a network or a local fake server);
`SNIPDECK_FAKE_CARGO` and `BackoffScale` hooks exist for the build tests. The size shown in the "needs the internet" message (`$script:ApproxDownloadMb` in `release.ps1`) is a guess until a real release zip exists.
