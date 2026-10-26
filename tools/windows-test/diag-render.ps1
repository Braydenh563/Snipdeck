<#
  Snipdeck render + hotkey diagnostic (docs/reviews/render-glitch-analysis.md).

  Run from the repo folder, in a normal (not admin) PowerShell window:
      powershell -NoProfile -ExecutionPolicy Bypass -File tools\windows-test\diag-render.ps1

  It builds Snipdeck (cargo build --release), runs it twice with SNIPDECK_DIAG set (first "render",
  then "render,fix"), asks you to do a few things and answer y/n, and writes ONE text file on your
  Desktop: Snipdeck-diag-<date>.txt. Send that file back. Nothing is sent anywhere by this script.
  The log never holds your snippets or window titles: only window sizes, positions, Windows
  messages, shortcut results and your y/n answers.

  Add -Quick for the short form the Quick check of the test wizard uses (run A render checks only, about
  3 minutes; no shortcut questions, no run B). It also writes spikes-results\diag\last-diag-path.txt.

  Windows PowerShell 5.1 compatible, ASCII only.
#>
param(
    [switch]$NoBuild,
    [switch]$Quick,   # the Quick check of the test wizard: run A render checks only (about 3 minutes); no shortcut questions, no run B
    [string]$DataDir = ''
)

$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$stamp = Get-Date -Format 'yyyyMMdd-HHmm'
$desktop = [Environment]::GetFolderPath('Desktop')
$outFile = Join-Path $desktop "Snipdeck-diag-$stamp.txt"
$work = Join-Path $repo 'spikes-results\diag'
$null = New-Item -ItemType Directory -Force -Path $work
$exe = Join-Path $repo 'target\release\snipdeck-app.exe'
if (-not $DataDir) {
    # The library the test wizard used (same settings and plan as in the owner test), else a new one.
    $wizardData = Join-Path $repo 'spikes-results\m3-data'
    if (Test-Path -LiteralPath $wizardData) { $DataDir = $wizardData } else { $DataDir = Join-Path $work 'data' }
}

function Say([string]$Text, [string]$Colour = 'White') { Write-Host $Text -ForegroundColor $Colour }

function Ask([string]$Question) {
    while ($true) {
        $a = (Read-Host "$Question  [y = yes / n = no / s = skip]").Trim().ToLower()
        if ($a -eq 'y' -or $a -eq 'n' -or $a -eq 's') { return $a }
    }
}

$header = New-Object System.Collections.Generic.List[string]
function Note([string]$Line) { $header.Add($Line) }

Say ''
Say '=== Snipdeck diagnostic: render glitches and shortcuts ===' Cyan
Say "The result goes to: $outFile" Cyan
Say ''

# ---------------------------------------------------------------- 1. No other Snipdeck running
$running = @(Get-Process -Name 'snipdeck-app', 'snipdeck' -ErrorAction SilentlyContinue)
if ($running.Count -gt 0) {
    Say 'Step 1. Snipdeck is already running. It must be closed (a second copy would just exit,' Yellow
    Say '        and a running copy holds the shortcuts).' Yellow
    $a = Read-Host '        Close it now? [Y/n]'
    if ($a.Trim().ToLower() -eq 'n') { Say 'Close Snipdeck from its tray icon, then run this again.' Red; exit 1 }
    $running | ForEach-Object { try { $_.CloseMainWindow() | Out-Null } catch {} }
    Start-Sleep -Seconds 3
    Get-Process -Name 'snipdeck-app', 'snipdeck' -ErrorAction SilentlyContinue | Stop-Process -Force
    Note "other Snipdeck copies were running at start: $($running.Count) (closed)"
} else {
    Say 'Step 1. No other Snipdeck is running. Good.' Green
    Note 'other Snipdeck copies were running at start: 0'
}

# ---------------------------------------------------------------- 2. Build
if (-not $NoBuild) {
    Say ''
    Say 'Step 2. Building Snipdeck (a few minutes the first time, seconds after that)...' Cyan
    if (-not $env:SNIPDECK_LICENCE_PUBLIC_KEY) {
        # The published throwaway test key the wizard uses (licence/key_selection.rs).
        $env:SNIPDECK_LICENCE_PUBLIC_KEY = 'Zr5+Myx6RTMyvZ0Kf32wVfXF7xoGraZtmLOftoEMRzo='
        $env:SNIPDECK_ALLOW_TEST_LICENCE_KEY = '1'
    }
    Push-Location $repo
    try {
        & cargo build --release --color never 2>&1 | ForEach-Object { Write-Host "  $_" -ForegroundColor DarkGray }
        if ($LASTEXITCODE -ne 0) { Say 'The build failed. Send the lines above.' Red; exit 1 }
    } finally { Pop-Location }
}
if (-not (Test-Path -LiteralPath $exe)) { Say "Not found: $exe" Red; exit 1 }

