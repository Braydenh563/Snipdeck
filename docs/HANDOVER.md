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

## Next steps (queue for the next session, in order)

Progress vs `docs/PLAN.md`: M1 done, M2 done in code (Windows unverified), M3 mostly done, M4 about half (UI built, not wired), M5 about 10%, M6 not started. About 55% of a v1 that can be built and tested off Windows.

1. **Merge the in-flight agent work first** (worktrees under `.claude/worktrees/`, branches `worktree-agent-*`; check `git log origin/claude/compassionate-pascal-pod8ux..` in each): Main window + editors + SettingsPage (a5d7314), Picker/fill/capture/paste-next wiring (a89b659), review fixes 1/3/4/6 (ac1ed36). For each: merge, fmt, clippy stable + `+1.99.1`, tests, Windows clippy (recipe above), `python3 scripts/i18n.py extract`, push, check CI. Anything uncommitted in a worktree is unmerged: look before deleting.
2. **Fix the rest of `docs/reviews/PR4-final-review.md`** (tick each `[ ]`): #2 attach panel on first ShowPanel with `--hidden`, #5 route OS close to `Controller::close`, #7 field cap, #8 hotkey toast by failure kind, #9 `args_os`, #10 select backend before services start, #11 CJK test skips silently (make it `#[ignore]` or bundle a CJK font), #12 blank sensitive preview, #14 hygiene (m2-harness lockfile not covered by `cargo deny`; `tools/windows-test/README.md` has the old `Backpack` URL).
3. **Wire the Main window** into `snipdeck-app` (add/edit items, lists, boards, tags, Settings page writing `settings.json`, Trash view, import/export UI). Right-align the toolbar buttons (view toggles + New item) was requested by the owner.
4. **Translations:** `python3 scripts/i18n.py check` lists untranslated entries (156+ and the Main window's); translate with a Haiku pass for es, de, fr, pt-BR, ja, zh-Hans; `check` must report 0. Also move `wording.rs` toasts into a translated Slint global.
5. **UI gaps from reviews:** split Esc (hide Panel) from the close button; search-focus-changed callback on PanelWindow; full high-contrast use of `SystemLook.hc-*`; 225% icon scaling; ItemCard HC focus ring on a selected compact card; Picker match highlighting and fill-form scrolling; items 12/13 of the component review (duplicated logic, API inconsistencies).
6. **M4 remainder:** text editor (16 KB cap + rope viewer), image editor (crop/rotate/resize/redact), Trash, template fill polish.
7. **M5:** capture overlay + OCR, Recent inbox, sensitive items (mask, clipboard auto-clear SPEC 5.7, master password), paste stack, licence UI (set `SNIPDECK_LICENCE_PUBLIC_KEY` at M6), onboarding, daily backups, crash log. `Free` board limit: callers must pass `max_boards: Some(3)` to import and restore.
8. **M6:** MSIX/MSI, signing, updater, winget, perf CI, accessibility audit (Narrator, contrast), docs site, CLA bot.
9a. **Licence tooling (owner request):** (a) a `snipdeck-keygen` tool (new crate under `tools/`, not shipped in the app) that signs `SNIP1.` keys with the production private key: `keygen --order FRIEND-001 [--email-hash ...]`, plus `keygen genkey` to make the keypair (format in `docs/licensing.md`; the signing code is `core/src/licence/testing.rs`, move the signer into the tool; the private key never enters the repo). Friends' keys use order ids like `GIFT-name` so they can be revoked. (b) Sharing: an offline key cannot stop sharing. Plan: signed revocation list (by `order`) fetched occasionally and cached; optional online activation cap (N devices per key) only if abuse shows up; accept some leakage, keep the key personal (name or email hash shown in the licence dialog). Decision needed from the owner before building any online check (privacy/no-telemetry promise).
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
