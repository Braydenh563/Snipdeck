# Changelog

All notable changes to Snipdeck are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project intends to follow
[Semantic Versioning](https://semver.org/) once it has releases.

Snipdeck has had no release yet. Everything below is under development; milestones are defined in
[`docs/PLAN.md`](docs/PLAN.md). "Unverified" marks Windows behaviour that no one has run on a real Windows
machine.

## [Unreleased]

### Changed
- Launcher: `Snipdeck.bat` now starts the prebuilt app (`app\snipdeck.exe`) and, when it is missing, downloads the
  latest GitHub Release zip and verifies its SHA-256 (the release `.sha256` and `tools/launcher/release-sha256.txt`).
  It no longer checks for Rust, Visual Studio, git or crates.io; the source build moved to `Snipdeck.bat build`
  (developers), and `Snipdeck.bat update` re-downloads the release instead of `git pull`. Written on Linux, never run
  on Windows. README quick start is now "download, unzip, double-click".

### Added

**v1 push (2026-10-18)**
- Main window, item/list/board editors, tag manager and Settings page (`snipdeck-ui`), merged from the main-window branch.
- Panel: the "More" (kebab) menu now works (Open Snipdeck, New snippet, Settings, Pause hotkeys, Help, Quit), the edge and corner grips resize the window (260 to 480 wide, remembered per monitor in `panel_geometry`), and Delete / Ctrl+E / F2 on a row act (Delete goes to the Trash with a 5 s Undo). Written on Linux; the OS resize and the per-monitor restore are unverified on Windows.
- Item editor: New snippet (Panel menu, Main window, Ctrl+N) and Edit (Panel quick edit, Picker Ctrl+E, Main window) open the item editor for text, code, link, template, image and file items. A new Image or File item takes the path of a file (typed, or chosen with Browse on Windows) and keeps its own copy. The first sensitive item on Pro makes the DPAPI-wrapped library key without a prompt and is stored encrypted; text over 16 KB opens read-only; unsaved changes are kept as a draft every 2 s and come back when the editor is opened again (never for a sensitive item); Free users are told prompted placeholders are Pro. The crash toast's "Open log" button opens the reports folder. Written on Linux; DPAPI and the Open dialog are unverified on Windows.
- Panel: the always-on-top toggle is a distinct `push-pin-simple` icon with a stable "Keep panel on top" label and tooltips on the header buttons.
- `snipdeck-keygen` (tools/): `genkey`, `sign`, `verify` for friends' licence keys; the signer moved into `snipdeck-core` behind the `keygen` feature.
- GitHub: RustSec `cargo audit` daily, Dependabot (cargo + actions), and a Windows portable-zip build (dev build on main, release build with SHA-256 on `v*` tags).
- `docs/reviews/fable-analysis.md`: ranked findings, architecture redesign, UX, security/privacy and feature brainstorm.

### Added

**M3 (in progress): UI foundation, Panel and app shell**
- `snipdeck-ui`: design tokens generated from `design/tokens/palette.json`, light and dark themes (high
  contrast is read but only partly applied), IBM Plex fonts, Phosphor icons, and a component library (buttons, inputs, tabs, menus, dialogs,
  toasts, item cards, pinned strip, board switcher), with a headless test harness (software renderer, frozen
  clock, golden PNGs, accessibility-tree tests).
- Panel, Picker and template fill-form windows with golden images, behaviour tests and pixel measurements.
- Translation catalogues for en, es, de, fr, pt-BR, ja and zh-Hans, with `scripts/i18n.py extract|check`
  (completeness varies; missing strings show in English).
- `snipdeck-app`: use-case layer (resolve an item, render its template, run the paste pipeline, count the
  use), Panel view-model, toast messages, tolerant settings with atomic debounced saving, a Slint-free
  controller (hotkeys, Target changes, tray events, close to tray), the Slint binding, and the binary with
  single-instance, hotkeys and tray wiring.
- `--demo`, `--screenshot <png>`, `--dark`, `--hidden`, `--data-dir` options, and `run-snipdeck-demo.bat`.
- ADR-0008: OS caption buttons on the main window, own chrome on the Panel, Win32 tray in the platform crate.
- Not done: Picker, quick capture and paste-next are registered but do nothing; no Main window, no Settings
  window, no fill form wiring, no way to add or edit items in the app. Unverified on Windows.

**M2: Windows platform layer (`snipdeck-platform`)**
- Clipboard read/write with delayed rendering and multiple formats, keyboard injection, Target tracking
  with a UI Automation fallback, global hotkeys, paste pipeline with a per-app paste-key table, autostart,
  single instance, DPAPI key provider, non-activating panel style, tray icon, system light / dark /
  high-contrast detection, and fakes for all of them.
- `tools/windows-test/m2-harness` and 19 wizard steps; M3 wizard steps for the app shell.
- Unverified on Windows: all of it.

**M1: foundation (`snipdeck-core`, `snipdeck-store`, `snipdeck-portable`)**
- Cargo workspace, CI (Linux fmt, clippy, tests, docs, `cargo deny`; Windows `cargo check`).
- Core: boards, lists, items, formats, tags; template engine with `{date}`, `{time}`, `{datetime:FORMAT}`,
  `{clipboard}`, `{cursor}`, and Pro prompts `{field:...}` and `{choice:...}`; in-memory fuzzy search;
  Free/Pro entitlements; offline Ed25519 licence verification.
- Store: SQLite with migrations and a pre-migration backup, FTS5 search, content-addressed blob store, ordering,
  30-day trash, transactions and batches, XChaCha20-Poly1305 encryption of sensitive items with an optional
  Argon2id master password and a `KeyProvider` trait.
- Portable: `.snipdeck` export and import, importers for folders of text files, Ditto and CopyQ JSON, daily
  local backups.

**M0: spikes and design**
- Product specification (`docs/SPEC.md`), ADRs 0001-0007, design language (`DESIGN.md`), OKLCH palette generator.
- Four spikes (non-activating panel and paste, IME and CJK, long text, Mica) with findings; Windows test
  wizard (`run-windows-tests.bat`). First Windows 11 run, 2026-10-17: S1 (focus-safe paste) GO, S2 (IME)
  inconclusive because no IME was installed, S4 (window chrome) partly measured.

**Project**
- README, contributing guide, security policy, code of conduct, issue and pull request templates, and the
  user guide, development guide and troubleshooting pages under `docs/`.
- `scripts/gate.sh` (fast checks, changed-file tests, full suite), `scripts/codemap.py`, `scripts/i18n.py`.

### Changed
- The GPL licence is GPL-3.0-only (was AGPL in the first draft).
- Windows paste restores the previous clipboard when the target has asked for the data (delayed rendering)
  instead of after a fixed 300 ms wait.
- Inline text editing is capped at 16 KB and search matches title, tags and the first 64 characters, from
  the S3 measurements.

### Fixed
- Findings of the Opus reviews of M1 (29 in core and store), the M3 security review, the M3 component review
  and the Win32 tray review are fixed or tracked; see [`docs/reviews/`](docs/reviews/).
- `Cargo.toml` `repository` points at `github.com/Braydenh563/Snipdeck` (the project was renamed from Backpack).

### Security
- Importers and the archive reader refuse decoy zip records, oversized values and views in Ditto databases,
  count memory while reading, keep imported sensitive payloads out of the blob folder and scrub them on
  rollback (second M3 security review).

[Unreleased]: https://github.com/Braydenh563/Snipdeck/commits/main
