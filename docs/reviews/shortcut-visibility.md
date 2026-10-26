# Shortcut visibility audit

Owner rule: hotkeys are boosts; every hotkey action must also have a visible button or menu item. This audit
walks every shortcut in the Help > Shortcuts catalogue (`CATALOGUE` in `crates/snipdeck-app/src/shortcuts.rs`, 52
rows) and every palette command (`Command::ALL` in `crates/snipdeck-app/src/commands.rs`, 33 rows) and names the
visible control that does the same thing.

Date: 2026-10-24. Method: read of the Slint files and the controller (no running Windows build), so "visible"
means "there is a control in the Slint tree"; how it looks on Windows is unverified.

"Palette" means the command palette (Ctrl+K, or the toolbar button "Commands (Ctrl+K)" in Main, or the Panel's
kebab menu "All commands…"). It is itself a visible control, so a palette command counts as a visible way.

## Result

Two gaps were found and fixed (both added to the palette), one gap is left open on purpose (the Picker), and
everything else already had a visible control.

| Gap | Fix | Where |
|---|---|---|
| Paste next (global hotkey, default Ctrl+Alt+V) had no visible control anywhere | New palette command "Paste next item", runs the same `paste_next()` as the hotkey | `Command::PasteNext`, `commands_ctl.rs` |
| Select all (Ctrl+A in Main) had no visible control | New palette command "Select all items" | `Command::SelectAll`, `commands_ctl.rs` |
| Picker: Ctrl+E (edit the selected result) has no button in the Picker | Not fixed: needs a control in the Picker window (a design change outside the palette/menu scope of this task). Suggested: a pencil icon button on the selected row. Editing the same item is available from Main and Panel item menus. | open |

Row density and theme (new in this branch) have a visible control in every place: Settings > Appearance, the Main
View menu and the palette. The Panel pin has its header button.

## Anywhere in Windows (global hotkeys)

| Hotkey | Action | Visible control | Status |
|---|---|---|---|
| Ctrl+Alt+Space (setting) | Show or hide the Panel | Main Help menu "Show Panel"; palette "Show Panel"; tray "Open Panel" | ok |
| Ctrl+Shift+Space (setting) | Open the Picker at the cursor | Palette "Open Picker" | ok (palette only; the Picker opens at the caret of another app, which a click inside Snipdeck would lose) |
| Capture (setting, empty by default) | Quick capture the clipboard | Palette "Quick capture"; Panel empty state "Save from clipboard"; tray "Take screenshot" for the screen variant; palette "Take screenshot" | ok |
| Ctrl+Alt+V (setting) | Paste the next item of a stack | Palette "Paste next item" (new); Panel stack bar shows the next title and Start / End | fixed |

## Main window

| Key | Action | Visible control | Status |
|---|---|---|---|
| Ctrl+K | Command palette | Toolbar button "Commands (Ctrl+K)" | ok |
| Ctrl+N | New item | Button "New item"; palette "New item" | ok |
| Ctrl+Shift+N | New list | Sidebar Lists "+" button; palette "New list" | ok |
| Ctrl+F | Search | The search field in the toolbar; palette "Search" | ok |
| Ctrl+, | Settings | Sidebar "Settings" row and icon; palette "Settings" | ok |
| Ctrl+/ | Keyboard shortcuts | Help menu "Keyboard shortcuts"; palette | ok |
| F1 | User guide | Help menu "User guide"; palette | ok |
| Enter | Use the item | Item menu (first row, label follows the item); bulk bar; palette "Use selected item" | ok |
| Shift+Enter | Copy the item | Item menu "Copy"; palette "Copy selected item" | ok |
| Ctrl+E | Edit the item | Item menu "Edit"; palette "Edit selected item" | ok |
| Ctrl+D | Duplicate the item | Item menu "Duplicate"; palette | ok |
| Ctrl+P | Pin or unpin | Item menu "Pin" / "Unpin"; bulk bar "Pin"; palette | ok |
| Del | Move to Trash | Item menu "Move to Trash"; bulk bar; palette | ok |
| Ctrl+A | Select every item | Palette "Select all items" (new) | fixed |
| Ctrl+Z / Ctrl+Y | Undo / Redo | Toolbar Undo and Redo buttons (tooltip names the step); the Undo action on the toast | ok |
| Ctrl+W | Close the window | The window's close button | ok |
| Alt+Up / Alt+Down | Move the item up or down | Item menu "Move up" / "Move down" | ok |

