## Morning summary

2026-11-23 (third update). Fixed and pushed (gate green): Main header row room for the focus ring (9a18360a), one
centred empty-state action group (e7861b11), Symbols freeze and the group strip (afe7ea01: the preview built an
element per cell and decoded every picture of the kind on each tab switch; the strip looped over grid rows).
Not verified in a running app on Windows: frame time while scrolling the grids (icons still rasterise per visible
cell, cached), the look of the header gap and empty state. Next, in order: board delete Undo toast, per-board
Edit/Delete and board-named menu labels; Panel "All" tab and the delete-list "also delete its items" toggle;
high-contrast sidebar focus ring and Panel cursor marker (.claude/audit/final2.md items 2 and 3). Weekly usage 95%.

2026-11-23 (second update). PR #5 is still held by the owner as 1.0.0: never merge, tag or release unasked.
- Done since: the final audit's 4 findings (Picker reveal of sensitive rows, chosen backup folder, group-delete docs,
  4 unused controller commands); the Windows test wizard builds again; owner findings fixed: sidebar focus ring inside
  the row, "New note" is "New item", Kaomoji/Characters group buttons fill the strip with a gap, the Panel cursor row
  is a faint fill (not the selection), Recent stays at the top for new copies (Slint-side, not measured in a running app).
- Owner's queue (verbatim in `.claude/now.md` Inbox, 2026-11-23): dropdowns as top-level popup windows; Symbols
  freeze/slow scroll/blank cells; collapsed dock outline; Main detail pane hide + resize; Main header gap; one centred
  empty state; boards (All boards view, "Default" first board, icon/colour shown, Manage boards, edit/delete per row,
  delete Undo); lists ("All" tab, delete with "also delete its items" toggle, Manage lists, Tags +); Settings "?" help
  tooltips, delete a backup, backups-to-keep near the list; symbol additions (fancy text converter, accented and
  Cyrillic letters, chess/cards/dice/zodiac/Braille/music, ~1,000 kaomoji, text emoticons).
- Decision: the stored thing stays "item", not "snippet" (owner left it to Claude).
- Blind usability tests (Sonnet/Opus) are still owed; the owner plans a Fable audit after the weekly reset (2026-11-28).
- Weekly usage 94% at the end (owner's line 98%).

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
- Item editor with Preview (`Alt+P`), draft recovery, triggers and typing expansion (Windows).
- Command palette (`Ctrl+K` or `Ctrl+Shift+P`), sidebar collapse button in the sidebar header (`Ctrl+B`),
  tinted list icons, Settings > Privacy "Clear recent emoji and colours".
- Library views (`snipdeck_core::{Sort, ViewFilter}`): one Sort for every section (a list's in the library, `All items`,
  Pinned and each tag's in `Settings::section_sorts`), a toolbar Filter (several tags any/all, untagged, kind, has trigger,
  sensitive; chips under the toolbar) that the Picker's Pinned scope also uses, group headers kept under any sort,
  group delete (at once, with Undo) and "Move group to list", tag colours with Merge on a rename clash, board accent and icon,
  and Trash search. Store schema V12 to V15 and archive format 1.3 carry them (`docs/format.md`). Group colour is built (Main window dot,
  menu row, dialog and the palette's "Group colour…"). Nested tags are built (`Tag.parent`, `TagTree` in core, V14, portable `parent`,
  sidebar tree, Tag manager "Nest under" dialog, undoable nest/merge/delete). Nested GROUPS are built (V15 `item_groups.parent`, `MAX_GROUP_DEPTH` 3, `GroupTree` in core, store `reparent_group`/`restore_group`, portable `parent`, shared `GroupWalk` in `snipdeck-app/src/group_walk.rs`, Main window indent, "Nest under…" in the header menu and the palette, subtree-aware ungroup/delete/move, one undo step each; see SPEC Groups). The Panel lists nested groups flat (headers and fold work, no indent yet); Ctrl+G on rows from several groups still makes a top-level group.
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
