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
- Its own hidden window on its own thread (`windows/hidden_window.rs`; the hotkeys keep their window-less thread queue);
  `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4`, and re-adding the icon on `TaskbarCreated`.
- **The window is a hidden top-level `WS_POPUP` tool window, not a message-only one (amended 2026-10-18).** Message-only
  windows are not top-level windows and never receive broadcasts, and `TaskbarCreated` and `WM_SETTINGCHANGE` are
  broadcasts. The same window also hears the Windows colour-mode change (`system_look`).
- Left click (`NIN_SELECT` / `NIN_KEYSELECT`) toggles the Panel; the menu (`TrackPopupMenu`) holds Open Panel, Open
  Snipdeck, Pause hotkeys (checkable), a separator and Quit. Settings joins it when the Settings window exists.
  Closing the Panel hides it to the tray when `close_to_tray` is on and a tray icon exists, otherwise it quits. The
  planned first-time toast is not built: the Panel is hidden when the user closes it, so a toast inside it would never
  be seen (a balloon notification, `NIF_INFO`, is the way, later).
- The decisions (what a click means, which entry is which event, when to re-add) are the portable `TrayModel`; the
  Windows file only talks to the shell. `testing::FakeTray` runs the same model.
- Reason: Slint's tray support is new, its event-loop coupling is untested for a never-focused panel, and we
  already own the message window.

## Dark mode and high contrast (2026-10-18)
- `SystemLook { dark, high_contrast }` comes from `HKCU\...\Themes\Personalize\AppsUseLightTheme` and
  `SystemParametersInfo(SPI_GETHIGHCONTRAST)` (colours from `GetSysColor`); no WinRT `UISettings`. A watcher on the same
  kind of hidden window re-reads on `WM_SETTINGCHANGE` (`ImmersiveColorSet`, `SPI_SETHIGHCONTRAST`), `WM_THEMECHANGED`
  and `WM_SYSCOLORCHANGE`.
- `settings.theme` (`system | light | dark`) decides dark; a high-contrast theme overrides it. The controller pushes the
  result to the Panel (`dark` property and the `SystemLook` global's `high-contrast` / `hc-*` colours).

## Consequences
- `SystemLook` gains `caption-inset`; the main window's title bar component must not draw a close button.
- A wizard step checks: one set of caption buttons, snap layout flyout on the maximise button, the title area is
  draggable, the inset matches `DWMWA_CAPTION_BUTTON_BOUNDS` at 100%, 150% and 200% DPI.
- Windows 10: no Mica; the same layout.
- The tray's popup menu is a classic Win32 menu: it does not follow dark mode (the only dark-menu route is undocumented
  uxtheme ordinals; not used).
- All Win32 behaviour here is unverified until the owner runs `tools/windows-test/tests/m3-app.json`.
