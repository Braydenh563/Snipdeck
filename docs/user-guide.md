# User guide

How Snipdeck is meant to work, and how much of that exists today. Snipdeck is a work in progress
with no release; read the status tags first.

| Tag | Meaning |
|---|---|
| **Demo** | Works in `snipdeck-app --demo` and is covered by automated tests. Not verified on Windows. |
| **Code only** | The logic is written and tested as a library, or the window exists in the UI crate, but the app has no screen or hotkey for it yet. |
| **Planned** | Specified in [`SPEC.md`](SPEC.md), not built. |

Try it with `run-snipdeck-demo.bat` (Windows) or `cargo run -p snipdeck-app -- --demo`. The demo
runs on a seeded library in memory: two boards (Work, Personal), lists such as "Email replies",
"Addresses" and "Code", and a sample of each item kind. Nothing is saved. On Linux and macOS the clipboard,
keyboard and target window are fakes, so a click copies to a pretend clipboard and pastes into a pretend
Outlook window. On Windows the demo uses the real clipboard, hotkeys and window tracking, so a click may
really paste into the window you were using (unverified on Windows).

## Concepts

- **Board**: a workspace such as "Work" or "Personal". The Free tier allows 3 boards.
- **List**: an ordered group of items inside a board, shown as a tab in the Panel. A list can have a colour.
- **Item**: one thing to paste. It has a title, one or more clipboard formats (text, HTML, RTF, image,
  files), tags and flags (pinned, sensitive). Kinds: text, template, code, link, image, file.
- **Target**: the window and text control you were typing in before you used Snipdeck.
- **Panel**: the small always-on-top window that shows a board. Clicking it does not take focus from the
  Target.
- **Picker**: a keyboard-first popup at the text caret with fuzzy search across boards.
- **Main window**: the full manager (sidebar, item list, detail editor).

## Hotkeys

Defaults; all are meant to be rebindable. They are global, so they work while another app has focus.

| Chord | Action | State |
|---|---|---|
| `Ctrl+Alt+Space` | Show or hide the Panel | Demo (Windows: unverified) |
| `Ctrl+Shift+Space` | Open the Picker at the caret | Registered; does nothing yet (Picker window is **Code only**) |
| `Ctrl+Shift+2` | Capture a screen region (and OCR) | Registered; does nothing yet. **Planned** |
| `Ctrl+Alt+V` | Paste the next item of the paste stack (Pro) | Registered; does nothing yet. **Planned** |

