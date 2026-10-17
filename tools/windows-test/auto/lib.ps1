<#
  Automation layer, part 1: pure helpers (no Windows calls, no input).
  Diagnostics parsers, pixel classifier, geometry, verdict builders, and the step runner.
  Everything here is unit-tested by `run-tests.ps1 -SelfTest` (see auto/tests/selftest.ps1).
  Dot-sourced by run-tests.ps1 into its script scope. Keep this file ASCII-only and PowerShell 5.1 compatible.
#>

$script:AutoState = @{
    Deadline       = [datetime]::MaxValue
    CountdownDone  = $false   # the 5 s countdown is shown once, before the first UI-driving step
    HandsOff       = $false   # true while a step is driving mouse/keyboard
    Aborted        = $false   # owner held Esc / moved the mouse: automation is off for the rest of the run
    WizardHwnd     = [long]0  # the wizard's own window, given focus back after each step
    FocusReturned  = $true
    Quiet          = $false   # self test: no console output, owner questions answered yes
    Notepad        = $null    # scratch Notepad used by the S1 automations
}

# ---------------------------------------------------------------- results and checks

function New-AutoResult([string]$Status, [string]$Evidence, $Screenshots = @(), [string]$ManualCheck = '') {
    return @{ status = $Status; evidence = $Evidence; screenshots = @($Screenshots | Where-Object { $_ }); manualCheck = $ManualCheck }
}

# One observable assertion. Ok: $true (seen), $false (seen wrong), $null (could not tell).
# Hard: a $false here is unambiguous evidence of the failure the step describes -> the step FAILS.
# Required: a $null or soft $false here makes the step 'unsure' (manual fallback) instead of 'pass'.
function New-AutoCheck([string]$Name, $Ok, [string]$Detail = '', [bool]$Hard = $false, [bool]$Required = $true) {
    return @{ Name = $Name; Ok = $Ok; Detail = $Detail; Hard = $Hard; Required = $Required }
}

function Format-AutoChecks($Checks) {
    $parts = @()
    foreach ($c in $Checks) {
        $state = if ($c.Ok -eq $true) { 'ok' } elseif ($c.Ok -eq $false) { if ($c.Hard) { 'FAILED' } else { 'no' } } else { 'not checked' }
        $p = "$($c.Name): $state"
        if ($c.Detail) { $p += " ($($c.Detail))" }
        $parts += $p
    }
    return ($parts -join '; ')
}

function Resolve-AutoChecks($Checks, $Screenshots = @(), [string]$ManualCheck = '') {
    $status = 'pass'
    foreach ($c in $Checks) { if ($c.Hard -and $c.Ok -eq $false) { $status = 'fail' } }
    if ($status -ne 'fail') {
        foreach ($c in $Checks) { if ($c.Required -and $c.Ok -ne $true) { $status = 'unsure' } }
    }
    $mc = if ($status -eq 'pass') { $ManualCheck } else { '' }
    return New-AutoResult $status (Format-AutoChecks $Checks) $Screenshots $mc
}

# ---------------------------------------------------------------- runner (cooperative abort + deadline)

function Write-AutoLog([string]$Text, [string]$Color = 'DarkCyan') {
    if (-not $script:AutoState.Quiet) { Write-Host "  [auto] $Text" -ForegroundColor $Color }
}

function Assert-AutoContinue {
    $why = Test-AutoAbortRequested
    if ($why) { throw "AUTO-ABORT: $why" }
    if ((Get-Date) -gt $script:AutoState.Deadline) { throw 'AUTO-TIMEOUT: the step took longer than its time limit' }
}

# Waits in 50 ms slices so Esc / mouse movement / the deadline are noticed quickly.
function Wait-Auto([int]$Ms) {
    $n = [Math]::Max(1, [int][Math]::Ceiling($Ms / 50.0))
    for ($i = 0; $i -lt $n; $i++) { Assert-AutoContinue; Invoke-AutoSleep 50 }
}

function Wait-AutoWindow([int]$ProcessId = 0, [string]$TitleLike = '', [string]$ClassName = '', [int]$TimeoutMs = 8000) {
    $n = [Math]::Max(1, [int]($TimeoutMs / 200))
    for ($i = 0; $i -lt $n; $i++) {
        $w = @(Get-AutoWindows -ProcessId $ProcessId -TitleLike $TitleLike -ClassName $ClassName)
        if ($w.Count -gt 0) { return $w[0] }
        Assert-AutoContinue
        Invoke-AutoSleep 200
    }
    return $null
}

function Wait-AutoForeground([long]$Hwnd, [int]$TimeoutMs) {
    $n = [Math]::Max(1, [int]($TimeoutMs / 200))
    for ($i = 0; $i -lt $n; $i++) {
        $fg = Get-AutoForeground
        if ($fg -and [long]$fg.Hwnd -eq $Hwnd) { return $true }
        Assert-AutoContinue
        Invoke-AutoSleep 200
    }
    return $false
}

# Start menu / Search / shell flyouts that a stray Win or Alt key would bring up.
function Test-AutoShellPopup($Win) {
    if (-not $Win) { return $false }
    return (@('StartMenuExperienceHost', 'SearchHost', 'SearchApp', 'SearchUI', 'ShellExperienceHost') -contains $Win.Process)
}

function Request-AutoOwnerOk([string]$Question) {
    if ($script:AutoState.Quiet) { return $true }
    return (Ask-YesNo $Question $true)
}

# Shown before a step that moves the mouse or types: a visible countdown (5 s the first time, 2 s after).
function Enter-AutoHandsOff([string]$What) {
    $secs = if ($script:AutoState.CountdownDone) { 2 } else { 5 }
    if (-not $script:AutoState.WizardHwnd) { try { $script:AutoState.WizardHwnd = [long](Get-AutoForeground).Hwnd } catch { } }
    if (-not $script:AutoState.Quiet) {
        Write-Host "  [auto] $What" -ForegroundColor Cyan
        Write-Host "  Hands off the mouse and keyboard. To stop: hold Esc, or move the mouse." -ForegroundColor Yellow
        Write-Host '  Starting in ' -NoNewline -ForegroundColor Yellow
    }
    for ($s = $secs; $s -ge 1; $s--) {
        if (-not $script:AutoState.Quiet) { Write-Host "$s... " -NoNewline -ForegroundColor Yellow }
        for ($k = 0; $k -lt 10; $k++) {
            if (Test-AutoEscHeld) { if (-not $script:AutoState.Quiet) { Write-Host '' }; throw 'AUTO-ABORT: Esc held during the countdown' }
            Invoke-AutoSleep 100
        }
    }
    if (-not $script:AutoState.Quiet) { Write-Host '' }
    $script:AutoState.CountdownDone = $true
    $script:AutoState.HandsOff = $true
    Start-AutoMouseGuard
}

