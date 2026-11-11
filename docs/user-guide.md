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
- The **target** is the app you were typing in. When you click an item, Snipdeck pastes into the target. A
  click on an item never takes the focus away from it. When you open the Panel with its hotkey, the tray icon or
  by starting Snipdeck again, it takes the keyboard so you can type to search, and gives it back to the target
  when it closes. Resting the pointer on a docked Panel's edge opens it without taking the keyboard.

## Add a snippet

Press `Ctrl+N` in the Panel, or in the Main window with no list open, or use the **New item** button. With a list open in the Main window, `Ctrl+N` jumps to the quick-add box above its items: type or paste, then press `Enter` to save it at the end of that list (`More options` opens the full editor). You can also choose **New
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

**Save text from the clipboard.** Copy something, then use **Save from clipboard** in an empty Panel. The text
becomes a new item at the top of the current list. Text over 5 MB is refused. The Capture shortcut does the same
from anywhere, saving whatever text is on the clipboard when you press it; it is off until you set a shortcut in
Settings, **Shortcuts**.

**Formatting.** Pasting into the Panel or the Main window (`Ctrl+V`), **Save from clipboard** and the Capture
shortcut keep the formatting (HTML and RTF) beside the text. Such an item pastes formatted into Word or Outlook and as
plain text into Notepad, and its row shows a **Formatted** mark. In the editor, changing the text removes the
formatting when you save, and the editor says so first; **Remove formatting** drops it and keeps the text.

### Templates

A template is an item whose text has placeholders. Snipdeck fills them in when you use the item. In the
editor, the **Placeholders** list inserts one for you.

| Placeholder | What you get | Plan |
|---|---|---|
| `{date}`, `{time}` | Today's date, or the time | Free |
| `{datetime:FORMAT}` | Date and time in a format you choose, such as `{datetime:%d %B %Y}` | Free |
| `{uuid}` | A new random ID (version 4), different for each `{uuid}` and each use | Free |
| `{cursor}` | Nothing. It marks where the text cursor ends up after pasting. | Free |
| `{field:Name}` or `{field:Name\|default}` | Asks you for a value each time | Supporter |
| `{choice:Name\|a\|b}` | Asks you to pick one of the options | Supporter |
| `{multiline:Name}` or `{multiline:Name\|default}` | Asks for text over several lines | Supporter |
| `{pickdate:Name}` or `{pickdate:Name\|FORMAT}` | Asks for a date typed as `YYYY-MM-DD`; left empty it is today. `FORMAT` is how it is written, such as `%d %B %Y`. | Supporter |
| `{optional:Name}...{/optional}` | A tick box. The text inside is inserted only when it is ticked. Sections do not nest. | Supporter |

`{date}`, `{time}` and `{datetime:FORMAT}` can move in time: add an offset such as `{date+3d}`,
`{date-1w:%Y-%m-%d}`, `{time+90min}` or `{datetime+1mo:%d %B %Y %H:%M}`. The units are `d` (days), `w` (weeks),
`mo` (months), `y` (years), `h` (hours) and `min` (minutes); you can chain offsets, as in `{date+1mo-1d}`. A month
that is too short gives its last day (`{date+1mo}` on 31 January is 28 or 29 February). `{date:FORMAT}` works like
`{datetime:FORMAT}`. Very large offsets are an error, and the editor says so when you save.

`{date}` and `{time}` use your computer's time zone. `{clipboard}` inserts the clipboard text. Type `{{` and `}}` for literal braces. Any other braces are an error, and the editor says so when
you save.

On the Free plan the **Placeholders** list in the editor marks every Supporter placeholder with a **Supporter**
badge. You can still type one. When you save, the editor refuses and says the text needs Supporter. The message
sits under the text box, with a **Get Supporter…** button right below it that opens the Supporter dialog. Remove
the placeholder to save on the Free plan.

In the Fill in template window, each `{uuid}` shows as "(new ID)" in the preview: the real ID is made when you insert.

