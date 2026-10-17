# Windows test wizard

Builds the M0 spike apps, launches each one with the right settings, walks you through the manual test
steps from `docs/spikes/`, and writes up the results. You answer **p**ass / **f**ail / **s**kip.

## Run it

1. Get the repo on your Windows PC (Windows 10 1903+ or Windows 11):
   - `git clone https://github.com/Braydenh563/Backpack` then `git checkout claude/clipboard-assistant-design-o1eool`, **or**
   - download the ZIP of that branch from GitHub (Code > Download ZIP) and unzip it.
2. Double-click **`run-windows-tests.bat`** in the repo root.
   (From Git Bash or WSL: `tools/windows-test/run-tests.sh`. From PowerShell: `.\tools\windows-test\run-tests.ps1`.)

The wizard then:

| Stage | What it does |
|---|---|
| Prerequisites | Checks for Rust and the Visual Studio C++ Build Tools; offers to install them with `winget` (y/n). |
| What to test | You pick which prototypes (S1, S2, S4) and how thorough: quick (go/no-go steps only, about 20 min), standard, or full. |
| Per prototype | Shows the setup checklist, builds it (`cargo build --release`), launches it with the right arguments and environment for each step, and relaunches automatically when a step needs different settings. |
| Per step | Shows what to do and what you should see. Keys: `p` pass, `f` fail (asks what went wrong), `s` skip, `n` add a note, `c` screenshot (minimises the wizard, waits 5 s so you can bring the app forward), `l` view the app's console output, `r` relaunch, `b` back, `q` finish now. Optional: some steps run by themselves (see [Automation](#automation-optional)). |
| Results | Prints a go/no-go verdict per prototype, copies a short summary to your clipboard, and saves everything to `spikes-results\current\`. Optionally zips it and commits the text report. |

**Afterwards:** paste the clipboard summary into the chat with Claude.

If you quit half-way, run it again and answer **y** to "Resume": steps you already answered are skipped.

Recommended order: **S1** (it decides the most), then **S2**, then **S4**.

## Automation (optional)

After you pick what to test, the wizard asks once: *"Let me run the automatable steps for you? Your mouse and
keyboard will move; don't touch them. [y/N]"* (skip the question with `-Auto` or `-NoAuto`; `-NoAuto` is the
manual wizard exactly as before). With **y**, steps that have an `auto` field are performed and checked by the
script; every other step is the normal manual screen.

| Spike | Automated | Stays manual, and why |
|---|---|---|
| S1 | 1 (click an empty spot of the panel: focus, title and caret of the target unchanged, panel topmost, no loader error dialog), 2 (type `abc`, click *Hello from Snipdeck*: Notepad keeps focus, text at the caret, clipboard back to `BEFORE` after ~1 s, item not added to Win+V history), 2a (unicode + two-line text exact), 2b (Shift+click: nothing inserted, item on the clipboard, Ctrl+V pastes it), 2c / 2d (Alt, Win, Ctrl held: pasted once, no menu mode, no Start menu, no stuck keys, typing works after), 5 (admin Notepad: copied not pasted, footer says why; **you accept the UAC prompt**) | Taskbar button / Alt+Tab (asked as one y/n question after step 1), Word, Chrome, the search box, password fields, fullscreen apps, terminals, Remote Desktop: they need other apps or judgement |
| S2 | 16a-16c: the Han / kana / Hangul fallback chain from the app's own start-up report (the glyph comparison is one y/n question) | Every IME step: composition and the candidate window are owned by the IME and cannot be driven reliably with synthetic key presses |
| S4 | 1, 2 (diagnostics + chrome pixels vs card zone: Mica-tinted / solid `bg.app` / black / white), 6 (dark/light), 7 (Transparency effects), 10 (Text size 150% / 225%: `text scale` line and the chrome getting taller), 17 (forced solid), W1, W1b, W3, W4, W7 on Windows 10 | 3, 4, 5a/5b (drag, inactive look), 8 (high contrast), 9 (DPI / monitors), 11-16 (matrix and eyeball checks), W2, W5, W6. Corners, shadow and border are asked as one y/n question |

How it decides: each automation returns **pass**, **fail** or **unsure**. *Fail* is recorded only on
unambiguous evidence (the panel took the focus, the item reached clipboard history, the clipboard was not
restored, pasted twice, a stuck modifier, black chrome next to correct cards, ...). Anything it cannot see clearly
is *unsure* and you get the normal manual screen for that step, with what the automation observed shown in yellow
(press `a` there to run it again). Results decided by the automation show `pass (auto)` / `fail (auto)` in
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

## Where things are saved

`spikes-results\current\` (git-ignored): `results.md` (full report), `paste-back.md` (what goes on the clipboard),
`state.json` (resume data), `screens\*.png`, and the apps' console logs. Nothing is uploaded anywhere.

## Troubleshooting

- **"running scripts is disabled"**: use the `.bat` launcher, which bypasses the policy for this one run.
- **Rust installed but "cargo not found"**: close the window and run the launcher again so the new PATH is picked up.
- **Build tools install does nothing**: it needs the admin (UAC) prompt; accept it, wait for it to finish, then re-run.
- **An app won't start**: press `l` to see its output; the wizard also prints the last lines when an app exits immediately.
- **Antivirus complains about the freshly built `.exe`**: expected for unsigned local builds; allow it for the `spikes\*\target` folders.

## Adding a prototype or changing a step

Each prototype is one file in `tests\*.json`; the script needs no changes.

```jsonc
{
  "id": "S5", "title": "...", "note": "docs/spikes/S5-....md",
  "dir": "spikes/s5-...", "exe": "s5-...",          // cargo package folder and binary name
  "setup": ["things to do before starting"],
  "captureLines": ["text to pull from the app's console into the report"],   // optional
  "tests": [{
    "id": "1", "title": "...", "do": "...", "expect": "...",
    "gate": true,                       // part of the go/no-go decision
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
(validates every file and the verdict logic, parses every script, and runs the automation's unit tests and
end-to-end tests against a simulated desktop). `-DryRun` fakes Windows, the build and the app so the whole
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