function Exit-AutoHandsOff {
    $wasHandsOff = $script:AutoState.HandsOff
    $script:AutoState.HandsOff = $false
    try { Stop-AutoMouseGuard } catch { }
    try { Clear-AutoModifiers } catch { }
    try { Restore-AutoRegistry } catch { }
    if ($wasHandsOff -and $script:AutoState.WizardHwnd) {
        $ok = $false
        try { $ok = [bool](Set-AutoForeground $script:AutoState.WizardHwnd) } catch { }
        $script:AutoState.FocusReturned = $ok
    }
}

# Runs one automation. Never throws: any exception, abort or timeout becomes 'unsure'.
function Invoke-AutoStep($Ctx, [string]$Name, [int]$TimeoutSec = 60) {
    if (-not $Name -or -not (Get-Command $Name -CommandType Function -ErrorAction SilentlyContinue)) {
        return New-AutoResult 'unsure' "no automation function named '$Name'"
    }
    $script:AutoState.Deadline = (Get-Date).AddSeconds($TimeoutSec)
    $script:AutoState.FocusReturned = $true
    try {
        $out = @(& $Name $Ctx)
        $r = $null
        foreach ($o in $out) { if ($o -is [hashtable] -and $o.ContainsKey('status')) { $r = $o } }
        if (-not $r -or @('pass', 'fail', 'unsure') -notcontains $r.status) { return New-AutoResult 'unsure' 'the automation returned no verdict' }
        if (-not $r.ContainsKey('screenshots')) { $r.screenshots = @() }
        if (-not $r.ContainsKey('manualCheck')) { $r.manualCheck = '' }
        if (-not $r.evidence) { $r.evidence = '' }
        return $r
    } catch {
        $msg = $_.Exception.Message
        if ($msg -like 'AUTO-ABORT*') {
            $script:AutoState.Aborted = $true
            return New-AutoResult 'unsure' ("stopped by you: " + $msg.Substring(12))
        }
        if ($msg -like 'AUTO-TIMEOUT*') { return New-AutoResult 'unsure' ("timed out after $TimeoutSec s: " + $msg.Substring(14)) }
        return New-AutoResult 'unsure' "automation error: $msg"
    } finally {
        $script:AutoState.Deadline = [datetime]::MaxValue
        Exit-AutoHandsOff
    }
}

# ---------------------------------------------------------------- text helpers

function ConvertTo-AutoNormText($Text) {
    if ($null -eq $Text) { return $null }
    $t = ([string]$Text) -replace "`r`n", "`n" -replace "`r", "`n"
    return $t.TrimEnd("`n")
}

function Format-AutoShort($Text, [int]$Max = 60) {
    if ($null -eq $Text) { return '<none>' }
    $t = ([string]$Text) -replace "`r", '\r' -replace "`n", '\n'
    if ($t.Length -gt $Max) { $t = $t.Substring(0, $Max) + '...' }
    return "'$t'"
}

function Get-AutoInvariantDouble([string]$Text) {
    $v = 0.0
    if ([double]::TryParse($Text, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$v)) { return $v }
    return $null
}

# ---------------------------------------------------------------- S4 diagnostics parser

# Parses one "[s4] last refresh: ..." line (format: spikes/s4-mica/src/main.rs `refresh`). $null for other lines.
function ConvertFrom-S4Diag([string]$Line) {
    if (-not $Line -or $Line -notmatch '\[s4\] last refresh: ') { return $null }
    $d = @{ Raw = $Line; Reason = $null; Build = $null; Theme = $null; Transparency = $null; HighContrast = $null
        Dpi = $null; TextScale = $null; Chrome = $null; Dwm = @{}; Renderer = $null; Sources = @{} }
    $m = [regex]::Match($Line, 'last refresh: (.*?) \|'); if ($m.Success) { $d.Reason = $m.Groups[1].Value }
    $m = [regex]::Match($Line, 'OS build (\d+)'); if ($m.Success) { $d.Build = [int]$m.Groups[1].Value }
    $m = [regex]::Match($Line, '\btheme (dark|light)\b'); if ($m.Success) { $d.Theme = $m.Groups[1].Value }
    $m = [regex]::Match($Line, '\btransparency (on|off)\b'); if ($m.Success) { $d.Transparency = $m.Groups[1].Value }
    $m = [regex]::Match($Line, '\bhigh contrast (on|off)\b'); if ($m.Success) { $d.HighContrast = $m.Groups[1].Value }
    $m = [regex]::Match($Line, 'DPI scale ([0-9.]+)'); if ($m.Success) { $d.Dpi = Get-AutoInvariantDouble $m.Groups[1].Value }
    $m = [regex]::Match($Line, 'text scale ([0-9.]+)'); if ($m.Success) { $d.TextScale = Get-AutoInvariantDouble $m.Groups[1].Value }
    $m = [regex]::Match($Line, 'chrome: ([A-Za-z]+\([A-Za-z]+\))'); if ($m.Success) { $d.Chrome = $m.Groups[1].Value }
    $m = [regex]::Match($Line, 'DWM: (.*?) \| renderer')
    if ($m.Success) {
        foreach ($kv in ($m.Groups[1].Value -split ' ')) {
            $i = $kv.IndexOf('=')
            if ($i -gt 0) { $d.Dwm[$kv.Substring(0, $i)] = $kv.Substring($i + 1) }
        }
    }
    $m = [regex]::Match($Line, 'renderer: (.*?) \| config'); if ($m.Success) { $d.Renderer = $m.Groups[1].Value }
    $m = [regex]::Match($Line, 'sources: (.*)$')
    if ($m.Success) {
        foreach ($k in 'AppsUseLightTheme', 'EnableTransparency', 'TextScaleFactor', 'AdvancedEffects', 'TextScale') {
            $mm = [regex]::Match($m.Groups[1].Value, "\b$k=([^\s;]+)")
            if ($mm.Success) { $d.Sources[$k] = $mm.Groups[1].Value }
        }
    }
    return $d
}

function Get-S4DiagList($Lines) {
    $out = @()
    foreach ($l in @($Lines)) { $d = ConvertFrom-S4Diag ([string]$l); if ($d) { $out += $d } }
    return , $out
}

# True when every DWM call in the report succeeded ("ok", or "<kind>:ok" for extend/backdrop).
function Test-S4DwmOk($Dwm, [string[]]$Keys) {
    foreach ($k in $Keys) {
        if (-not $Dwm.ContainsKey($k)) { return $false }
        if ($Dwm[$k] -ne 'ok' -and $Dwm[$k] -notlike '*:ok') { return $false }
    }
    return $true
}

# ---------------------------------------------------------------- S2 console report parser

