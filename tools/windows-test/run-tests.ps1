<#
.SYNOPSIS
  Snipdeck Windows test wizard: builds the apps to check, launches each with the right settings, walks you
  through the steps that need a person, and packs the results into one zip file on the Desktop.

.DESCRIPTION
  Start it by double-clicking "Test Snipdeck on Windows.bat" in the repo root. Test definitions live in
  tests\*.json (one file per part); adding a part means adding a JSON file, this script needs no changes.
  Pure flow logic (plans, time left, resume, result counts, file names) is in flow.ps1.
  Works on Windows PowerShell 5.1 and PowerShell 7.

.EXAMPLE
  .\run-tests.ps1                      # the recommended "v1 check", with a few plain questions
  .\run-tests.ps1 -Spike S1 -Mode quick
  .\run-tests.ps1 -NoAuto              # never run steps automatically (-Auto: always, no questions)
  .\run-tests.ps1 -SelfTest            # validate the test files, no Windows needed
#>
[CmdletBinding()]
param(
    [string[]]$Spike,
    [ValidateSet('v1', 'quick', 'standard', 'full')]
    [string]$Mode,
    [switch]$SkipPrereqs,
    [switch]$SelfTest,
    [switch]$DryRun,  # developer use: fakes Windows, build and launch so the flow can be tested anywhere
    [switch]$Auto,    # run the steps that have an "auto" field (the default for the recommended check)
    [switch]$NoAuto   # never run steps automatically: every step is manual
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [IO.Path]::GetFullPath((Join-Path (Join-Path $Here '..') '..'))
$TestsDir = Join-Path $Here 'tests'
$OutRoot = Join-Path $RepoRoot 'spikes-results'
$RunDir = Join-Path $OutRoot 'current'
$StateFile = Join-Path $RunDir 'state.json'
$IsWin = ($env:OS -eq 'Windows_NT') -or $DryRun
$LogFile = Join-Path $OutRoot 'wizard-log.txt'
. (Join-Path $Here 'flow.ps1')

# ---------------------------------------------------------------- console helpers

function Say([string]$Text = '', [string]$Color = 'Gray') { Write-Host $Text -ForegroundColor $Color }

function Get-Width {
    try { return [Math]::Max(60, [Math]::Min(100, $Host.UI.RawUI.WindowSize.Width - 4)) } catch { return 90 }
}

function Say-Wrapped([string]$Text, [int]$Indent = 2, [string]$Color = 'Gray') {
    $width = (Get-Width) - $Indent
    foreach ($para in ($Text -split "`n")) {
        $line = ''
        foreach ($word in ($para -split ' ')) {
            if (($line.Length + $word.Length + 1) -gt $width -and $line.Length -gt 0) {
                Say ((' ' * $Indent) + $line) $Color
                $line = $word
            } else {
                $line = if ($line.Length -eq 0) { $word } else { "$line $word" }
            }
        }
        Say ((' ' * $Indent) + $line) $Color
    }
}

function Read-Choice([string]$Prompt, [string]$Keys, [string]$EnterKey = '') {
    # Ctrl+C would also close the test app and lose the session, so it is turned into a normal key here.
    $oldCtrlC = $null
    try { $oldCtrlC = [Console]::TreatControlCAsInput; [Console]::TreatControlCAsInput = $true } catch { }
    try {
        $showPrompt = $true
        while ($true) {
            if ($showPrompt) {
                try { $Host.UI.RawUI.FlushInputBuffer() } catch { }
                Write-Host $Prompt -NoNewline -ForegroundColor Cyan
                $showPrompt = $false
            }
            $ch = ''
            $key = $null
            $enterPressed = $false
            try { $key = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown') } catch { $key = $null }
            if ($key) {
                if ([int]$key.Character -eq 3) {
                    Write-Host ''
                    Say-Wrapped 'Ctrl+C is turned off here (it would close the test app). To stop, press ? and then q. To copy text, select it and press Ctrl+Shift+C.' 2 Yellow
                    $showPrompt = $true
                    continue
                }
                # Shift, Ctrl, Alt, Win and arrow keys on their own have no character: skip them silently.
                if ([int]$key.Character -eq 0) { continue }
                # Pasted text arrives as a burst of keys. Ignore it so the test apps cannot trigger wizard commands.
                Start-Sleep -Milliseconds 50
                $burst = $false
                try { $burst = $Host.UI.RawUI.KeyAvailable } catch { }
                if ($burst) {
                    try { $Host.UI.RawUI.FlushInputBuffer() } catch { }
                    Write-Host ''
                    Say '  (ignored: that looked like pasted text, not a key press)' Yellow
                    $showPrompt = $true
                    continue
                }
                $ch = ([string]$key.Character).ToLower()
                $enterPressed = ($ch -eq "`r" -or $ch -eq "`n")
                if ($enterPressed -and $EnterKey) { $ch = $EnterKey }
            } else {
                $line = Read-Host
                if ($null -eq $line) { throw 'No keyboard input is available (the window was closed?).' }
                $ch = Resolve-MenuKey $line $EnterKey $Keys
            }
            if ($ch -and $Keys.Contains($ch)) { if ($key -and $enterPressed) { Write-Host '' } elseif ($key) { Write-Host $ch }; return $ch }
            Write-Host ''
            $showPrompt = $true
        }
    } finally {
        try { if ($null -ne $oldCtrlC) { [Console]::TreatControlCAsInput = $oldCtrlC } } catch { }
    }
}

function Ask-YesNo([string]$Question, [bool]$Default = $true) {
    $hint = if ($Default) { '(Y/n)' } else { '(y/N)' }
    $enter = if ($Default) { 'y' } else { 'n' }
    return ((Read-Choice "  $Question $hint " 'yn' $enter) -eq 'y')
}

function Pause-Enter([string]$Text = '  Press Enter to continue...') {
    [void](Read-Choice $Text 'x' 'x')
}

function Write-Banner([string]$Title, [string]$Sub = '') {
    Clear-Host
    Say ('=' * (Get-Width)) DarkCyan
    Say "  $Title" White
    if ($Sub) { Say "  $Sub" Gray }
    Say ('=' * (Get-Width)) DarkCyan
}

# The same header on every step screen: "Step 3 of 35 - about 40 min left", a text progress bar, which part we are in.
function Write-StepHeader([int]$Index, [int]$Total, [double]$MinutesLeft, [string]$Section, [string]$Small = '', [string]$Tag = '') {
    Clear-Host
    Say ('=' * (Get-Width)) DarkCyan
    $tagText = if ($Tag) { "   [$Tag]" } else { '' }
    Say ('  ' + (Format-StepHeader $Index $Total $MinutesLeft) + $tagText) White
    Say ('  ' + (Format-ProgressBar ($Index - 1) $Total)) Cyan
    Say "  $Section" Gray
    if ($Small) { Say "  $Small" DarkGray }
    Say ('=' * (Get-Width)) DarkCyan
}

# A small log for problems, so an error message can say where to look.
function Write-WizardLog([string]$Text) {
    try {
        if (-not (Test-Path $OutRoot)) { New-Item -ItemType Directory -Path $OutRoot | Out-Null }
        Add-Content -Path $LogFile -Value ("{0}  {1}" -f (Get-Date).ToString('s'), $Text) -Encoding UTF8
    } catch { }
}

function Write-Utf8([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, (New-Object System.Text.UTF8Encoding $false))
}

# ---------------------------------------------------------------- test definitions

function Load-Spikes {
    $list = @()
    foreach ($f in (Get-ChildItem -Path $TestsDir -Filter '*.json' | Sort-Object Name)) {
        $s = Get-Content -Raw -Path $f.FullName | ConvertFrom-Json
        $list += $s
    }
    return $list
}

function Test-Definitions($Spikes) {
    $problems = @()
    foreach ($s in $Spikes) {
        foreach ($field in 'id', 'title', 'dir', 'exe', 'tests') {
            if (-not $s.$field) { $problems += "$($s.id): missing '$field'" }
        }
        $seen = @{}
        foreach ($t in $s.tests) {
            foreach ($field in 'id', 'title', 'do', 'expect') {
                if (-not $t.$field) { $problems += "$($s.id):$($t.id): missing '$field'" }
            }
            if ($seen.ContainsKey($t.id)) { $problems += "$($s.id): duplicate test id '$($t.id)'" }
            $seen[$t.id] = $true
            if ($t.os -and @('any', 'win10', 'win11') -notcontains $t.os) { $problems += "$($s.id):$($t.id): bad os '$($t.os)'" }
            if ($null -ne $t.v1 -and $t.v1 -isnot [bool]) { $problems += "$($s.id):$($t.id): 'v1' must be true or false" }
            if ($t.auto -and ($t.auto -isnot [string] -or $t.auto -notmatch '^[A-Za-z][A-Za-z0-9-]*$')) { $problems += "$($s.id):$($t.id): bad auto '$($t.auto)'" }
        }
        if (-not (Test-Path (Join-Path $RepoRoot $s.dir))) { $problems += "$($s.id): folder '$($s.dir)' not found" }
    }
    return $problems
}

function Get-Applicable($SpikeDef, [string]$OsClass, [string]$RunMode) {
    $out = @()
    foreach ($t in $SpikeDef.tests) { if (Test-StepInMode $t $RunMode $OsClass) { $out += $t } }
    return , $out
}

function Get-Verdict($SpikeDef, $Tests, $Results) {
    $failed = @(); $missing = @(); $groups = @{}
    foreach ($t in $Tests) {
        $key = "$($SpikeDef.id):$($t.id)"
        $st = if ($Results.Contains($key)) { $Results[$key].status } else { 'none' }
        if ($t.anyGroup) {
            if (-not $groups.ContainsKey($t.anyGroup)) { $groups[$t.anyGroup] = @() }
            $groups[$t.anyGroup] += $st
        } elseif ($t.gate -eq $true) {
            if ($st -eq 'fail') { $failed += $t.id } elseif ($st -ne 'pass') { $missing += $t.id }
        }
    }
    foreach ($g in $groups.Keys) {
        $states = $groups[$g]
        if ($states -notcontains 'pass') {
            if ($states -contains 'fail') { $failed += "any of '$g'" } else { $missing += "any of '$g'" }
        }
    }
    if ($failed.Count -gt 0) { return @{ label = 'GATE FAILED'; detail = "failed: $($failed -join ', ')"; color = 'Red' } }
    if ($missing.Count -gt 0) { return @{ label = 'INCOMPLETE'; detail = "not passed yet: $($missing -join ', ')"; color = 'Yellow' } }
    return @{ label = 'GATE PASSED'; detail = 'all go/no-go steps passed'; color = 'Green' }
}

# ---------------------------------------------------------------- environment + prerequisites

function Get-EnvInfo {
    $info = [ordered]@{ os = 'unknown'; build = 0; gpu = ''; monitors = 0; dpi = ''; appsTheme = ''; textScale = ''; rust = ''; commit = '' }
    if ($DryRun) { $info.os = 'DryRun OS'; $info.build = 22631; $info.gpu = 'Fake GPU'; $info.monitors = 1; $info.dpi = 96; $info.appsTheme = 'light'; $info.textScale = 100; $info.rust = 'rustc dry-run'; $info.commit = 'dry'; return $info }
    if (-not $IsWin) { return $info }
    try {
        $os = Get-CimInstance Win32_OperatingSystem
        $info.os = $os.Caption; $info.build = [int]$os.BuildNumber
        $info.gpu = (Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }) -join '; '
    } catch { }
    try {
        Add-Type -AssemblyName System.Windows.Forms
        $info.monitors = [System.Windows.Forms.Screen]::AllScreens.Count
    } catch { }
    try { $info.dpi = (Get-ItemProperty 'HKCU:\Control Panel\Desktop\WindowMetrics' -ErrorAction Stop).AppliedDPI } catch { }
    try {
        $light = (Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Themes\Personalize' -ErrorAction Stop).AppsUseLightTheme
        $info.appsTheme = if ($light -eq 0) { 'dark' } else { 'light' }
    } catch { }
    try { $info.textScale = (Get-ItemProperty 'HKCU:\SOFTWARE\Microsoft\Accessibility' -ErrorAction Stop).TextScaleFactor } catch { }
    if (Get-Command rustc -ErrorAction SilentlyContinue) { try { $info.rust = (& rustc --version) } catch { } }
    if (Get-Command git -ErrorAction SilentlyContinue) { try { $info.commit = (& git -C $RepoRoot rev-parse --short HEAD) } catch { } }
    return $info
}

function Add-CargoToPath {
    if (-not $env:USERPROFILE) { return }
    $bin = Join-Path $env:USERPROFILE '.cargo\bin'
    if ((Test-Path $bin) -and ($env:PATH -notlike "*$bin*")) { $env:PATH = "$bin;$env:PATH" }
}

function Install-WithWinget([string]$Id, [string[]]$Extra = @()) {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Say '  Windows'' app installer (winget) is not on this PC, so this cannot be installed automatically.' Yellow
        Say-Wrapped 'Install it by hand (Rust: https://rustup.rs  |  Build Tools: https://visualstudio.microsoft.com/downloads/, choose "Desktop development with C++"), then double-click the file again.' 2 Yellow
        return $false
    }
    # Direct call: Start-Process -Wait would also wait for any helper processes the installer leaves running.
    & winget install --id $Id -e --accept-source-agreements --accept-package-agreements @Extra
    return ($LASTEXITCODE -eq 0)
}

function Test-MsvcTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $false }
    $path = & $vswhere -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -latest -property installationPath
    return [bool]$path
}