When you use a template with `{field}`, `{choice}`, `{multiline}`, `{pickdate}` or `{optional}`, a **Fill in template** window
opens. Fill the fields, check the preview and press `Ctrl+Enter` to insert. `Esc` cancels. In a multi-line box `Enter`
adds a line; `Ctrl+Enter` still inserts. A date that is not a real day (such as 2026-02-30) is marked on its box and
nothing is inserted until you fix it or clear it. `Enter` moves to the next box, and on the last box it inserts.
The tick box is switched with `Space`. The form opens for any template that asks something, even when every
question has a default: the defaults are already filled in.

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
| Press `Alt+1` to `Alt+9` | Uses the row with that number (digits without `Alt` are search text) |
| Click a pinned chip | Uses that item |
| Picker: type, then `Enter` | Pastes the first match |
| Main window: click a row | Selects it (`Ctrl`+click and `Shift`+click extend the selection) |
| Main window: double-click, `Enter`, or the row's **Use** button | Uses the selected item |

After a paste, Snipdeck puts your earlier clipboard back. If it cannot paste into the target (a password
box, a program running as administrator, a program in your "never paste into" list), it copies instead and
a message tells you why. The Panel footer always says where a click will go: "Pastes into Outlook" or
"Copies to clipboard". A "Copied" or "Pasted" message confirms every use, in the Panel and in the main window.

### The Picker

Press `Ctrl+Shift+Space`. A small search window opens at your caret. Type to filter. It matches titles, tags
and the start of the content. It opens one line below your caret, so it covers neither the line you are typing
nor the next one. **Settings, Panel, Picker position** moves it above, to the right or to the left of the caret;
it switches to the opposite side when there is no room. Where an app shows no caret it opens at the mouse.

- `Up`, `Down`, `PageUp`, `PageDown` move. `Enter` pastes. `Shift+Enter` copies.
- `Tab` and `Shift+Tab` change where it searches: **All**, **This board** or **Pinned**.
- `Alt+1` to `Alt+9` use that row, whatever the box holds. Digits without `Alt` are typed into the search.
- `Ctrl+E` edits the selected item. `Esc` closes the Picker.

### Paste next (Supporter)

`Ctrl+Shift+U` pastes the next item of the pinned strip (or, with nothing pinned, of the list you are looking
at), one per press. After the last one, a message says so and the next press starts again.

### Paste stack (Supporter)

A paste stack is a queue you choose yourself.

1. In the Panel, open an item's menu and choose **Add to paste stack**. Do this for each item. The Panel
   shows how many are marked. Choose **Remove from paste stack** to take one out.
2. Press **Start stack** in the strip that appears.
3. Press `Ctrl+Shift+U` once for each item. The strip shows "2 of 5 · next: Title".

`Esc` in the Panel, the **End stack** button, or pasting the last item ends the stack. **Clear** drops the
marks before you start. On the Free plan, Start stack tells you it is a Supporter feature.

## Organise

Use the Main window. The sidebar lists **All items**, **Pinned**, your lists, your tags and the **Trash**. The
toolbar searches (`Ctrl+F` or `/`) and switches between the compact, comfortable and tile views.

- **Boards.** Open the board name at the top of the sidebar or the Panel. The menu lists your boards. In the
  Panel it has **New board**, **Rename board** and **Delete board** for the board you are on, so you can manage
  boards without opening the Main window. The Main window can also move a board up or down. Deleting a board
  moves its lists and items to the Trash, and **Undo** brings it back. The Free plan stops at 5 boards.
- **Lists.** Add a list with **Add list** in the sidebar, or the **+** tab in the Panel. **Edit list** sets the name, a colour and the sort: manual, most
  used or recently used. Move a list with `Alt+Up` or `Alt+Down` while it is selected in the sidebar.
