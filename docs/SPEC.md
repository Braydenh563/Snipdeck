# Snipdeck — Product Specification (v1)

> Status: **Agreed** (grill rounds 1–2, 2026-10-17), amended with M0 spike outcomes (see [`docs/spikes/README.md`](spikes/README.md)). Anything marked **(unverified)** depends on a Windows test that has not been run yet. Design language: [`DESIGN.md`](../DESIGN.md).
> Working name: **Snipdeck** — kept in one constant (`APP_NAME`); trademark/domain check before release.

## 1. Problem

People who work at a computer re-paste the same things (replies, addresses, code blocks, IDs, images,
structured boilerplate) dozens of times a day. Clipboard-history tools bury these in noise; text
expanders need memorised abbreviations. Snipdeck is a **curated, always-at-hand deck of snippets**
that lives at the edge of the screen like a sticky note: one click and it's pasted where you were typing.

## 2. Principles

1. **One click to use.** The primary action on any item is a single click.
2. **Never steal focus.** Using the panel must not disturb the app the user is typing in.
3. **Native, light, instant.** No web runtime, no server, no network port. Strict perf budget (§9).
4. **Local-first & private.** No account, no telemetry, data on disk in an open format.
5. **Library first, history second.** Curated items are the product; clipboard history is a local inbox, on for new installs and off with one switch.

## 3. Platforms

| Release | Platform | Notes |
|---|---|---|
| v1.0 | Windows 10 (1903+) / 11 | Primary target |
| Later | macOS 13+ | Needs Accessibility permission for paste-into-field |
| later | Linux (X11 full, Wayland copy-only) | Wayland blocks key injection |

## 4. Domain model (glossary)

| Term | Meaning |
|---|---|
| **Board** | Top-level workspace (e.g. "Work", "Personal"). No limit on boards. |
| **List** | Ordered collection of Items inside a Board (e.g. "Email replies"). Has optional colour accent and an optional icon (one of 24 curated glyphs). |
| **Item** | One saved thing the user can paste. Has a title, one or more Formats, tags, flags, and an optional colour label (one of 8; a 4px bar on the card, filterable in the Main window; the name is always available as text). |
| **Format** | One clipboard representation of an Item: `text`, `html`, `rtf`, `image/png`, `files`. An Item may hold several (e.g. html + text fallback). |
| **Template** | An Item whose text contains placeholders, resolved at use time. |
| **Placeholder** | `{date}`, `{time}`, `{datetime:FORMAT}`, `{clipboard}`, `{cursor}` (static/free) and `{field:Name}` / `{field:Name|default}` / `{choice:Name|a|b}` (prompted). |
| **Tag** | Free-form label across Boards; filterable. |
| **Pinned** | Item shown in the panel's favourites strip, above list order. |
| **Sensitive** | Item that is masked and excluded from history; its clipboard copy is cleared after N s (default 30, Settings > Privacy); encrypted at rest (all tiers). |
| **Recent inbox** | Clipboard history (opt-in: asked on first run; nothing is kept until Yes; Settings and the welcome tour change it). Last 200 entries or 7 days (fixed in v1.0). Promote → List. |
| **Panel** | Small dockable, always-on-top, non-activating window showing a Board (lists as tabs). |
| **Floating list** | A List detached from the Panel into its own mini-panel. |
| **Picker** | Hotkey-summoned, keyboard-first popup at the cursor: type to filter, Enter to paste. |
| **Main window** | Full manager: sidebar / item list / detail editor. |
| **Use** | Executing an Item: resolve template → write clipboard → paste or copy. |
| **Target** | The last focused foreign window + control the user was typing in. |

## 5. Features

