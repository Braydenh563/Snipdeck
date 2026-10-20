# User guide

How to use Snipdeck 1.0 on Windows. To install it, see the [README](../README.md#install-and-first-run). If
something goes wrong, see [`troubleshooting.md`](troubleshooting.md).

Windows behaviour (paste, hotkeys, the tray icon) has not been tested by the maintainer on a real PC yet.
This guide says what the app is built to do.

## The idea in one minute

- A **board** is a workspace, such as "Work" or "Personal". The Free plan has up to 5 boards.
- A **list** is a group of items in a board, such as "Email replies". Lists show as tabs in the Panel. A new
  library starts with a board called "My snippets", a list called "Snippets" and a short **Getting started**
  list. Its four cards show how to paste, copy, try a template and open the menu. Delete the list when you
  no longer need it.
- An **item** is one thing to paste: text, code, a link, a template, an image or a file.
- The **Panel** is the small window on your board. It can dock to a screen edge (see [Settings](#settings)).
  The **Picker** searches every board at your caret. The **Main window** is where you organise everything.
- The **tray icon** keeps Snipdeck running. A left click shows or hides the Panel. A double-click opens the
  Main window. The right-click menu has Open Panel, Open Snipdeck, Pause hotkeys and Quit.
- The **target** is the app you were typing in. When you click an item, Snipdeck pastes into the target. The
  Panel never takes the focus away from it.

## Add a snippet

Press `Ctrl+N` in the Main window or the Panel, or use the **New item** button. You can also choose **New
snippet** in the Panel menu (the three dots). Pick a **Kind**, fill in the fields and press `Ctrl+Enter` or **Save**. The item
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
- Is put on the clipboard marked private, so Windows clipboard history and cloud clipboard skip it. If you only
  copied it, Snipdeck clears it from the clipboard after 30 seconds, if nothing else has been copied since.
  Change the time, or turn this off, under **Privacy** in Settings.
- Is masked in the fill form.
- Is also encrypted on disk, on every plan, with a key protected by Windows. Supporter can add a master
  password (see [Master password](#master-password-supporter)). The title, tags and search text are not
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

### Paste stack (Supporter)

A paste stack is a queue you choose yourself.

1. In the Panel, open an item's menu and choose **Add to paste stack**. Do this for each item. The Panel
   shows how many are marked. Choose **Remove from paste stack** to take one out.
2. Press **Start stack** in the strip that appears.
3. Press `Ctrl+Alt+V` once for each item. The strip shows "2 of 5 · next: Title".

`Esc` in the Panel, the **End stack** button, or pasting the last item ends the stack. **Clear** drops the
marks before you start. On the Free plan, Start stack tells you it is a Supporter feature.

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
- **Duplicate.** Choose **Duplicate** from the item menu in the Main window to make a copy of the item right
  after it.
- **Export a list as text.** Right-click a list in the sidebar and choose **Export as text**. Snipdeck writes
  a `.txt` file in your Documents folder, such as `Code-20261019.txt`, and a message gives its name. The file
  starts with the list name. Each item follows, with its title when it has one, and a line of dashes between
  items. Images, files and sensitive items are left out, because the file is plain text.
- **Tags.** Choose **Tags…** to tick the tags of the selected items. Delete a tag from the sidebar; its items
  stay. New tags are made in the item editor.
- **Pin.** Press `Ctrl+P` on a row, or choose **Pin** from its menu. Pinned items show in a strip under the
  Panel's tabs, and under **Pinned** in the sidebar.
- **Rename.** The item's title is in the editor (`Ctrl+E` or `F2`). Lists and boards have their own rename
  entries.
- **Select many.** `Ctrl`+click or `Ctrl+Space` adds a row, `Shift`+click or `Shift+Arrow` extends, and
  `Ctrl+A` selects all. `Esc` clears the selection.
- **Open the item menu** with a right-click, the `Menu` key or `Shift+F10`. Or point at a row or card, or
  focus it, and press the **More** (⋯) button that appears. It opens the same menu, for a mouse or touch
  screen. In the Main window the menu offers Use or Paste, Copy, Edit, Duplicate, Pin or Unpin, Move to list,
  Tags and Move to Trash. In the Panel it offers Use, Copy, Edit, Pin or Unpin, Add to paste stack and Move
  to Trash.
- **Edit or delete a list in the Panel.** Right-click its tab, or press `Menu`, `Shift+F10` or `F2` on it.
  Double-click a tab to edit it.

### Colour labels and list icons

- **Colour label.** In the item editor, choose one of eight named colours under *Colour label* (or *No label*).
  The card gets a thin bar on its left edge, and screen readers hear the colour's name. In the Main window a
  row of chips above the items filters the view by label: choose a chip to show only those items, choose it
  again or *Clear filter* to show everything.
- **List icon.** In the list editor, pick an icon under *Icon* (or *No icon*). It shows beside the list's name
  on its tab and in the sidebar. Use the arrow keys to move through the icons.

### Trash and undo

Deleting an item, list or board moves it to the **Trash**. A message with an **Undo** button stays for about
5 seconds, and longer while the pointer is on it. `Ctrl+Z` does the same as the button. In a search box
`Ctrl+Z` undoes your typing instead. Later, open **Trash** to restore items (`Enter` or **Restore**) or delete them for good. Deleted
things are removed from the Trash after 30 days, and **Empty Trash** removes them at once. A board comes back
only if the Free plan has room for it.

## Commands and shortcuts

- **All commands.** Press `Ctrl+K` (or `Ctrl+Shift+P`) in the Main window, or choose **All commands…** in
  the Panel menu. Type a few letters, such as "new" or "hotkeys", and press `Enter`. The palette lists the
  actions of the app, so you never need to remember where one lives.
- **Keyboard shortcuts.** Press `Ctrl+/` to see every shortcut. It is also a tab in the Help window.
  Menus and tooltips show the shortcut beside an action.

## Recent

The Recent inbox keeps the text you copy in other programs, so you can find it again. It is **off by default**.

- **Turn it on.** Settings ▸ **Recent** ▸ **Keep a Recent inbox of what you copy**, or the **Clipboard history**
  step of the welcome tour. Nothing is collected before that.
- **Where it shows.** A **Recent** entry in the Main window's sidebar, and a clock tab in the Panel. The newest
  copy is first, with the program it came from. Click a row to copy it. The menu (right-click, `Menu` or
  `Shift+F10`) has **Copy**, **Save to…** (put it in a list as a normal item) and **Remove from Recent**.
- **How much it keeps.** The last 200 copies, for 7 days at most. Older ones are removed. A copy of the same
  text moves to the top instead of making a second row. Text only, up to 64 KB per copy.
- **What it never keeps.** Sensitive items, text copied by Snipdeck itself, copies from password managers
  (programs that ask Windows to keep a copy out of clipboard history), and copies from programs on your own
  exclude list. Add a program under **Never keep copies from these programs** by its file name, for example
  `keepass.exe`.
- **Pause and clear.** The tray menu has **Pause history** while the inbox is on. **Clear recent history** in
  Settings, or at the top of the Recent view, removes everything now. You confirm with **Clear history**.
- **Encrypted.** Copies are encrypted on disk with the same key as Sensitive items (Windows protects the key
  for your account). Without a master password the inbox works as usual; with one, the library locked means
  the inbox shows **Recent copies are locked** and collects nothing until you unlock.

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
  **Accent colour** is Blue (the default), Teal, Green, Amber, Rose, Violet or Windows accent, which follows your
  Windows colour and is adjusted so text stays readable. **Reduce motion** turns off sliding and fading. It is
  also off when Windows has **Animation effects** off. **Panel width** is 260 to 480 pixels. You can also drag
  the Panel's edges.
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
- **Licence.** Shows your plan. **Get Supporter…** opens a dialog with the price and a Free and Supporter
  table. To turn on Supporter, paste your key (it starts with `SNIP1.`) and press **Activate**. See
  [Free and Supporter](#free-and-supporter) and [`licensing.md`](licensing.md).
- **Backups.** Snipdeck backs up once a day and keeps the newest ones: set **Backups to keep** from 1 to 60
  (default 7). **Restore** adds back what is missing and
  never removes anything. **Back up now** makes one on demand. **Open data folder** and **Open backups folder** open them.
- **Your data.** **Export library** saves one `.snipdeck` file in your Documents folder, named like
  `Snipdeck-library-20261018.snipdeck`. **Erase library** deletes every board, list, item and backup after
  you type `DELETE`. Your settings and licence stay. Tick **Also forget the encryption key** only if you do not
  need encrypted items in old exports.
- **Privacy.** **Clear the clipboard after copying a sensitive item** sets the seconds (30 by default, `0`
  never clears it). **Master password** sets, changes or removes the password (Supporter). Below them is a
  short summary of what is stored and where.

## Master password (Supporter)

Sensitive items are encrypted with a key that Windows protects. A master password adds a password on top.
Set it under **Privacy** in Settings with **Set password…**. It needs at least 8 characters, and Snipdeck
shows how strong it looks.

- **Unlock when needed.** With a password set, the library starts locked, but Snipdeck does not ask at
  start-up. It asks the first time you need a sensitive item. Other items always work.
- **Lock after 15 minutes.** After 15 minutes without use the library locks again. An open sensitive item
  closes without saving, and a message says so.
- **While locked,** Snipdeck will not back up, export or restore, because the copy would miss your sensitive
  items. It asks you to unlock first. The daily backup waits and is tried again later.
- **No recovery.** If you forget the password, your sensitive items can never be opened again. Snipdeck
  cannot reset it and keeps no recovery key. Your other items are not affected. Write the password down
  somewhere safe.
- **Change or remove it** with **Change…** and **Remove…**. Both ask for the current password. After you
  remove it, Windows alone protects the key, as on the Free plan. Only setting a password needs Supporter.
  You can always unlock, change or remove one, so a missing licence never traps your library.

If Windows cannot protect a key on your PC, the master password is not available. See
[`troubleshooting.md`](troubleshooting.md) and [`crypto.md`](crypto.md).

## Free and Supporter

Snipdeck is free software. Free covers everyday use. Supporter is pay what you want from A$5 (suggested A$8),
one time, with every 1.x update. Free stays fully usable, and nothing nags you.

| Feature | Free | Supporter |
|---|---|---|
| Boards | Up to 5 | Unlimited |
| Lists and items, Panel and Picker | Yes | Yes |
| Save from the clipboard | Yes | Yes |
| Dock to a screen edge | Yes | Yes |
| Templates with date, time and cursor | Yes | Yes |
| Sensitive items, masked | Yes | Yes |
| Encryption on disk | Yes | Yes |
| Backups, export, light and dark, 7 languages | Yes | Yes |
| Templates that ask for a value or a choice | No | Yes |
| Master password | No | Yes |
| Paste stack, and Paste next | No | Yes |

If you reach the board limit, a message says so. See [`licensing.md`](licensing.md).

## Import and export

Export is in Settings, as above. The file is a zip with JSON ([`format.md`](format.md)), so other tools can
read it. To export one list as text, see [Organise](#organise).

**Import.** Choose **Settings ▸ Your data ▸ Import…**, then what you are importing:

- **.snipdeck file:** an export of your own, or one from another PC.
- **Ditto database:** the database file Ditto keeps.
- **CopyQ export:** the JSON file CopyQ saves.
- **Folder of .txt and .md files:** every text and Markdown file in the folder becomes one item.

Snipdeck reads the source first and shows a preview before it changes anything: how many items and lists will
be added, how many new boards will be made, how many items you already have (these are skipped), and a **Will
not be imported** list with the reason for each item left out, such as a file over 5 MB, an empty file, a
picture or file that CopyQ or Ditto holds, or a sensitive item that this PC has no key for. Choose **Import**
to go ahead or **Cancel** to leave your library as it was. Everything arrives under one new board named after the
file or folder. The Free plan's board limit still applies, and the preview says when an import would pass it.

**Restore.** Restoring a backup in Settings shows the same preview. A restore puts each board back under its
own name.

Snipdeck does not import the Windows clipboard history (Win+V) yet. Turn on the [Recent inbox](#recent) to keep
what you copy from now on.

## Keyboard reference

| Where | Keys | Action |
|---|---|---|
| Anywhere | `Ctrl+Alt+Space` | Show or hide the Panel |
| Anywhere | `Ctrl+Shift+Space` | Open the Picker |
| Anywhere | `Ctrl+Shift+2` | Save the clipboard text as a new item |
| Anywhere | `Ctrl+Alt+V` | Paste next, or the next item of a paste stack (Supporter) |
| Panel | `Up`, `Down`, `PageUp`, `PageDown` | Move the selection |
| Panel | `Enter`, `Shift+Enter` | Use, or copy only |
| Panel | `1` to `9` | Use that row (search box empty) |
| Panel | `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next or previous list |
| Panel | `Ctrl+F` | Focus the search box |
| Panel | `Ctrl+D` | Duplicate |
| Panel | `Ctrl+K` | All commands (opens them in the Main window) |
| Panel | `Ctrl+N`, `Ctrl+Shift+N` | New item, new list |
| Panel | `Ctrl+,` | Settings |
| Panel | `Ctrl+/` | Show the keyboard shortcuts |
| Panel | `Ctrl+W` | Hide the Panel |
| Panel | `Ctrl+Z` | Undo the delete (search box empty) |
| Panel | `Ctrl+E`, `F2` | Edit the selected item |
| Panel | `Delete` | Move the selected item to the Trash (not while searching) |
| Panel | `Menu`, `Shift+F10` | Open the item menu |
| Panel | `Esc` | Clear the search; with nothing to clear, hide the Panel |
| Picker | `Tab`, `Shift+Tab` | Change scope |
| Picker | `Alt+1` to `Alt+9` | Use that row |
| Picker | `Ctrl+E`, `Esc` | Edit, close |
| Main window | `Ctrl+K`, `Ctrl+Shift+P` | All commands |
| Main window | `Ctrl+/` | Show the keyboard shortcuts |
| Main window | `Ctrl+N`, `Ctrl+Shift+N` | New item, new list |
| Main window | `Ctrl+,` | Settings |
| Main window | `Ctrl+W` | Close the window |
| Main window | `Ctrl+Z` | Undo the delete (not while typing in the search box) |
| Main window | `Ctrl+F`, `/` | Search |
| Main window | `Enter`, `Shift+Enter` | Use, or copy only |
| Main window | `Ctrl+E`, `F2` | Edit |
| Main window | `Ctrl+D` | Duplicate |
| Main window | `Ctrl+P` | Pin or unpin |
| Main window | `Delete` | Move to the Trash (in the Trash: delete for good) |
| Main window | `Ctrl+A`, `Ctrl+Space` | Select all, toggle a row |
| Main window | `Alt+Up`, `Alt+Down` | Move an item or list |
| Panel, Main window | `Space` (hold) | Reveal a sensitive item |
| Main window | `F1` | Open the Help window |
| Item editor | `Ctrl+Enter` | Save |
| Fill form | `Ctrl+Enter`, `Esc` | Insert, cancel |

## Drag and drop

Most drags have a keyboard or menu way to do the same thing (the Panel's row reorder has none yet: use `Alt+Up` / `Alt+Down` in the Main window).

- **Reorder snippets:** drag a snippet onto another one, in the Panel or in the Main window (list or tile
  view), in a list that is in manual order and with no search running. Keyboard: `Alt+Up` or `Alt+Down` in
  the Main window.
- **Move to another list:** drag a snippet onto a list in the Main window's sidebar, or onto a list tab in the
  Panel. Keyboard: **Move to list** in the snippet's menu, or the list field in the editor.
- **Reorder lists:** drag a list onto another in the sidebar. Keyboard: `Alt+Up` or `Alt+Down` on the list.
- **Save text:** drop text from another app on the Panel to save it as a new snippet at the top of the list the
  Panel shows. An address becomes a link. Keyboard: **+ From clipboard**.

A drag inside Snipdeck never offers anything to other apps.

## Command line

`Snipdeck.bat` passes extra options to the app: `--hidden` starts with the Panel hidden, `--dark` forces the
dark theme for this run, and `--data-dir <folder>` uses another library folder. `Snipdeck.bat demo` opens a
sample library that is kept in memory and saves nothing.

Two options print a report and exit, without opening a window. Close Snipdeck first. Run them from a
Command Prompt, with `app\snipdeck.exe` in the folder that holds `Snipdeck.bat`:

- `--check-db` checks the library for damage. It prints the result and exits with 0 (healthy), 1 (problems
  found) or 2 (the check could not run).
- `--troubleshoot` prints the same report as **Troubleshoot** in the Help window.

Add `--data-dir <folder>` to check a library somewhere else.

