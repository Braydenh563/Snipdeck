<#
  S4 automations (Mica chrome, theme, transparency, text size). Evidence comes from the app's own
  "[s4] last refresh: ..." diagnostics (stderr, captured by the wizard) and from screen pixels of the window:
  the chrome zone (header) against the card zone (bg.app from design/tokens/palette.json).
  System settings are flipped through HKCU registry values + a WM_SETTINGCHANGE broadcast, and are
  ALWAYS put back (Restore-AutoRegistry in the runner's finally, and auto-restore.json after a crash).
  Not automated: high contrast (8), DPI / monitors (9), drag/resize, inactive look, the optional matrix
  and eyeball steps (11-16), and every check of corners/shadow/border (asked as a one-key question).
#>

$script:AutoS4Colors = $null

function Get-S4Colors($Ctx) {
    if (-not $script:AutoS4Colors) {
        $path = Join-Path (Join-Path (Join-Path $Ctx.RepoRoot 'design') 'tokens') 'palette.json'
        $script:AutoS4Colors = Get-AutoPaletteColors (Get-Content -Raw -Path $path | ConvertFrom-Json)
    }
    return $script:AutoS4Colors
}

# Newest diagnostics once there are more than $After of them and the newest matches every key in $Where.
function Wait-S4Diag($Ctx, [int]$After = 0, [hashtable]$Where = @{}, [int]$TimeoutMs = 10000) {
    $n = [Math]::Max(1, [int]($TimeoutMs / 200))
    for ($i = 0; $i -lt $n; $i++) {
        $list = Get-S4DiagList (Get-AutoAppLines $Ctx)
        if ($list.Count -gt $After) {
            $d = $list[$list.Count - 1]
            $ok = $true
            foreach ($k in $Where.Keys) { if ($d[$k] -ne $Where[$k]) { $ok = $false } }
            if ($ok) { return @{ Diag = $d; Count = $list.Count; TimedOut = $false } }
        }
        Assert-AutoContinue
        Invoke-AutoSleep 200
    }
    $list = Get-S4DiagList (Get-AutoAppLines $Ctx)
    $last = $null
    if ($list.Count -gt 0) { $last = $list[$list.Count - 1] }
    return @{ Diag = $last; Count = $list.Count; TimedOut = $true }
}

function Get-S4Window($Ctx) {
    $w = Wait-AutoWindow -ProcessId $Ctx.App.proc.Id -TitleLike 'Snipdeck S4 spike' -TimeoutMs 10000
    if (-not $w) { throw 'the S4 window was not found' }
    return $w
}

# Makes the window fully visible and active (a click on the card zone, which has no click action),
# then parks the pointer outside it so no hover colour is captured.
function Show-S4Window($W, [double]$TextScale) {
    $rect = Get-AutoFrameRect $W.Hwnd
    $vs = Get-AutoVirtualScreen
    if ($rect.L -lt $vs.L -or $rect.T -lt $vs.T -or $rect.R -gt $vs.R -or $rect.B -gt $vs.B) {
        $work = Get-AutoWorkArea $W.Hwnd
        $wd = $rect.R - $rect.L; $ht = [Math]::Min($rect.B - $rect.T, $work.B - $work.T - 80)
        Set-AutoWindowRect $W.Hwnd @{ L = $work.L + 40; T = $work.T + 40; R = $work.L + 40 + $wd; B = $work.T + 40 + $ht }
        Wait-Auto 500
        $rect = Get-AutoFrameRect $W.Hwnd
    }
    Set-AutoWindowRect $W.Hwnd $null   # lift it above other windows (e.g. the S1 scratch Notepad) so the click lands on it
    Wait-Auto 200
    $z = Get-S4Zones ($rect.R - $rect.L) ($rect.B - $rect.T) (Get-AutoDpiScale $W.Hwnd) $TextScale
    $null = Invoke-AutoClick ($rect.L + $z.CardClick.X) ($rect.T + $z.CardClick.Y)
    $px = if ($rect.L - 40 -gt $vs.L) { $rect.L - 40 } else { $rect.R + 40 }
    Move-AutoMouse $px ($rect.T + 20)
    Wait-Auto 700
    return $rect
}

function Get-S4Observation($Ctx, [string]$Tag, [switch]$Activate) {
    $wd = Wait-S4Diag $Ctx 0 @{} 10000
    $d = $wd.Diag
    $w = Get-S4Window $Ctx
    $ts = 1.0; if ($d -and $d.TextScale) { $ts = $d.TextScale }
    $rect = if ($Activate) { Show-S4Window $w $ts } else { Get-AutoFrameRect $w.Hwnd }
    $fg = Get-AutoForeground
    $scale = if ($d -and $d.Dpi) { $d.Dpi } else { Get-AutoDpiScale $w.Hwnd }
    $theme = if ($d -and $d.Theme) { $d.Theme } else { 'light' }
    $shot = Join-Path (Join-Path $Ctx.RunDir 'screens') ('{0}-{1}-auto-{2}.png' -f $Ctx.SpikeId, $Ctx.TestId, $Tag)
    $img = Get-AutoScreenPixels $rect $shot
    $colors = Get-S4Colors $Ctx
    $m = Measure-S4Image $img $scale $ts $colors.$theme
    return @{ Diag = $d; Count = $wd.Count; Window = $w; Active = ($fg -and [long]$fg.Hwnd -eq [long]$w.Hwnd)
        Measure = $m; Shot = $shot; Theme = $theme; Colors = $colors }
}

# ---------------------------------------------------------------- steps 1 / 2: Mica

function Invoke-S4Mica($Ctx) {
    Enter-AutoHandsOff 'S4: activate the window, read the diagnostics, sample the chrome and card pixels'
    $o = Get-S4Observation $Ctx 'mica' -Activate
    return Resolve-AutoChecks (Get-S4MicaChecks $o) @($o.Shot) 'Click the S4 window once (so it is active) and look: header, tab bar and footer show a subtle wallpaper tint, the corners are rounded (8px) with a shadow and a thin grey border, and there is NO dark 1px line along the top edge.'
}

# ---------------------------------------------------------------- step 17 / W1 / W1b: solid chrome

function Invoke-S4ForcedSolid($Ctx) {
    Enter-AutoHandsOff 'S4: forced solid chrome: read the diagnostics and sample the chrome pixels'
    $o = Get-S4Observation $Ctx 'solid' -Activate
    $o.Reason = 'ForcedByConfig'; $o.ExpectBackdropNone = $true
    return Resolve-AutoChecks (Get-S4SolidChecks $o) @($o.Shot) 'Look at the S4 window: are the rounded corners, the shadow and the thin border still there? (The screenshot is saved for the DESIGN.md comparison.)'
}

function Invoke-S4Win10Solid($Ctx) {
    $d = (Wait-S4Diag $Ctx 0 @{} 10000).Diag
    if ($d -and $d.Build -ge 22621) { return New-AutoResult 'unsure' "this PC is build $($d.Build) (Mica-capable): the W steps are meant for Windows 10" }
    Enter-AutoHandsOff 'S4 (Windows 10): read the diagnostics and sample the chrome pixels'
    $o = Get-S4Observation $Ctx 'win10' -Activate
    $o.Reason = 'UnsupportedOs'; $o.ExpectBackdropNone = $false
    return Resolve-AutoChecks (Get-S4SolidChecks $o) @($o.Shot) 'Look at the S4 window: SQUARE corners, a shadow, a thin 1px border drawn around it, and no black areas?'
}

# ---------------------------------------------------------------- step 6 / W3: dark / light

function Invoke-S4ThemeSwitch($Ctx) { return Invoke-S4ThemeFlip $Ctx $true }
function Invoke-S4ThemeSwitchWin10($Ctx) { return Invoke-S4ThemeFlip $Ctx $false }

function Invoke-S4ThemeFlip($Ctx, [bool]$RequireDarkOk) {
    Enter-AutoHandsOff 'S4: switch the Windows app theme through the registry, then back (always restored)'
    $o0 = Get-S4Observation $Ctx 'theme0' -Activate
    $from = $o0.Theme
    $to = if ($from -eq 'dark') { 'light' } else { 'dark' }
    $val = if ($to -eq 'light') { 1 } else { 0 }
    Write-AutoLog "Windows theme -> $to"
    Set-AutoRegistryDword 'Personalize' 'AppsUseLightTheme' $val
    Set-AutoRegistryDword 'Personalize' 'SystemUsesLightTheme' $val
    Send-AutoSettingChange 'ImmersiveColorSet'
    $w1 = Wait-S4Diag $Ctx $o0.Count @{ Theme = $to } 6000
    Wait-Auto 800
    $o1 = Get-S4Observation $Ctx 'theme1'
    if ($w1.Diag -and $w1.Diag.Theme -eq $to -and (Get-AutoThemeOfColor $o1.Measure.Card.Mean $o1.Colors) -ne $to) {
        Wait-Auto 1500   # give a slow repaint a second chance before calling it a failure
        $o1 = Get-S4Observation $Ctx 'theme1b'
    }
    Write-AutoLog "Windows theme -> $from (restoring)"
    Restore-AutoRegistry
    $w2 = Wait-S4Diag $Ctx $w1.Count @{ Theme = $from } 6000
    Wait-Auto 800
    $o2 = Get-S4Observation $Ctx 'theme2'
    $o = @{ From = $from; To = $to; DiagFlip = $w1.Diag; DiagBack = $w2.Diag; CardFlip = $o1.Measure.Card.Mean; CardBack = $o2.Measure.Card.Mean
        Colors = $o0.Colors; RequireDarkOk = $RequireDarkOk }
    $r = Resolve-AutoChecks (Get-S4ThemeFlipChecks $o) @($o0.Shot, $o1.Shot, $o2.Shot)
    $r.evidence += '; note: driven by registry + WM_SETTINGCHANGE, not the Settings app'
    return $r
}

# ---------------------------------------------------------------- step 7: transparency effects

function Invoke-S4Transparency($Ctx) {
    Enter-AutoHandsOff 'S4: turn Transparency effects off, then on, through the registry (always restored)'
    $o0 = Get-S4Observation $Ctx 'transp0' -Activate
    Write-AutoLog 'Transparency effects -> off'
    Set-AutoRegistryDword 'Personalize' 'EnableTransparency' 0
    Send-AutoSettingChange 'ImmersiveColorSet'
    $w1 = Wait-S4Diag $Ctx $o0.Count @{ Transparency = 'off' } 6000
    Wait-Auto 800
    $o1 = Get-S4Observation $Ctx 'transp-off' -Activate
    Write-AutoLog 'Transparency effects -> on'
    Set-AutoRegistryDword 'Personalize' 'EnableTransparency' 1
    Send-AutoSettingChange 'ImmersiveColorSet'
    $w2 = Wait-S4Diag $Ctx $w1.Count @{ Transparency = 'on' } 6000
    Wait-Auto 800
    $o2 = Get-S4Observation $Ctx 'transp-on' -Activate
    Restore-AutoRegistry
    $o = @{ DiagOff = $w1.Diag; DiagOn = $w2.Diag; MeasureOff = $o1.Measure; MeasureOn = $o2.Measure; Colors = $o0.Colors; Theme = $o1.Theme }
    $r = Resolve-AutoChecks (Get-S4TransparencyChecks $o) @($o1.Shot, $o2.Shot)
    $r.evidence += '; note: driven by registry + WM_SETTINGCHANGE, not the Settings app'
    return $r
}

# ---------------------------------------------------------------- step 10 / W4: text size

function Invoke-S4TextScale($Ctx) { return Invoke-S4TextScaleRun $Ctx @(150, 225) }
function Invoke-S4TextScaleWin10($Ctx) { return Invoke-S4TextScaleRun $Ctx @(150) }

function Invoke-S4TextScaleRun($Ctx, [int[]]$Percents) {
    Enter-AutoHandsOff ("S4: set the Windows text size to {0}% through the registry, then back (always restored)" -f ($Percents -join '%, then '))
    $o0 = Get-S4Observation $Ctx 'text100' -Activate
    $steps = @()
    $count = $o0.Count
    $shots = @($o0.Shot)
    foreach ($p in $Percents) {
        Write-AutoLog "text size -> $p%"
        Set-AutoRegistryDword 'Accessibility' 'TextScaleFactor' $p
        Send-AutoSettingChange ''
        $w = Wait-S4Diag $Ctx $count @{ TextScale = ($p / 100.0) } 6000
        Wait-Auto 800
        $o = Get-S4Observation $Ctx "text$p"
        $steps += @{ Percent = $p; Diag = $w.Diag; Hairline = $o.Measure.Hairline }
        $shots += $o.Shot
        $count = $w.Count
    }
    Write-AutoLog 'text size -> back to the original'
    Restore-AutoRegistry
    $wb = Wait-S4Diag $Ctx $count @{ TextScale = $o0.Diag.TextScale } 6000
    $o = @{ Base = @{ Diag = $o0.Diag; Hairline = $o0.Measure.Hairline }; Steps = $steps; Back = @{ Diag = $wb.Diag } }
    $r = Resolve-AutoChecks (Get-S4TextScaleChecks $o) $shots
    $r.evidence += '; note: driven by registry + WM_SETTINGCHANGE: which signal the real Settings app sends is NOT recorded'
    return $r
}

# ---------------------------------------------------------------- W7: no loader error at start-up

function Invoke-S4NoLoaderError($Ctx) {
    Write-AutoLog 'relaunching the app and watching for a loader error dialog'
    Restart-AutoApp $Ctx
    Wait-Auto 3000
    $o = @{
        LoaderDialog = (Find-AutoLoaderDialog $Ctx.Spike.exe)
        Running      = (Test-AutoAppRunning $Ctx)
        WindowFound  = (@(Get-AutoWindows -ProcessId $Ctx.App.proc.Id -TitleLike 'Snipdeck S4 spike').Count -gt 0)
    }
    return Resolve-AutoChecks (Get-S4LoaderChecks $o)
}
