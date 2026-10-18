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
