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
5. **Library first, history second.** Curated items are the product; clipboard history is an opt-in inbox.

## 3. Platforms

| Release | Platform | Notes |
|---|---|---|
| v1.0 | Windows 10 (1903+) / 11 | Primary target |
| v1.1 | macOS 13+ | Needs Accessibility permission for paste-into-field |
| later | Linux (X11 full, Wayland copy-only) | Wayland blocks key injection |

## 4. Domain model (glossary)

| Term | Meaning |
|---|---|
| **Board** | Top-level workspace (e.g. "Work", "Personal"). Free tier: 5 boards. |
| **List** | Ordered collection of Items inside a Board (e.g. "Email replies"). Has optional colour accent. |
| **Item** | One saved thing the user can paste. Has a title, one or more Formats, tags, flags. |
| **Format** | One clipboard representation of an Item: `text`, `html`, `rtf`, `image/png`, `files`. An Item may hold several (e.g. html + text fallback). |
| **Template** | An Item whose text contains placeholders, resolved at use time. |
| **Placeholder** | `{date}`, `{time}`, `{datetime:FORMAT}`, `{clipboard}`, `{cursor}` (static/free) and `{field:Name}` / `{field:Name|default}` / `{choice:Name|a|b}` (prompted/Supporter). |
| **Tag** | Free-form label across Boards; filterable. |
| **Pinned** | Item shown in the panel's favourites strip, above list order. |
| **Sensitive** | Item that is masked and excluded from history; its clipboard copy is cleared after N s (default 30, Settings > Privacy); encrypted at rest (all tiers). |
| **Recent inbox** | Opt-in clipboard history. Last 200 entries or 7 days (configurable). Promote → List. |
| **Panel** | Small dockable, always-on-top, non-activating window showing a Board (lists as tabs). |
| **Floating list** | A List detached from the Panel into its own mini-panel (Supporter). |
| **Picker** | Hotkey-summoned, keyboard-first popup at the cursor: type to filter, Enter to paste. |
| **Main window** | Full manager: sidebar / item list / detail editor. |
| **Use** | Executing an Item: resolve template → write clipboard → paste or copy. |
| **Target** | The last focused foreign window + control the user was typing in. |

## 5. Features

### 5.1 Panel (always-available, Sticky-Notes-like)
- Dock to any screen edge/corner or float; always-on-top toggle; per-monitor position memory.
- **Edge dock with reveal (owner request 2026-10-18, modelled on Microsoft PC Manager's side panel):** docked to a screen edge the Panel collapses to a slim handle/grabber strip (as in PC Manager's "toolbar" mode, horizontal on top/bottom edges and vertical on left/right) and slides out on **hover and/or click** (setting: hover, click, or both; hover delay and leave delay configurable), then collapses again when the pointer leaves or after a paste. The handle stays on top, never takes focus, shows nothing sensitive, and is reachable by keyboard (the Panel hotkey toggles it). Multi-monitor: remembers the edge per monitor. Needs a Windows spike for the hover hit-test on a non-activating always-on-top strip (appbar vs plain topmost window) before building.
- **Auto-hide to edge**: collapses to a thin tab; reveals on hover or hotkey.
- **Non-activating**: clicking the panel does not take focus from the Target (see ADR-0003).
- Header: board switcher, list tabs, search, `+` menu, collapse.
- Views: **Compact** (1-line rows) and **Comfortable** (2–3 line preview, image thumbs). Per-list choice.
- **Favourites strip** of pinned items at top.
- **Click behaviour (smart)**: click → paste into Target if a text-input Target is known, else copy + toast.
  `Shift+click` → copy only. Hover reveals explicit **Copy** / **Paste** / **⋯** buttons.
- Keys when panel focused: `1`–`9` use Nth item, arrows navigate, `Enter` use, `F2` rename, `Del` delete.
- Add: **+ From clipboard**, `Ctrl+V` / drag-drop onto panel, **quick new note** (inline), global
  **save-selection** hotkey (copies selection under the hood, restores clipboard).
