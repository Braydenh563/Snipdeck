# Troubleshooting

Start with `Snipdeck.bat doctor`. It checks this PC and your data and writes a report you can send. Most
launcher failures print what happened, what to do, and the log file path.

If your problem is not here, open an issue with the bug report template. Never post a security problem
there: see [`../SECURITY.md`](../SECURITY.md). What to attach is listed at the end of this page.

## Starting Snipdeck with Snipdeck.bat

`Snipdeck.bat` (no arguments) starts `app\snipdeck.exe`. If the app is missing, it downloads the latest
release first. Every step is safe to repeat.

**"Snipdeck needs the internet once".** The app is not on this PC yet and GitHub could not be reached.
Connect and run `Snipdeck.bat` again. Behind a VPN, proxy or firewall, allow `github.com` and
`objects.githubusercontent.com`. If the download is blocked or cut off, try another network, then run it
again. An antivirus that scans the folder can also block it: wait a minute and retry.

**"There is no published release yet" (404).** No release is published, so there is nothing to download.
Wait for the first release, then run `Snipdeck.bat` again. If this folder holds the source code, the launcher
builds the app from source instead; that needs Rust and the Visual C++ build tools.

**GitHub answered 403 or 429.** GitHub is limiting requests from your network. Wait about an hour, or try
another network.

**"Its SHA-256 does not match".** The download was damaged or changed. Snipdeck deleted it and did not run
it. Run `Snipdeck.bat` again. If it fails twice, do not use the file and report it.

**"No pinned hash for ..." (unpinned release).** `tools\launcher\release-sha256.txt` has no pinned hash for
this release, so the only check is the `.sha256` file from the same release. The launcher asks before it
downloads, and refuses when nobody is at the keyboard (`-Yes` does not count as an answer). If you trust the
release, answer `y`, or run `Snipdeck.bat update --allow-unpinned`. The option works for `run`, `demo`,
`update` and `reinstall`.

**"Snipdeck is running" when updating.** Quit it from its tray icon and run `Snipdeck.bat update` again.

**The app will not start, or the `app` folder is damaged.** Run `Snipdeck.bat reinstall`. It closes the app,
downloads it again and swaps the folder in. If the download fails, the old folder stays. Your library is not
touched. The same is available by double-clicking
`tools\launcher\commands\Reinstall Snipdeck (keep my data).bat`.

**"The app folder could not be replaced".** Quit Snipdeck from the tray. If an antivirus is scanning the
folder, wait and run the command again.

## The Panel and hotkeys

**I cannot see the Panel.** It is probably hidden, not gone. Closing the Panel (or `Esc` with an empty
search) hides it to the tray. Press `Ctrl+Alt+Space`, left-click the Snipdeck icon in the notification area
(it may be under the `^` arrow), or right-click it and choose **Open Panel**. If none of that works, the
hotkeys may be paused: choose **Pause hotkeys** in the tray menu to switch them back on. If there is no tray
icon, closing the Panel quits Snipdeck, so start it again with `Snipdeck.bat`. Starting a second copy also
brings the running one forward.

**The Panel is not in the taskbar or Alt+Tab.** That is the default. Turn on **Show Snipdeck in the taskbar**
under Settings, **Window behaviour**.

**The Panel hides when I click another app.** **Hide the panel when it loses focus** is on by default. Pin the
Panel with its keep-on-top button, or turn the setting off.

**Closing a window quits Snipdeck.** Turn on **Keep running in the tray when the window closes** under
Settings, **Window behaviour**. With it off, closing the Panel or the Main window quits.

**The Panel is a thin strip on the screen edge.** It is docked and folded. Hover over the strip or click it
(depending on **Open a docked panel by**), or press the Panel shortcut. To undock, set **Dock to** to Floating
under Settings, **Panel dock**.

**A sensitive item shows no text.** Lists hide it. Hold `Space` on its row in the Main window to reveal it.

**The Panel opens off screen or on the wrong monitor.** Snipdeck remembers the Panel's size and place for each
monitor and keeps it inside the monitor. After you unplug a monitor, press `Ctrl+Alt+Space` once. If it is
still out of reach, quit from the tray and start Snipdeck again.

**A toast says a hotkey "is used by another app".** Another program already owns that combination, so
Snipdeck could not register it. The other hotkeys still work. Open Settings, **Shortcuts**, click the field
and press a different combination. Settings also refuses a combination that two Snipdeck actions would
share, and one that Windows does not allow as a shortcut.

**A hotkey does nothing.** Check that the hotkeys are not paused (tray menu, **Pause hotkeys**). Then check
Settings, **Shortcuts**: an empty field means the hotkey is off.

**A click copies instead of pasting.** Snipdeck copies when it does not know a text box to paste into, when the
target runs as administrator, or when the program is in **Never paste into** in Settings. The message says
which. The Panel footer shows where a click will go.

