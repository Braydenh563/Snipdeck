# Microsoft Store listing (English, for Partner Center)

The text the owner pastes into Partner Center ▸ Store listing, and the certification notes. Rules it follows
(`docs/store-submission.md` section 8, policy 10.1): plain text, no URLs or HTML in the description, every
limit stated, no other products' names in the search terms. The Store build is free: every feature unlocked,
no Get Supporter, no licence key (`.claude/now.md` Decisions, 2026-11-14).

The manifest declares seven UI languages (`packaging/msix/AppxManifest.xml`). Policy 10.7 asks for Store text in
each declared language: the owner adds those listings in Partner Center (the text below is English only).

## Product name

Snipdeck

## Short description (270 characters shown)

Keep the text, links, templates, images and files you paste every day on a small panel at the edge of your
screen. Click one and it lands where you were typing. Local only: no account, no cloud, nothing sent anywhere.

## Description

Snipdeck is a snippet deck for Windows. The Panel sits at the edge of your screen and never takes the focus
from the app you are typing in, so one click pastes a snippet straight into it. When your hands are on the
keyboard, the Picker opens a search box at your text cursor: type a few letters, press Enter, done.

You arrange the deck yourself. Boards hold lists, lists hold items, and an item can be text, code, a link, a
template, an image or a file. Templates fill in the date, the time, or fields and choices they ask you for.
The Main window gives you every board, tag, pinned item and the Trash, with search, drag and drop and a
command palette.

Your snippets stay on this PC. Snipdeck has no account and no server, and it never connects to the internet.
Items you mark Sensitive are hidden in lists, kept out of Windows clipboard history and encrypted on disk;
you can add a master password and unlock with Windows Hello.

Optional features that ask first or are off until you turn them on:
- Recent inbox keeps the text you copy in other programs (the last 200 copies, 7 days) so you can save it to a
  list later. Snipdeck asks on first start and keeps nothing until you say Yes. It skips password managers
  and Sensitive items, stays on this PC, is encrypted, and can be turned off in Settings.
- Expand abbreviations as I type and Suggest snippets as I type watch your typing in other programs only
  while turned on. Typed text is never stored or sent.
- Start with Windows is off until you turn it on.

Requirements: Windows 10 version 1903 or newer, or Windows 11, 64-bit (x64). Interface in English, German,
Spanish, French, Japanese, Portuguese (Brazil) and Simplified Chinese.

Snipdeck is free software under the GPL-3.0 licence. Every feature is included in this version.

## Product features (one per line, up to 20)

One-click paste from a panel that never steals the focus from your app
Picker: search every snippet from your text cursor and paste with Enter
Dock the Panel to a screen edge and fold it into a slim tab
Boards, lists, tags, pinned items and colour labels
Text, code, link, template, image and file items
Templates with date, time, fields and choices
Quick capture: save the clipboard as a new item with a hotkey
Recent inbox of copied text, only after you say yes, encrypted, on this PC only
Sensitive items: hidden, kept out of clipboard history, encrypted on disk
Master password with Windows Hello unlock
Optional abbreviation expansion and suggestions as you type, off by default
Paste a queue of items one after another
Import from a Snipdeck file, other clipboard tools or a folder of text files
Trash with Undo for 30 days and automatic daily backups
Light, dark or Windows theme with an accent colour
Hotkeys you can change
Seven interface languages
No account, no cloud, no telemetry

## Search terms (at most 7)

snippets, text expander, clipboard, paste, templates, productivity, quick text

## What's new in this version

First Microsoft Store release.

## Screenshots

PNG, 1366x768 or larger, real UI with the demo library (`Snipdeck.bat demo`), no added text or logos. Files and
captions: see "Screenshots" in `docs/store-submission.md` section 8. Store logo (1:1 app tile icon, 300x300):
`docs/images/store-logo-300.png`, rendered from `assets/logo/snipdeck-icon.svg` by
`python3 scripts/make_msix_assets.py`.

## Support and privacy

- Website: the GitHub repository page.
- Support: the repository's Issues page.
- Privacy policy: a public URL of `docs/privacy.md` (owner: publish it, see `docs/store-submission.md` policy risk 3).

## Certification notes (Submission options ▸ Notes for certification)

> Snipdeck is a local snippet deck. It needs no account, no server and no network. Notes:
> 1. Capability runFullTrust: Snipdeck is a Win32 app. It registers global hotkeys (RegisterHotKey) to show a panel, keeps a tray icon, reads and writes the clipboard, and sends Ctrl+V (SendInput) to the app the user was typing in, so a chosen snippet is pasted there. None of that works inside an AppContainer.
> 2. Optional features, off by default or opt-in: "Expand abbreviations as I type" and "Suggest snippets as I type" use a low-level keyboard hook only while turned on, never log or store typed text, and ignore other keys. "Recent inbox" (clipboard listener) asks on first start and records nothing until the user says Yes; it keeps recent copied text on this PC only, encrypted, and can be turned off in Settings.
> 3. Start with Windows uses the StartupTask SnipdeckStartup and is off until the user turns it on.
> 4. Nothing is sent anywhere. Data is stored per package.
> 5. No locked features: every feature is available in this Store version; there are no purchases.
> Date entered: [date].