## Panel

The Panel is being redesigned; this lists what the current Panel offers and was not changed.

| Key | Action | Visible control | Status |
|---|---|---|---|
| Type | Filter as you type | The search field | ok |
| Up / Down | Move through the items | Mouse and wheel | ok (navigation) |
| Enter / Shift+Enter | Use / copy | Click the row; item menu "Use", "Copy" | ok |
| Alt+Up / Alt+Down | Move the item | Item menu "Move up" / "Move down"; dragging the row | ok |
| Alt+1 to 9 | Use the Nth item | Click the row (the 1 to 9 numbers on the rows name the key; "Show numbers" in the kebab menu hides them) | ok |
| Ctrl+Tab | Next list | The list tabs | ok |
| Ctrl+F | Search | The search field | ok |
| Ctrl+E, Ctrl+D, Ctrl+P | Edit, duplicate, pin | Item menu | ok |
| Ctrl+Shift+P (new) | Keep the Panel on top | Header pin button "Keep panel on top" | ok |
| Ctrl+K | Commands in the Main window | Kebab menu "All commands…" | ok |
| Ctrl+N | New item | Kebab menu "New snippet" | ok |
| Ctrl+Shift+N | New list | The "Add list" button next to the tabs | ok |
| Ctrl+, | Settings | Kebab menu "Settings" | ok |
| Ctrl+Z / Ctrl+Y | Undo / Redo | Header Undo and Redo buttons | ok |
| Ctrl+W | Hide the Panel | Header "Close panel (Esc)" button | ok |
| Ctrl+/ | Keyboard shortcuts | Kebab menu "Keyboard shortcuts" | ok |
| Esc | Clear the search, then close | Header "Close panel (Esc)" button | ok |

## Picker

| Key | Action | Visible control | Status |
|---|---|---|---|
| Up / Down | Move through the results | Mouse and wheel | ok (navigation) |
| Enter / Shift+Enter | Paste / copy the result | Click the row pastes; copy has no button | partial: copy-only is keyboard only; the Panel and Main offer it in the item menu |
| Alt+1 to 9 | Paste the Nth result | Click the row | ok |
| Tab | Next scope | The three scope chips (All, This board, Pinned) | ok |
| Ctrl+E | Edit the result | none | open (see Result) |
| Esc | Close | Click outside closes it; no button | ok (a transient popup) |

## Editors

| Key | Action | Visible control | Status |
|---|---|---|---|
| Ctrl+Enter | Save | The "Save" button (tooltip "Save (Ctrl+Enter)") | ok |
| Esc | Close | The "Cancel" and "Close" buttons | ok |

## Palette commands against the controls

Every palette command is itself a visible menu row. These also have another control: New item, New list, Search,
Show Panel, Settings, User guide, Shortcuts, Troubleshoot, About, Tile view and the row styles (toolbar segments),
the item commands (item menu), Go to All / Pinned / Recent / Trash (sidebar). Commands that live only in the
palette on purpose: Open Picker, Quick capture, Paste next item, Select all items, Welcome tour, Reset Panel size
and position (also in the Panel kebab menu), and the three Theme and two Row density commands (also in the Main
View menu and Settings > Appearance).

## Not verified

Everything here is from reading the sources and the headless Slint tests. Whether the Panel kebab "All commands…"
and the palette commands behave on Windows with another window in front (Paste next from the palette pastes into
the window that was active before Snipdeck, using the same path as the hotkey) was not run on Windows.
