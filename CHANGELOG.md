# Changelog

All notable changes to Snipdeck are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/) from 1.0.0.

Snipdeck 1.0.0 is not released yet. "Unverified" marks Windows behaviour that no one has run on a real
Windows PC.

## [Unreleased] (1.0.0)

### Changed
- The Windows test wizard is now one file at the repo root, `Test Snipdeck on Windows.bat` (it replaces
  `run-windows-tests.bat`): a four-line welcome, Enter for the recommended "v1 check", plain-language steps with
  one layout (`DO THIS`, `YOU SHOULD SEE`, P / F / S), prerequisites installed after one question, a resume question,
  and one `Snipdeck-test-results-<date>.zip` on the Desktop at the end. Unverified on Windows.

### Added
- `Snipdeck.bat` installs by itself: it downloads the latest release once, checks its SHA-256, unpacks it to an
  `app` folder and starts it. After that it starts offline.
- Launcher commands `update`, `reinstall`, `backup`, `restore`, `doctor`, `logs` and `uninstall`, and one
  double-click `.bat` per command in `tools\launcher\commands`.
- When no release exists yet and the folder holds the source code, `Snipdeck.bat` builds the app from source.
- The Main window: sidebar with boards, lists, tags, Pinned and Trash; compact, comfortable and tile views;
  search; multi-select; move to list; tags; pin; and keyboard reordering of items and lists.
- Boards and lists can be created, renamed, coloured, sorted, reordered and deleted.
- The Trash keeps deleted items, lists and boards for 30 days. Deleting shows an Undo button for about 5 seconds.
  Items can be restored, deleted for good, or the Trash emptied.
- The Panel menu (three dots): Open Snipdeck, New snippet, Settings, Pause hotkeys and Quit.
- Resize the Panel by dragging its edges (260 to 480 pixels wide). Its size and place are remembered for each
  monitor.
- The Panel's keep-on-top button, tooltips on every icon button, and mouse-wheel scrolling of the pinned strip.
- Delete, `Ctrl+E` and `F2` on a Panel row.
- A Settings window: hotkeys with conflict messages, theme, language, panel width, start with Windows, keep
  running in the tray, never-paste-into programs, licence entry, backups, export and erase.
- Daily automatic backups (newest 7 kept), with restore in Settings.
- A weekly library check, and a one-time notice after a crash with a button that opens the log folder.
- Crash reports in `%LOCALAPPDATA%\Snipdeck\logs` (newest 10 kept).
- The Snipdeck icon on the app and in the tray, and a translated tray menu.
- Translations for Spanish, German, French, Brazilian Portuguese, Japanese and Simplified Chinese.
- The Picker, the template fill form and quick capture (`Ctrl+Shift+2` saves the clipboard text as a new item).
- Paste next (`Ctrl+Alt+V`) for Supporter, over the pinned strip or the current list.
- The item editor for text, code, link, template, image and file items, with drafts that survive a crash.
- Export of the whole library as one `.snipdeck` file from Settings.
- `snipdeck-licence-issuer`, a tool for making licence keys.
- Edge docking for the Panel: **Dock to** (Floating, Left, Right, Top or Bottom edge), **Open a docked panel by**
  (Off, Hover, Click or Hover and click), **Hover delay** and **Leave delay**. A docked Panel can fold into a thin
  strip. The Panel shortcut folds and unfolds it too. The edge is remembered for each monitor.
- A **Window behaviour** group in Settings: **Show Snipdeck in the taskbar**, **Hide the panel when it loses
  focus**, **Start with Windows** and **Keep running in the tray when the window closes**.
- **Backups to keep** in Settings: a stepper from 1 to 60 (default 7) for the daily backups.
- A toast window that shows messages from the Picker and the fill form. Sensitive items stay hidden in Panel and
  Main window lists until you hold `Space`.
- A Panel list tab menu: right-click a tab, or press `Menu`, `Shift+F10` or `F2`, to edit or delete that list.
  Double-click a tab to edit it.
- A Panel item menu (right-click, `Menu` or `Shift+F10`) with Use, Copy, Edit, Pin and Move to Trash. Hold `Space`
  on a Panel row to reveal a sensitive item. The fill form masks sensitive fields.
