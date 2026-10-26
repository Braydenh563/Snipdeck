<#
  Pure helpers for the wizard's flow: which steps a plan runs, how long it takes, step/progress text,
  resume detection, result counts and the results file name. No console, no Windows, no files, no clock:
  everything takes its inputs as arguments so auto\tests\selftest.ps1 can check it anywhere.
  Dot-sourced by run-tests.ps1. Keep it ASCII and Windows PowerShell 5.1 compatible (no ?? , no ternary, no &&).
#>

# ---------------------------------------------------------------- plan (what the run will do)

# The recommended run is "v1": the steps flagged "v1": true in tests\*.json, automation on, nothing pushed anywhere.
function New-Plan {
    return [pscustomobject]@{ mode = 'v1'; spikeIds = @(); auto = $true; advanced = $false; push = $false }
}

function Test-StepInMode($T, [string]$RunMode, [string]$OsClass) {
    if ($T.os -and $T.os -ne 'any' -and $T.os -ne $OsClass) { return $false }
    # The Quick check (about 10 minutes): only steps flagged "qc": true. Those steps ("qcOnly") never run in the other modes.
    if ($RunMode -eq 'qc') { return ($T.qc -eq $true) }
    if ($T.qcOnly -eq $true) { return $false }
    $isGate = ($T.gate -eq $true) -or [bool]$T.anyGroup
    $isOptional = ($T.optional -eq $true)
    if ($RunMode -eq 'v1') { return ($T.v1 -eq $true) }
    if ($RunMode -eq 'quick' -and -not $isGate) { return $false }
    if ($RunMode -eq 'standard' -and $isOptional) { return $false }
    return $true
}

# Which prototype files take part. Explicit ids win; "v1" means every file that has at least one v1 step.
function Get-PlanSpikeIds($Spikes, [string]$RunMode, [string[]]$Requested = @()) {
    $ids = @()
    foreach ($s in $Spikes) {
        $take = $false
        if (@($Requested).Count -gt 0) { $take = (@($Requested | ForEach-Object { $_.ToUpper() }) -contains $s.id) }
        elseif ($RunMode -eq 'qc') { $take = (@($s.tests | Where-Object { $_.qc -eq $true }).Count -gt 0) }
        elseif ($RunMode -eq 'v1') { $take = (@($s.tests | Where-Object { $_.v1 -eq $true }).Count -gt 0) }
        else { $take = ($s.quickOnly -ne $true) }
        if ($take) { $ids += $s.id }
    }
    return $ids
}

# Plain name for a prototype (the JSON "friendly" field), falling back to its title.
function Get-FriendlyName($SpikeDef) {
    if ($SpikeDef.friendly) { return [string]$SpikeDef.friendly }
    return [string]$SpikeDef.title
}

# ---------------------------------------------------------------- Quick check helpers (pure)

# The chord that shows the Panel, from the text of a settings.json (Exists = the file is there). No file = a new
# install (Ctrl+Shift+Y); a file without the setting = an install from before the new defaults (Ctrl+Alt+Space);
# a chord set to "" means the shortcut is off ($null).
function Get-PanelChordFromSettings([string]$Json, [bool]$Exists) {
    if (-not $Exists) { return 'Ctrl+Shift+Y' }
    $obj = $null
    try { $obj = $Json | ConvertFrom-Json } catch { return 'Ctrl+Alt+Space' }
    if ($obj -and $obj.hotkeys -and ($obj.hotkeys.PSObject.Properties.Name -contains 'toggle_panel') -and ($obj.hotkeys.toggle_panel -is [string])) {
        $c = ([string]$obj.hotkeys.toggle_panel).Trim()
        if ($c.Length -eq 0) { return $null }
        return $c
    }
    return 'Ctrl+Alt+Space'
}

# Which screen edge the taskbar is on, from a monitor's full bounds and working area (@{ L; T; R; B }). 'none' = hidden or none.
function Get-TaskbarEdge($Bounds, $Work) {
    $t = [int]($Work.T - $Bounds.T); $l = [int]($Work.L - $Bounds.L); $r = [int]($Bounds.R - $Work.R)
    $bottom = [int]($Bounds.B - $Work.B)
    $best = 'none'; $size = 0
    if ($bottom -gt $size) { $best = 'bottom'; $size = $bottom }
    if ($t -gt $size) { $best = 'top'; $size = $t }
    if ($l -gt $size) { $best = 'left'; $size = $l }
    if ($r -gt $size) { $best = 'right'; $size = $r }
    return $best
}

# Replaces {NAME} in step text with Vars[NAME] (a hashtable); an unknown name stays as it is.
function Expand-StepText([string]$Text, $Vars) {
    if (-not $Text -or -not $Vars) { return $Text }
    $out = $Text
    foreach ($k in @($Vars.Keys)) { $out = $out.Replace('{' + $k + '}', [string]$Vars[$k]) }
    return $out
}

