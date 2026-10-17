<#
  Mock primitives for -DryRun and -SelfTest: replaces every function in auto/primitives.ps1 with a small
  simulated desktop (wizard window, scratch Notepads, the S1 panel, the S4 window, clipboard + history,
  registry, the apps' console output). NO real input is ever sent and nothing outside memory is touched.
  Reset-AutoMock @{ Fault = $true } injects faults so the self test can check every verdict path.
  Faults: StealFocus HistoryLeak HistoryDisabled NoRestore DoublePaste ClickMiss UiaUnavailable NoCaret
          StartMenu MenuMode StuckCtrl ModifierNotSeen PasteIntoElevated UacDeclined TextUnreadable
          S4BlackChrome S4NoRepaint UISettingsStale LoaderDialog EscHeld AbortAfter=<n> Throw
#>

$script:Mock = $null

function Reset-AutoMock([hashtable]$Faults = @{}) {
    $script:Mock = @{
        Faults = $Faults; Windows = [ordered]@{}; Fg = [long]10; Target = [long]10
        Clip = ''; Seq = 1000; History = (New-Object System.Collections.ArrayList); HistoryEnabled = (-not $Faults.HistoryDisabled)
        Reg = @{ 'Personalize|AppsUseLightTheme' = 1; 'Personalize|SystemUsesLightTheme' = 1; 'Personalize|EnableTransparency' = 1 }
        RegOriginals = [ordered]@{}; Held = @{}; Stuck = @(); Cursor = @(500, 500); NextHwnd = [long]100; Actions = 0
        AppKey = ''; App = ''; AppPid = 0; Norm = $null; Build = 22631; Lines = (New-Object System.Collections.ArrayList)
        Status = ''; Relaunches = 0; ClearCalls = 0; Broadcasts = 0
    }
    [void](Add-MockWindow @{ Hwnd = [long]10; Title = 'Snipdeck test wizard'; Class = 'CASCADIA_HOSTING_WINDOW_CLASS'; Pid = 900; Process = 'WindowsTerminal'
        Kind = 'wizard'; Rect = @{ L = 0; T = 0; R = 900; B = 1040 } })
}

function Add-MockWindow($W) {
    foreach ($k in 'Visible', 'ExStyle', 'Text', 'Elevated', 'InMenuMode', 'SelectAll') { if (-not $W.ContainsKey($k)) { $W[$k] = $null } }
    $W.Visible = $true
    if (-not $W.ExStyle) { $W.ExStyle = 0 }
    if ($null -eq $W.Text) { $W.Text = '' }
    $script:Mock.Windows[[string]$W.Hwnd] = $W
    return $W
}

function Get-MockWindow([long]$Hwnd) { if (-not $script:Mock) { return $null }; return $script:Mock.Windows[[string]$Hwnd] }

# z-order: the Windows dictionary is bottom-to-top; raising moves a window to the end.
function Move-MockToTop([long]$Hwnd) {
    $w = Get-MockWindow $Hwnd
    if (-not $w) { return }
    $script:Mock.Windows.Remove([string]$Hwnd)
    $script:Mock.Windows[[string]$Hwnd] = $w
}

function Set-MockForeground([long]$Hwnd) {
    $script:Mock.Fg = $Hwnd
    $w = Get-MockWindow $Hwnd
    if ($w -and $w.Kind -ne 'panel') { $script:Mock.Target = $Hwnd; Move-MockToTop $Hwnd }
}

function ConvertTo-MockInfo($W) {
    if (-not $W) { return $null }
    return @{ Hwnd = [long]$W.Hwnd; Title = $W.Title; Class = $W.Class; Pid = $W.Pid; Visible = $true
        Rect = @{ L = $W.Rect.L; T = $W.Rect.T; R = $W.Rect.R; B = $W.Rect.B }; ExStyle = $W.ExStyle; Process = $W.Process }
}

function Step-MockAction { $script:Mock.Actions++ }

# ---------------------------------------------------------------- simulated apps

function Get-MockS4State {
    $f = $script:Mock.Faults
    $env = @{}
    if ($script:Mock.Norm -and $script:Mock.Norm.envMap) { foreach ($k in $script:Mock.Norm.envMap.Keys) { $env[$k] = $script:Mock.Norm.envMap[$k] } }
    $light = ($script:Mock.Reg['Personalize|AppsUseLightTheme'] -ne 0)
    $transReg = $script:Mock.Reg['Personalize|EnableTransparency']
    $trans = ($transReg -ne 0)
    $tsReg = $script:Mock.Reg['Accessibility|TextScaleFactor']
    $ts = 1.0
    if ($tsReg -and -not $f.UISettingsStale) { $ts = [Math]::Min(2.25, [Math]::Max(1.0, $tsReg / 100.0)) }
    $build = $script:Mock.Build
    $chrome = if ($build -lt 22621) { 'Solid(UnsupportedOs)' } elseif (-not $trans) { 'Solid(TransparencyOff)' } elseif ($env['S4_FORCE_SOLID'] -eq '1') { 'Solid(ForcedByConfig)' } else { 'Backdrop(Mica)' }
    return @{ Light = $light; Trans = $trans; TransReg = $transReg; TextScale = $ts; TsReg = $tsReg; Chrome = $chrome; Build = $build
        Renderer = $(if ($env['SLINT_BACKEND']) { $env['SLINT_BACKEND'] } else { 'default (winit-femtovg)' }); Env = $env }
}

function Add-MockS4Line([string]$Reason) {
    $s = Get-MockS4State
    $dot = [char]0xB7
    $theme = if ($s.Light) { 'light' } else { 'dark' }
    $dwm = if ($s.Build -ge 22621) {
        $sbt = if ($s.Chrome -like 'Backdrop*') { 2 } else { 1 }
        "dark=ok corners=ok border=ok extend=Full:ok backdrop=${sbt}:ok"
    } else { 'dark=0x80070057 extend=Full:ok' }
    $tsTxt = $s.TextScale.ToString('0.00', [Globalization.CultureInfo]::InvariantCulture)
    $reg = "reg AppsUseLightTheme=Some($($script:Mock.Reg['Personalize|AppsUseLightTheme'])) EnableTransparency=Some($($s.TransReg)) " +
        "TextScaleFactor=$(if ($s.TsReg) { "Some($($s.TsReg))" } else { 'None' }); UISettings AdvancedEffects=Some($($s.Trans.ToString().ToLower())) " +
        "TextScale=Some($($s.TextScale.ToString('0.0##', [Globalization.CultureInfo]::InvariantCulture))); last WM_SETTINGCHANGE=`"ImmersiveColorSet`""
    $line = "[s4] last refresh: $Reason | OS build $($s.Build) $dot theme $theme $dot transparency $(if ($s.Trans) { 'on' } else { 'off' }) $dot high contrast off | " +
        "DPI scale 1.00 $dot text scale $tsTxt | chrome: $($s.Chrome) | DWM: $dwm | renderer: $($s.Renderer) | " +
        "config: Config { backdrop: Mica, extend_frame: Full, undecorated_shadow: true, no_activate: false, keep_active_look: false, force_solid: false } | sources: $reg"
    [void]$script:Mock.Lines.Add($line)
}

function New-MockS2Lines($ArgList) {
    $mode = 'ja'
    $a = @($ArgList)
    for ($i = 0; $i -lt $a.Count - 1; $i++) { if ($a[$i] -eq '--cjk') { $mode = $a[$i + 1] } }
    $dot = [char]0xB7
    $dbg = @{ ja = 'Ja'; zh = 'Zh'; ko = 'Ko'; system = 'System' }[$mode]
    $l = @('== Snipdeck S2 IME spike ==', "backend: default (winit-femtovg) $dot --cjk $dbg $dot Win+Space switches IME")
    $l += '  system fallback before override  Hani: Microsoft YaHei UI'
    $l += '  system fallback before override  Hira: Yu Gothic UI'
    $l += '  system fallback before override  Kana: Yu Gothic UI'
    $l += '  system fallback before override  Hang: Malgun Gothic'
    $l += '  system fallback before override  Bopo: Microsoft YaHei UI'
    foreach ($f in 'Yu Gothic UI', 'Microsoft YaHei UI', 'Malgun Gothic', 'Meiryo UI', 'SimSun', 'IBM Plex Sans') {
        $l += ('  family {0,-20} found (before window creation): {1}' -f $f, $(if ($f -eq 'IBM Plex Sans') { 'false' } else { 'true' }))
    }
    if ($mode -eq 'system') { $l += '  --cjk system: fallback left to DirectWrite' }
    else {
        $han = if ($mode -eq 'zh') { 'Microsoft YaHei UI, SimSun, Yu Gothic UI, Malgun Gothic' } else { 'Yu Gothic UI, Meiryo UI, Microsoft YaHei UI, Malgun Gothic' }
        $l += "  override Hani: $han"
        $l += '  override Hira: Yu Gothic UI, Meiryo UI'
        $l += '  override Kana: Yu Gothic UI, Meiryo UI'
        $l += '  override Hang: Malgun Gothic'
        $l += '  override Bopo: Microsoft YaHei UI, SimSun'
    }
    $l += '  IBM Plex Sans registered after window creation: true'
    $first = if ($mode -eq 'zh' -or $mode -eq 'system') { 'Microsoft YaHei UI' } else { 'Yu Gothic UI' }
    $l += "  effective fallback after window creation Hani: $first"
    return $l
}

function New-MockApp($Ctx) {
    foreach ($k in @($script:Mock.Windows.Keys)) { if (@('panel', 's4', 's2') -contains $script:Mock.Windows[$k].Kind) { $script:Mock.Windows.Remove($k) } }
    $script:Mock.Lines.Clear()
    $script:Mock.App = $Ctx.SpikeId
    $script:Mock.Norm = $Ctx.App.norm
    $script:Mock.Build = if ($Ctx.OsClass -eq 'win10') { 19045 } else { 22631 }
    $script:Mock.Status = ''
    $procId = 4242; if ($Ctx.App -and $Ctx.App.proc) { $procId = $Ctx.App.proc.Id }
    switch ($Ctx.SpikeId) {
        'S1' {
            [void](Add-MockWindow @{ Hwnd = [long]200; Title = 'S1 panel'; Class = 'Window Class'; Pid = $procId; Process = 's1-noactivate'; Kind = 'panel'
                    Rect = @{ L = 1560; T = 300; R = 1860; B = 680 }; ExStyle = 0x08000088 })
        }
        'S4' {
            [void](Add-MockWindow @{ Hwnd = [long]300; Title = 'Snipdeck S4 spike'; Class = 'Window Class'; Pid = $procId; Process = 's4-mica'; Kind = 's4'
                    Rect = @{ L = 1000; T = 200; R = 1320; B = 800 } })
            [void]$script:Mock.Lines.Add('[s4] subclass=true UISettings events registered')
            Add-MockS4Line 'window created'
        }
        'S2' {
            [void](Add-MockWindow @{ Hwnd = [long]400; Title = 'Snipdeck S2 - IME / CJK spike'; Class = 'Window Class'; Pid = $procId; Process = 's2-ime'; Kind = 's2'
                    Rect = @{ L = 960; T = 100; R = 1760; B = 900 } })
            foreach ($x in (New-MockS2Lines $Ctx.App.norm.argList)) { [void]$script:Mock.Lines.Add($x) }
        }
    }
}

# Panel layout used by both the mock UI Automation tree and the mock click handling.
function Get-MockPanelElements {
    $p = $null
    foreach ($w in $script:Mock.Windows.Values) { if ($w.Kind -eq 'panel') { $p = $w } }
    if (-not $p) { return @() }
    $L = $p.Rect.L; $T = $p.Rect.T; $R = $p.Rect.R; $B = $p.Rect.B
    $items = Get-S1Items
    $centres = @(64, 112, 160, 208); $heights = @(32, 48, 32, 32)
    $els = @(@{ Name = 'Search (activates the panel)'; Type = 'Edit'; Rect = @{ L = $L + 8; T = $T + 8; R = $R - 8; B = $T + 40 } })
    for ($i = 0; $i -lt 4; $i++) {
        $els += @{ Name = $items[$i]; Type = 'Button'; Index = $i; Rect = @{ L = $L + 8; T = $T + $centres[$i] - $heights[$i] / 2; R = $R - 8; B = $T + $centres[$i] + $heights[$i] / 2 } }
    }
    if ($script:Mock.Status) { $els += @{ Name = $script:Mock.Status; Type = 'Text'; Rect = @{ L = $L + 8; T = $B - 90; R = $R - 8; B = $B - 70 } } }
    $els += @{ Name = (Get-MockFooter); Type = 'Text'; Rect = @{ L = $L + 8; T = $B - 60; R = $R - 8; B = $B - 10 } }
    return $els
}

function Get-MockFooter {
    $t = Get-MockWindow $script:Mock.Target
    if (-not $t) { return 'Copies to clipboard (no target window)' }
    $exe = "$($t.Process).exe"
    if ($t.Elevated) { return "Copies to clipboard ($exe runs as admin; Snipdeck does not)" }
    $caret = if ($t.Kind -ne 'notepad' -or $script:Mock.Faults.NoCaret) { ' (no caret seen)' } else { '' }
    return "Pastes into $exe $([char]0xB7) $($t.Title)$caret"
}

function Invoke-MockCopy([string]$Text, [string]$Status) {
    $script:Mock.Clip = $Text; $script:Mock.Seq++
    if ($script:Mock.HistoryEnabled) { $script:Mock.History.Insert(0, $Text) }
    $script:Mock.Status = $Status
}

function Invoke-MockPanelClick([int]$X, [int]$Y, [string[]]$Hold) {
    $f = $script:Mock.Faults
    if ($f.StealFocus) { $script:Mock.Fg = [long]200 }
    if ($f.ClickMiss) { return }
    $hit = $null
    foreach ($e in (Get-MockPanelElements)) { if ($e.Type -eq 'Button' -and (Test-AutoPointInRect $X $Y $e.Rect)) { $hit = $e } }
    if (-not $hit) { return }
    $item = $hit.Name
    $t = Get-MockWindow $script:Mock.Target
    if ($Hold -contains 'Shift') { Invoke-MockCopy $item 'Copied (Shift+click)' }
    elseif ($t -and $t.Elevated -and -not $f.PasteIntoElevated) { Invoke-MockCopy $item 'Copied: Notepad.exe runs as admin; Snipdeck does not' }
    elseif ($t -and $t.Kind -eq 'notepad') {
        if (-not $t.Elevated) { $t.Text += $item; if ($f.DoublePaste) { $t.Text += $item } }
        $script:Mock.Seq += 2
        if ($f.NoRestore) { $script:Mock.Clip = $item }
        if ($f.HistoryLeak -and $script:Mock.HistoryEnabled) { $script:Mock.History.Insert(0, $item) }
        $script:Mock.Status = "Pasted into Notepad.exe $([char]0xB7) $($t.Title)"
    }
    if ($Hold -contains 'Win' -and $f.StartMenu) {
        [void](Add-MockWindow @{ Hwnd = [long]700; Title = 'Start'; Class = 'Windows.UI.Core.CoreWindow'; Pid = 77; Process = 'StartMenuExperienceHost'; Kind = 'start'; Rect = @{ L = 600; T = 300; R = 1300; B = 1000 } })
        $script:Mock.Fg = [long]700
    }
    if ($Hold -contains 'Alt' -and $f.MenuMode -and $t) { $t.InMenuMode = $true }
    if ($f.StuckCtrl) { $script:Mock.Stuck = @('Ctrl') }
}

function Get-MockS4Image {
    $s = Get-MockS4State
    $m = Get-S4Metrics $s.TextScale
    $hdr = [int]$m.Header; $tab = [int]$m.Tab; $ftr = [int]$m.Footer; $W = 320; $H = 600
    $light = $s.Light
    if ($script:Mock.Faults.S4NoRepaint) { $light = $true }
    $bg = if ($light) { @(245, 243, 241) } else { @(17, 15, 13) }
    $surface = if ($light) { @(251, 250, 247) } else { @(23, 22, 19) }
    $border = if ($light) { @(222, 220, 216) } else { @(32, 30, 27) }
    $chrome = if ($script:Mock.Faults.S4BlackChrome) { @(0, 0, 0) } elseif ($s.Chrome -eq 'Backdrop(Mica)') { if ($light) { @(226, 236, 247) } else { @(30, 36, 48) } } else { $bg }
    $fn = {
        param($x, $y)
        if ($y -lt $hdr + $tab) { return $chrome }
        if ($y -eq $hdr + $tab) { return $border }
        if ($y -ge $H - $ftr) { return $chrome }
        if ($y -eq $H - $ftr - 1) { return $border }
        if ($x -lt 8 -or $x -ge $W - 8) { return $bg }
        return $surface
    }.GetNewClosure()
    return @{ W = $W; H = $H; Fn = $fn }
}

# ---------------------------------------------------------------- the primitives API

# -DryRun: SNIPDECK_MOCK_FAULTS="UISettingsStale,AbortAfter=40" injects faults to walk the fallback paths by hand.
function Initialize-AutoPrimitives([string]$StateDir) {
    if (-not $script:Mock) {
        $faults = @{}
        foreach ($f in (([string]$env:SNIPDECK_MOCK_FAULTS) -split ',' | Where-Object { $_.Trim() })) {
            $kv = $f.Trim() -split '=', 2
            $faults[$kv[0]] = if ($kv.Count -gt 1) { [int]$kv[1] } else { $true }
        }
        Reset-AutoMock $faults
    }
    return @{ Ok = $true }
}

function Register-AutoApp($Ctx) {
    if (-not $script:Mock) { Reset-AutoMock }
    $launch = 0; if (Get-Variable -Name LaunchN -Scope Script -ErrorAction SilentlyContinue) { $launch = $script:LaunchN }
    $key = "$($Ctx.SpikeId)|$launch|$($Ctx.App.sig)|$($Ctx.OsClass)"
    if ($key -ne $script:Mock.AppKey) { $script:Mock.AppKey = $key; New-MockApp $Ctx }
}

function Restart-AutoApp($Ctx) { $script:Mock.Relaunches++; New-MockApp $Ctx }
function Invoke-AutoSleep([int]$Ms) { }
function Test-AutoEscHeld { return [bool]$script:Mock.Faults.EscHeld }
function Start-AutoMouseGuard { }
function Stop-AutoMouseGuard { }

function Test-AutoAbortRequested {
    if (-not $script:Mock) { return $null }
    if ($script:Mock.Faults.EscHeld) { return 'Esc is held down (mock)' }
    if ($script:Mock.Faults.AbortAfter -and $script:Mock.Actions -ge $script:Mock.Faults.AbortAfter) { return 'the mouse was moved (mock)' }
    return $null
}

function Get-AutoWindows([int]$ProcessId = 0, [string]$TitleLike = '', [string]$ClassName = '', [switch]$IncludeHidden) {
    $out = @()
    foreach ($w in @($script:Mock.Windows.Values)) {
        if ($ProcessId -and $w.Pid -ne $ProcessId) { continue }
        if ($TitleLike -and $w.Title -notlike $TitleLike) { continue }
        if ($ClassName -and $w.Class -ne $ClassName) { continue }
        $out += ConvertTo-MockInfo $w
    }
    if ($script:Mock.Faults.LoaderDialog -and (-not $TitleLike -or 's4-mica.exe - Entry Point Not Found' -like $TitleLike) -and -not $ProcessId) {
        $out += @{ Hwnd = [long]900; Title = 's4-mica.exe - Entry Point Not Found'; Class = '#32770'; Pid = 1; Visible = $true; Rect = @{ L = 0; T = 0; R = 1; B = 1 }; ExStyle = 0; Process = 'csrss' }
    }
    return $out
}

function Get-AutoWindowInfo([long]$Hwnd) { return ConvertTo-MockInfo (Get-MockWindow $Hwnd) }
function Get-AutoForeground { return ConvertTo-MockInfo (Get-MockWindow $script:Mock.Fg) }

function Get-AutoGuiInfo([long]$Hwnd) {
    $w = Get-MockWindow $Hwnd
    if (-not $w) { return $null }
    $caret = if ($w.Kind -eq 'notepad') { $Hwnd + 1 } else { 0 }
    return @{ InMenuMode = [bool]$w.InMenuMode; FocusHwnd = ($Hwnd + 1); CaretHwnd = $caret }
}

function Get-AutoClientRect([long]$Hwnd) {
    $w = Get-MockWindow $Hwnd
    if ($w.Kind -eq 'notepad') { return @{ L = $w.Rect.L + 8; T = $w.Rect.T + 80; R = $w.Rect.R - 8; B = $w.Rect.B - 30 } }
    return @{ L = $w.Rect.L; T = $w.Rect.T; R = $w.Rect.R; B = $w.Rect.B }
}
function Get-AutoFrameRect([long]$Hwnd) { $w = Get-MockWindow $Hwnd; return @{ L = $w.Rect.L; T = $w.Rect.T; R = $w.Rect.R; B = $w.Rect.B } }
function Get-AutoWorkArea([long]$Hwnd) { return @{ L = 0; T = 0; R = 1920; B = 1040 } }
function Get-AutoDpiScale([long]$Hwnd) { return 1.0 }
function Get-AutoVirtualScreen { return @{ L = 0; T = 0; R = 1920; B = 1080 } }

function Set-AutoWindowRect([long]$Hwnd, $Rect) {
    $w = Get-MockWindow $Hwnd
    if ($w -and $Rect) { $w.Rect = @{ L = $Rect.L; T = $Rect.T; R = $Rect.R; B = $Rect.B } }
    if ($w) { Move-MockToTop $Hwnd }
}

function Set-AutoForeground([long]$Hwnd) { if (Get-MockWindow $Hwnd) { Set-MockForeground $Hwnd; return $true }; return $false }

function Move-AutoMouse([int]$X, [int]$Y) { Assert-AutoContinue; Step-MockAction; $script:Mock.Cursor = @($X, $Y) }

function Invoke-AutoClick([int]$X, [int]$Y, [string[]]$Hold = @()) {
    Move-AutoMouse $X $Y
    if ($script:Mock.Faults.Throw) { throw 'simulated failure in the click primitive' }
    foreach ($m in $Hold) { $script:Mock.Held[$m] = $true }
    try {
        Assert-AutoContinue
        $seen = if ($script:Mock.Faults.ModifierNotSeen) { @() } else { @($Hold) }
        $panel = Get-MockWindow 200
        if ($panel -and (Test-AutoPointInRect $X $Y $panel.Rect)) { Invoke-MockPanelClick $X $Y $Hold }
        else {
            $stack = @($script:Mock.Windows.Values); [array]::Reverse($stack)
            foreach ($w in $stack) {
                if ($w.Kind -ne 'panel' -and (Test-AutoPointInRect $X $Y $w.Rect)) { Set-MockForeground $w.Hwnd; break }
            }
        }
    } finally { Clear-AutoModifiers }
    return @{ HeldSeen = $seen }
}

function Get-AutoModifiersDown { return @($script:Mock.Stuck) + @($script:Mock.Held.Keys) }
function Clear-AutoModifiers { if ($script:Mock) { $script:Mock.Held = @{}; $script:Mock.ClearCalls++ } }

function Send-AutoKeys([string[]]$Chords) {
    foreach ($c in $Chords) {
        Assert-AutoContinue
        Step-MockAction
        $fg = Get-MockWindow $script:Mock.Fg
        switch ($c) {
            'Esc' {
                if ($fg -and $fg.Kind -eq 'start') { $script:Mock.Windows.Remove([string]$fg.Hwnd); $script:Mock.Fg = $script:Mock.Target }
                $t = Get-MockWindow $script:Mock.Target; if ($t) { $t.InMenuMode = $false }
            }
            default {
                if (-not $fg -or $fg.Kind -ne 'notepad' -or $fg.Elevated) { break }
                if ($c -eq 'Ctrl+A') { $fg.SelectAll = $true }
                elseif ($c -eq 'Delete') { if ($fg.SelectAll) { $fg.Text = '' }; $fg.SelectAll = $false }
                elseif ($c -eq 'Ctrl+V') { $fg.Text += $script:Mock.Clip }
                elseif ($c -eq 'X') { if ($script:Mock.Stuck -notcontains 'Ctrl' -and -not $fg.InMenuMode) { $fg.Text += 'x' } }
            }
        }
    }
}

function Send-AutoText([string]$Text) {
    Assert-AutoContinue
    Step-MockAction
    $fg = Get-MockWindow $script:Mock.Fg
    if ($fg -and $fg.Kind -eq 'notepad' -and -not $fg.Elevated) { $fg.Text += $Text; $fg.SelectAll = $false }
}

function Get-AutoClipboard { return $script:Mock.Clip }
function Set-AutoClipboard([string]$Text) { Invoke-MockCopy $Text $script:Mock.Status }
function Get-AutoClipboardSeq { return $script:Mock.Seq }

function Get-AutoClipboardHistory {
    if (-not $script:Mock.HistoryEnabled) { return @{ Available = $false; Reason = 'clipboard history is turned off (mock)'; Texts = @() } }
    return @{ Available = $true; Reason = ''; Texts = @($script:Mock.History) }
}

function Get-AutoUiaElements([long]$Hwnd) {
    if ($script:Mock.Faults.UiaUnavailable) { return $null }
    $w = Get-MockWindow $Hwnd
    if ($w -and $w.Kind -eq 'panel') { return Get-MockPanelElements }
    return $null
}

function Get-AutoEditText([long]$Hwnd) {
    $w = Get-MockWindow $Hwnd
    if (-not $w -or $w.Kind -ne 'notepad' -or $w.Elevated -or $script:Mock.Faults.TextUnreadable) { return @{ Ok = $false; Text = $null; Via = ''; Reason = 'mock: not readable' } }
    return @{ Ok = $true; Text = $w.Text; Via = 'mock' }
}

function Get-AutoScreenPixels($Rect, [string]$SavePath = '') {
    $s4 = Get-MockWindow 300
    if ($s4 -and $Rect.L -eq $s4.Rect.L -and $Rect.T -eq $s4.Rect.T) { return Get-MockS4Image }
    return @{ W = [int]($Rect.R - $Rect.L); H = [int]($Rect.B - $Rect.T); Fn = { param($x, $y) @(128, 128, 128) } }
}

function Get-AutoRegistryDword([string]$Key, [string]$Name) { return $script:Mock.Reg["$Key|$Name"] }

function Set-AutoRegistryDword([string]$Key, [string]$Name, [int]$Value) {
    $id = "$Key|$Name"
    if (-not $script:Mock.RegOriginals.Contains($id)) { $script:Mock.RegOriginals[$id] = @{ Existed = $script:Mock.Reg.ContainsKey($id); Value = $script:Mock.Reg[$id] } }
    $script:Mock.Reg[$id] = $Value
}

function Send-AutoSettingChange([string]$Area = 'ImmersiveColorSet') {
    $script:Mock.Broadcasts++
    if ($script:Mock.App -eq 'S4') { Add-MockS4Line 'WM_SETTINGCHANGE' }
}

function Restore-AutoRegistry {
    if (-not $script:Mock -or $script:Mock.RegOriginals.Count -eq 0) { return }
    foreach ($id in @($script:Mock.RegOriginals.Keys)) {
        $o = $script:Mock.RegOriginals[$id]
        if ($o.Existed) { $script:Mock.Reg[$id] = $o.Value } else { $script:Mock.Reg.Remove($id) }
    }
    $script:Mock.RegOriginals = [ordered]@{}
    Send-AutoSettingChange 'ImmersiveColorSet'
}

function Restore-AutoRegistryFromFile([string]$StateDir) { return $false }

function New-AutoScratchFile($Ctx, [string]$Prefix) {
    $script:Mock.NextHwnd++
    return (Join-Path (Join-Path ([IO.Path]::GetTempPath()) 'snipdeck-mock') ('{0}-{1}.txt' -f $Prefix, $script:Mock.NextHwnd))
}

function Start-AutoProcess([string]$File, [string[]]$Arguments = @(), [switch]$Elevated) {
    if ($Elevated -and $script:Mock.Faults.UacDeclined) { return @{ Ok = $false; Reason = 'The operation was canceled by the user.' } }
    if ($File -ne 'notepad.exe') { return @{ Ok = $false; Reason = "mock cannot start $File" } }
    $path = ([string]($Arguments -join ' ')).Trim('"')
    $name = ($path -split '[\\/]')[-1]
    $script:Mock.NextHwnd += 10
    $h = $script:Mock.NextHwnd
    [void](Add-MockWindow @{ Hwnd = $h; Title = "$name - Notepad"; Class = 'Notepad'; Pid = [int](5000 + $h); Process = 'Notepad'; Kind = 'notepad'
            Elevated = [bool]$Elevated; Rect = @{ L = 200; T = 150; R = 1000; B = 750 } })
    Set-MockForeground $h
    return @{ Ok = $true; Pid = [int](5000 + $h) }
}

function Get-AutoAppLines($Ctx) { return @($script:Mock.Lines) }
function Test-AutoAppRunning($Ctx) { return $true }
function Find-AutoLoaderDialog([string]$Exe) { if ($script:Mock.Faults.LoaderDialog) { return "$Exe.exe - Entry Point Not Found" }; return $null }
function Exit-AutoSession { }
