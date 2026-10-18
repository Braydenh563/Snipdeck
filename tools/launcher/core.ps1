# core.ps1 - console, logging, prompts, running programs, safe deletion and environment checks.
# Dot-sourced by snipdeck.ps1 after lib.ps1. Windows PowerShell 5.1 compatible; ASCII only.
# Expects $script:RepoRoot and $script:Opt (@{ Yes; WhatIf }) to be set by the caller.

$script:LogPath = $null
$script:LogOn = $false
$script:OwnedTemp = @()
$script:MinFreeBytes = 8GB
$script:ExeName = 'snipdeck-app'
if ($script:OnWindows) { $script:ExeName = 'snipdeck-app.exe' }

# ---------------------------------------------------------------- locations

function Get-HomeDir {
    if ($env:USERPROFILE) { return $env:USERPROFILE }
    if ($env:HOME) { return $env:HOME }
    return [System.IO.Path]::GetTempPath()
}

function Get-DataDir {
    if ($script:OnWindows) {
        $base = $env:LOCALAPPDATA
        if ([string]::IsNullOrEmpty($base)) { $base = [System.IO.Path]::Combine((Get-HomeDir), 'AppData', 'Local') }
        return [System.IO.Path]::Combine($base, 'Snipdeck')
    }
    $xdg = $env:XDG_DATA_HOME
    if ([string]::IsNullOrEmpty($xdg)) { $xdg = [System.IO.Path]::Combine((Get-HomeDir), '.local', 'share') }
    return [System.IO.Path]::Combine($xdg, 'snipdeck')
}

function Get-LogDir { return [System.IO.Path]::Combine((Get-DataDir), 'logs') }

function Get-BackupDir {
    $docs = ''
    try { $docs = [Environment]::GetFolderPath('MyDocuments') } catch { }
    if ([string]::IsNullOrEmpty($docs)) { $docs = [System.IO.Path]::Combine((Get-HomeDir), 'Documents') }
    return [System.IO.Path]::Combine($docs, 'Snipdeck-backups')
}

function Get-DistDir { return [System.IO.Path]::Combine($script:RepoRoot, 'dist') }
function Get-CacheExe { return [System.IO.Path]::Combine((Get-DistDir), $script:ExeName) }
function Get-StampFile { return [System.IO.Path]::Combine((Get-DistDir), 'snipdeck-app.stamp') }
function Get-TargetDirectory {
    if ($script:TargetOverride) { return $script:TargetOverride }
    return (Get-TargetDir $script:RepoRoot)
}


# ---------------------------------------------------------------- logging

function Start-Log([string]$Command) {
    $dir = Get-LogDir
    $ok = $false
    try { New-Item -ItemType Directory -Path $dir -Force | Out-Null; $ok = $true } catch { }
    if (-not $ok) {
        $dir = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), 'snipdeck-logs')
        try { New-Item -ItemType Directory -Path $dir -Force | Out-Null; $ok = $true } catch { }
    }
    if (-not $ok) { return }
    $script:LogPath = [System.IO.Path]::Combine($dir, 'launcher-' + (Get-Date).ToString('yyyyMMdd') + '.log')
    $script:LogOn = $true
    [void](Invoke-Rotation $dir '^launcher-\d{8}\.log$' 10)
    [void](Invoke-Rotation $dir '^doctor-(?<stamp>\d{8}-\d{6})\.txt$' 10)
    [void](Invoke-Rotation $dir '^app-(?<stamp>\d{8}-\d{6})\.(?<tag>out|err)\.txt$' 10)
    Write-Log 'INFO' ('---- launcher v{0}, command "{1}", PowerShell {2}, repo {3}' -f $script:LauncherVersion, $Command, $PSVersionTable.PSVersion, $script:RepoRoot)
}

function Stop-Log { $script:LogOn = $false }

function Write-Log([string]$Level, [string]$Message) {
    if (-not $script:LogOn) { return }
    try {
        $line = '{0} [{1}] {2}' -f (Get-Date).ToString('yyyy-MM-dd HH:mm:ss'), $Level, $Message
        [System.IO.File]::AppendAllText($script:LogPath, $line + "`r`n")
    } catch { }
}

function Write-LogRaw([string]$Text) {
    if (-not $script:LogOn -or [string]::IsNullOrEmpty($Text)) { return }
    try { [System.IO.File]::AppendAllText($script:LogPath, $Text) } catch { }
}

# ---------------------------------------------------------------- console