- Reorder by drag (also `Alt+↑/↓`); per-list sort: Manual (default) / Most used / Recently used.
- **Search:** typing while the panel is focused filters across the current board (panel and picker match **title, tags and the first 64 characters of content** in memory with `nucleo`, multi-threaded: measured 4–8 ms for 10k items against the 16 ms budget; full-content matching was 22–57 ms and is not used there. The main window's search uses SQLite FTS5 for full content) (the tab row becomes the search field; `Esc` restores). `Ctrl+F` and the search icon also work.
- **Recent tab:** when history is on, a clock tab sits at the far left of the tab row. Promote an entry by dragging it to a tab or with "Save to…".
- **Target footer:** always shows where a click will go ("Pastes into Outlook · New message" / "Copies to clipboard").
- **Target detection (M0 S1 + a first manual run):** `GetGUIThreadInfo` finds the focused control in classic apps (Notepad verified: paste and clipboard restore worked, item absent from Win+V history) but reports nothing for OneNote and sometimes for Windows Terminal. v1 therefore adds a **UI Automation fallback** (focused element supports text input / value pattern) and a per-app **paste-key table** (Ctrl+V, Ctrl+Shift+V for terminals, Shift+Insert). When neither method finds a target the footer says "Copies to clipboard" and a click copies only. **(unverified for UIA)**
- **First-run position:** docked to the right edge, full height minus taskbar, always-on-top, auto-hide off (changeable in onboarding).
- **Fullscreen apps:** the panel auto-hides while a fullscreen app is in front and returns afterwards; hotkeys keep working.
- Inline quick-edit for short text and titles; everything else opens the Editor.
- Detach a list tab → **Floating list** (Supporter).

### 5.1a Use semantics (paste & clipboard)
- **Paste** = write item to clipboard → paste key into Target → **restore the previous clipboard**. A fixed 300 ms wait is only the fallback: the primary signal is **delayed rendering** (the item's formats are rendered on demand and the previous clipboard is restored once the target has asked for them, with a 2 s cap), because slow targets such as Remote Desktop may otherwise receive the old content (M0 S1). **(unverified)**
- **Copy** (`Shift+click`, `Shift+Enter`, or no Target) = item stays on the clipboard.
- **Paste as plain text:** `Alt+click` / `Alt+Enter`; per-item "always paste as plain text" option.
- **Paste stack (Supporter):** multi-select items → "Start stack". The panel shows the queue; the global **Paste next** hotkey (default `Ctrl+Alt+V`) pastes the head and advances. `Ctrl+V` is never hooked. `Esc` in the panel or finishing the queue ends the stack.
- Elevated targets (UIPI) and failures fall back to Copy with an explaining toast ("Couldn't paste into Task Manager (runs as admin). Copied instead.").

### 5.2 Picker
- Global hotkey (default `Ctrl+Shift+Space`), appears at text caret (fallback mouse) in <80 ms.
- Fuzzy search across all boards (title, content, tags); `Tab` scopes to board/list; `1`–`9` quick use.
- `Enter` paste, `Shift+Enter` copy, `Ctrl+E` edit, `Esc` dismiss and restore focus.

### 5.3 Main window (3-pane)
- **Sidebar**: Search, Recent inbox, Pinned, Boards ▸ Lists, Tags, Trash (30-day).
- **Item list**: list/grid, multi-select, bulk move/tag/delete, drag between lists.
- **Detail**: preview + editor + metadata (formats present, uses, created/updated, source app).
- Settings, onboarding, import/export, licence entry live here.

### 5.4 Editors
- **Size limits (M0 S3, measured on a Linux VM with the software renderer):** Slint's `TextEdit` re-lays-out all text on every keystroke (about 1 ms per KB). Inline editing is therefore capped at **16 KB**. From 16 KB to **5 MB** an item opens in a **read-only virtualised viewer** (rope-based; find, copy, whole-text transforms; measured ≤ 12 ms per interaction). Text over 5 MB is rejected on capture with an explanatory toast (limit to be revisited after Windows measurements).
- **Text**: plain editor (wrap, monospace toggle, line numbers for long text), find/replace,
  transforms (UPPER/lower/Title, trim, strip formatting, dedupe lines, sort lines),
  **template field inserter** with live preview. Rich-text items: formatting preserved on paste;
  edited as text (rich-text editing is v1.x).
- **Image**: crop, rotate/flip, resize/scale, **redact** (blur / solid box). Non-destructive until save.

### 5.5 Capture & OCR
- Hotkey (default `Ctrl+Shift+2`) → Snipdeck's own region-select overlay (multi-monitor, DPI-aware). **Not built yet:** v1.0's capture hotkey saves the text on the clipboard as a new item in the list the Panel shows (off until a chord is set), and Settings says so.
- After selection an **action bar** appears at the selection: **Copy text** (one-off OCR, nothing stored) · **Save text** · **Save image** · **Save both**, plus a list picker (default: active list).
- Text actions open a small **review popover** with the OCR result, editable before copying/saving.
- OCR engine: **Windows.Media.Ocr** by default (zero bundle cost); optional **RapidOCR pack**
  (ONNX, downloaded on demand) for messy layouts / unsupported languages (ADR-0004).

### 5.6 Recent inbox (opt-in, off by default)
- Listens to clipboard changes; dedupes; skips sources that set
  `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0` (password managers),
  excluded apps, and Sensitive items.
- Retention: 200 entries / 7 days (configurable); image entries size-capped.
- One click **promote** to a List.

### 5.7 Sensitive items
- Free: masked `••••` (click-and-hold to reveal), excluded from history. Clipboard auto-clear: a **copy** of a sensitive item (Shift+click, no Target) is cleared from the clipboard N s later (Settings > Privacy, default 30, 0 = off) unless something else has been copied since; an auto-paste restores the previous clipboard, so it leaves nothing to clear.
- Free and Supporter: encrypted at rest (XChaCha20-Poly1305); key wrapped by OS (DPAPI). Supporter adds an optional
  **master password** (Argon2id) → unlock prompt per session / idle timeout.

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
- Importers: Ditto, CopyQ, Windows clipboard history (current session), folder of `.txt`/`.md`.
- Data folder relocatable (e.g. into OneDrive/Dropbox for DIY sync); DB is single-writer — warn on concurrent use.

## 6. Free vs Supporter

The paid tier is called Supporter (it was Pro; internal identifiers such as `Tier::Pro` keep the old name). Pay what you want from A$5, suggested A$8, one-time, every 1.x update included; a key is free on request ("Can't afford it? Email me"). Free stays fully usable and the code is GPL-3.0: Supporter pays for convenience and supports the app.

| Feature | Free | Supporter |
|---|---|---|
| Boards | 5 | Unlimited |
| Lists / items | Unlimited | Unlimited |
| Panel, picker, capture, OCR, editors, history | ✓ | ✓ |
| Static placeholders | ✓ | ✓ |
| Prompted template fields (`{field}`, `{choice}`) | — | ✓ |
| Sensitive masking + auto-clear | ✓ | ✓ |
| Sensitive encryption at rest | ✓ | ✓ |
| Master password | — | ✓ |
| Floating (detached) lists | — | ✓ |
| Paste stack | — | ✓ |

Licence key: offline Ed25519-signed token, no phone-home (ADR-0005).

**Upsell:** shown only when the user tries a Supporter action (a calm sheet: what it does, price, "Enter key" / "Get Supporter"; Get Supporter opens an email to the owner), plus a Free vs Supporter table in Settings > Licence (feature, one-line description, tick or cross per tier; a tick or cross is never colour alone). No nags, banners or countdowns.

## 7. Internationalisation & accessibility
- Launch locales: **en, es, de, fr, pt-BR, ja, zh-Hans**. All strings via Slint `@tr()` / gettext `.po`.
- CJK font fallback, IME-correct text input, locale-aware dates in `{date}`.
- WCAG 2.2 AA contrast in both themes, full keyboard operation, AccessKit/Narrator labels,
  visible focus, respects Windows reduced-motion and text-scale settings.

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
- **Microsoft Store** (paid listing) and **GitHub Releases + winget** (free, signed via SignPath Foundation).
- In-app update check against GitHub Releases (user-confirmed); Store handles its own updates.

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