# Parses the font lines S2 prints at start-up (spikes/s2-ime/src/main.rs `configure_cjk_fallback`).
function ConvertFrom-S2FontReport($Lines) {
    $r = @{ SystemFallback = @{}; Found = @{}; Override = @{}; Effective = @{}; Mode = $null; Backend = $null; SystemMode = $false }
    foreach ($l in @($Lines)) {
        $s = [string]$l
        $m = [regex]::Match($s, 'system fallback before override\s+(\w{4}):\s*(.*)$')
        if ($m.Success) { $r.SystemFallback[$m.Groups[1].Value] = $m.Groups[2].Value.Trim(); continue }
        $m = [regex]::Match($s, 'family\s+(.+?)\s+found \(before window creation\):\s*(true|false)')
        if ($m.Success) { $r.Found[$m.Groups[1].Value.Trim()] = ($m.Groups[2].Value -eq 'true'); continue }
        $m = [regex]::Match($s, '^\s*override (\w{4}):\s*(.*)$')
        if ($m.Success) {
            $v = $m.Groups[2].Value.Trim()
            $r.Override[$m.Groups[1].Value] = if ($v -like 'no candidate*') { @() } else { @($v -split ',\s*') }
            continue
        }
        $m = [regex]::Match($s, 'effective fallback after window creation (\w{4}):\s*(.*)$')
        if ($m.Success) { $r.Effective[$m.Groups[1].Value] = $m.Groups[2].Value.Trim(); continue }
        $m = [regex]::Match($s, '--cjk (\w+) ')
        if ($m.Success -and -not $r.Mode) { $r.Mode = $m.Groups[1].Value }
        if ($s -like '*--cjk system: fallback left to DirectWrite*') { $r.SystemMode = $true }
    }
    return $r
}

# ---------------------------------------------------------------- colours and pixels

function ConvertFrom-AutoHex([string]$Hex) {
    $h = $Hex.TrimStart('#')
    return @([Convert]::ToInt32($h.Substring(0, 2), 16), [Convert]::ToInt32($h.Substring(2, 2), 16), [Convert]::ToInt32($h.Substring(4, 2), 16))
}

# Expected solid colours from design/tokens/palette.json (the same source the S4 build uses).
function Get-AutoPaletteColors($PaletteJson) {
    $out = @{}
    foreach ($theme in 'light', 'dark') {
        $t = $PaletteJson.themes.$theme
        $out[$theme] = @{
            BgApp        = ConvertFrom-AutoHex $t.'bg.app'
            Surface      = ConvertFrom-AutoHex $t.'bg.surface'
            BorderSubtle = ConvertFrom-AutoHex $t.'border.subtle'
        }
    }
    return $out
}

function Measure-AutoColorDistance($A, $B) {
    $dr = $A[0] - $B[0]; $dg = $A[1] - $B[1]; $db = $A[2] - $B[2]
    return [Math]::Sqrt($dr * $dr + $dg * $dg + $db * $db)
}

function Get-AutoLuminance($C) { return (0.2126 * $C[0] + 0.7152 * $C[1] + 0.0722 * $C[2]) }

# An image is @{ W; H; Px = int[] ARGB } (screen capture) or @{ W; H; Fn = { param($x, $y) @(r, g, b) } } (synthetic).
function Get-AutoPixel($Img, [int]$X, [int]$Y) {
    if ($Img.Fn) { return (& $Img.Fn $X $Y) }
    $p = $Img.Px[$Y * $Img.W + $X]
    return @((($p -shr 16) -band 0xFF), (($p -shr 8) -band 0xFF), ($p -band 0xFF))
}

# Mean colour and spread (largest per-channel standard deviation) over a sparse grid in a rectangle.
function Get-AutoRegionStats($Img, $Rect, [int]$MaxSamples = 400) {
    $x0 = [Math]::Max(0, [int]$Rect.L); $y0 = [Math]::Max(0, [int]$Rect.T)
    $x1 = [Math]::Min($Img.W, [int]$Rect.R); $y1 = [Math]::Min($Img.H, [int]$Rect.B)
    if ($x1 -le $x0 -or $y1 -le $y0) { return $null }
    $area = ($x1 - $x0) * ($y1 - $y0)
    $step = [Math]::Max(1, [int][Math]::Floor([Math]::Sqrt($area / [double]$MaxSamples)))
    $n = 0; $s = @(0.0, 0.0, 0.0); $q = @(0.0, 0.0, 0.0)
    for ($y = $y0; $y -lt $y1; $y += $step) {
        for ($x = $x0; $x -lt $x1; $x += $step) {
            $c = Get-AutoPixel $Img $x $y
            for ($k = 0; $k -lt 3; $k++) { $s[$k] += $c[$k]; $q[$k] += $c[$k] * $c[$k] }
            $n++
        }
    }
    $mean = @(($s[0] / $n), ($s[1] / $n), ($s[2] / $n))
    $spread = 0.0
    for ($k = 0; $k -lt 3; $k++) { $v = [Math]::Sqrt([Math]::Max(0, $q[$k] / $n - $mean[$k] * $mean[$k])); if ($v -gt $spread) { $spread = $v } }
    $mx = [Math]::Max($mean[0], [Math]::Max($mean[1], $mean[2])); $mn = [Math]::Min($mean[0], [Math]::Min($mean[1], $mean[2]))
    return @{ Mean = $mean; Spread = $spread; Chroma = ($mx - $mn); Max = $mx; Min = $mn; N = $n }
}

function Format-AutoRgb($C) { if (-not $C) { return '?' }; return ('rgb({0},{1},{2})' -f [int]$C[0], [int]$C[1], [int]$C[2]) }

# Classifies the chrome zone against the card zone and the expected solid bg.app.
#   black : the failure mode where GL alpha is lost       white : opaque white
#   solid : identical to the card zone (bg.app)           mica  : differs from the cards and carries a colour tint
#   grey  : differs from the cards but neutral (inactive Mica, or opaque grey: cannot tell)
#   mixed : not uniform (something is drawn over the sample)
function Get-AutoChromeClass($Chrome, $Card, $ExpectedBg) {
    if (-not $Chrome -or -not $Card) { return @{ Class = 'unknown'; Detail = 'no pixels' } }
    $dCard = Measure-AutoColorDistance $Chrome.Mean $Card.Mean
    $dBg = Measure-AutoColorDistance $Chrome.Mean $ExpectedBg
    $detail = "chrome $(Format-AutoRgb $Chrome.Mean), cards $(Format-AutoRgb $Card.Mean), bg.app $(Format-AutoRgb $ExpectedBg)"
    $cls = 'grey'
    if ($Chrome.Spread -gt 12) { $cls = 'mixed' }
    elseif ($Chrome.Max -le 8) { $cls = 'black' }
    elseif ($Chrome.Min -ge 252) { $cls = 'white' }
    elseif ($dCard -le 3) { $cls = 'solid' }
    elseif ($Chrome.Chroma -ge 3 -and $dBg -gt 3) { $cls = 'mica' }
    return @{ Class = $cls; Detail = $detail; DistCard = $dCard; DistBg = $dBg }
}

