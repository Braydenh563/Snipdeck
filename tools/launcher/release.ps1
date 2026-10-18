# release.ps1 - run / update from the prebuilt portable zip on the GitHub Release (no Rust, Visual Studio or git needed).
# Dot-sourced by snipdeck.ps1 after lib.ps1 and core.ps1. Windows PowerShell 5.1 compatible; ASCII only.
# Uses only built-ins: Invoke-RestMethod, Invoke-WebRequest, Expand-Archive and the SHA-256 helper in lib.ps1.
# The pure parts (asset resolver, pin-file parser, SHA-256 compare) are covered by "snipdeck.ps1 selftest" with no network.

$script:ReleaseRepo = 'Braydenh563/Snipdeck'
$script:PortableExeName = 'snipdeck.exe'
# Shown in the "needs the internet once" message when the real size is not known yet (the API call failed).
# Not measured: no release has been built yet. Update it from the first real release zip.
$script:ApproxDownloadMb = 25

# ---------------------------------------------------------------- locations

function Get-ReleaseApiUrl { return ('https://api.github.com/repos/{0}/releases/latest' -f $script:ReleaseRepo) }
function Get-AppDir { return [System.IO.Path]::Combine($script:RepoRoot, 'app') }
function Get-PortableExe { return [System.IO.Path]::Combine((Get-AppDir), $script:PortableExeName) }
function Get-PinFilePath { return [System.IO.Path]::Combine($script:LauncherDir, 'release-sha256.txt') }

# ---------------------------------------------------------------- pure helpers (selftest covers these)

function Get-ReleaseAsset($Release) {
    # From the GitHub "latest release" JSON: the portable zip and its .sha256 sidecar. Returns $null when either is missing.
    if ($null -eq $Release) { return $null }
    $assets = @()
    if ($Release.PSObject.Properties['assets'] -and $null -ne $Release.assets) { $assets = @($Release.assets) }
    $zip = $null
    foreach ($a in $assets) {
        if ($null -eq $a -or -not $a.name) { continue }
        if ([string]$a.name -match '^snipdeck-[A-Za-z0-9._-]+-windows-x64\.zip$') { $zip = $a; break }
    }
    if ($null -eq $zip) { return $null }
    $shaName = [string]$zip.name + '.sha256'
    $sha = $null
    foreach ($a in $assets) {
        if ($null -ne $a -and [string]$a.name -eq $shaName) { $sha = $a; break }
    }
    if ($null -eq $sha) { return $null }
    $zipUrl = [string]$zip.browser_download_url
    $shaUrl = [string]$sha.browser_download_url
    if (-not $zipUrl.StartsWith('https://') -or -not $shaUrl.StartsWith('https://')) { return $null }
    $tag = ''
    if ($Release.PSObject.Properties['tag_name']) { $tag = [string]$Release.tag_name }
    $size = [int64]0
    if ($zip.PSObject.Properties['size'] -and $zip.size) { $size = [int64]$zip.size }
    return [pscustomobject]@{ Tag = $tag; ZipName = [string]$zip.name; ZipUrl = $zipUrl; ShaName = $shaName; ShaUrl = $shaUrl; Size = $size }
}

function Read-PinLine([string]$Line) {
    # Kind is 'skip' (blank or comment), 'ok' (<sha256>  <asset-name>) or 'bad'.
    $t = $Line.Trim()
    if ($t -eq '' -or $t.StartsWith('#')) { return [pscustomobject]@{ Kind = 'skip'; Hash = ''; Name = '' } }
    if ($t -match '^([0-9A-Fa-f]{64})\s+\*?(\S+)$') {
        return [pscustomobject]@{ Kind = 'ok'; Hash = $Matches[1].ToLowerInvariant(); Name = $Matches[2] }
    }
    return [pscustomobject]@{ Kind = 'bad'; Hash = ''; Name = '' }
}

