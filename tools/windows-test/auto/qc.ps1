<#
  Quick check automations (tests/quick-check.json). Each Invoke-Qc* function is named by an "auto" field and
  returns @{ status; evidence; screenshots; manualCheck } through Resolve-AutoChecks / New-AutoResult.
  The rule is the same as for the other parts: "pass" or "fail" only on clear evidence; anything unclear is
  "unsure" and the person gets the normal manual screen. Nothing here has run on Windows when it was written.
  Uses helpers from lib.ps1, primitives.ps1 and s1.ps1 (Open-S1Notepad) and Get-QcPanelChord from run-tests.ps1.
  ASCII only, Windows PowerShell 5.1 compatible.
#>

function Get-QcSettingsPath($Ctx) { return (Join-Path (Join-Path (Join-Path $Ctx.RepoRoot 'spikes-results') 'qc-data') 'settings.json') }

# The Panel: the biggest visible window of the test app (the tray's hidden window and tiny toasts are skipped).
function Get-QcPanel($Ctx) {
    $best = $null; $area = 0
    foreach ($w in @(Get-AutoWindows -ProcessId $Ctx.App.proc.Id)) {
        $wd = [int]($w.Rect.R - $w.Rect.L); $ht = [int]($w.Rect.B - $w.Rect.T)
        if ($wd -lt 150 -or $ht -lt 100) { continue }
        if (($wd * $ht) -gt $area) { $best = $w; $area = $wd * $ht }
    }
    return $best
}

function Wait-QcPanel($Ctx, [bool]$Visible, [int]$TimeoutMs = 2500) {
    $n = [Math]::Max(1, [int]($TimeoutMs / 150))
    for ($i = 0; $i -lt $n; $i++) {
        $p = Get-QcPanel $Ctx
        if ([bool]$p -eq $Visible) { return $true }
        Wait-Auto 150
    }
    return $false
}

# Presses the Panel chord when the Panel is not in the wanted state. Returns $true when it is in that state afterwards.
function Set-QcPanelVisible($Ctx, [string]$Chord, [bool]$Visible) {
    if ([bool](Get-QcPanel $Ctx) -eq $Visible) { return $true }
    Send-AutoKeys @($Chord)
    return (Wait-QcPanel $Ctx $Visible 2500)
}

# Gives the scratch Notepad the keyboard (a click on its text area, clear of the Panel). Returns the Notepad.
function Enter-QcNotepad($Ctx) {
    $np = Open-S1Notepad $Ctx
    $null = Set-AutoForeground $np.Hwnd
    Wait-Auto 400
    $avoid = @()
    $panel = Get-QcPanel $Ctx
    if ($panel) { $avoid = @($panel.Rect) }
    $pt = Get-AutoPointOutside (Get-AutoClientRect $np.Hwnd) $avoid
    if (-not $pt) { throw 'no free spot to click into Notepad' }
    $null = Invoke-AutoClick $pt.X $pt.Y
    Wait-Auto 300
    $fg = Get-AutoForeground
    if (-not $fg -or [long]$fg.Hwnd -ne [long]$np.Hwnd) { throw "the scratch Notepad did not come to the front (front window: '$($fg.Title)')" }
    return $np
}

# ---------------------------------------------------------------- step 1: the hotkey shows and hides the Panel

