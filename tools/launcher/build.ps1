# build.ps1 - source stamp, preflight, build with self-healing, the dist cache and starting the app.
# Dot-sourced by snipdeck.ps1 after lib.ps1 and core.ps1. Windows PowerShell 5.1 compatible; ASCII only.

$script:RebuiltOnce = $false
$script:BackoffScale = 1.0

function Wait-Backoff([int]$Seconds) {
    # selftest sets BackoffScale to 0 so the retry paths run instantly
    $ms = [int]($Seconds * 1000 * $script:BackoffScale)
    if ($ms -gt 0) { Start-Sleep -Milliseconds $ms }
}

# ---------------------------------------------------------------- source stamp

function Get-SourceScanHash {
    # Fallback when git is missing or fails (for example a ZIP download): paths, sizes and times of the build inputs.
    $root = $script:RepoRoot
    $lines = New-Object 'System.Collections.Generic.List[string]'
    $crates = [System.IO.Path]::Combine($root, 'crates')
    if (Test-Path -LiteralPath $crates) {
        foreach ($f in @(Get-ChildItem -LiteralPath $crates -Recurse -File -Force -ErrorAction SilentlyContinue)) {
            if ($f.FullName -match '[\\/]target[\\/]') { continue }
            $rel = $f.FullName.Substring($root.Length)
            $lines.Add(('{0}|{1}|{2}' -f $rel, $f.Length, $f.LastWriteTimeUtc.Ticks))
        }
    }
    foreach ($name in @('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo\config.toml')) {
        $p = [System.IO.Path]::Combine($root, $name)
        if (Test-Path -LiteralPath $p) {
            $i = Get-Item -LiteralPath $p
            $lines.Add(('{0}|{1}|{2}' -f $name, $i.Length, $i.LastWriteTimeUtc.Ticks))
        }
    }
    $arr = $lines.ToArray()
    [System.Array]::Sort($arr, [System.StringComparer]::Ordinal)
    return (Get-StringSha256 ($arr -join "`n"))
}

