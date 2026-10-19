# User guide

How to use Snipdeck 1.0 on Windows. To install it, see the [README](../README.md#install-and-first-run). If
something goes wrong, see [`troubleshooting.md`](troubleshooting.md).

Windows behaviour (paste, hotkeys, the tray icon) has not been tested by the maintainer on a real PC yet.
This guide says what the app is built to do.

## The idea in one minute

- A **board** is a workspace, such as "Work" or "Personal". The Free plan has up to 5 boards.
- A **list** is a group of items in a board, such as "Email replies". Lists show as tabs in the Panel. A new
  library starts with a board called "My snippets" and a list called "Snippets".
- An **item** is one thing to paste: text, code, a link, a template, an image or a file.
- The **Panel** is the small window on your board. It can dock to a screen edge (see Settings). The **Picker** searches every board at your caret. The
  **Main window** is where you organise everything.
- The **target** is the app you were typing in. When you click an item, Snipdeck pastes into the target. The
  Panel never takes the focus away from it.

## Add a snippet

Open the Main window and press `Ctrl+N` or the **New item** button. You can also choose **New snippet** in the
Panel menu (the three dots). Pick a **Kind**, fill in the fields and press `Ctrl+Enter` or **Save**. The item
goes into the list you were looking at.

| Kind | What to enter |
|---|---|
| **Text** | The text. The title is optional: the first line is used. |
| **Code** | Code, pasted as plain text. |
| **Link** | The address, such as `https://example.com`. |
| **Template** | Text with placeholders (see below). |
| **Image** | A PNG picture. Press **Browse** and choose the file. |
| **File** | Any file. Press **Browse** and choose it. Snipdeck keeps its own copy. |

The editor shows a banner if it restored changes you had not saved. Text over 16 KB opens read-only.

**Save text from the clipboard.** Copy something, then press `Ctrl+Shift+2` (or **Save from clipboard** in an
empty Panel). The text becomes a new item at the top of the current list. Text over 5 MB is refused.

### Templates

A template is an item whose text has placeholders. Snipdeck fills them in when you use the item. In the
editor, the **Placeholders** list inserts one for you.

| Placeholder | What you get | Plan |
|---|---|---|
| `{date}`, `{time}` | Today's date, or the time | Free |
| `{datetime:FORMAT}` | Date and time in a format you choose, such as `{datetime:%d %B %Y}` | Free |
| `{cursor}` | Nothing. It marks where the text cursor ends up after pasting. | Free |
| `{field:Name}` or `{field:Name\|default}` | Asks you for a value each time | Supporter |
| `{choice:Name\|a\|b}` | Asks you to pick one of the options | Supporter |

`{date}` and `{time}` use your computer's time zone. `{clipboard}` inserts the clipboard text. Type `{{` and `}}` for literal braces. Any other braces are an error, and the editor says so when
you save.

When you use a template with `{field}` or `{choice}`, a **Fill in template** window opens. Fill the fields,
check the preview and press `Ctrl+Enter` to insert. `Esc` cancels.

### Sensitive items

Turn on **Sensitive item** under **Privacy** in the editor. Only text, code, link and template items can be
sensitive. A sensitive item:

- Shows as a lock with a hidden preview in lists. In the Main window, hold `Space` on the row (in the Panel too) to reveal it.
- Is put on the clipboard marked private, so Windows clipboard history and cloud clipboard skip it. If you only copied it, Snipdeck
  clears it from the clipboard after 30 seconds (if the clipboard still holds it).
- Is masked in the fill form.
- With Supporter, is also encrypted on disk with a key protected by Windows. The title, tags and search text are not
  encrypted. See [`crypto.md`](crypto.md).

## Paste an item

| Do this | Result |
|---|---|
| Click an item in the Panel | Pastes it into the target |
| `Shift`+click, or `Shift+Enter` | Copies only, and does not paste |
| `Enter` on the selected row | Same as a click |
| Press `1` to `9` (search box empty) | Uses the row with that number |
| Click a pinned chip | Uses that item |
| Picker: type, then `Enter` | Pastes the first match |

After a paste, Snipdeck puts your earlier clipboard back. If it cannot paste into the target (a password
box, a program running as administrator, a program in your "never paste into" list), it copies instead and
a message tells you why. The Panel footer always says where a click will go: "Pastes into Outlook" or
"Copies to clipboard".

### The Picker

Press `Ctrl+Shift+Space`. A small search window opens at your caret. Type to filter. It matches titles, tags
and the start of the content.

- `Up`, `Down`, `PageUp`, `PageDown` move. `Enter` pastes. `Shift+Enter` copies.
- `Tab` and `Shift+Tab` change where it searches: **All**, **This board** or **Pinned**.
- `1` to `9` use that row while the box is empty. `Alt+1` to `Alt+9` work at any time.
- `Ctrl+E` edits the selected item. `Esc` closes the Picker.

### Paste next (Supporter)

`Ctrl+Alt+V` pastes the next item of the pinned strip (or, with nothing pinned, of the list you are looking
at), one per press. After the last one, a message says so and the next press starts again.

## Organise

Use the Main window. The sidebar lists **All items**, **Pinned**, your lists, your tags and the **Trash**. The
toolbar searches (`Ctrl+F` or `/`) and switches between the compact, comfortable and tile views.

- **Boards.** Open the board name at the top of the sidebar or the Panel. The menu lists your boards and can
  create, rename, move up or down, and delete a board. Deleting a board moves its lists and items to the
  Trash.
- **Lists.** Add a list with **Add list** in the sidebar, or the **+** tab in the Panel. **Edit list** sets the name, a colour and the sort: manual, most
  used or recently used. Move a list with `Alt+Up` or `Alt+Down` while it is selected in the sidebar.
- **Order of items.** Select a row and press `Alt+Up` or `Alt+Down`.
- **Move.** Choose **Move to list…** from the item menu to move one or more selected items.
- **Tags.** Choose **Tags…** to tick the tags of the selected items. Delete a tag from the sidebar; its items
  stay. New tags are made in the item editor.
- **Pin.** Press `Ctrl+P` on a row, or choose **Pin** from its menu. Pinned items show in a strip under the
  Panel's tabs, and under **Pinned** in the sidebar.
- **Rename.** The item's title is in the editor (`Ctrl+E` or `F2`). Lists and boards have their own rename
  entries.
- **Select many.** `Ctrl`+click or `Ctrl+Space` adds a row, `Shift`+click or `Shift+Arrow` extends, and
  `Ctrl+A` selects all. `Esc` clears the selection.
- **Open the item menu** with a right-click, the `Menu` key or `Shift+F10`. In the Panel it offers Use, Copy,
  Edit, Pin or Unpin, and Move to Trash.
- **Edit or delete a list in the Panel.** Right-click its tab, or press `Menu`, `Shift+F10` or `F2` on it.
  Double-click a tab to edit it.

### Trash and undo

Deleting an item, list or board moves it to the **Trash**. A message with an **Undo** button stays for about
5 seconds. Later, open **Trash** to restore items (`Enter` or **Restore**) or delete them for good. Deleted
things are removed from the Trash after 30 days, and **Empty Trash** removes them at once. A board comes back
only if the Free plan has room for it.

## Help and the welcome tour

Press `F1` or open **Help** in the Main window to see the Help window. **Troubleshoot** is a checklist that finds
common problems, and **About** shows the version. The welcome tour runs on the first start. To see it again,
use **Show the welcome tour again** in Help or the link in About.

When the Panel hides for the first time, a message tells you Snipdeck is still running and how to bring it
back.

## Settings

Open Settings from the Panel menu or the Main window. Changes apply at once and are saved.

- **Shortcuts.** The four hotkeys. Click a field and press the new combination. The clear button turns one
  off. Snipdeck refuses a combination another action uses, or one that Windows or another program owns, and
  says which.
- **Appearance.** **Theme** is Windows, Light or Dark. **Language** offers English, Español, Deutsch,
  Français, Português (Brasil), 日本語 and 简体中文, and takes effect the next time Snipdeck starts.
  **Panel width** is 260 to 480 pixels. You can also drag the Panel's edges.
- **Panel dock.** **Dock to** fixes the Panel against Floating, Left edge, Right edge, Top edge or Bottom edge
  of the monitor it is on. **Open a docked panel by** is Off, Hover, Click or Hover and click. Off keeps a docked
  Panel open. The other choices fold it into a thin strip, and the Panel shortcut opens it too. **Hover delay**
  (0 to 2000 ms, default 150) is how long the pointer rests on the strip. **Leave delay** (0 to 5000 ms, default
  400) is how long it stays off the Panel before it folds.
- **Window behaviour.** **Show Snipdeck in the taskbar** gives the Panel a taskbar button (off by default).
  **Hide the panel when it loses focus** hides an unpinned Panel when you switch to another app (on by default).
  **Start with Windows** starts Snipdeck hidden in the tray when you sign in. **Keep running in the tray when the
  window closes** is on by default. Turn it off and closing the Panel or the Main window quits Snipdeck.
- **Never paste into.** Program file names, such as `notepad.exe`. Snipdeck copies instead of pasting there.
- **Licence.** Paste your key (it starts with `SNIP1.`) and press **Activate**. See the
  [README](../README.md#free-and-supporter).
- **Backups.** Snipdeck backs up once a day and keeps the newest ones: set **Backups to keep** from 1 to 60
  (default 7). **Restore** adds back what is missing and
  never removes anything. **Back up now** makes one on demand. **Open data folder** and **Open backups folder** open them.
- **Your data.** **Export library** saves one `.snipdeck` file in your Documents folder, named like
  `Snipdeck-library-20261018.snipdeck`. **Erase library** deletes every board, list, item and backup after
  you type `DELETE`. Your settings and licence stay. Tick **Also forget the encryption key** only if you do not
  need encrypted items in old exports.
- **Privacy.** A short summary of what is stored and where.

## Import and export

Export is in Settings, as above. The file is a zip with JSON ([`format.md`](format.md)), so other tools can
read it. To bring a library back, restore one of the backups listed in Settings. Importing text files, Ditto
or CopyQ data has no screen in this version.

## Keyboard reference

| Where | Keys | Action |
|---|---|---|
| Anywhere | `Ctrl+Alt+Space` | Show or hide the Panel |
| Anywhere | `Ctrl+Shift+Space` | Open the Picker |
| Anywhere | `Ctrl+Shift+2` | Save the clipboard text as a new item |
| Anywhere | `Ctrl+Alt+V` | Paste next (Supporter) |
| Panel | `Up`, `Down`, `PageUp`, `PageDown` | Move the selection |
| Panel | `Enter`, `Shift+Enter` | Use, or copy only |
| Panel | `1` to `9` | Use that row (search box empty) |
| Panel | `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next or previous list |
| Panel | `Ctrl+F` | Focus the search box |
| Panel | `Ctrl+E`, `F2` | Edit the selected item |
| Panel | `Delete` | Move the selected item to the Trash (not while searching) |
| Panel | `Menu`, `Shift+F10` | Open the item menu |
| Panel | `Esc` | Clear the search; with nothing to clear, hide the Panel |
| Picker | `Tab`, `Shift+Tab` | Change scope |
| Picker | `Alt+1` to `Alt+9` | Use that row |
| Picker | `Ctrl+E`, `Esc` | Edit, close |
| Main window | `Ctrl+N` | New item |
| Main window | `Ctrl+F`, `/` | Search |
| Main window | `Enter`, `Shift+Enter` | Use, or copy only |
| Main window | `Ctrl+E`, `F2` | Edit |
| Main window | `Ctrl+P` | Pin or unpin |
| Main window | `Delete` | Move to the Trash (in the Trash: delete for good) |
| Main window | `Ctrl+A`, `Ctrl+Space` | Select all, toggle a row |
| Main window | `Alt+Up`, `Alt+Down` | Move an item or list |
| Panel, Main window | `Space` (hold) | Reveal a sensitive item |
| Main window | `F1` | Open the Help window |
| Item editor | `Ctrl+Enter` | Save |
| Fill form | `Ctrl+Enter`, `Esc` | Insert, cancel |

## Command line

`Snipdeck.bat` passes extra options to the app: `--hidden` starts with the Panel hidden, `--dark` forces the
dark theme for this run, and `--data-dir <folder>` uses another library folder. `Snipdeck.bat demo` opens a
sample library that is kept in memory and saves nothing.
