# Session state (2026-10-18, Fable session, branch `claude/compassionate-pascal-pod8ux` rebased on main after PR #4)

- **This session runs on Fable** and is the one permitted Fable agent (owner: "you are fable btw"); it does judgement and delegates implementation to Sonnet/Haiku. Do not spawn a second Fable agent.
- **Done:** `claude/snipdeck-main-window` merged (its ui tests pass; its goldens unchanged). Pin icon: `push-pin-simple`, stable label "Keep panel on top", header tooltips in `panel.slint`; Panel goldens regenerated. Stable fmt/clippy clean; `cargo +1.99.1 clippy` + doc running (`/tmp/claude-0/gate2.log`). Target dir `/tmp/target-main`.
- **In flight (three Sonnet agents in `.claude/worktrees/`, each on its own branch; merge their branches into this one, then delete `/tmp/target-a|b|c`):** (A) Main window wiring in `snipdeck-app` (`main_window.rs`, `main_binding.rs`, Trash + 5 s undo, list/board CRUD + reorder, `tests/main_window.rs`); (B) Panel UI in `snipdeck-ui` (kebab `app-menu-*`, `resize-requested(ResizeEdge)`, `edit/rename/delete-requested(int)`, pinned-strip wheel + carets, tooltips); (C) `tools/snipdeck-keygen`.
- **Wave 2 (after merging A+B):** bind the new Panel callbacks in the app (kebab entries: Open Snipdeck, New snippet, Settings, Pause hotkeys, Help, About, Quit; OS resize on `resize-requested`; quick edit via ItemEditor window; Settings window writing settings.json); Picker + TemplateFill windows bound to hotkeys (`View.picker`/`View.fill` are computed in `controller.rs:360-370` but never rendered); PR4 review items #2 #5 #7-#12 #14; translations (Haiku); Snipdeck.bat selftest; prebuilt zip (CI workflow edit needs the owner); then `ux-auditor` + `blind-tester`, then the Fable analysis (`docs/reviews/fable-analysis.md`: findings, missed-features brainstorm, logo critique), fixes, `scripts/gate.sh --full`, PR, `/retro`.
- Inbox and decisions: `.claude/now.md`. Owner's later asks (whole-app polish, optimisation, security, a11y, feature brainstorm, logo) are folded into the Fable pass.

# Morning summary (2026-10-18)