# ---------------------------------------------------------------- 3. Facts about this PC
Say ''
Say 'Step 3. Reading facts about this PC (Windows version, screens, graphics, transparency)...' Cyan
try {
    $os = Get-CimInstance Win32_OperatingSystem
    Note "windows: $($os.Caption) version $($os.Version) build $($os.BuildNumber)"
} catch { Note "windows: unknown ($($_.Exception.Message))" }
try {
    Get-CimInstance Win32_VideoController | ForEach-Object { Note "gpu: $($_.Name) driver $($_.DriverVersion)" }
} catch {}
try {
    Add-Type -AssemblyName System.Windows.Forms
    foreach ($s in [System.Windows.Forms.Screen]::AllScreens) {
        $b = $s.Bounds; $w = $s.WorkingArea
        Note ("screen: primary={0} bounds=({1},{2} {3}x{4}) work=({5},{6} {7}x{8})" -f $s.Primary, $b.X, $b.Y, $b.Width, $b.Height, $w.X, $w.Y, $w.Width, $w.Height)
    }
    $v = [System.Windows.Forms.SystemInformation]::VirtualScreen
    Note ("virtual screen: ({0},{1} {2}x{3})" -f $v.X, $v.Y, $v.Width, $v.Height)
} catch { Note "screens: unknown ($($_.Exception.Message))" }
try {
    $p = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -ErrorAction Stop
    Note "transparency effects (EnableTransparency): $($p.EnableTransparency); apps light theme: $($p.AppsUseLightTheme)"
} catch { Note 'transparency effects: unknown' }
try {
    $hc = Get-ItemProperty 'HKCU:\Control Panel\Accessibility\HighContrast' -ErrorAction Stop
    Note "high contrast flags: $($hc.Flags)"
} catch {}
try {
    $dpi = Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -ErrorAction Stop
    Note "applied dpi: $($dpi.AppliedDPI)"
} catch {}
# Only these well-known shortcut-grabbing programs are looked for; no other process names are read.
$grabbers = 'PowerToys', 'PowerToys.KeyboardManagerEngine', 'PowerToys.PowerLauncher', 'AutoHotkey', 'AutoHotkey64', 'AutoHotkeyU64',
    'Ditto', 'CopyQ', 'ShareX', 'Greenshot', 'Snagit32', 'SnagitEditor', 'Flow.Launcher', 'Everything', 'Listary', 'Keypirinha',
    'Wox', 'Clipboard', 'ClipClip', 'ClipboardFusion', 'TextExpander', 'espanso', 'PhraseExpress', 'Beeftext', 'Discord',
    'NVIDIA Share', 'nvcontainer', 'RadeonSoftware', 'LogiOptionsPlus', 'lghub', 'Teams', 'ms-teams', 'Slack', 'Zoom', 'OneNote', 'ONENOTE'
$found = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $grabbers -contains $_.ProcessName } | ForEach-Object { $_.ProcessName } | Sort-Object -Unique)
Note ("known shortcut-using programs running: " + ($(if ($found.Count) { $found -join ', ' } else { 'none' })))
Note "library folder: $DataDir"

# ---------------------------------------------------------------- the two runs
$markers = New-Object System.Collections.Generic.List[string]

