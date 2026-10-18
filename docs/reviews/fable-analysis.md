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

## Architecture redesign (Fable, 2026-10-18)
Measured: `controller.rs` is ~1010 lines of code + ~1280 lines of tests in one file; it owns panel state, picker, fill form, quick capture, paste stack, tray, hotkeys, toasts, settings saving and theme. `binding.rs` (612) mirrors it for one window. Agent A adds Main window logic beside it.
Target shape of `snipdeck-app` (replace, do not layer; each file < 800 lines, tests in sibling `tests.rs` files like `snipdeck-core`):
```
controller/mod.rs      Controller facade: new, view(), on_hotkey, on_tray, on_target_changed, show/hide/close, Effect, Library trait
controller/panel.rs    impl Controller: query, selection, tabs, board menu, add_list, activate/quick use
controller/picker.rs   impl Controller: open/close/query/scope/move/take/use
controller/fill.rs     impl Controller: open/changed/cancel/submit (prompted templates)
controller/stack.rs    impl Controller: quick_capture, paste_next (paste stack)
controller/toast.rs    impl Controller: show_toast, toast_tick, report_hotkey_problems, wording
controller/settings.rs impl Controller: update_settings, set_theme, saver polling, look
controller/tests/*.rs  the existing tests, split by the same seams
main_window.rs         MainModel (Slint-free view-model) + MainActions impl (agent A)
maintenance.rs         startup/daily jobs: trash purge (30 d), backup (24 h, keep 7, encrypted when keyed), integrity check (weekly), crash-log rotation
binding/{panel,main,editors,settings,picker,fill}.rs  one Slint binding per window; windows.rs owns creation/show/raise and the thread-local registry
```
Rules: every store access goes through `Library`; view-models are pure functions of (Library, Settings, query, selection); bindings only diff a view into Slint properties and forward callbacks; `Effect` stays the single channel from logic to the host. Mechanical move, no behaviour change: an Opus-free Sonnet task after agent A merges, verified by the unchanged test list (`cargo test -p snipdeck-app -- --list | sort` before and after must match).
Maintenance design (C1): run once 2 s after the first window shows (single-shot timer, off the hot path) and then every 6 h on a Slint timer; all decisions on the injected `Clock`; `backup` via `snipdeck_portable::backup` into `<data>/backups/snipdeck-YYYYMMDD.snipdeck`, keep the newest 7, skip when the newest is < 24 h old; purge `TrashRepo::purge(now - 30 d)`; `integrity_check` weekly with a danger toast + "Report a problem" on failure; every step logged under `SNIPDECK_LOG`. Tests with the frozen clock: a backup on day 1, none on day 1 + 1 h, rotation at 8 files, purge boundary at exactly 30 d.

## Optimisation, backend design, security and privacy (owner ask 2026-10-18)
What is already right: event-driven (no polling loops in app or platform; the settings saver runs on a quiet timer), no network crate in the app (`tokio` is dev/build only), SQLite WAL + `secure_delete` + atomic blob writes with fsync, zip import behind `SafeArchive` limits and BLAKE3 checks, DPAPI user-scope with UI forbidden, clipboard writes carry the history/monitor/cloud exclusion markers, data in `%LOCALAPPDATA%` (never roams).
- [ ] P1 No `[profile.release]` in `Cargo.toml`: ship with `lto = "fat"`, `codegen-units = 1`, `opt-level = 3` (or `"s"` for the installer budget < 15 MB, SPEC 9), `panic = "abort"` only after C4's panic hook writes the crash log, `strip = true`, `debug = false` + a separate `[profile.release-debug]` with symbols for the crash log. Measure the exe size and cold start before/after (SPEC 9: < 400 ms to Panel).
- [ ] P2 No performance test exists for the budgets of SPEC 9 except search: add `benches/` (criterion is heavy; a `--locked` test that asserts search < 16 ms on 10k items and Panel model load < 50 ms on the demo library, with a generous CI factor) and a release-size check in CI.
- [ ] P3 Panel model reload: `PanelModel::load` reads boards, lists, items and `all_items()` on every refresh; with 10k items that is a full scan per keystroke-free refresh. Keep a cached in-memory index per board, invalidated by the store's change counter, and search with `nucleo` over it (SPEC 5.1 measured 4-8 ms).
- [ ] S1 `SNIPDECK_LOG=1` logs every callback with its arguments (`binding.rs:226` `Shown(&[...])`): the search query and typed text land in `app-*.err.txt`. Redact string arguments to their length, keep ids and enums.
- [ ] S2 Privacy promise is implicit: add `docs/privacy.md` + a Settings "Privacy" page stating exactly what is stored (db, blobs, settings, backups, crash log), where, that nothing is sent, and how to wipe it (one button: export then delete the data folder).
- [ ] S3 Sensitive items at rest: the keyring exists but the app never provisions a key (see C2), so "sensitive" currently means masked in the UI and excluded from clipboard history, not encrypted. Until the master-password UI ships, provision the DPAPI-bound library key silently on first run (user scope) so sensitive content is encrypted at rest with zero UI; the master password becomes an upgrade later.
- [ ] S4 Clipboard auto-clear for sensitive items (SPEC 5.7): after a copy of a sensitive item, clear the clipboard after N s (default 30) if it still holds our sequence number; a toast says so.
- [ ] S5 Backups (C1) must be encrypted when the library has a key, or they leak the sensitive items in plain `.snipdeck` zips: reuse the export's encrypted-format path or wrap the backup with the library key.
- [ ] S6 Single instance uses `Local\Snipdeck-<id>` mutex + event: fine (per-session namespace). Verify the raise event cannot be abused by another user's process on a shared PC (Local\ is per logon session, so no); document in `docs/security.md`.
- [ ] S7 Launcher (`tools/launcher`): the prebuilt-zip path (HANDOVER 9c) must verify SHA-256 against a value pinned in the script or a signed release manifest, never a hash downloaded next to the zip; pin the download URL to the GitHub Release of a tagged version.
- [ ] B1 Backend seams: `controller.rs` 2292 lines (H1) owns hotkeys, tray, toasts, picker, fill, settings saving and the panel; `Library` trait is the right seam, keep every store call behind it and give the Main window its own view-model so the controller only routes.
- [ ] B2 Trash semantics: `TrashRepo::purge(cutoff)` is time-based only; the 5 s undo needs `purge_entry` (agent A is adding it) and the Main window needs "Delete forever" per entry.
- [ ] B3 Migrations are forward-only with a pre-migration backup (good). Add a `--check-db` CLI that runs `integrity_check` and the blob audit and prints a report, so support can ask for one line.

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