function Read-PinFile([string]$Text) {
    # Text of release-sha256.txt -> hashtable asset-name -> lowercase sha256. Bad lines are ignored here (see Get-PinFileProblems).
    $pins = @{}
    if ([string]::IsNullOrEmpty($Text)) { return $pins }
    foreach ($line in ($Text -split "\r?\n")) {
        $p = Read-PinLine $line
        if ($p.Kind -eq 'ok') { $pins[$p.Name] = $p.Hash }
    }
    return $pins
}

function Get-PinFileProblems([string]$Text) {
    $problems = @()
    if ([string]::IsNullOrEmpty($Text)) { return $problems }
    $seen = @{}
    $n = 0
    foreach ($line in ($Text -split "\r?\n")) {
        $n++
        $p = Read-PinLine $line
        if ($p.Kind -eq 'bad') { $problems += "line ${n}: expected '<sha256>  <asset-name>'" }
        elseif ($p.Kind -eq 'ok') {
            if ($seen.ContainsKey($p.Name)) { $problems += "line ${n}: more than one entry for $($p.Name)" }
            $seen[$p.Name] = $true
        }
    }
    return $problems
}

function Get-PinnedHash($Pins, [string]$Name) {
    # '' when there is no pin for this asset.
    if ($null -eq $Pins -or [string]::IsNullOrEmpty($Name)) { return '' }
    if ($Pins.ContainsKey($Name)) { return [string]$Pins[$Name] }
    return ''
}

function Read-ShaSidecar([string]$Text, [string]$Name) {
    # The release's .sha256 file: "<hash>  <name>" (the name may carry a leading *). '' when no valid hash for this file.
    if ([string]::IsNullOrEmpty($Text)) { return '' }
    foreach ($line in ($Text -split "\r?\n")) {
        $t = $line.Trim()
        if ($t -match '^([0-9A-Fa-f]{64})(\s+\*?(.+))?$') {
            $file = ''
            if ($Matches[3]) { $file = $Matches[3].Trim() }
            if ($file -eq '' -or $file -eq $Name) { return $Matches[1].ToLowerInvariant() }
        }
    }
    return ''
}

function Test-FileSha256([string]$Path, [string]$Expected) {
    if ($Expected -notmatch '^[0-9A-Fa-f]{64}$') { return $false }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    return ((Get-FileSha256 $Path) -eq $Expected.ToLowerInvariant())
}

function Get-HttpStatus($ErrorRecord) {
    # The HTTP status code behind a failed web call, or 0 when the server was never reached (no network, DNS, TLS, proxy).
    try {
        if ($null -eq $ErrorRecord) { return 0 }
        $resp = $ErrorRecord.Exception.Response
        if ($null -eq $resp) { return 0 }
        return [int]$resp.StatusCode
    } catch { return 0 }
}

# ---------------------------------------------------------------- network

function Initialize-Tls {
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 } catch { }
}

function Get-LatestRelease {
    Initialize-Tls
    try {
        $rel = Invoke-RestMethod -Uri (Get-ReleaseApiUrl) -UseBasicParsing -UserAgent 'Snipdeck-launcher' -Headers @{ Accept = 'application/vnd.github+json' } -TimeoutSec 30 -ErrorAction Stop
        return [pscustomobject]@{ Ok = $true; Release = $rel; Status = 200; Error = '' }
    } catch {
        Write-Log 'WARN' ('release lookup failed: ' + $_.Exception.Message)
        return [pscustomobject]@{ Ok = $false; Release = $null; Status = (Get-HttpStatus $_); Error = $_.Exception.Message }
    }
}

function Save-ReleaseFile([string]$Url, [string]$Dest) {
    Initialize-Tls
    try {
        Write-Log 'INFO' "downloading $Url"
        Invoke-WebRequest -Uri $Url -OutFile $Dest -UseBasicParsing -UserAgent 'Snipdeck-launcher' -TimeoutSec 1800 -ErrorAction Stop
        return [pscustomobject]@{ Ok = $true; Status = 200; Error = '' }
    } catch {
        Write-Log 'WARN' ("download failed: $Url : " + $_.Exception.Message)
        try { if (Test-Path -LiteralPath $Dest) { Remove-Item -LiteralPath $Dest -Force -ErrorAction SilentlyContinue } } catch { }
        return [pscustomobject]@{ Ok = $false; Status = (Get-HttpStatus $_); Error = $_.Exception.Message }
    }
}

