# Privacy: what Snipdeck stores, where, and how to remove it

Snipdeck is local-only. It has no account, no server, no telemetry and opens no network port. No setting
makes a network request; every icon and font it shows is packaged with it.

## What is stored

| What | Where | Notes |
|---|---|---|
| Your library (boards, lists, items, tags, use counts) | `%LOCALAPPDATA%\Snipdeck\snipdeck.db` | SQLite, write-ahead log, `secure_delete` on so deleted rows are overwritten |
| Large item content (images, files, long text) | `%LOCALAPPDATA%\Snipdeck\blobs\` | Content-addressed files, written atomically |
| Settings | `%LOCALAPPDATA%\Snipdeck\settings.json` | Plain JSON you can read and edit |
| Licence key (if you entered one) | `%LOCALAPPDATA%\Snipdeck\licence.key` | One line of text, checked offline at every start |
| Daily backups | `%LOCALAPPDATA%\Snipdeck\backups\` | Zipped `.snipdeck` exports. The newest 7 are kept; you can choose 1 to 60 |
| Exports and list text files (only when you ask) | Your Documents folder | A `.snipdeck` library export, or a `.txt` file from **Export as text** (no sensitive items) |
| Trash | inside `snipdeck.db` | 30 days, then purged |
| Crash log | `%LOCALAPPDATA%\Snipdeck\logs\` | Only written when the app crashes; never sent anywhere |
| Launcher logs and doctor reports (`Snipdeck.bat`) | `%LOCALAPPDATA%\Snipdeck\logs\` | Launcher output with your user name hidden; the newest 10 are kept |

The data folder never roams between PCs. Nothing is written outside it except the Windows autostart
entry (if you turn it on), the clipboard when you use an item, and the files you ask for in Documents.

## Sensitive items

Items marked sensitive are masked in the UI, excluded from Windows clipboard history and cloud
clipboard, and encrypted at rest on every plan with a key protected by Windows DPAPI. Titles, tags and search
text are not encrypted ([`crypto.md`](crypto.md)).

**Master password (Supporter).** You can add a password on top of Windows' protection. Snipdeck asks for it
the first time you need a sensitive item, and again after 15 minutes without use. Snipdeck never stores the
password. If you forget it, your sensitive items can never be opened again: there is no recovery key.
While the library is locked, backups, exports and restores wait, so a copy never lacks your sensitive items.

**Clipboard clear.** When you copy a sensitive item instead of pasting it, Snipdeck clears it from the
clipboard after 30 seconds, if nothing else has been copied since. Change the time, or turn it off with `0`,
under **Privacy** in Settings. Pasting restores your own clipboard instead. A text file made by **Export as
text** leaves sensitive items out, because it is plain text.

<!-- TODO(C1): Recent inbox. When it ships, say that it is off by default, what it keeps (last 200 entries or 7 days), what it skips (sensitive and password sources, and formats Windows marks "exclude from history"), and how to clear it. -->

## Removing everything

1. Optional: export your library first (Settings ▸ Your data ▸ Export library).
2. Quit Snipdeck from the tray icon.
3. Delete `%LOCALAPPDATA%\Snipdeck`. That is all of it.

Licence keys are verified offline ([`licensing.md`](licensing.md)). Vulnerabilities: see
[`../SECURITY.md`](../SECURITY.md).
