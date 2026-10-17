# ADR-0003: Paste via non-activating panel

- Status: Accepted with changes; spike S1 compiled, first manual run partly positive, full test script still to run (2026-10-17)

## Context
"Click an item → it appears where I was typing" requires the Target app to keep keyboard focus.

## Decision
Panel/floating windows use `WS_EX_NOACTIVATE` (+ `WS_EX_TOOLWINDOW`). A foreground hook (`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`) tracks the Target; `GetGUIThreadInfo` detects a caret. Use = write clipboard (with `ExcludeClipboardContentFromMonitorProcessing` marker) → `SendInput` Ctrl+V → restore previous clipboard after a short delay. When the panel needs text input (search, inline edit) it activates temporarily and re-focuses the Target afterwards. Fallback if no caret: copy + toast.

### Amendments from spike S1
- Create the window with `with_active(false)`; re-assert the panel style in a `WM_STYLECHANGING` subclass because winit rewrites `GWL_EXSTYLE`; read Shift with `GetAsyncKeyState` (Slint's modifier state is stale in a never-focused window).
- `GetGUIThreadInfo` is not enough: OneNote reported "nothing focused" and Windows Terminal sometimes did too. Add a **UI Automation fallback** and a per-app paste-key table.
- Restore the previous clipboard using **delayed rendering** rather than a fixed 300 ms delay.
- First manual evidence (Windows 11, Notepad): clicking an item inserted it at the caret, Ctrl+V afterwards pasted the previous clipboard, and the item was absent from Win+V history. The panel has no minimise button by design.
- If the full test script fails: custom `slint::platform::Platform`/`WindowAdapter` on our own HWND with the software renderer (see `docs/spikes/S1-noactivate.md`).

## Consequences
- Elevated (admin) Target apps can't receive input from a non-elevated Snipdeck (UIPI) → detect and fall back to copy with explanation.
- macOS v1.1 uses `NSPanel` non-activating + CGEvent (Accessibility permission).