function Invoke-QcHotkey($Ctx) {
    if (-not (Test-AutoAppRunning $Ctx)) { return New-AutoResult 'unsure' 'the test app is not running' }
    $chord = Get-QcPanelChord (Get-QcSettingsPath $Ctx)
    if (-not $chord) { return New-AutoResult 'unsure' 'the settings file says the Panel shortcut is off' }
    Enter-AutoHandsOff "Quick check step 1: press $chord twice and watch the Panel appear and go away"
    $np = Enter-QcNotepad $Ctx
    $startedVisible = [bool](Get-QcPanel $Ctx)
    $checks = @()
    Send-AutoKeys @($chord)
    $flipped = Wait-QcPanel $Ctx (-not $startedVisible) 2500
    $fg1 = Get-AutoForeground
    $flipNote = 'it was hidden, now showing'
    if ($startedVisible) { $flipNote = 'it was showing, now hidden' }
    $checks += New-AutoCheck "first press $chord toggles the Panel" $flipped $flipNote
    if ($flipped) {
        $checks += Get-AutoFocusCheck $fg1 ([long]$np.Hwnd) $Ctx.App.proc.Id 'Notepad'
        Send-AutoKeys @($chord)
        $back = Wait-QcPanel $Ctx $startedVisible 2500
        $checks += New-AutoCheck "second press $chord toggles it back" $back
    }
    # Leave the Panel hidden: the next steps say "open the Panel with the shortcut".
    $null = Set-QcPanelVisible $Ctx $chord $false
    return Resolve-AutoChecks $checks
}

# ---------------------------------------------------------------- step 3: Recent is on and keeps a normal copy

function Invoke-QcRecent($Ctx) {
    if (-not (Test-AutoAppRunning $Ctx)) { return New-AutoResult 'unsure' 'the test app is not running' }
    $chord = Get-QcPanelChord (Get-QcSettingsPath $Ctx)
    if (-not $chord) { return New-AutoResult 'unsure' 'the settings file says the Panel shortcut is off' }
    Enter-AutoHandsOff 'Quick check step 3: type quickcheck in Notepad, copy it, open the Panel and look for it in Recent'
    $np = Enter-QcNotepad $Ctx
    Send-AutoKeys @('Ctrl+A', 'Delete')
    Send-AutoText 'quickcheck'
    Wait-Auto 250
    Send-AutoKeys @('Ctrl+A', 'Ctrl+C')
    Wait-Auto 900
    $opened = Set-QcPanelVisible $Ctx $chord $true
    $found = $false; $seen = 0
    if ($opened) {
        for ($i = 0; $i -lt 8 -and -not $found; $i++) {
            $panel = Get-QcPanel $Ctx
            if ($panel) {
                $els = Get-AutoUiaElements $panel.Hwnd
                if ($els) {
                    $seen = @($els).Count
                    foreach ($e in @($els)) { if ($e.Name -and $e.Name -like '*quickcheck*') { $found = $true; break } }
                }
            }
            if (-not $found) { Wait-Auto 500 }
        }
    }
    $null = Set-QcPanelVisible $Ctx $chord $false
    $checks = @()
    $checks += New-AutoCheck 'the Panel opened' $opened
    $detail = if ($found) { 'seen in the Panel' } else { "not seen among $seen Panel elements; it may be on another list, or the Panel text cannot be read" }
    $foundOk = $null
    if ($found) { $foundOk = $true }
    $checks += New-AutoCheck 'quickcheck is in the Panel after a copy in Notepad' $foundOk $detail
    return Resolve-AutoChecks $checks
}

# ---------------------------------------------------------------- step 11: a docked Panel respects the work area

