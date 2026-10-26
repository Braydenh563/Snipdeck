# Test Snipdeck on Windows

## Quick check (about 10 minutes)

Double-click `Test Snipdeck on Windows.bat` and press **1**. It checks only what changed lately and can only be
judged on Windows, on its own new empty test library (`spikes-results\qc-data`, deleted at the start of every new
Quick check, so your snippets and settings are never touched). It offers to close a normal Snipdeck that is
running (a second copy would just exit). The same keys as the full test: **P** worked, **F** didn't (one line about
what happened), **S** skip. Results go to the same place and format as the full test (`results.md`,
`paste-back.md`, the zip on your Desktop; the mode shows as `qc`). From a prompt: `.\tools\windows-test\run-tests.ps1 -Mode qc`.

| # | What you do | Done for you |
|---|---|---|
| 1 | Press the Panel shortcut: it shows and hides, Notepad keeps the cursor. The screen names the key: **Ctrl+Shift+Y** on this new test library (read from `qc-data\settings.json` when it exists); if a normal Snipdeck is installed it also says what that one uses (an older install keeps **Ctrl+Alt+Space**) | Presses it twice and watches the Panel window |
| 2 | Settings: Layout List / Cards / Compact | - |
| 3 | Copy `quickcheck` in Notepad: it is in Recent (Recent is on in a new install) | Types, copies and looks for it in the Panel (falls back to you if it cannot read the Panel) |
| 4 | Copy from a password manager: not in Recent (S if none installed) | - |
| 5 | One click on a row pastes into Notepad and the Panel hides | - |
| 6 | Pin keeps the Panel open | - |
| 7 | Search marks the matching letters | - |
| 8 | Settings: Lists = Menu (board-name menu switcher) | - |
| 9 | Show the Panel in the taskbar and Alt+Tab: the paste goes to the window you were in before | - |
| 10-11 | Dock to the taskbar's edge (the screen says which edge); the Panel stays clear of the taskbar | Measures the Panel against the screen's work area |
| 12 | Render diagnostic | Runs `diag-render.ps1 -Quick` (render checks only, about 3 minutes, a few yes/no questions), shows the path of the file it saves on the Desktop and puts a copy in the results zip |

Steps 1, 3 and 11 are tried by the wizard first (mouse and keyboard move for a few seconds; hold Esc to stop). A
check it cannot judge becomes the normal screen for you. The steps are in `tests\quick-check.json` (flag `qc`;
`qcOnly` keeps them out of the other modes); the automations are in `auto\qc.ps1`. **Unverified:** none of this has
run on Windows yet, and the automations only read the Panel by its window size and UI Automation text, so expect the
first run to fall back to manual on some of them.

The full test, unchanged, is the second choice of the same `.bat` (or press Enter in the wizard).

---

## Full test

You do not need to read anything else on this page. Five steps:

1. **Get the Snipdeck folder on your Windows PC** (Windows 10 or 11). Either download the ZIP from GitHub
   (Code > Download ZIP), right-click it, choose **Extract All**, and open the new folder; or `git clone` it.
2. **Double-click `Test Snipdeck on Windows.bat`** in that folder. A black window opens. If Windows says
   "Windows protected your PC", choose **More info > Run anyway**.
3. **Press Enter** at the welcome screen. That starts the recommended "v1 check" (about an hour). If it says
   something is missing (Rust, Visual Studio C++ Build Tools), it tells you what, how big and how long, and
   asks **Install now? (Y/n)**: press Enter. The first build of the apps takes 10-20 minutes; lines scroll past
   and a "still building" note appears every 30 seconds. You can walk away.
4. **Follow each step on screen.** Every screen has the same layout: *DO THIS* (numbered), *YOU SHOULD SEE*, then
   **P** = it worked, **F** = it didn't, **S** = skip (and **?** for more options). Some steps run by
   themselves: your mouse and keyboard move for a few seconds, and a 5-second countdown comes first. To stop
   them, hold **Esc**. If you press F, type one line about what happened (or just Enter) and let it take a screenshot.