function Say([string]$Text = '', [string]$Color = 'Gray') {
    Write-Host $Text -ForegroundColor $Color
    Write-Log 'OUT' $Text
}

function Say-Step([string]$Text) {
    Say ''
    Say $Text 'Cyan'
}

function Test-Interactive {
    try { return (-not [Console]::IsInputRedirected) } catch { return $true }
}

function Ask-YesNo([string]$Question, [bool]$Default = $true) {
    $hint = '[y/N]'
    if ($Default) { $hint = '[Y/n]' }
    if ($script:Opt.WhatIf) {
        Say "  [whatif] would ask: $Question $hint (assuming the default)" 'DarkGray'
        return $Default
    }
    if ($script:Opt.Yes) {
        Say "  $Question -> yes (-Yes)" 'DarkGray'
        return $true
    }
    if (-not (Test-Interactive)) {
        Say "  $Question -> no (no keyboard attached; use -Yes to accept automatically)" 'DarkGray'
        return $false
    }
    $ans = Read-Host "  $Question $hint"
    Write-Log 'ASK' "$Question -> '$ans'"
    if ([string]::IsNullOrWhiteSpace($ans)) { return $Default }
    return (@('y', 'yes') -contains $ans.Trim().ToLowerInvariant())
}

function Read-Typed([string]$Prompt) {
    if ($script:Opt.WhatIf -or -not (Test-Interactive)) { return '' }
    $ans = Read-Host "  $Prompt"
    if ($null -eq $ans) { return '' }
    return $ans
}

function Wait-ForEnter {
    if ($script:Opt.Yes -or -not (Test-Interactive)) { return }
    [void](Read-Host '  Press Enter to close this window')
}

function Show-Check($Check) {
    $color = 'Gray'
    switch ($Check.Status) {
        'pass' { $color = 'Green' }
        'warn' { $color = 'Yellow' }
        'fail' { $color = 'Red' }
        default { $color = 'DarkGray' }
    }
    Say ('  ' + (Format-CheckLine $Check)) $color
    if ($Check.Fix -and $Check.Status -ne 'pass') { Say ('         -> ' + $Check.Fix) 'DarkGray' }
}

function Write-FailureReport([string]$What, [string]$Happened, [string[]]$Tried, [string[]]$Todo, [string[]]$Detail = @()) {
    # Every failure ends here: what happened, what was tried, what to do, and where the log is.
    Say ''
    Say "Snipdeck could not $What." 'Red'
    Say "  What happened: $Happened" 'Yellow'
    if (@($Detail).Count -gt 0) {
        foreach ($d in $Detail) { Say "    | $d" 'DarkGray' }
    }
    if (@($Tried).Count -gt 0) {
        Say '  What was tried:' 'Yellow'
        foreach ($t in $Tried) { Say "    - $t" 'Gray' }
    } else {
        Say '  What was tried: nothing automatic applied.' 'Yellow'
    }
    Say '  What to do:' 'Yellow'
    $i = 1
    foreach ($t in $Todo) { Say "    $i. $t" 'Gray'; $i++ }
    if ($script:LogPath) { Say "  Log file: $script:LogPath" 'Gray' }
    Say ''
}

# ---------------------------------------------------------------- files and safe deletion

function Get-DirSize([string]$Path, [int]$MaxSeconds = 4) {
    # Bytes in a folder, counted for at most $MaxSeconds (a big target folder takes long to walk).
    $script:LastSizeComplete = $true
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $sum = [int64]0
    try {
        foreach ($f in [System.IO.Directory]::EnumerateFiles($Path, '*', [System.IO.SearchOption]::AllDirectories)) {
            try { $sum += (New-Object System.IO.FileInfo($f)).Length } catch { }
            if ($sw.Elapsed.TotalSeconds -gt $MaxSeconds) { $script:LastSizeComplete = $false; break }
        }
    } catch { }
    return $sum
}

function Format-DirSize([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return 'not present' }
    if (Test-Path -LiteralPath $Path -PathType Leaf) { return (Format-Bytes ((Get-Item -LiteralPath $Path).Length)) }
    $b = Get-DirSize $Path
    if ($script:LastSizeComplete) { return (Format-Bytes $b) }
    return ('at least ' + (Format-Bytes $b))
}

function Register-OwnedTemp([string]$Path) { $script:OwnedTemp += (Get-NormalPath $Path) }

