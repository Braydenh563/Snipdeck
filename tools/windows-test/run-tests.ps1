<#
.SYNOPSIS
  Snipdeck Windows test wizard: builds the M0 spike apps, launches each with the right
  settings, walks you through the manual test steps, and records the results.

.DESCRIPTION
  Test definitions live in tests\*.json (one file per spike). Adding a spike means adding a
  JSON file; this script needs no changes. Works on Windows PowerShell 5.1 and PowerShell 7.

.EXAMPLE
  .\run-tests.ps1                      # interactive menus
  .\run-tests.ps1 -Spike S1 -Mode quick
  .\run-tests.ps1 -SelfTest            # validate the test files, no Windows needed
#>
[CmdletBinding()]
param(
    [string[]]$Spike,
    [ValidateSet('quick', 'standard', 'full')]
    [string]$Mode,
    [switch]$SkipPrereqs,
    [switch]$SelfTest,
    [switch]$DryRun   # developer use: fakes Windows, build and launch so the flow can be tested anywhere
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = [IO.Path]::GetFullPath((Join-Path (Join-Path $Here '..') '..'))
$TestsDir = Join-Path $Here 'tests'
$OutRoot = Join-Path $RepoRoot 'spikes-results'
$RunDir = Join-Path $OutRoot 'current'
$StateFile = Join-Path $RunDir 'state.json'
$IsWin = ($env:OS -eq 'Windows_NT') -or $DryRun

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
        while ($true) {
            try { $Host.UI.RawUI.FlushInputBuffer() } catch { }
            Write-Host $Prompt -NoNewline -ForegroundColor Cyan
            $ch = ''
            $key = $null
            try { $key = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown') } catch { $key = $null }
            if ($key) {
                if ([int]$key.Character -eq 3) {
                    Write-Host ''
                    Say '  Ctrl+C is turned off here (it would close the test app). Press q to finish. To copy text, select it and press Ctrl+Shift+C.' Yellow
                    continue
                }
                # Pasted text arrives as a burst of keys. Ignore it so the test apps cannot trigger wizard commands.
                Start-Sleep -Milliseconds 50
                $burst = $false
                try { $burst = $Host.UI.RawUI.KeyAvailable } catch { }
                if ($burst) {
                    try { $Host.UI.RawUI.FlushInputBuffer() } catch { }
                    Write-Host ''
                    Say '  (ignored: that looked like pasted text, not a key press)' Yellow
                    continue
                }
                $ch = ([string]$key.Character).ToLower()
                if (($ch -eq "`r" -or $ch -eq "`n") -and $EnterKey) { $ch = $EnterKey }
            } else {
                $line = (Read-Host).Trim().ToLower()
                $ch = if ($line.Length -gt 0) { $line.Substring(0, 1) } elseif ($EnterKey) { $EnterKey } else { '' }
            }
            if ($ch -and $Keys.Contains($ch)) { Write-Host $ch; return $ch }
            Write-Host ''
        }
    } finally {
        try { if ($null -ne $oldCtrlC) { [Console]::TreatControlCAsInput = $oldCtrlC } } catch { }
    }
}

function Ask-YesNo([string]$Question, [bool]$Default = $true) {
    $hint = if ($Default) { '[Y/n]' } else { '[y/N]' }
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
    if ($Sub) { Say "  $Sub" DarkGray }
    Say ('=' * (Get-Width)) DarkCyan
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
        }
        if (-not (Test-Path (Join-Path $RepoRoot $s.dir))) { $problems += "$($s.id): folder '$($s.dir)' not found" }
    }
    return $problems
}

function Get-Applicable($SpikeDef, [string]$OsClass, [string]$RunMode) {
    $out = @()
    foreach ($t in $SpikeDef.tests) {
        if ($t.os -and $t.os -ne 'any' -and $t.os -ne $OsClass) { continue }
        $isGate = ($t.gate -eq $true) -or [bool]$t.anyGroup
        $isOptional = ($t.optional -eq $true)
        if ($RunMode -eq 'quick' -and -not $isGate) { continue }
        if ($RunMode -eq 'standard' -and $isOptional) { continue }
        $out += $t
    }
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
    $bin = Join-Path $env:USERPROFILE '.cargo\bin'
    if ((Test-Path $bin) -and ($env:PATH -notlike "*$bin*")) { $env:PATH = "$bin;$env:PATH" }
}