5. **At the end** you get one file on your Desktop, `Snipdeck-test-results-<date>.zip`, Explorer opens with it
   selected, and a short summary is on your clipboard. **Drag the file into the Claude chat** (or paste the
   summary). Close the window when it says so.

Stopped half-way, or the PC restarted? Double-click the file again and press Enter at *Continue where you left
off?* Your earlier answers are kept.

Nothing is sent anywhere by the wizard. It never commits or pushes anything unless you choose that under
*Advanced options*.

---

# Details

This is the reference for the people who maintain the wizard. The wizard builds the apps to check, launches each
with the right settings, walks through the steps that need a person, and writes up the results. Entry point:
`Test Snipdeck on Windows.bat` (repo root) -> `tools/windows-test/run-tests.ps1` (Windows PowerShell 5.1, execution
policy bypassed for that one run). From Git Bash or WSL: `tools/windows-test/run-tests.sh`. From PowerShell:
`.\tools\windows-test\run-tests.ps1`. A problem shows a plain message and writes `spikes-results\wizard-log.txt`.

The pure logic (which steps a plan runs, time left, progress bar, resume detection, result counts, the zip name,
numbered actions, prerequisite wording) lives in `flow.ps1` and is covered by `auto\tests\selftest.ps1`.

## The flow

| Stage | What it does |
|---|---|
| Welcome | Four lines: what it does, how long, that the mouse may move (Esc stops it), that nothing is sent. Enter = the **v1 check**; **A** = *Advanced options*; **Q** = quit. If an unfinished run exists, the first question is *Continue where you left off? (Y/n)*. |
| Advanced options | Thoroughness (v1 check / quick / standard / full), which parts, automatic steps on/off, the Windows 10/11 step set, and (default no) offering to send the results text to GitHub at the end. Command line: `-Mode`, `-Spike`, `-Auto`, `-NoAuto`. |
| Getting ready | Detects Rust and the Visual Studio C++ Build Tools. If something is missing it says in plain words what, how big and how long, and asks once *Install now? (Y/n)*; it installs with `winget`. |
| Building | Builds every part of the run up front (`cargo build --release`), with a "still building" note every 30 s. A failed build offers retry / skip / stop. |
| Steps | Same layout on every screen (header `Step N of M - about X min left`, text progress bar, DO THIS, YOU SHOULD SEE). Keys: **P** worked, **F** didn't (asks *What happened?* and offers a screenshot), **S** skip, **?** shows the rest: **N** note, **C** screenshot, **L** the app's printed output, **R** restart the app, **K** put the clipboard text back, **B** back, **A** redo the automation, **Q** stop here. |
| Finish | Counts (worked / didn't / skipped / not done), a plain verdict per part, then the zip on the Desktop (`Snipdeck-test-results-<date>.zip`, `-2`, `-3` if that name exists), Explorer with the file selected, the one-line summary on the clipboard. |

The recommended **v1 check** is every step flagged `"v1": true` in `tests\*.json`. It covers what the owner must
verify before v1 ships (`docs/V1-PLAN.md`, G1): the M3 app shell (tray, taskbar, close to tray, light/dark, plus the
display-scaling, file-dialog and edge-dock steps 20-22), the M2 platform key steps plus single instance / autostart /
hotkey conflict / quit, the S1 floating-panel key steps (focus, Alt+Tab, caret, paste), and the S2 IME key steps
(needs Japanese, Chinese and Korean keyboards installed; press S at the start of that part to skip it). S4 (window
look) is not in it; pick it under *Advanced options*.
Time estimates: 1.5 minutes per manual step, 0.5 per automated step (an optional `"minutes"` field on a step
overrides that), plus the build.

## Automation

On by default for the v1 check (turn it off in *Advanced options* or with `-NoAuto`; `-Auto` forces it on). Steps
that have an `auto` field are performed and checked by the script; every other step is the normal manual screen.

| Spike | Automated | Stays manual, and why |
|---|---|---|
| S1 | 1 (click an empty spot of the panel: focus, title and caret of the target unchanged, panel topmost, no loader error dialog), 2 (type `abc`, click *Hello from Snipdeck*: Notepad keeps focus, text at the caret, clipboard back to `BEFORE` after ~1 s, item not added to Win+V history), 2a (unicode + two-line text exact), 2b (Shift+click: nothing inserted, item on the clipboard, Ctrl+V pastes it), 2c / 2d (Alt, Win, Ctrl held: pasted once, no menu mode, no Start menu, no stuck keys, typing works after), 5 (admin Notepad: copied not pasted, footer says why; **you accept the UAC prompt**) | Taskbar button / Alt+Tab (asked as one y/n question after step 1), Word, Chrome, the search box, password fields, fullscreen apps, terminals, Remote Desktop: they need other apps or judgement |
| S2 | 16a-16c: the Han / kana / Hangul fallback chain from the app's own start-up report (the glyph comparison is one y/n question) | Every IME step: composition and the candidate window are owned by the IME and cannot be driven reliably with synthetic key presses |
| S4 | 1, 2 (diagnostics + chrome pixels vs card zone: Mica-tinted / solid `bg.app` / black / white), 6 (dark/light), 7 (Transparency effects), 10 (Text size 150% / 225%: `text scale` line and the chrome getting taller), 17 (forced solid), W1, W1b, W3, W4, W7 on Windows 10 | 3, 4, 5a/5b (drag, inactive look), 8 (high contrast), 9 (DPI / monitors), 11-16 (matrix and eyeball checks), W2, W5, W6. Corners, shadow and border are asked as one y/n question |

How it decides: each automation returns **pass**, **fail** or **unsure**. *Fail* is recorded only on
unambiguous evidence (the panel took the focus, the item reached clipboard history, the clipboard was not
restored, pasted twice, a stuck modifier, black chrome next to correct cards, ...). Anything it cannot see clearly
is *unsure* and you get the normal manual screen for that step, with what the automation observed shown in yellow
(press `?` then `a` there to run it again). Results decided by the automation show `pass (auto)` / `fail (auto)` in
`results.md`, with the evidence in the notes. If a step needs your eyes (corners, Alt+Tab, Win+V when history
cannot be read), the wizard asks one y/n question; answering **n** turns it into the manual screen.

Safety:

- A **5-second countdown** before the first step that moves the mouse (2 s before later ones).
- **To stop: hold Esc, or move the mouse.** The automation checks both between every action; it then stops,
  the current step drops to the manual screen and every remaining step is manual. (Ctrl+C stays disabled.)
- A hard **time limit per step** (60 s; 90-150 s for the slow ones), after which the step drops to manual.
- Modifier keys it presses are always released (`finally`). It only types into its own scratch Notepad
  document (`spikes-results\current\auto\snipdeck-auto-*.txt`) and refuses to clear text unless that exact
  document is in front. It moves that Notepad next to the panel and puts it back at the end.
- Windows settings (app theme, Transparency effects, Text size) are changed through `HKCU` registry values plus a
  `WM_SETTINGCHANGE` broadcast, for a few seconds, and are **always restored** in a `finally`. The original
  values are also written to `spikes-results\auto-restore.json` first: if the wizard is killed mid-step, the next
  start of the wizard puts them back. (By hand: Settings > Personalization > Colors, and Accessibility > Text size.)
- The S4 checks use registry + broadcast, not the Settings app. If the app only listens to the Settings app's
  own signal, the step comes back *unsure* (it says so) and you do it by hand.
- Clipboard history (Win+V) can only be read from Windows PowerShell 5.1 (the `.bat` launcher uses it).
  Under PowerShell 7 the history check becomes the y/n question.

**First run on a new PC:** watch the first automated step. Things that were designed but could not be tried
without Windows are listed at the end of this file.

## M2: the Windows platform backend

`tests\m2-platform.json` checks `crates/snipdeck-platform` (clipboard with delayed rendering, `SendInput`,
Target tracking, global hotkeys, single instance, autostart, DPAPI) on a real PC. It is not a spike: the wizard
builds `tools\windows-test\m2-harness` (a console app, no window) and the steps drive it with global hotkeys.
Quick mode runs steps 1-6 (start-up report, Notepad paste and restore, Alt/Win held, HTML, plain paste, Copy); the v1 check adds 15-18.

| Chord | Does |
|---|---|
| Ctrl+Alt+1 / 2 / 3 | paste text / HTML item / plain-text paste of the HTML item |
| Ctrl+Alt+4 | Copy mode: the text item stays on the clipboard |
| Ctrl+Alt+5 / 6 / 8 | paste a 64x64 PNG with a transparent corner / two text files / long unicode text |
| Ctrl+Alt+7 | print Target info only (exe, title *length*, elevated, focus) |
| Ctrl+Alt+Q | quit (hotkeys are released) |

Every use prints one `[m2] use=... target=... focus=... route=... outcome=... ms=...` line; the wizard saves the
`[m2]` lines of each launch into the report. Clipboard content and window titles are never printed. Flags the steps
use: `--clobber` (another clipboard owner writes during the paste), `--probe <chord>` (also register this chord, to
see `InUse`), `--autostart-test [--hold <s>]` (writes then removes the real `Snipdeck` Run value: only run it
where no real Snipdeck is installed). Step 19 is `cargo test -p snipdeck-platform -- --ignored clipboard` run by
hand at the repo root. No step is automated yet. Nothing here has run on Windows when it was written: the first
run may find bugs in either the harness or the backend.

## M3: the app shell

`tests\m3-app.json` runs the real app (`cargo build --release` at the repo root, then `target\release\snipdeck-app.exe`
on an empty library in `spikes-results\m3-data`) and checks the tray icon (add, tooltip, left click, right-click menu,
Pause hotkeys, Quit, icon back after an Explorer restart), close to tray, following Windows dark mode and high contrast,
the `theme` setting, and that a damaged `settings.json` is kept as `settings.json.bad`. Quick mode runs the nine gate
steps. Writing settings has no step yet: nothing in the app changes a setting until the Settings window exists (M4).
Nothing here has run on Windows.

## Render and shortcut diagnostic (`diag-render.ps1`)

A separate, short run (about 10 minutes) for the render glitches and the shortcuts
(`docs/reviews/render-glitch-analysis.md`). From the repo folder, in a normal PowerShell window:

    powershell -NoProfile -ExecutionPolicy Bypass -File tools\windows-test\diag-render.ps1

It closes a running Snipdeck, builds, starts Snipdeck twice (`SNIPDECK_DIAG=render`, then `render,fix`, both
with `SNIPDECK_LOG=1`) on the wizard's library (`spikes-results\m3-data`, else `spikes-results\diag\data`),
prints numbered steps with a y/n question each, and writes one file, `Desktop\Snipdeck-diag-<date>.txt`.
`-NoBuild` skips the build; `-DataDir <folder>` picks another library.

## Where things are saved

`spikes-results\current\` (git-ignored): `results.md` (full report), `paste-back.md`, `paste-back-oneline.txt` (what goes
on the clipboard), `state.json` (resume data), `screens\*.png`, and the apps' console logs. The same folder is zipped to
the Desktop at the end. `spikes-results\wizard-log.txt` collects problems. An older run is moved to
`spikes-results\run-<date>` when a new one starts. Nothing is uploaded anywhere.

## Troubleshooting

- **"running scripts is disabled"**: use `Test Snipdeck on Windows.bat`, which bypasses the policy for this one run.
- **"Windows protected your PC"** when opening the `.bat` from a downloaded ZIP: More info > Run anyway.
- **The window seems frozen during a build**: clicking inside a console window pauses it while text is selected; press Enter.
- **Rust installed but "cargo not found"**: close the window and run the launcher again so the new PATH is picked up.
- **Build tools install does nothing**: it needs the admin (UAC) prompt; accept it, wait for it to finish, then re-run.
- **An app won't start**: press `?` then `l` to see its output; the wizard also prints the last lines when an app exits immediately.
- **The wizard stops with "Something went wrong"**: the details are in `spikes-results\wizard-log.txt`.
- **Antivirus complains about the freshly built `.exe`**: expected for unsigned local builds; allow it for the `spikes\*\target` folders.

## Adding a prototype or changing a step

Each prototype is one file in `tests\*.json`; the script needs no changes. Texts the person reads (`friendly`,
`title`, `do`, `expect`) are plain language: no spike ids, environment variables or jargon (ids show only in small grey text).

```jsonc
{
  "id": "S5", "title": "...", "friendly": "Plain name shown on screen", "note": "docs/spikes/S5-....md",
  "dir": "spikes/s5-...", "exe": "s5-...",          // cargo package folder and binary name
  "setup": ["things to do before starting"],
  "captureLines": ["text to pull from the app's console into the report"],   // optional
  "tests": [{
    "id": "1", "title": "...", "do": "...", "expect": "...",
    "gate": true,                       // part of the go/no-go decision (shown as "key step")
    "v1": true,                         // part of the recommended "v1 check" (every gate step of a part in it must be flagged)
    "minutes": 4,                       // optional: rough length for the time-left estimate (default 1.5 manual, 0.5 automated)
    "anyGroup": "mica",                 // passes if ANY step sharing this group name passes
    "optional": true,                   // skipped in quick and standard modes
    "os": "win11",                      // win11 = build 22621+, win10 = everything older; omit for both
    "launch": { "args": ["--x"], "env": { "NAME": "value" }, "elevated": false },  // omit to keep the app as it is
    "auto": "Invoke-S5Something",       // optional: automation function in auto\*.ps1
    "autoTimeout": 90                   // optional: its time limit in seconds (default 60)
  }]
}
```

Check your edits without Windows: `pwsh tools/windows-test/run-tests.ps1 -SelfTest`
(validates every file and the verdict logic, checks the v1 flags, parses every script, runs the flow tests
for `flow.ps1`, and runs the automation's unit tests and end-to-end tests against a simulated desktop). `-DryRun` fakes Windows, the build and the app so the whole
interactive flow can be exercised anywhere; with automation on it uses the simulated desktop
(`auto\mock\primitives.mock.ps1`), so no real input is ever sent. `SNIPDECK_MOCK_FAULTS=UISettingsStale,AbortAfter=5`
injects faults to walk the fallback paths (names are listed at the top of the mock file).

### Adding an automated step

1. Write `function Invoke-<Name>($Ctx)` in `auto\<spike>.ps1`. `$Ctx` has `SpikeId`, `TestId`, `Spike`, `Test`,
   `App` (`proc`, `norm`, `logOut`, `logErr`), `RunDir`, `RepoRoot`, `OsBuild`, `OsClass`.
2. Call `Enter-AutoHandsOff 'what is about to happen'` before the first mouse move or key press (countdown,
   abort guard). Use only the primitives (`Invoke-AutoClick`, `Send-AutoKeys`, `Send-AutoText`,
   `Get-AutoForeground`, `Get-AutoClipboard`, `Get-AutoUiaElements`, `Get-AutoScreenPixels`,
   `Set-AutoRegistryDword`, ...) and `Wait-Auto` / `Wait-AutoWindow` for waiting, so Esc, mouse movement and the
   time limit are noticed. Throwing is fine: any exception becomes *unsure*.
3. Turn observations into checks with `New-AutoCheck` in a pure function in `auto\lib.ps1` (mark a check
   `Hard` only when its failure is unambiguous) and return `Resolve-AutoChecks $checks $screenshots 'optional
   y/n question for the owner'`.
4. Add the `"auto"` field to the step, mirror the new primitive calls in `auto\mock\primitives.mock.ps1` if
   needed, and add cases (healthy and faulty) to `auto\tests\selftest.ps1`.

### Windows-only assumptions (not verifiable from Linux: watch the first run)

- The new wizard flow (welcome, Advanced options, resume question, up-front builds, step layout, finish screen) was
  exercised with `-DryRun` under PowerShell 7 on Linux with typed input, and its logic by `-SelfTest`; none of it
  has run on Windows PowerShell 5.1 or in a real console window.
- `Start-Process cargo -NoNewWindow -PassThru` with `WaitForExit(30000)` runs the build in this console, prints a
  "still building" note every 30 s and returns when `cargo` exits (the `mspdbsrv.exe` the linker leaves behind is not
  waited for); `ExitCode` is readable afterwards.
- `winget install` of `Rustlang.Rustup` and `Microsoft.VisualStudio.2022.BuildTools` finishes without further
  questions, and `vswhere` finds the C++ workload afterwards; Rust may need a new console window to be on the PATH.
- `[Environment]::GetFolderPath('Desktop')` is the real Desktop (also with OneDrive folder backup), `Compress-Archive`
  can read every file in `spikes-results\current` (the app logs are closed because the app was stopped), and
  `explorer.exe /select,"<zip>"` opens that folder with the file selected.
- The colours stay readable in the default console (black background, from the `.bat`) and in Windows Terminal;
  `DarkGray` is only used for small secondary text and is the weakest contrast (about 4.4:1 in Windows Terminal's
  default scheme). Pass and fail are always shown as words, never by colour alone.
- Step screens fit a 100-column console without horizontal scrolling and keep the whole screen above the cursor
  readable at the default 30-line window (long steps scroll; the key line is always last).
- `Clear-Host` between screens is fine in the legacy console host and in Windows Terminal.

- SendInput clicks on the never-activating S1 panel behave like real clicks, and injected Shift/Ctrl/Alt/Win
  are seen by the panel's `GetAsyncKeyState`.
- Slint's accessibility tree (AccessKit) exposes the S1 item buttons and footer/status texts to UI Automation
  with their text as the name, in physical-pixel coordinates (otherwise the step uses estimated positions,
  checked against what was pasted).
- Notepad's text is readable: classic `Edit` / Windows 11 `RichEditD2DPT` via `WM_GETTEXT`, else UI Automation;
  the Windows 11 Notepad opens the scratch file as a tab whose name appears in the window title.
- `SetWindowPos` (topmost, then not topmost) lifts Notepad / the S4 window without activating it; a synthesized
  click then activates it; `AttachThreadInput` + `SetForegroundWindow` hands the focus back to the wizard.
- `GetGUIThreadInfo` reports Notepad's menu mode and caret; Windows 11 Notepad's Alt access-key tips are not
  menu mode, so they show up only as "typing works afterwards: not checked".
- The Start menu / Search flyout shows up as the foreground window of `StartMenuExperienceHost` / `SearchHost`.
- Loader error dialogs are titled `<exe>.exe - ...` (or contain "Entry Point Not Found").
- WinRT clipboard history (`Clipboard.GetHistoryItemsAsync`) is allowed for a desktop PowerShell process.
- Screen capture (`CopyFromScreen`) returns the DWM-composed image including Mica; Mica over a colourful
  wallpaper differs from `bg.app` by more than 3 RGB units and has a colour tint (a grey wallpaper gives *unsure*).
- The S4 app notices registry + `WM_SETTINGCHANGE` changes; `UISettings` (which it reads first for
  transparency and text size) may NOT follow raw registry writes, which makes steps 7 / 10 / W4 *unsure*.
- UI Automation calls and the UAC prompt can block longer than the step's time limit (the limit is checked
  between actions, not inside a blocking call).