function Test-KnownDeletable([string]$Path) {
    # Only these may ever be deleted: the dist cache, the build folder, the data folder, backups inside the
    # backup folder, and temp folders this run created itself.
    $exact = @((Get-DistDir), (Get-DataDir)) + @(Get-CandidateTargets) + @($script:OwnedTemp)
    foreach ($e in $exact) { if ($e -and (Test-PathEquals $Path $e)) { return $true } }
    $roots = @((Get-DistDir), (Get-DataDir), (Get-BackupDir)) + @(Get-CandidateTargets) + @($script:OwnedTemp)
    foreach ($r in $roots) { if ($r -and (Test-PathUnder $Path $r)) { return $true } }
    return $false
}

function Remove-KnownPath([string]$Path) {
    # Returns 'removed', 'absent', 'whatif' or 'failed'. Throws when the path is not one Snipdeck owns.
    $full = Get-NormalPath $Path
    if (-not $full) { throw 'Refusing to delete an invalid path.' }
    if (-not (Test-KnownDeletable $full)) { throw "Refusing to delete '$full': it is not a file or folder that Snipdeck owns." }
    if (-not (Test-Path -LiteralPath $full)) { return 'absent' }
    if (Test-ReparsePoint $full) { throw "Refusing to delete '$full': it is a link, not a real folder." }
    $isDir = Test-Path -LiteralPath $full -PathType Container
    $leaf = Split-Path -Leaf $full
    foreach ($ct in (Get-CandidateTargets)) {
        if (Test-PathEquals $full $ct) {
            if (@('target', 'sdtarget') -notcontains $leaf.ToLowerInvariant()) { throw "Refusing to delete '$full': unexpected folder name." }
            if (-not (Test-LooksLikeCargoTarget $full)) { throw "Refusing to delete '$full': it does not look like a cargo build folder." }
        }
    }
    if (Test-PathEquals $full (Get-DistDir)) {
        if ($leaf.ToLowerInvariant() -ne 'dist') { throw "Refusing to delete '$full': unexpected folder name." }
    }
    if (Test-PathEquals $full (Get-DataDir)) {
        if ($leaf.ToLowerInvariant() -ne 'snipdeck') { throw "Refusing to delete '$full': unexpected folder name." }
    }
    if ($script:Opt.WhatIf) {
        Say "  [whatif] would remove $full" 'Yellow'
        return 'whatif'
    }
    Write-Log 'DELETE' $full
    if (-not $isDir) {
        try { Remove-Item -LiteralPath $full -Force -ErrorAction Stop } catch { Write-Log 'WARN' "could not delete file: $($_.Exception.Message)" }
        if (Test-Path -LiteralPath $full) { return 'failed' }
        return 'removed'
    }
    # Folder: cmd's rmdir is much faster than Remove-Item on a big build folder and does not follow links.
    if ($script:OnWindows) {
        try { & "$env:ComSpec" /d /c rmdir /s /q "$full" 2>$null | Out-Null } catch { }
    }
    if (Test-Path -LiteralPath $full) {
        try { Remove-Item -LiteralPath $full -Recurse -Force -ErrorAction Stop } catch { Write-Log 'WARN' "Remove-Item: $($_.Exception.Message)" }
    }
    if (Test-Path -LiteralPath $full) { return 'failed' }
    return 'removed'
}

# ---------------------------------------------------------------- running programs

function Get-AppProcesses {
    return @(Get-Process -Name 'snipdeck-app' -ErrorAction SilentlyContinue)
}

function Stop-ProcessTree([int]$ProcessId) {
    if ($script:OnWindows) {
        try { & taskkill.exe /PID $ProcessId /T /F 2>$null | Out-Null } catch { }
    }
    try { Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue } catch { }
}

function Stop-AppProcesses {
    # Stops only processes named exactly snipdeck-app. Closes the window first, then ends what is left.
    foreach ($p in (Get-AppProcesses)) {
        $where = ''
        try { $where = $p.Path } catch { }
        if ($script:Opt.WhatIf) { Say "  [whatif] would stop snipdeck-app.exe (process $($p.Id)) $where" 'Yellow'; continue }
        Say "  Stopping snipdeck-app.exe (process $($p.Id))" 'Gray'
        Write-Log 'STOP' "snipdeck-app pid $($p.Id) $where"
        try { [void]$p.CloseMainWindow() } catch { }
        $gone = $false
        try { $gone = $p.WaitForExit(3000) } catch { }
        if (-not $gone) { try { Stop-Process -Id $p.Id -Force -ErrorAction Stop } catch { Write-Log 'WARN' $_.Exception.Message } }
    }
    Start-Sleep -Milliseconds 400
    return (@(Get-AppProcesses).Count -eq 0)
}

