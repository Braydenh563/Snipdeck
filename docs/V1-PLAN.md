# V1 ship list (2026-10-19, authoritative)

Everything that must be done before `v1.0.0`, in order. Owner scope decision 2026-10-19: everything planned ships
except screen capture/OCR, an update check and an activation server. Status: `[x]` done, `[~]` in progress,
`[ ]` open, `[owner]` only the owner can do it. The waves below this list are history; where they disagree, this
list wins. Rungs as below (H Haiku, S Sonnet, O Opus, F the one Fable pass).

## A. In flight (wave 2.5)
- [~] A1 [S] Review fixes: Panel list editor without Main, Space reveal in Panel, drop `OpenOnboarding`, hash the sensitive clipboard text (wt-review).
- [~] A2 [S] Windows UI: menus sized to content, focus rings inset, `place_near` before show, "New list N" prefilled and selected, Open Panel button, Panel steps down for Snipdeck windows, dialogs over their owner, Settings fits the work area at 150 % (wt-windows-ui).
- [~] A3 [S] Learnability: Ctrl+K command palette (toolbar, Panel kebab, tray), shortcuts sheet (Ctrl+/ and F1 tab), shortcut hints in menus and tooltips, footer and empty-state hints (wt-learn).
- [x] A4 CI fixes for clippy 1.99 and tests (fill_window sensitive template, 051ece4).

## B. Wave 3: finish the features (three S agents)
- [ ] B1 [S] Import: Settings "Import…" for `.snipdeck`, Ditto, CopyQ, folder of .txt/.md; preview of what it adds (also for Restore); Free board limit respected.
- [ ] B2 [S] Windows clipboard-history importer (current session; Windows-only, wizard step).
- [ ] B3 [S] Export a list as text.
- [ ] B4 [S] Ctrl+Z undoes the last undoable action; Undo toast >= 5 s and pauses on hover.
- [ ] B5 [S] "Getting started" list seeded on first run (`starter.rs`).
- [ ] B6 [S] Duplicate item.
- [ ] B7 [S] Hover/focus "More" (⋯) button on rows and cards = the context menu.
- [ ] B8 [S] Shortcuts: Ctrl+N, Ctrl+Shift+N, Ctrl+F, Ctrl+D, Ctrl+W, Ctrl+, (where not already in A3).
- [ ] B9 [S] Tray double-click opens the Main window.
- [ ] B10 [S] Reduced-motion setting (and follow Windows "Animation effects").
- [ ] B11 [S] Drag and drop: reorder items and lists; drop items on a list tab to move; drop files/text on the Panel to create.
- [ ] B12 [S] Perf: cached per-board index on the store change counter; budget tests (search < 16 ms / 10k, Panel load < 50 ms); release size and idle RAM printed by the Windows build.
- [ ] B13 [S] CLI: `--check-db`, `--troubleshoot`.
- [ ] B14 [S] Sensitive clipboard clear: keep the copy-only clear, add the setting (30 s, 0 = off); SPEC 5.7 updated to match.
- [ ] B15 [S] Wording and a11y leftovers: Help glyph, "Delete for good" everywhere, list-bullets icon, live regions on status texts, HC selected-state tokens, tooltips on every icon button and rail item, German segmented widths, menu Tab dismisses, Pin/Unpin label, Move to Trash button variant, radio arrow keys, dock handle hit area, tab tooltips, Panel row focus ring (`.claude/remaining/ux-audit-2026-10-19.md`).

## C. Wave 4: the deferred features, now in v1
- [ ] C1 [S] Recent inbox (SPEC 5.6): opt-in, off by default, last 200 entries or 7 days, excludes sensitive/password sources and Windows "exclude from history" formats; Promote to a list; clock tab in the Panel; sidebar entry.
- [ ] C2 [S] Paste stack (Pro, SPEC 5.1): multi-select, Start stack, queue shown in the Panel, Paste next advances.
- [ ] C3 [S] Master password (Pro, SPEC 5.7): Argon2id, unlock per session and idle timeout, change and remove, recovery warning.
- [ ] C4 [S] Item colour label (8 DESIGN swatches, 4 px bar, filter chip) and list icon (curated set).
- [ ] C5 [S] Accent colour: Index Blue default; presets teal, green, amber, rose, violet; "Windows accent" option nudged to pass contrast; every token pair measured for WCAG 2.2 AA in a test; full high-contrast use; 225 % icon scaling.
- [ ] C6 [S] Edge dock ships (built: dock.rs, dock_ctl.rs): settings checked, wizard steps present.
- [ ] C7 [S] MSIX packaging files and `docs/store.md` steps (owner buys the account); code-signing steps for SignPath in the release docs.
- [ ] C8 [S] Get Pro: mailto link in Settings, the board-limit toast and the site; address is a placeholder until the owner supplies it.