**Ctrl+Alt+V says "This item uses a Supporter feature".** Paste next is a Supporter feature. Enter a key in Settings,
**Licence**.

**Using a template says "This template can't be read".** A placeholder in it is wrong. Open the item and fix
it. The editor lists the placeholders it understands.

## Crashes and logs

If Snipdeck closes unexpectedly, the next start shows "Snipdeck closed unexpectedly last time. Open the
log?". Press the button to open the folder. The reports are `crash-<date>-<time>.txt` in
`%LOCALAPPDATA%\Snipdeck\logs`. The newest 10 are kept. A report holds the app version, where it failed
and a backtrace. It never holds your snippets, and nothing is sent anywhere. You can also run
`Snipdeck.bat logs --open`.

## Your library

**"Snipdeck found a problem in your library".** A weekly check found something wrong. Export a copy first
(Settings, **Your data**, **Export library**), then restore from a backup if items are missing.

**Restore a backup.** In Settings, **Backups**, press **Restore** next to a date. This adds back what is
missing and never removes anything, so it is safe. Snipdeck keeps the newest 7 daily backups in the
`backups` folder of the data folder.

**Restore everything as it was.** Quit Snipdeck, then run `Snipdeck.bat restore` and choose a backup from
`Documents\Snipdeck-backups` (make one any time with `Snipdeck.bat backup`). It saves your current data
first. The app must be closed while it runs.

**My settings were reset.** If `settings.json` could not be read, Snipdeck moved it to `settings.json.bad`
in the data folder and used what it could read. Fix the `.bad` file and rename it back.

**"Locked" when opening the library.** Another Snipdeck holds `snipdeck.lock` in the data folder. Close it.
The system releases the lock if the program dies, so you should not need to delete it. A library in a synced
folder (OneDrive, Dropbox) must not be open on two computers at once.

**Where is my data?** `%LOCALAPPDATA%\Snipdeck`, or the folder given with `--data-dir`. After a database
upgrade a `snipdeck.db.pre-vN.bak` copy stays next to it. It can hold plain text of items you made sensitive
later, so delete it once the new version runs well ([`store.md`](store.md)).

**Check the library from a Command Prompt.** With Snipdeck closed, run `app\snipdeck.exe --check-db` to test
the library for damage (exit code 0 healthy, 1 problems, 2 could not run), or `app\snipdeck.exe --troubleshoot` to
print the Troubleshoot report. Add `--data-dir <folder>` for a library somewhere else.

## Locked library and the master password

These apply when you have set a master password (a Supporter feature). Your other items always work. Only
sensitive items are locked.

**"Unlock your library to use this item" or "Unlock your library to save this item".** The library is
locked. It locks when Snipdeck starts and after 15 minutes without use. Type your master password in the
**Unlock Snipdeck** window. If the window does not appear, use the item again. You can also choose
**Unlock…** under **Privacy** in Settings.

**"Your library locked after a while. The sensitive item you were editing was closed without saving."** The 15
minutes ran out while the editor was open. Unlock, then open the item and type your change again.

**"Unlock your library first, so your sensitive items are included. Nothing was done."** Back up now, Export
library and Restore do not run on a locked library. A copy made then would miss your sensitive items.
Unlock, then try again.

**Backups are deferred while locked.** The daily backup waits when the library is locked, and the Troubleshoot
checklist says "library locked, backup deferred". Nothing is lost. Snipdeck tries again later, and the backup
is made once you unlock. To make one sooner, unlock and press **Back up now**.