function Get-LockHolders([int]$ExceptPid = 0) {
    # Lines describing cargo/rustc-related processes that may hold the build folder.
    $out = @()
    $names = @('cargo', 'rustc', 'rust-analyzer', 'cargo-clippy', 'clippy-driver', 'rustup')
    if ($script:OnWindows) {
        try {
            $filter = ($names | ForEach-Object { "Name='$_.exe'" }) -join ' OR '
            foreach ($p in @(Get-CimInstance Win32_Process -Filter $filter -ErrorAction Stop)) {
                if ($p.ProcessId -eq $ExceptPid) { continue }
                $cl = [string]$p.CommandLine
                if ($cl.Length -gt 140) { $cl = $cl.Substring(0, 140) + '...' }
                $out += ('{0} (process {1}): {2}' -f $p.Name, $p.ProcessId, (Hide-UserName $cl))
            }
        } catch { }
    } else {
        foreach ($p in @(Get-Process -Name $names -ErrorAction SilentlyContinue)) {
            if ($p.Id -ne $ExceptPid) { $out += ('{0} (process {1})' -f $p.ProcessName, $p.Id) }
        }
    }
    return @($out)
}

function New-TailState([string]$Path) {
    return @{ Path = $Path; Offset = [int64]0; Decoder = [System.Text.Encoding]::UTF8.GetDecoder() }
}