# ---------------------------------------------------------------- time and progress text

# A step's rough length in minutes: an optional "minutes" field wins; automated steps are quick.
function Get-StepMinutes($T, [bool]$AutoOn) {
    if ($T.minutes) { return [double]$T.minutes }
    if ($AutoOn -and $T.auto) { return 0.5 }
    return 1.5
}

# "about 45 min", "about 1 h 30 min", "under a minute". Rounds to 5 minutes from 20 minutes up.
function Format-Duration([double]$Minutes) {
    if ($Minutes -lt 1) { return 'under a minute' }
    $n = [int][Math]::Ceiling($Minutes)
    if ($n -ge 20) { $n = 5 * [int][Math]::Round($n / 5.0, [MidpointRounding]::AwayFromZero) }
    if ($n -ge 90) {
        $h = [int][Math]::Floor($n / 60); $m = $n - 60 * $h
        if ($m -eq 0) { return "about $h h" }
        return "about $h h $m min"
    }
    return "about $n min"
}

# "Step 3 of 35 - about 40 min left" (Index is 1-based).
function Format-StepHeader([int]$Index, [int]$Total, [double]$MinutesLeft) {
    $left = if ($MinutesLeft -lt 1) { 'almost done' } else { (Format-Duration $MinutesLeft) + ' left' }
    return "Step $Index of $Total - $left"
}

# "[#########---------------]  38%" (Done of Total complete).
function Format-ProgressBar([int]$Done, [int]$Total, [int]$Width = 30) {
    if ($Total -lt 1) { $Total = 1 }
    if ($Done -lt 0) { $Done = 0 }
    if ($Done -gt $Total) { $Done = $Total }
    $fill = [int][Math]::Floor($Width * $Done / $Total)
    $pct = [int][Math]::Floor(100 * $Done / $Total)
    return ('[' + ('#' * $fill) + ('-' * ($Width - $fill)) + ']' + (' {0,3}%' -f $pct))
}

# Minutes still to go for queue items From..end (0-based), skipping the ones already answered.
function Get-MinutesLeft($Queue, $Results, [int]$From, [bool]$AutoOn) {
    $Queue = @($Queue)
    $sum = 0.0
    for ($i = [Math]::Max(0, $From); $i -lt @($Queue).Count; $i++) {
        $item = $Queue[$i]
        if (Test-StepAnswered $Results $item.Key) { continue }
        $sum += (Get-StepMinutes $item.Test $AutoOn)
    }
    return $sum
}

# ---------------------------------------------------------------- the queue of steps, results, resume

function Test-StepAnswered($Results, [string]$Key) {
    if (-not $Results) { return $false }
    $has = $false
    if ($Results -is [System.Collections.IDictionary]) { $has = $Results.Contains($Key) }
    if (-not $has) { return $false }
    return (@('pass', 'fail', 'skip') -contains $Results[$Key].status)
}

# Every step this run will go through, in order: @{ Key; SpikeId; Spike; Test }.
function Get-RunQueue($Spikes, [string[]]$SpikeIds, [string]$RunMode, [string]$OsClass) {
    $q = @()
    foreach ($s in $Spikes) {
        if (@($SpikeIds) -notcontains $s.id) { continue }
        foreach ($t in $s.tests) {
            if (-not (Test-StepInMode $t $RunMode $OsClass)) { continue }
            $q += [pscustomobject]@{ Key = "$($s.id):$($t.id)"; SpikeId = $s.id; Spike = $s; Test = $t }
        }
    }
    return $q
}

function Get-RunProgress($Queue, $Results) {
    $answered = 0
    foreach ($item in @($Queue)) { if (Test-StepAnswered $Results $item.Key) { $answered++ } }
    return [pscustomobject]@{ total = @($Queue).Count; answered = $answered; remaining = (@($Queue).Count - $answered) }
}

# An earlier run is worth continuing when it has some answers and some steps left.
function Test-RunResumable($Queue, $Results) {
    $p = Get-RunProgress $Queue $Results
    return ($p.answered -gt 0 -and $p.remaining -gt 0)
}

# Index of the first step without an answer (so a continued run opens where it stopped); 0 when none.
function Get-FirstOpenIndex($Queue, $Results) {
    $Queue = @($Queue)
    for ($i = 0; $i -lt @($Queue).Count; $i++) { if (-not (Test-StepAnswered $Results $Queue[$i].Key)) { return $i } }
    return 0
}