# ---------------------------------------------------------------- S4 layout (spikes/s4-mica/ui/panel.slint)

function Get-S4Metrics([double]$TextScale) {
    $ts = if ($TextScale -gt 0) { $TextScale } else { 1.0 }
    return @{
        Header = [Math]::Max(40, 20 * $ts + 16)
        Tab    = [Math]::Max(32, 20 * $ts + 8)
        Footer = [Math]::Max(32, 16 * $ts + 16)
    }
}

# Sample rectangles in image pixels for a window image W x H at DPI scale $Scale.
function Get-S4Zones([int]$W, [int]$H, [double]$Scale, [double]$TextScale) {
    $s = if ($Scale -gt 0) { $Scale } else { 1.0 }
    $m = Get-S4Metrics $TextScale
    $hdr = $m.Header * $s; $tab = $m.Tab * $s; $ftr = $m.Footer * $s
    $cardTop = $hdr + $tab + 1 * $s
    return @{
        Header      = @{ L = [int]($W * 0.42); R = [int]($W * 0.70); T = [int]($hdr * 0.3); B = [int]($hdr * 0.7) }
        Footer      = @{ L = [int]($W * 0.82); R = [int]($W * 0.94); T = [int]($H - $ftr * 0.7); B = [int]($H - $ftr * 0.3) }
        CardStrip   = @{ L = [int][Math]::Ceiling(2.5 * $s); R = [int][Math]::Ceiling(5 * $s); T = [int]($cardTop + 10 * $s); B = [int]($H - $ftr - 10 * $s) }
        CardClick   = @{ X = [int]($W * 0.6); Y = [int]($cardTop + 24 * $s) }
        HairX       = @([int]($W * 0.88), [int]($W * 0.92), [int]($W * 0.96))
        HairFrom    = [int](30 * $s)
        HairTo      = [int]($H * 0.6)
        HairExpect  = [int]($hdr + $tab)
    }
}

# First thin horizontal line (all columns match $Color within $Tol, the row 3 above does not) between YFrom and YTo.
function Find-AutoHairlineRow($Img, [int[]]$Xs, [int]$YFrom, [int]$YTo, $Color, [double]$Tol = 8) {
    $yTo = [Math]::Min($YTo, $Img.H - 1)
    for ($y = [Math]::Max(3, $YFrom); $y -le $yTo; $y++) {
        $all = $true; $above = $true
        foreach ($x in $Xs) {
            if ((Measure-AutoColorDistance (Get-AutoPixel $Img $x $y) $Color) -gt $Tol) { $all = $false; break }
        }
        if (-not $all) { continue }
        foreach ($x in $Xs) {
            if ((Measure-AutoColorDistance (Get-AutoPixel $Img $x ($y - 3)) $Color) -gt $Tol) { $above = $false; break }
        }
        if (-not $above) { return $y }
    }
    return $null
}

# Everything the S4 checks need from one capture of the window.
function Measure-S4Image($Img, [double]$Scale, [double]$TextScale, $Colors) {
    $z = Get-S4Zones $Img.W $Img.H $Scale $TextScale
    $chrome = Get-AutoRegionStats $Img $z.Header
    $footer = Get-AutoRegionStats $Img $z.Footer
    $card = Get-AutoRegionStats $Img $z.CardStrip 200
    $cls = Get-AutoChromeClass $chrome $card $Colors.BgApp
    $hair = Find-AutoHairlineRow $Img $z.HairX $z.HairFrom $z.HairTo $Colors.BorderSubtle 8
    return @{ Zones = $z; Chrome = $chrome; Footer = $footer; Card = $card; Class = $cls; Hairline = $hair }
}

function Get-AutoThemeOfColor($C, $Colors) {
    if (-not $C) { return $null }
    if ((Measure-AutoColorDistance $C $Colors.light.BgApp) -le 8) { return 'light' }
    if ((Measure-AutoColorDistance $C $Colors.dark.BgApp) -le 8) { return 'dark' }
    return $null
}

# ---------------------------------------------------------------- geometry

function Get-AutoRectCenter($R) { return @{ X = [int](($R.L + $R.R) / 2); Y = [int](($R.T + $R.B) / 2) } }

function Test-AutoRectOverlap($A, $B) {
    if (-not $A -or -not $B) { return $false }
    return ($A.L -lt $B.R -and $B.L -lt $A.R -and $A.T -lt $B.B -and $B.T -lt $A.B)
}

function Test-AutoPointInRect([int]$X, [int]$Y, $R) {
    if (-not $R) { return $false }
    return ($X -ge $R.L -and $X -lt $R.R -and $Y -ge $R.T -and $Y -lt $R.B)
}

# Finds a W x H rectangle inside $Work that overlaps none of $Must and, if possible, none of $Nice.
function Get-AutoFreeRect($Work, $Must, $Nice, [int]$W, [int]$H, [int]$Step = 40) {
    foreach ($avoid in @(@(@($Must) + @($Nice)), @($Must))) {
        for ($y = $Work.T; $y + $H -le $Work.B; $y += $Step) {
            for ($x = $Work.L; $x + $W -le $Work.R; $x += $Step) {
                $r = @{ L = $x; T = $y; R = $x + $W; B = $y + $H }
                $hit = $false
                foreach ($a in $avoid) { if ($a -and (Test-AutoRectOverlap $r $a)) { $hit = $true; break } }
                if (-not $hit) { return $r }
            }
        }
    }
    return $null
}

# A point inside $Rect (prefers the middle) that is outside every rectangle in $Avoid.
function Get-AutoPointOutside($Rect, $Avoid) {
    foreach ($f in @(@(0.5, 0.6), @(0.3, 0.6), @(0.7, 0.6), @(0.5, 0.8), @(0.2, 0.8), @(0.8, 0.8), @(0.5, 0.4))) {
        $x = [int]($Rect.L + ($Rect.R - $Rect.L) * $f[0]); $y = [int]($Rect.T + ($Rect.B - $Rect.T) * $f[1])
        $hit = $false
        foreach ($a in @($Avoid)) { if (Test-AutoPointInRect $x $y $a) { $hit = $true; break } }
        if (-not $hit) { return @{ X = $x; Y = $y } }
    }
    return $null
}

# ---------------------------------------------------------------- S1 panel (spikes/s1-noactivate/src/main.rs)

function Get-S1Items {
    # Built from code points so this file stays ASCII (Windows PowerShell 5.1 reads BOM-less scripts as ANSI).
    $u = -join @([char]0xFC, 'n', [char]0xEF, 'c', [char]0xF8, 'd', [char]0xE9, ' ', [char]0x2713, ' ', [char]0x65E5, [char]0x672C, [char]0x8A9E, ' ', [char]::ConvertFromUtf32(0x1F642))
    return @('Hello from Snipdeck', "Kind regards,`nAlex", $u, 'SELECT * FROM items;')
}

