# Troubleshooting

Snipdeck is a work in progress and most Windows behaviour has not been tested on Windows, so some of
these are known limits rather than faults. If your problem is not here, open an issue with the bug report
template (never post a security problem there: see [`../SECURITY.md`](../SECURITY.md)).

## Starting Snipdeck with Snipdeck.bat

`Snipdeck.bat` (no arguments) starts `app\snipdeck.exe`, and downloads the latest release first when it is missing.
It does not need Rust, Visual Studio or git. Every failure prints what happened, what to do and the log file path
(`%LOCALAPPDATA%\Snipdeck\logs`).

**"Snipdeck needs the internet once".** The app is not on this PC yet and GitHub could not be reached. Connect and run
`Snipdeck.bat` again. Behind a VPN, proxy or firewall, allow `github.com` and `objects.githubusercontent.com`.

**"There is no published release yet" (404).** No release has been published. Nothing to fix on your side; developers
can use `Snipdeck.bat build`.

**"Its SHA-256 does not match".** The download was damaged or changed. Snipdeck deleted it and did not run it. Run
`Snipdeck.bat` again; if it happens twice, do not use the file and report it.

**"Only the published .sha256 file was used".** Not an error: `tools\launcher\release-sha256.txt` has no pinned hash for
that release, so only the checksum file published with the release was compared.

**Update says Snipdeck is running.** Quit it from its tray icon and run `Snipdeck.bat update` again.

## Building and running

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

## Checks and tests

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

## The app

**Nothing happens when I press `Ctrl+Shift+Space` or `Ctrl+Shift+2`.** Expected today: the Picker and capture
hotkeys are registered but their features are not wired ([`user-guide.md`](user-guide.md#hotkeys)).

**A hotkey does not work at all.** Another program may own the chord. Snipdeck reports hotkeys that fail
to register and still registers the rest. Change the chord in `settings.json` (close Snipdeck first).

**I cannot add, edit or delete items.** Not built yet; the app only shows and uses a library. The
library code (store, import) works, but no screen drives it.

**Using a template shows "Fill in this template first."** The template has `{field:...}` or `{choice:...}`
placeholders and the fill form is not wired to the Panel yet. Static placeholders (`{date}`, `{time}`,
`{datetime:...}`, `{cursor}`) work.

**A click copies instead of pasting.** Snipdeck copies when it does not know a text-input Target, when the
Target is an elevated (administrator) window, or when the app is in the `no_paste_apps` list. The footer
shows where a click will go.

**Alt+click or releasing the Windows key does something odd in the target app.** A known issue found in the
first Windows run (S1 step 2c): the modifier is not masked yet. See [`spikes/README.md`](spikes/README.md).

**The tray icon is a generic icon, or its menu is English and light.** Known gaps: the executable has no icon
resource yet, the menu is not translated and ignores dark mode.

**Closing the Panel makes Snipdeck disappear.** With a tray icon, closing hides to the tray (setting
`close_to_tray`); left-click the tray icon, or press `Ctrl+Alt+Space`, to bring it back. Without a tray icon,
closing quits.

**A second launch does nothing.** Snipdeck is single-instance: the second copy hands over to the first and
exits.

**"Locked" when opening the library.** Another Snipdeck holds `snipdeck.lock` in the data folder. Close it.
The lock is released by the system if the program dies, so a stale lock should not need deleting. A library
in a synced folder (OneDrive, Dropbox) must not be open on two computers at once.

**My settings were reset.** If `settings.json` could not be parsed, Snipdeck moved it to
`settings.json.bad` in the data folder and used what it could read. Fix the `.bad` file and rename it
back. If the file could not be read or moved aside, Snipdeck runs on defaults and does not write the
file, so your copy stays untouched.

**Where is my data?** `%LOCALAPPDATA%\Snipdeck` on Windows, `$XDG_DATA_HOME/snipdeck` or
`~/.local/share/snipdeck` elsewhere, or the folder given with `--data-dir`. After a database upgrade a
`snipdeck.db.pre-vN.bak` copy stays next to it; it can hold plaintext of items made sensitive later, so
delete it once the new version runs well ([`store.md`](store.md)).

## The Windows test wizard

See [`../tools/windows-test/README.md`](../tools/windows-test/README.md#troubleshooting): "running scripts is
disabled" (use the `.bat`), a build tools install that needs the UAC prompt, an app that exits at once (press
`l` for its output), and antivirus on freshly built, unsigned `.exe` files. Do not answer `y` to "commit results?" in
a dry run; keep real result files out of the repo unless they came from a real Windows run.