function Test-HasRust {
    Add-CargoToPath
    return [bool](Get-Command cargo -ErrorAction SilentlyContinue)
}

function Invoke-Prereqs {
    Write-Banner 'Getting ready' 'Checking the tools needed to build Snipdeck'
    if ($DryRun) { Say '  [dry run] skipping tool checks' DarkGray; return $true }
    if (-not $IsWin) {
        Say-Wrapped 'This is not Windows. The test apps only run on Windows; you can still read the tests (-SelfTest).' 2 Yellow
        return $false
    }
    Say ''
    $hasRust = Test-HasRust
    $hasMsvc = Test-MsvcTools
    if ($hasRust) { Say '  [OK]      Rust is installed.' Green } else { Say '  [MISSING] Rust' Yellow }
    if ($hasMsvc) { Say '  [OK]      Visual Studio C++ Build Tools are installed.' Green } else { Say '  [MISSING] Visual Studio C++ Build Tools' Yellow }
    Say ''

    $need = @(Get-PrereqPlan $hasRust $hasMsvc)
    if ($need.Count -gt 0) {
        Say '  Snipdeck is built on this PC, and that needs:' White
        foreach ($line in (Format-PrereqLines $need)) { Say-Wrapped "- $line" 4 Gray }
        $mins = 0; foreach ($n in $need) { $mins += $n.minutes }
        Say ''
        Say-Wrapped "In total this takes $(Format-Duration $mins) and only has to be done once. Nothing else on your PC is changed." 2 Gray
        Say ''
        if (-not (Ask-YesNo 'Install now?')) {
            Say '  Okay. The check cannot run without them. Run this file again when you are ready.' Yellow
            Pause-Enter
            return $false
        }
        foreach ($n in $need) {
            Say ''
            if ($n.id -eq 'rust') {
                Say "  Installing Rust ($($n.size), $(Format-Duration $n.minutes))..." Cyan
                if (Install-WithWinget 'Rustlang.Rustup') { Add-CargoToPath } else { Write-WizardLog 'winget install Rustlang.Rustup failed' }
            } else {
                Say "  Installing the C++ Build Tools ($($n.size), $(Format-Duration $n.minutes))." Cyan
                Say-Wrapped 'A Windows permission window will appear: choose Yes. Then wait; the window below may sit quiet for a while, that is normal.' 2 Gray
                $extra = @('--override', '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
                if (-not (Install-WithWinget 'Microsoft.VisualStudio.2022.BuildTools' $extra)) { Write-WizardLog 'winget install BuildTools failed' }
            }
        }
        Say ''
    }

    $ok = $true
    if (-not (Test-HasRust)) {
        Say-Wrapped 'Rust is still not found. If it was just installed, close this window and double-click the file again so Windows picks it up.' 2 Red
        $ok = $false
    }
    if (-not (Test-MsvcTools)) {
        Say-Wrapped 'The C++ Build Tools are still not found. Finish their installation, then double-click the file again.' 2 Red
        $ok = $false
    }
    if ($ok -and (Get-Command rustc -ErrorAction SilentlyContinue)) {
        $hostLine = (& rustc -vV | Select-String '^host:').ToString()
        if ($hostLine -notmatch 'msvc') {
            Say '  Rust is set up for a different kind of Windows build than Snipdeck uses.' Yellow
            if (Ask-YesNo 'Switch Rust to the right one now?') { & rustup default stable-x86_64-pc-windows-msvc }
        }
        Say "  Ready. ($(& rustc --version))" Green
    }
    if (-not $ok) { Pause-Enter }
    return $ok
}

# ---------------------------------------------------------------- running the app under test

$script:App = $null          # @{ proc; sig; spike; logOut; logErr }
$script:LaunchN = 0

function Normalize-Launch($Launch) {
    $argList = @(); if ($Launch -and $Launch.args) { $argList = @($Launch.args) }
    $envMap = [ordered]@{}
    if ($Launch -and $Launch.env) { foreach ($p in $Launch.env.PSObject.Properties) { $envMap[$p.Name] = [string]$p.Value } }
    $elevated = $false; if ($Launch -and $Launch.elevated) { $elevated = [bool]$Launch.elevated }
    return [pscustomobject]@{ argList = $argList; envMap = $envMap; elevated = $elevated }
}

function Get-Sig($Norm) { return ($Norm | ConvertTo-Json -Compress -Depth 4) }

function Describe-Launch($Norm) {
    $parts = @()
    if ($Norm.argList.Count -gt 0) { $parts += "args: $($Norm.argList -join ' ')" }
    foreach ($k in $Norm.envMap.Keys) { $parts += "$k=$($Norm.envMap[$k])" }
    if ($Norm.elevated) { $parts += 'as administrator' }
    if ($parts.Count -eq 0) { return 'default settings' }
    return ($parts -join '; ')
}

function Stop-App {
    if ($script:App -and $script:App.proc) {
        try { if (-not $script:App.proc.HasExited) { $script:App.proc.Kill() } } catch {
            Say '  (could not close the test app automatically: close its window yourself)' Yellow
        }
    }
    $script:App = $null
}

function Test-AppRunning { return ($script:App -and $script:App.proc -and -not $script:App.proc.HasExited) }

function Read-LogTail([string]$Path, [int]$Lines = 15) {
    if (-not $Path -or -not (Test-Path $Path)) { return @() }
    try {
        $fs = [IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
        $sr = New-Object IO.StreamReader $fs
        $all = $sr.ReadToEnd(); $sr.Close(); $fs.Close()
        $arr = @($all -split "`r?`n" | Where-Object { $_ -ne '' })
        if ($arr.Count -gt $Lines) { $arr = $arr[($arr.Count - $Lines)..($arr.Count - 1)] }
        return $arr
    } catch { return @() }
}

function Start-App($SpikeDef, $Norm) {
    Stop-App
    $dir = Join-Path $RepoRoot $SpikeDef.dir
    if ($DryRun) {
        $fake = [pscustomobject]@{ HasExited = $false; Id = 4242; ExitCode = 0 }
        Add-Member -InputObject $fake -MemberType ScriptMethod -Name Kill -Value { $this.HasExited = $true }
        $script:LaunchN++
        $script:App = @{ proc = $fake; sig = (Get-Sig $Norm); norm = $Norm; spike = $SpikeDef.id; logOut = ''; logErr = ''; label = (Describe-Launch $Norm) }
        if ($SpikeDef.captureLines) { $script:State.diag["$($SpikeDef.id) launch $($script:LaunchN) ($(Describe-Launch $Norm))"] = @($SpikeDef.captureLines[0] + ' (dry-run sample)') }
        return
    }
    $exe = Join-Path $dir ("target\release\" + $SpikeDef.exe + ".exe")
    if (-not (Test-Path $exe)) { Say "  Cannot find ${exe} - build it first." Red; return }
    $script:LaunchN++
    $logOut = Join-Path $RunDir ("{0}-launch{1}.out.log" -f $SpikeDef.id, $script:LaunchN)
    $logErr = Join-Path $RunDir ("{0}-launch{1}.err.log" -f $SpikeDef.id, $script:LaunchN)

    # Clear any knob variables left in this shell, then apply the test's own.
    $saved = @{}
    $names = @('SLINT_BACKEND') + @(Get-ChildItem Env: | Where-Object { $_.Name -like 'S4_*' } | ForEach-Object { $_.Name }) + @($Norm.envMap.Keys)
    foreach ($n in ($names | Select-Object -Unique)) {
        $saved[$n] = [Environment]::GetEnvironmentVariable($n, 'Process')
        [Environment]::SetEnvironmentVariable($n, $null, 'Process')
    }
    foreach ($k in $Norm.envMap.Keys) { [Environment]::SetEnvironmentVariable($k, $Norm.envMap[$k], 'Process') }

    try {
        $splat = @{ FilePath = $exe; WorkingDirectory = $dir; PassThru = $true }
        if ($Norm.argList.Count -gt 0) { $splat.ArgumentList = $Norm.argList }
        if ($Norm.elevated) {
            $splat.Verb = 'RunAs'
            Say '  Accept the administrator (UAC) prompt...' Yellow
        } else {
            $splat.RedirectStandardOutput = $logOut
            $splat.RedirectStandardError = $logErr
        }
        $proc = Start-Process @splat
        $null = $proc.Handle   # keeps the exit code readable after the process ends
        $script:App = @{ proc = $proc; sig = (Get-Sig $Norm); norm = $Norm; spike = $SpikeDef.id; logOut = $logOut; logErr = $logErr; label = (Describe-Launch $Norm) }
    } catch {
        Say "  Could not start the app: $($_.Exception.Message)" Red
    } finally {
        foreach ($n in $saved.Keys) { [Environment]::SetEnvironmentVariable($n, $saved[$n], 'Process') }
    }

    Start-Sleep -Milliseconds 1800
    if ($script:App -and $script:App.proc.HasExited) {
        Say "  The app exited right away (code $($script:App.proc.ExitCode)). Last output:" Red
        foreach ($l in ((Read-LogTail $logErr 8) + (Read-LogTail $logOut 8))) { Say "    $l" DarkGray }
    }
    # Pull the diagnostic lines the spike's notes ask us to save.
    if ($SpikeDef.captureLines -and $script:State) {
        $found = @()
        foreach ($l in ((Read-LogTail $logOut 400) + (Read-LogTail $logErr 400))) {
            foreach ($needle in $SpikeDef.captureLines) { if ($l -like "*$needle*") { $found += $l.Trim() } }
        }
        if ($found.Count -gt 0) {
            $script:State.diag["$($SpikeDef.id) launch $($script:LaunchN) ($(Describe-Launch $Norm))"] = $found | Select-Object -Unique -First 12
        }
    }
}

# ---------------------------------------------------------------- build

function Build-Spike($SpikeDef, [int]$N = 1, [int]$Of = 1) {
    $dir = Join-Path $RepoRoot $SpikeDef.dir
    if ($DryRun) { Say "  [dry run] would build $($SpikeDef.id)" DarkGray; return $true }
    while ($true) {
        Say ''
        Say "  Building the app, part $N of ${Of}: $(Get-FriendlyName $SpikeDef)" White
        Say-Wrapped 'The first time takes about 5-10 minutes. Lines will scroll past, and a "still building" note appears every 30 seconds. Nothing is stuck.' 2 Gray
        Say-Wrapped '(If the window ever stops moving for good, click into it and press Enter: Windows pauses a window while text in it is selected.)' 2 DarkGray
        $started = Get-Date
        $code = -1
        Push-Location $dir
        try {
            # Not Start-Process -Wait: the MSVC linker leaves mspdbsrv.exe running and -Wait would wait for it too.
            # Waiting on the cargo process itself, 30 s at a time, lets us print a sign of life.
            # A release build refuses to compile without a licence key. Test builds use the published
            # throwaway test key (licence/key_selection.rs), set for this wizard process only.
            if (-not $env:SNIPDECK_LICENCE_PUBLIC_KEY) {
                $env:SNIPDECK_LICENCE_PUBLIC_KEY = 'Zr5+Myx6RTMyvZ0Kf32wVfXF7xoGraZtmLOftoEMRzo='
                $env:SNIPDECK_ALLOW_TEST_LICENCE_KEY = '1'
            }
            $proc = Start-Process -FilePath 'cargo' -ArgumentList @('build', '--release') -NoNewWindow -PassThru
            $null = $proc.Handle   # keeps the exit code readable after the process ends
            while (-not $proc.WaitForExit(30000)) {
                $mins = [int][Math]::Floor(((Get-Date) - $started).TotalMinutes)
                Say "  ... still building (about $mins min so far, this is normal)" Gray
            }
            $code = $proc.ExitCode
        } catch {
            Say "  Could not start the build: $($_.Exception.Message)" Red
            Write-WizardLog "cargo start failed: $($_.Exception.Message)"
        } finally { Pop-Location }
        if ($code -eq 0) { Say '  Build finished.' Green; return $true }
        Write-WizardLog "build failed for $($SpikeDef.id), exit code $code"
        Say "  The build did not finish (exit code $code). Scroll up to see why." Red
        Say '  If the message mentions link.exe or Visual Studio, the C++ Build Tools are not set up yet.' Gray
        $c = Read-Choice '  R = try again   S = skip this part   Q = stop  > ' 'rsq'
        if ($c -eq 'r') { continue }
        if ($c -eq 'q') { throw 'Stopped at your request.' }
        return $false
    }
}

# Builds every part of the run up front, so the long wait happens once and the steps never stall. Returns the ids that failed.
function Build-AllSpikes($Spikes, $Queue, $Results) {
    $todo = @()
    foreach ($s in $Spikes) {
        $open = @($Queue | Where-Object { $_.SpikeId -eq $s.id -and -not (Test-StepAnswered $Results $_.Key) })
        if ($open.Count -gt 0) { $todo += $s }
    }
    $failed = @()
    $n = 0
    foreach ($s in $todo) {
        $n++
        if (-not (Build-Spike $s $n $todo.Count)) { $failed += $s.id }
    }
    return $failed
}

# ---------------------------------------------------------------- screenshots

function Save-Screenshot([string]$Name, [int]$Delay = 5, [bool]$Quiet = $false) {
    if ($DryRun) { Say '  [dry run] would take a screenshot' DarkGray; return (Join-Path $RunDir "screens\$Name-dryrun.png") }
    if (-not $IsWin) { Say '  Screenshots only work on Windows.' Yellow; return $null }
    return (Save-ScreenshotNative $Name $Delay $Quiet)
}

# Kept apart so the dry run (and other systems) never compile the Windows-only type references.
function Save-ScreenshotNative([string]$Name, [int]$Delay, [bool]$Quiet) {
    try {
        if (-not ([System.Management.Automation.PSTypeName]'SnipTest.Win').Type) {
            Add-Type -Namespace SnipTest -Name Win -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
[DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr h, int n);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
'@
        }
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        [void][SnipTest.Win]::SetProcessDPIAware()
        $dir = Join-Path $RunDir 'screens'
        if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
        $file = Join-Path $dir ("{0}-{1}.png" -f $Name, (Get-Date -Format 'HHmmss'))

        if ($Quiet) { Say "  Taking a screenshot in $Delay seconds. This window hides for a moment..." Yellow }
        else { Say "  Hiding this window. Bring the test app to the front: capturing in $Delay seconds..." Yellow }
        $h = [SnipTest.Win]::GetConsoleWindow()
        [void][SnipTest.Win]::ShowWindowAsync($h, 6)
        Start-Sleep -Seconds $Delay
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
        $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        [void][SnipTest.Win]::ShowWindowAsync($h, 9)
        [void][SnipTest.Win]::SetForegroundWindow($h)
        Say "  Screenshot saved: $file" Green
        return $file
    } catch {
        Say "  Screenshot failed: $($_.Exception.Message)" Red
        return $null
    }
}

# ---------------------------------------------------------------- state + report

function New-State([string]$RunMode, [string]$OsClass, $EnvInfo, $SpikeIds, [bool]$AutoOn = $true) {
    return [ordered]@{
        started = (Get-Date).ToString('s'); mode = $RunMode; osClass = $OsClass; env = $EnvInfo
        spikes = @($SpikeIds); auto = $AutoOn; results = [ordered]@{}; diag = [ordered]@{}
    }
}

function Save-State {
    if (-not $script:State) { return }
    Write-Utf8 $StateFile ($script:State | ConvertTo-Json -Depth 8)
}

function Load-State {
    $raw = Get-Content -Raw -Path $StateFile | ConvertFrom-Json
    $res = [ordered]@{}
    foreach ($p in $raw.results.PSObject.Properties) {
        $v = $p.Value
        $res[$p.Name] = @{ status = $v.status; note = $v.note; shots = @($v.shots); at = $v.at }
        if ($v.auto) { $res[$p.Name].auto = $true }
    }
    $diag = [ordered]@{}
    if ($raw.diag) { foreach ($p in $raw.diag.PSObject.Properties) { $diag[$p.Name] = @($p.Value) } }
    $envInfo = [ordered]@{}
    foreach ($p in $raw.env.PSObject.Properties) { $envInfo[$p.Name] = $p.Value }
    $autoOn = $true; if ($null -ne $raw.auto) { $autoOn = [bool]$raw.auto }
    return [ordered]@{
        started = $raw.started; mode = $raw.mode; osClass = $raw.osClass; env = $envInfo
        spikes = @($raw.spikes); auto = $autoOn; results = $res; diag = $diag
    }
}

function Build-OneLine($Spikes, $State) {
    # One line, no line breaks: chat boxes often keep only the first line of a multi-line paste.
    $e = $State.env
    $counts = Get-ResultCounts (Get-RunQueue $Spikes $State.spikes $State.mode $State.osClass) $State.results
    $parts = @("Snipdeck Windows test results ($($State.mode), $($State.osClass))", "$($counts.worked) worked / $($counts.failed) failed / $($counts.skipped) skipped / $($counts.notDone) not done", "$($e.os) build $($e.build)", "GPU $($e.gpu)")
    $verdicts = @(); $others = @()
    foreach ($s in $Spikes) {
        if ($State.spikes -notcontains $s.id) { continue }
        $tests = Get-Applicable $s $State.osClass $State.mode
        $v = Get-Verdict $s $tests $State.results
        $verdicts += "$($s.id) $($v.label) ($($v.detail))"
        foreach ($t in $tests) {
            $key = "$($s.id):$($t.id)"
            if (-not $State.results.Contains($key)) { continue }
            $r = $State.results[$key]
            if ($r.status -eq 'pass' -and -not $r.note) { continue }
            $note = if ($r.note) { " '" + (($r.note -replace "`r?`n", ' ') -replace '\|', '/') + "'" } else { '' }
            $others += "$key $($r.status.ToUpper())$note"
        }
    }
    $line = ($parts -join ' | ') + ' | ' + ($verdicts -join '; ')
    if ($others.Count -gt 0) { $line += ' | steps: ' + ($others -join '; ') }
    $line = ($line -replace '\s+', ' ').Trim()
    if ($line.Length -gt 3000) { $line = $line.Substring(0, 2990) + ' ...(cut)' }
    return $line
}

function Build-Report($Spikes, $State, [bool]$ForPaste) {
    $sb = New-Object System.Text.StringBuilder
    $anyAuto = $false
    $e = $State.env
    $counts = Get-ResultCounts (Get-RunQueue $Spikes $State.spikes $State.mode $State.osClass) $State.results
    [void]$sb.AppendLine('# Snipdeck: Windows test results')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine("- Worked: $($counts.worked)  |  Failed: $($counts.failed)  |  Skipped: $($counts.skipped)  |  Not done: $($counts.notDone)")
    [void]$sb.AppendLine("- Date: $($State.started)  |  Mode: $($State.mode)  |  Steps run as: $($State.osClass)")
    [void]$sb.AppendLine("- OS: $($e.os) (build $($e.build))  |  GPU: $($e.gpu)")
    [void]$sb.AppendLine("- Monitors: $($e.monitors)  |  DPI: $($e.dpi)  |  Apps theme: $($e.appsTheme)  |  Text scale: $($e.textScale)")
    [void]$sb.AppendLine("- $($e.rust)  |  repo commit: $($e.commit)")
    [void]$sb.AppendLine('')
    foreach ($s in $Spikes) {
        if ($State.spikes -notcontains $s.id) { continue }
        $tests = Get-Applicable $s $State.osClass $State.mode
        $v = Get-Verdict $s $tests $State.results
        [void]$sb.AppendLine("## $($s.id): $($s.title)")
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine("**Result: $($v.label)** ($($v.detail))")
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine('| Step | Result | Title | Notes |')
        [void]$sb.AppendLine('|---|---|---|---|')
        foreach ($t in $tests) {
            $key = "$($s.id):$($t.id)"
            $r = if ($State.results.Contains($key)) { $State.results[$key] } else { $null }
            $st = if ($r) { $r.status } else { 'not run' }
            if ($ForPaste -and $st -eq 'pass' -and -not ($r -and $r.note)) { continue }
            $note = if ($r -and $r.note) { ($r.note -replace '\|', '/' -replace "`r?`n", ' ') } else { '' }
            if ($r -and $r.auto) { $st = "$st (auto)"; $anyAuto = $true }
            [void]$sb.AppendLine("| $($t.id) | $st | $($t.title) | $note |")
        }
        if ($anyAuto) {
            [void]$sb.AppendLine('')
            [void]$sb.AppendLine('(auto) = decided by the automation from what it observed (the evidence is in Notes).')
            $anyAuto = $false
        }
        [void]$sb.AppendLine('')
        foreach ($k in $State.diag.Keys) {
            if ($k -like "$($s.id) *") {
                [void]$sb.AppendLine("Diagnostics, $($k):")
                [void]$sb.AppendLine('```')
                foreach ($l in $State.diag[$k]) { [void]$sb.AppendLine($l) }
                [void]$sb.AppendLine('```')
            }
        }
    }
    if ($ForPaste) { [void]$sb.AppendLine('(Passing steps without notes are omitted. The full report and screenshots are in the results zip.)') }
    return $sb.ToString()
}

# ---------------------------------------------------------------- automation (see auto\ and README.md)

$AutoDir = Join-Path $Here 'auto'
$script:AutoOn = $false
$script:Queue = @()          # every step of this run, in order (see Get-RunQueue)

# auto\*.ps1 (helpers, Windows primitives, per-spike steps); with -DryRun the mock primitives load last and replace the real ones.
function Get-AutoModuleFiles([bool]$Mock) {
    $files = @(Get-ChildItem -Path $AutoDir -Filter '*.ps1' | Sort-Object Name | ForEach-Object { $_.FullName })
    if ($Mock) { $files += (Join-Path (Join-Path $AutoDir 'mock') 'primitives.mock.ps1') }
    return $files
}

function Test-HasAutoSteps($Queue, $Results) {
    foreach ($item in @($Queue)) {
        if ($item.Test.auto -and -not (Test-StepAnswered $Results $item.Key)) { return $true }
    }
    return $false
}

function Ask-AutoOptIn {
    Say ''
    Say-Wrapped 'Some steps can be done and checked for you: the wizard moves the mouse, types into a scratch Notepad document, switches a Windows setting for a few seconds (always switched back) and reads what happened. A 5-second countdown comes first. To stop at any time: hold Esc, or move the mouse. Anything it cannot judge falls back to a normal step for you.' 2
    Say ''
    return (Ask-YesNo 'Let the wizard do those steps for you? Your mouse and keyboard will move.' $true)
}

function Start-Automation {
    $init = Initialize-AutoPrimitives $OutRoot
    if (-not $init.Ok) {
        Say "  Automatic steps are not available here ($($init.Reason)). You will do every step yourself." Yellow
        Pause-Enter
        return $false
    }
    try { $script:AutoState.WizardHwnd = [long](Get-AutoForeground).Hwnd } catch { }
    return $true
}

function New-AutoCtx($SpikeDef, $T) {
    return [pscustomobject]@{
        SpikeId = $SpikeDef.id; TestId = $T.id; Spike = $SpikeDef; Test = $T; App = $script:App
        RunDir = $RunDir; RepoRoot = $RepoRoot; OsBuild = [int]$script:State.env.build; OsClass = $script:State.osClass
    }
}

# Runs the step's automation, asks the owner's one-key confirmation if it has one, and shows the outcome.
function Invoke-EngineAuto([int]$QIndex) {
    $item = $script:Queue[$QIndex]; $SpikeDef = $item.Spike; $T = $item.Test
    Write-QueueHeader $QIndex 'automatic'
    Say ''
    Say "  $($T.title)" White
    Say ''
    Say '  The wizard is doing this step for you. Hands off the mouse and keyboard; to stop, hold Esc.' Gray
    Say ''
    $timeout = 60
    if ($T.autoTimeout) { $timeout = [int]$T.autoTimeout }
    $ctx = New-AutoCtx $SpikeDef $T
    try {
        Register-AutoApp $ctx
        $ar = Invoke-AutoStep $ctx $T.auto $timeout
    } catch {
        $ar = @{ status = 'unsure'; evidence = "automation error: $($_.Exception.Message)"; screenshots = @(); manualCheck = '' }
    }
    if ($script:AutoState.Aborted) {
        $script:AutoOn = $false
        Say '  Automatic steps stopped. You will do every remaining step yourself.' Yellow
    }
    if (-not $script:AutoState.FocusReturned) { Say '  Click this window to continue.' Yellow }
    Say ''
    if ($ar.status -eq 'pass' -and $ar.manualCheck) {
        Say '  The automatic checks passed. One thing only you can judge:' Cyan
        Say-Wrapped $ar.manualCheck 4 Gray
        if (Ask-YesNo 'Is that right?' $true) { $ar.evidence = "$($ar.evidence); you confirmed: $($ar.manualCheck)" }
        else { $ar.status = 'unsure'; $ar.evidence = "$($ar.evidence); you said NO to: $($ar.manualCheck)" }
    }
    $label = @{ pass = 'PASSED (it worked)'; fail = 'FAILED (it did not work)'; unsure = 'NOT SURE (you decide on the next screen)' }[$ar.status]
    $color = @{ pass = 'Green'; fail = 'Red'; unsure = 'Yellow' }[$ar.status]
    Say "  Automatic result: $label" $color
    Say-Wrapped $ar.evidence 4 DarkGray
    if ($ar.status -eq 'fail') { Pause-Enter '  Saved as "did not work". Press Enter to continue (on the next screen, ? then b brings you back to change it)...' }
    elseif ($ar.status -eq 'pass' -and -not $DryRun) { Start-Sleep -Milliseconds 1500 }
    return $ar
}

function Save-AutoResult([string]$Key, $Ar, $Existing) {
    $shots = @()
    if ($Existing) { $shots = @($Existing.shots) }
    $shots = @(@($shots) + @($Ar.screenshots) | Where-Object { $_ })
    $note = "auto: $($Ar.evidence)"
    if ($note.Length -gt 900) { $note = $note.Substring(0, 900) + '...' }
    $script:State.results[$Key] = @{ status = $Ar.status; note = $note; shots = $shots; at = (Get-Date).ToString('s'); auto = $true }
    Save-State
}

# ---------------------------------------------------------------- one test screen

# Same header everywhere: step N of M, time left, progress bar, which part we are in (the technical id stays small and grey).
function Write-QueueHeader([int]$QIndex, [string]$Tag = '') {
    $item = $script:Queue[$QIndex]
    $left = Get-MinutesLeft $script:Queue $script:State.results $QIndex $script:AutoOn
    $small = "($($item.SpikeId) step $($item.Test.id))"
    Write-StepHeader ($QIndex + 1) @($script:Queue).Count $left (Get-FriendlyName $item.Spike) $small $Tag
}

# Wrapped text with a hanging indent: "  1. first line" then continuation lines under the text.
function Say-Hanging([string]$Prefix, [string]$Text, [string]$Color = 'Gray') {
    $width = (Get-Width) - $Prefix.Length
    $pad = ' ' * $Prefix.Length
    $line = ''; $first = $true
    foreach ($word in ($Text -split ' ')) {
        if (($line.Length + $word.Length + 1) -gt $width -and $line.Length -gt 0) {
            $lead = $pad; if ($first) { $lead = $Prefix }
            Say ($lead + $line) $Color
            $first = $false; $line = $word
        } else {
            $line = if ($line.Length -eq 0) { $word } else { "$line $word" }
        }
    }
    $lead = $pad; if ($first) { $lead = $Prefix }
    Say ($lead + $line) $Color
}

function Show-Test([int]$QIndex, $Existing, $AutoInfo = $null, [bool]$More = $false) {
    $item = $script:Queue[$QIndex]; $T = $item.Test
    $isKey = ($T.gate -eq $true -or $T.anyGroup)
    $tag = ''; if ($isKey) { $tag = 'key step' }
    Write-QueueHeader $QIndex $tag
    Say ''
    Say "  $($T.title)" White
    Say ''
    Say '  DO THIS:' Cyan
    $n = 0
    foreach ($action in (Split-Actions $T.do)) { $n++; Say-Hanging ("    {0}. " -f $n) $action Gray }
    Say ''
    Say '  YOU SHOULD SEE:' Cyan
    Say-Wrapped $T.expect 4 Gray
    Say ''
    if ($T.clipboard) { Say "  Your clipboard already holds the word '$($T.clipboard)' for this step." Green }
    if (Test-AppRunning) { Say "  The test app is open. ($($script:App.label), PID $($script:App.proc.Id))" DarkGray }
    else { Say '  The test app is not open. Press ? and then r to start it.' Yellow }
    if ($Existing) { Say "  Your earlier answer: $($Existing.status)  $($Existing.note)" Yellow }
    if ($AutoInfo) {
        Say "  The automatic check could not decide this one ($($AutoInfo.status)). What it saw:" Yellow
        Say-Wrapped $AutoInfo.evidence 4 Gray
    }
    Say ''
    Say '  P = it worked      F = it didn''t      S = skip      ? = more options' White
    if ($More) {
        Say '    N = add a note              C = take a screenshot' Gray
        Say '    L = show what the app printed   R = restart the app' Gray
        Say '    B = go back one step        Q = stop here (continue later)' Gray
        if ($T.clipboard) { Say '    K = put the clipboard text back' Gray }
        if ($script:AutoOn -and $T.auto) { Say '    A = let the wizard try this step again' Gray }
        Say-Wrapped 'Ctrl+C is turned off here. To copy text from this window: select it, then right-click.' 4 DarkGray
    }
}

# Shows one part's setup checklist the first time we reach it. Returns $false when the person skips the part.
function Show-PartIntro($SpikeDef, [int]$PartNo, [int]$PartCount, [int]$StepCount) {
    Write-Banner "Part $PartNo of ${PartCount}: $(Get-FriendlyName $SpikeDef)" "$StepCount steps to do in this part"
    Say "  ($($SpikeDef.id))" DarkGray
    Say ''
    Say '  Before you start:' Cyan
    foreach ($line in $SpikeDef.setup) { Say-Hanging '    - ' $line Gray }
    Say ''
    $c = Read-Choice '  Press Enter to start this part, or S to skip all of it > ' 'gs' 'g'
    return ($c -eq 'g')
}

function Set-StepResult([string]$Key, [string]$Status, [string]$Note, $Shots) {
    $script:State.results[$Key] = @{ status = $Status; note = $Note; shots = @($Shots | Where-Object { $_ }); at = (Get-Date).ToString('s') }
    Save-State
}

function Run-Queue($FailedBuilds) {
    $State = $script:State
    $q = $script:Queue
    $total = @($q).Count
    $partIds = @($q | ForEach-Object { $_.SpikeId } | Select-Object -Unique)
    $introDone = @{}
    $autoTried = @{}
    $pendingNote = ''
    $i = Get-FirstOpenIndex $q $State.results
    if ($i -gt 0) { Say "  Continuing at step $($i + 1) of $total." Green; Start-Sleep -Milliseconds 900 }

    while ($i -lt $total) {
        $item = $q[$i]; $spike = $item.Spike; $t = $item.Test; $key = $item.Key
        $existing = if ($State.results.Contains($key)) { $State.results[$key] } else { $null }

        # First time in a part: a build that failed skips the part; otherwise show its checklist.
        if (-not $introDone.ContainsKey($spike.id)) {
            $introDone[$spike.id] = $true
            Stop-App
            $open = @($q | Where-Object { $_.SpikeId -eq $spike.id -and -not (Test-StepAnswered $State.results $_.Key) })
            $skipWhy = ''
            if (@($FailedBuilds) -contains $spike.id) { $skipWhy = 'skipped: the app could not be built' }
            elseif (-not (Show-PartIntro $spike ([array]::IndexOf($partIds, $spike.id) + 1) $partIds.Count $open.Count)) { $skipWhy = 'skipped: the whole part was skipped' }
            if ($skipWhy) {
                foreach ($o in $open) { Set-StepResult $o.Key 'skip' $skipWhy @() }
                $i = Get-NextOpenIndex $q $State.results $i
                continue
            }
        }

        # Make sure the app matches this step's launch settings.
        $norm = Normalize-Launch $t.launch
        $sig = Get-Sig $norm
        $needLaunch = -not (Test-AppRunning)
        if ($script:App -and $script:App.spike -ne $spike.id) { $needLaunch = $true }
        if ($t.launch -and ($script:App -eq $null -or $script:App.sig -ne $sig)) { $needLaunch = $true }
        if ($needLaunch) {
            # A step without its own launch settings reuses whatever the app was last started with (same part only).
            $useNorm = if ($t.launch -or $script:App -eq $null -or $script:App.spike -ne $spike.id) { $norm } else { $script:App.norm }
            Start-App $spike $useNorm
        }

        if ($t.clipboard) { try { Set-Clipboard -Value $t.clipboard } catch { } }
        $autoInfo = $null
        if ($script:AutoOn -and $t.auto -and -not $autoTried.ContainsKey($key) -and -not (Test-StepAnswered $State.results $key)) {
            $autoTried[$key] = $true
            $ar = Invoke-EngineAuto $i
            if ($ar.status -eq 'pass' -or $ar.status -eq 'fail') { Save-AutoResult $key $ar $existing; $i = Get-NextOpenIndex $q $State.results $i; continue }
            $autoInfo = $ar
        }
        Show-Test $i $existing $autoInfo $false
        $advance = $false
        $more = $false
        while (-not $advance) {
            $valid = 'pfsnclrbqk?'
            if ($script:AutoOn -and $t.auto) { $valid += 'a' }
            $k = Read-Choice '  > ' $valid
            if ($k -eq '?') {
                $more = $true
                Show-Test $i $existing $autoInfo $true
            } elseif ($k -eq 'p' -or $k -eq 'f' -or $k -eq 's') {
                $status = @{ p = 'pass'; f = 'fail'; s = 'skip' }[$k]
                $note = $pendingNote; $pendingNote = ''
                if ($autoInfo) {
                    $ev = [string]$autoInfo.evidence
                    if ($ev.Length -gt 300) { $ev = $ev.Substring(0, 300) + '...' }
                    $note = ("[automatic check was not sure: $ev] " + $note).Trim()
                }
                $shots = if ($existing) { @($existing.shots) } else { @() }
                if ($k -eq 'f') {
                    Write-Host ''
                    $typed = Read-Host '  What happened? (one line, Enter to skip)'
                    if ($typed) { $note = (($note + ' ' + $typed)).Trim() }
                    if (Ask-YesNo 'Take a screenshot of the app now? (this window hides for a moment)' $true) {
                        $shot = Save-Screenshot ("{0}-{1}" -f $spike.id, $t.id) 3 $true
                        if ($shot) { $shots = @($shots) + $shot }
                    }
                }
                Set-StepResult $key $status $note $shots
                $i = Get-NextOpenIndex $q $State.results $i
                $advance = $true
            } elseif ($k -eq 'n') {
                $typed = Read-Host '  Note'
                if ($typed) { $pendingNote = (($pendingNote + ' ' + $typed)).Trim() }
            } elseif ($k -eq 'c') {
                $shot = Save-Screenshot ("{0}-{1}" -f $spike.id, $t.id)
                if ($shot) {
                    $r = if ($State.results.Contains($key)) { $State.results[$key] } else { @{ status = 'none'; note = ''; shots = @(); at = '' } }
                    $r.shots = @($r.shots) + $shot
                    $State.results[$key] = $r
                    Save-State
                }
            } elseif ($k -eq 'l') {
                if ($script:App) {
                    Say '  --- what the app printed (last lines) ---' DarkGray
                    foreach ($l in ((Read-LogTail $script:App.logOut 12) + (Read-LogTail $script:App.logErr 12))) { Say "    $l" DarkGray }
                    Say '  -----------------------------------------' DarkGray
                } else { Say '  The app is not open.' Yellow }
            } elseif ($k -eq 'r') {
                $useNorm = if ($t.launch -or $script:App -eq $null -or $script:App.spike -ne $spike.id) { $norm } else { $script:App.norm }
                Start-App $spike $useNorm
                Say '  The app was restarted.' Green
            } elseif ($k -eq 'k') {
                if ($t.clipboard) {
                    try { Set-Clipboard -Value $t.clipboard; Say "  '$($t.clipboard)' is on your clipboard." Green } catch { Say '  Could not set the clipboard.' Yellow }
                } else { Say '  This step has no clipboard text.' Yellow }
            } elseif ($k -eq 'a') {
                $ar = Invoke-EngineAuto $i
                if ($ar.status -eq 'pass' -or $ar.status -eq 'fail') { Save-AutoResult $key $ar $existing; $i = Get-NextOpenIndex $q $State.results $i; $advance = $true }
                else {
                    $autoInfo = $ar
                    Show-Test $i $existing $autoInfo $more
                }
            } elseif ($k -eq 'b') {
                if ($i -gt 0) { $i-- }; $advance = $true
            } elseif ($k -eq 'q') {
                if (Ask-YesNo 'Stop here? You can continue later by running the file again.' $true) { return 'quit' }
            }
        }
    }
    return 'done'
}

# ---------------------------------------------------------------- finish

# Makes the zip on the Desktop (or in spikes-results for a dry run / when there is no Desktop). Returns the path or $null.
function New-ResultsZip {
    $destDir = $OutRoot
    if (-not $DryRun) {
        try { $desk = [Environment]::GetFolderPath('Desktop'); if ($desk -and (Test-Path $desk)) { $destDir = $desk } } catch { }
    }
    $existing = @(Get-ChildItem -Path $destDir -Filter 'Snipdeck-test-results-*.zip' -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
    $zip = Join-Path $destDir (Get-ResultsZipName (Get-Date) $existing)
    try {
        Compress-Archive -Path (Join-Path $RunDir '*') -DestinationPath $zip -Force
        return $zip
    } catch {
        Write-WizardLog "zip failed: $($_.Exception.Message)"
        Say "  Could not make the zip file ($($_.Exception.Message))." Red
        Say "  Everything is in $RunDir : zip that folder yourself and send it." Yellow
        return $null
    }
}

function Finish-Run($Spikes, $State, [bool]$Push, [bool]$Stopped = $false) {
    $queue = $script:Queue
    $counts = Get-ResultCounts $queue $State.results
    if ($Stopped) { Write-Banner 'Stopped for now' 'Here is how far you got' } else { Write-Banner 'All done' 'Here is how it went' }
    Say ''
    Say ("  Worked: {0}     Didn't work: {1}     Skipped: {2}     Not done: {3}" -f $counts.worked, $counts.failed, $counts.skipped, $counts.notDone) White
    Say ''
    foreach ($s in $Spikes) {
        if ($State.spikes -notcontains $s.id) { continue }
        $tests = Get-Applicable $s $State.osClass $State.mode
        $v = Get-Verdict $s $tests $State.results
        Say ("  {0,-58} {1}" -f (Get-FriendlyName $s), (Get-PlainVerdict $v.label)) $v.color
    }
    Say ''
    $full = Build-Report $Spikes $State $false
    $paste = Build-Report $Spikes $State $true
    Write-Utf8 (Join-Path $RunDir 'results.md') $full
    Write-Utf8 (Join-Path $RunDir 'paste-back.md') $paste
    Save-State
    $oneLine = Build-OneLine $Spikes $State
    Write-Utf8 (Join-Path $RunDir 'paste-back-oneline.txt') $oneLine

    if ($script:AutoState -and $script:AutoState.Notepad) { Say-Wrapped 'The scratch Notepad document (snipdeck-auto-*.txt) can be closed without saving.' 2 Gray }
    $zip = New-ResultsZip
    $copied = $false
    try { Set-Clipboard -Value $oneLine; $copied = $true } catch { }
    if ($zip) {
        Say "  Your results file: $zip" Green
        if (-not $DryRun) { try { Start-Process -FilePath 'explorer.exe' -ArgumentList "/select,`"$zip`"" } catch { } }
    }
    if ($copied) { Say '  A short summary is on your clipboard.' Green }
    Say ''
    Say '  Drag this file into the Claude chat (or paste the summary).' White

    # Sending results to GitHub is only offered when chosen under Advanced options. A dry run never touches the repository.
    if ($Push -and -not $DryRun -and (Get-Command git -ErrorAction SilentlyContinue) -and (Test-Path (Join-Path $RepoRoot '.git'))) {
        $branch = (& git -C $RepoRoot rev-parse --abbrev-ref HEAD)
        if (Ask-YesNo "Send the results text to GitHub on branch '$branch' now?" $false) {
            $dest = Join-Path $RepoRoot 'docs\spikes\results'
            if (-not (Test-Path $dest)) { New-Item -ItemType Directory -Path $dest | Out-Null }
            $name = "windows-{0}.md" -f (Get-Date -Format 'yyyyMMdd-HHmm')
            Copy-Item (Join-Path $RunDir 'results.md') (Join-Path $dest $name)
            & git -C $RepoRoot add "docs/spikes/results/$name"
            & git -C $RepoRoot commit -m "docs(spikes): Windows manual test results ($name)"
            & git -C $RepoRoot push origin $branch
        }
    }
    Say ''
    Say '  Thank you!' White
}

# ---------------------------------------------------------------- self test (no Windows needed)

if ($SelfTest) {
    $spikes = Load-Spikes
    $problems = Test-Definitions $spikes
    Say "Loaded $(@($spikes).Count) spike file(s)." Gray
    foreach ($s in $spikes) {
        foreach ($m in 'v1', 'quick', 'standard', 'full') {
            foreach ($os in 'win10', 'win11') {
                $n = (Get-Applicable $s $os $m).Count
                Say ("  {0} {1,-8} {2,-5} -> {3,2} steps" -f $s.id, $m, $os, $n) DarkGray
            }
        }
    }
    # verdict logic: all pass => passed; one fail => failed; empty => incomplete
    foreach ($s in $spikes) {
        $tests = Get-Applicable $s 'win11' 'quick'
        $res = [ordered]@{}
        $empty = Get-Verdict $s $tests $res
        foreach ($t in $tests) { $res["$($s.id):$($t.id)"] = @{ status = 'pass' } }
        $allPass = Get-Verdict $s $tests $res
        $strict = @($tests | Where-Object { $_.gate -eq $true -and -not $_.anyGroup })[0]
        $res["$($s.id):$($strict.id)"] = @{ status = 'fail' }
        $oneFail = Get-Verdict $s $tests $res
        # an "any of" group fails only when every member failed
        foreach ($t in @($tests | Where-Object { $_.anyGroup })) { $res["$($s.id):$($t.id)"] = @{ status = 'fail' } }
        if ($empty.label -ne 'INCOMPLETE') { $problems += "$($s.id): empty results should be INCOMPLETE, got $($empty.label)" }
        if ($allPass.label -ne 'GATE PASSED') { $problems += "$($s.id): all-pass should be GATE PASSED, got $($allPass.label)" }
        if ($oneFail.label -ne 'GATE FAILED') { $problems += "$($s.id): a failed gate step should be GATE FAILED, got $($oneFail.label)" }
    }
    $problems = @($problems)
    # Every script must parse, and stay ASCII (Windows PowerShell 5.1 reads BOM-less scripts as ANSI).
    $scripts = @(Get-ChildItem -Path $Here -Filter '*.ps1' -Recurse)
    foreach ($f in $scripts) {
        $tokens = $null; $errs = $null
        [void][System.Management.Automation.Language.Parser]::ParseFile($f.FullName, [ref]$tokens, [ref]$errs)
        foreach ($e in @($errs)) { $problems += "parse error: $($f.Name):$($e.Extent.StartLineNumber): $($e.Message)" }
        if ([IO.File]::ReadAllText($f.FullName) -match '[^\x00-\x7F]') { $problems += "$($f.Name): non-ASCII characters" }
    }
    Say "Parsed $($scripts.Count) PowerShell file(s)." Gray
    # Automation layer: load it with the mock primitives, check every "auto" name, run its unit and end-to-end tests.
    foreach ($f in (Get-AutoModuleFiles $true)) { . $f }
    . (Join-Path (Join-Path $AutoDir 'tests') 'selftest.ps1')
    $autoSteps = 0
    foreach ($s in $spikes) {
        foreach ($t in $s.tests) {
            if (-not $t.auto) { continue }
            $autoSteps++
            if (-not (Get-Command $t.auto -CommandType Function -ErrorAction SilentlyContinue)) { $problems += "$($s.id):$($t.id): no automation function '$($t.auto)'" }
        }
    }
    $flowProblems = @(Invoke-FlowSelfTest $spikes)
    Say "Wizard flow (plans, time left, resume, counts, file names): $(if ($flowProblems.Count) { "$($flowProblems.Count) problem(s)" } else { 'OK' })." Gray
    $problems += $flowProblems
    $autoProblems = @(Invoke-AutoSelfTest)
    Say "Automation: $autoSteps step(s) have an 'auto' field; unit and mock end-to-end tests: $(if ($autoProblems.Count) { "$($autoProblems.Count) problem(s)" } else { 'OK' })." Gray
    $problems += $autoProblems
    # the clipboard summary must be a single line
    $fake = [ordered]@{ mode = 'quick'; osClass = 'win11'; env = [ordered]@{ os = 'Test OS'; build = 1; gpu = 'GPU' }
        spikes = @($spikes[0].id); results = [ordered]@{}; diag = [ordered]@{} }
    $first = Get-Applicable $spikes[0] 'win11' 'quick'
    $fake.results["$($spikes[0].id):$($first[0].id)"] = @{ status = 'fail'; note = "two`nlines | with a pipe"; shots = @(); at = '' }
    $one = Build-OneLine $spikes $fake
    if ($one -match "[`r`n]") { $problems += 'Build-OneLine returned more than one line' }
    if ($one -notmatch 'FAIL') { $problems += 'Build-OneLine lost the failed step' }
    if ($problems.Count -gt 0) { $problems | ForEach-Object { Say "PROBLEM: $_" Red }; exit 1 }
    Say 'Self test OK.' Green
    exit 0
}

# ---------------------------------------------------------------- main

$script:State = $null
$script:CtrlCWas = $null

# The questions behind "Advanced options". Changes and returns the plan (see New-Plan in flow.ps1).
function Read-AdvancedPlan($Spikes, $Plan) {
    Write-Banner 'Advanced options' 'Most people should go back and just press Enter for the recommended check'
    $Plan.advanced = $true
    Say ''
    Say '  How thorough?' White
    Say '    V  The v1 check (recommended): only what is needed to ship v1' Gray
    Say '    Q  Quick: just the key steps of the parts you pick' Gray
    Say '    S  Standard: every step except the optional ones' Gray
    Say '    F  Full: everything' Gray
    $c = Read-Choice '  > ' 'vqsf' 'v'
    $Plan.mode = @{ v = 'v1'; q = 'quick'; s = 'standard'; f = 'full' }[$c]
    if ($Plan.mode -ne 'v1') {
        Say ''
        Say '  Which parts do you want to check?' White
        $ids = @()
        foreach ($s in $Spikes) { if (Ask-YesNo (Get-FriendlyName $s) $true) { $ids += $s.id } }
        $Plan.spikeIds = $ids
    }
    if (-not $NoAuto) { $Plan.auto = Ask-AutoOptIn } else { $Plan.auto = $false }
    if ((Get-Command git -ErrorAction SilentlyContinue) -and (Test-Path (Join-Path $RepoRoot '.git'))) {
        Say ''
        $Plan.push = Ask-YesNo 'At the end, also offer to send the results text to GitHub? (most people say no)' $false
    }
    return $Plan
}

try {
    try { $script:CtrlCWas = [Console]::TreatControlCAsInput; [Console]::TreatControlCAsInput = $true } catch { }
    try { $Host.UI.RawUI.WindowTitle = 'Test Snipdeck on Windows' } catch { }
    $spikes = Load-Spikes
    $problems = Test-Definitions $spikes
    if ($problems.Count -gt 0) {
        $problems | ForEach-Object { Say "  PROBLEM: $_" Red; Write-WizardLog "definition problem: $_" }
        Say '  The test files are damaged. Download Snipdeck again, or ask Claude.' Red
        exit 1
    }

    # ---- welcome
    $plan = New-Plan
    $demoIds = @(Get-PlanSpikeIds $spikes 'v1' @())
    $demoQueue = @(Get-RunQueue $spikes $demoIds 'v1' 'win11')
    $demoTime = Format-Duration (Get-MinutesLeft $demoQueue ([ordered]@{}) 0 $true)
    Write-Banner 'Test Snipdeck on Windows'
    Say ''
    Say-Wrapped 'This checks that Snipdeck works on this PC. The wizard builds the app, opens it, and tells you what to try.' 2 White
    Say-Wrapped "It takes $demoTime, plus a one-time build of 10-20 minutes the first time." 2 Gray
    Say-Wrapped 'Some steps run by themselves: your mouse and keyboard may move. To stop, hold the Esc key.' 2 Gray
    Say-Wrapped 'Nothing is sent anywhere. At the end you get one results file to share.' 2 Gray
    Say ''
    if (-not $IsWin) {
        Say '  This is not Windows, so the test apps cannot run here. Use -SelfTest to check the test files.' Yellow
        exit 1
    }

    # An automation run that died while a Windows setting was switched left auto-restore.json: put the setting back.
    if (-not $DryRun -and (Test-Path (Join-Path $OutRoot 'auto-restore.json'))) {
        try {
            . (Join-Path $AutoDir 'lib.ps1'); . (Join-Path $AutoDir 'primitives.ps1')
            if (Restore-AutoRegistryFromFile $OutRoot) { Say-Wrapped 'Put back the Windows settings an interrupted automatic step had switched (theme / transparency / text size).' 2 Yellow }
        } catch { Say "  Could not restore settings listed in $(Join-Path $OutRoot 'auto-restore.json'): $($_.Exception.Message)" Red }
    }
    if (-not (Test-Path $OutRoot)) { New-Item -ItemType Directory -Path $OutRoot | Out-Null }

    # ---- an unfinished run? Continuing is the default.
    $resumed = $false
    if (Test-Path $StateFile) {
        $old = $null
        try { $old = Load-State } catch { Write-WizardLog "state.json unreadable: $($_.Exception.Message)" }
        if ($old) {
            $oldQueue = @(Get-RunQueue $spikes $old.spikes $old.mode $old.osClass)
            if (Test-RunResumable $oldQueue $old.results) {
                $p = Get-RunProgress $oldQueue $old.results
                Say "  Last time you answered $($p.answered) of $($p.total) steps and stopped." Gray
                if (Ask-YesNo 'Continue where you left off?' $true) { $script:State = $old; $resumed = $true }
            }
        }
        if (-not $resumed) {
            $archive = Join-Path $OutRoot ("run-{0}" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
            Move-Item $RunDir $archive
            Say "  The earlier run was kept in $archive" DarkGray
        }
    }

    # ---- what to run
    if (-not $resumed) {
        if ($Spike -or $Mode) {
            # Command-line choices: no menu.
            if ($Mode) { $plan.mode = $Mode } else { $plan.mode = 'quick' }
            if ($Spike) { $plan.spikeIds = @($Spike | ForEach-Object { $_.ToUpper() }) }
            $plan.advanced = $true
        } else {
            Say-Wrapped 'Press Enter to start the recommended "v1 check" (the steps needed before v1 can ship).' 2 White
            Say-Wrapped 'Or press A for advanced options, or Q to quit.' 2 Gray
            $c = Read-Choice '  > ' 'gaq' 'g'
            if ($c -eq 'q') { Say '  Okay, nothing was changed.' Gray; exit 0 }
            if ($c -eq 'a') { $plan = Read-AdvancedPlan $spikes $plan }
        }
        if (@($plan.spikeIds).Count -eq 0) { $plan.spikeIds = @(Get-PlanSpikeIds $spikes $plan.mode @()) }
        else { $plan.spikeIds = @(Get-PlanSpikeIds $spikes $plan.mode $plan.spikeIds) }
        if (@($plan.spikeIds).Count -eq 0) { Say '  Nothing selected.' Yellow; exit 0 }
        if ($NoAuto) { $plan.auto = $false }
    }

    if (-not $SkipPrereqs) { if (-not (Invoke-Prereqs)) { exit 1 } } else { Add-CargoToPath }

    # ---- start (or continue) the run
    if (-not (Test-Path $RunDir)) { New-Item -ItemType Directory -Path $RunDir | Out-Null }
    if (-not $script:State) {
        $envInfo = Get-EnvInfo
        $osClass = if ($envInfo.build -ge 22621) { 'win11' } else { 'win10' }
        $hasOsSteps = (@($spikes | Where-Object { $plan.spikeIds -contains $_.id } | ForEach-Object { $_.tests } | Where-Object { $_.os -and $_.os -ne 'any' }).Count -gt 0)
        if ($plan.advanced -and $hasOsSteps) {
            $other = if ($osClass -eq 'win11') { 'Windows 10' } else { 'Windows 11' }
            Say ''
            Say-Wrapped "Some steps depend on your Windows version. This PC looks like $(if ($osClass -eq 'win11') { 'Windows 11 (22H2 or newer)' } else { 'Windows 10 or an older Windows 11' })." 2 Gray
            if (Ask-YesNo "Use the $other steps instead?" $false) { $osClass = if ($osClass -eq 'win11') { 'win10' } else { 'win11' } }
        }
        $script:State = New-State $plan.mode $osClass $envInfo $plan.spikeIds $plan.auto
        Save-State
    }
    $script:Queue = @(Get-RunQueue $spikes $script:State.spikes $script:State.mode $script:State.osClass)
    if (@($script:Queue).Count -eq 0) { Say '  There are no steps for this choice.' Yellow; exit 0 }

    # ---- automatic steps (on by default for the recommended check)
    $wantAuto = ($script:State.auto -and -not $NoAuto) -or $Auto
    if ($wantAuto -and (Test-HasAutoSteps $script:Queue $script:State.results)) {
        $loadError = $null
        foreach ($f in (Get-AutoModuleFiles ([bool]$DryRun))) {
            try { . $f } catch { $loadError = "$([IO.Path]::GetFileName($f)): $($_.Exception.Message)"; break }
        }
        if ($loadError) { Say-Wrapped "The automatic steps could not be loaded ($loadError). You will do every step yourself." 2 Red; Pause-Enter }
        else { $script:AutoOn = Start-Automation }
    }

    # ---- build everything first, then the steps
    Write-Banner 'Building the app' 'You can leave this running and come back'
    Say ''
    $failedBuilds = @(Build-AllSpikes $spikes $script:Queue $script:State.results)
    Pause-Enter '  Press Enter to start the steps...'
    $outcome = Run-Queue $failedBuilds
    Stop-App
    Finish-Run $spikes $script:State $plan.push ($outcome -eq 'quit')
    if ($outcome -eq 'quit') { Say-Wrapped 'To carry on later, double-click Test Snipdeck on Windows.bat again and press Enter at "Continue where you left off?".' 2 Yellow }
}
catch {
    $msg = $_.Exception.Message
    if ($msg -eq 'Stopped at your request.') {
        Say ''
        Say '  Stopped. Run the file again to continue where you left off.' Yellow
        exit 0
    }
    Write-WizardLog (($_ | Out-String) + $_.ScriptStackTrace)
    Say ''
    Say "  $(Get-PlainError $msg)" Red
    Say "  Technical details are saved in: $LogFile" Gray
    Say '  If you ask Claude for help, send that file.' Gray
    exit 1
}
finally {
    Stop-App
    if (Get-Command Exit-AutoSession -CommandType Function -ErrorAction SilentlyContinue) { try { Exit-AutoSession } catch { } }
    try { if ($null -ne $script:CtrlCWas) { [Console]::TreatControlCAsInput = $script:CtrlCWas } } catch { }
}
