# Changelog

All notable changes to Snipdeck are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/) from 1.0.0.

"Unverified" marks Windows behaviour that no one has run on a real Windows PC.

## [1.0.0] - 2026-10-21

First release. Windows 10 and 11, 64-bit. Not code-signed, so Windows SmartScreen asks you to confirm the
first start.

### Added
- **The Panel**: a strip at the screen edge that never takes the focus from the app you are typing in. List
  tabs, a pinned strip, search, a row menu (Use, Copy, Edit, Pin, Move, Colour, Sensitive, Delete), and a menu
  of its own (Open Snipdeck, New snippet, Settings, Pause hotkeys, Quit).
- A **New item** row in the Panel, multi-select with a bulk bar (move, colour, sensitive, delete), **Show
  numbers**, and **Reset size and position**. Resize by dragging its edges (260 to 480 pixels wide); size and
  place are remembered for each monitor. The keep-on-top button is a pin icon that looks pressed when on.
- **Edge docking**: Floating, Left, Right, Top or Bottom, opened by hover, click or both, with hover and leave
  delays. A docked Panel folds into a thin strip. Undocking brings back its earlier size.
- **The Picker** (`Ctrl+Shift+Space`), the template fill form with a live preview, and quick capture
  (`Ctrl+Shift+2`). `Alt+1` to `Alt+9` use that row. **Paste next** and **paste stack** (Supporter).
- **The Main window**: boards, lists, tags, Pinned, Recent and Trash in a sidebar; compact, comfortable and
  tile views; search; multi-select; move to list; tags; colour labels; list icons; drag and drop; keyboard
  reordering; a **Hide Panel / Show Panel** button; Duplicate (`Ctrl+D`); Export as text.
- **Undo and redo** for library changes: `Ctrl+Z`, `Ctrl+Y` and buttons in the header and toolbar. Deleting
  also shows an Undo button for at least 5 seconds.
- **The item editor** for text, code, link, template, image and file items. It opens at the height of its
  form, and unsaved changes are kept as a draft that survives a crash. Editors for boards, lists and tags.
- **The Trash** keeps deleted items, lists and boards for 30 days.
- **Settings**: hotkeys with conflict messages, theme, accent colour, language, window behaviour, start with
  Windows, tray, never-paste-into programs, licence, backups, export, import and erase. **Reduce motion** also
  follows Windows.
- **Import** from a `.snipdeck` file, Ditto, CopyQ, a folder of `.txt` and `.md` files, and Windows clipboard
  history, always after a preview. Export of the whole library, daily backups (1 to 60 kept) and restore.
- **Recent inbox** (off by default): the last 200 copies or 7 days, skipping password managers and your
  exclude list, with **Save to…**, **Pause history** and **Clear recent history**. Entries are encrypted on disk.
- **Sensitive items**: hidden in lists until you hold `Space`, kept out of Windows clipboard history, encrypted
  on disk on every plan, and cleared from the clipboard after 30 seconds.
- **Supporter**: pay what you want from A$5 (suggested A$8), once. Adds unlimited boards, templates that ask
  for values, paste next, paste stack and a **master password**. Free has up to 5 boards.
- Templates with `{date}`, `{time}`, `{clipboard}`, `{cursor}`, and fields and choices (Supporter).
- A command palette (`Ctrl+K`), a shortcuts sheet (`Ctrl+/`), a welcome tour, a Getting started list, Help
  (`F1`) with a Troubleshoot checklist and About, and **Take screenshot** in the Panel, tray and palette.
- A tray icon with a translated menu. Left click toggles the Panel and double-click opens the Main window.
- Overlay scrollbars on every scrolling surface.
- Spanish, German, French, Brazilian Portuguese, Japanese and Simplified Chinese.
- Screen-reader names on every icon button, status messages that are announced, a focus ring, and text that
  follows the Windows text size.
- `Snipdeck.bat`: double-click to download the latest release, check its SHA-256, unpack and start; later it
  starts offline. Also `update`, `reinstall`, `backup`, `restore`, `doctor`, `logs` and `uninstall`.
- `--check-db` and `--troubleshoot`. Crash reports (newest 10 kept) that never quote your snippets.
- Releases carry a SHA-256 file and build provenance (`gh attestation verify`).

- Help > Troubleshoot has **Rebuild from backup…** for a damaged database: it sets the old file aside and
  recovers the items it can read.
- The Main window toolbar is grouped by function, the Panel's board menu has New board, Rename board and
  Delete board, and the Help tools row is tidied.

### Fixed
- **Open data folder** in Settings and Help opens the library folder again.
- The first paste after a start pasted the previous clipboard content. The clipboard write is now confirmed
  before the paste key is sent.
- A window opened by the Panel or Main window no longer stays behind when you close it. A blur-hide of the
  Panel no longer closes windows it opened.
- Deleting a list or board shows one confirmation. Windows open without a white flash and beside the window
  that opened them.
- No see-through window after Alt+Tab or restore.
- "Dock to the nearest edge" never picks the taskbar's edge.
- The library syncs every commit to disk, so a power cut cannot roll back an edit or a Trash purge.

### Security
- No network access. Licence keys are checked offline against a public key built into the release.
- Imported archives are size-limited; the launcher checks the download's SHA-256 against the release and a
  pinned list, and starts the app without `cmd.exe`.

Windows behaviour (focus, tray, IME, DPAPI, scaling) was developed on Linux and checked on a real PC only in
part before this release.

## Development history before 1.0.0

Internal milestones, kept for reference.

**M3: UI foundation, Panel and app shell**
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

**Earlier, changed**
- The GPL licence is GPL-3.0-only (was AGPL in the first draft).
- Windows paste restores the previous clipboard when the target has asked for the data (delayed rendering)
  instead of after a fixed 300 ms wait.
- Inline text editing is capped at 16 KB and search matches title, tags and the first 64 characters, from
  the S3 measurements.

**Earlier, fixed**
- Findings of the independent reviews of M1 (29 in core and store), the M3 security review, the M3 component review
  and the Win32 tray review are fixed or tracked; see [`docs/reviews/`](docs/reviews/).
- `Cargo.toml` `repository` points at `github.com/Braydenh563/Snipdeck` (the project was renamed from Backpack).

**Earlier, security**
- Importers and the archive reader refuse decoy zip records, oversized values and views in Ditto databases,
  count memory while reading, keep imported sensitive payloads out of the blob folder and scrub them on
  rollback (second M3 security review).

[Unreleased]: https://github.com/Braydenh563/Snipdeck/commits/main
