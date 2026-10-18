# lib.ps1 - pure helpers for snipdeck.ps1 (dot-sourced; no side effects when loaded).
# Everything here must run on Windows PowerShell 5.1 and on PowerShell 7 (Linux too, for `selftest`).
# ASCII only: Windows PowerShell 5.1 reads a BOM-less file in the ANSI code page.

$script:OnWindows = ($env:OS -eq 'Windows_NT')

# ---------------------------------------------------------------- small utilities

function Get-StringSha256([string]$Text) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Text))
        return (($bytes | ForEach-Object { $_.ToString('x2') }) -join '')
    } finally { $sha.Dispose() }
}

function Get-FileSha256([string]$Path) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $stream = $null
    try {
        $stream = New-Object System.IO.FileStream($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        $bytes = $sha.ComputeHash($stream)
        return (($bytes | ForEach-Object { $_.ToString('x2') }) -join '')
    } finally {
        if ($stream) { $stream.Dispose() }
        $sha.Dispose()
    }
}

function Format-Bytes([double]$Bytes) {
    if ($Bytes -ge 1GB) { return ('{0:N1} GB' -f ($Bytes / 1GB)) }
    if ($Bytes -ge 1MB) { return ('{0:N0} MB' -f ($Bytes / 1MB)) }
    if ($Bytes -ge 1KB) { return ('{0:N0} KB' -f ($Bytes / 1KB)) }
    return ('{0:N0} bytes' -f $Bytes)
}

function ConvertTo-ArgString([string[]]$Items) {
    # One command-line string for Start-Process (Windows PowerShell 5.1 does not quote array items itself).
    $parts = @()
    foreach ($item in $Items) {
        if ($null -eq $item) { continue }
        if ($item -eq '') { $parts += '""'; continue }
        if ($item -match '[\s"]') {
            $escaped = $item -replace '(\\*)"', '$1$1\"'
            $escaped = $escaped -replace '(\\+)$', '$1$1'
            $parts += ('"' + $escaped + '"')
        } else {
            $parts += $item
        }
    }
    return ($parts -join ' ')
}

function Hide-UserName([string]$Text) {
    # Replaces the profile folder with %USERPROFILE% so a report can be sent without the user name.
    if ([string]::IsNullOrEmpty($Text)) { return $Text }
    $out = $Text
    foreach ($p in @($env:USERPROFILE, $env:HOME)) {
        if (-not [string]::IsNullOrEmpty($p) -and $p.Length -gt 3) {
            $out = $out.Replace($p, '%USERPROFILE%')
        }
    }
    return $out
}

# ---------------------------------------------------------------- path guards

function Get-NormalPath([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) { return $null }
    if ($Path.IndexOf([char]0) -ge 0) { return $null }
    try { $full = [System.IO.Path]::GetFullPath($Path) } catch { return $null }
    $root = [System.IO.Path]::GetPathRoot($full)
    if ($full.Length -gt $root.Length) { $full = $full.TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar) }
    return $full
}

function Test-PathUnder([string]$Path, [string]$Root, [switch]$AllowEqual) {
    # True only when $Path is inside $Root after normalisation (so `..` and sibling prefixes do not pass).
    # A Root that is a drive root or too shallow is refused outright.
    $p = Get-NormalPath $Path
    $r = Get-NormalPath $Root
    if (-not $p -or -not $r) { return $false }
    $rootOfR = [System.IO.Path]::GetPathRoot($r)
    if ($r.Length -le $rootOfR.Length) { return $false }
    $cmp = [System.StringComparison]::Ordinal
    if ($script:OnWindows) { $cmp = [System.StringComparison]::OrdinalIgnoreCase }
    if ($p.Equals($r, $cmp)) { return [bool]$AllowEqual }
    $sep = [string][System.IO.Path]::DirectorySeparatorChar
    return $p.StartsWith($r + $sep, $cmp)
}

function Test-PathEquals([string]$A, [string]$B) {
    $x = Get-NormalPath $A
    $y = Get-NormalPath $B
    if (-not $x -or -not $y) { return $false }
    $cmp = [System.StringComparison]::Ordinal
    if ($script:OnWindows) { $cmp = [System.StringComparison]::OrdinalIgnoreCase }
    return $x.Equals($y, $cmp)
}

function Test-ReparsePoint([string]$Path) {
    try {
        $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
        return (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)
    } catch { return $false }
}