function Run-App([string]$Diag, [string]$Label) {
    $err = Join-Path $work "$Label.err.log"
    $out = Join-Path $work "$Label.out.log"
    Remove-Item -LiteralPath $err, $out -ErrorAction SilentlyContinue
    $env:SNIPDECK_DIAG = $Diag
    $env:SNIPDECK_LOG = '1'
    $proc = Start-Process -FilePath $exe -ArgumentList @('--data-dir', "`"$DataDir`"") -WorkingDirectory $repo -PassThru `
        -RedirectStandardError $err -RedirectStandardOutput $out
    $null = $proc.Handle
    $started = Get-Date
    Remove-Item Env:\SNIPDECK_DIAG, Env:\SNIPDECK_LOG -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 3
    return @{ proc = $proc; err = $err; out = $out; started = $started; label = $Label }
}

function Mark($Run, [string]$Text) {
    $ms = [int]((Get-Date) - $Run.started).TotalMilliseconds
    $markers.Add(("{0}`t{1}`tsnipdeck-log +{2}ms ===== OWNER [{3}] {4}" -f $Run.label, $ms, $ms, $Run.label, $Text))
}

function Step($Run, [int]$N, [string]$Do, [string]$Question) {
    Say ''
    Say "  $N. $Do" White
    Mark $Run "step $N start: $Do"
    $a = Ask "     $Question"
    Mark $Run "step $N answer=$a : $Question"
}

function Stop-App($Run) {
    Say ''
    Say '  Last: right-click the Snipdeck tray icon and choose Quit (this window waits 20 seconds).' White
    if (-not $Run.proc.WaitForExit(20000)) {
        Say '  Still running: closing it.' Yellow
        try { $Run.proc.Kill() } catch {}
        $null = $Run.proc.WaitForExit(5000)
    }
}

function Render-Steps($Run) {
    Step $Run 1 'Open the Panel (Ctrl+Shift+Y or click the tray icon). With the Panel open, open About Snipdeck (Panel menu or tray menu). Wait 3 seconds.' 'Did About (or the Panel) show transparent patches or a grey box?'
    Step $Run 2 'Close About. With the Panel open, open Edit on any item. Wait 3 seconds.' 'Did the Edit window or the Panel show transparent patches or a grey box?'
    Step $Run 3 'Close Edit. Drag the Panel around for about 5 seconds, also half off the right edge of the screen and back.' 'Did a part of the Panel go transparent, even briefly?'
    Step $Run 4 'Open the Main window, open the Tags window. Drag the Tags window so half of it is off the bottom or right of the screen, let go, then drag it fully back on screen.' 'Did the part that was off screen stay transparent or empty?'
    Step $Run 5 'Alt+Tab to another app and back to the Main window; minimise it and restore it.' 'Did any Snipdeck window show transparent or empty areas?'
}

Say ''
Say 'Step 4. Run A: logging only (no fix). Snipdeck starts now. Do each numbered thing, then answer here.' Cyan
$a = Run-App 'render' 'runA'
if (-not $Quick) {
Say '  Shortcuts first (focus Notepad or any text field before each one):' White
Step $a 1 'Press Ctrl+Shift+Y. Press it again.' 'Did the Panel appear the first time and hide the second time?'
Step $a 2 'Press Ctrl+Shift+Space, then Esc.' 'Did the Picker appear?'
Step $a 3 'In Notepad, press Ctrl+Shift+U.' 'Did Snipdeck paste something or show a message?'
Step $a 4 'Press Ctrl+Shift+2 (only if you set it in Settings > Shortcuts; else answer s).' 'Did Snipdeck save the clipboard or show a message?'
Step $a 5 'Open Settings > Shortcuts and look at each row.' 'Does any row show a red message? (type the gist in the next question)'
$why = Read-Host '     If a row shows a message, type what it says (or press Enter)'
Mark $a "settings rows say: $why"
}
Say ''
Say '  Now the render checks (same Snipdeck, still running):' White
Render-Steps $a
Stop-App $a

$b = $null
if (-not $Quick) {
    Say ''
    Say 'Step 5. Run B: the candidate fix switched on. Same render checks once more.' Cyan
    $b = Run-App 'render,fix' 'runB'
    Render-Steps $b
    Stop-App $b
}

# ---------------------------------------------------------------- 6. One file
Say ''
Say 'Step 6. Writing the result file...' Cyan
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("Snipdeck diagnostic $stamp")
$lines.Add("commit: " + ((& git -C $repo rev-parse --short HEAD 2>$null) -join ''))
foreach ($h in $header) { $lines.Add($h) }
foreach ($run in @($a, $b)) {
    if (-not $run) { continue }
    $lines.Add('')
    $lines.Add("==================== $($run.label) (SNIPDECK_DIAG=" + $(if ($run.label -eq 'runA') { 'render' } else { 'render,fix' }) + ") exit code $($run.proc.ExitCode)")
    # Merge the app's lines and the owner's markers by their +ms time.
    $entries = New-Object System.Collections.Generic.List[object]
    $seq = 0
    foreach ($f in @($run.err, $run.out)) {
        if (Test-Path -LiteralPath $f) {
            foreach ($l in [IO.File]::ReadAllLines($f)) {
                $ms = 0
                if ($l -match '^snipdeck-log \+(\d+)ms') { $ms = [int64]$Matches[1] } elseif ($entries.Count -gt 0) { $ms = $entries[$entries.Count - 1].ms }
                $entries.Add([pscustomobject]@{ ms = $ms; seq = $seq; text = $l }); $seq++
            }
        }
    }
    foreach ($m in $markers) {
        $parts = $m.Split("`t", 3)
        if ($parts[0] -eq $run.label) { $entries.Add([pscustomobject]@{ ms = [int64]$parts[1]; seq = $seq; text = $parts[2] }); $seq++ }
    }
    foreach ($e in ($entries | Sort-Object ms, seq)) { $lines.Add($e.text) }
}
[IO.File]::WriteAllLines($outFile, $lines)
# The test wizard reads this to show the path and to put a copy in its results zip.
try { [IO.File]::WriteAllText((Join-Path $work 'last-diag-path.txt'), $outFile) } catch {}
Say ''
Say "Done. Send this one file back: $outFile" Green
try { Start-Process explorer.exe -ArgumentList "/select,`"$outFile`"" } catch {}