function Get-SourceStamp {
    # Hash of what the build depends on: git (tree ids of the build inputs at HEAD, Cargo.lock, the uncommitted
    # diff and untracked build files) or, without git, a scan of the same files.
    $root = $script:RepoRoot
    $toolchain = ''
    $lock = ''
    $f = [System.IO.Path]::Combine($root, 'rust-toolchain.toml')
    if (Test-Path -LiteralPath $f) { $toolchain = Get-FileSha256 $f }
    $f = [System.IO.Path]::Combine($root, 'Cargo.lock')
    if (Test-Path -LiteralPath $f) { $lock = Get-FileSha256 $f }
    $head = ''
    $commit = ''
    $diff = ''
    $untracked = ''
    $method = 'git'
    $note = ''
    $git = Find-Command 'git'
    if ($git -and (Test-Path -LiteralPath ([System.IO.Path]::Combine($root, '.git')))) {
        try {
            $r = Invoke-Native $git @('-C', $root, 'rev-parse', 'HEAD') '' 30
            if ($r.ExitCode -ne 0) { throw ('git rev-parse failed: ' + ((Get-LastLines $r.Text 1) -join '')) }
            $commit = $r.Out.Trim()
            # Tree ids of the inputs: a commit that only touches docs does not change them.
            $r = Invoke-Native $git @('-C', $root, 'rev-parse', 'HEAD:crates', 'HEAD:Cargo.toml', 'HEAD:Cargo.lock', 'HEAD:rust-toolchain.toml') '' 30
            if ($r.ExitCode -eq 0) { $head = (($r.Out -split "\r?\n" | Where-Object { $_ }) -join ',') } else { $head = $commit }
            $paths = @('crates', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo')
            $r = Invoke-Native $git (@('-C', $root, '-c', 'core.quotepath=off', 'diff', 'HEAD', '--no-ext-diff', '--no-color', '--binary', '--') + $paths) '' 120
            if ($r.ExitCode -ne 0) { throw ('git diff failed: ' + ((Get-LastLines $r.Text 1) -join '')) }
            $diff = Get-StringSha256 $r.Out
            $r = Invoke-Native $git (@('-C', $root, '-c', 'core.quotepath=off', 'ls-files', '--others', '--exclude-standard', '--') + $paths) '' 60
            if ($r.ExitCode -ne 0) { throw ('git ls-files failed: ' + ((Get-LastLines $r.Text 1) -join '')) }
            $names = @($r.Out -split "\r?\n" | Where-Object { $_ } | Sort-Object)
            if ($names.Count -gt 5000) { throw 'too many untracked files' }
            $sb = New-Object System.Text.StringBuilder
            foreach ($n in $names) {
                $p = [System.IO.Path]::Combine($root, $n.Replace('/', [string][System.IO.Path]::DirectorySeparatorChar))
                $h = 'unreadable'
                try { $h = Get-FileSha256 $p } catch { }
                [void]$sb.Append("$n=$h`n")
            }
            $untracked = Get-StringSha256 $sb.ToString()
        } catch {
            $method = 'scan'
            $note = $_.Exception.Message
            Write-Log 'WARN' "git stamp failed, scanning files instead: $note"
        }
    } else {
        $method = 'scan'
        $note = 'git or the .git folder is not available'
    }
    if ($method -eq 'scan') {
        $head = 'scan'
        $diff = ''
        $untracked = Get-SourceScanHash
    }
    $hash = New-StampHash $head $lock $diff $untracked $toolchain 'debug' $script:LauncherVersion
    return [pscustomobject]@{ Hash = $hash; Head = $head; Commit = $commit; Method = $method; Note = $note }
}

function Read-StampFile {
    $sf = Get-StampFile
    if (-not (Test-Path -LiteralPath $sf)) { return $null }
    try { return ([System.IO.File]::ReadAllText($sf) | ConvertFrom-Json) } catch { return $null }
}

function Get-CacheState($Stamp) {
    # 'valid', 'stale' (source changed), 'missing' or 'corrupt'
    $exe = Get-CacheExe
    if (-not (Test-Path -LiteralPath $exe) -or -not (Test-Path -LiteralPath (Get-StampFile))) { return 'missing' }
    $s = Read-StampFile
    if ($null -eq $s) { return 'corrupt' }
    if (-not (Test-StampMatch $s $Stamp.Hash)) { return 'stale' }
    try {
        if ([int64]$s.exeLength -ne (Get-Item -LiteralPath $exe).Length) { return 'corrupt' }
    } catch { return 'corrupt' }
    return 'valid'
}

# ---------------------------------------------------------------- preflight

function Invoke-Preflight {
    Say-Step 'Checking this PC'
    $root = $script:RepoRoot
    if (-not (Test-Path -LiteralPath ([System.IO.Path]::Combine($root, 'crates', 'snipdeck-app')))) {
        Write-FailureReport 'start' 'this folder does not contain the Snipdeck source (crates\snipdeck-app is missing).' @() @('Run Snipdeck.bat from the folder you downloaded or cloned, with all its files.', 'If you moved only Snipdeck.bat, move the whole folder instead.')
        return $false
    }

    $c = Test-DiskSpace (Get-TargetDirectory)
    Show-Check $c
    if ($c.Status -eq 'fail') {
        $hasOld = (Test-Path -LiteralPath (Get-TargetDirectory)) -or (Test-Path -LiteralPath (Get-DistDir))
        if ($hasOld -and (Ask-YesNo 'Delete old build files (the same as "Snipdeck.bat clean") to make room?' $true)) {
            [void](Invoke-CleanBuild)
            $c = Test-DiskSpace (Get-TargetDirectory)
            Show-Check $c
        }
        if ($c.Status -eq 'fail') {
            Write-FailureReport 'build' $c.Detail @('Offered to clear old build files.') @('Free up disk space on that drive (empty the Recycle Bin, run Windows Disk Cleanup).', 'Or move the Snipdeck folder to a drive with more space.', 'Then run "Snipdeck.bat build" again.')
            return $false
        }
    }

    $c = Test-RustInstalled
    Show-Check $c
    if ($c.Status -eq 'fail') {
        if (-not (Find-Cargo)) {
            if (Ask-YesNo 'Install Rust now?' $true) { [void](Install-Rust) }
        } else {
            if (Ask-YesNo 'Install the Rust toolchain now?' $true) { [void](Install-Toolchain) }
        }
        $c = Test-RustInstalled
        Show-Check $c
        if ($c.Status -eq 'fail') {
            Write-FailureReport 'find Rust' $c.Detail @('Tried winget (Rustlang.Rustup) and the official rustup installer where allowed.') @('Install Rust from https://rustup.rs (choose the default options).', 'Close this window, then run "Snipdeck.bat build" again so the new PATH is picked up.')
            return $false
        }
    }

    $c = Test-RustHost
    Show-Check $c
    if ($c.Status -eq 'warn' -and $script:OnWindows) {
        if (Ask-YesNo 'Switch to the MSVC toolchain (rustup default stable-x86_64-pc-windows-msvc)?' $true) {
            $rustup = Find-Command 'rustup'
            if ($rustup) { [void](Invoke-Native $rustup @('default', 'stable-x86_64-pc-windows-msvc') '' 1200 -Stream) }
            Show-Check (Test-RustHost)
        }
    }

    $c = Test-MsvcInstalled
    Show-Check $c
    if ($c.Status -eq 'fail') {
        if (Install-MsvcTools) {
            $c = Test-MsvcInstalled
            Show-Check $c
        }
        if ($c.Status -eq 'fail') {
            Write-FailureReport 'find the C++ build tools' 'Visual Studio Build Tools with the C++ workload are not installed.' @('Offered to install them with winget and the Microsoft bootstrapper.') @('Install "Build Tools for Visual Studio 2022" from https://visualstudio.microsoft.com/visual-cpp-build-tools/ and tick "Desktop development with C++".', 'Restart Windows if the installer asks, then run "Snipdeck.bat build" again.')
            return $false
        }
    }

    Show-Check (Test-GitInstalled)
    Show-Check (Test-NetworkCheck)
    return $true
}

# ---------------------------------------------------------------- build with self-healing

function Get-FailureAdvice([string]$Id) {
    switch ($Id) {
        'network' { return @('Check that this PC is online (open https://crates.io in a browser; check VPN, proxy or firewall).', 'Run "Snipdeck.bat build" again: packages that already downloaded are kept.') }
        'net-revoke' { return @('Your network blocks certificate checks (common behind a company proxy). Ask IT, or run "Snipdeck.bat build" again and answer yes when asked to skip the check.') }
        'file-lock' { return @('Close editors or terminals that are building Snipdeck (rust-analyzer, other cargo windows).', 'Run "Snipdeck.bat build" again.') }
        'msvc-missing' { return @('Install the C++ Build Tools: https://visualstudio.microsoft.com/visual-cpp-build-tools/ ("Desktop development with C++").', 'Restart Windows if asked, then run "Snipdeck.bat build" again.') }
        'no-space' { return @('Free disk space, or run "Snipdeck.bat clean" to delete build files, then run "Snipdeck.bat build" again.') }
        'corrupt-build' { return @('Run "Snipdeck.bat clean", then "Snipdeck.bat build" again (a full rebuild takes several minutes).') }
        'lock-mismatch' { return @('Cargo.lock is out of date. Run "git pull" to get the matching one, or answer yes when asked to allow cargo to update it.') }
        'exe-locked' { return @('Quit Snipdeck from its tray icon (or close the window), then run "Snipdeck.bat build" again.') }
        'toolchain-old' { return @('Run "rustup update stable" in a terminal, then run "Snipdeck.bat build" again.') }
        'no-toolchain' { return @('Run "rustup toolchain install stable" in a terminal, then run "Snipdeck.bat build" again.') }
        'long-path' { return @('Move the Snipdeck folder to a short path such as C:\Snipdeck, then run "Snipdeck.bat build" again.') }
        'out-of-memory' { return @('Close other programs to free memory, then run "Snipdeck.bat build" again.') }
        'access-denied' { return @('Your antivirus may be scanning or blocking the build folder. Add an exclusion for the Snipdeck folder, then run "Snipdeck.bat build" again.') }
        default { return @('Run "Snipdeck.bat doctor" and send the report file together with the log file named below.') }
    }
}

function Invoke-Remedy($Next, [hashtable]$St, [string]$Text) {
    # Returns 'retry' (build again), 'skip' (try the next remedy without building) or 'abort'.
    $id = $Next.Remedy
    switch ($id) {
        'retry-5' {
            $St.Notes += 'waited 5 seconds and tried again'
            Say '  Waiting 5 seconds, then trying again.' 'Gray'
            Wait-Backoff 5
            return 'retry'
        }
        'retry-20' {
            $St.Notes += 'waited 20 seconds and tried again'
            Say '  Waiting 20 seconds, then trying again.' 'Gray'
            Wait-Backoff 20
            return 'retry'
        }
        'http-no-multiplex' {
            $env:CARGO_HTTP_MULTIPLEXING = 'false'
            $env:CARGO_NET_RETRY = '10'
            $St.Notes += 'switched cargo to plain one-at-a-time downloads with more retries'
            Say '  Switching to slower, more careful downloads for this run.' 'Gray'
            return 'retry'
        }
        'offline' {
            if (Test-RegistryCache) {
                $St.Offline = $true
                $St.Notes += 'built with --offline from the packages already downloaded'
                Say '  Using the packages already downloaded on this PC (--offline).' 'Gray'
                return 'retry'
            }
            $St.Notes += 'offline mode was not possible: no downloaded packages cached yet'
            Say '  No packages are cached on this PC yet, so offline mode is not possible.' 'Yellow'
            return 'skip'
        }
        'no-revoke-check' {
            Say '  Windows could not check whether a certificate was revoked (typical behind a proxy).' 'Gray'
            Say '  Turning that check off affects only cargo downloads in this run, from crates.io over HTTPS.' 'Gray'
            if (Ask-YesNo 'Skip the revocation check for this run?' $false) {
                $env:CARGO_HTTP_CHECK_REVOKE = 'false'
                $St.Notes += 'skipped the certificate revocation check for this run'
                return 'retry'
            }
            $St.Notes += 'declined to skip the certificate revocation check'
            return 'abort'
        }
        'wait-lock' {
            $holders = @(Get-LockHolders)
            if ($holders.Count -gt 0) {
                Say '  Programs that may hold the build folder:' 'Yellow'
                foreach ($h in $holders) { Say "    - $h" 'Gray' }
                $St.Notes += ('another program held the build folder: ' + ($holders -join '; '))
            } else {
                $St.Notes += 'the build folder was locked but no other cargo or rustc process was found'
            }
            Say '  Waiting 30 seconds, then trying again.' 'Gray'
            Wait-Backoff 30
            return 'retry'
        }
        'install-msvc' {
            if (Install-MsvcTools) {
                $St.Notes += 'installed the C++ Build Tools'
                return 'retry'
            }
            $St.Notes += 'the C++ Build Tools were not installed'
            return 'abort'
        }
        'offer-clean' {
            $free = Get-FreeBytes $St.Target
            $freeText = 'unknown'
            if ($null -ne $free) { $freeText = Format-Bytes $free }
            Say "  Free space on this drive: $freeText. The build needs several GB." 'Yellow'
            if (Ask-YesNo 'Delete the build folder (it is rebuilt automatically) to free space?' $true) {
                if (Invoke-CleanBuild) {
                    $St.Notes += 'deleted the build folder to free disk space'
                    return 'retry'
                }
            }
            $St.Notes += 'did not free disk space'
            return 'abort'
        }
        'clean-pkg' {
            $crates = @(Get-FailedCrates $Text)
            $inc = [System.IO.Path]::Combine($St.Target, 'debug', 'incremental')
            $did = $false
            if ((Test-Path -LiteralPath $inc) -and (Test-PathUnder $inc $St.Target) -and (Test-KnownDeletable $inc)) {
                Say '  Clearing the incremental build cache.' 'Gray'
                [void](Remove-KnownPath $inc)
                $did = $true
            }
            $cargo = Find-Cargo
            foreach ($c in $crates) {
                Say "  Cleaning the build files of '$c'." 'Gray'
                [void](Invoke-Native $cargo @('clean', '-p', $c) $script:RepoRoot 600 -Stream)
                $did = $true
            }
            if ($did) {
                $St.Notes += ('cleared the incremental cache and the build files of: ' + ($crates -join ', '))
                return 'retry'
            }
            return 'skip'
        }
        'clean-all' {
            Say '  Partly written build files are still a problem. The cure is deleting the whole build folder' 'Yellow'
            Say '  and building from scratch, which takes several minutes. Your library is not touched.' 'Yellow'
            if (Ask-YesNo 'Delete the build folder and rebuild?' $true) {
                if (Invoke-CleanBuild) {
                    $St.Notes += 'deleted the whole build folder and rebuilt'
                    return 'retry'
                }
            }
            $St.Notes += 'did not delete the build folder'
            return 'abort'
        }
        'unlocked-retry' {
            Say '  Cargo.lock does not match the project files (for example after an update or an edit).' 'Gray'
            Say '  Building without --locked lets cargo fix Cargo.lock. It may change that file.' 'Gray'
            if (Ask-YesNo 'Build without --locked?' $true) {
                $St.Locked = $false
                $St.Notes += 'built without --locked so cargo could update Cargo.lock'
                return 'retry'
            }
            $St.Notes += 'declined to build without --locked'
            return 'abort'
        }
        'short-target' {
            $short = [System.IO.Path]::Combine([System.IO.Path]::GetPathRoot($script:RepoRoot), 'sdtarget')
            if (Test-PathEquals $St.Target $short) { return 'skip' }
            $St.Target = $short
            $script:TargetOverride = $short
            $St.Notes += "moved the build folder to the shorter path $short"
            Say "  Building in a shorter folder: $short" 'Gray'
            return 'retry'
        }
        'update-rust' {
            $rustup = Find-Command 'rustup'
            if (-not $rustup) { return 'skip' }
            Say '  Updating Rust (rustup update stable)...' 'Gray'
            $r = Invoke-Native $rustup @('update', (Get-ToolchainChannel)) '' 1800 -Stream
            if ($r.ExitCode -eq 0) { $St.Notes += 'updated Rust with rustup'; return 'retry' }
            $St.Notes += 'rustup update failed'
            return 'abort'
        }
        'install-toolchain' {
            if (Install-Toolchain) { $St.Notes += 'installed the Rust toolchain'; return 'retry' }
            $St.Notes += 'could not install the Rust toolchain'
            return 'abort'
        }
        'fewer-jobs' {
            $env:CARGO_BUILD_JOBS = '2'
            $St.Notes += 'limited cargo to 2 parallel jobs to use less memory'
            Say '  Limiting the build to 2 parallel jobs to use less memory.' 'Gray'
            return 'retry'
        }
        'stop-app' {
            $running = @(Get-AppProcesses)
            if ($running.Count -eq 0) {
                $St.Notes += 'no running snipdeck-app.exe was found (antivirus may be holding the file)'
                return 'skip'
            }
            Say '  A running snipdeck-app.exe is locking the file that the build must replace.' 'Yellow'
            if (Ask-YesNo 'Stop the running snipdeck-app.exe (only that program)?' $true) {
                if (Stop-AppProcesses) { $St.Notes += 'stopped the running snipdeck-app.exe'; return 'retry' }
                $St.Notes += 'could not stop snipdeck-app.exe'
                return 'abort'
            }
            $St.Notes += 'declined to stop the running snipdeck-app.exe'
            return 'abort'
        }
    }
    return 'skip'
}

function Invoke-Build {
    # Builds the debug app (never --release: a release build needs the production licence key).
    # Returns the path of the built exe, or $null after printing a failure report.
    $cargo = Find-Cargo
    if ($env:SNIPDECK_FAKE_CARGO) { $cargo = $env:SNIPDECK_FAKE_CARGO }   # test hook: a script that prints sample output
    if (-not $cargo) {
        Write-FailureReport 'build' 'cargo was not found.' @() @('Run "Snipdeck.bat build" again; it offers to install Rust.')
        return $null
    }
    $st = @{ Locked = $true; Offline = $false; Target = (Get-TargetDirectory); Tried = @{}; Notes = @(); Attempt = 0 }
    $firstId = ''
    $lastText = ''
    $lastCode = 0
    $ok = $false
    while ($st.Attempt -lt 12) {
        $st.Attempt++
        $env:CARGO_TARGET_DIR = $st.Target
        $env:CARGO_TERM_COLOR = 'never'
        $argv = @('build', '-p', 'snipdeck-app')
        if ($st.Locked) { $argv += '--locked' }
        if ($st.Offline) { $argv += '--offline' }
        Say-Step ('Building Snipdeck (attempt {0}). The first build takes several minutes.' -f $st.Attempt)
        Say ('  cargo ' + ($argv -join ' ') + '   [build folder: ' + (Hide-UserName $st.Target) + ']') 'DarkGray'
        $r = Invoke-Native $cargo $argv $script:RepoRoot -Stream
        $lastText = $r.Text
        $lastCode = $r.ExitCode
        if ($r.ExitCode -eq 0) { $ok = $true; break }
        Write-Log 'BUILD' ("failed, exit {0}; known patterns: {1}" -f $r.ExitCode, ((Get-FailureMatches $r.Text) -join ','))
        $retry = $false
        while ($true) {
            $next = Get-NextRemedy $lastText $st.Tried
            if (-not $next) { break }
            $st.Tried[$next.Remedy] = $true
            if (-not $firstId) { $firstId = $next.FailureId }
            Say ''
            Say ('Problem found: ' + $next.Title) 'Yellow'
            $res = Invoke-Remedy $next $st $lastText
            Write-Log 'REMEDY' ("{0} for {1} -> {2}" -f $next.Remedy, $next.FailureId, $res)
            if ($res -eq 'retry') { $retry = $true; break }
            if ($res -eq 'abort') { break }
        }
        if (-not $retry) { break }
    }
    if (-not $ok) {
        $happened = "cargo stopped with exit code $lastCode."
        if ($firstId) { $happened = (Get-FailureTitle $firstId) + " (cargo exit code $lastCode)." }
        $tail = @(Get-LastLines (Hide-UserName $lastText) 12)
        $todo = @(Get-FailureAdvice $firstId)
        if ($firstId) { $todo += 'If it keeps failing, run "Snipdeck.bat doctor" and send the report with the log file named below.' }
        Write-FailureReport 'build' $happened $st.Notes $todo $tail
        return $null
    }
    $exe = [System.IO.Path]::Combine($st.Target, 'debug', $script:ExeName)
    if (-not (Test-Path -LiteralPath $exe)) {
        Write-FailureReport 'build' ("cargo reported success but the program file is missing: " + (Hide-UserName $exe)) $st.Notes @('Run "Snipdeck.bat clean" and then "Snipdeck.bat build" again.', 'If it happens again, send the log file named below.')
        return $null
    }
    if ($st.Notes.Count -gt 0) {
        Say ''
        Say 'The build worked after some automatic fixes:' 'Green'
        foreach ($n in $st.Notes) { Say "  - $n" 'Gray' }
    }
    return $exe
}

# ---------------------------------------------------------------- dist cache and launch

function Publish-Build([string]$BuiltExe, $Stamp) {
    # Copies the new exe to dist\ and writes the stamp; returns the exe to start.
    $dist = Get-DistDir
    try { New-Item -ItemType Directory -Path $dist -Force | Out-Null } catch {
        Say '  Could not create the dist folder; starting from the build folder instead.' 'Yellow'
        return $BuiltExe
    }
    $final = Get-CacheExe
    $stampFile = Get-StampFile
    $tmp = $final + '.new'
    for ($i = 0; $i -lt 2; $i++) {
        try {
            if (Test-Path -LiteralPath $stampFile) { Remove-Item -LiteralPath $stampFile -Force -ErrorAction Stop }
            Copy-Item -LiteralPath $BuiltExe -Destination $tmp -Force -ErrorAction Stop
            if (Test-Path -LiteralPath $final) { Remove-Item -LiteralPath $final -Force -ErrorAction Stop }
            Move-Item -LiteralPath $tmp -Destination $final -Force -ErrorAction Stop
            $rustc = ''
            $rc = Find-Command 'rustc'
            if ($rc) { $rustc = (Invoke-Native $rc @('--version') '' 60).Out.Trim() }
            $meta = [ordered]@{
                hash = $Stamp.Hash; head = $Stamp.Head; commit = $Stamp.Commit; method = $Stamp.Method
                exeLength = (Get-Item -LiteralPath $final).Length
                builtAt = (Get-Date).ToString('s'); launcher = $script:LauncherVersion; rustc = $rustc
                target = (Get-TargetDirectory)
            }
            $json = $meta | ConvertTo-Json
            $utf8 = New-Object System.Text.UTF8Encoding($false)
            $stampTmp = $stampFile + '.new'
            [System.IO.File]::WriteAllText($stampTmp, $json, $utf8)
            Move-Item -LiteralPath $stampTmp -Destination $stampFile -Force -ErrorAction Stop
            Say '  Saved a fast-start copy; the next start skips the build while the source is unchanged.' 'DarkGray'
            return $final
        } catch {
            Write-Log 'WARN' "publish to dist failed: $($_.Exception.Message)"
            if ($i -eq 0 -and @(Get-AppProcesses).Count -gt 0) {
                Say '  An older snipdeck-app.exe is still running, so the new copy cannot replace it.' 'Yellow'
                if (Ask-YesNo 'Stop the running snipdeck-app.exe (only that program)?' $true) {
                    if (Stop-AppProcesses) { continue }
                }
            }
            break
        }
    }
    try { if (Test-Path -LiteralPath $tmp) { Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue } } catch { }
    Say '  Could not save the fast-start copy; starting straight from the build folder. The next start will check again.' 'Yellow'
    return $BuiltExe
}

function Get-LaunchAdvice([int]$Code, [string]$ErrText) {
    if ($ErrText -match '(?i)locked') { return @('Another Snipdeck holds the library. Quit it from its tray icon and try again; a stale lock clears itself when the program ends.') }
    if ($Code -eq 2) { return @('Snipdeck did not understand an option. Run "Snipdeck.bat help" or start it without options.') }
    if (($Code -eq -1073741515) -or ($Code -eq -1073741701)) {
        return @('Windows could not load a part the program needs (a missing or wrong DLL).', 'Install the Visual C++ Redistributable: https://aka.ms/vs/17/release/vc_redist.x64.exe', 'Then run Snipdeck.bat again.')
    }
    return @('Run "Snipdeck.bat doctor" and send the report with the log file named below.')
}

function Start-SnipdeckApp([string]$Exe, [string[]]$AppArgs) {
    # Starts the app without a console window and waits a few seconds to see whether it stays up.
    # Returns Code (exit code if it ended), Running, and Crash (true when the exit looks like a loader crash).
    $res = [pscustomobject]@{ Code = 0; Running = $false; Crash = $false }
    $ts = (Get-Date).ToString('yyyyMMdd-HHmmss')
    $logDir = Get-LogDir
    try { New-Item -ItemType Directory -Path $logDir -Force | Out-Null } catch { $logDir = [System.IO.Path]::GetTempPath() }
    $outF = [System.IO.Path]::Combine($logDir, "app-$ts.out.txt")
    $errF = [System.IO.Path]::Combine($logDir, "app-$ts.err.txt")
    $argStr = ConvertTo-ArgString $AppArgs
    Say ''
    Say ('Starting Snipdeck' + $(if ($argStr) { " ($argStr)" } else { '' }) + '...') 'Cyan'
    Write-Log 'START' "$Exe $argStr"
    $p = $null
    try {
        if ($script:OnWindows) {
            $psi = New-Object System.Diagnostics.ProcessStartInfo
            $psi.FileName = $env:ComSpec
            $psi.Arguments = '/d /s /c ""' + $Exe + '" ' + $argStr + ' >"' + $outF + '" 2>"' + $errF + '""'
            $psi.WorkingDirectory = Split-Path -Parent $Exe
            $psi.UseShellExecute = $false
            $psi.CreateNoWindow = $true
            $p = [System.Diagnostics.Process]::Start($psi)
        } else {
            $sp = @{ FilePath = $Exe; PassThru = $true; RedirectStandardOutput = $outF; RedirectStandardError = $errF; NoNewWindow = $true }
            if ($argStr) { $sp['ArgumentList'] = $argStr }
            $p = Start-Process @sp
        }
    } catch {
        $res.Code = 1
        Write-FailureReport 'start' ('Windows refused to start the program: ' + $_.Exception.Message) @() @('If your antivirus quarantined the Snipdeck program file (app\snipdeck.exe, or dist\snipdeck-app.exe for a developer build), allow it and run Snipdeck.bat again (it downloads or rebuilds when the file is missing).', 'Run "Snipdeck.bat doctor" and send the report if it keeps happening.')
        return $res
    }
    $exited = $false
    try { $exited = $p.WaitForExit(4000) } catch { }
    if (-not $exited) {
        $res.Running = $true
        Say 'Snipdeck is running. Look for its tray icon; you can close this window.' 'Green'
        return $res
    }
    $code = [int]$p.ExitCode
    $res.Code = $code
    $outText = ''
    $errText = ''
    try { $outText = [System.IO.File]::ReadAllText($outF) } catch { }
    try { $errText = [System.IO.File]::ReadAllText($errF) } catch { }
    foreach ($l in (Get-LastLines $outText 15)) { Say "  $l" 'Gray' }
    if ($code -eq 0) {
        Say 'Snipdeck has finished (exit code 0). If you expected a window, it may have handed over to a copy that was already running.' 'Green'
        return $res
    }
    $res.Crash = ($code -lt 0)
    Write-FailureReport 'keep running' ("Snipdeck stopped right after starting (exit code $code).") @() (Get-LaunchAdvice $code $errText) @(Get-LastLines (Hide-UserName $errText) 10)
    return $res
}

# ---------------------------------------------------------------- the run / demo flow

function Invoke-RunFlow([bool]$Demo, [string[]]$AppArgs, [bool]$ForceBuild) {
    $appArgs2 = @()
    if ($Demo -and (@($AppArgs) -notcontains '--demo')) { $appArgs2 += '--demo' }
    $appArgs2 += @($AppArgs)
    if ($Demo) { Say 'Snipdeck demo: a seeded library in memory, nothing is saved.' 'Cyan' } else { Say 'Snipdeck: opening your real library.' 'Cyan' }

    $stamp = Get-SourceStamp
    $state = Get-CacheState $stamp
    Write-Log 'INFO' ("stamp {0} (method {1}); cache {2}" -f $stamp.Hash, $stamp.Method, $state)
    if ($stamp.Method -eq 'scan') { Say "  (Checking for changes by scanning files: $($stamp.Note))" 'DarkGray' }

    if ($script:Opt.WhatIf) {
        if ($state -eq 'valid' -and -not $ForceBuild) {
            Say '  [whatif] the cached build matches the source: would start dist\snipdeck-app.exe now.' 'Yellow'
        } else {
            Say "  [whatif] cache is '$state': would check this PC, build (debug), save dist\snipdeck-app.exe and start it." 'Yellow'
        }
        return 0
    }

    $fromCache = $false
    if ($state -eq 'valid' -and -not $ForceBuild) {
        $exe = Get-CacheExe
        $fromCache = $true
        Say 'Fast start: the source has not changed since the last build.' 'Green'
    } else {
        if ($state -eq 'stale') { Say 'The source changed since the last build, so it is rebuilt.' 'Gray' }
        if ($state -eq 'corrupt') { Say 'The saved fast-start copy looks damaged; rebuilding.' 'Yellow' }
        if (-not (Invoke-Preflight)) { return 1 }
        $built = Invoke-Build
        if (-not $built) { return 1 }
        $exe = Publish-Build $built $stamp
    }
    $r = Start-SnipdeckApp $exe $appArgs2
    if ($r.Crash -and $fromCache -and -not $script:RebuiltOnce) {
        $script:RebuiltOnce = $true
        Say 'The saved copy would not start; rebuilding it once.' 'Yellow'
        foreach ($p in @((Get-CacheExe), (Get-StampFile))) { try { if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force } } catch { } }
        return (Invoke-RunFlow $Demo $AppArgs $true)
    }
    if ($r.Code -ne 0) { return 1 }
    return 0
}
