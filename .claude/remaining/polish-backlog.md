# Polish backlog (owner request 2026-10-18 23:58: "the small things people expect")

Checked against the code on 2026-10-18 (grep; nothing below exists unless marked "exists").
Tiers: A = in v1 before ship (expected by anyone), B = v1 if time, C = after v1.

## Windows and app behaviour
- A1 Main window remembers size, position, maximised state (per monitor) and restores them; fit to the work area (fit_to_screen done).
- A2 Main window in the taskbar and Alt+Tab, never hidden by focus loss; close = hide to tray or quit per the existing "Keep running in the tray" setting; minimise works. Owner reports it hides today: verify on Windows with SNIPDECK_LOG=1.
- A3 Window icon (title bar + taskbar) on every window from the .ico (winit `with_window_icon`); the Panel keeps none.
- A4 Editors sized to their content (New list has a large empty bottom); dialogs centred on the Main window and kept above it (owner window).
- A5 Escape cancels an editor / closes Settings; Enter saves a one-line editor; Ctrl+Enter saves the item editor.
- A6 Settings "Window behaviour" group: Panel hides when it loses focus (on/off), Panel always on top, remember window positions, Main window at start-up (hidden to tray / open). "Start with Windows" exists.
- A7 Tray: double-click opens the Main window; tooltip names the app and "hotkeys paused"; exists: left click toggles the Panel, menu.
- A8 Second launch raises the Main window (single_instance exists; check it raises).
- B1 Keyboard: Ctrl+N new item, Ctrl+Shift+N new list, Ctrl+, Settings, Ctrl+F search, Delete = Move to Trash, Ctrl+D duplicate, F1 Help, Ctrl+W close, Ctrl+Q quit (none wired in main_window.slint today).
- B2 About page: version, licence state, data folder and log folder buttons, "Copy diagnostics".

## Items, lists, boards
- A9 Right-click menu on Panel rows and pinned chips (callbacks exist in panel.slint; host side not built).
- A10 Clipboard restore after an auto-paste: Snipdeck pastes by putting the item on the clipboard and sending Ctrl+V; the user's previous clipboard must come back afterwards (not implemented; every snippet tool does this).
- A11 Sensitive items: clear the clipboard after 30 s (docs promised it; no code) and show "cleared" in the toast.
- A12 Templates: `{clipboard}` filled from the real clipboard (controller passes None), `{date}` in the local time zone (jiff `tz-system` feature; today UTC); field defaults.
- A13 Duplicate item; Move to list…; Copy to board… (none exist; "Delete, Ctrl+E, F2" exist).
- A14 Mouse drag-and-drop to reorder items and lists, and to move an item onto another list in the sidebar (Alt+Up/Down exists).
- A15 Drop text, a file or an image from another app onto the Panel or a list to create an item (Slint drop events).
- A16 Add from clipboard: tray entry and Panel kebab entry; fix the "Capture a region" label to say what it does (saves the clipboard text) until region capture exists.
- B3 Full-text preview on hover (tooltip) for long snippets; usage count and last used in the Main window detail.
- B4 Multi-select with Shift/Ctrl click in the Main window (SelectMode exists: check mouse modifiers), bulk Move/Tag/Trash.
- B5 Tag manager: rename and delete a tag across items (store has `tags` and `delete_tag`; rename missing); the sidebar gear is a no-op today (owner found it).
- B6 Import from a plain text / markdown list; export a list as text.
- C1 Per-item hotkeys for pinned items (Ctrl+Alt+1..9).

## Feedback and learnability
- A17 First-hide toast (hotkey + tray click) and "Show Snipdeck in the taskbar" toggle (work order step 13).
- A18 Onboarding (step 7a) and Help window (7b) with the keyboard reference.
- A19 Toasts never take focus; one toast at a time; paste feedback names the target app.
- A20 Empty states carry the action (exists); "No results" offers "Create item named …".
- B7 Reduced motion setting (Windows "Show animations" respected).
- B8 "What's new" note after an update (CHANGELOG excerpt) and About shows the version.

## Reliability
- A21 Autosave drafts for the item editor (step 8 has drafts) and "unsaved changes" prompt on close.
- A22 Hotkey conflict: toast names the other app's key when known (exists partly: toast by failure kind).
- A23 Backups: "Open backups folder" and "Open data folder" buttons in Settings; restore shows what it will add.
- A24 Launcher: second start says "Snipdeck is already running" instead of building.
