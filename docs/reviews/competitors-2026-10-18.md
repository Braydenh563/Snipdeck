# Competitors and inspiration (2026-10-18)

Snipdeck's position: a visible, always-on-top deck of snippet cards at the screen edge, click to
paste into the app you were typing in, offline, portable. Nobody sells exactly that; the nearest
products fall into three groups.

## 1. Text expanders (type an abbreviation, it expands)
TextExpander (subscription, Mac-first), PhraseExpress (Windows, free for personal use, the most
automation, cluttered UI; has a clipboard manager and a phrase menu you can pop at the caret),
Text Blaze (browser first, forms with fields, Windows app), Espanso (open source, YAML files, no
GUI), Beeftext (open source, Windows, in maintenance mode), aText (one-time purchase).
Take: abbreviation triggers as an optional second way to use an item (`;sig` expands), form-style
fields with defaults and dropdowns (Text Blaze), a global search popup at the caret (PhraseExpress
phrase menu). Avoid: file-only configuration, subscriptions.

## 2. Clipboard managers (history first, pins second)
Windows Win+V (25 entries, pin survives restart; the free baseline every user already has), Ditto
(free, popup at caret, groups, hotkeys per clip), CopyQ (open source, tabs, scripts, editable
items, tray), PasteBar (open source, Mac + Windows, PIN-protected collections, choose the data
folder), PinClipBoard and ClipNest (pin boards with tags), Paste and Maccy on Mac.
Take: the Win+V pin model is what users know, so Snipdeck's pinned strip should behave like it;
Ditto's "paste by number" (Ctrl+1..9 in the popup); CopyQ's editable items and tabs; PasteBar's
protected collections and custom data location (Snipdeck has sensitive items and a portable
folder). A Recent inbox that captures copies is the bridge from "clipboard manager" to "deck".

## 3. Edge docks and sticky notes
Microsoft PC Manager (side toolbar that collapses to a strip, slides out on hover: the owner's
model for step 3a), Windows Sticky Notes (always-visible notes, colours, no paste action),
Stardock Fences / Sidebar Diagnostics (docked panels), Raycast and Alfred (Mac launchers with
snippets, Raycast now on Windows in beta).
Take: PC Manager's collapse-to-handle, Sticky Notes' colours and "always there" feel, Raycast's
single search box that pastes.

## What would make Snipdeck feel complete next to them (in order)
1. Clipboard restore after a paste and the 30 s clear for sensitive items (every manager does this).
2. Paste by number in the Picker and the Panel (Ctrl+1..9), like Ditto.
3. Abbreviation trigger per item (optional), like the expanders.
4. Recent inbox (last copies) with one click to keep, like Win+V pins.
5. Drag text or a file onto the Panel to make an item.
6. Form fields with defaults and choices in templates, like Text Blaze.
7. Protected collections (a board that asks for the master password), like PasteBar.

## Functions and design decisions worth adopting (owner question 2026-10-19 00:05)

Take (ordered by value for cost):
1. **Type-to-paste fallback** (Beeftext, PhraseExpress): when Ctrl+V is blocked (terminals, RDP, some
   games, password fields) send the text as key strokes. Setting per app in "Never paste into".
2. **Paste as plain text by default, formatted on request** (Ditto Shift+Enter, Win+V): our items are
   text, but links and template output should offer "paste as plain" and "paste as link".
3. **Actions menu on the selected card, one key** (Raycast Cmd+K, Ditto right click): Paste, Copy, Paste
   as plain, Edit, Pin, Move to, Duplicate, Trash, with the shortcut shown next to each.
4. **Paste by number** (Ditto): Ctrl+1..9 in the Picker and on the pinned strip.
5. **Search-as-you-type across everything with fuzzy match and recency ranking** (Raycast, Ditto); the
   Picker already searches, add "recently used first" and match highlighting.
6. **Cursor placeholder** `{cursor}` (TextExpander %|, Raycast, Espanso): after pasting, put the caret
   there (send Left arrows). Also `{date+3d}` date maths and `{time}`.
7. **Suggest a snippet** (TextExpander): when the same text is copied or pasted three times, offer
   "Keep as an item?" in the Recent inbox. No typing capture, no keylogging.
8. **Abbreviation trigger per item, opt-in** (PhraseExpress, Espanso, Beeftext): `;sig` expands. Needs a
   keyboard hook: ship after v1, as its own ADR, off by default.
9. **Paste from the tray menu** (CopyQ): the tray menu lists the pinned items; click pastes.
10. **Checklist and formatting in text items** (Sticky Notes): bold, bullets, check boxes; v1 keeps
    plain text, so just keep the first line as the title (we do) and show a bullet preview.
11. **Protected board** (PasteBar PIN collections): a board that opens with the master password.
12. **Starter boards** (Win+V symbols and emoji panels): a "Symbols" board (arrows, dashes, quotes,
    Greek) and an "Emoji" board as optional starter content in onboarding.
13. **Per-app rules** (PhraseExpress, Espanso): we have "never paste into"; add "copy only" and
    "type instead of paste" per app in one table.
14. **Statistics** (TextExpander): uses per item (we count) and "characters saved" in About.
15. **Portable mode flag** (Ditto, CopyQ): data beside the exe when a `portable` file exists; our
    launcher already keeps data in the folder, make it explicit.

Keep as our difference: the visible deck (nobody shows items on screen all the time), click to
paste without taking focus, offline and portable, one-time licence, no config files.

Leave alone: cloud sync and accounts (every expander sells it; we are offline), macro recorders
(PhraseExpress), YAML config (Espanso), browser extensions (Text Blaze), AI rewrite buttons
(Raycast, TextExpander 2025), usage-based pricing.
