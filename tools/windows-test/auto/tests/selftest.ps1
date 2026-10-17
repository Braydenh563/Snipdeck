<#
  Unit tests for the automation layer, run by `run-tests.ps1 -SelfTest` after loading auto/*.ps1 and the
  mock primitives. Returns a list of problems (empty = OK). No Windows, no real input.
#>

function Get-AutoFixtureS4Lines {
    $dot = [char]0xB7
    # Format from spikes/s4-mica/src/main.rs `refresh` (Windows 11 23H2, light, defaults).
    return @(
        '[s4] subclass=true UISettings events registered',
        "[s4] last refresh: window created | OS build 22631 $dot theme light $dot transparency on $dot high contrast off | DPI scale 1.50 $dot text scale 1.00 | chrome: Backdrop(Mica) | DWM: dark=ok corners=ok border=ok extend=Full:ok backdrop=2:ok | renderer: default (winit-femtovg) | config: Config { backdrop: Mica, extend_frame: Full, undecorated_shadow: true, no_activate: false, keep_active_look: false, force_solid: false } | sources: reg AppsUseLightTheme=Some(1) EnableTransparency=Some(1) TextScaleFactor=None; UISettings AdvancedEffects=Some(true) TextScale=Some(1.0); last WM_SETTINGCHANGE=`"`"",
        "[s4] last refresh: WM_SETTINGCHANGE | OS build 22631 $dot theme dark $dot transparency off $dot high contrast off | DPI scale 1.00 $dot text scale 2.25 | chrome: Solid(TransparencyOff) | DWM: dark=ok corners=ok border=ok extend=Full:ok backdrop=1:ok | renderer: winit-software | config: Config { } | sources: reg AppsUseLightTheme=Some(0) EnableTransparency=Some(0) TextScaleFactor=Some(225); UISettings AdvancedEffects=Some(false) TextScale=Some(2.25); last WM_SETTINGCHANGE=`"ImmersiveColorSet`"",
        "[s4] last refresh: window created | OS build 19045 $dot theme light $dot transparency on $dot high contrast off | DPI scale 1.00 $dot text scale 1.00 | chrome: Solid(UnsupportedOs) | DWM: dark=0x80070057 extend=Full:ok | renderer: default (winit-femtovg) | config: Config { } | sources: reg AppsUseLightTheme=None EnableTransparency=None TextScaleFactor=None; UISettings AdvancedEffects=None TextScale=None; last WM_SETTINGCHANGE=`"`""
    )
}

function New-AutoTestCtx([string]$Spike, [string]$TestId, $Launch = $null, [string]$OsClass = 'win11') {
    $norm = Normalize-Launch $Launch
    $proc = [pscustomobject]@{ HasExited = $false; Id = 4242; ExitCode = 0 }
    $spikeDef = [pscustomobject]@{ id = $Spike; exe = @{ S1 = 's1-noactivate'; S2 = 's2-ime'; S4 = 's4-mica' }[$Spike] }
    return [pscustomobject]@{
        SpikeId = $Spike; TestId = $TestId; Spike = $spikeDef; Test = $null
        App = @{ proc = $proc; norm = $norm; sig = (Get-Sig $norm); logOut = ''; logErr = '' }
        RunDir = [IO.Path]::GetTempPath(); RepoRoot = $RepoRoot; OsBuild = 22631; OsClass = $OsClass
    }
}

function Invoke-AutoSelfTest {
    $p = New-Object System.Collections.ArrayList
    function Expect([bool]$Cond, [string]$What) { if (-not $Cond) { [void]$p.Add("auto: $What") } }

    $script:AutoState.Quiet = $true
    $colorsJson = Get-Content -Raw -Path (Join-Path (Join-Path (Join-Path $RepoRoot 'design') 'tokens') 'palette.json') | ConvertFrom-Json
    $colors = Get-AutoPaletteColors $colorsJson

    # ---- S4 diagnostics parser
    $lines = Get-AutoFixtureS4Lines
    $list = Get-S4DiagList $lines
    Expect ($list.Count -eq 3) "S4 parser: expected 3 refresh lines, got $($list.Count)"
    $d = $list[0]
    Expect ($d.Reason -eq 'window created' -and $d.Build -eq 22631 -and $d.Theme -eq 'light' -and $d.Transparency -eq 'on') 'S4 parser: header fields'
    Expect ($d.Dpi -eq 1.5 -and $d.TextScale -eq 1.0) "S4 parser: DPI/text scale ($($d.Dpi)/$($d.TextScale))"
    Expect ($d.Chrome -eq 'Backdrop(Mica)' -and $d.Dwm['backdrop'] -eq '2:ok' -and $d.Dwm['extend'] -eq 'Full:ok') 'S4 parser: chrome / DWM'
    Expect (Test-S4DwmOk $d.Dwm @('dark', 'corners', 'border', 'extend', 'backdrop')) 'S4 parser: DWM all ok'
    Expect ($d.Renderer -eq 'default (winit-femtovg)' -and $d.Sources['EnableTransparency'] -eq 'Some(1)' -and $d.Sources['TextScale'] -eq 'Some(1.0)') 'S4 parser: renderer / sources'
    Expect ($list[1].TextScale -eq 2.25 -and $list[1].Theme -eq 'dark' -and $list[1].Chrome -eq 'Solid(TransparencyOff)' -and $list[1].Sources['TextScaleFactor'] -eq 'Some(225)') 'S4 parser: second line'
    Expect ($list[2].Dwm['dark'] -eq '0x80070057' -and -not $list[2].Dwm.ContainsKey('backdrop') -and -not (Test-S4DwmOk $list[2].Dwm @('dark'))) 'S4 parser: Windows 10 line'
    Expect ($null -eq (ConvertFrom-S4Diag 'random output')) 'S4 parser: ignores other lines'

    # ---- S2 font report parser
    $r2 = ConvertFrom-S2FontReport (New-MockS2Lines @('--cjk', 'ja'))
    Expect ($r2.SystemFallback['Hani'] -eq 'Microsoft YaHei UI' -and $r2.Found['Yu Gothic UI'] -eq $true -and $r2.Found['IBM Plex Sans'] -eq $false) 'S2 parser: fallback / found'
    Expect ($r2.Override['Hani'][0] -eq 'Yu Gothic UI' -and @($r2.Override['Hani']).Count -eq 4 -and $r2.Effective['Hani'] -eq 'Yu Gothic UI') 'S2 parser: override chain'
    Expect ((Resolve-AutoChecks (Get-S2FontChecks $r2 'ja')).status -eq 'pass') 'S2 verdict: ja chain passes'
    Expect ((Resolve-AutoChecks (Get-S2FontChecks $r2 'zh')).status -eq 'unsure') 'S2 verdict: ja chain is not a zh pass'
    Expect ((Resolve-AutoChecks (Get-S2FontChecks (ConvertFrom-S2FontReport @()) 'ja')).status -eq 'unsure') 'S2 verdict: no output -> unsure'
    $rs = ConvertFrom-S2FontReport (New-MockS2Lines @('--cjk', 'system'))
    Expect ($rs.SystemMode -and (Resolve-AutoChecks (Get-S2FontChecks $rs 'system')).status -eq 'pass') 'S2 verdict: system mode records the pick'

    # ---- pixel classifier (synthetic images)
    $mk = { param($c) $cc = $c; @{ W = 40; H = 40; Fn = { param($x, $y) $cc }.GetNewClosure() } }
    $region = @{ L = 0; T = 0; R = 40; B = 40 }
    $card = Get-AutoRegionStats (& $mk @(245, 243, 241)) $region
    $cls = { param($c) (Get-AutoChromeClass (Get-AutoRegionStats (& $mk $c) $region) $card $colors.light.BgApp).Class }
    Expect ((& $cls @(226, 236, 247)) -eq 'mica') 'classifier: tinted chrome -> mica'
    Expect ((& $cls @(245, 243, 241)) -eq 'solid') 'classifier: bg.app chrome -> solid'
    Expect ((& $cls @(0, 0, 0)) -eq 'black') 'classifier: black chrome -> black'
    Expect ((& $cls @(255, 255, 255)) -eq 'white') 'classifier: white chrome -> white'
    Expect ((& $cls @(232, 232, 232)) -eq 'grey') 'classifier: neutral grey -> grey (cannot tell)'
    $noisy = @{ W = 40; H = 40; Fn = { param($x, $y) if (($x % 4) -lt 2) { @(0, 0, 0) } else { @(255, 255, 255) } } }
    Expect ((Get-AutoChromeClass (Get-AutoRegionStats $noisy $region) $card $colors.light.BgApp).Class -eq 'mixed') 'classifier: text in the sample -> mixed'
    $darkCard = Get-AutoRegionStats (& $mk @(17, 15, 13)) $region
    Expect ((Get-AutoChromeClass (Get-AutoRegionStats (& $mk @(17, 15, 13)) $region) $darkCard $colors.dark.BgApp).Class -eq 'solid') 'classifier: dark bg.app is solid, not black'
    Expect ((Get-AutoThemeOfColor @(17, 15, 13) $colors) -eq 'dark' -and (Get-AutoThemeOfColor @(245, 243, 241) $colors) -eq 'light' -and $null -eq (Get-AutoThemeOfColor @(120, 0, 0) $colors)) 'theme of colour'
    $argb = @{ W = 2; H = 1; Px = [int[]]@(((0xFF -shl 24) -bor (245 -shl 16) -bor (243 -shl 8) -bor 241), (0xFF -shl 24)) }
    $px0 = Get-AutoPixel $argb 0 0
    Expect ($px0[0] -eq 245 -and $px0[1] -eq 243 -and $px0[2] -eq 241) 'ARGB pixel decode'

    # ---- S4 zones + hairline on the mock renderer at three text scales
    foreach ($ts in 1.0, 1.5, 2.25) {
        $m = Get-S4Metrics $ts
        $expect = [int]$m.Header + [int]$m.Tab
        $img = @{ W = 320; H = 600; Fn = { param($x, $y) if ($y -lt $expect) { @(226, 236, 247) } elseif ($y -eq $expect) { @(222, 220, 216) } elseif ($x -lt 8) { @(245, 243, 241) } else { @(251, 250, 247) } }.GetNewClosure() }
        $meas = Measure-S4Image $img 1.0 $ts $colors.light
        Expect ($meas.Hairline -eq $expect) "hairline at text scale ${ts}: expected $expect, got $($meas.Hairline)"
        Expect ($meas.Class.Class -eq 'mica') "zones at text scale ${ts}: chrome sample classified $($meas.Class.Class)"
    }
    $z = Get-S4Zones 480 900 1.5 1.0
    Expect ($z.Header.B -le 60 -and $z.CardStrip.L -ge 3 -and $z.CardStrip.R -le 8 -and $z.HairExpect -eq 108) 'zones scale with DPI'

    # ---- geometry
    $work = @{ L = 0; T = 0; R = 1920; B = 1040 }
    $panel = @{ L = 1560; T = 300; R = 1860; B = 680 }; $wiz = @{ L = 0; T = 0; R = 960; B = 1040 }
    $free = Get-AutoFreeRect $work @($panel) @($wiz) 560 380
    Expect ($free -and -not (Test-AutoRectOverlap $free $panel) -and -not (Test-AutoRectOverlap $free $wiz)) 'free rect avoids panel and wizard'
    $free2 = Get-AutoFreeRect $work @($panel) @(@{ L = 0; T = 0; R = 1920; B = 1040 }) 560 380
    Expect ($free2 -and -not (Test-AutoRectOverlap $free2 $panel)) 'free rect falls back to avoiding the panel only'
    $pt = Get-AutoPointOutside @{ L = 1400; T = 300; R = 1800; B = 700 } @($panel)
    Expect ($pt -and -not (Test-AutoPointInRect $pt.X $pt.Y $panel)) 'point outside the panel'
    Reset-AutoMock; New-MockApp (New-AutoTestCtx 'S1' '1')
    $els = Get-MockPanelElements
    $items = Get-S1Items
    $r = Find-AutoElementRect $els "Kind regards,`r`n Alex" 'Button'
    Expect ($r -and $r.T -eq 388) 'UIA lookup normalises whitespace in multi-line names'
    $ep = Get-S1EmptyPoint $panel $els 1.0
    $onButton = $false; foreach ($e in $els) { if ($e.Type -eq 'Button' -and (Test-AutoPointInRect $ep.X $ep.Y $e.Rect)) { $onButton = $true } }
    Expect ($ep.FromUia -and -not $onButton) 'empty-area point avoids the buttons'
    $ep2 = Get-S1EmptyPoint $panel $null 1.0
    $onButton = $false; foreach ($e in $els) { if ($e.Type -eq 'Button' -and (Test-AutoPointInRect $ep2.X $ep2.Y $e.Rect)) { $onButton = $true } }
    Expect (-not $ep2.FromUia -and -not $onButton) 'empty-area fallback avoids the buttons'
    for ($i = 0; $i -lt 4; $i++) {
        $fp = Get-S1ItemPointFallback $panel 1.0 $i
        $hit = $els | Where-Object { $_.Type -eq 'Button' -and (Test-AutoPointInRect $fp.X $fp.Y $_.Rect) } | Select-Object -First 1
        Expect ($hit -and $hit.Name -eq $items[$i]) "fallback point for item $i"
    }
    Expect ((Get-S1ItemPointFallback @{ L = 0; T = 100; R = 300; B = 500 } 1.5 0).Y -eq 196) 'fallback point scales with DPI'
    $tx = Get-S1PanelTexts @(@{ Name = 'Pasted into Notepad.exe' }, @{ Name = 'Pastes into Notepad.exe - x' }, @{ Name = 'Hello from Snipdeck' })
    Expect ($tx.Footer -like 'Pastes into*' -and $tx.Status -like 'Pasted into*') 'panel footer / status split'
    Expect (([int][char]$items[2][0]) -eq 0xFC -and $items[2].Length -eq 16) 'unicode sample item built from code points'

    # ---- verdict builder
    $ok = New-AutoCheck 'a' $true; $unk = New-AutoCheck 'b' $null; $hard = New-AutoCheck 'c' $false 'x' $true; $opt = New-AutoCheck 'd' $null '' $false $false
    Expect ((Resolve-AutoChecks @($ok, $opt)).status -eq 'pass') 'verdict: optional unknown still passes'
    Expect ((Resolve-AutoChecks @($ok, $unk)).status -eq 'unsure') 'verdict: required unknown -> unsure'
    Expect ((Resolve-AutoChecks @($ok, $unk, $hard)).status -eq 'fail') 'verdict: hard failure wins'
    Expect ((Resolve-AutoChecks @($ok) @() 'q?').manualCheck -eq 'q?' -and (Resolve-AutoChecks @($unk) @() 'q?').manualCheck -eq '') 'verdict: owner question only on pass'
    $base = @{ FgAfter = @{ Hwnd = 101; Pid = 5101; Title = 'n'; Process = 'Notepad' }; TargetHwnd = 101; PanelPid = 4242; Footer = 'Pastes into Notepad.exe - n'; Status = 'Pasted into Notepad.exe'
        TextBefore = 'abc'; TextAfter = "abc$($items[0])"; Item = $items[0]; ClipBefore = 'BEFORE'; ClipAfter = 'BEFORE'; HistoryAvailable = $true; HistoryBefore = 0; HistoryAfter = 0 }
    $v = { param($over) $o = $base.Clone(); foreach ($k in $over.Keys) { $o[$k] = $over[$k] }; (Resolve-AutoChecks (Get-S1PasteChecks $o)).status }
    Expect ((& $v @{}) -eq 'pass') 'S1 paste: all good -> pass'
    Expect ((& $v @{ FgAfter = @{ Hwnd = 200; Pid = 4242; Title = 'S1 panel' } }) -eq 'fail') 'S1 paste: panel took focus -> fail'
    Expect ((& $v @{ HistoryAfter = 1 }) -eq 'fail') 'S1 paste: item in history -> fail'
    Expect ((& $v @{ ClipAfter = $items[0] }) -eq 'fail') 'S1 paste: clipboard not restored -> fail'
    Expect ((& $v @{ TextAfter = 'abc' }) -eq 'unsure') 'S1 paste: nothing inserted -> unsure'
    Expect ((& $v @{ Footer = 'Pastes into Notepad.exe (no caret seen)' }) -eq 'unsure') 'S1 paste: no caret seen -> unsure (owner judges)'
    Expect ((& $v @{ Footer = $null; HistoryAvailable = $false }) -eq 'pass') 'S1 paste: unreadable footer/history do not block'
    Expect ((& $v @{ FgAfter = @{ Hwnd = 55; Pid = 1; Title = 'other' } }) -eq 'unsure') 'S1 paste: some other window in front -> unsure'

    # ---- runner: exceptions, bad returns, abort, timeout
    Reset-AutoMock
    function Test-AutoThrows($Ctx) { throw 'boom' }
    function Test-AutoGarbage($Ctx) { 'text'; 42 }
    function Test-AutoSlow($Ctx) { $script:AutoState.Deadline = (Get-Date).AddSeconds(-1); Wait-Auto 100; return New-AutoResult 'pass' 'x' }
    function Test-AutoHeld($Ctx) { $null = Invoke-AutoClick 1 1 @('Ctrl'); $script:Mock.Held['Ctrl'] = $true; throw 'after press' }
    $ctx = New-AutoTestCtx 'S1' '1'
    $rr = Invoke-AutoStep $ctx 'Test-AutoThrows'; Expect ($rr.status -eq 'unsure' -and $rr.evidence -like '*boom*') 'runner: exception -> unsure'
    $rr = Invoke-AutoStep $ctx 'Test-AutoGarbage'; Expect ($rr.status -eq 'unsure') 'runner: no verdict -> unsure'
    $rr = Invoke-AutoStep $ctx 'No-SuchAutomation'; Expect ($rr.status -eq 'unsure') 'runner: unknown function -> unsure'
    $rr = Invoke-AutoStep $ctx 'Test-AutoSlow'; Expect ($rr.status -eq 'unsure' -and $rr.evidence -like 'timed out*') 'runner: deadline -> unsure'
    $rr = Invoke-AutoStep $ctx 'Test-AutoHeld'; Expect ($rr.status -eq 'unsure' -and $script:Mock.Held.Count -eq 0) 'runner: modifiers released after an exception'
    $script:AutoState.Aborted = $false

    # ---- end to end against the mock desktop: every automation, healthy and with faults
    $cases = @(
        @('S1', '1', 'Invoke-S1NoActivate', @{}, 'pass'), @('S1', '1', 'Invoke-S1NoActivate', @{ StealFocus = $true }, 'fail'),
        @('S1', '1', 'Invoke-S1NoActivate', @{ LoaderDialog = $true }, 'fail'), @('S1', '1', 'Invoke-S1NoActivate', @{ UiaUnavailable = $true }, 'pass'),
        @('S1', '2', 'Invoke-S1Paste', @{}, 'pass'), @('S1', '2', 'Invoke-S1Paste', @{ StealFocus = $true }, 'fail'),
        @('S1', '2', 'Invoke-S1Paste', @{ HistoryLeak = $true }, 'fail'), @('S1', '2', 'Invoke-S1Paste', @{ NoRestore = $true }, 'fail'),
        @('S1', '2', 'Invoke-S1Paste', @{ ClickMiss = $true }, 'unsure'), @('S1', '2', 'Invoke-S1Paste', @{ UiaUnavailable = $true }, 'pass'),
        @('S1', '2', 'Invoke-S1Paste', @{ HistoryDisabled = $true }, 'pass'), @('S1', '2', 'Invoke-S1Paste', @{ NoCaret = $true }, 'unsure'),
        @('S1', '2', 'Invoke-S1Paste', @{ TextUnreadable = $true }, 'unsure'), @('S1', '2', 'Invoke-S1Paste', @{ Throw = $true }, 'unsure'),
        @('S1', '2', 'Invoke-S1Paste', @{ AbortAfter = 3 }, 'unsure'), @('S1', '2', 'Invoke-S1Paste', @{ EscHeld = $true }, 'unsure'),
        @('S1', '2a', 'Invoke-S1UnicodePaste', @{}, 'pass'), @('S1', '2a', 'Invoke-S1UnicodePaste', @{ DoublePaste = $true }, 'unsure'),
        @('S1', '2b', 'Invoke-S1ShiftCopy', @{}, 'pass'), @('S1', '2b', 'Invoke-S1ShiftCopy', @{ ModifierNotSeen = $true }, 'unsure'),
        @('S1', '2c', 'Invoke-S1AltWinPaste', @{}, 'pass'), @('S1', '2c', 'Invoke-S1AltWinPaste', @{ StartMenu = $true }, 'fail'),
        @('S1', '2c', 'Invoke-S1AltWinPaste', @{ MenuMode = $true }, 'fail'), @('S1', '2d', 'Invoke-S1CtrlPaste', @{}, 'pass'),
        @('S1', '2d', 'Invoke-S1CtrlPaste', @{ DoublePaste = $true }, 'fail'), @('S1', '2d', 'Invoke-S1CtrlPaste', @{ StuckCtrl = $true }, 'fail'),
        @('S1', '5', 'Invoke-S1ElevatedCopy', @{}, 'pass'), @('S1', '5', 'Invoke-S1ElevatedCopy', @{ PasteIntoElevated = $true }, 'fail'),
        @('S1', '5', 'Invoke-S1ElevatedCopy', @{ UacDeclined = $true }, 'unsure'), @('S1', '5', 'Invoke-S1ElevatedCopy', @{ ClickMiss = $true }, 'unsure'),
        @('S2', '16a', 'Invoke-S2FontRows', @{}, 'pass'), @('S2', '16c', 'Invoke-S2FontRows', @{}, 'pass'),
        @('S4', '1', 'Invoke-S4Mica', @{}, 'pass'), @('S4', '1', 'Invoke-S4Mica', @{ S4BlackChrome = $true }, 'fail'),
        @('S4', '6', 'Invoke-S4ThemeSwitch', @{}, 'pass'), @('S4', '6', 'Invoke-S4ThemeSwitch', @{ S4NoRepaint = $true }, 'fail'),
        @('S4', '7', 'Invoke-S4Transparency', @{}, 'pass'), @('S4', '10', 'Invoke-S4TextScale', @{}, 'pass'),
        @('S4', '10', 'Invoke-S4TextScale', @{ UISettingsStale = $true }, 'unsure'), @('S4', '17', 'Invoke-S4ForcedSolid', @{}, 'pass'),
        @('S4', '17', 'Invoke-S4ForcedSolid', @{ S4BlackChrome = $true }, 'fail'), @('S4', 'W1', 'Invoke-S4Win10Solid', @{}, 'pass'),
        @('S4', 'W1', 'Invoke-S4Win10Solid', @{ Win11 = $true }, 'unsure'), @('S4', 'W3', 'Invoke-S4ThemeSwitchWin10', @{}, 'pass'),
        @('S4', 'W4', 'Invoke-S4TextScaleWin10', @{}, 'pass'), @('S4', 'W7', 'Invoke-S4NoLoaderError', @{}, 'pass'),
        @('S4', 'W7', 'Invoke-S4NoLoaderError', @{ LoaderDialog = $true }, 'fail')
    )
    $launches = @{ '16a' = @{ args = @('--cjk', 'ja') }; '16c' = @{ args = @('--cjk', 'system') }; '17' = @{ env = @{ S4_FORCE_SOLID = '1' } } }
    foreach ($c in $cases) {
        $spike = $c[0]; $id = $c[1]; $fn = $c[2]; $faults = $c[3]; $want = $c[4]
        Reset-AutoMock $faults
        $script:AutoState.Notepad = $null; $script:AutoState.Aborted = $false; $script:AutoState.WizardHwnd = [long]10
        $launch = $null; if ($launches.ContainsKey($id)) { $launch = [pscustomobject]$launches[$id] }
        if ($launch -and $launch.env) { $launch.env = [pscustomobject]$launch.env }
        $os = if ($id -like 'W*' -and -not $faults.Win11) { 'win10' } else { 'win11' }
        $ctx = New-AutoTestCtx $spike $id $launch $os
        Register-AutoApp $ctx
        $regBefore = ($script:Mock.Reg.Keys | Sort-Object | ForEach-Object { "$_=$($script:Mock.Reg[$_])" }) -join ','
        $res = Invoke-AutoStep $ctx $fn 120
        $regAfter = ($script:Mock.Reg.Keys | Sort-Object | ForEach-Object { "$_=$($script:Mock.Reg[$_])" }) -join ','
        $label = "$spike $id $fn $(($faults.Keys | Sort-Object) -join '+')"
        Expect ($res.status -eq $want) "${label}: expected $want, got $($res.status) [$($res.evidence)]"
        Expect ($regBefore -eq $regAfter) "${label}: registry not restored ($regBefore -> $regAfter)"
        Expect ($script:Mock.Held.Count -eq 0) "${label}: modifiers left held"
        Expect ($script:Mock.Fg -eq 10 -or $res.status -ne 'pass') "${label}: focus not handed back to the wizard"
        if (($faults.AbortAfter -or $faults.EscHeld) -and -not $script:AutoState.Aborted) { [void]$p.Add("auto: ${label}: abort not reported") }
    }
    # A window left above the S4 window (the S1 scratch Notepad) must not swallow the activating click.
    Reset-AutoMock
    $script:AutoState.WizardHwnd = [long]10
    $ctx = New-AutoTestCtx 'S4' '1'
    Register-AutoApp $ctx
    $np = Start-AutoProcess 'notepad.exe' @('"overlap.txt"')
    $npw = @(Get-AutoWindows -TitleLike '*overlap.txt*')[0]
    Set-AutoWindowRect $npw.Hwnd @{ L = 900; T = 150; R = 1500; B = 900 }
    $res = Invoke-AutoStep $ctx 'Invoke-S4Mica' 60
    Expect ($res.status -eq 'pass') "S4 Mica with an overlapping window on top: expected pass, got $($res.status) [$($res.evidence)]"

    $script:AutoState.Aborted = $false
    $script:AutoState.Quiet = $false
    $script:AutoState.CountdownDone = $false
    $script:AutoState.Notepad = $null
    $script:AutoState.WizardHwnd = [long]0
    return @($p)
}