### 5.1 Panel (always-available, Sticky-Notes-like)
- Dock to any screen edge/corner or float; always-on-top toggle; per-monitor position memory.
- **Edge dock with reveal (owner request 2026-10-18, modelled on Microsoft PC Manager's side panel):** docked to a screen edge the Panel collapses to a slim handle/grabber strip (as in PC Manager's "toolbar" mode, horizontal on top/bottom edges and vertical on left/right) and slides out on **hover and/or click** (setting: hover, click, or both; hover delay and leave delay configurable), then collapses again when the pointer leaves or after a paste. The handle stays on top, never takes focus, shows nothing sensitive, and is reachable by keyboard (the Panel hotkey toggles it). Multi-monitor: remembers the edge per monitor. Needs a Windows spike for the hover hit-test on a non-activating always-on-top strip (appbar vs plain topmost window) before building. **Rebuilt 2026-11-12 (`docs/reviews/dock-redesign.md`):** the Panel floats wherever it is dropped; only the cursor held at a work-area edge (4 px, 200 ms) offers a dock, with a live preview, and pulling back 12 px takes the offer away. The app moves the window itself from the cursor. Floating frame and docked size are remembered apart per monitor. A docked Panel tucks by default (trigger Both; one docked by an older version keeps Off until it is docked again), is always on top except under a full-screen app on its own monitor, does not hide on focus loss, reveals when the pointer is on its tab (or at the edge within 12 px past the tab's ends) and stays while hovered, focused or being dragged or resized. The top 24 px of the Panel is a full-width grab strip (its top 6 px resize, the 18 px below them drag).
- **Auto-hide to edge**: collapses to a thin tab; reveals on hover or hotkey.
- **Non-activating**: clicking the panel does not take focus from the Target (see ADR-0003).
- Header: board picker, search icon, `New` menu, keep-on-top, `More` menu (Undo, Redo, Paste stack, Dock to edge, Keyboard shortcuts, Settings), close; list tabs sit on the row below (setting: tabs or a menu). Layout details: `docs/reviews/panel-v2-spec.md`.
- Layouts: **List** (default), **Cards** and **Compact**, one global setting (Settings > Panel). It is not per list: a layout that changes when you switch tabs is a surprise in a 2-second window. The store keeps each list's `ListView` (persisted data) but the Panel neither reads nor writes it. Every row shows its type as an icon tile.
- **Favourites strip** of pinned items at top.
- **Click behaviour (smart)**: click → paste into Target if a text-input Target is known, else copy + toast.
  `Shift+click` → copy only. There is no Paste button: the row is the paste target and the footer says where it goes. Hover or selection reveals **Copy** / **Edit** / **More** on the preview line (Copy is hidden when there is no target).
- Keys when panel focused: `Alt+1`–`Alt+9` use Nth item (bare digits are search text), arrows navigate, `Enter` use, `F2` rename, `Del` delete.
- Add: **+ From clipboard**, `Ctrl+V` / drag-drop onto panel, **quick new note** (inline), global
  **save-selection** hotkey (copies selection under the hood, restores clipboard).
- Reorder by drag (also `Alt+↑/↓`); per-list sort: Manual (default) / Most used / Recently used.
- **Groups:** with the list on Manual sort, marked rows gather under a named, foldable header (`Ctrl+G`, item menu "Group…"). `Left`/`Right` fold and open a header, `F2` renames it, Ungroup keeps the items, Delete group sends group and items to the Trash as one undo step. Dragging a header moves the whole run. Search lists matches inside folded groups under their header. Groups survive export, import, backup and undo (schema v8, `item_groups` + `items.group_id`).
- **Search:** typing while the panel is focused filters across **every board** and shows where each match lives ("Board · List"); a miss on another board used to send people to the Main window. (Panel and picker match **title, tags and the first 64 characters of content** in memory with `nucleo`, multi-threaded: measured 4–8 ms for 10k items against the 16 ms budget; full-content matching was 22–57 ms and is not used there. The main window's search uses SQLite FTS5 for full content) (the search field expands over the tab row, with a visible Close button; `Esc` clears and collapses). `Ctrl+F` and the search icon also work; a setting can keep the field always shown.
- **Recent tab:** when history is on, a clock tab sits at the far left of the tab row. Promote an entry by dragging it to a tab or with "Save to…".
- **Symbols tab:** a smiley tab after the clock tab. One search covers emoji, kaomoji, icons and characters (punctuation, arrows, maths, currency, letters, boxes, shapes); kind tabs (Emoji, Kaomoji, Icons, Characters) and a focusable group strip for every kind narrow it, and `Ctrl+Left/Right` steps the kind. The catalogue is every default-tone emoji of `emoji-test.txt` (an emoji with no bundled art is drawn by the system font, marked "system style" in the data), 341 kaomoji, 1,512 Phosphor icons and about 2,700 characters. Search tolerates one typo in a word of five letters or more, folds accents and kana marks, finds by code point (`U+1F600`), shortcode, emoticon and CJK reading, and also matches the UI language (the folded names are built when the tab opens). All lists the most used kind first, with two rows each and a Show all link; a lone kind shows all its rows. Click pastes an emoji, kaomoji or character into the Target (copies when there is none), `Shift+click` copies only, and `Ctrl+S` adds the selection to the current list as a Text item. The grid is its own focus stop: `Tab` or `Down` from the field enters it, all arrows, Home, End and Page keys move in it, typing returns to the field, and `Up` on the first row returns too. Emoji art is 72 px, and a larger preview of the selection sits under the grid. The cell menu (right-click, `Shift+F10`) has Paste, Copy, Add to list; for an emoji with skin tones a row per tone (the tone chosen is remembered as the default and used by the next paste); for an icon Copy SVG (`currentColor` kept), Copy PNG, Save as SVG and Save as PNG, and two rows that step the PNG size (24, 32, 48, 64, 128, 256) and flip the ink between dark (the default, safe on a white document) and the theme colour; both are remembered in the settings. Recents: "Recently used" on top of the grid, most recent first, at most 24, saved in the settings. Details: `docs/reviews/symbols-spec.md`. Windows rendering of system style emoji and of the kaomoji fallback fonts is unverified.
- **Colours tab:** a palette tab after the Symbols tab (Settings > Panel "Show Colours tab"; palette commands "Colours" and "Pick a colour from the screen"). Open by default: a gradient square or hue wheel (Segmented switch), hue and alpha sliders, and typed per-channel values (HEX, RGB, HSL, HSB, CMYK) that update every other format live. Then Recently used, Saved, a row per format (click pastes into the Target, `Shift+click` copies; the default format is tagged), Contrast against white, black and any colour (WCAG AA/AAA marks, truncated ratios) and Schemes (rotations in OKLCH). Swatches make a colour current (`Shift+click` copies). A saved colour is a Text item in a list named "Colours" (or any list: Save to…); colour items are detected from the item preview, so no schema change. A colour item's menu has Open in Colours and a Paste as list of formats; a list holding colours has Copy as CSS variables. The screen eyedropper is Windows only (freeze-frame overlay, the Panel steps aside, the colour is copied when "Copy a colour picked from the screen" is on). Details: `docs/reviews/colour-spec.md`.
- **Target footer:** always shows where a click will go ("Pastes into Outlook · New message" / "Copies to clipboard").
- **Target detection (M0 S1 + a first manual run):** `GetGUIThreadInfo` finds the focused control in classic apps (Notepad verified: paste and clipboard restore worked, item absent from Win+V history) but reports nothing for OneNote and sometimes for Windows Terminal. v1 therefore adds a **UI Automation fallback** (focused element supports text input / value pattern) and a per-app **paste-key table** (Ctrl+V, Ctrl+Shift+V for terminals, Shift+Insert). When neither method finds a target the footer says "Copies to clipboard" and a click copies only. **(unverified for UIA)**
- **First-run position:** floating near the top left of the work area (48 px in), default width, 480 px tall, not pinned (only a docked Panel that tucks is always on top). Docked to an edge later it keeps that height (the full edge is a setting, off by default) and tucks into its tab (trigger Both).
- **Fullscreen apps:** while a fullscreen app covers the Panel's own monitor (in front or not; one on another monitor does not count) a docked Panel is not topmost (its tab is under the app) and the pointer at the screen edge does not reveal it; it returns afterwards. Hotkeys keep working: the Panel hotkey opens the Panel over the app, and it tucks again when the keyboard goes back. (A pinned floating Panel is not covered by this yet.)
- Inline quick-edit for short text and titles; everything else opens the Editor.
- Detach a list tab → **Floating list**.

### 5.1a Use semantics (paste & clipboard)
- **Paste** = write item to clipboard → paste key into Target → **restore the previous clipboard**. A fixed 300 ms wait is only the fallback: the primary signal is **delayed rendering** (the item's formats are rendered on demand and the previous clipboard is restored once the target has asked for them, with a 2 s cap), because slow targets such as Remote Desktop may otherwise receive the old content (M0 S1). **(unverified)**
- **Copy** (`Shift+click`, `Shift+Enter`, or no Target) = item stays on the clipboard.
- **Paste as plain text:** `Alt+click` / `Alt+Enter` (Panel items and Recent rows, Picker, Main window; also "Paste as plain text" in the Recent row menu); per-item "always paste as plain text" option.
- **Paste stack:** multi-select items → "Start stack". The panel shows the queue; the global **Paste next** hotkey (default `Ctrl+Shift+U`) pastes the head and advances. `Ctrl+V` is never hooked. `Esc` in the panel or finishing the queue ends the stack.
- Elevated targets (UIPI) and failures fall back to Copy with an explaining toast ("Couldn't paste into Task Manager (runs as admin). Copied instead.").

### 5.2 Picker
- Global hotkey (default `Ctrl+Shift+Space`), appears at text caret (fallback mouse) in <80 ms.
- **Placement:** the Picker setting chooses the side of the caret: Below (default), Above, Right or Left. Below and Above leave a one-line gap, so the caret's line and the neighbouring line stay visible; Right and Left sit beside the caret, top-aligned, and may cover the rest of that line. A side without room flips to the opposite one, and the window is clamped to the monitor's work area. A mouse anchor behaves like Below.
- Fuzzy search across all boards (title, content, tags); `Tab` scopes to board/list; `Alt+1`–`Alt+9` quick use (digits without `Alt` are search text).
- `Enter` paste, `Shift+Enter` copy, `Ctrl+E` edit, `Esc` dismiss and restore focus.

### 5.3 Main window (3-pane)
- **Sidebar**: Search, Recent inbox, Pinned, Boards ▸ Lists, Tags, Trash (30-day).
- **Item list**: list/grid, multi-select, bulk move/tag/delete, drag between lists.
- **Detail**: preview + editor + metadata (formats present, uses, created/updated, source app).
- Settings, onboarding, import/export and the optional Support Snipdeck link live here.

### 5.4 Editors
- **Size limits (M0 S3, measured on a Linux VM with the software renderer):** Slint's `TextEdit` re-lays-out all text on every keystroke (about 1 ms per KB). Inline editing is therefore capped at **16 KB**. From 16 KB to **5 MB** an item opens in a **read-only virtualised viewer** (rope-based; find, copy, whole-text transforms; measured ≤ 12 ms per interaction). Text over 5 MB is rejected on capture with an explanatory toast (limit to be revisited after Windows measurements).
- **Text**: plain editor (wrap, monospace toggle, line numbers for long text), find/replace,
  transforms (UPPER/lower/Title, trim, strip formatting, dedupe lines, sort lines),
  **template field inserter** with live preview. Rich-text items: formatting preserved on paste;
  edited as text (rich-text editing is v1.x).
- **Editor footer:** Trash, Cancel and Save share one row; when the row does not fit, Trash moves above Cancel/Save instead of clipping. The template placeholder help lists its rows with one shared chip column so descriptions line up.
- **Image**: crop, rotate/flip, resize/scale, **redact** (blur / solid box). Non-destructive until save.

### 5.5 Capture & OCR
- Hotkey (off until the user sets a chord; no default) → Snipdeck's own region-select overlay (multi-monitor, DPI-aware). **Not built yet:** v1.0's capture hotkey saves the text on the clipboard as a new item in the list the Panel shows (off until a chord is set), and Settings says so.
- After selection an **action bar** appears at the selection: **Copy text** (one-off OCR, nothing stored) · **Save text** · **Save image** · **Save both**, plus a list picker (default: active list).
- Text actions open a small **review popover** with the OCR result, editable before copying/saving.
- OCR engine: **Windows.Media.Ocr** by default (zero bundle cost); optional **RapidOCR pack**
  (ONNX, downloaded on demand) for messy layouts / unsupported languages (ADR-0004).

### 5.6 Recent inbox (asked on first run, nothing kept until Yes)
- **Opt-in.** The first time the Panel opens (after the welcome tour, if it runs) a Yes/No question asks "Keep a Recent list of what you copy?"; Esc or a click outside answers No. Nothing is recorded before Yes. A settings file without `recent_history_asked` (any older build) is asked too, because it cannot tell a choice from the old default. The welcome tour (step 4) can answer early, and Settings has the switch. Default chords for a new install: Show/hide Panel `Ctrl+Shift+Y`, Picker `Ctrl+Shift+Space`, Paste next `Ctrl+Shift+U`, none use Ctrl+Alt (AltGr on many layouts; `docs/reviews/default-chords.md`); an existing file keeps its chords.
- Listens to clipboard changes; dedupes; skips sources that set
  `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0` (password managers),
  excluded apps, and Sensitive items.
- Retention: 200 entries / 7 days (fixed in v1.0, not a setting); image entries size-capped.
- Encrypted at rest with the Sensitive-item key (OS-wrapped, optional master password); locked = hidden and paused. Search filters after decrypt, in memory.
- One click **promote** to a List.

### 5.7 Sensitive items
- Free: masked `••••` (click-and-hold to reveal), excluded from history. Clipboard auto-clear: a **copy** of a sensitive item (Shift+click, no Target) is cleared from the clipboard N s later (Settings > Privacy, default 30, 0 = off) unless something else has been copied since; an auto-paste restores the previous clipboard, so it leaves nothing to clear.
- Everyone: encrypted at rest (XChaCha20-Poly1305); key wrapped by OS (DPAPI). An optional
  **master password** (Argon2id) → unlock prompt per session / idle timeout.
  - Settings > Privacy sets, changes or removes it (typed twice, at least 8 characters, a strength hint in words). Setting, changing, removing and unlocking are always allowed.
  - With a password set the library starts locked and says nothing. The unlock window opens the first time a locked sensitive item is needed (use, reveal, edit), and again after 15 minutes without use (`IDLE_LOCK`). A wrong password changes nothing and allows another try; Esc leaves it locked.
  - Locking (idle or otherwise) also closes an open editor of a sensitive item without saving it, so no plain text stays on screen; typing in such an editor counts as use. A backup, export or restore never runs on a locked library (it would leave out or lose the sensitive items): it asks for the password instead, and the scheduled backup waits.
  - Win+L and sleep lock through the same path, so an unsaved draft in an open sensitive editor is lost then too (no draft is kept, to keep no plain text anywhere); a known limit, not changed. Windows behaviour (session lock and sleep events) is unverified until run on Windows.
  - Forgetting it loses the sensitive items for good: while a password is set the data key exists only sealed under it inside the OS-wrapped record, there is no recovery key, and the form says so before the password is set. Other items are not affected.

### 5.7a Deletion & recovery
- Items delete instantly with a 5 s **Undo** toast; lists and boards ask for confirmation (showing the item count).
- **Trash** keeps everything 30 days; restore from the main window.
- **Daily local auto-backup** (zipped `.snipdeck` in the data folder, keep 7); restore from Settings.
- **Crash log** stored locally; "Report a problem" lets the user review the log, then opens a prefilled GitHub issue or email. Nothing is sent automatically.

### 5.7b Onboarding (≤ 60 s, skippable)
1. Welcome. 2. Hotkeys (with conflict detection). 3. Panel position. 4. Start with Windows + history opt-in.
Seeds a **Getting started** list whose cards teach by doing ("Click me to paste", "Shift+click to copy", "Try a template").

### 5.8 System integration
- Tray icon (ADR-0008): a left click toggles the Panel; the menu is Open Panel, Open Snipdeck, Pause hotkeys (check mark), Quit. The Picker and quick capture are hotkeys, not menu entries. "Pause history" joins the menu when clipboard history exists. Closing the Panel hides it to the tray while an icon is in the notification area (`close_to_tray`), otherwise it quits.
- Start with Windows (opt-in during onboarding). Single instance.
- Per-app exclusions: no history capture and/or no auto-paste in listed apps.
- All hotkeys rebindable; conflicts detected and reported.

### 5.9 Data portability
- Export/import `.snipdeck` (zip: `manifest.json`, `items.json`, `assets/`), schema documented in `docs/format.md`.
- Importers: Ditto, CopyQ, folder of `.txt`/`.md`. The Windows clipboard history importer is **v1.x** (not implemented).
- Importer notes are typed (`Note` in `snipdeck-portable`) and worded in the UI through `@tr`, so every note, including its "entry N" / "Ditto clip N" / "file, match N" label, appears in the user's language with its values intact.
- Relocatable data folder (e.g. into OneDrive/Dropbox for DIY sync) with a concurrent-use warning is **v1.x** (not implemented): in v1.0 the data folder is always `%LOCALAPPDATA%\Snipdeck`; the DB is single-writer.

## 6. Free, with an optional Buy Me a Coffee link

Owner decision 2026-11-15: everything is free on every build. There is no paid tier, no Supporter, no licence key, no
entitlement check and no limit (boards, prompted placeholders, master password, paste stack, Paste next and
floating lists are always on). The code is GPL-3.0.

**Support Snipdeck** is an optional link, in Help ▸ About, Settings ▸ Support and the command palette. It opens
`https://buymeacoffee.com/braydenh563` in the default browser through the same open-URL path as Help's other links,
and its text says it is optional, keeps the app free and unlocks nothing. The app handles no payment data, sends
nothing, and never nags (no banners, countdowns or prompts). Settings and libraries written by builds that had a
licence key still load: unknown fields are ignored and a stale `licence.key` file is never read.

## 7. Internationalisation & accessibility
- Launch locales: **en, es, de, fr, pt-BR, ja, zh-Hans**. All strings via Slint `@tr()` / gettext `.po`.
- CJK font fallback, IME-correct text input, locale-aware dates in `{date}`.
- WCAG 2.2 AA contrast in both themes, full keyboard operation, AccessKit/Narrator labels,
  visible focus, respects Windows reduced-motion and text-scale settings.
- Tab order (audit 1b): focus moves in reading order through the Panel, editors and scrolling forms; a control that scrolls into view takes focus with it, and the Tips cards are reached with `Tab` card to card.

## 8. Appearance
- **Own minimal brand** (Linear/Raycast-class restraint): neutral surfaces, one accent, sharp type, dense but calm.
- Light / Dark / System; user-selectable accent from a curated set; optional per-List colour accent.
- Full design system defined in the design round (`docs/design/`).

## 9. Performance budget (acceptance criteria)

| Metric | Budget |
|---|---|
| Idle RAM (panel visible) | < 40 MB |
| Idle CPU | ~0 % (event-driven, no polling) |
| Cold start → panel visible | < 400 ms |
| Panel show (from hidden) | < 100 ms |
| Hotkey → picker interactive | < 80 ms |
| Search over 10k items (title + 64 chars, 4 threads) | < 16 ms (measured 4–8 ms on a Linux VM) |
| Installer (excl. OCR pack) | < 15 MB |

## 10. Architecture (summary — see ADRs)

Cargo workspace:

| Crate | Responsibility |
|---|---|
| `snipdeck-core` | Domain types, template engine, fuzzy search (nucleo), use-pipeline logic. Pure, OS-free. |
| `snipdeck-store` | SQLite (rusqlite, WAL, FTS5), migrations, content-addressed blob store, crypto. |
| `snipdeck-platform` | Traits + Windows impl: clipboard (multi-format r/w + listener), global hotkeys, focus/Target tracking, paste injection, non-activating windows, screen capture, OS OCR, autostart. |
| `snipdeck-ocr-rapid` | Optional RapidOCR backend (ort), on-demand model pack. |
| `snipdeck-ui` | Slint design tokens, component library, windows (panel, picker, main, editors, capture overlay, onboarding, settings). |
| `snipdeck-app` | Binary: wiring, tray, single instance, updater, licensing, i18n, settings. |

Dependency rule: `app → ui, platform, store, core`; `store → core`; `platform → core`; `core` depends on nothing internal.

## 11. Distribution
- **Microsoft Store** (free; Individual Partner Center account) and **GitHub Releases + winget** (free, signed via SignPath Foundation).
- No network update check: **Check for updates** opens the GitHub releases page, or the Store's updates page in the Store build; the Store handles its own updates.

## 12. Quality gates (CI, Windows runner)
`cargo fmt --check`, `cargo clippy -D warnings`, unit + integration tests (store, templates,
clipboard format round-trips), Slint UI tests (testing backend), screenshot regression,
`cargo deny` (licences/advisories), perf smoke benchmarks against §9.

## 13. De-risking spikes (M0 outcomes in [`docs/spikes/`](spikes/README.md))
1. **Non-activating Slint window** (WS_EX_NOACTIVATE via raw HWND) + Target tracking + `SendInput` paste;
   handle panel text input (search/inline edit) by temporarily activating, then restoring Target.
2. **IME / CJK input** in Slint `TextInput`.
3. **Long-text editor performance** in Slint (multi-MB text).

### 13a. Engineering constraints from M0
- **Slint is pinned exactly** (`=1.18.x`); all winit/Win32 calls stay inside `snipdeck-platform` because `unstable-winit-030` and `unstable-fontique-011` can change in a minor release.
- **Renderer: software renderer by default** on Windows. Skia renders transparent areas black (Slint #4120, #13572) and FemtoVG/OpenGL reportedly does on NVIDIA. Re-evaluate only with a Windows test.
- **Lists use Slint `ListView`** (virtualised); a plain scroll area with every row built costs 69 ms per step at 10k items.
- **CJK fonts:** bundle IBM Plex Sans, and set the Han/Kana/Hangul fallback order per UI locale (Yu Gothic UI / Microsoft YaHei UI / Malgun Gothic). Composition is drawn as a selection highlight (no underline); force-commit the composition before the panel deactivates (slint#10861); act on key-press only. **(unverified)**
- **Windows "Text size"** is read explicitly; the tab bar scrolls instead of overflowing at 175%+.
- Report upstream: software renderer panics selecting text on a no-wrap line wider than 32,767 px.

## 14. Delivery process
1. **M0 spikes** (Opus): non-activating window + paste, IME, long-text editor, Mica in Slint. Each ends in a short findings note in `docs/spikes/`.
2. **Implementation plan** written from SPEC + DESIGN + spike findings.
3. **Milestones**, one PR each, built by parallel Sonnet implementers (one per crate/area), Haiku for i18n strings, test scaffolding and docs, and an Opus reviewer per milestone PR.
4. CI builds Windows artifacts per PR. The owner smoke-tests focus, paste and OCR on real Windows, since cloud CI can't verify these.

## 15. Out of scope for v1
Cloud sync service, rich-text editing, image annotation (arrows/text), scripting/macros,
Linux/macOS builds, mobile.

Post-1.0, decided:
- Global eyedropper hotkey chord: needs Windows-only verification, not possible before release.
- Per-swatch Ctrl+S: per-swatch save is reachable from the swatch menu.
- OLE `IDropTarget` drops from other apps: a v1.1 deferral (`docs/reviews/v101-spec.md:68`).
- Symbols, emoji art for the ~70 system-style emoji (mostly ZWJ families and couples, and emoji newer than the Fluent set): no free art covers them; the system emoji font draws them until a licensed set exists.
- Symbols, a skin tone drawn in the grid as art: tones paste and are remembered, but the grid shows the default (yellow) art, because tone art would cost about 5x the people art (~1.2 MB).
- Symbols, Add to list for an icon: an icon is not text; saving it as an Image item needs the item editor's PNG path and is not planned before 1.1.
- Symbols, the Windows verification of system-style emoji and kaomoji fonts: needs the owner on Windows (`tools/windows-test/`).
- Colours and Symbols as standalone or pinnable windows (TOOLS-58): the Panel is one window with tabs and hides on focus loss by design; a second always-open window needs its own focus, dock and z-order rules and a Windows pass that cannot be done before release. The eyedropper already hides and restores the main window around a pick.
- Colours, nearest Tailwind colour: needs a vendored palette and a licence check; the nearest CSS name already covers naming.
- Colours, a custom copy-format template editor and a hex-case setting UI: `custom_format` exists in the colour crate and is tested, but a template editor needs Settings layout, validation wording and six translations; hex stays lower-case.