function ConvertTo-AutoKey([string]$Text) { if ($null -eq $Text) { return '' }; return (($Text -replace '\s+', ' ').Trim()) }

# UI Automation element (Name/Type/Rect) whose name matches $Text, preferring buttons.
function Find-AutoElementRect($Elements, [string]$Text, [string]$PreferType = 'Button') {
    $want = ConvertTo-AutoKey $Text
    $best = $null
    foreach ($e in @($Elements)) {
        if (-not $e -or -not $e.Rect) { continue }
        if (($e.Rect.R - $e.Rect.L) -le 0 -or ($e.Rect.B - $e.Rect.T) -le 0) { continue }
        if ((ConvertTo-AutoKey $e.Name) -ne $want) { continue }
        if ($e.Type -eq $PreferType) { return $e.Rect }
        if (-not $best) { $best = $e.Rect }
    }
    return $best
}

# Footer ("Pastes into ..." / "Copies to clipboard (...)") and status line from the panel's UI Automation names.
function Get-S1PanelTexts($Elements) {
    $r = @{ Footer = $null; Status = $null }
    foreach ($e in @($Elements)) {
        if (-not $e) { continue }
        $n = [string]$e.Name
        if (-not $r.Footer -and $n -match '^(Pastes into|Copies to clipboard)') { $r.Footer = $n }
        elseif (-not $r.Status -and $n -match '^(Pasted into|Copied|Couldn)') { $r.Status = $n }
    }
    return $r
}

# Estimated item centres when UI Automation is unavailable: Fluent VerticalBox (8px padding/spacing),
# 32px search field and buttons, the two-line item about 48px. Only used together with a check of what was pasted.
function Get-S1ItemPointFallback($Client, [double]$Scale, [int]$Index) {
    $s = if ($Scale -gt 0) { $Scale } else { 1.0 }
    $centres = @(64, 112, 160, 208)
    return @{ X = [int](($Client.L + $Client.R) / 2); Y = [int]($Client.T + $centres[$Index] * $s) }
}

# An empty spot of the panel: between the last item button and the status/footer text.
function Get-S1EmptyPoint($Client, $Elements, [double]$Scale) {
    $s = if ($Scale -gt 0) { $Scale } else { 1.0 }
    $x = [int](($Client.L + $Client.R) / 2)
    $buttons = @($Elements | Where-Object { $_ -and $_.Type -eq 'Button' -and $_.Rect -and ($_.Rect.B - $_.Rect.T) -gt 0 })
    if ($buttons.Count -gt 0) {
        $bottom = ($buttons | ForEach-Object { $_.Rect.B } | Measure-Object -Maximum).Maximum
        $below = @($Elements | Where-Object { $_ -and $_.Rect -and $_.Type -ne 'Button' -and $_.Rect.T -gt $bottom -and ($_.Rect.B - $_.Rect.T) -gt 0 })
        $next = if ($below.Count -gt 0) { ($below | ForEach-Object { $_.Rect.T } | Measure-Object -Minimum).Minimum } else { $Client.B - 60 * $s }
        if (($next - $bottom) -ge 12 * $s) { return @{ X = $x; Y = [int](($bottom + $next) / 2); FromUia = $true } }
    }
    return @{ X = $x; Y = [int]($Client.T + ($Client.B - $Client.T) * 0.70); FromUia = $false }
}

# ---------------------------------------------------------------- S1 verdicts

function Get-AutoFocusCheck($Fg, [long]$ExpectHwnd, [int]$PanelPid, [string]$ExpectName = 'Notepad') {
    if (-not $Fg) { return New-AutoCheck 'focus stayed in the target' $null 'foreground unknown' }
    if ($PanelPid -and $Fg.Pid -eq $PanelPid) { return New-AutoCheck 'focus stayed in the target' $false "the test panel took the foreground ('$($Fg.Title)')" $true }
    if ([long]$Fg.Hwnd -eq $ExpectHwnd) { return New-AutoCheck 'focus stayed in the target' $true "$ExpectName still in front" }
    return New-AutoCheck 'focus stayed in the target' $null "foreground is now '$($Fg.Title)' ($($Fg.Process))"
}

function Get-S1NoActivateChecks($O) {
    $c = @()
    $c += if ($O.LoaderDialog) { New-AutoCheck 'no entry-point error dialog' $false "found '$($O.LoaderDialog)'" $true } else { New-AutoCheck 'no entry-point error dialog' $true }
    $c += New-AutoCheck 'panel window found' ([bool]$O.PanelFound)
    $c += if ($O.PanelFound) { New-AutoCheck 'panel stays on top (WS_EX_TOPMOST)' ([bool]$O.Topmost) '' $true } else { New-AutoCheck 'panel stays on top (WS_EX_TOPMOST)' $null }
    $c += Get-AutoFocusCheck $O.FgAfter $O.TargetHwnd $O.PanelPid
    $c += New-AutoCheck 'target title unchanged' ($O.FgAfter -and $O.FgAfter.Title -eq $O.FgBefore.Title) ''
    $c += if ($null -eq $O.CaretBefore -or $null -eq $O.CaretAfter) { New-AutoCheck 'target caret unchanged' $null 'caret not readable' $false $false } else { New-AutoCheck 'target caret unchanged' ($O.CaretBefore -eq $O.CaretAfter) '' }
    $c += if ($O.TextChanged -eq $true) { New-AutoCheck 'click landed on an empty area' $null 'text was inserted: an item was hit, not an empty area' } else { New-AutoCheck 'click landed on an empty area' $true }
    $c += New-AutoCheck 'extended style' $true ("0x{0:X}" -f [long]$O.ExStyle) $false $false
    return $c
}