function Format-ApproxSize([int64]$Bytes) {
    if ($Bytes -gt 0) { return ('about ' + (Format-Bytes $Bytes)) }
    return ('about {0} MB' -f $script:ApproxDownloadMb)
}

function Write-ReleaseNetworkFailure($Result, [string]$What, [int64]$Bytes) {
    # One place for the "why could the release not be fetched" messages.
    $detail = @()
    if ($Result.Error) { $detail = @(Hide-UserName $Result.Error) }
    $status = [int]$Result.Status
    if ($status -eq 404) {
        if ($What -eq 'find the latest release') {
            Write-FailureReport $What 'there is no published release of Snipdeck yet (GitHub answered 404, not found).' @() @('Wait for the first release to be published, then run Snipdeck.bat again.', 'If this folder is a source checkout (it has Cargo.toml), Snipdeck.bat builds from source by itself; developers can also run: Snipdeck.bat build') $detail
        } else {
            Write-FailureReport $What 'the release lists the file, but GitHub could not find it (404, not found).' @() @('Run Snipdeck.bat again in a few minutes.', 'If it keeps happening, report it at https://github.com/Braydenh563/Snipdeck/issues') $detail
        }
        return
    }
    if ($status -eq 403 -or $status -eq 429) {
        Write-FailureReport $What ("GitHub is limiting requests from this network for now (HTTP $status).") @() @('Wait about an hour and run Snipdeck.bat again, or try another network.') $detail
        return
    }
    if ($status -gt 0) {
        Write-FailureReport $What ("GitHub answered with HTTP $status.") @() @('Run Snipdeck.bat again in a few minutes.', 'If it keeps happening, run "Snipdeck.bat doctor" and send the report with the log file named below.') $detail
        return
    }
    Write-FailureReport $What ('Snipdeck needs the internet once, to download the app (' + (Format-ApproxSize $Bytes) + ').') @() @('Connect and run Snipdeck.bat again.', 'On a VPN, proxy or strict firewall, allow github.com and objects.githubusercontent.com.') $detail
}

# ---------------------------------------------------------------- download, verify, unpack

