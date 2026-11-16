# fix-audit-extras (branch claude/fix-audit-extras)

## Done (committed unless noted)
- EXTRAS-1 language applied after first component: `language::apply`, `tests/language.rs`
- EXTRAS-2 `scripts/checks.sh` runs every script test + notices + halfbuilt (gate calls it). CI change: owner, below
- EXTRAS-3 notices + licence embedded in the exe (`notices.rs`), OFL text in THIRD_PARTY_NOTICES.md, `bundled_problems()` check
- EXTRAS-5, 10 i18n rules, glossary, 6 locales (scripts/i18n.py, scripts/glossary.json, test_i18n.py)
- EXTRAS-6 log file `logs/snipdeck.log` + `warn!`, report tail
- EXTRAS-7 (part) line tables kept, string panics keep std wording, early panics queued, anchor line
- EXTRAS-8 read-only store + `peek_settings`
- EXTRAS-9 dead loading state, dead Rust, dead tokens, Store row, `no-name` removed; limits fed from Rust (`binding::limit`); build date from SOURCE_DATE_EPOCH; `scripts/halfbuilt.py --check` in checks.sh
- EXTRAS-11 `--version`, `--backup`, `--` values, `--dark` keeps saving, bad args in a message box
- EXTRAS-13 (txt notices) / 14 (version resource CompanyName, LegalCopyright)

## Next
EXTRAS-4 (Rust-side English into *Texts globals), 12 (portable marker, trash retention, live language), 15 (fit per locale), 7 (SEH filter, unverified), not-reached files

## For the owner (workflow edits are not allowed for this worker)
- EXTRAS-2: add a CI job `bash scripts/checks.sh`; add `-p snipdeck-platform` and the app crate's non-window tests to the Windows job; fix the comment at ci.yml:57-58
- EXTRAS-3(c): remove `-ErrorAction SilentlyContinue` at release.yml:278
- EXTRAS-7: upload `snipdeck.pdb` as a release artifact (not in the installer); release profile now keeps line tables
- EXTRAS-9: set `SOURCE_DATE_EPOCH` (tagged commit time) in the release build so Help shows the build date
- EXTRAS-13: add a Pages workflow, or delete the claim in site/README.txt
- EXTRAS-14: pin the release toolchain to an exact version (release.yml:209) and enforce `rust-version`
- Windows-only code is unverified until run on Windows
