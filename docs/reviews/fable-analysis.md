# Fable analysis (2026-10-18, running document)

Ranked findings. Status: `[ ]` open, `[~]` ticketed in HANDOVER, `[x]` fixed (commit).

## Critical (data loss, security, broken core flows)
- [ ] C1 No daily backup, no trash purge and no integrity check are ever run by the app (SPEC 5.7a: daily zipped `.snipdeck`, keep 7; Trash 30 days). The store has `backup`, `TrashRepo::purge`, `PRAGMA integrity_check`; `snipdeck-app` never calls them. Plan: a `maintenance` module run once at startup on the frozen clock (purge older than 30 days, backup if the newest is older than 24 h, rotate to 7) + a Settings "Restore backup".
- [ ] C2 Sensitive items: no clipboard auto-clear (SPEC 5.7), no master password, no encryption key lifecycle in the app (store supports it: `crypto/keyring.rs`). Sensitive rows are shown masked and stay out of history only via the platform markers.
- [ ] C3 Licence is not wired outside `--demo` (`main.rs:336`): every real user is Free; Pro keys cannot be entered. Needs the licence dialog (SPEC 5.3) + `SNIPDECK_LICENCE_PUBLIC_KEY` at release.
- [ ] C4 No crash log / panic hook (SPEC 5.7a): a panic on the UI thread loses the process silently; nothing for "Report a problem".
- [ ] C5 Picker and TemplateFill are computed in the controller (`View.picker`/`View.fill`) but never rendered: the Picker hotkey and prompted templates do nothing visible (PR4 review #13).

## High (architecture, robustness)
- [ ] H1 `crates/snipdeck-app/src/controller.rs` is 2292 lines (limit 800) and will grow with the Main window: split into `controller/{panel,picker,fill,tray,hotkeys,toast}.rs` behind one `Controller` facade, or move window-specific logic into per-window view-models.
- [ ] H2 Panel position / size memory per monitor (SPEC 5.1) is absent from `settings.rs`.
- [ ] H3 `windows/clipboard.rs` (1268 lines), `template.rs` (1005), `import.rs` (925), `blobs.rs` (902), `chord.rs` (866): over the 800-line limit; split by responsibility where a seam exists (clipboard: owner window + jobs; template: parse vs render).

## Medium / Low
- (to be filled from the pre-Fable facts file)

## UX: instinctive without a guide (design decisions, owner ask 2026-10-18)
Principle: every control answers "what happens if I click?" before the click, and the first five minutes teach by doing.
- U1 **First run teaches by doing** (SPEC 5.7b): seed a "Getting started" list whose cards are the lesson: "Click me to paste into the app you were in", "Shift+click copies instead", "Try a template: {date}", "Pin me (right-click > Pin)", "Press Ctrl+Alt+Space to hide and show me". Each card self-deletes when the user has used it twice (or a "Done, remove this list" card). No wizard pages beyond hotkeys + position.
- U2 **The Target footer is the main teacher**: it always reads "Click pastes into Notepad" / "Click copies (no text field in front)"; when the user hovers a row it switches to the row's concrete verb; Shift held changes it to "Shift+click copies". One line, verb first.
- U3 **Tooltips on every icon** (500 ms, keyboard-focus too), including rows' hover Copy/Paste/More, tabs' `+`, pin, kebab, close; keyboard shortcut shown in the tooltip ("Keep panel on top", "Search  Ctrl+F").
- U4 **Empty states explain the next action**, not the absence: empty list = "Nothing here yet. Save from clipboard (Ctrl+V), drag text onto this panel, or press + to write one." Empty search = "No match for 'x' in Work. Search all boards?" with a button.
- U5 **Discoverable power features**: the row's hover `⋯` opens the same context menu as right-click (Paste, Copy, Paste as plain text, Edit Ctrl+E, Pin, Move to, Delete Del); keyboard users get Shift+F10. Menus show shortcuts so they are learnt in passing.
- U6 **A visible way out of every state**: close hides to tray with a one-time toast "Snipdeck keeps running in the tray. Ctrl+Alt+Space brings it back."; Esc clears search before it hides; a stack/fill form has Cancel.
- U7 **Undo instead of confirmation for item actions**, confirmation only for lists/boards (SPEC 5.7a); the toast's Undo button is keyboard reachable (Ctrl+Z while the toast is up).
- U8 **The Main window kebab/Help**: Help opens `docs/user-guide.md` in the browser; About shows version, licence tier, data folder, "Report a problem".
- U9 **Names over icons where cheap**: the Panel header gets a small wordmark-less logo at left (owner item 6), the view toggles in the Main window are labelled in their tooltips, and the board switcher shows a caret so it reads as a dropdown.
- U10 Measure: `ux-auditor` (WCAG 2.2 target size, contrast, focus order) and `blind-tester` (first-run without docs) on the Panel and Main window after wave 2; every failed step becomes a ranked finding here.

## Missed features in scope (brainstorm)
Tiny (hours each, v1 candidates): duplicate item; "paste as plain text" per item is in SPEC, add "paste and press Enter" per item (chat/terminal one-liners); `{counter}`, `{uuid}`, `{random:a|b}` and `{selection}` template fields; transforms on paste (UPPER/lower/slug/trim) from the context menu; item expiry ("delete after 1 h", for one-time codes; opt-in); colour label per item; drag text from any app onto the Panel to create an item; tag autocompletion in the editor; "Recently used" row at the top of each list (toggle); copy the item's title with Ctrl+Shift+C; keyboard-only quick capture with title prompt; per-list default paste mode; "Move to…" with a type-ahead list chooser; sort by most used/recent in the Panel (SPEC lists it, not built); jump to item from search with Ctrl+Enter (reveal in list, HANDOVER gap); show item size and use count on hover; a portable mode (data folder next to the exe, `portable.txt` marker); `snipdeck.exe --export` CLI for backups in scripts.
Large (v1.x unless cheap): text expansion by abbreviation (type `;sig` in any app, Snipdeck replaces it; the biggest reason people buy TextExpander/Espanso, and the clipboard + paste machinery is already here); folder sync (a `.snipdeck` library folder in OneDrive/Dropbox with last-writer-wins per item and a conflict list; no server, matches the privacy promise); per-item global hotkeys (Pro); a command palette in the Main window (Ctrl+K: every action, every item); PowerToys Run / Windows Search plugin; a read-only "team board" imported from a shared folder; browser extension for save-selection with source URL; Windows Share target; jump list on the taskbar icon (last 5 items); "Smart paste" per-app rules UI (paste key, plain text, no-paste) instead of a hidden table.
Reliability and trust (ship-blocking in spirit): startup health check (integrity_check, blob audit, orphan cleanup) with a one-click repair; "Restore from backup" in Settings listing the 7 daily backups by date and size; editor autosave of drafts every 2 s to `drafts/` so a crash loses nothing; export-on-uninstall prompt; a privacy page in Settings that says exactly what is stored where and that nothing leaves the PC.

## Logo
Current (`assets/logo/snipdeck-icon.svg`, rendered at 128 px): three fanned cards with a blue spine on a near-black rounded tile; wordmark in IBM Plex Sans SemiBold. It is competent, but:
- L1 The near-black tile contradicts DESIGN.md ("warm card stock", Index Blue only for selection): use neutral-850 warm (hue 80) or the light `bg.app` tile with a hairline, so the icon matches the app's chrome in both themes.
- L2 At 16-32 px (tray, taskbar) three cards + fold + three text lines blur into a grey smudge; ship size-specific variants: 16/20/24 px = one card with the blue index tab and two bars; 32-48 px = two cards; 64+ = the full stack. `scripts/make_logo.py` should emit all sizes into the `.ico` (16-256) the exe and tray need (HANDOVER 9b, still missing).
- L3 The blue spine reads as a notebook/binder. The product metaphor is an index card deck: make the blue a left index *tab* (short, rounded, like the list tabs in the Panel) rather than a full-height spine, and drop the folded corner so the only accent is the tab.
- L4 The text bars are mid-grey on white: at small sizes contrast fails; use ink (neutral-850) bars, two of them, the first longer (title + preview, matching ItemCard anatomy).
- L5 The wordmark is fine; keep letter-spacing slightly tighter (-1%) and align its x-height with the tile's optical centre (currently sits a little low).