- The first time the Panel hides, a toast says Snipdeck is still running and how to bring it back.
- `{date}` and `{time}` in templates use your local time, and `{clipboard}` inserts the clipboard text. A sensitive
  item you only copied is cleared from the clipboard after 30 seconds.
- The Main window remembers its size and place. The tray tooltip says when hotkeys are paused. Menus show a
  focus ring.
- A Help menu in the Main window, `F1` for the Help window, and a link to the welcome tour in About.
- A welcome tour on the first start. **Show the welcome tour again** in Help replays it.
- A Help window with a **Troubleshoot** checklist and **About**.
- **Ctrl+Z** undoes the last delete. The Undo message stays at least 5 seconds and waits while the pointer is on
  it.
- New shortcuts: `Ctrl+N` (new item), `Ctrl+Shift+N` (new list), `Ctrl+F` (search), `Ctrl+W` (close or hide) and
  `Ctrl+,` (Settings), in the Main window and the Panel.
- **Duplicate** in the item menu of the Main window.
- **Export as text** in a list's menu writes the list's text items to a `.txt` file in Documents.
- A **More** (⋯) button on rows and cards opens the item menu for a mouse or touch screen.
- A **Getting started** list on the first run, with four cards that show how to use Snipdeck.
- A double-click on the tray icon opens the Main window.
- **Reduce motion** in Settings. It is also on when Windows has Animation effects off.
- **Clear the clipboard after copying a sensitive item** in Settings (30 seconds by default, 0 never clears it).
- **Accent colour** in Settings: Blue, Teal, Green, Amber, Rose, Violet or the Windows accent.
- **Command palette** (`Ctrl+K`) and a shortcuts sheet (`Ctrl+/`), with shortcut hints in menus and tooltips.
- **Paste stack** (Supporter): mark items, press **Start stack**, then `Ctrl+Alt+V` pastes them one by one.
- **Master password** (Supporter) for sensitive items. Snipdeck asks for it when you first need a sensitive item
  and again after 15 minutes without use. There is no recovery if you forget it.
- **Supporter**: pay what you want from A$5 (suggested A$8), one time. **Get Supporter…** in Settings, and a
  Free and Supporter table that shows what each plan has.
- **Import** in Settings ▸ Your data: a `.snipdeck` file, a Ditto database, a CopyQ export or a folder of `.txt`
  and `.md` files. A preview says what will be added, what is already there and what will be left out, and why.
  Restoring a backup shows the same preview. On Windows, **Windows clipboard history** is a fifth source: the
  text still in the Win+V history of this sign-in (unverified on Windows).
- **Recent inbox**, off by default: the text you copy, for the last 200 copies and 7 days, in the Main window's
  sidebar and a clock tab in the Panel. **Save to…** puts a copy in a list. Sensitive items, password managers
  and programs on your exclude list are skipped. **Pause history** in the tray and **Clear recent history**
  in Settings stop or empty it. It is also offered in the welcome tour.
- **Colour labels** (eight colours, with a filter) on items, and an **icon** for each list.
- Drag and drop in the Main window: reorder items and lists, and drop an item on a list to move it. In the
  Panel, drop an item on a list tab to move it, or drop text on the Panel to save it as a new item.
- `Ctrl+D` duplicates the selected item in the Main window and the Panel, and **Duplicate selected item** is in
  the command palette.
- Messages such as "Copied" and "Pasted into …" follow the language you chose.
- Screen-reader and keyboard polish: tooltips and names on every icon button and rail item, status texts that
  are announced, arrow keys in radio groups, `Tab` closes a menu, and a clear focus ring on Panel rows.
- `--check-db` checks the library for damage and exits. `--troubleshoot` prints the Troubleshoot report and exits.

### Changed
- The Free plan now has up to 5 boards. The paid plan is called Supporter.
- Encrypting sensitive items on disk is free on every plan. Supporter adds the master password.
- Quick start is now "get the folder, double-click `Snipdeck.bat`". Users no longer need Rust, Visual Studio or git.
- `Snipdeck.bat update` downloads the newest release instead of running `git pull`. The source build moved to
  `Snipdeck.bat build` and is for developers.