# Index of the first unanswered step after From; the queue length when there is none (= the run is over).
function Get-NextOpenIndex($Queue, $Results, [int]$From) {
    $Queue = @($Queue)
    for ($i = $From + 1; $i -lt @($Queue).Count; $i++) { if (-not (Test-StepAnswered $Results $Queue[$i].Key)) { return $i } }
    return @($Queue).Count
}

function Get-ResultCounts($Queue, $Results) {
    $c = [ordered]@{ worked = 0; failed = 0; skipped = 0; notDone = 0 }
    foreach ($item in @($Queue)) {
        $st = 'none'
        if (Test-StepAnswered $Results $item.Key) { $st = $Results[$item.Key].status }
        if ($st -eq 'pass') { $c.worked++ } elseif ($st -eq 'fail') { $c.failed++ } elseif ($st -eq 'skip') { $c.skipped++ } else { $c.notDone++ }
    }
    return [pscustomobject]$c
}

# On-screen wording for a verdict (results.md keeps the original labels).
function Get-PlainVerdict([string]$Label) {
    if ($Label -eq 'GATE PASSED') { return 'All key steps worked' }
    if ($Label -eq 'GATE FAILED') { return 'A key step did not work' }
    return 'Key steps not finished'
}

# ---------------------------------------------------------------- keys and menus

# What a typed or pressed key means: Enter (empty) gives the default; anything not in Valid gives ''.
function Resolve-MenuKey([string]$Typed, [string]$Default, [string]$Valid) {
    $t = if ($null -eq $Typed) { '' } else { $Typed.Trim().ToLower() }
    if ($t.Length -eq 0) { return $Default }
    $c = $t.Substring(0, 1)
    if ($Valid.Contains($c)) { return $c }
    return ''
}

# ---------------------------------------------------------------- step text

# Splits a step's "do" paragraph into short numbered actions. Splits only at ". " + capital/digit, and glues fragments under 4 characters on.
function Split-Actions([string]$Do) {
    $text = ($Do -replace '\s+', ' ').Trim()
    if (-not $text) { return @() }
    $parts = @([regex]::Split($text, '(?<=[.!?])\s+(?=[A-Z0-9])'))
    $out = New-Object System.Collections.ArrayList
    foreach ($p in $parts) {
        $p = $p.Trim()
        if ($p.Length -eq 0) { continue }
        if ($out.Count -gt 0 -and $p.Length -lt 4) { $out[$out.Count - 1] = $out[$out.Count - 1] + ' ' + $p } else { [void]$out.Add($p) }
    }
    return @($out.ToArray())
}

# ---------------------------------------------------------------- prerequisites

# What is missing, in plain words, with a rough time and size. HasRust / HasMsvc come from the detectors.
function Get-PrereqPlan([bool]$HasRust, [bool]$HasMsvc) {
    $items = @()
    if (-not $HasRust) {
        $items += [pscustomobject]@{ id = 'rust'; name = 'Rust'; plain = 'the programming tool Snipdeck is built with'; minutes = 5; size = 'about 300 MB'; admin = $false }
    }
    if (-not $HasMsvc) {
        $items += [pscustomobject]@{ id = 'msvc'; name = 'Visual Studio C++ Build Tools'; plain = 'Microsoft''s free tools that turn the code into a Windows program'; minutes = 20; size = 'about 3 to 6 GB'; admin = $true }
    }
    return $items
}

function Format-PrereqLines($Items) {
    $lines = @()
    foreach ($i in @($Items)) {
        $adminNote = if ($i.admin) { ' Windows will ask "Do you want to allow this app to make changes?": choose Yes.' } else { '' }
        $lines += ("{0}: {1}. Download {2}, about {3} min.{4}" -f $i.name, $i.plain, $i.size, $i.minutes, $adminNote)
    }
    return $lines
}

# ---------------------------------------------------------------- results file

# Snipdeck-test-results-2026-10-20.zip, or -2, -3 ... when that name is already taken (Existing = file names there).
function Get-ResultsZipName([datetime]$Now, [string[]]$Existing = @()) {
    $base = 'Snipdeck-test-results-' + $Now.ToString('yyyy-MM-dd', [System.Globalization.CultureInfo]::InvariantCulture)
    $taken = @($Existing | ForEach-Object { $_.ToLower() })
    $name = "$base.zip"
    $n = 2
    while ($taken -contains $name.ToLower()) { $name = "$base-$n.zip"; $n++ }
    return $name
}

# ---------------------------------------------------------------- errors

# One plain sentence for an exception the wizard did not expect.
function Get-PlainError([string]$Message) {
    $m = if ($Message) { ($Message -replace '\s+', ' ').Trim() } else { 'unknown error' }
    if ($m.Length -gt 200) { $m = $m.Substring(0, 200) + '...' }
    return "Something went wrong: $m"
}