function Read-Tail($State) {
    # New text appended to a file since the last call (the file may still be open by the writer).
    if (-not (Test-Path -LiteralPath $State.Path)) { return '' }
    $fs = $null
    try {
        $fs = New-Object System.IO.FileStream($State.Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        if ($fs.Length -le $State.Offset) { return '' }
        [void]$fs.Seek($State.Offset, [System.IO.SeekOrigin]::Begin)
        $len = [int]($fs.Length - $State.Offset)
        $buf = New-Object byte[] $len
        $n = $fs.Read($buf, 0, $len)
        $State.Offset += $n
        $count = $State.Decoder.GetCharCount($buf, 0, $n)
        $chars = New-Object char[] $count
        $c = $State.Decoder.GetChars($buf, 0, $n, $chars, 0)
        return (New-Object System.String($chars, 0, $c))
    } catch {
        return ''
    } finally {
        if ($fs) { $fs.Dispose() }
    }
}

function Invoke-Native {
    # Runs a program, captures stdout+stderr, optionally echoes and logs it live (-Stream).
    # Returns ExitCode, Out, Err, Text, TimedOut, LockGaveUp, Started.
    param(
        [string]$Exe,
        [string[]]$ArgList = @(),
        [string]$WorkDir = '',
        [int]$TimeoutSec = 0,
        [switch]$Stream,
        [int]$LockWaitSec = 600
    )
    $id = [Guid]::NewGuid().ToString('N')
    $tmp = [System.IO.Path]::GetTempPath()
    $outFile = [System.IO.Path]::Combine($tmp, "snipdeck-$id.out")
    $errFile = [System.IO.Path]::Combine($tmp, "snipdeck-$id.err")
    $res = [ordered]@{ ExitCode = -1; Out = ''; Err = ''; Text = ''; TimedOut = $false; LockGaveUp = $false; Started = $false }
    $proc = $null
    Write-Log 'RUN' ("{0} {1}" -f $Exe, (Hide-UserName (ConvertTo-ArgString $ArgList)))
    try {
        $sp = @{ FilePath = $Exe; PassThru = $true; RedirectStandardOutput = $outFile; RedirectStandardError = $errFile; NoNewWindow = $true }
        if (@($ArgList).Count -gt 0) { $sp['ArgumentList'] = (ConvertTo-ArgString $ArgList) }
        if ($WorkDir) { $sp['WorkingDirectory'] = $WorkDir }
        $proc = Start-Process @sp
        $null = $proc.Handle   # without this, ExitCode can come back empty on Windows PowerShell 5.1
        $res.Started = $true
        $outSt = New-TailState $outFile
        $errSt = New-TailState $errFile
        $sbOut = New-Object System.Text.StringBuilder
        $sbErr = New-Object System.Text.StringBuilder
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $lockSince = $null
        $lockReported = $false
        while ($true) {
            $a = Read-Tail $outSt
            $b = Read-Tail $errSt
            if ($a) { [void]$sbOut.Append($a) }
            if ($b) { [void]$sbErr.Append($b) }
            $fresh = $a + $b
            if ($fresh) {
                if ($Stream) { Write-Host $fresh -NoNewline; Write-LogRaw $fresh }
                if ($fresh -match 'Blocking waiting for file lock') {
                    if (-not $lockSince) { $lockSince = Get-Date }
                    if (-not $lockReported) {
                        $lockReported = $true
                        Say ''
                        Say '  Another program is using the build folder, so cargo is waiting its turn.' 'Yellow'
                        $holders = @(Get-LockHolders $proc.Id)
                        if ($holders.Count -gt 0) {
                            Say '  These programs may be holding it:' 'Yellow'
                            foreach ($h in $holders) { Say "    - $h" 'Gray' }
                        } else {
                            Say '  No other cargo or rustc process was found; an editor (rust-analyzer) or antivirus may have it.' 'Yellow'
                        }
                        Say "  Waiting up to $([int]($LockWaitSec / 60)) minutes. Close the other program to continue sooner." 'Gray'
                    }
                } else {
                    $lockSince = $null
                }
            }
            if ($proc.HasExited) { break }
            if ($TimeoutSec -gt 0 -and $sw.Elapsed.TotalSeconds -gt $TimeoutSec) {
                $res.TimedOut = $true
                Stop-ProcessTree $proc.Id
                break
            }
            if ($lockSince -and ((Get-Date) - $lockSince).TotalSeconds -gt $LockWaitSec) {
                $res.LockGaveUp = $true
                Stop-ProcessTree $proc.Id
                break
            }
            Start-Sleep -Milliseconds 150
        }
        try { [void]$proc.WaitForExit(5000) } catch { }
        Start-Sleep -Milliseconds 100
        $a = Read-Tail $outSt
        $b = Read-Tail $errSt
        if ($a) { [void]$sbOut.Append($a) }
        if ($b) { [void]$sbErr.Append($b) }
        $fresh = $a + $b
        if ($fresh -and $Stream) { Write-Host $fresh -NoNewline; Write-LogRaw $fresh }
        $res.Out = $sbOut.ToString()
        $res.Err = $sbErr.ToString()
        $res.Text = $res.Out + "`n" + $res.Err
        if ($proc.HasExited) { $res.ExitCode = [int]$proc.ExitCode }
        if ($res.LockGaveUp) { $res.Text += "`nBlocking waiting for file lock (launcher stopped waiting)" }
    } catch {
        $res.Err = [string]$_.Exception.Message
        $res.Text = $res.Err
        Write-Log 'ERROR' "could not run ${Exe}: $($_.Exception.Message)"
    } finally {
        if ($proc -and -not $proc.HasExited) { Stop-ProcessTree $proc.Id }
        foreach ($f in @($outFile, $errFile)) { try { if (Test-Path -LiteralPath $f) { Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue } } catch { } }
    }
    if (-not $Stream) { Write-Log 'RUN' ("exit {0}" -f $res.ExitCode) }
    return [pscustomobject]$res
}

function Invoke-Interactive([string]$Exe, [string[]]$ArgList) {
    # For installers that show their own progress or prompts (winget, rustup-init): not captured.
    Write-Log 'RUN' ("{0} {1}" -f $Exe, (ConvertTo-ArgString $ArgList))
    $code = -1
    try {
        & $Exe @ArgList | Out-Host
        $code = $LASTEXITCODE
    } catch {
        Write-Log 'ERROR' "could not run ${Exe}: $($_.Exception.Message)"
    }
    Write-Log 'RUN' "exit $code"
    return $code
}

function Find-Command([string]$Name) {
    $c = Get-Command $Name -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($c) { return [string]$c.Source }
    return $null
}

# ---------------------------------------------------------------- PATH helpers

function Add-CargoToPath {
    $home_ = $env:CARGO_HOME
    if ([string]::IsNullOrEmpty($home_)) { $home_ = [System.IO.Path]::Combine((Get-HomeDir), '.cargo') }
    $bin = [System.IO.Path]::Combine($home_, 'bin')
    if ((Test-Path -LiteralPath $bin) -and ($env:PATH -notlike "*$bin*")) { $env:PATH = $bin + [System.IO.Path]::PathSeparator + $env:PATH }
}

function Update-PathFromRegistry {
    # After an installer ran, make its PATH changes visible to this window.
    if (-not $script:OnWindows) { return }
    try {
        $have = @($env:PATH -split ';' | Where-Object { $_ })
        foreach ($scope in @('Machine', 'User')) {
            $p = [Environment]::GetEnvironmentVariable('Path', $scope)
            foreach ($e in @($p -split ';' | Where-Object { $_ })) {
                if ($have -notcontains $e) { $env:PATH = $env:PATH + ';' + $e; $have += $e }
            }
        }
    } catch { }
}

# ---------------------------------------------------------------- environment checks

function Get-FreeBytes([string]$Path) {
    try {
        $p = Get-NormalPath $Path
        while ($p -and -not (Test-Path -LiteralPath $p)) {
            $parent = Split-Path -Parent $p
            if ([string]::IsNullOrEmpty($parent) -or $parent -eq $p) { break }
            $p = $parent
        }
        $root = [System.IO.Path]::GetPathRoot($p)
        $di = New-Object System.IO.DriveInfo($root)
        return [int64]$di.AvailableFreeSpace
    } catch { return $null }
}

function Test-DiskSpace([string]$Dir) {
    $root = [System.IO.Path]::GetPathRoot((Get-NormalPath $Dir))
    $free = Get-FreeBytes $Dir
    if ($null -eq $free) {
        return (New-Check 'Disk space' 'warn' "could not measure free space on $root" 'Make sure about 8 GB are free there.')
    }
    if ($free -lt $script:MinFreeBytes) {
        return (New-Check 'Disk space' 'fail' ("{0} free on {1}, 8 GB needed for the first build" -f (Format-Bytes $free), $root) "Free some space on $root, or run 'Snipdeck.bat clean' to delete old build files.")
    }
    return (New-Check 'Disk space' 'pass' ("{0} free on {1}" -f (Format-Bytes $free), $root))
}

function Find-Cargo {
    Add-CargoToPath
    return (Find-Command 'cargo')
}

function Get-ToolchainChannel {
    $f = [System.IO.Path]::Combine($script:RepoRoot, 'rust-toolchain.toml')
    try {
        $t = [System.IO.File]::ReadAllText($f)
        if ($t -match '(?m)^\s*channel\s*=\s*"([^"]+)"') { return $Matches[1] }
    } catch { }
    return 'stable'
}

function Test-RustInstalled {
    $cargo = Find-Cargo
    if (-not $cargo) {
        return (New-Check 'Rust (cargo)' 'fail' 'not installed' 'Snipdeck can install it for you (winget, or the official rustup installer).')
    }
    $r = Invoke-Native (Find-Command 'rustc') @('--version') $script:RepoRoot 1200
    if ($r.ExitCode -ne 0) {
        $first = (Get-LastLines $r.Text 1) -join ''
        return (New-Check 'Rust (cargo)' 'fail' "cargo is installed but rustc does not run: $first" 'Snipdeck can install the toolchain that rust-toolchain.toml asks for.')
    }
    return (New-Check 'Rust (cargo)' 'pass' ($r.Out.Trim()))
}

function Test-RustHost {
    if (-not $script:OnWindows) { return (New-Check 'Rust target' 'info' 'not Windows') }
    $rustc = Find-Command 'rustc'
    if (-not $rustc) { return (New-Check 'Rust target' 'fail' 'rustc not found') }
    $r = Invoke-Native $rustc @('-vV') $script:RepoRoot 60
    if ($r.Out -match '(?m)^host:\s*(\S+)') {
        $h = $Matches[1]
        if ($h -match 'msvc') { return (New-Check 'Rust target' 'pass' $h) }
        return (New-Check 'Rust target' 'warn' "$h (not MSVC)" 'Snipdeck can switch to the MSVC toolchain (rustup default stable-x86_64-pc-windows-msvc).')
    }
    return (New-Check 'Rust target' 'warn' 'could not read the toolchain host')
}

function Get-VsWherePath {
    $pf = ${env:ProgramFiles(x86)}
    if ([string]::IsNullOrEmpty($pf)) { return $null }
    $p = [System.IO.Path]::Combine($pf, 'Microsoft Visual Studio', 'Installer', 'vswhere.exe')
    if (Test-Path -LiteralPath $p) { return $p }
    return $null
}

function Test-MsvcInstalled {
    if (-not $script:OnWindows) { return (New-Check 'C++ build tools' 'info' 'not needed off Windows') }
    $vsw = Get-VsWherePath
    if ($vsw) {
        $r = Invoke-Native $vsw @('-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-latest', '-property', 'installationPath') '' 60
        $path = $r.Out.Trim()
        if ($r.ExitCode -eq 0 -and $path) { return (New-Check 'C++ build tools' 'pass' "Visual Studio C++ tools at $(Hide-UserName $path)") }
    }
    $link = Find-Command 'link.exe'
    if ($link -and $link -notmatch '(?i)[\\/]usr[\\/]bin[\\/]') {
        return (New-Check 'C++ build tools' 'pass' "link.exe on PATH ($(Hide-UserName $link))")
    }
    return (New-Check 'C++ build tools' 'fail' 'Visual Studio Build Tools with the C++ workload not found' 'Snipdeck can install them with winget (a few GB, shows an admin prompt).')
}

function Test-GitInstalled {
    $git = Find-Command 'git'
    if (-not $git) { return (New-Check 'git' 'info' 'not installed (optional: only needed for "update" and the fastest cache check)') }
    $r = Invoke-Native $git @('--version') '' 30
    return (New-Check 'git' 'pass' ($r.Out.Trim()))
}

function Test-RegistryCache {
    $home_ = $env:CARGO_HOME
    if ([string]::IsNullOrEmpty($home_)) { $home_ = [System.IO.Path]::Combine((Get-HomeDir), '.cargo') }
    $cache = [System.IO.Path]::Combine($home_, 'registry', 'cache')
    if (-not (Test-Path -LiteralPath $cache)) { return $false }
    $one = Get-ChildItem -LiteralPath $cache -Recurse -Filter '*.crate' -File -ErrorAction SilentlyContinue | Select-Object -First 1
    return [bool]$one
}

function Test-NetworkReachable {
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch { }
    foreach ($url in @('https://index.crates.io/config.json')) {
        try {
            $req = [Net.WebRequest]::Create($url)
            $req.Method = 'HEAD'
            $req.Timeout = 8000
            try { $req.Proxy = [Net.WebRequest]::GetSystemWebProxy(); $req.Proxy.Credentials = [Net.CredentialCache]::DefaultCredentials } catch { }
            $resp = $req.GetResponse()
            $resp.Close()
            return $true
        } catch [Net.WebException] {
            # An HTTP error status still proves the server was reached.
            if ($_.Exception.Response) { return $true }
        } catch { }
    }
    return $false
}

function Test-NetworkCheck {
    if (Test-NetworkReachable) { return (New-Check 'Network (crates.io)' 'pass' 'index.crates.io reachable') }
    if (Test-RegistryCache) {
        return (New-Check 'Network (crates.io)' 'warn' 'not reachable, but downloaded packages are cached' 'The build can continue offline.')
    }
    return (New-Check 'Network (crates.io)' 'warn' 'not reachable and nothing cached yet' 'Connect to the internet (check VPN or proxy); the first build downloads packages.')
}

# ---------------------------------------------------------------- installers

function Get-RustupUrls {
    $arch = 'x86_64'
    $pa = $env:PROCESSOR_ARCHITECTURE
    if ($pa -eq 'ARM64') { $arch = 'aarch64' } elseif ($pa -eq 'x86' -and -not $env:PROCESSOR_ARCHITEW6432) { $arch = 'i686' }
    $triple = "$arch-pc-windows-msvc"
    return @("https://win.rustup.rs/$arch", "https://static.rust-lang.org/rustup/dist/$triple/rustup-init.exe")
}

function Save-Download([string[]]$Urls, [string]$Dest, [int64]$MinBytes) {
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch { }
    foreach ($u in $Urls) {
        $wc = $null
        try {
            Say "  Downloading $u" 'Gray'
            $wc = New-Object Net.WebClient
            try { $wc.Proxy = [Net.WebRequest]::GetSystemWebProxy(); $wc.Proxy.Credentials = [Net.CredentialCache]::DefaultCredentials } catch { }
            $wc.DownloadFile($u, $Dest)
            $len = (Get-Item -LiteralPath $Dest).Length
            $head = New-Object byte[] 2
            $fs = [System.IO.File]::OpenRead($Dest)
            try { [void]$fs.Read($head, 0, 2) } finally { $fs.Dispose() }
            if ($len -ge $MinBytes -and $head[0] -eq 0x4D -and $head[1] -eq 0x5A) {
                Write-Log 'INFO' ("downloaded {0} ({1} bytes, sha256 {2})" -f $u, $len, (Get-FileSha256 $Dest))
                return $true
            }
            Write-Log 'WARN' "download from $u was not a valid program ($len bytes)"
        } catch {
            Write-Log 'WARN' "download from $u failed: $($_.Exception.Message)"
        } finally {
            if ($wc) { $wc.Dispose() }
        }
        try { if (Test-Path -LiteralPath $Dest) { Remove-Item -LiteralPath $Dest -Force } } catch { }
    }
    return $false
}

function Install-Toolchain {
    if ($script:Opt.WhatIf) { Say '  [whatif] would install the Rust toolchain with rustup.' 'Yellow'; return $false }
    $rustup = Find-Command 'rustup'
    if (-not $rustup) { return $false }
    $channel = Get-ToolchainChannel
    Say "  Installing the Rust toolchain '$channel' (rustup)..." 'Gray'
    $r = Invoke-Native $rustup @('toolchain', 'install', $channel, '--profile', 'minimal') $script:RepoRoot 1800 -Stream
    return ($r.ExitCode -eq 0)
}

function Install-Rust {
    # winget first, then the official rustup-init.exe; both are tried at most once.
    if ($script:Opt.WhatIf) { Say '  [whatif] would install Rust with winget or the official rustup installer.' 'Yellow'; return $false }
    Say '  Installing Rust...' 'Cyan'
    $winget = Find-Command 'winget'
    if ($winget) {
        $code = Invoke-Interactive $winget @('install', '--id', 'Rustlang.Rustup', '-e', '--accept-source-agreements', '--accept-package-agreements')
        Update-PathFromRegistry
        if (Find-Cargo) { [void](Install-Toolchain); return $true }
        Say "  winget finished (exit $code) but cargo is still not available. Trying the official installer." 'Yellow'
    } else {
        Say '  winget is not available on this PC. Using the official rustup installer.' 'Yellow'
    }
    $exe = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), 'snipdeck-rustup-init.exe')
    Register-OwnedTemp $exe
    if (Save-Download (Get-RustupUrls) $exe 1MB) {
        [void](Invoke-Interactive $exe @('-y', '--default-toolchain', (Get-ToolchainChannel), '--profile', 'minimal'))
        try { Remove-Item -LiteralPath $exe -Force -ErrorAction SilentlyContinue } catch { }
        Update-PathFromRegistry
        if (Find-Cargo) { return $true }
    }
    return $false
}

