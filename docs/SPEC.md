# Snipdeck — Product Specification (v1)

> Status: **Agreed** (grill round 1, 2026-10-17). Design-system spec follows in `docs/design/`.
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
| **Board** | Top-level workspace (e.g. "Work", "Personal"). Free tier: 3 boards. |
| **List** | Ordered collection of Items inside a Board (e.g. "Email replies"). Has optional colour accent. |
| **Item** | One saved thing the user can paste. Has a title, one or more Formats, tags, flags. |
| **Format** | One clipboard representation of an Item: `text`, `html`, `rtf`, `image/png`, `files`. An Item may hold several (e.g. html + text fallback). |
| **Template** | An Item whose text contains placeholders, resolved at use time. |
| **Placeholder** | `{date}`, `{time}`, `{datetime:FORMAT}`, `{clipboard}`, `{cursor}` (static/free) and `{field:Name}` / `{field:Name|default}` / `{choice:Name|a|b}` (prompted/Pro). |
| **Tag** | Free-form label across Boards; filterable. |
| **Pinned** | Item shown in the panel's favourites strip, above list order. |
| **Sensitive** | Item that is masked, excluded from history, auto-cleared from clipboard after use; encrypted at rest (Pro). |
| **Recent inbox** | Opt-in clipboard history. Last 200 entries or 7 days (configurable). Promote → List. |
| **Panel** | Small dockable, always-on-top, non-activating window showing a Board (lists as tabs). |
| **Floating list** | A List detached from the Panel into its own mini-panel (Pro). |
| **Picker** | Hotkey-summoned, keyboard-first popup at the cursor: type to filter, Enter to paste. |
| **Main window** | Full manager: sidebar / item list / detail editor. |
| **Use** | Executing an Item: resolve template → write clipboard → paste or copy. |
| **Target** | The last focused foreign window + control the user was typing in. |

## 5. Features

### 5.1 Panel (always-available, Sticky-Notes-like)
- Dock to any screen edge/corner or float; always-on-top toggle; per-monitor position memory.
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
- Inline quick-edit for short text and titles; everything else opens the Editor.
- Detach a list tab → **Floating list** (Pro).

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
- **Text**: plain editor (wrap, monospace toggle, line numbers for long text), find/replace,
  transforms (UPPER/lower/Title, trim, strip formatting, dedupe lines, sort lines),
  **template field inserter** with live preview. Rich-text items: formatting preserved on paste;
  edited as text (rich-text editing is v1.x).
- **Image**: crop, rotate/flip, resize/scale, **redact** (blur / solid box). Non-destructive until save.

### 5.5 Capture & OCR
- Hotkey (default `Ctrl+Shift+2`) → Snipdeck's own region-select overlay (multi-monitor, DPI-aware).
- After selection: **Extract text** / **Save image** / **Both** → into chosen list (default: active list).
- OCR engine: **Windows.Media.Ocr** by default (zero bundle cost); optional **RapidOCR pack**
  (ONNX, downloaded on demand) for messy layouts / unsupported languages (ADR-0004).

### 5.6 Recent inbox (opt-in, off by default)
- Listens to clipboard changes; dedupes; skips sources that set
  `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0` (password managers),
  excluded apps, and Sensitive items.
- Retention: 200 entries / 7 days (configurable); image entries size-capped.
- One click **promote** to a List.

### 5.7 Sensitive items
- Free: masked `••••` (click-and-hold to reveal), excluded from history, clipboard auto-clear after N s (default 30).
- Pro: encrypted at rest (XChaCha20-Poly1305); key wrapped by OS (DPAPI) and optionally by a
  **master password** (Argon2id) → unlock prompt per session / idle timeout.

### 5.8 System integration
- Tray icon: show panel, picker, capture, open main, pause history, quit.
- Start with Windows (opt-in during onboarding). Single instance.
- Per-app exclusions: no history capture and/or no auto-paste in listed apps.
- All hotkeys rebindable; conflicts detected and reported.

### 5.9 Data portability
- Export/import `.snipdeck` (zip: `manifest.json`, `items.json`, `assets/`), schema documented in `docs/format.md`.
- Importers: Ditto, CopyQ, Windows clipboard history (current session), folder of `.txt`/`.md`.
- Data folder relocatable (e.g. into OneDrive/Dropbox for DIY sync); DB is single-writer — warn on concurrent use.

## 6. Free vs Pro

| Feature | Free | Pro |
|---|---|---|
| Boards | 3 | Unlimited |
| Lists / items | Unlimited | Unlimited |
| Panel, picker, capture, OCR, editors, history | ✓ | ✓ |
| Static placeholders | ✓ | ✓ |
| Prompted template fields (`{field}`, `{choice}`) | — | ✓ |
| Sensitive masking + auto-clear | ✓ | ✓ |
| Sensitive encryption + master password | — | ✓ |
| Floating (detached) lists | — | ✓ |

Licence key: offline Ed25519-signed token, no phone-home (ADR-0005).

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
| Search over 10k items | < 16 ms |
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

## 13. De-risking spikes (before feature work)
1. **Non-activating Slint window** (WS_EX_NOACTIVATE via raw HWND) + Target tracking + `SendInput` paste;
   handle panel text input (search/inline edit) by temporarily activating, then restoring Target.
2. **IME / CJK input** in Slint `TextInput`.
3. **Long-text editor performance** in Slint (multi-MB text).

## 14. Out of scope for v1
Cloud sync service, rich-text editing, image annotation (arrows/text), scripting/macros,
Linux/macOS builds, mobile.