function Get-S1PasteChecks($O) {
    $c = @()
    $c += Get-AutoFocusCheck $O.FgAfter $O.TargetHwnd $O.PanelPid
    if ($null -eq $O.Footer) { $c += New-AutoCheck 'footer before click' $null 'not readable via UI Automation' $false $false }
    elseif ($O.Footer -like 'Pastes into Notepad*' -and $O.Footer -notlike '*(no caret seen)*') { $c += New-AutoCheck 'footer before click' $true (Format-AutoShort $O.Footer 80) }
    else { $c += New-AutoCheck 'footer before click' $null ("expected 'Pastes into Notepad.exe ...' without '(no caret seen)', saw " + (Format-AutoShort $O.Footer 80)) }
    $before = ConvertTo-AutoNormText $O.TextBefore; $after = ConvertTo-AutoNormText $O.TextAfter; $item = ConvertTo-AutoNormText $O.Item
    $pasted = ($after -eq ($before + $item))
    if ($null -eq $after) { $c += New-AutoCheck 'text inserted at the caret' $null 'Notepad text not readable' }
    elseif ($pasted) { $c += New-AutoCheck 'text inserted at the caret' $true (Format-AutoShort $after) }
    elseif ($after -eq $before) { $c += New-AutoCheck 'text inserted at the caret' $null 'nothing inserted (the click may have missed the item)' }
    else { $c += New-AutoCheck 'text inserted at the caret' $null "unexpected text $(Format-AutoShort $after)" }
    if ($O.ClipAfter -eq $O.ClipBefore) { $c += New-AutoCheck 'clipboard restored after ~1 s' $true (Format-AutoShort $O.ClipAfter) }
    elseif ($pasted -and $O.ClipAfter -eq $O.Item) { $c += New-AutoCheck 'clipboard restored after ~1 s' $false 'the item is still on the clipboard' $true }
    else { $c += New-AutoCheck 'clipboard restored after ~1 s' $null "clipboard is $(Format-AutoShort $O.ClipAfter)" }
    if (-not $O.HistoryAvailable) { $c += New-AutoCheck 'item kept out of Win+V history' $null "history not readable: $($O.HistoryReason)" $false $false }
    elseif ($O.HistoryAfter -gt $O.HistoryBefore) { $c += New-AutoCheck 'item kept out of Win+V history' $false 'the item was added to clipboard history' $true }
    else { $c += New-AutoCheck 'item kept out of Win+V history' $true }
    if ($O.Status) { $c += New-AutoCheck 'status' ($O.Status -like 'Pasted into*') (Format-AutoShort $O.Status 80) $false $false }
    return $c
}

function Get-S1ExactTextChecks($O) {
    $c = @()
    $c += Get-AutoFocusCheck $O.FgAfter $O.TargetHwnd $O.PanelPid
    $exp = ConvertTo-AutoNormText $O.Expected; $got = ConvertTo-AutoNormText $O.TextAfter
    if ($null -eq $got) { $c += New-AutoCheck 'exact text with line breaks' $null 'Notepad text not readable' }
    elseif ($got -eq $exp) { $c += New-AutoCheck 'exact text with line breaks' $true (Format-AutoShort $got 80) }
    else { $c += New-AutoCheck 'exact text with line breaks' $null "expected $(Format-AutoShort $exp 80), got $(Format-AutoShort $got 80)" }
    return $c
}

function Get-S1ShiftChecks($O) {
    $c = @()
    $c += New-AutoCheck 'Shift was down during the click' ([bool]$O.ModWasDown)
    $c += Get-AutoFocusCheck $O.FgAfter $O.TargetHwnd $O.PanelPid
    $before = ConvertTo-AutoNormText $O.TextBefore; $after = ConvertTo-AutoNormText $O.TextAfterClick; $item = ConvertTo-AutoNormText $O.Item
    if ($null -eq $after) { $c += New-AutoCheck 'nothing inserted on the click' $null 'Notepad text not readable' }
    elseif ($after -eq $before) { $c += New-AutoCheck 'nothing inserted on the click' $true }
    elseif ($O.ModWasDown -and $after.Contains($item)) { $c += New-AutoCheck 'nothing inserted on the click' $false 'the item was pasted although Shift was held' $true }
    else { $c += New-AutoCheck 'nothing inserted on the click' $null "unexpected text $(Format-AutoShort $after)" }
    $c += if ($O.ClipAfter -eq $O.Item) { New-AutoCheck 'item copied to the clipboard' $true } else { New-AutoCheck 'item copied to the clipboard' $null "clipboard is $(Format-AutoShort $O.ClipAfter)" }
    $c += if ((ConvertTo-AutoNormText $O.TextAfterPaste) -eq ($before + $item)) { New-AutoCheck 'Ctrl+V then pastes the item' $true } else { New-AutoCheck 'Ctrl+V then pastes the item' $null "text is $(Format-AutoShort $O.TextAfterPaste)" }
    if ($O.Status) { $c += New-AutoCheck 'status' ($O.Status -like 'Copied (Shift+click)*') (Format-AutoShort $O.Status 60) $false $false }
    return $c
}

# Alt / Win / Ctrl held while clicking: pasted exactly once, no menu, no Start menu, nothing stuck.
function Get-S1ModifierChecks($O) {
    $c = @()
    $m = $O.Modifier
    $c += New-AutoCheck "$m was down during the click" ([bool]$O.ModWasDown)
    if ($O.StartMenu) { $c += New-AutoCheck 'Start menu stayed closed' $false "'$($O.StartMenu)' came to the front" $true }
    else {
        if ($m -eq 'Win') { $c += New-AutoCheck 'Start menu stayed closed' $true }
        $c += Get-AutoFocusCheck $O.FgAfter $O.TargetHwnd $O.PanelPid
    }
    $before = ConvertTo-AutoNormText $O.TextBefore; $after = ConvertTo-AutoNormText $O.TextAfter; $item = ConvertTo-AutoNormText $O.Item
    if ($null -eq $after) { $c += New-AutoCheck 'pasted exactly once' $null 'Notepad text not readable' }
    elseif ($after -eq ($before + $item)) { $c += New-AutoCheck 'pasted exactly once' $true }
    elseif ($after -eq ($before + $item + $item)) { $c += New-AutoCheck 'pasted exactly once' $false 'pasted twice' $true }
    elseif ($after -eq $before) { $c += New-AutoCheck 'pasted exactly once' $null 'nothing inserted (the click may have missed)' }
    else { $c += New-AutoCheck 'pasted exactly once' $null "unexpected text $(Format-AutoShort $after)" }
    $c += if ($O.InMenuMode) { New-AutoCheck 'menu bar not activated' $false 'Notepad is in menu mode' $true } else { New-AutoCheck 'menu bar not activated' $true }
    $stuck = @($O.StuckModifiers)
    $c += if ($stuck.Count -gt 0) { New-AutoCheck 'no stuck modifier keys' $false ("still down: " + ($stuck -join ', ')) $true } else { New-AutoCheck 'no stuck modifier keys' $true }
    $c += if ($O.ProbeOk -eq $true) { New-AutoCheck 'typing works normally afterwards' $true } else { New-AutoCheck 'typing works normally afterwards' $null 'a typed x did not appear (access-key tips or a modifier held?)' }
    return $c
}