If another program already owns a chord, that hotkey fails to register and is reported; the others still
work. If `settings.json` gives two actions the same chord, the later one is turned off. Chords are changed by editing `settings.json` (see
[Settings](#settings)); there is no screen for it yet.

## The Panel

**Demo.** The header has the board switcher, a pin (keep on top) and a close button; below it are the
search field, the list tabs, a strip of pinned items, the item rows with a number gutter, and a footer
that says where a click will go ("Pastes into Outlook · New message" or "Copies to clipboard").

| Action | Result |
|---|---|
| Click an item | Paste into the Target if a text-input Target is known; otherwise copy and show a toast |
| `Shift`+click, `Shift+Enter` | Copy only |
| Type | Filters the items of the current board (title, tags, first 64 characters of content) |
| `Up` / `Down`, `PageUp` / `PageDown` | Move the selection |
| `Enter` | Use the selected item |
| `1`-`9` | Use the Nth row, while the search field is empty |
| `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next / previous list |
| `Ctrl+F` | Focus the search field |
| `Esc` | Clear the search; with nothing to clear, hide the Panel |
| Click a pinned chip | Use that item |

After a paste the previous clipboard contents are restored. If the Target cannot be pasted into (a
password field, an elevated window), Snipdeck copies instead and says so. Closing the Panel hides it to the
tray when there is a tray icon, otherwise it quits.

**Code only / Planned:** context menus, add-from-clipboard, add list, rename, delete with Undo, drag to
reorder, docking to screen edges, auto-hide, Compact and Comfortable views, the Recent tab, floating lists
(Pro), saved position, `Alt+click` for plain text.

### Tray icon

**Demo** on Windows, unverified. A left click toggles the Panel. The menu has Open Panel, Open Snipdeck
(shows the Panel until the Main window exists), Pause hotkeys, and Quit. The executable has no icon of its
own yet, and the menu does not follow dark mode.

## The Picker

**Code only.** The window exists with tests (see the screenshots in the [README](../README.md#screenshots)):
type to filter across all boards, arrow keys to move, `Enter` to paste, `Shift+Enter` to copy, `1`-`9` to
use a row, `Esc` to dismiss. The app does not open it when you press the hotkey.

## Boards and lists

Boards and lists, their order, colours, trash and search exist in the store and are tested. In the app,
you can switch board and list (Demo). Creating, renaming, reordering, deleting and restoring from Trash
(kept 30 days) have no screens yet; the Main window is **Planned** (milestone M4).

## Templates and placeholders

**Code only.** A template is an item whose text has placeholders, filled in when you use it.

| Placeholder | Result | Tier |
|---|---|---|
| `{date}`, `{time}` | Today's date or the time, in your locale format | Free |
| `{datetime:FORMAT}` | Date and time with a strftime-style format, for example `{datetime:%d %B %Y}` | Free |
| `{clipboard}` | The text on the clipboard when you use the item | Free; not wired in the app yet |
| `{cursor}` | Nothing; marks where the caret ends up | Free |
| `{field:Name}`, `{field:Name\|default}` | Asks you for a value | Pro |
| `{choice:Name\|a\|b}` | Asks you to pick one | Pro |

`{{` and `}}` give literal braces. Anything else in braces is an error, reported when the template is saved.
The supported format letters are in the module docs of `snipdeck-core`'s `template.rs`.

The fill form (the window that asks for `{field}` and `{choice}` values) exists with tests but is not wired:
using a prompted template in the Panel currently shows the toast "Fill in this template first." and
pastes nothing. The demo's "Invoice reminder" item is one. Time zone is UTC until the platform time zone
is read.

## Sensitive items

- **Demo:** a sensitive item shows a lock and a masked preview in the list.
- **Code only:** excluded from history, and in the Pro tier encrypted at rest (XChaCha20-Poly1305, key wrapped by
  Windows DPAPI, optional master password). See [`crypto.md`](crypto.md). There is no screen to mark an
  item sensitive or set a master password.
- **Planned:** reveal on click-and-hold, clipboard auto-clear after N seconds (default 30), unlock prompts.

Titles, tags and search text of sensitive items are not encrypted.

## Import and export

**Code only.** The library code can write and read `.snipdeck` files (a zip with `manifest.json`,
`items.json` and `assets/`; [`format.md`](format.md)), import a folder of `.txt` / `.md` files, a Ditto
database, or CopyQ JSON, and make one local backup per day (newest 7 kept, in `backups/` in the data
folder). There is no screen for any of it, and the daily backup is not scheduled by the app yet.
Windows clipboard history import and CopyQ's binary export are not supported.

## Settings

**Code only.** The app reads `settings.json` from the data folder when it starts without `--demo`. There is no
Settings window; edit the file in a text editor while Snipdeck is closed. Missing or invalid values fall
back to their defaults, and an unreadable file is moved to `settings.json.bad`.

| Key | Default | Meaning |
|---|---|---|
| `hotkeys.toggle_panel`, `.picker`, `.capture`, `.paste_next` | see [Hotkeys](#hotkeys) | Chord text such as `Ctrl+Shift+Space`; empty turns it off |
| `theme` | `"system"` | `"system"` (follows Windows), `"light"` or `"dark"` |
| `language` | `""` | Locale tag such as `de` or `pt-BR`; empty follows Windows |
| `autostart` | `false` | Start with Windows |
| `close_to_tray` | `true` | Closing the Panel hides it when a tray icon exists |
| `panel_width` | `320` | Panel width in pixels, 260 to 480 |
| `no_paste_apps` | `[]` | Executable names (lower case) that never get an automatic paste; Snipdeck copies instead |
| `site_icons` | `false` | Fetch icons for links; the only setting that makes a network call |

Data folder: `%LOCALAPPDATA%\Snipdeck` on Windows (`$XDG_DATA_HOME/snipdeck` or `~/.local/share/snipdeck`
elsewhere), or the folder given with `--data-dir`. It holds `snipdeck.db`, `blobs/`, `backups/` and
`settings.json` ([`store.md`](store.md)). Only one copy of Snipdeck can use a library at a time.

## Command line

```text
snipdeck-app [--demo] [--screenshot <file>] [--dark] [--hidden] [--data-dir <folder>] [--help]
```

`--hidden` starts with the Panel hidden (used for Start with Windows). `--screenshot` renders the demo Panel
to a PNG and exits; it needs no display.

## Privacy

No account, no telemetry, no network port. See [`../SECURITY.md`](../SECURITY.md) for the design summary.
