# Privacy: what Snipdeck stores, where, and how to remove it

Snipdeck is local-only. It has no account, no server, no telemetry and opens no network port. The one
setting that can make a network request (site icons for link items) is off by default and says so.

## What is stored

| What | Where | Notes |
|---|---|---|
| Your library (boards, lists, items, tags, use counts) | `%LOCALAPPDATA%\Snipdeck\snipdeck.db` | SQLite, write-ahead log, `secure_delete` on so deleted rows are overwritten |
| Large item content (images, files, long text) | `%LOCALAPPDATA%\Snipdeck\blobs\` | Content-addressed files, written atomically |
| Settings | `%LOCALAPPDATA%\Snipdeck\settings.json` | Plain JSON you can read and edit |
| Licence key (if you entered one) | `%LOCALAPPDATA%\Snipdeck\licence.key` | One line of text, checked offline at every start |
| Daily backups | `%LOCALAPPDATA%\Snipdeck\backups\` | Zipped `.snipdeck` exports, newest 7 kept |
| Trash | inside `snipdeck.db` | 30 days, then purged |
| Crash log | `%LOCALAPPDATA%\Snipdeck\logs\` | Only written when the app crashes; never sent anywhere |
| Launcher logs (`Snipdeck.bat`) | `dist\logs\` next to the launcher | Build output and, with `SNIPDECK_LOG=1`, debug tracing |

The data folder never roams between PCs. Nothing is written outside it except the Windows autostart
entry (if you turn it on) and the clipboard when you use an item.

## Sensitive items

Items marked sensitive are masked in the UI, excluded from Windows clipboard history and cloud
clipboard, and (Pro) encrypted at rest with a key protected by Windows DPAPI and, optionally, a master
password. Titles, tags and search text are not encrypted ([`crypto.md`](crypto.md)).

## Removing everything

1. Optional: export your library first (Main window ▸ kebab ▸ Export).
2. Quit Snipdeck from the tray icon.
3. Delete `%LOCALAPPDATA%\Snipdeck`. That is all of it.

Licence keys are verified offline ([`licensing.md`](licensing.md)). Vulnerabilities: see
[`../SECURITY.md`](../SECURITY.md).
