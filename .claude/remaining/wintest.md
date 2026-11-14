# wintest (Windows test wizard UX), branch claude/windows-test-ux

## Done
- `Test Snipdeck on Windows.bat` at the repo root replaces `run-windows-tests.bat` and `tools/windows-test/run-tests.bat`
  (Windows PowerShell 5.1, Bypass, window kept open, plain error + `spikes-results\wizard-log.txt`).
- `tools/windows-test/flow.ps1`: pure flow logic (plan, v1 mode, time left, progress bar, resume, counts, zip name,
  numbered actions, prerequisite wording); tested in `auto/tests/selftest.ps1` (`Invoke-FlowSelfTest`).
- `run-tests.ps1` rewritten flow: welcome, Enter = v1 check, Advanced options, resume question first, prerequisites with
  one "Install now? (Y/n)", up-front builds with a sign of life, one step layout, F asks what happened + screenshot,
  finish screen with counts, zip on the Desktop, Explorer select, clipboard summary; push to GitHub only under Advanced.
- `tests/*.json`: `friendly` names, `v1` flags, M3 steps 20-22 (scaling, file dialog, edge dock); key hints say "? then l".
- Docs: tools/windows-test/README.md quick start + Details, V1-PLAN, HANDOVER, development, troubleshooting, PLAN,
  CONTRIBUTING, CHANGELOG.

## Next
- Owner runs it on Windows 10/11; fix whatever the first real run shows (see "Windows-only assumptions" in the README).

## Blocked
- Anything Windows-only (winget installs, Start-Process build wait, Desktop zip, Explorer select, colours) is unverified.