function Get-S1ElevatedChecks($O) {
    $c = @()
    $c += Get-AutoFocusCheck $O.FgAfter $O.TargetHwnd $O.PanelPid 'the admin Notepad'
    if ($O.ClipAfter -eq $O.Item) { $c += New-AutoCheck 'item copied, not pasted' $true }
    elseif ($O.SeqAfter -eq $O.SeqBefore) { $c += New-AutoCheck 'item copied, not pasted' $null 'clipboard untouched: the click may have missed the item' }
    elseif ($O.ClipAfter -eq $O.ClipBefore -and ($O.SeqAfter - $O.SeqBefore) -ge 2) { $c += New-AutoCheck 'item copied, not pasted' $false 'the panel tried to paste (clipboard was swapped and restored)' $true }
    else { $c += New-AutoCheck 'item copied, not pasted' $null "clipboard is $(Format-AutoShort $O.ClipAfter)" }
    if ($null -eq $O.Footer) { $c += New-AutoCheck 'footer names the admin fallback' $null 'not readable via UI Automation' $false $false }
    elseif ($O.Footer -like 'Copies to clipboard*runs as admin*') { $c += New-AutoCheck 'footer names the admin fallback' $true (Format-AutoShort $O.Footer 90) }
    elseif ($O.Footer -like 'Pastes into*') { $c += New-AutoCheck 'footer names the admin fallback' $false (Format-AutoShort $O.Footer 90) $true }
    else { $c += New-AutoCheck 'footer names the admin fallback' $null (Format-AutoShort $O.Footer 90) }
    if ($null -eq $O.TextChanged) { $c += New-AutoCheck 'nothing injected' $null 'admin window text not readable' $false $false }
    elseif ($O.TextChanged) { $c += New-AutoCheck 'nothing injected' $false 'text appeared in the admin Notepad' $true }
    else { $c += New-AutoCheck 'nothing injected' $true }
    if ($O.Status) { $c += New-AutoCheck 'status' ($O.Status -like 'Copied*') (Format-AutoShort $O.Status 80) $false $false }
    return $c
}

# ---------------------------------------------------------------- S4 verdicts

function Get-S4DiagCheck($Diag) {
    if (-not $Diag) { return New-AutoCheck 'diagnostics line' $null 'no [s4] line in the app output' }
    return New-AutoCheck 'diagnostics line' $true ("chrome $($Diag.Chrome), theme $($Diag.Theme), build $($Diag.Build)") $false $false
}

function Get-S4PixelBlackCheck($M, $Colors, [string]$Theme) {
    # Black chrome is only trusted when the card zone next to it shows the expected opaque colour.
    $cardOk = $M.Card -and ((Measure-AutoColorDistance $M.Card.Mean $Colors.$Theme.BgApp) -le 6)
    if ($M.Class.Class -eq 'black' -and $cardOk) { return New-AutoCheck 'chrome not black' $false $M.Class.Detail $true }
    if ($M.Class.Class -eq 'black') { return New-AutoCheck 'chrome not black' $null "capture looks wrong: $($M.Class.Detail)" }
    return $null
}

function Get-S4MicaChecks($O) {
    $c = @()
    $d = $O.Diag
    $c += Get-S4DiagCheck $d
    if ($d) {
        if ($d.Chrome -eq 'Backdrop(Mica)') { $c += New-AutoCheck 'chrome: Backdrop(Mica)' $true }
        elseif ($d.Chrome -eq 'Solid(BackdropCallFailed)') { $c += New-AutoCheck 'chrome: Backdrop(Mica)' $false 'Solid(BackdropCallFailed)' $true }
        else { $c += New-AutoCheck 'chrome: Backdrop(Mica)' $null "$($d.Chrome) (a system setting decided this, check Transparency effects / contrast)" }
        $keys = @('dark', 'corners', 'border', 'extend', 'backdrop')
        if (Test-S4DwmOk $d.Dwm $keys) { $c += New-AutoCheck 'DWM calls ok' $true }
        elseif ($d.Dwm.Count -gt 0) { $c += New-AutoCheck 'DWM calls ok' $false ((($keys | ForEach-Object { "$_=$($d.Dwm[$_])" }) -join ' ')) $true }
        else { $c += New-AutoCheck 'DWM calls ok' $null 'no DWM report' }
    }
    $c += New-AutoCheck 'window active' ([bool]$O.Active) $(if ($O.Active) { '' } else { 'could not activate the window (inactive Mica is flat)' })
    $black = Get-S4PixelBlackCheck $O.Measure $O.Colors $O.Theme
    if ($black) { $c += $black }
    else {
        $k = $O.Measure.Class.Class
        $ok = if ($k -eq 'mica') { $true } else { $null }
        $c += New-AutoCheck "chrome pixels look like Mica ($k)" $ok $O.Measure.Class.Detail
    }
    return $c
}

function Get-S4SolidChecks($O) {
    $c = @()
    $d = $O.Diag
    $c += Get-S4DiagCheck $d
    if ($d) {
        $want = "Solid($($O.Reason))"
        $c += if ($d.Chrome -eq $want) { New-AutoCheck "chrome: $want" $true } else { New-AutoCheck "chrome: $want" $null "diagnostics say $($d.Chrome)" }
        if ($O.ExpectBackdropNone) {
            $c += if ($d.Dwm['backdrop'] -eq '1:ok') { New-AutoCheck 'backdrop=1:ok' $true } else { New-AutoCheck 'backdrop=1:ok' $false "backdrop=$($d.Dwm['backdrop'])" $true }
        }
    }
    $black = Get-S4PixelBlackCheck $O.Measure $O.Colors $O.Theme
    if ($black) { $c += $black }
    else {
        $m = $O.Measure
        $bg = $O.Colors.($O.Theme).BgApp
        $solidOk = ($m.Class.Class -eq 'solid') -and $m.Chrome -and ((Measure-AutoColorDistance $m.Chrome.Mean $bg) -le 6)
        $c += New-AutoCheck 'chrome painted solid bg.app (same as the card zone)' $(if ($solidOk) { $true } else { $null }) $m.Class.Detail
    }
    return $c
}

# Dark/light flip through the registry: the app must notice, repaint, and come back.
function Get-S4ThemeFlipChecks($O) {
    $c = @()
    $c += if ($O.DiagFlip -and $O.DiagFlip.Theme -eq $O.To) { New-AutoCheck "app switched to $($O.To)" $true "trigger: $($O.DiagFlip.Reason)" } else { New-AutoCheck "app switched to $($O.To)" $null 'no refresh with the new theme within 5 s' }
    $seen = Get-AutoThemeOfColor $O.CardFlip $O.Colors
    if ($O.DiagFlip -and $O.DiagFlip.Theme -eq $O.To) {
        if ($seen -eq $O.To) { $c += New-AutoCheck 'card zone repainted' $true (Format-AutoRgb $O.CardFlip) }
        elseif ($seen -eq $O.From) { $c += New-AutoCheck 'card zone repainted' $false "still $($O.From): $(Format-AutoRgb $O.CardFlip)" $true }
        else { $c += New-AutoCheck 'card zone repainted' $null (Format-AutoRgb $O.CardFlip) }
        if ($O.RequireDarkOk) {
            $c += if ($O.DiagFlip.Dwm['dark'] -eq 'ok') { New-AutoCheck 'Mica dark flag (dark=ok)' $true } else { New-AutoCheck 'Mica dark flag (dark=ok)' $false "dark=$($O.DiagFlip.Dwm['dark'])" $true }
        }
    }
    $back = Get-AutoThemeOfColor $O.CardBack $O.Colors
    $c += if ($O.DiagBack -and $O.DiagBack.Theme -eq $O.From -and $back -eq $O.From) { New-AutoCheck 'reverses cleanly' $true } else { New-AutoCheck 'reverses cleanly' $null "after restore: theme $($O.DiagBack.Theme), cards $(Format-AutoRgb $O.CardBack)" }
    return $c
}

