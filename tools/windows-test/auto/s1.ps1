<#
  S1 automations (non-activating panel + paste). Each Invoke-* function is named by an "auto" field in
  tests/s1-noactivate.json and returns @{ status; evidence; screenshots; manualCheck }.
  Target app: a scratch Notepad document opened by the automation (never the owner's own text).
  Not automated (judgement or other apps): Word, Chrome, search box, terminals, fullscreen, Remote Desktop,
  taskbar/Alt+Tab visibility (asked as a one-key question instead).
#>

function Get-S1PanelWindow($Ctx) {
    $w = Wait-AutoWindow -ProcessId $Ctx.App.proc.Id -TitleLike 'S1 panel' -TimeoutMs 8000
    if (-not $w) { throw 'the S1 panel window was not found' }
    return $w
}

function Open-S1Notepad($Ctx) {
    $np = $script:AutoState.Notepad
    if ($np -and -not (Get-AutoWindowInfo $np.Hwnd)) { $np = $null }
    if ($np) { return $np }
    $file = New-AutoScratchFile $Ctx 'snipdeck-auto'
    $name = [IO.Path]::GetFileName($file)
    Write-AutoLog "opening a scratch Notepad document ($name)"
    $p = Start-AutoProcess 'notepad.exe' @("`"$file`"")
    if (-not $p.Ok) { throw "could not start Notepad: $($p.Reason)" }
    $w = Wait-AutoWindow -TitleLike "*$name*" -TimeoutMs 15000
    if (-not $w) { throw 'the scratch Notepad window did not appear' }
    $np = @{ Hwnd = [long]$w.Hwnd; Marker = $name; OrigRect = $w.Rect }
    $script:AutoState.Notepad = $np
    return $np
}

function Read-S1Text($Np) {
    $t = Get-AutoEditText $Np.Hwnd
    if ($t.Ok) { return $t.Text }
    return $null
}

# Brings the scratch Notepad to the front (clear of the panel and, if possible, the wizard), empties it
# and types $Prefix. Refuses to clear anything unless the front window is that exact document.
function Reset-S1Notepad($Ctx, $Panel, [string]$Prefix = 'abc') {
    $np = Open-S1Notepad $Ctx
    $work = Get-AutoWorkArea $Panel.Hwnd
    $wiz = $null
    if ($script:AutoState.WizardHwnd) { $wi = Get-AutoWindowInfo $script:AutoState.WizardHwnd; if ($wi) { $wiz = $wi.Rect } }
    $scale = Get-AutoDpiScale $Panel.Hwnd
    $want = Get-AutoFreeRect $work @($Panel.Rect) @($wiz) ([int](560 * $scale)) ([int](380 * $scale))
    Set-AutoWindowRect $np.Hwnd $want
    Wait-Auto 300
    $pt = Get-AutoPointOutside (Get-AutoClientRect $np.Hwnd) @($Panel.Rect)
    if (-not $pt) { throw 'no free spot to click into Notepad' }
    $null = Invoke-AutoClick $pt.X $pt.Y
    Wait-Auto 300
    $fg = Get-AutoForeground
    if (-not $fg -or [long]$fg.Hwnd -ne [long]$np.Hwnd -or $fg.Title -notlike "*$($np.Marker)*") {
        throw "the scratch Notepad did not come to the front (front window: '$($fg.Title)')"
    }
    Send-AutoKeys @('Ctrl+A', 'Delete')
    Wait-Auto 200
    $t = Get-AutoEditText $np.Hwnd
    if (-not $t.Ok) { throw 'cannot read the Notepad text (no Edit/RichEdit control, no UI Automation text)' }
    if ((ConvertTo-AutoNormText $t.Text) -ne '') { throw "could not empty the scratch Notepad (text: $(Format-AutoShort $t.Text))" }
    if ($Prefix) {
        Send-AutoText $Prefix
        Wait-Auto 250
        $now = Read-S1Text $np
        if ((ConvertTo-AutoNormText $now) -ne $Prefix) { throw "typing into Notepad did not work (text: $(Format-AutoShort $now))" }
    }
    return $np
}

function Get-S1ItemPoint($Panel, [int]$Index) {
    $els = Get-AutoUiaElements $Panel.Hwnd
    $r = $null
    $name = (Get-S1Items)[$Index]
    if ($els) { $r = Find-AutoElementRect $els $name 'Button' }
    if ($r) {
        $c = Get-AutoRectCenter $r
        # UI Automation rectangles in another coordinate space (DPI virtualisation) would miss the panel: ignore them then.
        $frame = (Get-AutoWindowInfo $Panel.Hwnd).Rect
        if (Test-AutoPointInRect $c.X $c.Y $frame) { return @{ X = $c.X; Y = $c.Y; FromUia = $true } }
    }
    $p = Get-S1ItemPointFallback (Get-AutoClientRect $Panel.Hwnd) (Get-AutoDpiScale $Panel.Hwnd) $Index
    return @{ X = $p.X; Y = $p.Y; FromUia = $false }
}

function Read-S1PanelTexts($Panel) {
    $els = Get-AutoUiaElements $Panel.Hwnd
    if (-not $els) { return @{ Footer = $null; Status = $null } }
    return Get-S1PanelTexts $els
}

function Get-S1HistoryCount($Hist, [string]$Item) {
    if (-not $Hist -or -not $Hist.Available) { return $null }
    $want = ConvertTo-AutoNormText $Item
    return @($Hist.Texts | Where-Object { (ConvertTo-AutoNormText $_) -eq $want }).Count
}

# ---------------------------------------------------------------- step 1

function Invoke-S1NoActivate($Ctx) {
    $panel = Get-S1PanelWindow $Ctx
    $loader = Find-AutoLoaderDialog $Ctx.Spike.exe
    Enter-AutoHandsOff 'S1 step 1: give Notepad focus, click an empty spot of the panel, check nothing moved'
    $np = Reset-S1Notepad $Ctx $panel 'abc'
    $fg0 = Get-AutoForeground
    $g0 = Get-AutoGuiInfo $np.Hwnd
    $client = Get-AutoClientRect $panel.Hwnd
    $pt = Get-S1EmptyPoint $client (Get-AutoUiaElements $panel.Hwnd) (Get-AutoDpiScale $panel.Hwnd)
    if (-not (Test-AutoPointInRect $pt.X $pt.Y $client)) { $pt = Get-S1EmptyPoint $client $null (Get-AutoDpiScale $panel.Hwnd) }
    $null = Invoke-AutoClick $pt.X $pt.Y
    Wait-Auto 800
    $fg1 = Get-AutoForeground
    $g1 = Get-AutoGuiInfo $np.Hwnd
    $after = Read-S1Text $np
    $now = Get-AutoWindowInfo $panel.Hwnd
    $o = @{
        LoaderDialog = $loader; PanelFound = [bool]$now; Topmost = ($now -and (([long]$now.ExStyle -band 0x8) -ne 0))
        ExStyle = $(if ($now) { $now.ExStyle } else { 0 })
        FgBefore = $fg0; FgAfter = $fg1; TargetHwnd = $np.Hwnd; PanelPid = $panel.Pid
        CaretBefore = $(if ($g0) { $g0.CaretHwnd -ne 0 } else { $null }); CaretAfter = $(if ($g1) { $g1.CaretHwnd -ne 0 } else { $null })
        TextChanged = ($null -ne $after -and (ConvertTo-AutoNormText $after) -ne 'abc')
    }
    return Resolve-AutoChecks (Get-S1NoActivateChecks $o) @() 'Check by eye: the S1 panel has NO taskbar button (and showed none when it started), and it is NOT in the Alt+Tab list (hold Alt, press Tab, look, press Esc, release Alt).'
}

# ---------------------------------------------------------------- step 2

function Invoke-S1Paste($Ctx) {
    $item = (Get-S1Items)[0]
    $panel = Get-S1PanelWindow $Ctx
    Enter-AutoHandsOff "S1 step 2: type abc in Notepad, click 'Hello from Snipdeck', check focus, text, clipboard and history"
    $np = Reset-S1Notepad $Ctx $panel 'abc'
    Set-AutoClipboard 'BEFORE'
    Wait-Auto 500
    $h0 = Get-AutoClipboardHistory
    $pt = Get-S1ItemPoint $panel 0
    Move-AutoMouse $pt.X $pt.Y
    Wait-Auto 500   # hovering refreshes the footer
    $t0 = Read-S1PanelTexts $panel
    $null = Invoke-AutoClick $pt.X $pt.Y
    Wait-Auto 1200
    $fg = Get-AutoForeground
    $after = Read-S1Text $np
    $clip = Get-AutoClipboard
    Wait-Auto 600
    $h1 = Get-AutoClipboardHistory
    $t1 = Read-S1PanelTexts $panel
    $o = @{
        FgAfter = $fg; TargetHwnd = $np.Hwnd; PanelPid = $panel.Pid; Footer = $t0.Footer; Status = $t1.Status
        TextBefore = 'abc'; TextAfter = $after; Item = $item; ClipBefore = 'BEFORE'; ClipAfter = $clip
        HistoryAvailable = ($h0.Available -and $h1.Available); HistoryReason = $(if (-not $h0.Available) { $h0.Reason } else { $h1.Reason })
        HistoryBefore = (Get-S1HistoryCount $h0 $item); HistoryAfter = (Get-S1HistoryCount $h1 $item)
    }
    $mc = ''
    if (-not $o.HistoryAvailable) { $mc = "Clipboard history could not be read ($($o.HistoryReason)). Press Win+V: the newest entry should be 'BEFORE', NOT 'Hello from Snipdeck' (Esc closes it)." }
    return Resolve-AutoChecks (Get-S1PasteChecks $o) @() $mc
}

# ---------------------------------------------------------------- step 2a

function Invoke-S1UnicodePaste($Ctx) {
    $items = Get-S1Items
    $panel = Get-S1PanelWindow $Ctx
    Enter-AutoHandsOff 'S1 step 2a: paste the unicode item and the two-line item into an empty Notepad'
    $np = Reset-S1Notepad $Ctx $panel ''
    Set-AutoClipboard 'BEFORE'
    foreach ($idx in 2, 1) {
        $pt = Get-S1ItemPoint $panel $idx
        $null = Invoke-AutoClick $pt.X $pt.Y
        Wait-Auto 900
    }
    $o = @{ FgAfter = (Get-AutoForeground); TargetHwnd = $np.Hwnd; PanelPid = $panel.Pid; Expected = ($items[2] + $items[1]); TextAfter = (Read-S1Text $np) }
    return Resolve-AutoChecks (Get-S1ExactTextChecks $o)
}

# ---------------------------------------------------------------- step 2b

function Invoke-S1ShiftCopy($Ctx) {
    $item = (Get-S1Items)[0]
    $panel = Get-S1PanelWindow $Ctx
    Enter-AutoHandsOff 'S1 step 2b: Shift+click an item (copy only), then Ctrl+V in Notepad'
    $np = Reset-S1Notepad $Ctx $panel 'abc'
    Set-AutoClipboard 'BEFORE'
    Wait-Auto 300
    $pt = Get-S1ItemPoint $panel 0
    $click = Invoke-AutoClick $pt.X $pt.Y @('Shift')
    Wait-Auto 1000
    $fg = Get-AutoForeground
    $afterClick = Read-S1Text $np
    $clip = Get-AutoClipboard
    $texts = Read-S1PanelTexts $panel
    $afterPaste = $null
    if ($fg -and [long]$fg.Hwnd -eq [long]$np.Hwnd -and (ConvertTo-AutoNormText $afterClick) -eq 'abc') {
        Send-AutoKeys @('Ctrl+V')
        Wait-Auto 500
        $afterPaste = Read-S1Text $np
    }
    $o = @{
        ModWasDown = (@($click.HeldSeen) -contains 'Shift'); FgAfter = $fg; TargetHwnd = $np.Hwnd; PanelPid = $panel.Pid
        TextBefore = 'abc'; TextAfterClick = $afterClick; TextAfterPaste = $afterPaste; Item = $item; ClipAfter = $clip; Status = $texts.Status
    }
    return Resolve-AutoChecks (Get-S1ShiftChecks $o)
}

# ---------------------------------------------------------------- steps 2c / 2d

function Invoke-S1ModifierRound($Ctx, $Panel, [string]$Mod) {
    $item = (Get-S1Items)[0]
    $np = Reset-S1Notepad $Ctx $Panel 'abc'
    Set-AutoClipboard 'BEFORE'
    Wait-Auto 300
    $pt = Get-S1ItemPoint $Panel 0
    $click = Invoke-AutoClick $pt.X $pt.Y @($Mod)
    Wait-Auto 1000
    $fg = Get-AutoForeground
    $start = $null
    if ($fg -and (Test-AutoShellPopup $fg)) {
        $start = "$($fg.Process): $($fg.Title)"
        Send-AutoKeys @('Esc')
        Wait-Auto 400
    }
    $gui = Get-AutoGuiInfo $np.Hwnd
    $after = Read-S1Text $np
    Wait-Auto 300
    $stuck = @(Get-AutoModifiersDown)
    $menu = [bool]($gui -and $gui.InMenuMode)
    $probe = $null
    if (-not $start -and -not $menu -and $stuck.Count -eq 0 -and $fg -and [long]$fg.Hwnd -eq [long]$np.Hwnd -and $null -ne $after) {
        Send-AutoKeys @('X')
        Wait-Auto 300
        $p = Read-S1Text $np
        $probe = ((ConvertTo-AutoNormText $p) -eq ((ConvertTo-AutoNormText $after) + 'x'))
        if (-not $probe) { Send-AutoKeys @('Esc') }
    }
    if ($menu) { Send-AutoKeys @('Esc') }
    return @{
        Modifier = $Mod; ModWasDown = (@($click.HeldSeen) -contains $Mod); StartMenu = $start; FgAfter = $fg; TargetHwnd = $np.Hwnd
        PanelPid = $Panel.Pid; TextBefore = 'abc'; TextAfter = $after; Item = $item; InMenuMode = $menu; StuckModifiers = $stuck; ProbeOk = $probe
    }
}

function Invoke-S1AltWinPaste($Ctx) {
    $panel = Get-S1PanelWindow $Ctx
    Enter-AutoHandsOff 'S1 step 2c: Alt+click, then Win+click an item into Notepad, then type normally'
    $all = @()
    foreach ($mod in 'Alt', 'Win') {
        foreach ($c in @(Get-S1ModifierChecks (Invoke-S1ModifierRound $Ctx $panel $mod))) {
            if ($c.Name -notlike "$mod *") { $c.Name = "${mod}: $($c.Name)" }
            $all += $c
        }
    }
    return Resolve-AutoChecks $all
}

function Invoke-S1CtrlPaste($Ctx) {
    $panel = Get-S1PanelWindow $Ctx
    Enter-AutoHandsOff 'S1 step 2d: Ctrl+click an item into Notepad, then type normally'
    return Resolve-AutoChecks (Get-S1ModifierChecks (Invoke-S1ModifierRound $Ctx $panel 'Ctrl'))
}

# ---------------------------------------------------------------- step 5 (admin Notepad: needs the owner for UAC)

function Invoke-S1ElevatedCopy($Ctx) {
    $item = (Get-S1Items)[0]
    $panel = Get-S1PanelWindow $Ctx
    if (-not (Request-AutoOwnerOk 'Step 5 needs an ADMIN Notepad. Windows will show a UAC prompt that only you can accept. Start it now?')) {
        return New-AutoResult 'unsure' 'you chose not to start the admin Notepad'
    }
    $file = New-AutoScratchFile $Ctx 'snipdeck-admin'
    $name = [IO.Path]::GetFileName($file)
    $p = Start-AutoProcess 'notepad.exe' @("`"$file`"") -Elevated
    if (-not $p.Ok) { return New-AutoResult 'unsure' "the admin Notepad was not started: $($p.Reason)" }
    Write-AutoLog 'waiting for the admin Notepad: accept the UAC prompt (Yes)...' Yellow
    $w = Wait-AutoWindow -TitleLike "*$name*" -TimeoutMs 60000
    if (-not $w) { return New-AutoResult 'unsure' 'the admin Notepad window did not appear within 60 s' }
    if (-not (Wait-AutoForeground $w.Hwnd 1500)) {
        Write-AutoLog 'Click ONCE inside the admin Notepad text area now (waiting up to 30 s)...' Yellow
        if (-not (Wait-AutoForeground $w.Hwnd 30000)) { return New-AutoResult 'unsure' 'the admin Notepad never came to the front' }
    }
    Enter-AutoHandsOff 'S1 step 5: click an item while the admin Notepad has focus'
    Set-AutoClipboard 'BEFORE5'
    Wait-Auto 300
    $seq0 = Get-AutoClipboardSeq
    $text0 = Read-S1Text @{ Hwnd = $w.Hwnd }
    $pt = Get-S1ItemPoint $panel 0
    Move-AutoMouse $pt.X $pt.Y
    Wait-Auto 500
    $t0 = Read-S1PanelTexts $panel
    $null = Invoke-AutoClick $pt.X $pt.Y
    Wait-Auto 1200
    $fg = Get-AutoForeground
    $clip = Get-AutoClipboard
    $seq1 = Get-AutoClipboardSeq
    $text1 = Read-S1Text @{ Hwnd = $w.Hwnd }
    $t1 = Read-S1PanelTexts $panel
    $changed = $null
    if ($null -ne $text0 -and $null -ne $text1) { $changed = ((ConvertTo-AutoNormText $text1) -ne (ConvertTo-AutoNormText $text0)) }
    $o = @{
        FgAfter = $fg; TargetHwnd = $w.Hwnd; PanelPid = $panel.Pid; ClipBefore = 'BEFORE5'; ClipAfter = $clip; SeqBefore = $seq0; SeqAfter = $seq1
        Item = $item; Footer = $t0.Footer; Status = $t1.Status; TextChanged = $changed
    }
    return Resolve-AutoChecks (Get-S1ElevatedChecks $o) @() 'Now press Ctrl+V in the admin Notepad yourself: does the item paste there? (Then close that Notepad without saving.)'
}