function Install-WithWinget([string]$Id, [string[]]$Extra = @()) {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Say '  winget is not available on this PC. Install manually (see tools\windows-test\README.md).' Yellow
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

function Invoke-Prereqs {
    Write-Banner 'Step 1: prerequisites' 'Checks the tools needed to build the test apps'
    if ($DryRun) { Say '  [dry run] skipping tool checks' DarkGray; return $true }
    if (-not $IsWin) {
        Say '  This is not Windows. The test apps are Windows-only; you can still read the tests (-SelfTest).' Yellow
        return $false
    }
    Add-CargoToPath
    $ok = $true

    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Say "  [ok] Rust: $(& rustc --version)" Green
    } else {
        Say '  [missing] Rust (cargo)' Yellow
        if (Ask-YesNo 'Install Rust now with winget (Rustlang.Rustup)?') {
            if (Install-WithWinget 'Rustlang.Rustup') {
                Add-CargoToPath
                Say '  Installed. If the next check fails, close this window and run the wizard again.' Yellow
            }
        }
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { $ok = $false }
    }

    if (Get-Command rustc -ErrorAction SilentlyContinue) {
        $host_ = (& rustc -vV | Select-String '^host:').ToString()
        if ($host_ -match 'msvc') {
            Say "  [ok] Rust toolchain targets MSVC ($($host_.Trim()))" Green
        } else {
            Say "  [warn] Rust host is not MSVC: $($host_.Trim())" Yellow
            if (Ask-YesNo 'Switch to the MSVC toolchain (rustup default stable-x86_64-pc-windows-msvc)?') {
                & rustup default stable-x86_64-pc-windows-msvc
            }
        }
    }

    if (Test-MsvcTools) {
        Say '  [ok] Visual Studio Build Tools (C++ workload)' Green
    } else {
        Say '  [missing] Visual Studio Build Tools with the C++ workload (needed to link Windows programs)' Yellow
        Say '            This is a large download (a few GB) and shows an admin (UAC) prompt.' DarkGray
        if (Ask-YesNo 'Install it now with winget?') {
            $extra = @('--override', '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
            [void](Install-WithWinget 'Microsoft.VisualStudio.2022.BuildTools' $extra)
        }
        if (-not (Test-MsvcTools)) { Say '  Still missing. Finish the install, then run the wizard again.' Red; $ok = $false }
    }

    if (Get-Command git -ErrorAction SilentlyContinue) { Say '  [ok] git' Green } else { Say '  [info] git not found: only needed for the optional commit/push at the end' DarkGray }
    Say ''
    if (-not $ok) {
        Say '  Some prerequisites are missing. Fix them and run the wizard again.' Red
        Pause-Enter
    }
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

function Build-Spike($SpikeDef) {
    $dir = Join-Path $RepoRoot $SpikeDef.dir
    if ($DryRun) { Say "  [dry run] would build $($SpikeDef.id)" DarkGray; return $true }
    while ($true) {
        Say "  Building $($SpikeDef.id) (cargo build --release). The first build can take several minutes..." Cyan
        # Direct call, NOT Start-Process -Wait: the MSVC linker leaves mspdbsrv.exe running, and -Wait
        # waits for the whole process tree, so the wizard would hang after "Finished".
        Push-Location $dir
        try { & cargo build --release; $code = $LASTEXITCODE } finally { Pop-Location }
        if ($code -eq 0) { Say '  Build OK.' Green; return $true }
        Say "  Build FAILED (exit code $code). Scroll up for the error." Red
        $c = Read-Choice '  [r]etry, [s]kip this spike, or [q]uit? ' 'rsq'
        if ($c -eq 'r') { continue }
        if ($c -eq 'q') { throw 'Quit requested.' }
        return $false
    }
}

# ---------------------------------------------------------------- screenshots

function Save-Screenshot([string]$Name) {
    if ($DryRun) { Say '  [dry run] would take a screenshot' DarkGray; return (Join-Path $RunDir "screens\$Name-dryrun.png") }
    if (-not $IsWin) { Say '  Screenshots only work on Windows.' Yellow; return $null }
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

        Say '  Minimising this window. Bring the test app to the front: capturing in 5 seconds...' Yellow
        $h = [SnipTest.Win]::GetConsoleWindow()
        [void][SnipTest.Win]::ShowWindowAsync($h, 6)
        Start-Sleep -Seconds 5
        $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
        $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        [void][SnipTest.Win]::ShowWindowAsync($h, 9)
        [void][SnipTest.Win]::SetForegroundWindow($h)
        Say "  Saved: $file" Green
        return $file
    } catch {
        Say "  Screenshot failed: $($_.Exception.Message)" Red
        return $null
    }
}

# ---------------------------------------------------------------- state + report

function New-State([string]$RunMode, [string]$OsClass, $EnvInfo, $SpikeIds) {
    return [ordered]@{
        started = (Get-Date).ToString('s'); mode = $RunMode; osClass = $OsClass; env = $EnvInfo
        spikes = @($SpikeIds); results = [ordered]@{}; diag = [ordered]@{}
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
    }
    $diag = [ordered]@{}
    if ($raw.diag) { foreach ($p in $raw.diag.PSObject.Properties) { $diag[$p.Name] = @($p.Value) } }
    $envInfo = [ordered]@{}
    foreach ($p in $raw.env.PSObject.Properties) { $envInfo[$p.Name] = $p.Value }
    return [ordered]@{
        started = $raw.started; mode = $raw.mode; osClass = $raw.osClass; env = $envInfo
        spikes = @($raw.spikes); results = $res; diag = $diag
    }
}

function Build-Report($Spikes, $State, [bool]$ForPaste) {
    $sb = New-Object System.Text.StringBuilder
    $e = $State.env
    [void]$sb.AppendLine('# Snipdeck M0 spikes: Windows test results')
    [void]$sb.AppendLine('')
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
            [void]$sb.AppendLine("| $($t.id) | $st | $($t.title) | $note |")
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
    if ($ForPaste) { [void]$sb.AppendLine('(Passing steps without notes are omitted. Full report and screenshots: spikes-results\current\)') }
    return $sb.ToString()
}

# ---------------------------------------------------------------- one test screen

function Show-Test($SpikeDef, $T, [int]$Index, [int]$Total, $Existing) {
    $gate = if ($T.gate -eq $true -or $T.anyGroup) { '  [GO/NO-GO STEP]' } else { '' }
    Write-Banner "$($SpikeDef.id): $($SpikeDef.title)" "Step $Index of $Total$gate"
    Say ''
    Say "  $($T.id)  $($T.title)" White
    Say ''
    Say '  What to do:' Cyan
    Say-Wrapped $T.do 4 Gray
    Say ''
    Say '  What you should see:' Cyan
    Say-Wrapped $T.expect 4 Gray
    Say ''
    if (Test-AppRunning) { Say "  App: running (PID $($script:App.proc.Id)), $($script:App.label)" DarkGreen }
    else { Say '  App: NOT running (press r to launch it)' Yellow }
    if ($Existing) { Say "  Previous answer for this step: $($Existing.status)  $($Existing.note)" DarkYellow }
    Say ''
    Say '  [p] pass   [f] fail   [s] skip   [n] add note   [c] screenshot   [l] app log   [r] relaunch   [b] back   [q] finish now' DarkGray
    Say '  Tip: Ctrl+C is turned off here. Copy text with Ctrl+Shift+C. Pasted text is ignored.' DarkGray
}

function Run-Spike($SpikeDef, $Tests, $State) {
    Write-Banner "$($SpikeDef.id): $($SpikeDef.title)" "Notes: $($SpikeDef.note)"
    Say ''
    Say '  Before you start:' Cyan
    foreach ($line in $SpikeDef.setup) { Say-Wrapped "- $line" 4 Gray }
    Say ''
    if (-not (Ask-YesNo "Run $($SpikeDef.id) now? ($($Tests.Count) steps)" $true)) { return 'skipped' }
    if (-not (Build-Spike $SpikeDef)) { return 'skipped' }

    $i = 0
    $pendingNote = ''
    if ($script:Resumed) {
        # Skip steps already answered in the previous session (press b to revisit one).
        while ($i -lt $Tests.Count) {
            $k0 = "$($SpikeDef.id):$($Tests[$i].id)"
            if ($State.results.Contains($k0) -and @('pass', 'fail', 'skip') -contains $State.results[$k0].status) { $i++ } else { break }
        }
        if ($i -gt 0) { Say "  Resuming: $i step(s) already answered." Green; Start-Sleep -Milliseconds 900 }
    }
    while ($i -lt $Tests.Count) {
        $t = $Tests[$i]
        $key = "$($SpikeDef.id):$($t.id)"
        $existing = if ($State.results.Contains($key)) { $State.results[$key] } else { $null }

        # Make sure the app matches this step's launch settings.
        $norm = Normalize-Launch $t.launch
        $sig = Get-Sig $norm
        $needLaunch = -not (Test-AppRunning)
        if ($t.launch -and ($script:App -eq $null -or $script:App.sig -ne $sig)) { $needLaunch = $true }
        if ($needLaunch) {
            # A step without its own launch settings reuses whatever the app was last started with.
            $useNorm = if ($t.launch -or $script:App -eq $null) { $norm } else { $script:App.norm }
            Start-App $SpikeDef $useNorm
        }

        Show-Test $SpikeDef $t ($i + 1) $Tests.Count $existing
        $advance = $false
        while (-not $advance) {
            $k = Read-Choice '  > ' 'pfsnclrbq'
            if ($k -eq 'p' -or $k -eq 'f' -or $k -eq 's') {
                $status = @{ p = 'pass'; f = 'fail'; s = 'skip' }[$k]
                $note = $pendingNote; $pendingNote = ''
                if ($k -eq 'f') {
                    $typed = Read-Host '  What went wrong? (Enter to leave blank)'
                    if ($typed) { $note = (($note + ' ' + $typed)).Trim() }
                }
                $shots = if ($existing) { @($existing.shots) } else { @() }
                $State.results[$key] = @{ status = $status; note = $note; shots = $shots; at = (Get-Date).ToString('s') }
                Save-State
                $i++; $advance = $true
            } elseif ($k -eq 'n') {
                $typed = Read-Host '  Note'
                if ($typed) { $pendingNote = (($pendingNote + ' ' + $typed)).Trim() }
            } elseif ($k -eq 'c') {
                $shot = Save-Screenshot ("{0}-{1}" -f $SpikeDef.id, $t.id)
                if ($shot) {
                    $r = if ($State.results.Contains($key)) { $State.results[$key] } else { @{ status = 'none'; note = ''; shots = @(); at = '' } }
                    $r.shots = @($r.shots) + $shot
                    $State.results[$key] = $r
                    Save-State
                }
            } elseif ($k -eq 'l') {
                if ($script:App) {
                    Say '  --- app output (last lines) ---' DarkGray
                    foreach ($l in ((Read-LogTail $script:App.logOut 12) + (Read-LogTail $script:App.logErr 12))) { Say "    $l" DarkGray }
                    Say '  -------------------------------' DarkGray
                } else { Say '  No app running.' Yellow }
            } elseif ($k -eq 'r') {
                $useNorm = if ($t.launch -or $script:App -eq $null) { $norm } else { $script:App.norm }
                Start-App $SpikeDef $useNorm
                Say "  Relaunched ($(Describe-Launch $useNorm))." Green
            } elseif ($k -eq 'b') {
                if ($i -gt 0) { $i-- }; $advance = $true
            } elseif ($k -eq 'q') {
                return 'quit'
            }
        }
    }
    return 'done'
}

# ---------------------------------------------------------------- finish

function Finish-Run($Spikes, $State) {
    Write-Banner 'Results' 'Saved in spikes-results\current\'
    Say ''
    foreach ($s in $Spikes) {
        if ($State.spikes -notcontains $s.id) { continue }
        $tests = Get-Applicable $s $State.osClass $State.mode
        $v = Get-Verdict $s $tests $State.results
        Say ("  {0}  {1,-14} {2}" -f $s.id, $v.label, $v.detail) $v.color
    }
    Say ''
    $full = Build-Report $Spikes $State $false
    $paste = Build-Report $Spikes $State $true
    Write-Utf8 (Join-Path $RunDir 'results.md') $full
    Write-Utf8 (Join-Path $RunDir 'paste-back.md') $paste
    Save-State

    try { Set-Clipboard -Value $paste; Say '  A short summary is on your clipboard: paste it into the chat with Claude.' Green }
    catch { Say "  Summary saved in $RunDir\paste-back.md (open it and copy it)." Yellow }
    Say "  Full report: $RunDir\results.md" Gray

    if ((Test-Path (Join-Path $RunDir 'screens')) -and (Ask-YesNo 'Zip the screenshots and results into one file?' $true)) {
        $zip = Join-Path $OutRoot ("snipdeck-m0-results-{0}.zip" -f (Get-Date -Format 'yyyyMMdd-HHmm'))
        Compress-Archive -Path (Join-Path $RunDir '*') -DestinationPath $zip -Force
        Say "  Created $zip" Green
    }
    if (-not $DryRun -and (Ask-YesNo 'Open the results folder?' $true)) { Invoke-Item $RunDir }

    if ((Get-Command git -ErrorAction SilentlyContinue) -and (Test-Path (Join-Path $RepoRoot '.git'))) {
        $branch = (& git -C $RepoRoot rev-parse --abbrev-ref HEAD)
        if (Ask-YesNo "Commit results.md to the repo and push branch '$branch'? (text only, no screenshots)" $false) {
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
    Say '  Done. Thank you!' White
}

# ---------------------------------------------------------------- self test (no Windows needed)

if ($SelfTest) {
    $spikes = Load-Spikes
    $problems = Test-Definitions $spikes
    Say "Loaded $(@($spikes).Count) spike file(s)." Gray
    foreach ($s in $spikes) {
        foreach ($m in 'quick', 'standard', 'full') {
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
    if ($problems.Count -gt 0) { $problems | ForEach-Object { Say "PROBLEM: $_" Red }; exit 1 }
    Say 'Self test OK.' Green
    exit 0
}

# ---------------------------------------------------------------- main

$script:State = $null
$script:Resumed = $false
try {
    Write-Banner 'Snipdeck: Windows test wizard' 'Builds the M0 spike apps and walks you through the manual tests'
    Say ''
    Say-Wrapped 'You will answer p (pass), f (fail) or s (skip) for each step. Nothing is sent anywhere: results are saved on this PC. At the end a short summary is copied to your clipboard so you can paste it to Claude.' 2
    Say ''
    if (-not $IsWin) {
        Say '  Not running on Windows: use -SelfTest to validate the test files only.' Yellow
        exit 1
    }

    $spikes = Load-Spikes
    $problems = Test-Definitions $spikes
    if ($problems.Count -gt 0) { $problems | ForEach-Object { Say "  PROBLEM: $_" Red }; exit 1 }

    if (-not $SkipPrereqs) { if (-not (Invoke-Prereqs)) { exit 1 } } else { Add-CargoToPath }

    # Resume or start fresh.
    if (-not (Test-Path $OutRoot)) { New-Item -ItemType Directory -Path $OutRoot | Out-Null }
    if (Test-Path $StateFile) {
        Write-Banner 'Previous run found' $RunDir
        if (Ask-YesNo 'Resume the previous run (answers already given are kept)?' $true) {
            $script:State = Load-State
            $script:Resumed = $true
        } else {
            $archive = Join-Path $OutRoot ("run-{0}" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
            Move-Item $RunDir $archive
            Say "  Old run archived to $archive" DarkGray
        }
    }
    if (-not (Test-Path $RunDir)) { New-Item -ItemType Directory -Path $RunDir | Out-Null }

    if (-not $script:State) {
        $envInfo = Get-EnvInfo
        $osClass = if ($envInfo.build -ge 22621) { 'win11' } else { 'win10' }
        Write-Banner 'Step 2: what to test'
        Say ''
        Say "  This PC: $($envInfo.os), build $($envInfo.build)" Gray
        Say "  GPU: $($envInfo.gpu)" Gray
        Say ''
        $label = if ($osClass -eq 'win11') { 'Windows 11 22H2+ (Mica supported)' } else { 'Windows 10 / older Windows 11 (no Mica)' }
        Say "  The S4 steps will use the '$osClass' set: $label." Cyan
        if (Ask-YesNo 'Use the other set instead?' $false) { $osClass = if ($osClass -eq 'win11') { 'win10' } else { 'win11' } }
        Say ''

        $ids = @()
        if ($Spike) { $ids = @($Spike | ForEach-Object { $_.ToUpper() }) }
        else {
            Say '  Which prototypes? (recommended order: S1, then S2, then S4)' Cyan
            foreach ($s in $spikes) {
                if (Ask-YesNo "$($s.id): $($s.title)" $true) { $ids += $s.id }
            }
        }
        if ($ids.Count -eq 0) { Say '  Nothing selected.' Yellow; exit 0 }

        $runMode = $Mode
        if (-not $runMode) {
            Say ''
            Say '  How thorough?' Cyan
            Say '    [q] quick    only the go/no-go steps (about 20 minutes)' Gray
            Say '    [s] standard all steps except optional ones' Gray
            Say '    [f] full     everything' Gray
            $c = Read-Choice '  > ' 'qsf' 'q'
            $runMode = @{ q = 'quick'; s = 'standard'; f = 'full' }[$c]
        }
        $script:State = New-State $runMode $osClass $envInfo $ids
        Save-State
    }

    $quit = $false
    foreach ($s in $spikes) {
        if ($quit) { break }
        if ($script:State.spikes -notcontains $s.id) { continue }
        $tests = Get-Applicable $s $script:State.osClass $script:State.mode
        $outcome = Run-Spike $s $tests $script:State
        Stop-App
        if ($outcome -eq 'quit') { $quit = $true }
    }
    Finish-Run $spikes $script:State
}
finally {
    Stop-App
}
