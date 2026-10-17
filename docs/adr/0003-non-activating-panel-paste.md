# ADR-0003: Paste via non-activating panel

- Status: Accepted, pending spike (2026-10-17)

## Context
"Click an item → it appears where I was typing" requires the Target app to keep keyboard focus.

## Decision
Panel/floating windows use `WS_EX_NOACTIVATE` (+ `WS_EX_TOOLWINDOW`). A foreground hook (`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`) tracks the Target; `GetGUIThreadInfo` detects a caret. Use = write clipboard (with `ExcludeClipboardContentFromMonitorProcessing` marker) → `SendInput` Ctrl+V → restore previous clipboard after a short delay. When the panel needs text input (search, inline edit) it activates temporarily and re-focuses the Target afterwards. Fallback if no caret: copy + toast.

## Consequences
- Elevated (admin) Target apps can't receive input from a non-elevated Snipdeck (UIPI) → detect and fall back to copy with explanation.
- macOS v1.1 uses `NSPanel` non-activating + CGEvent (Accessibility permission).
