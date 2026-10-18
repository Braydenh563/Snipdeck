# Snipdeck launcher

`Snipdeck.bat` (repo root) runs `tools/launcher/snipdeck.ps1` (Windows PowerShell 5.1 or 7). Run `Snipdeck.bat help` for the commands:
`run` (default), `demo`, `doctor`, `update`, `backup`, `restore`, `clean`, `uninstall`, `logs`, `help`. Options: `-Yes`, `--whatif`, `--rebuild`.

- Logs: `%LOCALAPPDATA%\Snipdeck\logs` (newest 10). Backups: `Documents\Snipdeck-backups` (newest 10 per kind). Fast-start copy: `dist\`.
- Debug build only (a release build needs the production licence key). Known cargo failures and their fixes are in `lib.ps1` (`Get-FailureTable`); sample logs and expected matches are in `tests/`.
- Files: `lib.ps1` pure helpers, `cli.ps1` arguments, `core.ps1` console/logging/processes/checks, `build.ps1` stamp/build/cache/launch, `data.ps1` backup/restore/uninstall/doctor/update, `selftest.ps1` (`snipdeck.ps1 selftest`).

## Status: written, never run

This code has NOT been executed by anyone: not on Windows, and not on PowerShell 7 either (the build session could not run `pwsh`).
Only the failure-pattern table was checked (with Python `re`, against `tests/*.log`).
First thing to do: `pwsh -NoProfile -File tools/launcher/snipdeck.ps1 selftest` and fix whatever fails, then a real run on Windows.

Unfinished or untested: selftest is reduced (no temp-repo stamp test, no fake-cargo build-loop test, no backup/restore round trip, no Remove-KnownPath guard test);
`SNIPDECK_FAKE_CARGO` and `BackoffScale` hooks exist for those tests. Docs (README quick start, troubleshooting, user guide) were not updated.