- **Order of items.** Select a row and press `Alt+Up` or `Alt+Down`, in the Main window or the Panel, or choose **Move up** / **Move down** in the snippet's menu.
- **Groups.** A list sorted by hand (Manual) can hold named groups of items, in the Panel and in the Main window.
  - **Make one.** Select two or more rows and press `Ctrl+G`, or choose **Group…** from the item menu (the row
    at the cursor when nothing is selected). The rows gather under a header and its name opens for typing: type a
    name and press `Enter` (the default is "Group"). In a list that sorts itself, **Group…** offers to switch to
    Manual first.
  - **Read the header.** It shows the name, how many items the group holds and an arrow. A screen reader reads it
    as "group, 3 items" and says whether it is expanded or collapsed.
  - **Fold or unfold a group.** Click the header, or press `Left` to fold and `Right` to unfold. `Enter` toggles it
    (in the Panel `Space` does too). A folded group hides its items and stays folded.
  - **Rename.** Press `F2` on the header, press `Ctrl+G` on it with nothing selected, or choose **Rename group**.
  - **Ungroup or delete.** **Ungroup** dissolves the group and keeps its items. **Delete group** (or `Delete`
    on the header) moves the group and its items to the Trash, and `Ctrl+Z` brings them all back in one step.
  - **Move.** Drag a header to move the whole group, or press `Alt+Up` or `Alt+Down` on it. Dropping an item on a
    header adds it to that group.
  - **Search.** Searching lists every match, even inside a folded group, under the group's header in the Panel;
    the Main window shows a flat list of matches with no headers.
  - **Layout.** A list that holds groups is drawn as rows, so a header spans the width; the Tile view button
    rests (greyed out) until the list has no groups. `Alt+1` to `Alt+9` count items only, never headers.
  - **Main window.** The command palette (`Ctrl+K`) has the same actions, and the header's menu has Rename group,
    Ungroup and Delete group.
  - **Undo.** `Ctrl+Z` takes back a grouping, a rename, an ungroup, a delete and a drag of a header or of an
    item onto one.
- **Move.** Choose **Move to list…** from the item menu to move one or more selected items.
- **Paste as…** Choose **Paste as…** from the item menu (text items, one at a time) to paste the text changed: **Plain
  text**, **UPPERCASE**, **lowercase**, **Title Case**, **Trimmed** (no space at the ends) or **slug-like-this** (`my-title`).
  A template is filled in first. The result is always plain text, with the cursor at the end.
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
  screen. In the Main window the menu offers Use or Paste, Copy, Paste as…, Edit, Duplicate, Pin or Unpin, Move to list,
  Tags and Move to Trash. In the Panel it offers Use, Copy, Paste as…, Edit, Pin or Unpin, Add to paste stack and Move
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

- **Commands.** Press `Ctrl+K` (or `Ctrl+Shift+P`) in the Main window, or choose **Commands…** in
  the Panel menu. Type a few letters, such as "new" or "hotkeys", and press `Enter`. The palette lists the
  actions of the app, so you never need to remember where one lives.
- **Keyboard shortcuts.** Press `Ctrl+/` to see every shortcut. It is also a tab in the Help window.
  Menus and tooltips show the shortcut beside an action.

## Recent

The Recent inbox keeps the text you copy in other programs, so you can find it again. It is **on by default for a new install** (an install that existed before keeps its choice). Passwords from password managers and Sensitive items are never kept.

- **Turn it off (or on).** Settings ▸ **Recent** ▸ **Keep a Recent inbox of what you copy**, or the **Clipboard
  history** step of the welcome tour. Nothing is collected while it is off.
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

## Symbols

The Panel's **Symbols** tab (the smiley next to Recent) holds emoji, kaomoji and icons. Type in **Search emoji,
kaomoji, icons** or pick **Emoji**, **Kaomoji** or **Icons** and a group. Click a symbol to paste it into the app you
were typing in, the same as an item; `Shift`+click copies it instead. **Recently used** keeps the ones you use. To hide
the tab and its `Ctrl+K` command, turn off **Show Symbols tab** in Settings.

## Help and the welcome tour

Press `F1` or open **Help** in the Main window to see the Help window. **Troubleshoot** is a checklist that finds
common problems, and **About** shows the version. A row that finds a problem has a button that fixes it or
shows you where:

- **Back up now** makes a backup at once (for a missing or old backup).
- **Open Settings** opens Settings at the page that holds the setting (for example a hotkey Windows refused).
- **Open logs folder** opens the folder with the crash reports.
- **Rebuild from backup…** appears when the database is damaged. After you confirm, Snipdeck sets the old file
  aside (it is kept, not deleted) and recovers the items it can read into a new one. Check your items
  afterwards.
- **Open data folder** opens `%LOCALAPPDATA%\Snipdeck` in Explorer.

The welcome tour runs on the first start. To see it again,
use **Show the welcome tour again** in Help or the link in About.

When the Panel hides for the first time, a message tells you Snipdeck is still running and how to bring it
back.

### Tips

The **Tips** tab of the Help window has eleven short cards on what Snipdeck can do. Each has a title, two lines, a
picture and a **Try it** button that runs the action for you (for example, showing the Panel). Open it from the
tray icon's menu (**Tips**), from Help, from the last page of the welcome tour, or from the **More tips** card in
the **Getting started** list. Snipdeck never shows a badge or a reminder about tips; they wait until you look.
Press `Tab` to move from card to card.