- The Windows build no longer needs the Visual C++ Redistributable.
- The Panel is raised each time it is shown, and the tray's left click toggles it.
- A new library starts with a board called "My snippets" and a list called "Snippets".
- The Panel is out of the taskbar and Alt+Tab by default, and hides when you switch to another app unless it is
  pinned. Both can be changed under **Window behaviour**.
- Settings now fits the screen. The Item editor, Settings and Main window open as normal windows with
  their own taskbar buttons.
- "Delete for good" is the wording everywhere an item is removed for good, and the item menu says **Pin** or
  **Unpin** to match the item.
- Searching a large library stays quick, and typing a master password no longer freezes the window.
- The icon-fetching "website icons" setting is gone. Snipdeck never goes online, so icons are packaged.

### Fixed
- A backup is no longer made while the encrypted-items key is locked. That backup would have left sensitive
  items out, so it is tried again later.
- A crash report no longer quotes snippet text. Report file names use UTC, so a daylight-saving change cannot
  hide a new report.
- `Snipdeck.bat` stops a folder name or argument with special characters (`&`, `^`, `|`, `%`) from running as a
  command.
- Backup, restore and clean refuse to run while the downloaded app is running.
- Launcher logs hide your user name more thoroughly.
- **+ New item** in the Main window opens the editor.
- The tag manager in the Main window now renames and deletes tags.

### Security
- The launcher checks the SHA-256 of the download against the release and against a pinned list. A release
  with no pinned hash needs an explicit yes, or `--allow-unpinned`, and is refused when unattended.
- `reinstall` closes the app and keeps the old folder until the new one downloads and verifies.
- The launcher starts the app without `cmd.exe`, and downloaded installers run from a private folder after a
  signature check.
- `restore` accepts only a zip that holds `snipdeck-backup.json`.
**v1 push (2026-10-18)**
- Main window, item/list/board editors, tag manager and Settings page (`snipdeck-ui`), merged from the main-window branch.
- Panel: the "More" (kebab) menu now works (Open Snipdeck, New snippet, Settings, Pause hotkeys, Help, Quit), the edge and corner grips resize the window (260 to 480 wide, remembered per monitor in `panel_geometry`), and Delete / Ctrl+E / F2 on a row act (Delete goes to the Trash with a 5 s Undo). Written on Linux; the OS resize and the per-monitor restore are unverified on Windows.
- Item editor: New snippet (Panel menu, Main window, Ctrl+N) and Edit (Panel quick edit, Picker Ctrl+E, Main window) open the item editor for text, code, link, template, image and file items. A new Image or File item takes the path of a file (typed, or chosen with Browse on Windows) and keeps its own copy. The first sensitive item on Pro makes the DPAPI-wrapped library key without a prompt and is stored encrypted; text over 16 KB opens read-only; unsaved changes are kept as a draft every 2 s and come back when the editor is opened again (never for a sensitive item); Free users are told prompted placeholders are Pro. The crash toast's "Open log" button opens the reports folder. Written on Linux; DPAPI and the Open dialog are unverified on Windows.
- Panel: the always-on-top toggle is a distinct `push-pin-simple` icon with a stable "Keep panel on top" label and tooltips on the header buttons.
- `snipdeck-licence-issuer` (tools/): `genkey`, `sign`, `verify` for friends' licence keys; the signer moved into `snipdeck-core` behind the `licence-issuer` feature.
- GitHub: RustSec `cargo audit` daily, Dependabot (cargo + actions), and a Windows portable-zip build (dev build on main, release build with SHA-256 on `v*` tags).
- `docs/reviews/fable-analysis.md`: ranked findings, architecture redesign, UX, security/privacy and feature brainstorm.

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
- Findings of the Opus reviews of M1 (29 in core and store), the M3 security review, the M3 component review
  and the Win32 tray review are fixed or tracked; see [`docs/reviews/`](docs/reviews/).
- `Cargo.toml` `repository` points at `github.com/Braydenh563/Snipdeck` (the project was renamed from Backpack).

**Earlier, security**
- Importers and the archive reader refuse decoy zip records, oversized values and views in Ditto databases,
  count memory while reading, keep imported sensitive payloads out of the blob folder and scrub them on
  rollback (second M3 security review).

[Unreleased]: https://github.com/Braydenh563/Snipdeck/commits/main