function Test-ZipEntryName([string]$Name) {
    # Guards restore against entries that would land outside the data folder ("zip slip").
    if ([string]::IsNullOrEmpty($Name)) { return $false }
    if ($Name.IndexOf([char]0) -ge 0) { return $false }
    if ($Name.StartsWith('/') -or $Name.StartsWith('\')) { return $false }
    if ($Name -match '^[A-Za-z]:') { return $false }
    foreach ($seg in ($Name -split '[\\/]')) { if ($seg -eq '..') { return $false } }
    return $true
}

function Get-TargetDir([string]$RepoRoot, [string]$ShortRoot = '') {
    # `target` inside the repo, unless that path is long (Windows path limits bite deep in target\debug\build).
    $default = [System.IO.Path]::Combine($RepoRoot, 'target')
    if ($default.Length -le 100) { return $default }
    if ([string]::IsNullOrEmpty($ShortRoot)) { $ShortRoot = [System.IO.Path]::GetPathRoot($RepoRoot) }
    return [System.IO.Path]::Combine($ShortRoot, 'sdtarget')
}

function Test-LooksLikeCargoTarget([string]$Path) {
    # Extra guard before deleting a build folder: cargo leaves these markers in its target root.
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { return $false }
    foreach ($m in @('CACHEDIR.TAG', '.rustc_info.json', 'debug', 'release')) {
        if (Test-Path -LiteralPath ([System.IO.Path]::Combine($Path, $m))) { return $true }
    }
    return $false
}

# ---------------------------------------------------------------- build stamp

function New-StampHash([string]$Head, [string]$Lock, [string]$Diff, [string]$Untracked, [string]$Toolchain, [string]$Mode = 'debug', [string]$Version = '1') {
    $text = "snipdeck-stamp|v=$Version|mode=$Mode|head=$Head|lock=$Lock|diff=$Diff|untracked=$Untracked|toolchain=$Toolchain"
    return (Get-StringSha256 $text)
}

function Test-StampMatch($Stamp, [string]$Hash) {
    if ($null -eq $Stamp) { return $false }
    try { return (($Stamp.hash -is [string]) -and ($Stamp.hash.Length -eq 64) -and ($Stamp.hash -eq $Hash)) } catch { return $false }
}

# ---------------------------------------------------------------- backups and rotation

$script:BackupNameRegex = '^snipdeck-(?<stamp>\d{8}-\d{6})(?:-(?<tag>[a-z][a-z-]*))?\.zip$'

function New-BackupName([datetime]$When, [string]$Tag = '') {
    $name = 'snipdeck-' + $When.ToString('yyyyMMdd-HHmmss')
    if ($Tag) { $name += '-' + $Tag }
    return ($name + '.zip')
}

function Select-ToRotate([string[]]$Names, [string]$Regex, [int]$Keep = 10) {
    # Names that match $Regex are grouped by their optional 'tag' group; the newest $Keep of each group
    # (by the 'stamp' group, else by name) stay, the rest are returned for deletion. Others are ignored.
    $groups = @{}
    foreach ($n in @($Names)) {
        if ($n -match $Regex) {
            $tag = ''
            if ($Matches.ContainsKey('tag')) { $tag = [string]$Matches['tag'] }
            $stamp = $n
            if ($Matches.ContainsKey('stamp')) { $stamp = [string]$Matches['stamp'] }
            if (-not $groups.ContainsKey($tag)) { $groups[$tag] = @() }
            $groups[$tag] += , @($stamp, $n)
        }
    }
    $doomed = @()
    foreach ($tag in $groups.Keys) {
        $sorted = @($groups[$tag] | Sort-Object { $_[0] } -Descending)
        if ($sorted.Count -gt $Keep) {
            for ($i = $Keep; $i -lt $sorted.Count; $i++) { $doomed += $sorted[$i][1] }
        }
    }
    return @($doomed)
}

function Invoke-Rotation([string]$Dir, [string]$Regex, [int]$Keep = 10, [switch]$WhatIf) {
    # Deletes old files in exactly $Dir (never below it). Returns the names removed (or that would be).
    if (-not (Test-Path -LiteralPath $Dir -PathType Container)) { return @() }
    $names = @(Get-ChildItem -LiteralPath $Dir -File -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
    $doomed = @(Select-ToRotate $names $Regex $Keep)
    $done = @()
    foreach ($n in $doomed) {
        $full = [System.IO.Path]::Combine($Dir, $n)
        if (-not (Test-PathUnder $full $Dir)) { continue }
        if (-not $WhatIf) {
            try { Remove-Item -LiteralPath $full -Force -ErrorAction Stop } catch { continue }
        }
        $done += $n
    }
    return @($done)
}

# ---------------------------------------------------------------- settings.json checks

function Test-JsonText([string]$Text) {
    # Returns @{ Ok; Error; Value }. Empty text is not valid settings.
    if ([string]::IsNullOrWhiteSpace($Text)) { return @{ Ok = $false; Error = 'the file is empty'; Value = $null } }
    try {
        $v = $Text | ConvertFrom-Json -ErrorAction Stop
        return @{ Ok = $true; Error = ''; Value = $v }
    } catch {
        return @{ Ok = $false; Error = $_.Exception.Message; Value = $null }
    }
}

function ConvertTo-ChordKey([string]$Chord) {
    # "Ctrl+Alt+Space" and "alt + control + space" are the same chord.
    if ([string]::IsNullOrWhiteSpace($Chord)) { return '' }
    $mods = @()
    $key = ''
    foreach ($part in ($Chord -split '\+')) {
        $t = $part.Trim().ToLowerInvariant()
        if ($t -eq '') { continue }
        switch ($t) {
            'control' { $mods += 'ctrl' }
            'ctrl' { $mods += 'ctrl' }
            'alt' { $mods += 'alt' }
            'shift' { $mods += 'shift' }
            'win' { $mods += 'win' }
            'super' { $mods += 'win' }
            'meta' { $mods += 'win' }
            default { $key = $t }
        }
    }
    return ((($mods | Sort-Object -Unique) -join '+') + '+' + $key)
}

function Get-HotkeyConflicts($Settings) {
    # Pairs of actions in settings.json that share a chord (the app turns the later one off).
    $found = @()
    if ($null -eq $Settings) { return @() }
    $hk = $null
    try { $hk = $Settings.hotkeys } catch { }
    if ($null -eq $hk) { return @() }
    $byKey = @{}
    foreach ($prop in $hk.PSObject.Properties) {
        $key = ConvertTo-ChordKey ([string]$prop.Value)
        if ($key -eq '') { continue }
        if (-not $byKey.ContainsKey($key)) { $byKey[$key] = @() }
        $byKey[$key] += $prop.Name
    }
    foreach ($k in $byKey.Keys) {
        if ($byKey[$k].Count -gt 1) { $found += ("{0} share {1}" -f ($byKey[$k] -join ' and '), $k) }
    }
    return @($found)
}

# ---------------------------------------------------------------- checklist results

function New-Check([string]$Name, [string]$Status, [string]$Detail = '', [string]$Fix = '') {
    # Status: pass | warn | fail | info
    return [pscustomobject]@{ Name = $Name; Status = $Status; Detail = $Detail; Fix = $Fix }
}

function Get-OverallStatus($Checks) {
    $items = @($Checks)
    if (@($items | Where-Object { $_.Status -eq 'fail' }).Count -gt 0) { return 'fail' }
    if (@($items | Where-Object { $_.Status -eq 'warn' }).Count -gt 0) { return 'warn' }
    return 'pass'
}

function Format-CheckLine($Check) {
    $tag = switch ($Check.Status) {
        'pass' { '[ ok ]' }
        'warn' { '[warn]' }
        'fail' { '[FAIL]' }
        default { '[info]' }
    }
    $line = "$tag $($Check.Name)"
    if ($Check.Detail) { $line += ": $($Check.Detail)" }
    return $line
}

# ---------------------------------------------------------------- known cargo failures

function Get-FailureTable {
    # Ordered: earlier entries are considered first. Remedies are tried in order, each at most once per run.
    # Pattern is a .NET regex matched against the whole cargo output.
    return @(
        @{ Id = 'exe-locked'; Title = 'A running snipdeck-app.exe is locking the output file'
            Pattern = '(?i)(LNK1104[^\r\n]*snipdeck[-_]app[^\r\n]*\.exe|snipdeck[-_]app(-[0-9a-f]+)?\.exe[\s\S]{0,600}?(access is denied|being used by another process|os error 32|os error 5))'
            Remedies = @('stop-app') },
        @{ Id = 'no-space'; Title = 'The disk is full'
            Pattern = '(?i)(no space left on device|os error 112|not enough space on the disk|there is not enough space|disk is full|ENOSPC)'
            Remedies = @('offer-clean') },
        @{ Id = 'file-lock'; Title = 'Another build is using the same folder'
            Pattern = '(?i)blocking waiting for file lock'
            Remedies = @('wait-lock') },
        @{ Id = 'msvc-missing'; Title = 'The Microsoft C++ build tools were not found'
            Pattern = '(?i)(linker\s+\W?link\.exe\W?\s+not found|link\.exe[^\r\n]{0,12}not found|failed to find tool\W{1,3}(cl|link)\.exe|LNK1181|LNK1104[^\r\n]*\.lib|cannot open input file\W+(kernel32|msvcrt|libcmt|ucrt)|link: extra operand|vcvarsall)'
            Remedies = @('install-msvc') },
        @{ Id = 'no-toolchain'; Title = 'The Rust toolchain named by rust-toolchain.toml is not installed yet'
            Pattern = '(?i)(toolchain .{1,60} is not installed|no override and no default toolchain|rustup could not choose a version)'
            Remedies = @('install-toolchain') },
        @{ Id = 'toolchain-old'; Title = 'The Rust toolchain is too old for this project'
            Pattern = '(?i)(requires rustc \d+\.\d+|rustc \d+\.\d+(\.\d+)? is not supported by the following package|feature\W+edition2024\W+is required)'
            Remedies = @('update-rust') },
        @{ Id = 'lock-mismatch'; Title = 'Cargo.lock does not match the Cargo.toml files'
            Pattern = '(?i)(lock file[^\r\n]*needs to be updated but --locked was passed|failed to parse lock file)'
            Remedies = @('unlocked-retry') },
        @{ Id = 'long-path'; Title = 'A file path in the build folder is too long for Windows'
            Pattern = '(?i)(os error 206|file name is too long|filename or extension is too long|path too long)'
            Remedies = @('short-target') },
        @{ Id = 'net-revoke'; Title = 'Windows could not check a certificate revocation list (proxy or firewall)'
            Pattern = '(?i)(CRYPT_E_NO_REVOCATION_CHECK|revocation function was unable to check)'
            Remedies = @('no-revoke-check') },
        @{ Id = 'network'; Title = 'Cargo could not reach the crates.io package server'
            Pattern = '(?i)(spurious network error|could not resolve host|couldn.t resolve host|failed to download from|unable to update registry|network failure seems to have happened|failed to fetch into|ssl connect error|connection (was )?(reset|timed out|refused)|operation timed out|couldn.t connect to server|download of config\.json failed|failed to get \W[^\r\n]{1,80} as a dependency of package)'
            Remedies = @('retry-5', 'retry-20', 'http-no-multiplex', 'offline') },
        @{ Id = 'out-of-memory'; Title = 'The build ran out of memory'
            Pattern = '(?i)(memory allocation of \d+ bytes failed|out of memory|os error 1455|paging file is too small)'
            Remedies = @('fewer-jobs') },
        @{ Id = 'corrupt-build'; Title = 'The build folder looks damaged (partly written files)'
            Pattern = '(?i)(internal compiler error|error\[E0786\]|found invalid metadata files|could not parse/generate dep info|file too small to be an archive|is not a valid (rlib|object|archive)|invalid (rlib|archive)|incremental compilation:[^\r\n]*(fail|corrupt)|failed to (write|read|load) [^\r\n]{0,200}(incremental|\.rlib|\.rmeta)|unexpected end of file|object file [^\r\n]{0,200} is truncated|rustc-LLVM ERROR)'
            Remedies = @('clean-pkg', 'clean-all') },
        @{ Id = 'access-denied'; Title = 'Windows refused access to a build file (often antivirus scanning it)'
            Pattern = '(?i)(access is denied\.? \(os error 5\)|os error 5\))'
            Remedies = @('retry-5') }
    )
}

function Get-FailureMatches([string]$Text) {
    # Ids of every known failure that appears in $Text, in table order.
    $ids = @()
    if ([string]::IsNullOrEmpty($Text)) { return @() }
    foreach ($row in (Get-FailureTable)) {
        if ($Text -match $row.Pattern) { $ids += $row.Id }
    }
    return @($ids)
}

function Get-NextRemedy([string]$Text, [hashtable]$Tried) {
    # The first remedy (in table order) of any matched failure that has not been tried yet, or $null.
    if ($null -eq $Tried) { $Tried = @{} }
    if ([string]::IsNullOrEmpty($Text)) { return $null }
    foreach ($row in (Get-FailureTable)) {
        if ($Text -match $row.Pattern) {
            foreach ($r in $row.Remedies) {
                if (-not $Tried.ContainsKey($r)) {
                    return [pscustomobject]@{ FailureId = $row.Id; Title = $row.Title; Remedy = $r }
                }
            }
        }
    }
    return $null
}

function Get-FailureTitle([string]$Id) {
    foreach ($row in (Get-FailureTable)) { if ($row.Id -eq $Id) { return $row.Title } }
    return $Id
}

function Get-FailedCrates([string]$Text) {
    # Crate names cargo says it could not compile (for `cargo clean -p <crate>`). Only safe names are returned.
    $names = @()
    if ([string]::IsNullOrEmpty($Text)) { return @() }
    foreach ($rx in @('could not compile `([A-Za-z0-9_-]+)`', 'failed to run custom build command for `([A-Za-z0-9_-]+)[ `]')) {
        foreach ($m in [regex]::Matches($Text, $rx)) {
            $n = $m.Groups[1].Value
            if ($n -match '^[A-Za-z0-9_-]{1,64}$' -and ($names -notcontains $n)) { $names += $n }
        }
    }
    return @($names)
}

function Get-LastLines([string]$Text, [int]$Count = 25) {
    if ([string]::IsNullOrEmpty($Text)) { return @() }
    $lines = @($Text -split "\r?\n" | Where-Object { $_.Trim() -ne '' })
    if ($lines.Count -le $Count) { return $lines }
    return @($lines[($lines.Count - $Count)..($lines.Count - 1)])
}
