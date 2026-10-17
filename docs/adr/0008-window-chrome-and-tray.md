# ADR-0008: Window chrome and tray

- Status: Accepted (2026-10-17). Windows behaviour unverified until the owner runs the wizard.

## Context
S4 on real Windows 11 showed the OS caption buttons (minimise, maximise, close) drawn over the app's own small close
button: two close buttons in one corner (`docs/spikes/windows-results-20261017.md`, W2).

## Decision
**Main window (activated normally): keep the OS caption buttons; the app draws none.**
- Keeps snap layouts, correct hit-testing, high-contrast styling, Narrator names and the user's accent for free, and
  avoids owning WM_NCHITTEST emulation.
- The window keeps `WS_CAPTION | WS_SIZEBOX`; the non-client area is removed in `WM_NCCALCSIZE`; the client area
  reserves the caption-button strip at the top right. The platform layer reads its bounds
  (`DWMWA_CAPTION_BUTTON_BOUNDS`) and sets `SystemLook.caption-inset` (logical px) on start-up, on DPI and on theme
  change; the title bar lays out around it. Until measured, the default is 138 px (3 x 46).
- Mica sits behind the title bar and sidebar only (DESIGN.md §5).

**Panel, Picker, floating lists, Snip overlay (never activated): no OS caption.**
- `WS_POPUP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`; the panel header has its own close (hide) button and a drag
  region. No minimise or maximise, by design (ADR-0003).

**Tray: a Win32 notification icon in `snipdeck-platform`, not Slint's `system-tray` feature.**
- Same hidden message-only window and thread as the hotkeys; `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4`, and
  re-adding the icon on `TaskbarCreated`.
- Left click toggles the Panel; the menu holds Open Snipdeck, Pause hotkeys, Settings and Quit. Closing the main window
  hides it to the tray (first time: a toast says so).
- Reason: Slint's tray support is new, its event-loop coupling is untested for a never-focused panel, and we
  already own the message window.

## Consequences
- `SystemLook` gains `caption-inset`; the main window's title bar component must not draw a close button.
- A wizard step checks: one set of caption buttons, snap layout flyout on the maximise button, the title area is
  draggable, the inset matches `DWMWA_CAPTION_BUTTON_BOUNDS` at 100%, 150% and 200% DPI.
- Windows 10: no Mica; the same layout.
