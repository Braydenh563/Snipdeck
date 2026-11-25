# Default global chords (decision 2026-10-26)

New installs register these; an existing `settings.json` keeps the chords it names. A file that
omits a chord keeps the old one (`Settings::from_json`), so only a missing file gets the new set.

| Action | Old default | New default |
|---|---|---|
| Show or hide the Panel | `Ctrl+Alt+Space` | `Ctrl+Shift+Y` |
| Picker | `Ctrl+Shift+Space` | unchanged |
| Paste next (Supporter) | `Ctrl+Alt+V` | `Ctrl+Shift+U` |
| Capture | off | unchanged |

## Why not Ctrl+Alt
Windows treats AltGr as Ctrl+Alt, so on many European layouts `Ctrl+Alt+<letter>` types a character
or is eaten by the layout. `Ctrl+Alt+V` is also Paste Special in Word and Excel.

## Why not Win+<key>
The `RegisterHotKey` page says of `MOD_WIN`: "Keyboard shortcuts that involve the WINDOWS key are
reserved for use by the operating system", and Windows 11 already uses Win+V (clipboard history),
Win+. (emoji), Win+Shift+S (snipping), Win+Space (input language) and many more. The call may
succeed, but a later Windows update can claim the chord, so the defaults use Ctrl+Shift.

## Why Y and U
Ctrl+Shift+letter is layout-safe (no AltGr). Most letters are taken: Ctrl+Shift+V (paste plain
text), N, P, T, W, B, D, E, F, K, L, M, O, S, Q (Chrome quits). Y and U have no default meaning in
Windows 11, Word, Excel, Outlook, Chrome or Edge (Firefox: Ctrl+Shift+Y opens Downloads; VS Code:
Y is the Debug Console, U the Output panel, both inside an editor and only when it has focus).
A chord another program already holds is reported in Settings > Shortcuts, and any chord can be
changed there. Not verified on a running Windows machine.

## Sources
- RegisterHotKey, MOD_WIN remark: https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerhotkey
- WM_HOTKEY: https://learn.microsoft.com/windows/win32/inputdev/wm-hotkey
- (Only the RegisterHotKey page was fetched for this note; the next three are the standard reference pages and the app-by-app claims are unverified.)
- Windows keyboard shortcuts: https://support.microsoft.com/windows/keyboard-shortcuts-in-windows-dcc61a57-8ff0-cffe-9796-cb9706c75eec
- Office shortcuts (Word/Excel): https://support.microsoft.com/office/keyboard-shortcuts-in-word-95ef89dd-7142-4b50-afb2-f762f663ceb2
- Chrome / VS Code shortcut references: https://support.google.com/chrome/answer/157179 , https://code.visualstudio.com/shortcuts/keyboard-shortcuts-windows.pdf
