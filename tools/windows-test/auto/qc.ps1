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

# ---------------------------------------------------------------- step 3: Recent asks first, keeps nothing before Yes

# The Panel's UI Automation elements whose name matches $Like, tried a few times (the Panel draws after it shows).
function Find-QcPanelElements($Ctx, [string]$Like) {
    for ($i = 0; $i -lt 8; $i++) {
        $panel = Get-QcPanel $Ctx
        if ($panel) {
            $els = Get-AutoUiaElements $panel.Hwnd
            if ($els) {
                $hit = @($els | Where-Object { $_.Name -and $_.Name -like $Like })
                if ($hit.Count -gt 0) { return $hit }
            }
        }
        Wait-Auto 500
    }
    return @()
}

# Copies $Word from the scratch Notepad (selects all, types it, copies it).
function Copy-QcWord($Ctx, [string]$Word) {
    $null = Enter-QcNotepad $Ctx
    Send-AutoKeys @('Ctrl+A', 'Delete')
    Send-AutoText $Word
    Wait-Auto 250
    Send-AutoKeys @('Ctrl+A', 'Ctrl+C')
    Wait-Auto 900
}

function Invoke-QcRecent($Ctx) {
    if (-not (Test-AutoAppRunning $Ctx)) { return New-AutoResult 'unsure' 'the test app is not running' }
    $chord = Get-QcPanelChord (Get-QcSettingsPath $Ctx)
    if (-not $chord) { return New-AutoResult 'unsure' 'the settings file says the Panel shortcut is off' }
    Enter-AutoHandsOff 'Quick check step 3: copy beforeyes, answer Yes to the Recent question, copy quickcheck and look in Recent'
    $checks = @()
    Copy-QcWord $Ctx 'beforeyes'
    $opened = Set-QcPanelVisible $Ctx $chord $true
    $checks += New-AutoCheck 'the Panel opened' $opened
    if (-not $opened) { return Resolve-AutoChecks $checks }
    $yes = @(Find-QcPanelElements $Ctx 'Yes' | Where-Object { $_.Type -eq 'Button' -and $_.Rect })
    if ($yes.Count -eq 0) {
        $null = Set-QcPanelVisible $Ctx $chord $false
        $checks += New-AutoCheck 'the Panel asks "Keep a Recent list of what you copy?"' $null 'no Yes button found (the welcome tour may be open, or the Panel text cannot be read)'
        return Resolve-AutoChecks $checks
    }
    $checks += New-AutoCheck 'the Panel asks "Keep a Recent list of what you copy?"' $true
    $r = $yes[0].Rect
    $null = Invoke-AutoClick ([int](($r.L + $r.R) / 2)) ([int](($r.T + $r.B) / 2))
    Wait-Auto 500
    $null = Set-QcPanelVisible $Ctx $chord $false
    Copy-QcWord $Ctx 'quickcheck'
    $null = Set-QcPanelVisible $Ctx $chord $true
    $found = (@(Find-QcPanelElements $Ctx '*quickcheck*').Count -gt 0)
    $early = @(Find-QcPanelElements $Ctx '*beforeyes*').Count
    $null = Set-QcPanelVisible $Ctx $chord $false
    $foundOk = $null
    if ($found) { $foundOk = $true }
    $checks += New-AutoCheck 'quickcheck is in Recent after Yes' $foundOk 'not seen; it may be on another list, or the Panel text cannot be read'
    # Absence is only evidence once the Panel text was read (quickcheck seen).
    if ($found) { $checks += New-AutoCheck 'beforeyes (copied before Yes) is not in Recent' ($early -eq 0) "$early element(s) named beforeyes" }
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