- [ ] C9 [S] Free vs Pro made clear (owner 2026-10-19): a compact comparison table (feature, one-line description, tick or cross per tier) in Settings Licence and the Get Pro dialog, the README and the site. Honest framing: Pro pays for convenience and supports the app; the code is GPL; Free stays fully usable. No walls of text.
## D. Images, site, controller
- [ ] D1 [S] Regenerate every golden after A-C, read each one; UX fixes from what they show.
- [ ] D2 [S] README shots and hero images at 2x (`SNIPDECK_SHOT_SCALE=2`, `scripts/readme_shots.py`); Help shot back in.
- [ ] D3 [S] De-slop pass over every window state against DESIGN.md (13a).
- [ ] D4 [S] GitHub Pages site: self-hosted assets, no CDN, DESIGN.md tokens, WCAG 2.2 AA, download, tour, Pro, privacy.
- [ ] D5 [S] Controller split (H1): `controller.rs` under 800 lines, test list identical before and after (may be done inside F1).

## E. Docs and going public
- [ ] E1 [H] Docs: user guide matches every menu, shortcut and new feature; CHANGELOG; troubleshooting; privacy (Recent inbox); licensing (Get Pro); HANDOVER rewritten to the shipped state; CODEMAP regenerated.
- [ ] E2 [S] Pre-public sweep (`docs/going-public.md` section 1): full-history secret scan, review `docs/` and `.claude/` for private notes, read licensing and SECURITY as a stranger.
- [ ] E3 [owner] Make the repo public; GitHub settings in `docs/going-public.md` sections 2-3 (secret scanning, Dependabot, branch protection, tag ruleset).

## F. Verification
- [ ] F1 [F] Fable critical sweep (`docs/fable-brief.md`): architecture, complexity, frontend, backend, UI/UX, efficiency, security, bugs; ranked findings, then fixes.
- [ ] F2 [O] Independent review of the whole diff since `main`; fixes by S.
- [ ] F3 [S] ux-auditor and blind-tester runs on the final build; fixes.
- [ ] F4 [S] `scripts/gate.sh --full` on stable and 1.99.1, Windows target check, `cargo deny`; CI green (needs E3 for free runners); PR #5 body with the gate lines; morning summary; retro.

## G. Owner on Windows and release
- [ ] G1 [owner] Windows run: `Snipdeck.bat` on a clean PC, `run-windows-tests.bat` (M2, M3 and new steps), tray, taskbar, Alt+Tab, placement at 100/150/225 %, file dialog, caret, DPAPI, autostart, dock hover, IME (S2).
- [ ] G2 [owner] Supply the Get Pro address; review and merge PR #5.
- [ ] G3 [owner] Tag `v1.0.0`; the release workflow builds the zip; add its SHA-256 to `tools/launcher/release-sha256.txt`.
- [ ] G4 [owner] After release: apply to SignPath Foundation; Microsoft Store submission (optional).

Not in v1 (owner, 2026-10-19): screen capture and OCR (SPEC 5.5), an update check (breaks the offline promise), an activation server.

---

# V1 plan: waves and steps (Fable, 2026-10-18)

Owner's order: finish v1 with nothing left undone; lightweight, robust, instinctive. This file is the work order: an
agent is briefed with a step number and reads the step; it does not re-plan. Rungs: **H** Haiku (mechanical, runnable
check), **S** Sonnet high (implementation), **O** Opus medium (review, design-level bugs). Every step: tests first,
never weaken a test, `scripts/gate.sh --changed` clean on stable and `+1.99.1`, commit every 20 min, report ≤30 lines.
Findings referenced as `C1`, `H1`, `U3`… are in `docs/reviews/fable-analysis.md`. Windows-only behaviour stays
"unverified" until the owner runs `run-windows-tests.bat`; every Windows step adds a wizard step in `tools/windows-test/tests/`.

