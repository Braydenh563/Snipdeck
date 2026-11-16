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
| **Placeholder** | `{date}`, `{time}`, `{datetime:FORMAT}`, `{clipboard}`, `{selection}`, `{cursor}`, `{key:tab}`, `{snippet:Title}` (static) and `{field:Name}` / `{choice:Name|a|*b}` / `{number:Name}` / `{pickdate:Name}` (prompted); see 5.4a. |
| **Tag** | Free-form label across Boards; filterable (several at once, matching any or all). Has an optional colour (one of the 8 item colours), shown as a dot beside it. |
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
- Reorder by drag (also `Alt+↑/↓`); one sort for every section (Manual, Name, Created, Modified, Most used, Recently used, each ascending or descending; never-used items come last), kept per section: a list's in the library, the system views' and tags' in the settings. Groups keep their headers under any sort, sorting inside each group. The Main window filters by several tags (any or all), untagged, kind, has trigger, sensitive and colour label, as chips in its toolbar; the Picker's scopes use the same filter.
- **Groups:** when the list is on Manual sort, marked rows gather under a named, foldable header (`Ctrl+G`, item menu "Group…"). `Left`/`Right` fold and open a header, `F2` renames it, Ungroup keeps the items, Delete group sends group and items to the Trash as one undo step. Dragging a header moves the whole run. Search lists matches inside folded groups under their header. Under any other sort the headers stay and each group's items sort inside it (reordering waits for Manual). Deleting a folded group asks first, with the item count; "Move group to list…" takes the header and its items to another list. Groups survive export, import, backup and undo (schema v8, `item_groups` + `items.group_id`).
- **Search:** typing while the panel is focused filters across **every board** and shows where each match lives ("Board · List"); a miss on another board used to send people to the Main window. (Panel, Picker and Main share one pipeline, see §5.2: an in-memory index of title, tags, list name, trigger and the first 64 characters of content, multi-threaded from 2,000 items, plus the store's trigram FTS5 index for the rest of the content; at 10k items a query is held to the 16 ms budget in release by `search_10k_items_stays_within_budget`) (the search field expands over the tab row, with a visible Close button; `Esc` clears and collapses). `Ctrl+F` and the search icon also work; a setting can keep the field always shown.
- **Recent tab:** when history is on, a clock tab sits at the far left of the tab row. Promote an entry by dragging it to a tab or with "Save to…".
- **Symbols tab:** a smiley tab after the clock tab. One search covers emoji, kaomoji and icons; kind tabs (Emoji, Kaomoji, Icons) and a group strip narrow it, and `Ctrl+Left/Right` steps the kind. Click pastes an emoji or kaomoji into the Target (copies when there is none), `Shift+click` copies only, and `Ctrl+S` adds the selected emoji or kaomoji to the current list as a Text item. An icon always copies, as a PNG (64px, in the theme's text colour); the SVG copy (`currentColor` kept) and Save as (SVG or PNG) are in the controller, with their menu planned (slice 6). Recents: "Recently used" on top of the grid, most recent first, at most 24, saved in the settings. Details: `docs/reviews/symbols-spec.md`.
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
- Search across all boards (title, tags, list name, trigger, full content); `Tab` scopes to board/list; `Alt+1`–`Alt+9` quick use (digits without `Alt` are search text).
- **One query language on every surface** (Panel, Picker, Main; `core/src/search.rs`): words separated by spaces, each of which must match in *any* field; `"quoted phrase"` matches the words in that order; `-word` (or `-"phrase"`, `-tag:x`) excludes; `tag:name`, `list:name` (prefix of the name, quotes for spaces), `kind:text|template|code|link|image|file`, `is:pinned` filter. Matching ignores case, accents and width (NFKC fold), so `RESUME`, `résumé` and `ｒｅｓｕｍｅ` are one word. A word matches exactly, as a prefix, at a word start, anywhere inside, or (title and tags, 2+ characters) as a subsequence; any run of 3+ characters, or 1+ CJK characters, is found in the full content. Ranking: match quality, then the field (trigger, title, tag, list, content), then use count and recency. A query is cut to 32 words and 256 characters. Marks show where the matcher matched (the longest run in the title, the first matching preview line, else the tag).
- `Enter` paste, `Shift+Enter` copy, `Ctrl+E` edit, `Esc` dismiss and restore focus.

### 5.3 Main window (3-pane)
- **Sidebar**: Search, Recent inbox, Pinned, Boards ▸ Lists, Tags, Trash (30-day).
- **Item list**: list/grid, multi-select, bulk move/tag/delete, drag between lists.
- **Detail**: preview + editor + metadata (formats present, uses, created/updated, source app).
- Settings, onboarding, import/export and the optional Support Snipdeck link live here.

### 5.4 Editors
- **Size limits (M0 S3, measured on a Linux VM with the software renderer):** Slint's `TextEdit` re-lays-out all text on every keystroke (about 1 ms per KB). Inline editing is therefore capped at **16 KB**. From 16 KB to **5 MB** an item opens in a **read-only virtualised viewer** (rope-based; find, copy, whole-text transforms; measured ≤ 12 ms per interaction). Text over 5 MB is rejected on capture with an explanatory toast (limit to be revisited after Windows measurements). **Not built yet:** the rope viewer, and with it find and the whole-text transforms for text over 16 KB. Text over 16 KB opens as a prefix of the first 16 KB; the rest of the text is kept as it was.
- **Text**: plain editor (wrap, monospace toggle, line numbers for long text), find/replace,
  transforms (UPPER/lower/Title, trim, strip formatting, dedupe lines, sort lines),
  **template field inserter** with live preview. Rich-text items: formatting preserved on paste;
  edited as text (rich-text editing is v1.x).
- **Editor footer:** Trash, Cancel and Save share one row; when the row does not fit, Trash moves above Cancel/Save instead of clipping. The template placeholder help lists its rows with one shared chip column so descriptions line up.
- **Image**: crop, rotate/flip, resize/scale, **redact** (blur / solid box). Non-destructive until save. **Not built yet:** the whole image editor, decoding JPEG, WebP, GIF and BMP to the stored PNG (an Image item takes a PNG only; the `image` crate is only a transitive dependency with PNG and JPEG, so the other formats need new crates and a `deny.toml` check), and Replace on an existing Image or File item (delete it and add a new one). A File item takes one file.
- **Text tools (built):** the editor's *Text tools* row opens find and replace (match count, *Match case*, *Replace all*), the whole-text changes UPPER CASE, lower case, Title Case, trim (spaces at line ends and blank lines at both ends), remove repeated lines (blank lines stay) and sort lines A to Z, a *Monospace font* switch (Code items start on) and *Line numbers* (lines stop wrapping and a long line scrolls sideways with the caret). A change is one step: *Undo the change* brings the text back until the next keystroke. The font and line-number choices last for the open editor only. They apply to text of 16 KB or less; a larger text is read-only, as above. **Not built yet:** these tools inside the read-only viewer for large text (the rope viewer is not built), and a replace that picks matches one by one.

### 5.4a Templates
A template is an Item whose text has placeholders, resolved at use time by the pure engine in `snipdeck-core` (`template.rs`). It reads no clock, clipboard, selection, locale or library itself: the caller passes them in.
- **Static:** `{date}`, `{time}`, `{datetime}`, each with an optional format (`{date:%d %B %Y}`) and offsets (`{date+3d}`, `{time-90min}`; at most 16 per placeholder); `{uuid}`; `{clipboard}` and `{clipboard:N}` (N = 2 to 25: the copy N-1 steps before the current one, from the clipboard history; empty where history is off); `{selection}` (text selected in the target app); `{cursor}`; `{key:tab}` and `{key:enter}`; `{snippet:Title}`.
- **Prompted (the fill form):** `{field:Name}` and `{field:Name|default}` (`{field:Name|}` is optional), `{multiline:Name|default}`, `{number:Name|default}`, `{choice:Name|a|*b|c}` (`*` marks the option the form starts on; one empty option is the "none" choice), `{pickdate:Name|FORMAT}`, `{optional:Name}...{/optional}` (a tick box; a required field inside an unticked section is disabled and never blocks Insert).
- **Derived:** `{value:Name|transform}` (an answer again, shaped), `{calc:Qty*Price|2}` (`+ - * /`, parentheses, number prompts by name; a division by zero shows `#DIV/0!`), `{clipboard|upper}` and `{selection|title}`. Transforms: plain, upper, lower, title, trim, slug, sentence, camel, snake, urlencode, firstword, lastword (the same list is the "Paste as..." menu and the editor help, generated from one enum).
- **Names** are compared trimmed and case-folded. At most **64** different prompts per template; the editor states the limits under the placeholder chips and refuses a longer template when it is saved, with the reason.
- **Escapes:** `{{` and `}}` are literal braces in the text. Inside a placeholder a backslash writes `\|`, `\{`, `\}`, `\\` (and `\*` for an option that starts with `*`). A parse error names the fix.
- **Dates:** `{date}`, `{time}` and `{datetime}` use one rule: the locale's short pattern (the Windows regional format where it can be read, else the UI language's table). Format flags `%-d` (no padding) and `%_d` (space padding); specifiers `%Y %y %G %m %B %b %h %d %e %j %A %a %u %V %q %H %I %M %S %p %z %Z %F %T %R %%`. Month and weekday names exist for all seven launch locales. `{pickdate}` is typed in the locale's numeric order (placeholder shows it, ISO always accepted) or picked in a month grid under the box (arrows, PageUp/PageDown, Home/End, Enter; a button after the box opens it, and Esc closes the grid before the next Esc cancels the form).
- **Nested snippets:** `{snippet:Title}` is replaced, when the template is read, by the text of the one item with that title (or trigger). A plain-text item is inserted literally; a template item brings its own placeholders (a prompt both use is one prompt). Depth 4, 32 uses, cycles and Sensitive items are refused with a message. Nothing substituted is ever parsed again.
- **Keys:** a template with `{key:tab}` or `{key:enter}` is pasted in pieces with the key between them; a copy, or a route that can only copy, keeps the text whole and presses nothing. A transform drops the keys.
- **Selection:** `{selection}` is read before the fill form opens: Snipdeck copies with Ctrl+C into the clipboard, reads the result and puts the previous clipboard back. It does this only where Ctrl+C is safe: never in a console or terminal (where Ctrl+C interrupts), and only for the Ctrl+V route. In those apps `{selection}` is empty. *Windows behaviour (the copy, the restore, the key presses and the clipboard history) is unverified until the owner runs it on Windows.*
- **Languages:** month and weekday names exist for the seven launch locales; any other language tag uses English names.

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
- **Trash** keeps everything 30 days; restore from the main window. The search box also searches the Trash (title and preview, in memory, with the same marks; a sensitive item has no preview to search).
- **Undo and Redo** (Ctrl+Z, Ctrl+Y) reach the last 100 steps made since Snipdeck started. The history lives in memory only: it is forgotten when the app closes and when the library is erased, and kept across a lock. It is not saved between runs (post-1.0, with its reasons): a saved step would have to hold the old text of every sensitive item, and a Trash purge or a restored backup could leave steps that point at things that no longer exist. Every item, list, board, tag, group and order change is a step, as is a restore from the Trash. An edit of a sensitive item's text is a marker only (its text is not kept, so Undo stops there with a message, and steps older than it stay in the list). Deleting for good (one item, Empty Trash, or the 30-day purge) removes only the steps that need what was deleted. A toast's Undo button is withdrawn when a later step touched the same item. Undo of an Add offers "Delete forever" instead of an Undo-the-Undo. Help > Shortcuts says the same in one note.
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
| `snipdeck-core` | Domain types, template engine, search (query grammar, folding, ranking), use-pipeline logic. Pure, OS-free. |
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
- Group colour and nested groups: a group is a flat, named run of items. A colour would compete with the item colour label on the same rows, and a second level would turn the one-header-per-run drag, fold and keyboard model into a tree for no use a list or a tag does not already serve.
- Nested tags: a tag is a flat label, filterable by several at once (any or all). A hierarchy would need parent rules for rename, merge, delete, export and the search grammar (`tag:a/b`), and the multi-tag filter covers "work and invoices" without it.
- OLE `IDropTarget` drops from other apps: a v1.1 deferral (`docs/reviews/v101-spec.md:68`).