function Test-ZipEntries([string]$ZipPath) {
    # '' when every entry stays inside the target folder; otherwise the first unsafe entry name.
    $fs = $null
    $zip = $null
    try {
        Add-Type -AssemblyName System.IO.Compression
        $fs = New-Object System.IO.FileStream($ZipPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
        $zip = New-Object System.IO.Compression.ZipArchive($fs, [System.IO.Compression.ZipArchiveMode]::Read)
        foreach ($e in $zip.Entries) {
            if (-not (Test-ZipEntryName $e.FullName)) { return $e.FullName }
        }
        return ''
    } finally {
        if ($zip) { $zip.Dispose() }
        if ($fs) { $fs.Dispose() }
    }
}

function Remove-OwnedFolder([string]$Path) {
    # Deletes a temp or staging folder this run registered; never throws.
    if ([string]::IsNullOrEmpty($Path)) { return }
    try {
        Register-OwnedTemp $Path
        if (Test-Path -LiteralPath $Path) { [void](Remove-KnownPath $Path) }
    } catch { Write-Log 'WARN' ("could not remove ${Path}: " + $_.Exception.Message) }
}

function Get-PortableProcesses {
    $appDir = Get-AppDir
    $found = @()
    foreach ($p in @(Get-Process -Name 'snipdeck' -ErrorAction SilentlyContinue)) {
        $path = ''
        try { $path = [string]$p.Path } catch { }
        if ($path -and (Test-PathUnder $path $appDir)) { $found += $p }
    }
    return $found
}

function Install-PortableApp {
    # Downloads the latest release zip, verifies it, unpacks it to app\ (replacing an older app\). Returns 0 or 1.
    Say-Step 'Getting the Snipdeck app'
    Say ('  From: https://github.com/{0}/releases/latest' -f $script:ReleaseRepo) 'Gray'

    $latest = Get-LatestRelease
    if (-not $latest.Ok) {
        Write-ReleaseNetworkFailure $latest 'find the latest release' 0
        return 1
    }
    $asset = Get-ReleaseAsset $latest.Release
    if ($null -eq $asset) {
        Write-FailureReport 'find the app in the latest release' 'the latest release has no portable zip (snipdeck-<version>-windows-x64.zip) with its .sha256 file.' @() @('Run Snipdeck.bat again later: the release may still be uploading.', 'If it keeps happening, report it at https://github.com/Braydenh563/Snipdeck/issues') @()
        return 1
    }
    Say ('  Latest release: {0}   file: {1}   size: {2}' -f $asset.Tag, $asset.ZipName, (Format-ApproxSize $asset.Size)) 'Gray'

    $pins = @{}
    $pinPath = Get-PinFilePath
    if (Test-Path -LiteralPath $pinPath) {
        try {
            $pinText = [System.IO.File]::ReadAllText($pinPath)
            $pins = Read-PinFile $pinText
            foreach ($prob in @(Get-PinFileProblems $pinText)) { Say "  Note: release-sha256.txt $prob" 'Yellow' }
        } catch { Say ('  Note: could not read release-sha256.txt: ' + $_.Exception.Message) 'Yellow' }
    }
    $pinned = Get-PinnedHash $pins $asset.ZipName

    $tmp = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), 'snipdeck-dl-' + [Guid]::NewGuid().ToString('N'))
    Register-OwnedTemp $tmp
    try {
        New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    } catch {
        Write-FailureReport 'save the download' ('the temporary folder could not be created: ' + $_.Exception.Message) @() @('Free some disk space or check that the Temp folder is writable, then run Snipdeck.bat again.') @()
        return 1
    }
    $zipPath = [System.IO.Path]::Combine($tmp, $asset.ZipName)
    $shaPath = [System.IO.Path]::Combine($tmp, $asset.ShaName)
    $rc = 1
    try {
        Say '  Downloading (this can take a minute)...' 'Gray'
        $d = Save-ReleaseFile $asset.ZipUrl $zipPath
        if (-not $d.Ok) { Write-ReleaseNetworkFailure $d 'download the app' $asset.Size; return 1 }
        $d = Save-ReleaseFile $asset.ShaUrl $shaPath
        if (-not $d.Ok) { Write-ReleaseNetworkFailure $d 'download the checksum file' 0; return 1 }

        # Verify: the .sha256 published with the release, and the pin in the repo when there is one.
        $actual = Get-FileSha256 $zipPath
        $published = ''
        try { $published = Read-ShaSidecar ([System.IO.File]::ReadAllText($shaPath)) $asset.ZipName } catch { }
        Write-Log 'INFO' ("sha256 of {0}: {1}; published {2}; pinned {3}" -f $asset.ZipName, $actual, $published, $pinned)
        if (-not $published) {
            Remove-OwnedFolder $tmp
            Write-FailureReport 'check the download' 'the .sha256 file published with the release could not be read, so the zip cannot be verified.' @() @('The download was deleted and nothing was installed.', 'Run Snipdeck.bat again. If it keeps happening, report it at https://github.com/Braydenh563/Snipdeck/issues') @()
            return 1
        }
        if ($actual -ne $published) {
            Remove-OwnedFolder $tmp
            Write-FailureReport 'trust the download' 'its SHA-256 does not match the .sha256 file published with the release. The file may be damaged or changed.' @() @('The download was deleted and nothing was installed. Snipdeck will not run an unverified app.', 'Run Snipdeck.bat again (a flaky connection can damage a download).', 'If it happens again, do not use the file; report it at https://github.com/Braydenh563/Snipdeck/issues') @("expected $published", "got      $actual")
            return 1
        }
        if ($pinned) {
            if ($actual -ne $pinned) {
                Remove-OwnedFolder $tmp
                Write-FailureReport 'trust the download' 'its SHA-256 does not match the hash pinned in this copy of Snipdeck (tools\launcher\release-sha256.txt).' @() @('The download was deleted and nothing was installed. Snipdeck will not run an unverified app.', 'If you updated this folder from git recently, run Snipdeck.bat again; otherwise do not use the file and report it at https://github.com/Braydenh563/Snipdeck/issues') @("pinned   $pinned", "got      $actual")
                return 1
            }
            Say '  Checked: SHA-256 matches the .sha256 file and the hash pinned in this copy of Snipdeck.' 'Green'
        } else {
            Say '  Checked: SHA-256 matches the .sha256 file published with the release.' 'Green'
            Say '  No pinned hash for this release in tools\launcher\release-sha256.txt, so only the published .sha256 file was used.' 'Yellow'
        }

        $bad = ''
        try { $bad = Test-ZipEntries $zipPath } catch {
            Remove-OwnedFolder $tmp
            Write-FailureReport 'unpack the app' ('the zip could not be read: ' + $_.Exception.Message) @() @('The download was deleted. Run Snipdeck.bat again.') @()
            return 1
        }
        if ($bad) {
            Remove-OwnedFolder $tmp
            Write-FailureReport 'unpack the app' ("the zip contains an unsafe file name ('$bad'), so it was not unpacked.") @() @('The download was deleted and nothing was installed.', 'Report it at https://github.com/Braydenh563/Snipdeck/issues') @()
            return 1
        }

        # Unpack next to app\ (same drive), then swap folders so a failure never leaves a half-installed app\.
        $app = Get-AppDir
        $stage = $app + '.new'
        $old = $app + '.old'
        Remove-OwnedFolder $stage
        Remove-OwnedFolder $old
        Say '  Unpacking...' 'Gray'
        $srcDir = $stage
        try {
            Expand-Archive -LiteralPath $zipPath -DestinationPath $stage -Force
            $exe = Get-ChildItem -LiteralPath $stage -Recurse -Filter $script:PortableExeName -File -ErrorAction Stop | Select-Object -First 1
            if ($null -eq $exe) { throw ('the zip does not contain ' + $script:PortableExeName) }
            $srcDir = $exe.DirectoryName
        } catch {
            Remove-OwnedFolder $stage
            Write-FailureReport 'unpack the app' ('the zip could not be unpacked: ' + $_.Exception.Message) @() @('Check that the disk has free space and that an antivirus is not blocking the folder.', 'Run Snipdeck.bat again.') @()
            return 1
        }
        $hadOld = Test-Path -LiteralPath $app
        try {
            if ($hadOld) { Rename-Item -LiteralPath $app -NewName (Split-Path -Leaf $old) -ErrorAction Stop }
            try {
                Move-Item -LiteralPath $srcDir -Destination $app -ErrorAction Stop
            } catch {
                if ($hadOld -and -not (Test-Path -LiteralPath $app) -and (Test-Path -LiteralPath $old)) {
                    try { Rename-Item -LiteralPath $old -NewName (Split-Path -Leaf $app) -ErrorAction Stop } catch { }
                }
                throw
            }
        } catch {
            Remove-OwnedFolder $stage
            Write-FailureReport 'put the app in place' ('the app folder could not be replaced: ' + $_.Exception.Message) @() @('If Snipdeck is running, quit it from its tray icon.', 'If an antivirus is scanning the folder, wait a minute.', 'Then run Snipdeck.bat update again. Your library and settings were not touched.') @()
            return 1
        }
        Remove-OwnedFolder $stage
        Remove-OwnedFolder $old
        Say ('  Installed Snipdeck {0} in {1}' -f $asset.Tag, $app) 'Green'
        $rc = 0
    } finally {
        Remove-OwnedFolder $tmp
    }
    return $rc
}