function Get-S4TransparencyChecks($O) {
    $c = @()
    $off = $O.DiagOff
    if (-not $off -or $off.Transparency -ne 'off') {
        $c += New-AutoCheck 'app saw Transparency effects off' $null ("registry EnableTransparency=$($off.Sources['EnableTransparency']) but UISettings AdvancedEffects=$($off.Sources['AdvancedEffects']): the registry write did not reach UISettings")
        return $c
    }
    $c += New-AutoCheck 'app saw Transparency effects off' $true "trigger: $($off.Reason)"
    $c += if ($off.Chrome -eq 'Solid(TransparencyOff)' -and $off.Dwm['backdrop'] -eq '1:ok') { New-AutoCheck 'off: Solid(TransparencyOff), backdrop=1:ok' $true }
          elseif ($off.Chrome -like 'Backdrop*') { New-AutoCheck 'off: Solid(TransparencyOff), backdrop=1:ok' $false "chrome $($off.Chrome)" $true }
          else { New-AutoCheck 'off: Solid(TransparencyOff), backdrop=1:ok' $null "chrome $($off.Chrome) backdrop=$($off.Dwm['backdrop'])" }
    $black = Get-S4PixelBlackCheck $O.MeasureOff $O.Colors $O.Theme
    if ($black) { $c += $black }
    else { $c += New-AutoCheck 'off: chrome matches the card zone' $(if ($O.MeasureOff.Class.Class -eq 'solid') { $true } else { $null }) $O.MeasureOff.Class.Detail }
    $c += New-AutoCheck 'off: hairlines still separate the zones' $(if ($null -ne $O.MeasureOff.Hairline) { $true } else { $null }) $(if ($null -ne $O.MeasureOff.Hairline) { "row $($O.MeasureOff.Hairline)" } else { 'hairline not found' })
    $on = $O.DiagOn
    $c += if ($on -and $on.Chrome -eq 'Backdrop(Mica)') { New-AutoCheck 'on: Mica returns' $true } else { New-AutoCheck 'on: Mica returns' $null "chrome $($on.Chrome)" }
    $c += New-AutoCheck 'on: chrome no longer solid' $(if ($O.MeasureOn.Class.Class -eq 'mica') { $true } else { $null }) $O.MeasureOn.Class.Detail
    return $c
}

# $O.Steps: @(@{ Percent; Diag; Hairline }) in increasing order; $O.Base / $O.Back: the same before / after.
function Get-S4TextScaleChecks($O) {
    $c = @()
    $prev = $O.Base.Hairline
    foreach ($s in $O.Steps) {
        $want = $s.Percent / 100.0
        $label = 'text scale {0:0.00}' -f $want
        $label = $label.Replace(',', '.')
        $d = $s.Diag
        if ($d -and $null -ne $d.TextScale -and [Math]::Abs($d.TextScale - $want) -lt 0.006) { $c += New-AutoCheck $label $true "trigger: $($d.Reason)" }
        elseif ($d -and $d.Sources['TextScaleFactor'] -eq "Some($($s.Percent))") { $c += New-AutoCheck $label $null "registry TextScaleFactor=$($s.Percent) but UISettings TextScale=$($d.Sources['TextScale']): the registry write did not reach UISettings" }
        else { $c += New-AutoCheck $label $null "diagnostics: text scale $($d.TextScale)" }
        if ($null -ne $s.Hairline -and $null -ne $prev -and $s.Hairline -gt $prev) { $c += New-AutoCheck "chrome grew at $($s.Percent)%" $true "tab bar edge row $prev -> $($s.Hairline)" }
        else { $c += New-AutoCheck "chrome grew at $($s.Percent)%" $null "tab bar edge row $prev -> $($s.Hairline)" }
        $prev = $s.Hairline
    }
    $bd = $O.Back.Diag
    $c += if ($bd -and $O.Base.Diag -and [Math]::Abs($bd.TextScale - $O.Base.Diag.TextScale) -lt 0.006) { New-AutoCheck 'restored' $true } else { New-AutoCheck 'restored' $null "text scale after restore: $($bd.TextScale)" }
    return $c
}

function Get-S4LoaderChecks($O) {
    $c = @()
    $c += if ($O.LoaderDialog) { New-AutoCheck "no 'entry point not found' dialog" $false "found '$($O.LoaderDialog)'" $true } else { New-AutoCheck "no 'entry point not found' dialog" $true }
    $c += New-AutoCheck 'app still running after 3 s' ([bool]$O.Running)
    $c += New-AutoCheck 'window shown' ([bool]$O.WindowFound)
    return $c
}

# ---------------------------------------------------------------- S2 verdicts

function Get-S2FontChecks($R, [string]$Mode) {
    $c = @()
    $any = ($R.SystemFallback.Count + $R.Found.Count + $R.Override.Count) -gt 0
    $c += New-AutoCheck 'start-up font report captured' $(if ($any) { $true } else { $null })
    if (-not $any) { return $c }
    $hani = if ($R.SystemFallback.ContainsKey('Hani')) { $R.SystemFallback['Hani'] } else { '?' }
    if ($Mode -eq 'system') {
        $c += New-AutoCheck 'fallback left to DirectWrite' ([bool]$R.SystemMode)
        $c += New-AutoCheck 'DirectWrite picks for Han' $true $hani $false $false
        return $c
    }
    $first = if ($Mode -eq 'zh') { 'Microsoft YaHei UI' } else { 'Yu Gothic UI' }
    if ($R.Found.ContainsKey($first) -and -not $R.Found[$first]) { $c += New-AutoCheck "$first installed" $null 'not installed on this PC' }
    $han = @($R.Override['Hani'])
    $c += if ($han.Count -gt 0 -and $han[0] -eq $first) { New-AutoCheck "Han fallback starts with $first" $true ($han -join ', ') } else { New-AutoCheck "Han fallback starts with $first" $null ("override Hani: " + ($han -join ', ')) }
    $kana = @($R.Override['Kana'])
    $c += if ($kana.Count -gt 0 -and $kana[0] -eq 'Yu Gothic UI') { New-AutoCheck 'Kana fallback Yu Gothic UI' $true } else { New-AutoCheck 'Kana fallback Yu Gothic UI' $null ($kana -join ', ') }
    $hang = @($R.Override['Hang'])
    $c += if ($hang.Count -gt 0 -and $hang[0] -eq 'Malgun Gothic') { New-AutoCheck 'Hangul fallback Malgun Gothic' $true } else { New-AutoCheck 'Hangul fallback Malgun Gothic' $null ($hang -join ', ') }
    $c += New-AutoCheck 'system fallback before override (Han)' $true $hani $false $false
    return $c
}
