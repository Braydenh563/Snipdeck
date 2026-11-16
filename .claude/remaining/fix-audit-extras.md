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

## Added this stretch
- EXTRAS-4 (part): doctor labels and details are translated (`DoctorTexts`, `doctor::Finding`). Still open: `starter.rs`, `wording.rs`, `commands.rs`, start-up failure reason
- EXTRAS-12 (part): live language change (`language::apply`), `portable` marker file beside the exe (`paths::portable_dir`). Still open: Trash retention setting
- Open: EXTRAS-7 SEH filter (unverified), EXTRAS-15 fit-per-locale test, not-reached file audit

## Deferred until the EXTRAS-4 agent has finished (shares the lang files)
- Stale tip count in strings: onboarding.slint:422 "Ten short tips" (11 cards now): drop the number from the English, .pot and 6 .po; starter card "More tips" says "ten short cards" (agent is rewriting starter.rs: check)
- CHANGELOG.md:98 says ten Tips cards (history entry, leave)
- tips pictures: `scripts/tips_images.py --check` is now in checks.sh; regenerate pictures (python3 scripts/tips_images.py) AFTER the last golden change, before the final gate
- OWNER (CI): the scripts job needs `pip install pillow` for the tips-images check
- palette.py --check now runs in checks.sh (was in no gate or CI job)
- make_logo.py: no --check; needs cairosvg + fontTools (not installed here), so the generated assets/logo files are not verified against it. Owner: add a --check and `pip install cairosvg fonttools` in CI, or accept
- readme_shots.py: now exits 1 when renders are missing
- EXTRAS-7 done in code: `windows::exception` filter (unverified on Windows; compiles for windows-gnu), `crash::native_report`/`on_native_exception`, maintenance checks for a crash report on every tick. Needs wording follow-up in the lang batch: toast "Snipdeck closed unexpectedly last time. Open the log?" is wrong for a mid-session panic -> "Snipdeck ran into a problem. Open the log?" (toast_texts.slint:248, .pot + 6 .po)

## Update (latest batch)
- EXTRAS-4 done by the agent (NameTexts, StarterTexts, key words, first-run content after language::apply, structural library_is_untouched, 40 strings x 6 locales). Left English, noted: shortcuts.rs Custom/Plain `spoken` consts; main.rs "creating the Panel: {e}" (no window exists to translate it).
- EXTRAS-12 done: `trash_days` setting (1..=365, default 30) in Settings > Backups; purge, window texts, editor delete dialogs use it (`Jobs::purge_trash` lost its cutoff). Tests: settings `trash_days_default_to_thirty...`, `the_trash_keeps_things_for_the_days_the_setting_says`, `days_in_the_trash_is_held_between_one_and_365_and_shown`, editors_logic delete dialog. The gallery mock (`gallery/inputs.slint`) still shows "30 days" as sample text.
- Stale strings fixed: "Short tips", "Snipdeck ran into a problem. Open the log?" (+ docs/troubleshooting.md), Tips card text.
- EXTRAS-7 done: native SEH filter (unverified on Windows) + crash check every maintenance tick (every 6 h).
- EXTRAS-15 done: `tests/locale_fit.rs` (`a_language_does_not_push_more_past_the_window_edge_than_english_does`). It found and fixed: Help > Tools buttons ran past the window in es/fr/pt-BR/ja (now stacked). Open (English too): Main window toolbar at 2.25x text in a 480 px window runs past the edge; skipped in the test until branch fix/narrow-toolbar lands.
- Not audited in depth: main.rs lines 260-870.