# ---------------------------------------------------------------- uninstall support

function Get-UninstallTargets([string]$RepoRoot, [string]$DataDir, [string]$TempRoot, [string[]]$TempLeftovers, [bool]$IncludeData) {
    # Pure: every folder "uninstall" may remove, as objects with Kind (app, app-staging, download, dist, data) and Path.
    # The data folder is listed only when $IncludeData is true. Nothing here touches the disk.
    $t = @()
    $app = [System.IO.Path]::Combine($RepoRoot, 'app')
    $t += [pscustomobject]@{ Kind = 'app'; Path = $app }
    $t += [pscustomobject]@{ Kind = 'app-staging'; Path = ($app + '.new') }
    $t += [pscustomobject]@{ Kind = 'app-staging'; Path = ($app + '.old') }
    foreach ($n in @($TempLeftovers)) {
        if ($n -match '^snipdeck-dl-[0-9a-f]{32}$') { $t += [pscustomobject]@{ Kind = 'download'; Path = [System.IO.Path]::Combine($TempRoot, $n) } }
    }
    $t += [pscustomobject]@{ Kind = 'dist'; Path = [System.IO.Path]::Combine($RepoRoot, 'dist') }
    if ($IncludeData) { $t += [pscustomobject]@{ Kind = 'data'; Path = $DataDir } }
    return $t
}

