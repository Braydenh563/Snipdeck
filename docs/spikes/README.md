# M0 spikes: summary

Four throwaway prototypes (`spikes/`) to de-risk the choice of Slint before milestone work starts.
Everything compiled, linted and cross-linked for Windows from Linux. **Nothing has run on Windows.**
Only S3 produced measurements; S1, S2 and S4 are source-level analysis plus a test script.

| Spike | Question | Verdict | Verified | Unverified (needs Windows) |
|---|---|---|---|---|
| [S1](S1-noactivate.md) | Click the panel without stealing focus, then paste into the target | GO WITH CHANGES (provisional) | Compiles, lints, links | Whether focus is really preserved; paste into real apps |
| [S2](S2-ime.md) | Japanese / Chinese input in Slint text fields | GO WITH CHANGES (provisional), risk MEDIUM | Compiles, lints, links, renders under Xvfb | All IME behaviour |
| [S3](S3-longtext.md) | Long text and 10k-row list performance | GO WITH CHANGES | Measured (Linux VM, software renderer) | Windows GPU numbers |
| [S4](S4-mica.md) | Mica backdrop, corners, theme, DPI | GO WITH CHANGES (provisional) | Compiles, lints, links, solid-chrome render | Mica, corners, shadow, theme, DPI |

## Changes these results require

**Spec / design**
- **Inline text editing cap: 16 KB** (stock `TextEdit` re-lays-out all text per keystroke, about 1 ms/KB). 16 KB to 5 MB opens a read-only rope-based viewer (find, copy, whole-text transforms). Above 5 MB untested. (S3)
- **Search matches title + first 64 chars**, on multiple threads (4–8 ms for 10k items). Full-content matching misses the 16 ms budget. (S3)
- **Item lists must use Slint `ListView`** (virtualised). A plain scroll area with every row built costs 69 ms per step. (S3)
- **Mica may look flat grey** because the panel is never the active window. `DESIGN.md` §1/§5 must treat Mica as an enhancement over solid `bg.app` chrome, not the baseline. (S4)
- **Windows "Text size" setting must be read explicitly**; at 175% the tab bar overflows. (S4)
- **Caret detection needs a UI Automation fallback**; `GetGUIThreadInfo` misses Chrome, Store apps, terminals. (S1)
- **Clipboard restore at a fixed 300 ms is a guess**; slow targets (Remote Desktop) may paste the old content. Delayed rendering is the reliable signal. (S1)
- **IME composition shows as a selection highlight** (no underline, no active-clause marker), and the candidate window is anchored at the start of the composition. Acceptable, but different from native fields. (S2)

**Engineering**
- Create the panel with `with_active(false)`; re-apply the panel's extended style in a window subclass because winit rewrites it; read Shift with `GetAsyncKeyState`. (S1)
- Use the **software renderer** for transparent windows; Skia and FemtoVG have open transparency bugs on Windows (Slint #4120, #13572). (S4)
- **Pin Slint exactly**; set the CJK font fallback order per UI locale; force-commit IME composition before the panel deactivates. (S2)
- Act on **key-press only** in search (IME key-release still reaches Slint). (S2)
- Report upstream: software renderer panics selecting text on a no-wrap line wider than 32,767 px (`i16` coordinates). (S3)
- Open upstream bugs to track: slint#10861 (composition dropped on focus loss), #5206 (freeze with Microsoft Pinyin on Windows 10). (S2)

## Gate to proceed

Run the manual test scripts on Windows 11 (and the Windows 10 part of S4). Slint stays as the toolkit unless:
- **S1 NO-GO** (panel steals focus or paste fails in Notepad/Word/Chrome): fall back to a custom Slint platform on our own window with the software renderer, or the "re-focus last window" paste mode.
- **S2 NO-GO** (composition unusable in LineEdit/TextEdit): evaluate patching Slint upstream versus a different toolkit for text entry.
- **S4**: no gate. Mica failing just means solid chrome.

Suggested order: S1 (30 min) → S2 → S4. S3 needs no Windows run.

## Automation

The Windows test wizard ([`tools/windows-test`](../../tools/windows-test/README.md#automation-optional)) can run
part of each script by itself when the owner opts in. It records `pass`/`fail` only from observed evidence and
hands everything else to the manual screen. Nothing in it has run on Windows yet (see the README's list of
Windows-only assumptions).

| Spike | Automated steps | Manual steps |
|---|---|---|
| S1 | 1 (focus kept; taskbar / Alt+Tab confirmed with one y/n), 2, 2a, 2b, 2c, 2d, 5 (owner accepts UAC) | 2e, 2f, 3, 4, 4a, 4b, 5b, 6, 7, 8, 9, 10 |
| S2 | 16a, 16b, 16c (fallback chain from the app's start-up report; glyphs confirmed with one y/n) | 1-15, 17, 18 (IME composition cannot be driven reliably with synthetic input) |
| S4 | 1, 2, 6, 7, 10, 17; Windows 10: W1, W1b, W3, W4, W7 (corners / shadow confirmed with one y/n) | 3, 4, 5a, 5b, 8, 9, 11a-11f, 12-16, W2, W5, W6 |

S4 steps 6, 7, 10 and W3, W4 switch the setting through the registry plus `WM_SETTINGCHANGE`, not through the
Settings app, so they do not record which signal the Settings app itself sends.
