# launcher agent: V1-PLAN step 17 (prebuilt zip launcher)

## Done
- tools/launcher/release.ps1: asset resolver, pin-file parser, sidecar parser, SHA-256 compare, download, verify, unpack, swap, run, update.
- snipdeck.ps1 / cli.ps1: `run` and `demo` use the prebuilt app; `update` re-downloads; `build` is the source build (banner line); help text.
- data.ps1: git-pull `Invoke-Update` removed (replaced). build.ps1 / core.ps1 messages point to `Snipdeck.bat build`.
- selftest.ps1: new no-network checks (resolver on tests/release.json, pin parser, sidecar parser, SHA-256 compare, HTTP status, options). Existing checks kept.
- Docs: README quick start, docs/user-guide.md install section, docs/troubleshooting.md, tools/launcher/README.md, CHANGELOG Unreleased.

## Next (owner, on Windows; nothing here could be run on Linux)
- `powershell -NoProfile -ExecutionPolicy Bypass -File tools\launcher\snipdeck.ps1 selftest` and fix any FAIL.
- First real release: add its line to tools/launcher/release-sha256.txt; run `Snipdeck.bat` on a PC without the app folder.
- Measure the real zip size and set `$script:ApproxDownloadMb` in release.ps1.

## Blocked
- No pwsh/powershell on this machine: the PowerShell was only checked by reading and a brace/ASCII scan.
- No release exists yet, so the download path has never run. The release workflow zip holds snipdeck.exe + docs, not Snipdeck.bat (agents may not edit .github/workflows).
