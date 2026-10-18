<div align="center">

<picture><source media="(prefers-color-scheme: dark)" srcset="assets/logo/snipdeck-logo-dark.svg"><img src="assets/logo/snipdeck-logo-light.svg" alt="Snipdeck" height="88"></picture>

**Your reusable snippets, one click from any app.**

[![CI](https://github.com/Braydenh563/Snipdeck/actions/workflows/ci.yml/badge.svg)](https://github.com/Braydenh563/Snipdeck/actions/workflows/ci.yml)
![Platform: Windows](https://img.shields.io/badge/platform-Windows-2870D4)
[![Licence: GPL-3.0-only](https://img.shields.io/badge/licence-GPL--3.0--only-2ea44f)](LICENSE)

[Install](#install-and-first-run) · [The three windows](#the-three-windows) · [Your data](#your-data) · [Licence](#licence) · [User guide](docs/user-guide.md) · [Troubleshooting](docs/troubleshooting.md)

</div>

Snipdeck is a snippet deck for Windows. You keep the text, links, templates, images and files you paste
all the time on a small panel at the edge of the screen. Click one and it lands in the app you were typing
in. The panel never takes focus away from that app.

You arrange the deck yourself: boards hold lists, lists hold items. It is not a clipboard history and it
does not need abbreviations.

| Panel, light | Panel, dark |
|---|---|
| ![The Panel in the light theme](docs/images/panel-light.png) | ![The Panel in the dark theme](docs/images/panel-dark.png) |

These are images the test suite renders, not screenshots of a Windows desktop.

> [!NOTE]
> Snipdeck 1.0 is not released yet. The code is tested on Linux. Nobody has run it on a real Windows PC
> beyond the checks in [`docs/spikes/`](docs/spikes/README.md), so expect rough edges on Windows.

## Requirements

- Windows 10 (version 1903 or newer) or Windows 11, 64-bit.
- Windows PowerShell 5.1, which Windows already includes. `Snipdeck.bat` uses it.
- A one-time internet connection to download the app.
- You do not need Rust, Visual Studio or git.

## Install and first run

1. Get the Snipdeck folder: clone the repository, or download and unzip the release zip,
   `snipdeck-<version>-windows-x64.zip`, from [Releases](https://github.com/Braydenh563/Snipdeck/releases/latest).
2. Double-click `Snipdeck.bat`.

On the first start `Snipdeck.bat` downloads the app from the latest release, checks its SHA-256, unpacks it to
an `app` folder next to the `.bat`, and starts it. This is the only time Snipdeck uses the network. After
that it starts straight away, offline.

If no release is published yet and the folder holds the source code (it has a `Cargo.toml`), `Snipdeck.bat`
builds the app from source instead. That needs Rust and the Visual C++ build tools, and it offers to install
what is missing. It is slow the first time.

More launcher commands, typed as `Snipdeck.bat <command>`:

| Command | What it does |
|---|---|
| `update` | Downloads the newest release again. Your library is kept. |
| `reinstall` | Closes the app, removes the `app` folder and installs it again. The old folder stays if the download fails. Your library is kept. |
| `backup`, `restore` | Zip your library to `Documents\Snipdeck-backups`, or put a backup back. |
| `doctor` | Checks this PC and your data and writes a report you can send. |
| `logs` | Shows the newest launcher log. `--open` opens the folder. |
| `uninstall` | Removes the app. Your library is kept. See [Uninstall](#uninstall). |
| `help` | Lists every command and option. |

Do you prefer clicking to typing? The folder `tools\launcher\commands` has one `.bat` file per command
(update, reinstall, uninstall, back up, restore, check my setup, open the logs, help). Double-click one.
Details are in [`tools/launcher/README.md`](tools/launcher/README.md).

## The three windows

| Window | What it is | How to open it |
|---|---|---|
| **Panel** | A small always-on-top window with your current board. Click an item to paste it. | `Ctrl+Alt+Space`, or the tray icon |
| **Picker** | A search box at your text caret. Type, press `Enter`, and the item is pasted. It searches every board. | `Ctrl+Shift+Space` |
| **Main window** | The full manager: boards, lists, items, tags, Trash. | Panel menu (the three dots), then Open Snipdeck; or the tray menu |

Settings opens from the Panel menu and from the Main window.

### Default hotkeys

| Hotkey | Action |
|---|---|
| `Ctrl+Alt+Space` | Show or hide the Panel |
| `Ctrl+Shift+Space` | Open the Picker |
| `Ctrl+Shift+2` | Save the text on the clipboard as a new item at the top of the current list |
| `Ctrl+Alt+V` | Paste the next item of the pinned strip or the current list (Pro) |

You can change each one in Settings. If another program already uses a hotkey, Snipdeck tells you and the
other hotkeys keep working.

### If the Panel is hidden

Closing the Panel, or pressing `Esc` with an empty search box, hides it to the tray. Snipdeck keeps running.
To get it back, do any one of these:

- Press `Ctrl+Alt+Space`.
- Left-click the Snipdeck icon in the notification area (it may be under the `^` arrow).
- Right-click the tray icon and choose **Open Panel**.

If there is no tray icon, closing the Panel quits Snipdeck instead. You can turn "Keep running in the tray"
off in Settings. Starting Snipdeck a second time also brings the running copy forward.

## Your data

Everything is in `%LOCALAPPDATA%\Snipdeck`: the library (`snipdeck.db`), large content (`blobs`), your
settings (`settings.json`), your licence key, backups and crash logs. The app lives in the `app` folder next
to `Snipdeck.bat`, so updating or deleting it never touches your library.

- **Daily backup.** Snipdeck saves a copy of your library once a day to `backups` in the data folder and
  keeps the newest 7. Settings lists them and can restore one. Restoring adds back what is missing and never
  removes anything. You can also press **Back up now**.
- **Export.** Settings, **Your data**, **Export library** saves one `.snipdeck` file (a zip with JSON) in
  your Documents folder.
- **Launcher backup.** `Snipdeck.bat backup` zips the whole data folder to `Documents\Snipdeck-backups`.
- **Trash.** Deleted items stay in the Trash for 30 days.

## Privacy

Snipdeck has no account, no server and no telemetry, and it opens no network port. The only network use is
the launcher's one download. Every icon and font the app shows is packaged with it; no setting makes a
network request. Licence keys are checked on your PC. See [`docs/privacy.md`](docs/privacy.md).

## Licence

Snipdeck is free software under [GPL-3.0-only](LICENSE). It has two tiers:

- **Free:** everything needed day to day, with up to 3 boards.
- **Pro:** more than 3 boards, templates that ask you for values, encryption of sensitive items, and
  paste next.

To enter a key, open Settings, find **Licence**, paste the key (it starts with `SNIP1.`) and press
**Activate**. Pro turns on at once. The key is checked on your PC and nothing is sent anywhere. Details:
[`docs/licensing.md`](docs/licensing.md).

## Uninstall

- `Snipdeck.bat uninstall` removes the app, build files, the start-with-Windows entry and shortcuts. It keeps
  your library. It offers a backup first.
- `Snipdeck.bat uninstall --data` also erases the library. It asks you to type `DELETE`.
- To install again, run `Snipdeck.bat`. It downloads the app and finds the library you kept.

## For developers

Build, test and contribute: [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`docs/development.md`](docs/development.md).
The product is specified in [`docs/SPEC.md`](docs/SPEC.md) and the look in [`DESIGN.md`](DESIGN.md). The
current project state is in [`docs/HANDOVER.md`](docs/HANDOVER.md), and history is in
[`CHANGELOG.md`](CHANGELOG.md). Report a vulnerability as [`SECURITY.md`](SECURITY.md) describes. Everyone
taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