function Get-DownloadLeftovers {
    # Names of download folders an interrupted run left in the Temp folder.
    $names = @()
    try {
        foreach ($d in @(Get-ChildItem -LiteralPath ([System.IO.Path]::GetTempPath()) -Directory -Filter 'snipdeck-dl-*' -ErrorAction SilentlyContinue)) { $names += $d.Name }
    } catch { }
    return $names
}

function Stop-PortableApp {
    # Closes the app running from app\ (window first, then ends what is left). True when none is left.
    foreach ($p in (Get-PortableProcesses)) {
        if ($script:Opt.WhatIf) { Say "  [whatif] would stop snipdeck.exe (process $($p.Id))" 'Yellow'; continue }
        Say "  Stopping snipdeck.exe (process $($p.Id))" 'Gray'
        Write-Log 'STOP' "snipdeck pid $($p.Id)"
        try { [void]$p.CloseMainWindow() } catch { }
        $gone = $false
        try { $gone = $p.WaitForExit(3000) } catch { }
        if (-not $gone) { try { Stop-Process -Id $p.Id -Force -ErrorAction Stop } catch { Write-Log 'WARN' $_.Exception.Message } }
    }
    Start-Sleep -Milliseconds 400
    return (@(Get-PortableProcesses).Count -eq 0)
}

# ---------------------------------------------------------------- commands

function Test-PrebuiltPlatform {
    if ($script:OnWindows) { return $true }
    Say 'The prebuilt Snipdeck app is for Windows, so it cannot be downloaded or started on this computer.' 'Yellow'
    return $false
}