- **Done (PR #4, CI running on daca819):** M2 Windows layer, M3 UI library + Panel + tray/settings/theme, Picker/fill UI + logic (no window binding yet), store batch API, 9 security + 13 Win32 + UI review fixes, final PR review fixes #1/#3/#4/#6, README/docs/logo, one-click launcher `Snipdeck.bat` (works on the owner's PC up to preflight).
- **Not merged:** Main window + editors + Settings page (branch `claude/snipdeck-main-window`, gate unconfirmed); launcher self-test never run.
- **Blocked on the owner:** all Windows behaviour is unverified except the owner's first demo run: see "Owner's first real Windows run" below (item 0: Panel buttons and board dropdown dead = top priority).
- **Progress:** about 55% of a v1 (M1 done, M2/M3 mostly, M4 half, M5 ~10%, M6 not started).
- **Next:** item 0 regression with tracing, then the owner's list 1-9, review doc `[ ]` items, Main window PR, translations, M4 remainder, M5, M6, then the one permitted Fable pass.
- **Start a new session:** "read docs/HANDOVER.md and continue" on branch `claude/compassionate-pascal-pod8ux` (do not merge PR #4 unasked).

# Handover (2026-10-17)

Read this first in a new session, then `docs/PLAN.md`, `docs/SPEC.md`, `DESIGN.md`.

## Where things stand

- **Repo:** `Braydenh563/Snipdeck`, local folder `/home/user/Snipdeck`. PR #1, #2 (M0/M1) and #3 (lean workflow) are merged into `main`.
- **Product:** Snipdeck, a Windows-first sticky-note-style snippet deck in Rust + Slint. Decisions are in
  `docs/SPEC.md` (grill rounds 1-2), `docs/adr/0001-0007`, `DESIGN.md` ("calm, tactile deck": warm neutrals,
  Index Blue for selection only, ink primary buttons, IBM Plex, number gutter).
- **M0 spikes:** first real Windows run 2026-10-17 (Windows 11 only, quick mode): `docs/spikes/windows-results-20261017.md`.
  **S1 = GO** (focus kept; paste into Notepad/Chrome; Shift copy-only; elevated fallback; password fields).
  S1 fix needed: Alt+click opens the target's menu hints and releasing Win opens Start (step 2c).
  S2 is **inconclusive** (no IME was installed, so nothing could be composed). S4: DWM calls ok, theme switching ok,
  but OS caption buttons overlap the app's own close button. S3 measured on Linux. Constraints: `docs/spikes/README.md`.
- **M1 (core, store, licence, crypto, portability): done and merged into the PR branch.** 488 tests, fmt/clippy/doc/deny
  clean on Rust 1.97 and 1.99.1, Windows target check passes. Opus review in `docs/reviews/M1-review.md`; all
  29 core/store findings fixed. Known gaps: `docs/PLAN.md` "M1 known gaps" (CopyQ `.cpq` not parsed, Windows clipboard-history importer, `DataKey` not `Clone`).
- **Windows test wizard:** `run-windows-tests.bat` -> `tools/windows-test/` (manual steps in `tests/*.json`,
  opt-in automation in `auto/`). The automation has **never run on Windows**; its unverified assumptions are
  listed at the end of `tools/windows-test/README.md`.
- **CI** (`.github/workflows/ci.yml`): Linux fmt/clippy/test/doc/deny + Windows `cargo check`. It uses the
  newest stable Rust, which has lints that older Rust lacks: always check with `cargo +1.99.1 ...` too.

## What is blocked on the owner

- Optional re-run on Windows: S2 after adding Japanese / Chinese / Korean keyboards (`run-windows-tests.bat`, S2 only),
  S1 step 2c after the modifier fix, and the untested S4 steps (W4-W7, Windows 10). None of these gate M2 start.

## Overnight queue (away mode, started 2026-10-17)

Goal: advance M2 -> M3 -> M4 logic as far as Linux can verify; branch `claude/compassionate-pascal-pod8ux`, PR #4 (open one PR per 3-5 items; never merge).
1. Land the three M2 backend agents' work (verify on Rust stable + 1.99.1 + Windows target, Opus `reviewer` on the unsafe code, commit, push).
2. M2 test harness example + `tools/windows-test/tests/m2-*.json` wizard steps (incl. S1 2c re-run, UIA OneNote/Terminal, UIPI).
3. Pin Slint, add `snipdeck-ui`: `tokens.slint` generated from `design/tokens/palette.json`, component library (DESIGN.md), software renderer, screenshot tests under the Slint testing backend (our "end-to-end" on Linux).
4. Panel UI (ListView, number gutter, tabs scroll at large text) wired to `snipdeck-platform` traits with fakes; `snipdeck-app` skeleton (single instance, hotkeys, tray crate decision).
5. i18n scaffolding (`@tr`, 7 locales extracted), CJK font order, accessibility properties on every control.
6. Picker, Main window, editors, template fill form (logic + screenshot tests).
7. Security/tests pass: fuzz/proptest on parsers, cargo deny, review of crypto/DPAPI wiring.

Not verifiable here (stays unverified until the owner runs the wizard on Windows): every Win32 behaviour, IME, Mica, DPI, Narrator, real-app paste. Playwright is for browsers; Slint apps are tested with the Slint testing backend plus software-renderer screenshots.

- **Try the app (Windows):** double-click `run-snipdeck-demo.bat` (needs Rust). It builds and opens `snipdeck-app --demo` on a seeded in-memory library. Unverified on Windows until the owner runs it.

## State (end of M2, M3 under way; branch claude/compassionate-pascal-pod8ux, PR #4)

- **M2 landed** (`snipdeck-platform`): portable logic + Windows backend (clipboard with delayed render, keyboard, Target
  tracking + UIA, hotkeys, autostart, single instance, DPAPI, panel style) + `tools/windows-test/m2-harness` and 19 wizard
  steps in `tests/m2-platform.json`. Opus review done, fixes applied. All Win32 behaviour **unverified** until the wizard runs.
  Windows check from Linux: `cargo clippy -p snipdeck-platform --all-targets --target x86_64-pc-windows-msvc --locked`.
- **`snipdeck-ui`** (M3): tokens generated from `design/tokens/palette.json` by `build.rs`, `Theme`/`Type`/`Space`/`Radius`/`Motion`,
  Phosphor icons (`scripts/ui_icons.py`), IBM Plex bundled, Button, headless harness (`src/testing.rs`: software renderer,
  frozen clock, golden PNGs in `tests/golden/`, `UPDATE_GOLDEN=1`), accessibility-tree tests via `i-slint-backend-testing`,
  bundled translations (7 locales, `scripts/i18n.py extract|check`, wired into the gate). Conventions: `crates/snipdeck-ui/README.md`.
  Component library merged (primitives, inputs/navigation, deck; ~150 tests). Opus review findings fixed (`docs/reviews/M3-ui-components-review.md`).
- **Store batch API** landed: `Store::transaction` / `Batch` (savepoints, rollback incl. blobs); import is atomic via `Library::atomically`.
- **`snipdeck-app`** (skeleton runs): `usecase` (use an item: content -> template -> pipeline -> count use),
  `panel` (view-model), `toast` (messages), `settings` (tolerant load, chord rules), `controller` (Slint-free: hotkeys,
  Target changes, use/copy, template prompt state, toast lifecycle, `Effect` list), `binding` (`Controller::bind(this, window, on_effect)`:
  callbacks in, changed properties out), `host` (Clock / TargetSource / HotkeyBackend traits + fakes), `platform` (Windows services, or fakes
  off Windows), `demo` (seeded library), `screenshot`, `main.rs` (args, single instance, hotkeys, software renderer, event loop).
  Tests: 89 unit + 13 headless Panel window tests (`tests/panel_window.rs`) + 2 screenshot tests. Run `cargo run -p snipdeck-app -- --demo`
  (dev window on fakes) or `-- --screenshot out.png [--dark]` (no display needed). Windows check from Linux needs a stand-in
  archiver for the C build scripts: `CC_x86_64_pc_windows_msvc=true AR_x86_64_pc_windows_msvc=<script that touches the output .lib>`.
  The `cfg(windows)` code type-checks and passes clippy for the Windows target; it has **never run** on Windows.
- App shell (2026-10-18): `Controller::on_tray`, `set_system_look`, `update_settings`, `close`; `Binding::tray` / `system_look`;
  `Host` has `tray`, `warnings`, `keep_alive` (was `guard`). Wizard: `tools/windows-test/tests/m3-app.json`.
- Decisions: ADR-0008 (OS caption buttons on the main window, own chrome on the panel, Win32 tray in the platform crate).
  Default hotkeys: Ctrl+Alt+Space toggles the Panel (new, not in SPEC), Ctrl+Shift+Space Picker, Ctrl+Shift+2 capture, Ctrl+Alt+V paste next.
- CI note: Slint pulls `fontique` -> `yeslogic-fontconfig-sys`; `snipdeck-ui` enables `i-slint-common/fontconfig-dlopen` so CI needs no
  `libfontconfig-dev`. Debug `target/` for snipdeck-ui is about 4 GB: keep few copies, `rm -rf` old ones.

## Owner's first real Windows run (2026-10-18, `Snipdeck demo` build) - fix these first

0. **TOP PRIORITY, a regression:** "none of the buttons work (except the X that closes the panel); the top-left dropdown (board switcher) no longer renders its contents or works" (owner, later 2026-10-18, after pulling newer commits; earlier the dropdown worked). Hypotheses, in order: (a) the Win32 hardening (`windows/dpi.rs` per-monitor DPI awareness set in `platform::connect` before any window exists, tray window thread DPI context) makes Slint's pointer coordinates and its scale factor disagree, so clicks land elsewhere and popups are drawn outside the clipped window (X still works because it sits at the window edge) - test by running with DPI awareness off and at 100%/150%; (b) `install_panel_style` / non-activating style (WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW) breaks Slint `PopupWindow` (popups need their own HWND; check the winit/Slint popup path) and mouse activation; (c) the `Controller` dropping callbacks (frozen target / dialog-open state added with the Picker logic, `toggle` state out of sync after a close). Plan: add `SNIPDECK_LOG=1` tracing of every UI callback, controller effect and pointer position to the launcher log, add a wizard step that records which of the 9 clicks reach the controller, reproduce, then fix and add a headless regression test (click each Panel control with the testing backend and assert the controller saw it; include the board-switcher popup open and a popup item click).
   **Analysis (2026-10-18, panel-clicks agent):** the code path is not broken in the demo: new headless tests click every header control, the board menu and a menu item, a tab and a pinned chip with real pointer events (`tests/panel_window.rs` `pointer_clicks_*`) and pass. (a) is ruled out: the owner's first run, where the dropdown worked, already had 211068d, and winit sets the very same per-monitor-v2 context itself (`become_dpi_aware`). (b): the board menu is not a `PopupWindow`, it is drawn inside the Panel (`panel.slint` end). (c): no controller guard drops callbacks. What changed between the two runs is the launcher: double-clicking `Snipdeck.bat` (no command) runs `run`, the **real library**, which on a new PC is empty: no board, so the switcher is blank, its menu has no entries, there are no tabs, pinned chips or rows, and the empty state's "New list" / "Save from clipboard" and the `+` tab were not wired; only X did anything. Fixed: a library without boards gets "My snippets" / "Snippets" at launch (`starter.rs`), `+` and "New list" add a list, "Save from clipboard" runs quick capture. `SNIPDECK_LOG=1` logs every callback, effect, tray/hotkey event, dropped cross-thread event and pointer press (physical, logical, scale) to stderr (the launcher's `app-*.err.txt`). Wizard steps M3 18 (demo, click each control) and 19 (first run) check it on Windows. If the owner did run `Snipdeck.bat demo`, step 18's log tells which clicks reach the window.

The demo ran on Windows (first time ever). Owner's report, verbatim in substance, plus a first guess at the cause:

1. **Cannot minimise the Panel.** Own chrome has only close/hide. Add minimise/hide and a way to reopen (tray, hotkey).
2. **Pinned-badge row scrolls only with the trackpad**, not mouse wheel or touch. Needs wheel (and Shift+wheel), drag/touch flick, visible scroll affordance (edge fade or chevrons) or a different layout (wrap, or a "more" overflow). Component: `pinned-strip.slint`.
3. **The `+` button in the tab row does nothing** (add list not wired; no feedback). Wire it to the list editor once the Main window lands, or hide it until then.
4. **No help or tooltips; unclear how to use it.** Add tooltips on every icon button, a first-run hint/coach marks, an empty-state explaining pin/paste/shift-click, a Help entry (links to docs/user-guide.md).
5. **Top item is cut off for a moment when clicked** (selection/focus ring clipped by the ListView; owner says low priority).
6. **No way to open the larger window; no logo in the Panel header; no kebab/meatball menu** for more options and settings. Add header: logo, board switcher, search, kebab menu (Open Snipdeck, Settings, Pause hotkeys, Dock, Help, About, Quit).
7. **The pin button does nothing.** Define it (keep Panel open/always-on-top vs pin item) and wire it, or remove it.
8. **Not shown in the taskbar (by design: skip-taskbar), shown in the tray, but none of the tray menu items work** (Open Panel, Open Snipdeck, Pause hotkeys, Quit). The menu itself appears. Suspect the tray-to-controller event path (`TrayEvent` handling in `main.rs` effect handler, `invoke_from_event_loop` before the backend is selected, review finding 10) or the Win32 fixes of 2026-10-18 (IconPresent, WM_CLOSE mapping). Add logging of every tray command and reproduce with `tools/windows-test` m3 wizard steps. Consider showing in the taskbar when the main window is open.
8b. **No quick edit of items in the Panel** (owner, 2026-10-18): add inline quick edit (Ctrl+E or double-click / context menu on a row: edit text, rename, move, pin, delete, with undo) using the ItemEditor from the Main window branch. SPEC has Ctrl+E; no handler exists.
8a. **The Panel cannot be resized** (owner, 2026-10-18). Add edge/corner resize (OS resize border or own grips), min/max size, remember size per monitor (SPEC: per-monitor position memory); works with the non-activating style and docking.
9. **Slow build every time the demo launcher runs.** The launcher agent (a375ca0) builds `Snipdeck.bat` with a stamped exe cache in `dist\`; until it is merged each launch runs `cargo run` (an up-to-date cargo run is still seconds-slow and rebuilds when sources change).

## Next PR: drive the app to shipping (owner, 2026-10-18)

The owner confirmed the Panel buttons work now. Missing utility the owner listed: delete and edit lists, open the larger window and its menu, manage snippets and list items, more. Plan for the next PR(s), in order: (1) merge `claude/snipdeck-main-window` and wire it (items 1-3 below), with list/board/snippet CRUD, Trash, undo, Panel kebab menu, resize, quick edit; (2) Windows verification loop with the owner using `Snipdeck.bat` and the wizard; (3) M5/M6 essentials for a first public release (licence UI, onboarding, backups, installer/prebuilt zip, signing later); (4) ONE Fable pass last, brief in `docs/fable-brief.md` (owner named Fable for this task on 2026-10-18).

**The pin icon:** in the Panel header it is the "keep on top" toggle (`pin-toggled` -> window always-on-top), but it has no tooltip or visible state, and the same push-pin icon is used for pinned snippets, so it reads as "pin an item". Fix: tooltip + pressed state, a different icon for always-on-top, and verify always-on-top works on Windows (wizard step).

## Next steps (queue for the next session, in order)

Progress vs `docs/PLAN.md`: M1 done, M2 done in code (Windows unverified), M3 mostly done, M4 about half (UI built, not wired), M5 about 10%, M6 not started. About 55% of a v1 that can be built and tested off Windows.

1. **Main window branch (the only unmerged work):** branch `worktree-agent-a5d7314cbba05d676` (worktree `.claude/worktrees/agent-a5d7314cbba05d676`, last commit cbb6d10, pushed to origin as `claude/snipdeck-main-window`; open the follow-up PR from it). Main window, editors, SettingsPage, forms, CJK font helper, toolbar right-aligned. **Gate never confirmed**: finish per its `.claude/remaining/ui-main.md` (rebuild, run plain `cargo test -p snipdeck-ui`, regenerate only its own goldens, READ every golden, check the 225% claim, fmt, clippy stable + `+1.99.1`, doc), merge `origin/claude/compassionate-pascal-pod8ux` into it first (done once, conflicts resolved in lib.slint/button.slint/po files), then wire it into `snipdeck-app`.
   Merged into the PR branch this session (verify CI): Win32 tray fixes, docs/README/logo, review fixes #1/#3/#4/#6 (+ `write_salvaged` wired in `main.rs`), Picker/fill/capture/paste-next **logic** (`picker.rs`, `fill.rs`, `capture.rs`, `paste_next.rs`; no window binding yet: `binding.rs` must render `View.picker`/`View.fill`, register their HWNDs with the target tracker, toasts must show while the Panel is hidden), launcher (`Snipdeck.bat`, `tools/launcher/`: **never run**, run `pwsh tools/launcher/snipdeck.ps1 selftest` first; README quick start, troubleshooting and user-guide still describe `cargo run`).
2. **Fix the rest of `docs/reviews/PR4-final-review.md`** (tick each `[ ]`): #2 attach panel on first ShowPanel with `--hidden`, #5 route OS close to `Controller::close`, #7 field cap, #8 hotkey toast by failure kind, #9 `args_os`, #10 select backend before services start, #11 CJK test skips silently (make it `#[ignore]` or bundle a CJK font), #12 blank sensitive preview, #14 hygiene (m2-harness lockfile not covered by `cargo deny`; `tools/windows-test/README.md` has the old `Backpack` URL).
3. **Wire the Main window** into `snipdeck-app` (add/edit items, lists, boards, tags, Settings page writing `settings.json`, Trash view, import/export UI). Right-align the toolbar buttons (view toggles + New item) was requested by the owner.
3a. **Edge dock with hover/click reveal (owner request, see SPEC 5 Panel):** collapsed handle strip on a screen edge that slides the Panel out on hover and/or click, like Microsoft PC Manager's side panel/toolbar (owner sent screenshots: a vertical strip on the right edge with a grabber, and a horizontal one at the top). Settings: trigger (hover/click/both), delays, edge per monitor. Start with a Windows spike (S5: appbar vs topmost window, hover hit-test without stealing focus), then logic + tests with fakes, then the wizard steps.
4. **Translations:** `python3 scripts/i18n.py check` lists untranslated entries (156+ and the Main window's); translate with a Haiku pass for es, de, fr, pt-BR, ja, zh-Hans; `check` must report 0. Also move `wording.rs` toasts into a translated Slint global.
5. **UI gaps from reviews:** split Esc (hide Panel) from the close button; search-focus-changed callback on PanelWindow; full high-contrast use of `SystemLook.hc-*`; 225% icon scaling; ItemCard HC focus ring on a selected compact card; Picker match highlighting and fill-form scrolling; items 12/13 of the component review (duplicated logic, API inconsistencies).
6. **M4 remainder:** text editor (16 KB cap + rope viewer), image editor (crop/rotate/resize/redact), Trash, template fill polish.
7. **M5:** capture overlay + OCR, Recent inbox, sensitive items (mask, clipboard auto-clear SPEC 5.7, master password), paste stack, licence UI (set `SNIPDECK_LICENCE_PUBLIC_KEY` at M6), onboarding, daily backups, crash log. `Free` board limit: callers must pass `max_boards: Some(3)` to import and restore.
8. **M6:** MSIX/MSI, signing, updater, winget, perf CI, accessibility audit (Narrator, contrast), docs site, CLA bot.
9c. **Users must not need Rust, Visual Studio or git (owner, 2026-10-18).** `Snipdeck.bat` is the developer/tester launcher (builds from source; the owner's PC passed all its checks). For everyone else ship a prebuilt app: (a) CI job (owner must allow a workflow edit: agents may not touch `.github/workflows`) that builds `snipdeck-app.exe` on windows-latest and uploads a portable zip as an Actions artifact / GitHub Release (dev builds use the development licence key; releases need `SNIPDECK_LICENCE_PUBLIC_KEY`); (b) `Snipdeck.bat` prefers a downloaded prebuilt zip (verify SHA-256) and only falls back to building from source when asked (`Snipdeck.bat build`); (c) later the M6 installer (MSIX/MSI, winget). Until then the README must say clearly that building from source is for developers.
9a. **Licence tooling (owner request):** (a) DONE: `tools/snipdeck-keygen` (`genkey`, `sign`, `verify`; guide in `docs/licensing.md` "Keys for friends"; signer in `core/src/licence/signing.rs` behind feature `keygen`). Original request: a a `snipdeck-keygen` tool (new crate under `tools/`, not shipped in the app) that signs `SNIP1.` keys with the production private key: `keygen --order FRIEND-001 [--email-hash ...]`, plus `keygen genkey` to make the keypair (format in `docs/licensing.md`; the signing code is `core/src/licence/testing.rs`, move the signer into the tool; the private key never enters the repo). Friends' keys use order ids like `GIFT-name` so they can be revoked. (b) Sharing: an offline key cannot stop sharing. Plan: signed revocation list (by `order`) fetched occasionally and cached; optional online activation cap (N devices per key) only if abuse shows up; accept some leakage, keep the key personal (name or email hash shown in the licence dialog). Decision needed from the owner before building any online check (privacy/no-telemetry promise).
9b. **Logo:** `assets/logo/snipdeck-icon.svg`, `snipdeck-logo-light.svg`, `snipdeck-logo-dark.svg`, generated by `scripts/make_logo.py` (wordmark outlined from IBM Plex Sans SemiBold); the owner asked for further refinement. Still to do: export `.ico` (16-256 px) for the exe icon resource and the tray, and MSIX tile PNGs.
9. **Run the fable pass last** (the owner allowed ONE focused Fable agent at the very end of full development: analyse, poke holes, challenge architecture, fix bugs and security issues, polish and validate).
10. Known gaps: Windows clipboard-history importer, CopyQ `.cpq`, no first-time "hidden to tray" notice, tray menu is English and ignores dark mode, exe has no icon resource, `{clipboard}` template text is `None`, time zone is UTC, licence not wired outside `--demo`, context menus, reveal-in-list, undo.
11. **Owner on Windows** (never run, everything Win32 is unverified): `run-windows-tests.bat` (M2 and M3 steps), `run-snipdeck-demo.bat` (builds and opens the demo app), S2 with IMEs, S1 step 2c, S4 on Windows 10.

## Working notes that cost time this session

- **Disk is small.** Cargo `target/` dirs reach several GB each; use `CARGO_TARGET_DIR=/tmp/target-<name>` and delete
  it after use. Spike `target/` folders were deleted once (23 GB); the wizard rebuilds them on Windows.
- **Parallel agents:** give each its own crate/files and its own `CARGO_TARGET_DIR`; verify and commit subsets from a
  clean `git worktree` (copy only the finished files, regenerate `Cargo.lock` from HEAD's lock with
  `cargo metadata`, then run fmt/clippy/test/doc/deny/Windows check on both Rust versions before committing).
- **The stop hook** asks to commit whenever the tree is dirty; do not commit another agent's work in progress.
- **After a session restart** the `origin` remote may revert to the old `Backpack` URL:
  `git remote set-url origin https://github.com/Braydenh563/Snipdeck`.
- **Never answer `y` to the wizard's final "commit results?" question in dry-run tests.** That happened four times
  and pushed fake results files (now removed); `-DryRun` no longer touches git, but keep real result files out of
  the repo unless they came from a real Windows run.
- PowerShell 7 for parse checks and the wizard's `-SelfTest` / `-DryRun` is not installed by default (download the
  Linux tarball from the PowerShell GitHub releases into the scratchpad).
- The owner prefers terse answers and dislikes manual steps: automate what can be automated, give y/n prompts.