## Wave 1 (running): foundations
1. **[done]** Merge `claude/snipdeck-main-window`, gate, pin icon, Rust 1.99 lints, keygen tool, GitHub workflows, release profiles, privacy doc.
2. **[done, merged e6c4905; not done: ItemEditor (step 8), TagManager, SettingsPage (step 7), Recent inbox; move has no undo]** Main window wiring: `snipdeck-app/src/main_window.rs` (MainModel), `main_binding.rs`, `Effect::OpenMainWindow` creates/raises the window; Trash + 5 s undo (`TrashRepo::purge_entry`), list/board add/rename/delete/reorder via ListEditor/BoardEditor; `tests/main_window.rs`. Accept: every ItemAction has a test; Free cap of 3 boards enforced; undo restores within 5 s on the frozen clock.
3. **[done, merged 21c0870]** Panel UI: kebab (`app-menu-*`), `resize-requested(ResizeEdge)`, `edit/rename/delete-requested(int)`, pinned-strip wheel + carets, tooltips on every icon. Accept: a11y-tree tests + goldens; `scripts/i18n.py extract` run.

## Wave 2: bind, windows, maintenance (after wave 1 merges; three agents, separate files)
4. **[S, running]** App binding of the new Panel API (`binding.rs`, `controller/panel.rs`): kebab entries Open Snipdeck / New snippet / Settings / Pause hotkeys / Help / About / Quit; `resize-requested` → `winit` `drag_resize_window` with min 260 / max 480 width (DESIGN §5), size and position remembered per monitor in settings (H2); `edit-requested` → ItemEditor window on the item (quick edit, owner 8b); `rename-requested` → inline title edit; `delete-requested` → trash + undo toast (reuse step 2). Tests in `tests/panel_window.rs` with real pointer events. Wizard: M3 steps for resize, kebab, quick edit.
5. **[S] Picker + TemplateFill windows** (C5, PR4 #13): `binding/picker.rs`, `binding/fill.rs` render `View.picker` / `View.fill` into `PickerWindow` / `TemplateFillWindow`; windows created lazily, non-activating like the Panel, placed at the caret (SPEC 5.2: below caret, flip above, fallback mouse; `Target` already carries the caret rect or `None`); register their HWNDs with the target tracker; Esc restores focus; toasts show while the Panel is hidden. Tests: `tests/picker_window.rs`, `tests/fill_window.rs` (open from hotkey, type, Enter pastes, Shift+Enter copies, Esc closes, prompted template → fill → paste). Wizard steps.
6. **[S, running]** Maintenance + crash log (C1, C4, S5): `maintenance.rs` per the design in fable-analysis "Architecture redesign": run 2 s after first show and every 6 h on the injected clock: `backup::run_if_due` (keep 7, encrypted when the library has a key), `TrashRepo::purge(now - 30 d)`, weekly `integrity_check` → danger toast; `crash.rs`: `std::panic::set_hook` writes `logs/crash-<date>.txt` (message, location, version, no item content), rotates 5; About ▸ "Report a problem" opens the log folder and a prefilled GitHub issue URL. Tests with the frozen clock: backup day 1 / none at +1 h / rotation at 8 / purge boundary at exactly 30 d / a panic in a test thread produces one file.
7. **[S] Settings window** (`binding/settings.rs`): bind `SettingsPage` to `Settings` + saver: hotkeys with conflict detection (reuse `HotkeyProblem`), theme, language, autostart, close-to-tray, site icons, panel width, no-paste apps; "Privacy" section (S2 text) and "Backups" section (list + Restore + "Open data folder"); licence entry (C3): paste a `SNIP1.` key → `verify_licence` → tier shown; `--demo` stays Pro. Tests: each toggle writes settings.json after the quiet time; a bad key is rejected with a specific toast; restore round-trips the demo library. Also a "Data" section (owner 2026-10-18): Open data folder, Back up now, Restore…, Export library…, Erase library… (typed DELETE, closes the store, deletes the data folder contents except the key file question: ask "also forget the encryption key?", then restarts onboarding).
7a. **[S] Onboarding** (SPEC 5.7b; owner 2026-10-18: "wizard setup?"): `onboarding.slint` + `binding/onboarding.rs`, one window, 4 steps ≤ 60 s, Skip on every step: 1 Welcome (what the Panel, Picker and hotkeys do, one picture each) · 2 Hotkeys (recorder with conflict detection, defaults prefilled) · 3 Panel position (left/right edge, keep on top) · 4 Start with Windows + clipboard history opt-in. Finishing seeds the Getting started list (step 13 `starter.rs`) and sets `settings.onboarded`; shown on first run of an empty library, and from Help ▸ "Show the welcome tour again". Tests: logic tests per step, a11y tree, one golden per step light/dark. Accept: a new user reaches a working Panel without reading docs (blind-tester run).
7b. **[S] Help & troubleshooting window** (owner 2026-10-18: "troubleshooting wizard GUI for the launcher tools, from the Panel and app"). One window `troubleshoot.slint` + `binding/troubleshoot.rs`, opened from the Panel kebab ▸ Help ▸ "Troubleshoot…", the Main window Help menu, `--troubleshoot` on the command line, and offered automatically by the crash-log toast (step 6). Not a multi-step wizard: a checklist with a Fix button per failed row, because every check is independent; the only stepped flow is Restore (pick a backup ▸ preview counts ▸ confirm). Rows (each a pure Rust check in `snipdeck-app/src/doctor.rs`, testable with fakes, run again on "Check again"): app version + build date; data folder path, free space, writable; database opens, `integrity_check` ok, FTS index consistent (Fix = reindex); blob store orphan count (Fix = sweep); backups: newest date, count, Fix = Back up now; encryption key present when sensitive items exist (DPAPI; Fix = explain + Settings link); hotkeys registered, conflicts named (Fix = open Settings ▸ Hotkeys); tray icon present; single-instance lock holder; autostart entry matches the setting; last crash log (Open, Copy). Buttons: Open data folder, Open logs folder, Copy diagnostic report (versions, OS build, DPI, settings flags, check results, last crash tail; never snippet content or paths outside the data folder), Check for updates (spawns `Snipdeck.bat update` from the folder above `app\` and quits; greyed when the bat is absent), Uninstall… (same, `Snipdeck.bat uninstall`). Data tools move to Rust here (`backup::run_now`, `restore`, `--check-db` from step 16); the launcher keeps only download, update and uninstall, and `Snipdeck.bat doctor` prints the same checks by running `snipdeck.exe --doctor` when `app\` exists. Tests: every check has a pass and a fail case with the fake host; a11y tree; goldens light/dark with one failing row. Accept: a blind-tester with a corrupted FTS index and a missing backup reaches a green checklist from the window alone.
8. **[S] ItemEditor binding + new item** (`binding/editors.rs`): New snippet from kebab, Main window and `Ctrl+N`; edit from quick edit, Picker `Ctrl+E`, Main window; kinds text/link/template/image/file; sensitive toggle provisions the DPAPI-wrapped library key silently on first use (S3; fakes off Windows); autosave draft every 2 s to `drafts/` and restore on reopen; 16 KB cap → read-only viewer. Tests: create/edit/cancel/discard/draft-restore; sensitive item round-trips encrypted; Free tier blocks prompted fields with the Pro toast.
9. **[S, running]** Tray + icons (owner: tray "Open Panel" dead, no icon): `scripts/make_logo.py` emits `assets/logo/snipdeck.ico` (16-256) and tray PNGs per L2; `snipdeck-app/build.rs` embeds the icon resource (`winresource` or `embed-resource`, Windows only); tray loads `IDI_APP`; tray menu translated and themed; "Open Panel"/"Open Snipdeck" routed through `Controller::on_tray` with `SNIPDECK_LOG` tracing of every command. Headless test with the fake tray; wizard step M3 20 verifies on Windows.

## Wave 3: polish, i18n, review items, refactor
10. **[done: 0 untranslated; review list in docs/reviews/translation-review.md]** Translations: `python3 scripts/i18n.py check` → 0 untranslated for es, de, fr, pt-BR, ja, zh-Hans; `wording.rs` toasts moved to a translated Slint global; Haiku with the glossary in `crates/snipdeck-ui/README.md`. Accept: `check` reports 0; CJK test not silently skipped (PR4 #11: `#[ignore]` with reason or a bundled CJK test font).
11. **[S] PR4 review items** #2, #5, #7, #8, #9, #10, #12, #14 as listed in `docs/reviews/PR4-final-review.md`; each ticked with its commit.
12. **[S] Controller split** (H1) exactly per fable-analysis "Architecture redesign"; `cargo test -p snipdeck-app -- --list | sort` identical before and after; no behaviour change.
13. **[S] UX learnability** U1-U9: Getting-started list (`starter.rs`), footer wording, empty states, hover `⋯` = context menu, hide-to-tray one-time toast, Ctrl+Z undo, Help/About; one golden per state. Then **[S ux-auditor]** and **[S blind-tester]** runs; findings fixed in ranked order.
13a. **[S + skills unslop-ui, design-critique, accessibility-review] De-slop pass** (owner 2026-10-18): over the goldens of every window state (Panel, Picker, fill, Main window in all three views, editors, Settings, dialogs, toasts, light/dark/HC/225 %), against DESIGN.md: type scale and hierarchy, 4 px rhythm, alignment to the gutter, consistent radii and borders, one accent, no gradient/shadow slop, motion 80-150 ms only on state changes, icon optical sizes, wording (verbs, sentence case, no exclamation marks). Output: a ranked list in `docs/reviews/design-pass.md`; fixes land with regenerated goldens, each looked at; `ux-auditor` measures the result. Also: audit every IconButton and menu icon against the DESIGN §3 "Icon vocabulary" table (owner 2026-10-18: meatball vs kebab vs hamburger vs bento); fix the glyph, not the table.
14. **[post-v1, dropped 2026-10-18 for the ship date] Item colour label + list icon** (owner 2026-10-18): `Item.label: Option<Label>` (8 DESIGN swatches), 4 px left bar on rows/cards, filter chip in the Main window; `List.icon: Option<IconName>` from a curated set of 24 Phosphor icons + none, shown in tabs and sidebar; migration v+1 (forward-only with backup), export/import round-trip, editors updated, goldens. No per-row background colour, no emoji.
15. **[post-v1, dropped 2026-10-18 for the ship date] Accent colour + high contrast**: Settings accent from DESIGN "User accent choices"; full use of `SystemLook.hc-*`; 225 % icon scaling; contrast measured in a test for every token pair (WCAG 2.2 AA).
16. **[S] Performance** P2, P3: cached per-board index invalidated by the store change counter; budget tests (search < 16 ms / 10k, Panel load < 50 ms); release exe size and idle RAM printed by the Windows build workflow; `--check-db` CLI (B3).
17. **[done in code, merged 8dc9fd0; unverified on Windows: owner runs the 9-step checklist in .claude/remaining/launcher.md]** Launcher + prebuilt zip (9c, S7): `Snipdeck.bat` prefers the release zip from the GitHub Release, verifies the pinned SHA-256, falls back to source build only with `Snipdeck.bat build`; `pwsh tools/launcher/snipdeck.ps1 selftest` passes; README says building from source is for developers.
18. **[post-v1, dropped 2026-10-18 for the ship date] Edge dock with reveal** (SPEC 5.1, 3a): logic + fakes + settings (edge per monitor, trigger hover/click/both, delays) and wizard steps; the Windows hit-test spike S5 is the owner's run.
19. **[H] Docs**: README quick start = download zip; user guide matches every menu and shortcut; CHANGELOG; `docs/HANDOVER.md` rewritten to the shipped state.

## Wave 4: verification and release
20. **[O reviewer]** Independent review of the whole diff since `main` against SPEC, DESIGN and `docs/reviews/*`: correctness, regressions, security, data safety, missing tests. Findings fixed by **[S]**.
21. **[S] Fable prep** per `docs/fable-brief.md`: `scripts/gate.sh --full` → `docs/reviews/pre-fable-gate.md`; Haiku scouts → `docs/reviews/pre-fable-facts.md`. Then the Fable session closes the ranked list in `fable-analysis.md` (fix or ticket every item), re-runs the gate, updates the PR body, `/retro`.
22. **[owner]** Windows: `Snipdeck.bat`, `run-windows-tests.bat` (M2, M3, new steps), the release workflow with `SNIPDECK_LICENCE_PUBLIC_KEY`, signing (SignPath Foundation for open source), tag `v1.0.0`.
