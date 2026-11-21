## Morning summary

(written at the end of the run)

# Snipdeck handover

Current state as of 2026-11-15, branch `claude/compassionate-pascal-pod8ux` (PR #5, still open). Older, long
session notes are in the git history of this file (`git log -p docs/HANDOVER.md`). Read this, then `docs/PLAN.md`,
`docs/SPEC.md` and `DESIGN.md`.

## What the app is

Snipdeck is a Windows-first sticky-note snippet deck written in Rust with Slint. A small Panel sits at the edge of
the screen and never takes focus from the app you are typing in; one click pastes an item there. The Picker
(`Ctrl+Shift+Space`) is a search box at the caret. The Main window holds boards, lists, tags, Pinned, Recent and
Trash. It is local-only: no account, server, telemetry or network port.

Everything is free. There is no paid tier, licence key or board limit (the Supporter tier and all licence code were
removed 2026-11-15). The optional "Support Snipdeck" link (Settings, Help > About, command palette) opens
`https://buymeacoffee.com/braydenh563` (`help_binding::SUPPORT_URL`) and unlocks nothing. Old settings and a stale
`licence.key` still load and are ignored. `docs/licensing.md` has the short current text.

Features that exist now (user-facing list in `README.md` and `docs/user-guide.md`):

- Panel with edge docking (drag to an edge, "Release to dock", tucks into a tab; open by hover or click), list
  tabs, pinned strip, quick add (`Ctrl+N`), drag to reorder, groups, bulk bar, and a pin button for keep-on-top
  (it has no keyboard chord).
- Colours tab: picker, saved and recent colours, eyedropper (Windows), formats, contrast, schemes.
- Six item kinds, templates with fields and date maths, paste as plain text, rich paste (HTML and RTF).
- Item editor with Preview (`Ctrl+P`), draft recovery, triggers and typing expansion (Windows).
- Command palette (`Ctrl+K` or `Ctrl+Shift+P`), sidebar collapse button in the sidebar header (`Ctrl+B`),
  tinted list icons, Settings > Privacy "Clear recent emoji and colours".
- Library views (`snipdeck_core::{Sort, ViewFilter}`): one Sort for every section (a list's in the library, `All items`,
  Pinned and each tag's in `Settings::section_sorts`), a toolbar Filter (several tags any/all, untagged, kind, has trigger,
  sensitive; chips under the toolbar) that the Picker's Pinned scope also uses, group headers kept under any sort,
  group delete confirmation and "Move group to list", tag colours with Merge on a rename clash, board accent and icon,
  and Trash search. Store schema V12 and V13 (group colour) and archive format 1.3 carry them (`docs/format.md`). Group colour is built (Main window dot,
  menu row and dialog); nested groups and nested tags are NOT built yet (design in `.claude/remaining/views.md`, SPEC post-1.0 list).
- Recent copies, with a first-run question; nothing is kept until the owner of the PC says yes.
- Master password with optional Windows Hello unlock; Sensitive items encrypted on disk.
- Import from `.snipdeck`, Ditto, CopyQ, Espanso, Beeftext, TextExpander, PhraseExpress, Windows clipboard
  history and folders of text files; export and daily backups.
- Seven languages (en, es, de, fr, pt-BR, ja, zh-Hans); catalogues are checked by `scripts/i18n.py`.

Not built yet (SPEC 5.4, library audit 29 and 30): the rope viewer for text of 16 KB to 5 MB (such text opens as a
read-only prefix, so find and the text tools do not reach it), the image editor (crop, rotate, resize, redact),
decoding JPEG/WebP/GIF/BMP to a stored PNG, and Replace on an existing Image or File item. Reason: each is a large
feature of its own (a virtualised text widget, an image pipeline, new crates) and was outside the time boxes of the
audit fixes. Built since: find/replace, whole-text changes, the monospace switch and line numbers for text of 16 KB
or less (the editor's Text tools row; `snipdeck_core::text_tools` holds the logic).

## Crate map

| Crate | Role |
|---|---|
| `snipdeck-core` | Domain types and pure logic (boards, items, template engine, fuzzy search). No I/O. |
| `snipdeck-store` | SQLite, migrations, FTS5, content-addressed blobs, trash, encryption of sensitive items. |
| `snipdeck-portable` | `.snipdeck` export and import, importers, daily backups. |
| `snipdeck-colour` | Colour maths: formats, OKLCH, schemes, WCAG contrast. No UI. |
| `snipdeck-symbols` | Emoji, kaomoji and icon catalogue, search, recents. No UI. |
| `snipdeck-platform` | OS layer: clipboard, hotkeys, Target tracking, paste injection, tray, DPAPI, Hello. Windows backend behind `cfg(windows)`; fakes for tests. |
| `snipdeck-ui` | Slint design tokens, components, windows (`ui/*.slint`), headless screenshot tests and goldens. |
| `snipdeck-app` | Use cases, view-models, controller and bindings, settings, the binary. |

Other folders: `tools/launcher` (`Snipdeck.bat` and the PowerShell launcher), `tools/windows-test` (Windows test
wizard), `scripts/` (gate, codemap, i18n, readme shots, version bump), `docs/` (guides, ADRs, reviews).
Find code by grepping `docs/CODEMAP.md` (regenerate with `python3 scripts/codemap.py`), then read line ranges.

## Build, test, gate

Run on Linux; the Windows backend only compiles there as stubs. Use a separate target dir when disk is tight:

```
export CARGO_TARGET_DIR=/tmp/target-panel CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
scripts/orient.sh                # start of a session
scripts/gate.sh --staged         # before every commit (fmt, clippy -D warnings, rustdoc -D warnings, third-party notices)
scripts/gate.sh --changed        # routine run: plus the tests of changed crates
scripts/gate.sh --full           # once at the end: cargo test --workspace --locked
```

CI (`.github/workflows/ci.yml`) runs the same on the newest stable Rust, plus `cargo deny check` and a Windows
`cargo check`. Run only one cargo process at a time on the shared machine.

Screenshots (README, user guide, Store) come from the UI golden tests:

```
SNIPDECK_SHOT_SCALE=2 SNIPDECK_SHOTS_DIR=/tmp/shots cargo test -p snipdeck-ui --no-fail-fast -- --test-threads=4
python3 scripts/readme_shots.py /tmp/shots     # needs pillow and pyoxipng; writes docs/images/ and docs/images/store/
```

Goldens (`crates/snipdeck-ui/tests/`) are compared by `testing.rs`; tests use an injected clock, never the real one.

## Verified on Linux only versus needs Windows

Verified here: core, store, portable, colour and symbols logic; the controller with fakes; every Slint window
rendered headless (software renderer, golden PNGs, accessibility tree); importers; i18n completeness; clippy and
docs.

Not verified until the owner runs it on Windows (`Test Snipdeck on Windows.bat`, `tools/windows-test/run-tests.ps1`,
quick check in `tools/windows-test/tests/quick-check.json`):

- Focus-safe paste into real apps, clipboard restore, delayed rendering, `Alt`+click menu-bar side effect.
- Panel docking at real screen edges and the taskbar, DPI changes, multi-monitor, render flicker.
- Eyedropper (screen freeze, HDR), typing triggers and suggest, Recent clipboard listener, tray menu.
- DPAPI key provider, Windows Hello (steps 71-77), autostart, file dialogs, accent colour read.
- Narrator on groups, IME and CJK input (S2 spike was inconclusive: no IME installed).
- Launcher `selftest`, `Snipdeck.bat` on a clean PC, the installer and the Store MSIX.
- Native-speaker review of the six non-English locales (`docs/reviews/translation-review.md`).

A claim about how a UI looks needs a measurement from a running app; say what was not verified.

## Owner-only release steps

Agents cannot do these (accounts, secrets, signing, Windows, merging). Do not merge PR #5, tag, or run
`release.yml` without the owner.

1. Windows checks: the wizard above, then re-test the 2026-10-21 findings T1-T13 in `.claude/now.md`.
2. Review and merge PR #5 into `main`; wait for CI green on `main`.
3. Release workflow: Actions > Release > Run workflow on `main`, version without the `v` (for example `1.0.0`).
   It runs `scripts/bump_version.py` (renames `## [Unreleased]` in `CHANGELOG.md` to the version and date), commits,
   tags and publishes the Release with the installer, the portable zip, `.sha256` files and attestations
   (`docs/packaging.md`). Put the first release's SHA-256 in `tools/launcher/release-sha256.txt`.
4. Delete the repository secret `SNIPDECK_LICENCE_PUBLIC_KEY` (no longer used). Agents may not edit workflows
   (a hook blocks it), so copy `THIRD_PARTY_NOTICES` and clean any remaining licence mentions in
   `release.yml` and `release-build.yml` by hand.
5. Microsoft Store: Partner Center account, name, listing, IARC rating, privacy URL (`docs/store-submission.md`);
   upload the MSIX workflow artifact.
6. Signing and certification: `docs/packaging.md` checks (MakeAppx, test-signed install, WACK `appcert.exe`),
   apply to SignPath, then check SmartScreen behaviour.
7. Repository hygiene before going public: `docs/going-public.md` (CodeQL test-password alerts, secret-scanning
   alert "Used in tests", commit author emails).
8. Fix the `CHANGELOG.md` date at the real release. The installer is not code-signed until step 6 is done, so the
   README note about SmartScreen stays until then.

## Working rules

See `CLAUDE.md` (lean workflow): batch tool calls, tests first, never weaken a test, independent review of risky
or UI changes, no model ids in docs, commit trailers as in recent commits, never commit `spikes-results/`.
`.claude/now.md` holds the Inbox and Decisions; per-agent progress is in `.claude/remaining/`.