## Settings

Open Settings from the Panel menu or the Main window. Changes apply at once and are saved.

- **Shortcuts.** The four hotkeys. Click a field and press the new combination. The clear button turns one
  off. Snipdeck refuses a combination another action uses, or one that Windows or another program owns, and
  says which.
- **Appearance.** **Theme** is Windows, Light or Dark. **Language** offers English, Español, Deutsch,
  Français, Português (Brasil), 日本語 and 简体中文, and takes effect the next time Snipdeck starts.
  **Accent colour** is Blue (the default), Teal, Green, Amber, Rose, Violet or Windows accent, which follows your
  Windows colour and is adjusted so text stays readable. **Text size** is Windows (the default, it follows
  Settings > Accessibility > Text size in Windows) or a fixed 100, 125, 150 or 175 %. **Reduce motion** turns off
  sliding and fading. It is also off when Windows has **Animation effects** off. **Panel width** is 260 to 480
  pixels. You can also drag the Panel's edges.
- **Panel layout.** **Layout** is List, Cards (two per row) or Compact. **Search field** is Appears when you type
  or Always shown. **Lists** are Tabs or a Menu in the board picker. **Quick-use numbers** numbers the first nine
  rows for `Alt+1` to `Alt+9`. **Show Symbols tab** shows or hides the Symbols tab and its `Ctrl+K` command.
- **Opening the Panel.** **Tray icon click opens** the Panel or the Main window. **Panel opens** where you left it,
  at the mouse or at the text cursor. **After a paste or copy** hides the Panel or keeps it open.
  **Hide the panel when it loses focus** hides an unpinned Panel when you switch to another app (on by
  default). **Show the Panel in the taskbar and Alt+Tab** gives the Panel a taskbar button and an Alt+Tab entry
  while it is on screen (off by default); a paste from a Panel reached with Alt+Tab still goes to the app you
  were in before. **Keep Panel visible during screenshots** leaves it on screen for Take screenshot.
  **Picker position** is the side of your text cursor the Picker opens on. **Start with Windows** starts Snipdeck
  hidden in the tray when you sign in, and **When Windows starts** can open the Panel as well. **Keep running in
  the tray when the window closes** is on by default. Turn it off and closing the Panel or the Main window quits
  Snipdeck.
- **Docking** is also in Opening the Panel. **Dock to** fixes the Panel against Floating, Left edge, Right edge,
  Top edge or Bottom edge of the monitor it is on. The edge the taskbar is on is never used. You can also drag
  the Panel by its header to within a few pixels of an edge: a strip marks the edge, and the Panel docks there
  when you let go. The Panel's menu names the edge it would dock to ("Dock to the right edge"). **Open a docked
  panel by** is Off, Hover, Click or Hover and click. Off keeps a docked Panel open. The other choices fold it
  into a thin strip, and the Panel shortcut opens it too. **Hover delay** (0 to 2000 ms, default 150) is how long
  the pointer rests on the strip. **Leave delay** (0 to 5000 ms, default 400) is how long it stays off the Panel
  before it folds.
