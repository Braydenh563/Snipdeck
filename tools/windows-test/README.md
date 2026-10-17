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
| Per step | Shows what to do and what you should see. Keys: `p` pass, `f` fail (asks what went wrong), `s` skip, `n` add a note, `c` screenshot (minimises the wizard, waits 5 s so you can bring the app forward), `l` view the app's console output, `r` relaunch, `b` back, `q` finish now. |
| Results | Prints a go/no-go verdict per prototype, copies a short summary to your clipboard, and saves everything to `spikes-results\current\`. Optionally zips it and commits the text report. |

**Afterwards:** paste the clipboard summary into the chat with Claude.

If you quit half-way, run it again and answer **y** to "Resume": steps you already answered are skipped.

Recommended order: **S1** (it decides the most), then **S2**, then **S4**.

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
    "launch": { "args": ["--x"], "env": { "NAME": "value" }, "elevated": false }  // omit to keep the app as it is
  }]
}
```

Check your edits without Windows: `pwsh tools/windows-test/run-tests.ps1 -SelfTest`
(validates every file and the verdict logic). `-DryRun` fakes Windows, the build and the app so the whole
interactive flow can be exercised anywhere.