function Invoke-RunPrebuilt([bool]$Demo, [string[]]$AppArgs) {
    # Snipdeck.bat with no arguments: start app\snipdeck.exe, downloading it first when it is not there yet.
    $appArgs2 = @()
    if ($Demo -and (@($AppArgs) -notcontains '--demo')) { $appArgs2 += '--demo' }
    $appArgs2 += @($AppArgs)
    if ($Demo) { Say 'Snipdeck demo: a seeded library in memory, nothing is saved.' 'Cyan' } else { Say 'Snipdeck: opening your real library.' 'Cyan' }

    $exe = Get-PortableExe
    $have = Test-Path -LiteralPath $exe -PathType Leaf
    if ($script:Opt.WhatIf) {
        if ($have) { Say "  [whatif] app\$($script:PortableExeName) is there: would start it now." 'Yellow' }
        else { Say "  [whatif] app\$($script:PortableExeName) is missing: would download the latest release zip, verify its SHA-256, unpack it to app\ and start it." 'Yellow' }
        return 0
    }
    if (-not (Test-PrebuiltPlatform)) { return 1 }
    if (-not $have) {
        Say 'First start: the Snipdeck app is not on this PC yet, so it is downloaded once.' 'Gray'
        if (Test-SourceCheckout) {
            # No release published yet, but this folder holds the source: build it instead of giving up
            # (the build offers to install the Rust tools it needs; that is the one-time cost of a dev checkout).
            $latest = Get-LatestRelease
            if ((-not $latest.Ok) -and ([int]$latest.Status -eq 404)) {
                Say 'No release is published yet, and this folder contains the source code,' 'Yellow'
                Say 'so Snipdeck builds itself from source this once (same as "Snipdeck.bat build").' 'Yellow'
                return (Invoke-RunFlow $Demo $AppArgs $false)
            }
        }
        $rcs = @(Install-PortableApp)
        if ($rcs[$rcs.Count - 1] -ne 0) { return 1 }
    }
    $r = Start-SnipdeckApp $exe $appArgs2
    if ($r.Code -ne 0) { return 1 }
    return 0
}

function Test-SourceCheckout {
    # True when Snipdeck.bat sits in a source checkout (Cargo.toml and crates\ beside it), not in a release zip.
    $root = $script:RepoRoot
    return ((Test-Path -LiteralPath ([System.IO.Path]::Combine($root, 'Cargo.toml')) -PathType Leaf) -and
        (Test-Path -LiteralPath ([System.IO.Path]::Combine($root, 'crates')) -PathType Container))
}

function Invoke-ReinstallPrebuilt([string[]]$AppArgs) {
    # Snipdeck.bat reinstall: throw away app\ (and the dev build output), fetch or build the app again, start it.
    # The library and settings are never touched here.
    Say 'Reinstall: the app folder is removed and Snipdeck is set up again. Your library and settings are kept.' 'Cyan'
    $appDir = Split-Path -Parent (Get-PortableExe)
    if ($script:Opt.WhatIf) {
        Say "  [whatif] would delete $(Hide-UserName $appDir) and then run the normal first start." 'Yellow'
        return 0
    }
    if (Test-Path -LiteralPath $appDir) {
        try { Remove-Item -LiteralPath $appDir -Recurse -Force -ErrorAction Stop }
        catch {
            Write-FailureReport 'reinstall' ('the app folder could not be removed: ' + (Hide-UserName $_.Exception.Message)) @() @('Close Snipdeck if it is running (right-click the tray icon, Quit) and run this again.')
            return 1
        }
        Say "  Removed $(Hide-UserName $appDir)." 'Gray'
    }
    if (Test-SourceCheckout) { [void](Invoke-CleanBuild $false) }
    return (Invoke-RunPrebuilt $false @($AppArgs))
}

function Invoke-UpdatePrebuilt {
    # Snipdeck.bat update: download the latest release again even when app\ exists. The library and settings live elsewhere and are kept.
    Say-Step 'Updating Snipdeck'
    if ($script:Opt.WhatIf) {
        Say '  [whatif] would download the latest release zip, verify its SHA-256, replace app\ and start Snipdeck. Your library and settings are not touched.' 'Yellow'
        return 0
    }
    if (-not (Test-PrebuiltPlatform)) { return 1 }
    if (@(Get-PortableProcesses).Count -gt 0) {
        Say '  Snipdeck is running from app\, so it cannot be replaced yet.' 'Yellow'
        Say '  Quit it from its tray icon, then run Snipdeck.bat update again.' 'Gray'
        return 1
    }
    Say ('  Your library and settings ({0}) are kept.' -f (Get-DataDir)) 'Gray'
    $rcs = @(Install-PortableApp)
    if ($rcs[$rcs.Count - 1] -ne 0) { return 1 }
    $r = Start-SnipdeckApp (Get-PortableExe) @()
    if ($r.Code -ne 0) { return 1 }
    return 0
}
