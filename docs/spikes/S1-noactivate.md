# Spike S1: non-activating Slint panel + paste into the Target

- Date: 2026-10-17
- Code: [`spikes/s1-noactivate/`](../../spikes/s1-noactivate/) (standalone, throwaway crate)
- Relates to: SPEC §5.1, §5.1a, §13.1 · [ADR-0003](../adr/0003-non-activating-panel-paste.md) · DESIGN §9 rule 2
- Versions: Slint 1.18.1 (winit backend, `unstable-winit-030`), winit 0.30.13, `windows` 0.62.2, rustc 1.97

## Status

The code compiles, lints clean and links for Windows. **Nothing has been run on Windows.** The spike was
built on Linux with no Windows machine available, so every runtime claim below is either sourced from
reading the Slint/winit code or marked **UNVERIFIED**. The owner must run the [manual test script](#manual-test-script-real-windows)
before this note can confirm anything.

## Recommendation: GO WITH CHANGES (provisional until the test script passes)

Slint exposes everything this feature needs, but only through its `unstable-winit-030` feature: a hook on
the window attributes, async access to the winit window (and so the HWND), and the HWND is
owned by the UI thread so we can subclass it. Nothing found in the source rules the approach out. The
required changes:

1. **Guard the extended style.** winit overwrites `GWL_EXSTYLE` whenever Slint changes a window property, so the
   panel has to re-assert `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` in `WM_STYLECHANGING` (implemented).
2. **Read modifiers from the OS, not Slint.** Shift+click has to use `GetAsyncKeyState`, because Slint's modifier
   state is stale in a window that never has keyboard focus (implemented).
3. **Pin Slint exactly** and keep every winit/Win32 call inside `snipdeck-platform`, because `unstable-winit-030` can change
   in a Slint minor release.
4. **Caret detection needs a UI Automation fallback.** `GetGUIThreadInfo` alone will miss many apps (see Risks).
5. **Handle text entry with temporary activation.** It works in principle, but it has the pitfalls listed below
   and needs a "did activation succeed?" check.

**This becomes NO-GO** if test steps 1–2 show that clicking a Slint button (a) activates the panel or takes
the caret from Notepad despite the style and subclass, or (b) is ignored or needs a second click because the
window is inactive, and neither can be fixed within about a day.

**Fallback if NO-GO:** keep Slint and drop winit for the Panel and Picker. Implement a small custom
`slint::platform::Platform` / `WindowAdapter` that owns a Win32 HWND we create ourselves
(`CreateWindowExW` with `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, our own WndProc) and renders with
Slint's `SoftwareRenderer` into a DIB section (`UpdateLayeredWindow` / `BitBlt`), forwarding mouse messages
as `WindowEvent`s. This keeps the components and design tokens and gives full control over activation.
The main window and editors stay on winit. If that is also too costly, the last resort is a native Direct2D
panel, which means duplicating the design system and is not recommended.

## What was built

One small window with four sample items, a search field, a status line and a **Target footer**.

| Requirement | Implementation (`src/platform/windows.rs` unless noted) |
|---|---|
| 1. Non-activating window | `window_attributes()` → `with_active(false)` + `with_skip_taskbar(true)` via `BackendSelector::with_winit_window_attributes_hook` (`main.rs`). `install()` awaits `WinitWindowAccessor::winit_window()`, takes the HWND from `raw_window_handle`, `SetWindowSubclass`es it, then sets `WS_EX_NOACTIVATE \| WS_EX_TOOLWINDOW` (and strips `WS_EX_APPWINDOW`). The subclass returns `MA_NOACTIVATE` for `WM_MOUSEACTIVATE` and re-applies the style in `WM_STYLECHANGING`. Topmost via Slint `always-on-top: true` (winit `WindowLevel::AlwaysOnTop` → `HWND_TOPMOST`). |
| 2. Target tracking | `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, …, WINEVENT_OUTOFCONTEXT \| WINEVENT_SKIPOWNPROCESS)` stores the last foreign foreground HWND. `route()` resolves the process name (`QueryFullProcessImageNameW`), window title, and `GetGUIThreadInfo` (`hwndFocus` / `hwndCaret`) at display time and again at click time. The footer is refreshed on foreground change and when the pointer enters the panel (no polling). |
| 3. Use pipeline | `use_text()`: snapshot every HGLOBAL clipboard format → write `CF_UNICODETEXT` + `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`, `CanUploadToCloudClipboard=0` → `SendInput` Ctrl+V (modifier-safe, see below) → after 300 ms (`slint::Timer`, no thread) restore the snapshot only if `GetClipboardSequenceNumber` is unchanged. Shift+click = plain copy, no markers, no restore. |
| 4. UIPI | `can_send_input_to()` compares the target's token integrity level with ours (`OpenProcessToken` + `TokenIntegrityLevel`). A higher level, or an unreadable token, routes to Copy with a reason: "Copies to clipboard (X.exe runs as admin; Snipdeck does not)". |
| 5. Text entry | When the search `LineEdit` gains focus, `begin_text_entry()` snapshots the route and calls `SetForegroundWindow(panel)`. Esc (FocusScope `capture-key-pressed`), Enter, an item click, or focus loss calls `end_text_entry()`, which runs `SetForegroundWindow(target)` if the panel is still foreground. A paste issued while the panel is foreground re-activates the target first and waits 50 ms. Any foreign foreground change ends text entry. |
| Fullscreen (SPEC 5.1) | `SHQueryUserNotificationState` ∈ {BUSY, D3D_FULL_SCREEN, PRESENTATION_MODE} → `hide()`, else `show()`. Uses `run_event_loop_until_quit` so hiding doesn't exit. |
| Linux | `src/platform/stub.rs`: same API, copy-only stub. |

The code is about 770 lines. The UI is unstyled (it ignores DESIGN tokens on purpose because it is a spike).

## Verification actually performed (Linux host)

| Command | Result |
|---|---|
| `cargo check` (Linux, stub) | ✅ |
| `cargo check --target x86_64-pc-windows-gnu` | ✅ |
| `cargo clippy -- -D warnings` (Linux) | ✅ |
| `cargo clippy --target x86_64-pc-windows-gnu -- -D warnings` | ✅ |
| `cargo fmt --check` | ✅ |
| `cargo build --target x86_64-pc-windows-gnu` (mingw-w64 linker) | ✅ links. The PE import table contains `comctl32!SetWindowSubclass`, `user32!SetWinEventHook/SendInput/GetGUIThreadInfo`, `shell32!SHQueryUserNotificationState` |

The `.slint` markup compiled, so these Slint 1.18 features exist: `changed has-focus =>`, `capture-key-pressed`,
`clear-focus()`, and public functions.

## Findings: what Slint/winit actually allow (from reading the source)

- **No extended-style API.** Neither Slint nor winit 0.30's `WindowAttributesExtWindows` can set `WS_EX_NOACTIVATE`, so we use raw `SetWindowLongPtrW`.
- **The HWND only exists after the event loop starts.** With winit 0.30, Slint creates the window lazily on
  `show()` once the loop runs, so `with_winit_window` returns `None` before that. Use
  `slint::spawn_local(async { window.winit_window().await })`.
- **`with_active(false)` is essential.** winit's `WindowFlags::apply_diff` calls `ShowWindow(SW_SHOW)` (which activates)
  on *every* flag change while the window is visible, unless the window was created with `active = false`, in which
  case it always uses `SW_SHOWNOACTIVATE`.
- **winit clobbers `GWL_EXSTYLE`.** `apply_diff` computes the styles from its own flags and calls `SetWindowLongW`.
  Slint triggers this from `update_window_properties` (title, decorations, window level, visibility), and winit
  also always adds `WS_EX_APPWINDOW` to top-level windows, which defeats `WS_EX_TOOLWINDOW`. The fix is the
  `WM_STYLECHANGING` guard.
- **Window-proc hook choice.** winit's only hook, `EventLoopBuilderExtWindows::with_msg_hook`, sees posted messages
  from `GetMessage`. `WM_MOUSEACTIVATE` and `WM_STYLECHANGING` are *sent*, so it cannot see them. The spike uses
  `SetWindowSubclass` on the winit HWND (same thread), which runs before winit's WndProc.
- **Slint's modifier state is stale in an inactive window.** On mouse-down, winit refreshes modifiers with `GetKeyState`, which
  only updates for a thread that reads keyboard messages. Our window never does, so `PointerEvent.modifiers.shift`
  is expected to be wrong (**UNVERIFIED**, test step 2c). Use `GetAsyncKeyState`.
- **Never call winit `focus_window()` / Slint `bring_to_front()` on the panel.** On Windows they use winit's
  `force_window_active`, which injects a synthetic **Alt tap**. That would land in the Target and can activate
  its menu bar. Slint 1.18 does not call it internally.
- **The HWND survives hide/show** on Windows (Slint only destroys on Wayland or with `SLINT_DESTROY_WINDOW_ON_HIDE`), so the
  subclass persists. If the window is ever re-created, `install` must run again.

## API choices and why

- **`SetWinEventHook` out-of-context + `WINEVENT_SKIPOWNPROCESS`:** event-driven (satisfies the ~0 % idle CPU budget), needs
  no DLL injection, and activating our own panel never replaces the Target. The callback can run inside
  nested modal loops (e.g. a window drag), so UI updates are deferred with `Weak::upgrade_in_event_loop`.
- **Integrity level instead of `TokenElevation`:** UIPI is defined by integrity levels, not by the elevation flag. This
  also handles a Snipdeck that is itself elevated, and uiAccess or low-IL targets. From a medium-IL process,
  `OpenProcessToken` on an elevated target is expected to fail with access denied, so an unreadable token
  counts as "blocked" (safe failure: copy). `SendInput` does **not** report UIPI drops, so this up-front check is the only signal.
- **Raw Win32 clipboard instead of `arboard`:** we need custom formats, an owner HWND (`OpenClipboard(NULL)` + `EmptyClipboard`
  makes `SetClipboardData` fail), sequence numbers, and an all-formats snapshot.
- **Modifier-safe Ctrl+V:** press Ctrl first (unless the user is already holding it), then send key-ups for any physically held
  Shift/Alt/Win, then V down/up, then Ctrl up. Because Ctrl is down before Alt/Win go up, a lone Alt-up can't
  open the menu bar and a lone Win-up can't open Start. We don't re-press released keys. The user's later
  physical key-up is harmless.
- **`GetClipboardSequenceNumber` guard:** if the user (or another app) copied something within the 300 ms window, we don't
  overwrite it with the old snapshot.
- **`slint::Timer` for delays:** keeps everything on the UI thread with no sleeps (except the ≤50 ms `OpenClipboard` retry).

### Clipboard formats: what the snapshot preserves

- **Saved:** every HGLOBAL-backed format. That includes `CF_UNICODETEXT`/`CF_TEXT`/`CF_OEMTEXT`/`CF_LOCALE`, `CF_DIB`/`CF_DIBV5`,
  `CF_HDROP` (files), and registered formats such as `HTML Format`, `Rich Text Format`, `PNG`, `Shell IDList Array`,
  `FileGroupDescriptorW`, and app-private formats (Office, Chromium web custom data, …).
- **Not saved:** GDI-handle formats (`CF_BITMAP` is re-synthesised from `CF_DIB`; **`CF_ENHMETAFILE`/`CF_METAFILEPICT` are lost**,
  which affects Office vector pictures), `CF_PALETTE`, owner-display, the private range `0x200–0x2FF`, and OLE-only data
  (`FileContents` streams, e.g. Outlook attachments).
- **For the product:** add EMF via `CopyEnhMetaFile`, cap the snapshot size (e.g. 32 MB), and accept that apps relying on
  *owning* the clipboard (Excel's marching-ants copy) degrade after a restore.

## Text entry (search / inline edit): approach and pitfalls

Implemented approach: activate the panel only while a text field has Slint focus, then hand the foreground
back. `SetForegroundWindow(panel)` is permitted because our process received the last input event (the click).
`SetForegroundWindow(target)` is permitted because we are foreground at that point. Pitfalls:

1. **Route snapshot required.** Once the Target is deactivated, most apps destroy their caret, so
   `GetGUIThreadInfo` can no longer tell whether a text field is focused. The route is captured *before* activating.
2. **Visible focus loss in the Target.** Its title bar goes inactive, autocomplete dropdowns, context menus and IntelliSense popups close, and
   "on blur" logic runs (validation, autosave, chat presence).
3. **Re-activation may select-all.** Windows restores the Target's focused control, but some controls select their whole
   content on focus (browser address bars, some dialog edits). The paste then *replaces* the text (test 4b).
4. **Foreground lock can refuse.** If focus arrives by keyboard rather than a click, or another app took the foreground in between,
   `SetForegroundWindow(panel)` fails silently or flashes the taskbar, and the user's typing goes into the Target. The
   product should verify `GetForegroundWindow() == panel` and show a hint if it isn't.
5. **The 50 ms refocus delay is a guess.** The Target must process `WM_ACTIVATE`/`WM_SETFOCUS` before Ctrl+V arrives. Tune it from testing.
6. **IME context switches** to the panel while searching (covered by spike S2).
7. **Alternatives rejected:** a low-level keyboard hook to "type" into a non-active panel (breaks IME and
   accessibility, and is fragile). The Picker (SPEC §5.2) is keyboard-first anyway, so it should be a normal activating window
   that restores the Target on dismiss, using the same `end_text_entry` logic.

## Risks found

1. **Caret detection coverage.** `hwndCaret` only exists for apps that use the Win32 system caret. Chromium/Electron, WinUI/UWP,
   Windows Terminal, Firefox, Qt/Java apps and games may report focus but no caret. The spike pastes anyway when
   there is a focus HWND and labels the footer "(no caret seen)". As a result, clicking the desktop probably reports
   `explorer.exe` as a paste target (test 8). The product needs a UIA fallback (`IUIAutomation::GetFocusedElement`
   → Edit/Document control type, `ValuePattern.IsReadOnly`, `IsPassword`) on a worker thread, triggered by
   `EVENT_OBJECT_FOCUS` and cached, because cross-process UIA can be slow or hang.
2. **Restore timing.** 300 ms is a guess. Slow targets (RDP/Citrix clipboard redirection, VMs, a busy Electron app) may read
   *after* the restore and paste the **old** clipboard. A better design to test next: offer the item with
   **delayed rendering** (`SetClipboardData(fmt, NULL)`) and restore shortly after the `WM_RENDERFORMAT` caused by the Target's read.
   That gives a deterministic "target has read it" signal.
3. **UI-thread blocking.** `GetClipboardData` on delay-rendered formats sends `WM_RENDERFORMAT` synchronously to the
   clipboard owner. A hung owner freezes the panel, and big Excel ranges render many formats. The product should run
   clipboard work on a worker thread with its own message-only owner window.
4. **Startup race.** The ex-style is applied one event-loop turn after the window first shows. It shows *without*
   activation, but may briefly have a taskbar button or `WS_EX_APPWINDOW`. Mitigation: show it off-screen or transparent until
   `install` completes.
5. **Wrong app name for UWP apps.** The foreground window is `ApplicationFrameHost.exe` and the real app lives in a child
   `CoreWindow` from another process, so the footer shows the wrong name and the caret probe may miss. The product needs to resolve the child window.
6. **Paste semantics differ by app.** In terminals, Ctrl+V can be literal (vim treats it as visual-block), and some games and anti-cheat
   software ignore injected input. Needs per-app paste-key overrides (Shift+Insert) alongside SPEC §5.8 exclusions.
   `SendInput` also fails on the secure desktop (UAC, lock screen).
7. **Fullscreen detection is coarse.** `SHQueryUserNotificationState` has no change event, so the spike re-checks only on foreground
   change. F11 in an already-foreground window isn't noticed. The product should register an appbar and use
   `ABN_FULLSCREENAPP`.
8. **API stability.** `unstable-winit-030` may change in minor Slint releases, so pin `slint = "=x.y.z"`.
9. **Accessibility.** A never-activated window can't be reached with Tab or Alt+Tab, so keyboard and Narrator users need a hotkey that
   *activates* the panel (the "keys when panel focused" mode in SPEC §5.1).
10. **Elevated targets via uiAccess.** Pasting into elevated apps would require Snipdeck to be uiAccess-signed and installed in
    Program Files. That is out of scope for v1, so the copy fallback stays.

## Manual test script (real Windows)

**Setup:** Windows 10 22H2 or 11, MSVC Rust toolchain. Turn on clipboard history (Win+V → Turn on).
`cd spikes\s1-noactivate` then `cargo run --release` from a normal (non-admin) terminal.
For every step, record: footer text before the click, status text after, whether the Target kept focus (title bar
active, caret blinking), and anything unexpected.

| # | Steps | Expected |
|---|---|---|
| 1 | Launch. Click the terminal, then click an empty area of the panel. | Panel appears **without** taking focus from the terminal. No "entry point not found" error. No taskbar button (note any brief flash at start-up). Not listed in Alt+Tab. Stays above other windows. Clicking the panel leaves the terminal's title bar active. |
| 2 | **Notepad:** copy the word `BEFORE` to the clipboard, type `abc` in Notepad, click **Hello from Snipdeck**. | Footer before the click: `Pastes into Notepad.exe · … - Notepad` with **no** "(no caret seen)". The text is inserted at the caret, Notepad stays active with the caret blinking, and the status shows `Pasted into …`. Then press Ctrl+V in Notepad: it pastes `BEFORE` (restore worked). Win+V history does **not** contain the item. |
| 2a | Click the `ünïcødé ✓ 日本語 🙂` item and the two-line item. | Exact text, with line breaks intact. |
| 2b | Hold Shift and click an item. | Nothing inserted. Status `Copied (Shift+click)`. Ctrl+V pastes the item. (Validates `GetAsyncKeyState`.) |
| 2c | Hold Alt and click an item, then hold the Win key and click an item. | Pasted. Notepad's menu bar is **not** activated (no underlined access keys) and the Start menu does **not** open. After releasing, typing works normally (no stuck modifiers). |
| 2d | Hold Ctrl and click an item. | Pasted once. Ctrl not stuck afterwards. |
| 2e | Restore of non-text data: copy an image in Paint, paste an item into Notepad, then Ctrl+V in Paint. Repeat with a file copied in Explorer, pasting back into a folder. | The image or file is back on the clipboard and pastes correctly. |
| 2f | Drag the panel by its title bar while Notepad is active. | Record whether Notepad loses activation (product will be frameless; informational). |
| 3 | **Word:** click into a document and run steps 2–2b. Then copy formatted text (bold plus a picture) in Word, paste an item into Notepad, and Ctrl+V back in Word. | The footer reports a caret and the paste works. Formatting comes back after the restore. Record whether the picture is missing (EMF isn't preserved). Optional: try the same with an **Excel** cell range and record the degradation. |
| 4 | **Chrome text field:** click into a `<textarea>` (e.g. a GitHub comment box), then click an item. Repeat in the address bar. | Record the footer (caret seen or "(no caret seen)"). The item is pasted at the caret. |
| 4a | Text entry: put the caret in the middle of the Chrome textarea text, click the panel's **Search** field, type `kind`, press Enter. | Typing goes to the panel (Chrome's title bar is inactive while you type). The list filters to "Kind regards". Enter re-activates Chrome and inserts the text **at the old caret position without replacing the text**. Record any select-all. Repeat starting from the address bar and record whether its contents get replaced. |
| 4b | Search, then press Esc. Search, then click an item. Search, then click into Notepad. | Esc: Chrome returns to the front with the caret where it was. Item click: pasted into Chrome. Notepad: Notepad stays in front (no bounce back to Chrome). |
| 5 | **Elevated Notepad:** start Notepad with "Run as administrator", click into it, then click an item. | Footer `Copies to clipboard (Notepad.exe runs as admin; Snipdeck does not)`. Status `Copied: …`. Nothing injected. Ctrl+V pastes the item manually. Then rerun the spike from an **admin** terminal: the footer becomes `Pastes into …` and pasting works. |
| 6 | **Password field:** a web login page password input in Chrome, and (if available) a Win32 password box such as the KeePass master-password dialog. | The paste works and shows as masked characters. The item is **not** in Win+V history. After ~0.5 s, Ctrl+V elsewhere gives the previous clipboard. Record the footer caret state. |
| 7 | **Fullscreen app:** with the panel visible, open a fullscreen video (YouTube with **f**), a PowerPoint slideshow, or a fullscreen game, switching to it with Alt+Tab or a click. Then Alt+Tab away. | The panel hides while the fullscreen app is in front and reappears afterwards **without** taking focus. Known gap: F11 inside an already-focused window doesn't hide the panel until the next foreground change. Record it. |
| 8 | **No text target:** click the desktop, then an item. Also try Calculator. | Record the footer and what happens. This is data for the caret/UIA policy. Ideally nothing bad happens. |
| 9 | **Terminal:** Windows Terminal (PowerShell), then vim inside it. | Record whether the paste works. Vim is expected to treat Ctrl+V as visual-block, which confirms the need for per-app paste keys. |
| 10 | **Slow target (optional):** paste into a Remote Desktop session. | Record whether the remote side receives the item or the *old* clipboard (restore-timing risk). |

**GO** if 1, 2, 2b–2d, 4, 4a (textarea), 5 and 6 behave as expected. Failures in 3, 4a (address bar), 7–10 are
"with changes" items. **NO-GO** criteria are listed under [Recommendation](#recommendation-go-with-changes-provisional-until-the-test-script-passes).