- **Typing.** **Expand abbreviations as I type** and **Suggest snippets as I type**, both off by default. See
  [Triggers and typing](#triggers-and-typing-windows).
- **Privacy.** **Keep a Recent inbox of what you copy** and the programs it never keeps copies from. **Clear the
  clipboard after copying a sensitive item** sets the seconds (30 by default, `0`
  never clears it). **Master password** sets, changes or removes the password (Supporter). With a password set,
  **Unlock with Windows Hello** appears under it (see [Windows Hello](#windows-hello-supporter-windows)). Below them is a
  short summary of what is stored and where.
- **Programs.** One row per program, by file name, such as `windowsterminal.exe`, with the key Snipdeck presses to
  paste there: **Ctrl+V**, **Ctrl+Shift+V**, **Shift+Insert**, or **Never paste** (Snipdeck copies instead).
  Programs not listed get Ctrl+V. The list starts with the terminals known to need another key (PuTTY, mintty,
  KiTTY, Alacritty, WezTerm); type a file name and press **Add** to add a program, and the cross removes it.
- **Backups and your data.** Snipdeck backs up once a day and keeps the newest ones: set **Backups to keep** from
  1 to 60 (default 7). The line above the list says when the last backup was made. **Restore** adds back what is
  missing and never removes anything. **Back up now** makes one on demand. **Open data folder** and **Open backups
  folder** open them. **Export library** asks for a folder and saves one `.snipdeck` file there, named like
  `Snipdeck-library-20261018.snipdeck`. **Erase library** deletes every board, list, item and backup after
  you type `DELETE`. Your settings and licence stay. Tick **Also forget the encryption key** only if you do not
  need encrypted items in old exports.
- **Licence.** Shows your plan. **Get Supporter…** opens a dialog with the price and a Free and Supporter
  table. To turn on Supporter, paste your key (it starts with `SNIP1.`) and press **Activate**. See
  [Free and Supporter](#free-and-supporter) and [`licensing.md`](licensing.md).

The page lists its groups in this order: Shortcuts, Appearance, Panel layout, Opening the Panel, Privacy,
Programs, Typing, Backups and your data, Licence. Press `F1` or choose **Help** at the bottom of the page to open this guide.

## Triggers and typing (Windows)

A **trigger** is a short code you give an item, such as `;sig`. It is 1 to 32 characters with no spaces, and
no two items share one. Set it in the item editor, in the **Trigger** box. A Sensitive item shows "Sensitive items can't be typed in, so they have no trigger." there instead. Items
imported from Espanso or similar tools keep their abbreviations as triggers.

Two options under **Typing** in Settings use triggers while you type in other programs. Both are off by
default, and both work only on Windows (not yet tested on a real PC). On other systems Settings says "Typing triggers work on Windows only."

- **Expand abbreviations as I type.** When you type a trigger in another program, Snipdeck deletes it and
  pastes the item. A trigger must begin after a space or punctuation (or at the start of a line) and fires on
  its last character: no extra key is needed. If the item has fill-in fields, the form opens first.
- **Suggest snippets as I type.** After you type the first two characters of a trigger, a small hint at the
  text cursor shows the item's title and "Tab to insert". Press `Tab` to insert it. Snipdeck takes `Tab` only while the hint shows; any other key carries on as
  usual, and `Esc` (like the arrow keys, `Enter` and `Delete`) clears what you typed so the hint goes away. The hint also goes if you stop typing for about 5 seconds. If several triggers match, the most used item is offered. No text cursor, no hint.

The hint shows only while what you typed is the start of a trigger. It needs at least two characters, and it
waits for a space, punctuation or the start of a line, like a trigger does. When you have typed the whole
trigger, the item is pasted instead of hinted. Only trigger codes count, never titles.

In the item editor, the **Trigger** box has the same rules: 1 to 32 characters with no spaces, and no two
items share one. A one-character trigger shows a warning, because it fires every time you type it, even inside words; two or more characters are safer. If you save a code that another trigger starts with, or that starts another trigger, a message says "Saved. This trigger overlaps the trigger of “Title”, so one of them may never fire." with the other item's title (without a title it says "another trigger"). The item is still saved.

If Snipdeck cannot start the typing watcher, a message says "Couldn't start typing triggers. Nothing will be expanded or suggested."

Nothing expands in password boxes, in programs set to **Never paste**, in programs running as administrator,
while Snipdeck is in front, or for sensitive items. Sensitive items are never suggested either. Snipdeck keeps only the last 32 characters you typed, in
memory, and never writes them to disk or the log. The typing watcher runs only while one of the options is on.

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

## Windows Hello (Supporter, Windows)

With a master password set, Windows Hello can unlock the library with your face, fingerprint or PIN instead of
typing the password. It needs a Supporter plan and a PC where Windows Hello is set up. The option is hidden
otherwise.

- **Turn it on.** Settings, under **Privacy**, below **Master password**: switch on **Unlock with Windows
  Hello**. Windows asks for Hello twice, one after the other, to check it repeats. If you cancel or it fails,
  the switch goes back off and a message says "Windows Hello was not turned on." (or "didn't work, so it was
  not turned on."). The same happens if the library locks, or the plan changes, before it finishes. You can
  turn it off the same way. Settings search finds it too.
- **Unlock.** When the library is locked and Snipdeck asks for the password, press **Unlock with Windows
  Hello** and use your face, fingerprint or PIN. The password box stays there: the password always works.
- **When the password is needed.** After Snipdeck restarts, and 7 days after the last time you typed the
  password. After a password unlock, Windows asks for Hello once so the next unlock can use it.
- **When Hello is cancelled.** If you cancel the prompt, the unlock window says "Windows Hello was cancelled.
  Use your master password." The button stays, so you can try again.
- **When Hello fails.** If it fails or the PIN was removed, the unlock window says "Windows Hello did not
  unlock Snipdeck. Use your master password." The button goes away and the password box stays. **Unlock with
  Windows Hello** stays switched on, and the next password unlock sets Hello up again. If it fails twice in a
  row after password unlocks, or Hello gives different answers each time, Snipdeck turns it off and says so
  (a message, and a line in the unlock window). Turn it on again in Settings.
- **A changed Hello.** If you reset your PIN or change your face or fingerprint, Windows makes a new
  credential and the old copy no longer opens. The unlock window says "Windows Hello changed. Unlock with your
  password once to set it up again." Type your password. Snipdeck then asks Hello twice to check it repeats,
  and the next unlock can use Hello. Snipdeck asks only right after a password unlock. Opening Settings or
  the unlock window never puts up a Hello prompt. If you cancel that prompt, Hello stays on and it asks again
  after your next password unlock.
- **Lock.** The library still locks after 15 minutes without use, as described above. Hello is another way
  back in; locking does not turn it off. Closing the unlock window stops a Hello prompt under way.
- **Change, remove or erase.** Changing the master password re-seals the Hello copy with one prompt. Setting or
  removing the password, or erasing the library, turns Hello off. If the licence stops being Supporter, the
  saved Hello copy is dropped and only the password unlocks.

Hello never stores your key or password on disk. Snipdeck keeps an encrypted copy of the key in memory only,
which a restart forgets. The details are in [`crypto.md`](crypto.md). This part works only on Windows and has
not yet been tested on a real PC.

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
- **Espanso (folder)** or **Espanso (single .yml file):** Espanso's configuration folder (its `match` files) or
  one match file. Each file becomes a list. Files whose name starts with `_` (Espanso treats them as switched off),
  matches with only a `regex` trigger, and matches with no text are left out, and the preview says why.
- **Beeftext export:** the JSON file Beeftext exports.
- **TextExpander export (CSV)** and **PhraseExpress export (CSV):** the CSV file each program exports.

From Espanso, Beeftext, TextExpander and PhraseExpress, each snippet's abbreviation is kept as its trigger.

Snipdeck reads the source first and shows a preview before it changes anything: how many items and lists will
be added, how many new boards will be made, how many items you already have (these are skipped), and a **Will
not be imported** list with the reason for each item left out, such as a file over 5 MB, an empty file, a
picture or file that CopyQ or Ditto holds, or a sensitive item that this PC has no key for. Choose **Import**
to go ahead or **Cancel** to leave your library as it was. Everything arrives under one new board named after the
file or folder. The Free plan's board limit still applies, and the preview says when an import would pass it.
Notes in the preview, such as which item they belong to ("entry 3", "Ditto clip 7" or a file name with the
match number), are shown in your language like the rest of Snipdeck.

**Restore.** Restoring a backup in Settings shows the same preview. A restore puts each board back under its
own name.

**Windows clipboard history.** On Windows, **Windows clipboard history** is a fifth choice. It brings in the
text that Windows still holds in its Win+V history for this sign-in (Windows clears it at sign-out), newest
first, as one new board, after the same preview. Pictures are not imported. If clipboard history is off in
Windows, or holds no text, or Windows refuses access, the preview says so and offers nothing to import; turn
the history on under Windows Settings ▸ System ▸ Clipboard. For what you copy from now on, turn on the
[Recent inbox](#recent).

## Keyboard reference

| Where | Keys | Action |
|---|---|---|
| Anywhere | `Ctrl+Shift+Y` | Show or hide the Panel |
| Anywhere | `Ctrl+Shift+Space` | Open the Picker |
| Anywhere | none until you set one | Save the clipboard text as a new item (Capture) |
| Anywhere | `Ctrl+Shift+U` | Paste next, or the next item of a paste stack (Supporter; not registered on the Free plan) |
| Panel | `Up`, `Down`, `PageUp`, `PageDown` | Move the selection |
| Panel | `Enter`, `Shift+Enter` | Use, or copy only |
| Panel | `Alt+1` to `Alt+9` | Use that row (digits without `Alt` are search text) |
| Panel | `Ctrl+Tab`, `Ctrl+Shift+Tab` | Next or previous list |
| Panel | `Ctrl+F` | Focus the search box |
| Panel | `Ctrl+D` | Duplicate |
| Panel | `Ctrl+K` | Commands (opens them in the Main window) |
| Panel | `Ctrl+N`, `Ctrl+Shift+N` | New item, new list |
| Panel | `Ctrl+,` | Settings |
| Panel | `Ctrl+/` | Show the keyboard shortcuts |
| Panel | `Ctrl+W` | Hide the Panel |
| Panel | `Ctrl+Z` | Undo the last delete, grouping, group rename, ungroup or group drag (search box empty) |
| Panel | `Ctrl+E`, `F2` | Edit the selected item |
| Panel | `Ctrl+P` | Pin or unpin the selected item |
| Panel | `Ctrl+G` | Group the selected rows |
| Panel | `Left`, `Right`, `Enter` or `Space` (on a group header) | Fold or unfold the group |
| Panel | `Alt+Up`, `Alt+Down` (on a group header) | Move the whole group |
| Panel | `Menu`, `Shift+F10` (on a group header) | Open its menu: Rename group, Ungroup, Delete group |
| Panel | `F2`, or `Ctrl+G` with nothing selected (on a group header) | Rename the group |
| Panel | `Delete` | Move the selected item to the Trash (not while searching) |
| Panel | `Menu`, `Shift+F10` | Open the item menu |
| Panel | `Esc` | Clear the search; with nothing to clear, hide the Panel |
| Picker | `Tab`, `Shift+Tab` | Change scope |
| Picker | `Alt+1` to `Alt+9` | Use that row |
| Picker | `Ctrl+E`, `Esc` | Edit, close |
| Main window | `Ctrl+K`, `Ctrl+Shift+P` | Commands |
| Main window | `Ctrl+/` | Show the keyboard shortcuts |
| Main window | `Ctrl+N`, `Ctrl+Shift+N` | New item (the quick-add box in an open list), new list |
| Main window | `Ctrl+,` | Settings |
| Main window | `Ctrl+W` | Close the window |
| Main window | `Ctrl+B` | Show or hide the sidebar |
| Main window | `Ctrl+Z` | Undo the delete (not while typing in the search box) |
| Main window | `Ctrl+F`, `/` | Search |
| Main window | `Enter`, `Shift+Enter` | Use, or copy only |
| Main window | `Ctrl+E`, `F2` | Edit |
| Main window | `Ctrl+D` | Duplicate |
| Main window | `Ctrl+P` | Pin or unpin |
| Main window | `Delete` | Move to the Trash (in the Trash: delete for good) |
| Main window | `Ctrl+A`, `Ctrl+Space` | Select all, toggle a row |
| Main window | `Alt+Up`, `Alt+Down` | Move an item or list |
| Main window | `Ctrl+G` | Group the selected rows (the row at the cursor when none is selected) |
| Main window | `Left`, `Right`, `Enter` (on a group header) | Fold or unfold the group |
| Main window | `F2` (on a group header) | Rename the group |
| Main window | `Alt+Up`, `Alt+Down` (on a group header) | Move the whole group |
| Main window | `Delete` (on a group header) | Move the group and its items to the Trash |
| Panel | `Alt+Up`, `Alt+Down` | Move the selected item |
| Panel, Main window | `Space` (hold) | Reveal a sensitive item |
| Main window | `F1` | Open the Help window |
| Any program (Typing on) | the item's trigger | Replace the trigger with the item |
| Any program (Suggest on) | `Tab` while the hint shows | Insert the suggested item |
| Unlock window | **Unlock with Windows Hello** | Unlock with face, fingerprint or PIN |
| Item editor | `Ctrl+Enter` | Save |
| Fill form | `Ctrl+Enter`, `Esc` | Insert, cancel |

## Drag and drop

Every drag has a keyboard or menu way to do the same thing.

- **Reorder snippets:** drag a snippet onto another one, in the Panel or in the Main window (list or tile
  view), in a list that is in manual order and with no search running. Keyboard: `Alt+Up` or `Alt+Down`, or
  **Move up** / **Move down** in the snippet's menu.
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