function Invoke-QcDockWorkArea($Ctx) {
    if (-not (Test-AutoAppRunning $Ctx)) { return New-AutoResult 'unsure' 'the test app is not running' }
    $p = Get-QcPanel $Ctx
    if (-not $p) { return New-AutoResult 'unsure' 'the Panel is not showing: open it with the shortcut (the dock step before this one), then use ? and a to run this again' }
    $work = Get-AutoWorkArea $p.Hwnd
    $bounds = $null
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $b = [System.Windows.Forms.Screen]::FromHandle([IntPtr][long]$p.Hwnd).Bounds
        $bounds = @{ L = [int]$b.Left; T = [int]$b.Top; R = [int]$b.Right; B = [int]$b.Bottom }
    } catch { }
    $r = $p.Rect
    $tol = 12
    $over = @()
    if ($r.L -lt $work.L - $tol) { $over += "left by $($work.L - $r.L)" }
    if ($r.T -lt $work.T - $tol) { $over += "top by $($work.T - $r.T)" }
    if ($r.R -gt $work.R + $tol) { $over += "right by $($r.R - $work.R)" }
    if ($r.B -gt $work.B + $tol) { $over += "bottom by $($r.B - $work.B)" }
    $touch = @()
    if ([Math]::Abs($r.L - $work.L) -le $tol) { $touch += 'left' }
    if ([Math]::Abs($r.T - $work.T) -le $tol) { $touch += 'top' }
    if ([Math]::Abs($r.R - $work.R) -le $tol) { $touch += 'right' }
    if ([Math]::Abs($r.B - $work.B) -le $tol) { $touch += 'bottom' }
    $edge = 'unknown'
    if ($bounds) { $edge = Get-TaskbarEdge $bounds $work }
    $checks = @()
    $inside = ($over.Count -eq 0)
    $insideNote = 'sticks out: ' + ($over -join ', ')
    if ($inside) { $insideNote = "Panel $($r.L),$($r.T)-$($r.R),$($r.B); work area $($work.L),$($work.T)-$($work.R),$($work.B)" }
    $checks += New-AutoCheck 'Panel inside the work area (clear of the taskbar)' $inside $insideNote
    $dockedOk = $null
    $dockedNote = 'not against any edge: dock was probably not set'
    if ($touch.Count -gt 0) { $dockedOk = $true; $dockedNote = 'touches: ' + ($touch -join ', ') + "; taskbar edge: $edge" }
    $checks += New-AutoCheck 'Panel is against a screen edge (docked)' $dockedOk $dockedNote
    return Resolve-AutoChecks $checks @() 'Look once at the docked Panel: no part of it is hidden behind the taskbar, and its bottom row is fully visible.'
}

# ---------------------------------------------------------------- step 12: the render diagnostic

function Invoke-QcDiagnostic($Ctx) {
    $script = Join-Path (Join-Path (Join-Path $Ctx.RepoRoot 'tools') 'windows-test') 'diag-render.ps1'
    $ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    if (-not (Test-Path -LiteralPath $script)) { return New-AutoResult 'unsure' "not found: $script" }
    if (-not (Test-Path -LiteralPath $ps)) { return New-AutoResult 'unsure' 'Windows PowerShell was not found' }
    $marker = Join-Path (Join-Path (Join-Path $Ctx.RepoRoot 'spikes-results') 'diag') 'last-diag-path.txt'
    Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue
    $dataDir = Join-Path (Join-Path $Ctx.RepoRoot 'spikes-results') 'qc-data'
    # The diagnostic starts the app itself (twice the same way), so the wizard's copy must be closed first.
    Stop-App
    Write-Host ''
    Write-Host '  Starting the render diagnostic. Answer its questions in this window.' -ForegroundColor Cyan
    & $ps -NoProfile -ExecutionPolicy Bypass -File $script -NoBuild -Quick -DataDir $dataDir
    $code = $LASTEXITCODE
    $file = ''
    if (Test-Path -LiteralPath $marker) { $file = (Get-Content -LiteralPath $marker -TotalCount 1) }
    if ($code -ne 0 -or -not $file -or -not (Test-Path -LiteralPath $file)) {
        return New-AutoResult 'unsure' "the diagnostic did not finish (exit code $code)"
    }
    Write-Host ''
    Write-Host "  Diagnostic file: $file" -ForegroundColor Green
    $copy = ''
    try {
        $dest = Join-Path $Ctx.RunDir 'diag'
        if (-not (Test-Path -LiteralPath $dest)) { New-Item -ItemType Directory -Path $dest | Out-Null }
        Copy-Item -LiteralPath $file -Destination $dest -Force
        $copy = ' (a copy is in the results zip)'
    } catch { }
    return New-AutoResult 'pass' "diagnostic file: $file$copy"
}
