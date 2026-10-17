# Windows run, 2026-10-17 (quick mode, commit 85711bb)

Source: owner's `spikes-results/current/state.json` and two screenshots. Windows 11 Home build 26300, Intel Iris Xe,
1 monitor, 200% DPI (192), dark theme, rustc 1.99.1. Real run, but **Windows 11 only**: no Windows 10 data.

## S1: focus-safe paste

| Step | Result | Note |
|---|---|---|
| 1 Panel opens without stealing focus | pass | |
| 2 Notepad: click item pastes | pass | |
| 2b Shift+click copies only | pass | |
| 2c Alt / Win held while clicking | **fail (finding)** | Alt+click showed the access-key hints in Notepad (menu bar activated). Win+click did nothing, and releasing Win opened the Start menu. Needs a fix, see below. |
| 2d Ctrl held | pass | |
| 4 Chrome text field | pass | |
| 4a Typing in panel search, then Enter | pass | Typing in the panel search box deselected the target's field; clicking an item still pasted at the right caret and restored focus. Acceptable. |
| 5 Elevated Notepad falls back to copy | pass | Status text was cut off in the owner's paste: `Copied: Notepad.exe runs as admin; Snipdeck does` (message is truncated or wraps off-panel; check). |
| 6 Password fields | pass | |

Not run: 2a, 2e, 2f, 3 (Word), 4b, 5b, 7-10 (full mode only). OneNote / Windows Terminal were not retested.

**Verdict S1: GO.** Core paste path works on real apps (Notepad, Chrome, elevated fallback, password fields).

Required changes:
- **Modifier hygiene:** when Alt or Win is down at click time, inject a neutral key (VK 0xFF / unassigned, the usual
  "mask key" trick) before releasing, or ignore the click while Alt/Win is held, so the target's menu bar and the
  Start menu do not trigger. Decide and add a test step; re-run 2c.
- Check the elevated-fallback message fits the footer (truncation).

## S2: IME

All three composition steps (1 Japanese, 3 Chinese, 4 Korean) were recorded **fail** ("nothing popped up"), and the
owner noted that Win+Space showed nothing, so no IME was probably installed. **Treat as INCONCLUSIVE, not a Slint
failure.** Re-run after adding Japanese, Chinese (Simplified) and Korean keyboards (Settings, Time & language).
Steps 10 and 15 skipped. Font fallback diagnostics are fine: Yu Gothic UI, Microsoft YaHei UI and Malgun Gothic are
present; Meiryo UI and IBM Plex Sans are not installed system-wide (Plex is registered after window creation).

## S4: Mica, corners, theme

| Step | Result | Note |
|---|---|---|
| W1 Windows 10 solid chrome | skipped | Machine is Windows 11, so this step does not apply. Screenshots show the panel with Mica requested and all DWM calls `ok` (dark, corners, border, extend=Full, backdrop=2). |
| W2 drag and resize | pass | **Defect:** the OS caption buttons (minimise, maximise, close) draw on top of the app's own small custom close button, so two close buttons overlap in the top-right (visible in both screenshots). |
| W3 dark / light switching | pass | Theme flips follow `UISettings.ColorValuesChanged` and `WM_SETTINGCHANGE` ("ImmersiveColorSet"); log shows dark, light, dark. |

Diagnostics: text scale 1.0 (TextScaleFactor registry value absent), transparency on, high contrast off, renderer
winit-femtovg. Mica visibility was not commented on; the screenshot shows a dark opaque-looking panel, so whether
Mica is visible is **unverified**.

Required changes:
- Fix the duplicate caption buttons: either suppress the OS caption buttons (custom frame) or drop the app's own
  close button. Decide in ADR-0003 / DESIGN.md.
- Not covered: W4 text size, W5 high contrast, W6 two monitors, W7 entry-point dialog, Windows 10.