function Install-MsvcTools {
    if ($script:Opt.WhatIf) { Say '  [whatif] would offer to install the Microsoft C++ Build Tools.' 'Yellow'; return $false }
    Say '  The Microsoft C++ Build Tools are needed to link Windows programs.' 'Gray'
    Say '  It is a large download (a few GB) and shows an administrator (UAC) prompt. Accept it and wait.' 'Gray'
    if (-not (Ask-YesNo 'Install the C++ Build Tools now?' $true)) { return $false }
    $override = '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    $winget = Find-Command 'winget'
    $code = -1
    if ($winget) {
        $code = Invoke-Interactive $winget @('install', '--id', 'Microsoft.VisualStudio.2022.BuildTools', '-e', '--accept-source-agreements', '--accept-package-agreements', '--override', $override)
    }
    if ($code -ne 0 -and $code -ne 3010 -and -not (Test-MsvcInstalled).Status.Equals('pass')) {
        Say '  Trying the Visual Studio Build Tools bootstrapper instead.' 'Yellow'
        $exe = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), 'snipdeck-vs_BuildTools.exe')
        Register-OwnedTemp $exe
        if (Save-Download @('https://aka.ms/vs/17/release/vs_BuildTools.exe') $exe 1MB) {
            try {
                $p = Start-Process -FilePath $exe -ArgumentList '--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended' -Verb RunAs -Wait -PassThru
                $code = $p.ExitCode
                Write-Log 'RUN' "vs_BuildTools exit $code"
            } catch { Write-Log 'WARN' "bootstrapper failed: $($_.Exception.Message)" }
            try { Remove-Item -LiteralPath $exe -Force -ErrorAction SilentlyContinue } catch { }
        }
    }
    Update-PathFromRegistry
    if ($code -eq 3010) { Say '  The installer asks for a restart. Restart Windows, then run Snipdeck.bat again.' 'Yellow' }
    return ((Test-MsvcInstalled).Status -eq 'pass')
}