**I forgot my master password.** There is no way to get it back. Snipdeck cannot reset it and keeps no
recovery key. Backups and exports hold your sensitive items under the same key, so they cannot be opened
either. Try the passwords you may have used. Your other items are not affected, and they still work in the
app. Snipdeck will not export or restore while the library is locked, so to keep your other items, use
**Export as text** on each list (see the [user guide](user-guide.md#organise)). It leaves sensitive items
out. If you give up on the sensitive items, **Erase library** in Settings with **Also forget the encryption
key** ticked removes the whole library and the key. Then add your items again. Read the warning in that window
before you press **Erase everything**.

**"Couldn't open the encryption key. Nothing was saved."** (Also: "Snipdeck couldn't open the encryption key,
so nothing was saved.") Snipdeck could not read the key that Windows protects. This can happen if the data
folder came from another Windows account or PC, or if Windows could not unprotect the key. Restart Snipdeck
and try again. If it keeps happening, your other items still work. Sensitive items made under another
Windows account cannot be opened here. Run `Snipdeck.bat doctor` and attach the report to a bug report.

**The master password row says Snipdeck can't protect a key on this PC, or couldn't read your key.** The first
means Windows cannot protect a key here, so a master password is not available. The second is a read failure:
restart Snipdeck to try again.

## High DPI and several monitors

- The Panel is 260 to 480 pixels wide at 100% scale. Drag its edges to resize it. Its size and place are
  remembered per monitor.
- The size limits follow the display scale of the monitor the Panel is on.
- A docked Panel's edge, the strip's hit area and the taskbar buttons have not been tested on Windows either.
- The Panel's area is the whole monitor, not the area without the taskbar, so it can sit under the taskbar.
  Move it up if that happens.
- Scaling, resizing and moving between monitors with different scales have not been tested on Windows. If
  text is blurry, cut off or the wrong size, report it with your scale setting for each monitor.

## How to report a bug

Open an issue with the bug report template and attach:

- What you did, what you expected and what happened.
- The Snipdeck version and your Windows version.
- The report from `Snipdeck.bat doctor` (it hides your user name). It is saved in
  `%LOCALAPPDATA%\Snipdeck\logs`.
- The newest `crash-*.txt` from the same folder, if Snipdeck closed on its own.
- For display problems: your scale setting and monitor layout, and a screenshot with private text hidden.

Do not attach your library or a backup unless you have looked at what is in it.

## For developers

### Building and running

**`cargo` is not found (Windows).** `run-snipdeck-demo.bat` stops with "Rust is not installed". Run
`run-windows-tests.bat` once (it offers to install Rust and the Visual Studio C++ Build Tools with `winget`),
or install them yourself from <https://rustup.rs>, then close the window and run the batch file again so the
new PATH is picked up.

**The first build is slow and `target/` is huge.** Slint and a bundled SQLite are compiled from source,
and a debug `snipdeck-ui` build is about 4 GB. Set
`CARGO_TARGET_DIR` to a scratch folder and delete it afterwards. Out-of-disk errors mid-build are this.

**`release builds must embed the production licence public key`.** Release builds refuse to compile without
`SNIPDECK_LICENCE_PUBLIC_KEY` on purpose (the development key must never ship). Run the demo as a debug build:
`run-snipdeck-demo.bat` or `cargo run -p snipdeck-app -- --demo` (no `--release`).

**Linker or C compiler errors.** Windows needs the Visual Studio C++ Build Tools; Linux needs a C compiler.
`rust-toolchain.toml` selects stable; the minimum version is `rust-version` in `Cargo.toml`.

**The demo window does not open on Linux.** The window needs a display (X11 or Wayland). Without one, render
a picture instead: `cargo run -p snipdeck-app -- --screenshot panel.png` (add `--dark` for the dark theme).

**Fonts look wrong or a test says it skipped a CJK check.** The UI bundles IBM Plex. CJK text uses a system
font; where none is installed (CI, minimal containers) those width checks skip themselves.

**`--demo` pasted into my window.** On Windows the demo uses the real clipboard and keyboard, so clicking an
item can really paste into the app you were using. Click an item with the target you want focused, or use
`Shift`+click to copy only. (Off Windows, the demo's clipboard and target are fakes.)

### Checks and tests

**`scripts/gate.sh` says the code map is stale.** Run `python3 scripts/codemap.py` and commit
`docs/CODEMAP.md`.

**`scripts/gate.sh` says the translation catalogue is stale.** Run `python3 scripts/i18n.py extract`, commit
the result, then re-run the gate. Missing translations are a warning, not a failure.

**A golden image test fails.** Look at the difference. If the change is intended, run
`UPDATE_GOLDEN=1 cargo test -p snipdeck-ui`, open each changed PNG to check it, and commit. If it is not
intended, a token, a font or a layout changed by accident. Goldens are rendered by the software renderer, so
a GPU or OS cannot cause a difference; a different font on the machine can.

**Clippy fails in CI but not locally.** CI uses the newest stable Rust, which has lints older versions lack.
Check with `cargo +1.99.1 clippy --workspace --all-targets --locked -- -D warnings`.

**`cargo deny` fails.** A new dependency has a licence that is not GPL-3.0-compatible, an advisory, or comes
from outside crates.io. `deny.toml` explains the four advisories that are ignored on purpose.

**A Windows check from Linux fails in a C build script.** Provide a stand-in archiver, as described in
[`../CONTRIBUTING.md`](../CONTRIBUTING.md#testing-on-windows).

**Slint tests panic or hang on startup.** Check whether a test file mixed the screenshot `Harness`
with the testing backend. Keep them in separate files ([`development.md`](development.md#test-strategy)).

### The Windows test wizard

See [`../tools/windows-test/README.md`](../tools/windows-test/README.md#troubleshooting): "running scripts is
disabled" (use the `.bat`), a build tools install that needs the UAC prompt, an app that exits at once (press
`l` for its output), and antivirus on freshly built, unsigned `.exe` files. Do not answer `y` to "commit results?" in
a dry run; keep real result files out of the repo unless they came from a real Windows run.
