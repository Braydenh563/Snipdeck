# data.ps1 - backup, restore, clean, uninstall, doctor and logs.
# Dot-sourced by snipdeck.ps1 after lib.ps1, core.ps1 and build.ps1. Windows PowerShell 5.1 compatible; ASCII only.

$script:KeepBackups = 10

# ---------------------------------------------------------------- backup

function Get-BackupFiles([string]$DataDir) {
    # Everything in the data folder except logs\, the lock file and temporary files.
    $out = @()
    if (-not (Test-Path -LiteralPath $DataDir -PathType Container)) { return @() }
    $prefix = $DataDir.TrimEnd([char]'\', [char]'/') + [string][System.IO.Path]::DirectorySeparatorChar
    foreach ($f in @(Get-ChildItem -LiteralPath $DataDir -Recurse -File -Force -ErrorAction SilentlyContinue)) {
        if (-not $f.FullName.StartsWith($prefix)) { continue }
        $rel = $f.FullName.Substring($prefix.Length)
        if ($rel -match '^logs[\\/]') { continue }
        if ($f.Name -ieq 'snipdeck.lock') { continue }
        if (($f.Extension -ieq '.tmp') -or ($f.Extension -ieq '.partial')) { continue }
        $out += [pscustomobject]@{ Path = $f.FullName; Rel = $rel.Replace('\', '/'); Length = $f.Length; Time = $f.LastWriteTime }
    }
    return @($out)
}

function Test-BackupZip([string]$Path) {
    # Reads every entry; refuses unsafe names and a zip without snipdeck-backup.json (not made by Snipdeck). Returns Ok, Count, Error.
    $r = @{ Ok = $false; Count = 0; Error = '' }
    $fs = $null
    $zip = $null
    $hasManifest = $false
    try {
        Add-Type -AssemblyName System.IO.Compression
        $fs = New-Object System.IO.FileStream($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
        $zip = New-Object System.IO.Compression.ZipArchive($fs, [System.IO.Compression.ZipArchiveMode]::Read)
        foreach ($e in $zip.Entries) {
            if ($e.FullName.EndsWith('/')) { continue }
            if (-not (Test-ZipEntryName $e.FullName)) { $r.Error = "unsafe entry name '$($e.FullName)'"; return $r }
            $s = $e.Open()
            try { $s.CopyTo([System.IO.Stream]::Null) } finally { $s.Dispose() }
            if ($e.FullName -eq 'snipdeck-backup.json') { $hasManifest = $true }
            $r.Count++
        }
        if ($r.Count -eq 0) { $r.Error = 'the backup contains no files'; return $r }
        if (-not $hasManifest) { $r.Error = 'snipdeck-backup.json is missing, so this zip was not made by Snipdeck'; return $r }
        $r.Ok = $true
    } catch {
        $r.Error = $_.Exception.Message
    } finally {
        if ($zip) { $zip.Dispose() }
        if ($fs) { $fs.Dispose() }
    }
    return $r
}

function New-DataBackup {
    # Zips the data folder to the backup folder (entries use "/" separators). Returns the zip path or $null.
    param([string]$DataDir = '', [string]$BackupDir = '', [string]$Tag = '', [datetime]$Now = (Get-Date))
    if (-not $DataDir) { $DataDir = Get-DataDir }
    if (-not $BackupDir) { $BackupDir = Get-BackupDir }
    $files = @(Get-BackupFiles $DataDir)
    if ($files.Count -eq 0) {
        Say '  There is no Snipdeck data to back up yet.' 'Gray'
        return $null
    }
    $total = [int64]0
    foreach ($f in $files) { $total += $f.Length }
    $free = Get-FreeBytes $BackupDir
    if ($null -ne $free -and $free -lt ($total + 50MB)) {
        Say ("  Not enough free space for a backup: {0} free, about {1} needed." -f (Format-Bytes $free), (Format-Bytes ($total + 50MB))) 'Red'
        return $null
    }
    $when = $Now
    $final = [System.IO.Path]::Combine($BackupDir, (New-BackupName $when $Tag))
    $guard = 0
    while ((Test-Path -LiteralPath $final) -and $guard -lt 60) {
        $when = $when.AddSeconds(1)
        $final = [System.IO.Path]::Combine($BackupDir, (New-BackupName $when $Tag))
        $guard++
    }
    if ($script:Opt.WhatIf) {
        Say ("  [whatif] would back up {0} files ({1}) to {2}" -f $files.Count, (Format-Bytes $total), $final) 'Yellow'
        return $final
    }
    try { New-Item -ItemType Directory -Path $BackupDir -Force | Out-Null } catch {
        Say "  Could not create the backup folder $BackupDir : $($_.Exception.Message)" 'Red'
        return $null
    }
    $partial = $final + '.partial'
    $fs = $null
    $zip = $null
    $added = 0
    try {
        Add-Type -AssemblyName System.IO.Compression
        $fs = New-Object System.IO.FileStream($partial, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
        $zip = New-Object System.IO.Compression.ZipArchive($fs, [System.IO.Compression.ZipArchiveMode]::Create, $false)
        foreach ($f in $files) {
            $src = $null
            $dst = $null
            try {
                # ReadWrite sharing lets us copy a database file the running app has open.
                $src = New-Object System.IO.FileStream($f.Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
            } catch {
                Write-Log 'WARN' "backup skipped $($f.Rel): $($_.Exception.Message)"
                continue
            }
            try {
                $entry = $zip.CreateEntry($f.Rel, [System.IO.Compression.CompressionLevel]::Optimal)
                try { $entry.LastWriteTime = [System.DateTimeOffset]$f.Time } catch { }
                $dst = $entry.Open()
                $src.CopyTo($dst)
                $added++
            } finally {
                if ($dst) { $dst.Dispose() }
                $src.Dispose()
            }
        }
        $manifest = $zip.CreateEntry('snipdeck-backup.json')
        $ms = $manifest.Open()
        try {
            $sw = New-Object System.IO.StreamWriter($ms, (New-Object System.Text.UTF8Encoding($false)))
            $sw.Write((@{ app = 'snipdeck'; created = $when.ToString('s'); files = $added; bytes = $total; tag = $Tag; launcher = $script:LauncherVersion } | ConvertTo-Json))
            $sw.Flush()
        } finally { $ms.Dispose() }
    } catch {
        Say "  The backup failed: $($_.Exception.Message)" 'Red'
        Write-Log 'ERROR' "backup failed: $($_.Exception.Message)"
        $added = -1
    } finally {
        if ($zip) { $zip.Dispose() }
        if ($fs) { $fs.Dispose() }
    }
    if ($added -lt 0 -or $added -eq 0) {
        try { if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force } } catch { }
        return $null
    }
    $check = Test-BackupZip $partial
    if (-not $check.Ok -or $check.Count -ne ($added + 1)) {
        Say "  The new backup did not pass its own check ($($check.Error)); it was discarded." 'Red'
        try { Remove-Item -LiteralPath $partial -Force } catch { }
        return $null
    }
    Move-Item -LiteralPath $partial -Destination $final -Force
    Say ("  Backup saved: {0} ({1} files, {2})" -f $final, $added, (Format-Bytes (Get-Item -LiteralPath $final).Length)) 'Green'
    Write-Log 'BACKUP' $final
    $removed = @(Invoke-Rotation $BackupDir $script:BackupNameRegex $script:KeepBackups)
    if ($removed.Count -gt 0) { Say ("  Removed {0} older backup(s); the newest {1} of each kind are kept." -f $removed.Count, $script:KeepBackups) 'DarkGray' }
    return $final
}

function Get-BackupList([string]$BackupDir = '') {
    if (-not $BackupDir) { $BackupDir = Get-BackupDir }
    $list = @()
    if (-not (Test-Path -LiteralPath $BackupDir -PathType Container)) { return @() }
    foreach ($f in @(Get-ChildItem -LiteralPath $BackupDir -File -ErrorAction SilentlyContinue)) {
        if ($f.Name -match $script:BackupNameRegex) {
            $list += [pscustomobject]@{ Name = $f.Name; Path = $f.FullName; Stamp = $Matches['stamp']; Size = $f.Length; Time = $f.LastWriteTime }
        }
    }
    return @($list | Sort-Object Stamp -Descending)
}

function Invoke-Backup {
    Say-Step 'Backing up your Snipdeck data'
    if (@(Get-AppProcesses).Count -gt 0) {
        Say '  Snipdeck is running. The copy is taken anyway; for a guaranteed-consistent copy, quit Snipdeck first.' 'Yellow'
    }
    $p = New-DataBackup
    if ($script:Opt.WhatIf) { return 0 }
    if (-not $p) {
        if (@(Get-BackupFiles (Get-DataDir)).Count -eq 0) { return 0 }
        Write-FailureReport 'back up your data' 'the backup could not be written.' @() @('Check that there is free space in your Documents folder.', 'Run "Snipdeck.bat doctor" and send the report if it keeps happening.')
        return 1
    }
    return 0
}

# ---------------------------------------------------------------- restore

function Expand-BackupZip([string]$ZipPath, [string]$Dest) {
    # Extracts into $Dest (which must be empty or new); entry names are checked so nothing lands outside it.
    $count = 0
    $fs = $null
    $zip = $null
    try {
        Add-Type -AssemblyName System.IO.Compression
        New-Item -ItemType Directory -Path $Dest -Force | Out-Null
        $fs = New-Object System.IO.FileStream($ZipPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
        $zip = New-Object System.IO.Compression.ZipArchive($fs, [System.IO.Compression.ZipArchiveMode]::Read)
        foreach ($e in $zip.Entries) {
            if ($e.FullName.EndsWith('/')) { continue }
            if ($e.FullName -eq 'snipdeck-backup.json') { continue }
            if (-not (Test-ZipEntryName $e.FullName)) { throw "unsafe entry name '$($e.FullName)'" }
            $target = [System.IO.Path]::Combine($Dest, $e.FullName.Replace('/', [string][System.IO.Path]::DirectorySeparatorChar))
            if (-not (Test-PathUnder $target $Dest)) { throw "entry '$($e.FullName)' would land outside the target folder" }
            $dir = Split-Path -Parent $target
            if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
            $src = $e.Open()
            $out = $null
            try {
                $out = New-Object System.IO.FileStream($target, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
                $src.CopyTo($out)
            } finally {
                if ($out) { $out.Dispose() }
                $src.Dispose()
            }
            $count++
        }
    } finally {
        if ($zip) { $zip.Dispose() }
        if ($fs) { $fs.Dispose() }
    }
    return $count
}

function Invoke-Restore([string[]]$Pass) {
    Say-Step 'Restoring a backup'
    $running = @(Get-AppProcesses)
    if ($running.Count -gt 0) {
        Write-FailureReport 'restore a backup' 'Snipdeck is running, and restoring under a running program could damage the library.' @() @('Quit Snipdeck from its tray icon (right-click, Quit).', 'Then run Snipdeck.bat restore again.')
        return 1
    }
    $list = @(Get-BackupList)
    $chosen = $null
    if (@($Pass).Count -gt 0 -and $Pass[0]) {
        $arg = $Pass[0]
        $cand = @($arg, [System.IO.Path]::Combine((Get-BackupDir), $arg))
        foreach ($c in $cand) { if (-not $chosen -and (Test-Path -LiteralPath $c -PathType Leaf)) { $chosen = (Get-Item -LiteralPath $c).FullName } }
        if (-not $chosen) {
            Write-FailureReport 'restore a backup' "the backup '$arg' was not found." @() @('Run "Snipdeck.bat restore" with no file name to pick from the list.')
            return 1
        }
    } else {
        if ($list.Count -eq 0) {
            Say ("  There are no backups in {0}." -f (Get-BackupDir)) 'Yellow'
            Say '  Run "Snipdeck.bat backup" to make one.' 'Gray'
            return 1
        }
        Say ("  Backups in {0}:" -f (Get-BackupDir)) 'Gray'
        $i = 1
        foreach ($b in $list) {
            Say ("   {0,2}. {1}   {2}   {3}" -f $i, $b.Name, $b.Time.ToString('yyyy-MM-dd HH:mm'), (Format-Bytes $b.Size)) 'Gray'
            $i++
        }
        if ($script:Opt.WhatIf) {
            Say '  [whatif] would ask which backup to restore (the newest is the default), make a safety backup, then restore.' 'Yellow'
            return 0
        }
        if ($script:Opt.Yes) {
            $chosen = $list[0].Path
            Say "  -Yes: using the newest backup, $($list[0].Name)" 'DarkGray'
        } elseif (-not (Test-Interactive)) {
            Write-FailureReport 'restore a backup' 'no keyboard is attached to choose a backup.' @() @('Name the backup: Snipdeck.bat restore snipdeck-YYYYMMDD-HHMMSS.zip', 'Or add -Yes to use the newest one.')
            return 1
        } else {
            $ans = Read-Typed 'Which one? Enter a number (Enter = 1, q = cancel)'
            if ($ans.Trim().ToLowerInvariant() -eq 'q') { Say '  Cancelled.' 'Gray'; return 0 }
            $n = 1
            if ($ans.Trim() -ne '') {
                if (-not [int]::TryParse($ans.Trim(), [ref]$n) -or $n -lt 1 -or $n -gt $list.Count) {
                    Say '  That is not one of the numbers in the list. Nothing was changed.' 'Yellow'
                    return 1
                }
            }
            $chosen = $list[$n - 1].Path
        }
    }
    Say "  Checking $(Split-Path -Leaf $chosen)..." 'Gray'
    $check = Test-BackupZip $chosen
    if (-not $check.Ok) {
        Write-FailureReport 'restore a backup' "that backup cannot be used: $($check.Error)" @() @('Pick another backup from the list.', 'Nothing was changed.')
        return 1
    }
    Say "  It holds $($check.Count) files." 'Gray'
    if (-not (Ask-YesNo 'Replace your current library and settings with this backup? (They are saved first.)' $true)) {
        Say '  Nothing was changed.' 'Gray'
        return 0
    }
    $data = Get-DataDir
    if (@(Get-BackupFiles $data).Count -gt 0) {
        Say '  Saving your current data first...' 'Gray'
        $safety = New-DataBackup '' '' 'pre-restore'
        if (-not $safety -and -not $script:Opt.WhatIf) {
            Write-FailureReport 'restore a backup' 'the safety backup of your current data could not be made, so nothing was replaced.' @() @('Free some space in your Documents folder and try again.')
            return 1
        }
    }
    $parent = Split-Path -Parent $data
    $ts = (Get-Date).ToString('yyyyMMdd-HHmmss')
    $stage = [System.IO.Path]::Combine($parent, "Snipdeck.restore-$ts")
    $old = [System.IO.Path]::Combine($parent, "Snipdeck.old-$ts")
    Register-OwnedTemp $stage
    Register-OwnedTemp $old
    if ($script:Opt.WhatIf) {
        Say "  [whatif] would unpack the backup to $stage, move your current files to $old, move the backup's files into place, then delete both." 'Yellow'
        return 0
    }
    $moved = @()
    $placed = @()
    try {
        Say '  Unpacking...' 'Gray'
        [void](Expand-BackupZip $chosen $stage)
        New-Item -ItemType Directory -Path $data -Force | Out-Null
        New-Item -ItemType Directory -Path $old -Force | Out-Null
        foreach ($item in @(Get-ChildItem -LiteralPath $data -Force)) {
            if ($item.Name -ieq 'logs') { continue }
            Move-Item -LiteralPath $item.FullName -Destination ([System.IO.Path]::Combine($old, $item.Name)) -Force
            $moved += $item.Name
        }
        foreach ($item in @(Get-ChildItem -LiteralPath $stage -Force)) {
            Move-Item -LiteralPath $item.FullName -Destination ([System.IO.Path]::Combine($data, $item.Name)) -Force
            $placed += $item.Name
        }
    } catch {
        Write-Log 'ERROR' "restore failed: $($_.Exception.Message)"
        Say "  Something went wrong while restoring: $($_.Exception.Message)" 'Red'
        Say '  Putting your previous files back...' 'Yellow'
        foreach ($n in $placed) { try { [void](Remove-KnownPath ([System.IO.Path]::Combine($data, $n))) } catch { } }
        foreach ($n in $moved) {
            try { Move-Item -LiteralPath ([System.IO.Path]::Combine($old, $n)) -Destination ([System.IO.Path]::Combine($data, $n)) -Force } catch { Write-Log 'ERROR' "rollback of $n failed" }
        }
        Write-FailureReport 'restore a backup' 'the files could not be swapped; your previous data was put back.' @('Rolled back to the previous data.') @('Close programs that may be using the Snipdeck data folder and try again.', 'Your safety backup is in the backup folder if anything looks wrong.')
        return 1
    }
    foreach ($p in @($stage, $old)) { try { [void](Remove-KnownPath $p) } catch { Write-Log 'WARN' $_.Exception.Message } }
    Say '  Restored. Start Snipdeck with Snipdeck.bat.' 'Green'
    return 0
}

# ---------------------------------------------------------------- clean and uninstall

function Get-CandidateTargets {
    $t = @([System.IO.Path]::Combine($script:RepoRoot, 'target'), [System.IO.Path]::Combine([System.IO.Path]::GetPathRoot($script:RepoRoot), 'sdtarget'))
    if ($script:TargetOverride) { $t += $script:TargetOverride }
    $seen = @()
    foreach ($x in $t) { $n = Get-NormalPath $x; if ($n -and ($seen -notcontains $n)) { $seen += $n } }
    return @($seen)
}

function Invoke-CleanBuild([bool]$Confirm = $false) {
    # Deletes dist\ and the build folder. Returns $true when nothing is left behind.
    $paths = @()
    foreach ($t in (Get-CandidateTargets)) { if (Test-Path -LiteralPath $t) { $paths += $t } }
    if (Test-Path -LiteralPath (Get-DistDir)) { $paths += (Get-DistDir) }
    if ($paths.Count -eq 0) {
        Say '  There are no build files to delete.' 'Gray'
        return $true
    }
    if ($Confirm) {
        Say '  These will be deleted (your library and settings are not touched; the next start rebuilds):' 'Gray'
        foreach ($p in $paths) { Say ("    {0}   {1}" -f $p, (Format-DirSize $p)) 'Gray' }
        if (-not (Ask-YesNo 'Delete them?' $true)) { Say '  Nothing was deleted.' 'Gray'; return $false }
    }
    if ((Test-Path -LiteralPath (Get-DistDir)) -and @(Get-AppProcesses).Count -gt 0) {
        Say '  A running snipdeck-app.exe may hold files in dist\.' 'Yellow'
        if (Ask-YesNo 'Stop the running snipdeck-app.exe (only that program)?' $true) { [void](Stop-AppProcesses) }
    }
    $allGone = $true
    foreach ($p in $paths) {
        $isTarget = -not (Test-PathEquals $p (Get-DistDir))
        if ($isTarget -and -not (Test-KnownDeletable $p)) { continue }
        $res = 'failed'
        try { $res = Remove-KnownPath $p } catch { Say "  $($_.Exception.Message)" 'Red' }
        if ($res -eq 'failed') {
            $allGone = $false
            Say "  Could not fully delete $p. A program may be using it (cargo, an editor, antivirus). Close it and run clean again." 'Yellow'
        } elseif ($res -eq 'removed') {
            Say "  Deleted $p" 'Green'
        }
    }
    return $allGone
}

function Invoke-Clean {
    Say-Step 'Cleaning build files'
    if (Invoke-CleanBuild $true) { return 0 }
    if ($script:Opt.WhatIf) { return 0 }
    return 1
}

function Remove-ShortcutFile([string]$Path) {
    # Only the exact Start-menu shortcut paths below may be removed here.
    $allowed = @()
    if ($env:APPDATA) {
        $programs = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft', 'Windows', 'Start Menu', 'Programs')
        $allowed += [System.IO.Path]::Combine($programs, 'Snipdeck.lnk')
        $allowed += [System.IO.Path]::Combine($programs, 'Snipdeck', 'Snipdeck.lnk')
    }
    $ok = $false
    foreach ($a in $allowed) { if (Test-PathEquals $Path $a) { $ok = $true } }
    if (-not $ok) { throw "Refusing to delete '$Path': not a known Snipdeck shortcut." }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    if ($script:Opt.WhatIf) { Say "  [whatif] would remove the shortcut $Path" 'Yellow'; return $true }
    Remove-Item -LiteralPath $Path -Force -ErrorAction Stop
    Write-Log 'DELETE' $Path
    Say "  Removed the shortcut $Path" 'Green'
    return $true
}

function Remove-Autostart {
    # Removes only the value named Snipdeck from the current user's Run key (and its StartupApproved flag).
    if (-not $script:OnWindows) { return $false }
    $did = $false
    $run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    $appr = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
    foreach ($key in @($run, $appr)) {
        $has = $false
        try { $has = [bool](Get-ItemProperty -Path $key -Name 'Snipdeck' -ErrorAction SilentlyContinue) } catch { }
        if (-not $has) { continue }
        if ($script:Opt.WhatIf) { Say "  [whatif] would remove the Snipdeck value from $key" 'Yellow'; $did = $true; continue }
        try {
            Remove-ItemProperty -Path $key -Name 'Snipdeck' -ErrorAction Stop
            Write-Log 'DELETE' "$key\Snipdeck"
            Say "  Removed the Snipdeck start-with-Windows entry ($key)" 'Green'
            $did = $true
        } catch {
            Say "  Could not remove the autostart entry in ${key}: $($_.Exception.Message)" 'Yellow'
        }
    }
    return $did
}

function Invoke-Uninstall {
    Say-Step 'Uninstall Snipdeck'
    $data = Get-DataDir
    $targets = @()
    foreach ($t in (Get-CandidateTargets)) { if (Test-Path -LiteralPath $t) { $targets += $t } }
    $hasDist = Test-Path -LiteralPath (Get-DistDir)
    $hasData = Test-Path -LiteralPath $data
    $appTargets = @()
    foreach ($x in @(Get-UninstallTargets $script:RepoRoot $data ([System.IO.Path]::GetTempPath()) @(Get-DownloadLeftovers) $false)) {
        if ((@('app', 'app-staging', 'download') -contains $x.Kind) -and (Test-Path -LiteralPath $x.Path)) { $appTargets += $x }
    }
    Say '  This can remove:' 'Gray'
    foreach ($x in $appTargets) {
        $what = 'the app folder'
        if ($x.Kind -eq 'app-staging') { $what = 'an unfinished app update' } elseif ($x.Kind -eq 'download') { $what = 'a leftover download' }
        Say ("    - {0} {1}:   {2}" -f $what, $x.Path, (Format-DirSize $x.Path)) 'Gray'
    }
    Say ("    - the fast-start copy (dist\):   {0}" -f $(if ($hasDist) { Format-DirSize (Get-DistDir) } else { 'not present' })) 'Gray'
    foreach ($t in $targets) { Say ("    - the build folder {0}:   {1}" -f $t, (Format-DirSize $t)) 'Gray' }
    Say ("    - your library and settings ({0}):   {1}" -f $data, $(if ($hasData) { Format-DirSize $data } else { 'not present' })) 'Gray'
    Say '    - the start-with-Windows entry and a Start-menu shortcut, if there are any' 'Gray'
    Say '  It never touches anything else, and not the Snipdeck program folder itself.' 'Gray'
    $go = $script:Opt.WhatIf
    if (-not $go) { $go = Ask-YesNo 'Continue?' $false }
    if (-not $go) { Say '  Nothing was changed.' 'Gray'; return 0 }

    if ((@(Get-AppProcesses).Count -gt 0) -or (@(Get-PortableProcesses).Count -gt 0)) {
        Say '  Snipdeck is running.' 'Yellow'
        if (-not (Ask-YesNo 'Stop the running Snipdeck (only that program) so it can be removed?' $true)) {
            Say '  Nothing was changed.' 'Gray'
            return 1
        }
        $stopped = $true
        if (-not (Stop-AppProcesses)) { $stopped = $false }
        if (-not (Stop-PortableApp)) { $stopped = $false }
        if (-not $stopped -and -not $script:Opt.WhatIf) {
            Write-FailureReport 'stop Snipdeck' 'Snipdeck would not close.' @() @('Quit Snipdeck from its tray icon, then run the uninstall again.')
            return 1
        }
    }

    if ($hasData -and @(Get-BackupFiles $data).Count -gt 0) {
        if (Ask-YesNo 'Back up your library and settings first?' $true) {
            $b = New-DataBackup '' '' 'pre-uninstall'
            if (-not $b -and -not $script:Opt.WhatIf) {
                Write-FailureReport 'uninstall' 'the backup you asked for could not be made, so nothing was removed.' @() @('Free some space in your Documents folder and try again.', 'Or answer n to the backup question if you really do not want one.')
                return 1
            }
        }
    }

    $removeTarget = $false
    if ($targets.Count -gt 0) {
        $removeTarget = $script:Opt.RemoveTarget
        if (-not $removeTarget) { $removeTarget = Ask-YesNo 'Also delete the build folder? (It is only a cache; a later start rebuilds it.)' $true }
    }

    $removeData = $false
    if ($hasData) {
        if ($script:Opt.Yes) {
            $removeData = $script:Opt.RemoveData
            if ($removeData) { Say '  -Yes --data: your library and settings will be deleted.' 'Yellow' }
        } elseif ($script:Opt.WhatIf) {
            $removeData = $script:Opt.RemoveData
        } else {
            if ($script:Opt.RemoveData -or (Ask-YesNo 'Also DELETE your library and settings? This cannot be undone.' $false)) {
                $typed = Read-Typed 'Type DELETE (capitals) to confirm, anything else keeps your data'
                if ($typed -ceq 'DELETE') { $removeData = $true } else { Say '  Not confirmed: your data is kept.' 'Gray' }
            }
        }
    }

    Say ''
    $failed = $false
    foreach ($x in $appTargets) {
        Register-OwnedTemp $x.Path
        $res = Remove-KnownPath $x.Path
        if ($res -eq 'removed') { Say "  Removed $($x.Path)" 'Green' } elseif ($res -eq 'failed') { $failed = $true; Say "  Could not fully remove $($x.Path) (is Snipdeck or an antivirus scan using it?)" 'Yellow' }
    }
    $res = Remove-KnownPath (Get-DistDir)
    if ($res -eq 'removed') { Say '  Removed dist\' 'Green' } elseif ($res -eq 'failed') { $failed = $true; Say '  Could not fully remove dist\ (is a program using it?)' 'Yellow' }
    if ($removeTarget) {
        foreach ($t in $targets) {
            $res = Remove-KnownPath $t
            if ($res -eq 'removed') { Say "  Removed $t" 'Green' } elseif ($res -eq 'failed') { $failed = $true; Say "  Could not fully remove $t (close cargo, editors, antivirus scans and try again)" 'Yellow' }
        }
    }
    [void](Remove-Autostart)
    if ($env:APPDATA) {
        $programs = [System.IO.Path]::Combine($env:APPDATA, 'Microsoft', 'Windows', 'Start Menu', 'Programs')
        foreach ($lnk in @([System.IO.Path]::Combine($programs, 'Snipdeck.lnk'), [System.IO.Path]::Combine($programs, 'Snipdeck', 'Snipdeck.lnk'))) {
            try { [void](Remove-ShortcutFile $lnk) } catch { Say "  $($_.Exception.Message)" 'Yellow' }
        }
        $folder = [System.IO.Path]::Combine($programs, 'Snipdeck')
        if ((Test-Path -LiteralPath $folder -PathType Container) -and -not $script:Opt.WhatIf) {
            if (@(Get-ChildItem -LiteralPath $folder -Force -ErrorAction SilentlyContinue).Count -eq 0) {
                try { Remove-Item -LiteralPath $folder -ErrorAction Stop; Say "  Removed the empty Start-menu folder $folder" 'Green' } catch { }
            }
        }
    }
    if ($removeData) {
        Write-Log 'INFO' 'removing the data folder as confirmed'
        $res = Remove-KnownPath $data
        if ($res -eq 'removed') { Stop-Log; Say '  Removed your library and settings.' 'Green' } elseif ($res -eq 'failed') { $failed = $true; Say '  Could not fully remove the data folder (is Snipdeck still running?)' 'Yellow' }
    } elseif ($hasData) {
        Say "  Kept your library and settings in $data" 'Gray'
    }
    Say ''
    if ($failed) {
        Say 'Uninstall finished with some items left. Close what is using them and run it again; it is safe to repeat.' 'Yellow'
        return 1
    }
    Say 'Uninstall finished. It is safe to run again.' 'Green'
    Say "The Snipdeck program folder was not touched: $script:RepoRoot (delete it yourself if you want it gone)." 'Gray'
    Say 'To install again, run Snipdeck.bat: it downloads the app again and finds your library if you kept it.' 'Gray'
    return 0
}

# ---------------------------------------------------------------- logs

function Invoke-Logs([bool]$Open) {
    $dir = Get-LogDir
    Say-Step "Snipdeck logs ($dir)"
    if (-not (Test-Path -LiteralPath $dir)) { Say '  No logs yet.' 'Gray'; return 0 }
    $files = @(Get-ChildItem -LiteralPath $dir -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 15)
    foreach ($f in $files) { Say ("  {0}   {1}   {2}" -f $f.Name, $f.LastWriteTime.ToString('yyyy-MM-dd HH:mm'), (Format-Bytes $f.Length)) 'Gray' }
    $latest = $files | Where-Object { $_.Name -like 'launcher-*.log' } | Select-Object -First 1
    if ($latest) {
        Say ''
        Say "  Last lines of $($latest.Name):" 'Cyan'
        $text = ''
        try { $text = [System.IO.File]::ReadAllText($latest.FullName) } catch { }
        foreach ($l in (Get-LastLines $text 30)) { Say "    $l" 'DarkGray' }
    }
    if ($Open -and $script:OnWindows) { try { Start-Process -FilePath 'explorer.exe' -ArgumentList (ConvertTo-ArgString @($dir)) } catch { } }
    return 0
}

# ---------------------------------------------------------------- doctor

function Invoke-Doctor {
    Say-Step 'Snipdeck doctor: checking this PC and Snipdeck itself'
    $checks = New-Object System.Collections.ArrayList
    $add = { param($c) [void]$checks.Add($c); Show-Check $c }

    Say ''
    Say '  This PC' 'Cyan'
    if ($script:OnWindows) {
        $build = [Environment]::OSVersion.Version.Build
        $caption = ''
        try {
            $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
            $caption = $os.Caption
            $build = [int]$os.BuildNumber
        } catch { }
        if ($build -ge 18362) { & $add (New-Check 'Windows' 'pass' "$caption (build $build)") }
        else { & $add (New-Check 'Windows' 'warn' "$caption (build $build) is older than Windows 10 1903" 'Snipdeck needs Windows 10 1903 or newer.') }
    } else {
        & $add (New-Check 'Windows' 'info' 'this is not Windows; Snipdeck is Windows-first')
    }
    & $add (New-Check 'PowerShell' 'pass' ([string]$PSVersionTable.PSVersion))
    & $add (Test-DiskSpace (Get-TargetDirectory))
    $repoNote = ''
    if ($script:RepoRoot -match '(?i)onedrive|dropbox|google drive') {
        & $add (New-Check 'Snipdeck folder' 'warn' 'it sits inside a cloud-sync folder' 'Sync clients lock and upload the multi-GB build folder. Move Snipdeck to a plain folder such as C:\Snipdeck.')
    } elseif ((Get-TargetDirectory) -ne [System.IO.Path]::Combine($script:RepoRoot, 'target')) {
        & $add (New-Check 'Snipdeck folder' 'info' "path is long; the build folder is placed at $(Get-TargetDirectory)")
    } else {
        & $add (New-Check 'Snipdeck folder' 'pass' (Hide-UserName $script:RepoRoot))
    }

    Say ''
    Say '  Build tools' 'Cyan'
    & $add (Test-RustInstalled)
    & $add (Test-RustHost)
    & $add (Test-MsvcInstalled)
    & $add (Test-GitInstalled)
    & $add (Test-NetworkCheck)
    if (Test-RegistryCache) { & $add (New-Check 'Downloaded packages' 'pass' 'cached (offline builds are possible)') }
    else { & $add (New-Check 'Downloaded packages' 'info' 'none cached yet (the first build downloads them)') }
    if (Find-Command 'winget') { & $add (New-Check 'winget' 'pass' 'available (used for automatic installs)') }
    else { & $add (New-Check 'winget' 'info' 'not available (Snipdeck falls back to the official installers)') }

    Say ''
    Say '  Build and cache' 'Cyan'
    $tdir = Get-TargetDirectory
    if (Test-Path -LiteralPath $tdir) { & $add (New-Check 'Build folder' 'pass' ("{0}, {1}" -f (Hide-UserName $tdir), (Format-DirSize $tdir))) }
    else { & $add (New-Check 'Build folder' 'info' 'not created yet') }
    $stamp = Get-SourceStamp
    $state = Get-CacheState $stamp
    switch ($state) {
        'valid' { & $add (New-Check 'Fast-start copy' 'pass' 'up to date; Snipdeck starts without building') }
        'stale' { & $add (New-Check 'Fast-start copy' 'info' 'the source changed since the last build; the next start rebuilds') }
        'corrupt' { & $add (New-Check 'Fast-start copy' 'warn' 'damaged' 'The next start rebuilds it, or run "Snipdeck.bat clean".') }
        default { & $add (New-Check 'Fast-start copy' 'info' 'none yet; the first start builds one') }
    }

    Say ''
    Say '  Your data' 'Cyan'
    $data = Get-DataDir
    if (Test-Path -LiteralPath $data -PathType Container) {
        $probe = [System.IO.Path]::Combine($data, ('write-test-' + [Guid]::NewGuid().ToString('N') + '.tmp'))
        $writable = $false
        try { [System.IO.File]::WriteAllText($probe, 'x'); Remove-Item -LiteralPath $probe -Force; $writable = $true } catch { }
        if ($writable) { & $add (New-Check 'Data folder' 'pass' ("{0}, {1}" -f (Hide-UserName $data), (Format-DirSize $data))) }
        else { & $add (New-Check 'Data folder' 'fail' "$(Hide-UserName $data) is not writable" 'Check the folder permissions or antivirus (controlled folder access).') }
    } else {
        & $add (New-Check 'Data folder' 'info' "$(Hide-UserName $data) does not exist yet (created on first start)")
    }
    $db = [System.IO.Path]::Combine($data, 'snipdeck.db')
    if (Test-Path -LiteralPath $db) { & $add (New-Check 'Library' 'pass' ("snipdeck.db, {0}" -f (Format-Bytes (Get-Item -LiteralPath $db).Length))) }
    else { & $add (New-Check 'Library' 'info' 'no library file yet') }
    $sj = [System.IO.Path]::Combine($data, 'settings.json')
    if (Test-Path -LiteralPath $sj) {
        $txt = ''
        try { $txt = [System.IO.File]::ReadAllText($sj) } catch { }
        $j = Test-JsonText $txt
        if (-not $j.Ok) {
            & $add (New-Check 'settings.json' 'fail' "not valid JSON: $($j.Error)" 'Snipdeck moves a broken file to settings.json.bad and uses defaults. Fix the file, or run "Snipdeck.bat restore".')
        } elseif ($j.Value -isnot [System.Management.Automation.PSCustomObject]) {
            & $add (New-Check 'settings.json' 'warn' 'valid JSON but not a settings object' 'Snipdeck will ignore it and use defaults.')
        } else {
            & $add (New-Check 'settings.json' 'pass' 'valid JSON')
            $dups = @(Get-HotkeyConflicts $j.Value)
            if ($dups.Count -gt 0) { & $add (New-Check 'Hotkey settings' 'warn' ($dups -join '; ') 'The app turns off the later action. Give each action its own chord in settings.json.') }
        }
    } else {
        & $add (New-Check 'settings.json' 'info' 'none yet (defaults are used)')
    }
    $bad = @()
    if (Test-Path -LiteralPath $data -PathType Container) { $bad = @(Get-ChildItem -LiteralPath $data -File -Force -Filter '*.bad' -ErrorAction SilentlyContinue) }
    if ($bad.Count -gt 0) { & $add (New-Check '.bad files' 'warn' (($bad | ForEach-Object { $_.Name }) -join ', ') 'Snipdeck set a damaged file aside. Fix it and rename it back, or run "Snipdeck.bat restore".') }
    else { & $add (New-Check '.bad files' 'pass' 'none') }
    $paks = @()
    if (Test-Path -LiteralPath $data -PathType Container) { $paks = @(Get-ChildItem -LiteralPath $data -File -Force -Filter 'snipdeck.db.pre-v*.bak' -ErrorAction SilentlyContinue) }
    if ($paks.Count -gt 0) { & $add (New-Check 'Upgrade copies' 'info' (($paks | ForEach-Object { $_.Name }) -join ', ') 'These can hold plaintext of items made sensitive later. Delete them once the new version runs well.') }
    $blist = @(Get-BackupList)
    if ($blist.Count -gt 0) { & $add (New-Check 'Backups' 'pass' ("{0} in {1}; newest {2}" -f $blist.Count, (Hide-UserName (Get-BackupDir)), $blist[0].Time.ToString('yyyy-MM-dd HH:mm'))) }
    elseif (Test-Path -LiteralPath $db) { & $add (New-Check 'Backups' 'warn' 'none yet' 'Run "Snipdeck.bat backup".') }
    else { & $add (New-Check 'Backups' 'info' 'none yet') }

    Say ''
    Say '  Running now' 'Cyan'
    $apps = @(Get-AppProcesses)
    if ($apps.Count -gt 0) { & $add (New-Check 'Snipdeck' 'info' ("running ({0} process)" -f $apps.Count)) } else { & $add (New-Check 'Snipdeck' 'info' 'not running') }
    $grabbers = @(Get-Process -Name 'PowerToys*', 'AutoHotkey*', 'Ditto', 'CopyQ', 'ClipClip', 'ClipboardFusion' -ErrorAction SilentlyContinue | ForEach-Object { $_.ProcessName } | Sort-Object -Unique)
    $hkDetail = 'Windows only tells the program that registers a chord whether it was free, so a conflict cannot be read from outside.'
    if ($grabbers.Count -gt 0) { $hkDetail = 'running now and known to use global hotkeys: ' + ($grabbers -join ', ') + '. If a Snipdeck hotkey does nothing, change its chord in settings.json.' }
    & $add (New-Check 'Hotkey conflicts' 'info' $hkDetail)
    if ($script:OnWindows) {
        $auto = $null
        try { $auto = (Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'Snipdeck' -ErrorAction SilentlyContinue).Snipdeck } catch { }
        if ($auto) { & $add (New-Check 'Start with Windows' 'info' 'on (Run key entry named Snipdeck)') } else { & $add (New-Check 'Start with Windows' 'info' 'off') }
    }

    $all = @($checks)
    $overall = Get-OverallStatus $all
    $fails = @($all | Where-Object { $_.Status -eq 'fail' }).Count
    $warns = @($all | Where-Object { $_.Status -eq 'warn' }).Count
    Say ''
    $sumColor = 'Green'
    if ($overall -eq 'warn') { $sumColor = 'Yellow' }
    if ($overall -eq 'fail') { $sumColor = 'Red' }
    Say ("Result: {0} failed, {1} to look at, {2} checks." -f $fails, $warns, $all.Count) $sumColor

    $report = New-Object System.Collections.ArrayList
    [void]$report.Add('Snipdeck doctor report')
    [void]$report.Add('Created: ' + (Get-Date).ToString('yyyy-MM-dd HH:mm:ss'))
    [void]$report.Add('Launcher: v' + $script:LauncherVersion + '   PowerShell: ' + $PSVersionTable.PSVersion)
    [void]$report.Add('Source: ' + $stamp.Method + ' ' + $stamp.Commit)
    [void]$report.Add('Result: ' + $overall)
    [void]$report.Add('')
    foreach ($c in $all) {
        [void]$report.Add((Format-CheckLine $c))
        if ($c.Fix -and $c.Status -ne 'pass') { [void]$report.Add('        -> ' + $c.Fix) }
    }
    $logText = ''
    if ($script:LogPath -and (Test-Path -LiteralPath $script:LogPath)) { try { $logText = [System.IO.File]::ReadAllText($script:LogPath) } catch { } }
    [void]$report.Add('')
    [void]$report.Add('---- end of today''s launcher log ----')
    foreach ($l in (Get-LastLines $logText 40)) { [void]$report.Add($l) }
    $text = Hide-UserName ($report -join "`r`n")
    if ($script:Opt.WhatIf) {
        Say '  [whatif] would write the report file.' 'Yellow'
    } else {
        $dir = Get-LogDir
        try { New-Item -ItemType Directory -Path $dir -Force | Out-Null } catch { $dir = [System.IO.Path]::GetTempPath() }
        $rp = [System.IO.Path]::Combine($dir, 'doctor-' + (Get-Date).ToString('yyyyMMdd-HHmmss') + '.txt')
        try {
            [System.IO.File]::WriteAllText($rp, $text + "`r`n", (New-Object System.Text.UTF8Encoding($false)))
            Say "Report written (your user name is hidden): $rp" 'Cyan'
            Say 'Send that file if you need help.' 'Gray'
            if ($script:OnWindows -and (Test-Interactive) -and -not $script:Opt.Yes) {
                if (Ask-YesNo 'Show the report in Explorer?' $true) {
                    try { Start-Process -FilePath 'explorer.exe' -ArgumentList ('/select,"' + $rp + '"') } catch { }
                }
            }
        } catch {
            Say "Could not write the report file: $($_.Exception.Message)" 'Yellow'
        }
    }
    if ($fails -gt 0) { return 1 }
    return 0
}
