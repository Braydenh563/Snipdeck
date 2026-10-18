# selftest.ps1 - "snipdeck.ps1 selftest": exercises the pure helpers. Prints PASS/FAIL; exit code 0 only if all pass.
# Dot-sourced by snipdeck.ps1. ASCII only. Reduced first version: see tools/launcher/README.md for what is not covered.

function Test-Case([string]$Name, [scriptblock]$Body) {
    $detail = ''
    $ok = $false
    try {
        $r = & $Body
        if ($r -eq $true) { $ok = $true } else { $detail = [string]$r }
    } catch { $detail = $_.Exception.Message }
    if ($ok) { Write-Host "PASS $Name" -ForegroundColor Green; $script:StPass++ }
    else { Write-Host "FAIL $Name $detail" -ForegroundColor Red; $script:StFail++ }
}

function Invoke-SelfTest {
    $script:StPass = 0
    $script:StFail = 0
    $dir = $script:LauncherDir
    $tmpRoot = [System.IO.Path]::GetTempPath()
    $tmp = [System.IO.Path]::Combine($tmpRoot, 'snipdeck-selftest-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp -Force | Out-Null
    try {
        # 1. known cargo failures against the sample logs
        $expect = [System.IO.Path]::Combine($dir, 'tests', 'expect.txt')
        foreach ($line in @(Get-Content -LiteralPath $expect)) {
            if ($line.Trim() -eq '' -or $line.StartsWith('#')) { continue }
            $parts = $line -split '=', 2
            $file = $parts[0].Trim()
            $want = $parts[1].Trim()
            if ($want -eq 'none') { $want = '' }
            Test-Case "failure patterns: $file" {
                $text = [System.IO.File]::ReadAllText([System.IO.Path]::Combine($dir, 'tests', $file))
                return ((@(Get-FailureMatches $text) -join ',') -eq $want)
            }
        }
        Test-Case 'remedy order for a network failure, each once' {
            $text = [System.IO.File]::ReadAllText([System.IO.Path]::Combine($dir, 'tests', 'network.log'))
            $tried = @{}
            $seq = @()
            while ($true) {
                $n = Get-NextRemedy $text $tried
                if (-not $n) { break }
                $seq += $n.Remedy
                $tried[$n.Remedy] = $true
            }
            return (($seq -join ',') -eq 'retry-5,retry-20,http-no-multiplex,offline')
        }
        Test-Case 'failed crate names' {
            $text = [System.IO.File]::ReadAllText([System.IO.Path]::Combine($dir, 'tests', 'corrupt.log'))
            return ((@(Get-FailedCrates $text) -join ',') -eq 'snipdeck-ui')
        }
        Test-Case 'crate names with odd characters are ignored' {
            return (@(Get-FailedCrates 'could not compile `a;b` and `..\x`').Count -eq 0)
        }

        # 2. stamp hashing
        Test-Case 'sha256 test vector' { return ((Get-StringSha256 'abc') -eq 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad') }
        Test-Case 'stamp changes with each input' {
            $a = New-StampHash 'h' 'l' 'd' 'u' 't'
            $same = ($a -eq (New-StampHash 'h' 'l' 'd' 'u' 't'))
            $diff = ($a -ne (New-StampHash 'h2' 'l' 'd' 'u' 't')) -and ($a -ne (New-StampHash 'h' 'l2' 'd' 'u' 't')) -and ($a -ne (New-StampHash 'h' 'l' 'd2' 'u' 't')) -and ($a -ne (New-StampHash 'h' 'l' 'd' 'u2' 't')) -and ($a -ne (New-StampHash 'h' 'l' 'd' 'u' 't2')) -and ($a -ne (New-StampHash 'h' 'l' 'd' 'u' 't' 'release'))
            return ($same -and $diff -and $a.Length -eq 64)
        }
        Test-Case 'stamp match' {
            $h = New-StampHash 'h' 'l' 'd' 'u' 't'
            return ((Test-StampMatch ([pscustomobject]@{ hash = $h }) $h) -and -not (Test-StampMatch ([pscustomobject]@{ hash = $h }) ('0' * 64)) -and -not (Test-StampMatch $null $h))
        }

        # 3. path guards
        $root = [System.IO.Path]::Combine($tmp, 'root')
        New-Item -ItemType Directory -Path $root -Force | Out-Null
        $sep = [string][System.IO.Path]::DirectorySeparatorChar
        Test-Case 'path guard: child ok, sibling prefix and dot-dot refused' {
            return ((Test-PathUnder ([System.IO.Path]::Combine($root, 'a', 'b')) $root) -and -not (Test-PathUnder ($root + '2') $root) -and -not (Test-PathUnder ([System.IO.Path]::Combine($root, '..', 'x')) $root) -and -not (Test-PathUnder $root $root) -and (Test-PathUnder $root $root -AllowEqual))
        }
        Test-Case 'path guard: drive root and empty refused' {
            return (-not (Test-PathUnder $root ([System.IO.Path]::GetPathRoot($root))) -and -not (Test-PathUnder '' $root))
        }
        Test-Case 'zip entry names' {
            return ((Test-ZipEntryName 'a/b.txt') -and -not (Test-ZipEntryName '../x') -and -not (Test-ZipEntryName 'a/../../x') -and -not (Test-ZipEntryName '/abs') -and -not (Test-ZipEntryName 'C:\x') -and -not (Test-ZipEntryName ''))
        }
        Test-Case 'target dir: short stays, long moves' {
            $short = Get-TargetDir ([System.IO.Path]::Combine($tmp, 'r')) 'ROOTX'
            $long = Get-TargetDir ([System.IO.Path]::Combine($tmp, ('x' * 120))) 'ROOTX'
            return (($short -eq [System.IO.Path]::Combine($tmp, 'r', 'target')) -and ($long -eq 'ROOTX' + $sep + 'sdtarget'))
        }
        Test-Case 'argument quoting' {
            return ((ConvertTo-ArgString @('a b', 'c')) -eq '"a b" c')
        }

        # 4. backup naming and rotation
        Test-Case 'backup name' {
            return ((New-BackupName ([datetime]'2026-10-18T10:15:00') '') -eq 'snipdeck-20261018-101500.zip')
        }
        $bdir = [System.IO.Path]::Combine($tmp, 'backups')
        New-Item -ItemType Directory -Path $bdir -Force | Out-Null
        for ($i = 0; $i -lt 14; $i++) { [System.IO.File]::WriteAllText([System.IO.Path]::Combine($bdir, (New-BackupName ([datetime]'2026-10-01T10:00:00').AddMinutes($i) '')), 'x') }
        for ($i = 0; $i -lt 3; $i++) { [System.IO.File]::WriteAllText([System.IO.Path]::Combine($bdir, (New-BackupName ([datetime]'2026-10-01T10:00:00').AddMinutes($i) 'pre-restore')), 'x') }
        [System.IO.File]::WriteAllText([System.IO.Path]::Combine($bdir, 'notes.zip'), 'x')
        Test-Case 'rotation: dry run deletes nothing' {
            $would = @(Invoke-Rotation $bdir $script:BackupNameRegex 10 -WhatIf)
            return (($would.Count -eq 4) -and (@(Get-ChildItem -LiteralPath $bdir).Count -eq 18))
        }
        Test-Case 'rotation: keeps newest 10 per kind, ignores other files, is repeatable' {
            $gone = @(Invoke-Rotation $bdir $script:BackupNameRegex 10)
            $again = @(Invoke-Rotation $bdir $script:BackupNameRegex 10)
            $main = @(Get-ChildItem -LiteralPath $bdir -Filter 'snipdeck-????????-??????.zip').Count
            return (($gone.Count -eq 4) -and ($again.Count -eq 0) -and ($main -eq 10) -and (Test-Path -LiteralPath ([System.IO.Path]::Combine($bdir, 'notes.zip'))))
        }

        # 5. settings checks
        Test-Case 'json validity' {
            return ((Test-JsonText '{"a":1}').Ok -and -not (Test-JsonText '{"a":').Ok -and -not (Test-JsonText '').Ok)
        }
        Test-Case 'hotkey conflicts' {
            $s = '{"hotkeys":{"toggle_panel":"Ctrl+Alt+Space","picker":"alt+control+space","capture":"Ctrl+Shift+2"}}' | ConvertFrom-Json
            return (@(Get-HotkeyConflicts $s).Count -eq 1)
        }
        Test-Case 'overall status' {
            $c = @((New-Check 'a' 'pass'), (New-Check 'b' 'warn'))
            return ((Get-OverallStatus $c) -eq 'warn' -and (Get-OverallStatus @((New-Check 'a' 'pass'))) -eq 'pass')
        }

        # 6. arguments
        Test-Case 'options: demo with app flag' {
            $o = Read-Options @('demo', '--dark')
            return (($o.Command -eq 'demo') -and ($o.Pass -contains '--dark') -and -not $o.Error)
        }
        Test-Case 'options: flags and unknown command' {
            $o = Read-Options @('uninstall', '-Yes', '--whatif')
            $bad = Read-Options @('frobnicate')
            return ($o.Yes -and $o.WhatIf -and ($o.Command -eq 'uninstall') -and [bool]$bad.Error)
        }

        Test-Case 'options: build and update are commands; run takes app flags' {
            $b = Read-Options @('build')
            $u = Read-Options @('update')
            $uBad = Read-Options @('update', '--dark')
            $r = Read-Options @('--dark')
            return (($b.Command -eq 'build') -and ($u.Command -eq 'update') -and -not $u.Error -and [bool]$uBad.Error -and ($r.Command -eq '') -and ($r.Pass -contains '--dark'))
        }

        # 6b. release download path (no network): asset resolver, pin file, SHA-256 compare
        $relJson = [System.IO.File]::ReadAllText([System.IO.Path]::Combine($dir, 'tests', 'release.json'))
        Test-Case 'release: latest-release API url' {
            return ((Get-ReleaseApiUrl) -eq 'https://api.github.com/repos/Braydenh563/Snipdeck/releases/latest')
        }
        Test-Case 'release: asset resolver on the saved sample JSON' {
            $a = Get-ReleaseAsset ($relJson | ConvertFrom-Json)
            if (-not $a) { return 'no asset found' }
            $okName = ($a.ZipName -eq 'snipdeck-v0.1.0-windows-x64.zip') -and ($a.Tag -eq 'v0.1.0') -and ($a.Size -eq 23456789)
            $okUrl = ($a.ZipUrl -eq 'https://github.com/Braydenh563/Snipdeck/releases/download/v0.1.0/snipdeck-v0.1.0-windows-x64.zip') -and ($a.ShaName -eq 'snipdeck-v0.1.0-windows-x64.zip.sha256') -and ($a.ShaUrl -like '*/v0.1.0/snipdeck-v0.1.0-windows-x64.zip.sha256')
            return ($okName -and $okUrl)
        }
        Test-Case 'release: resolver returns nothing without a zip or a sidecar' {
            $none = '{"tag_name":"v1","assets":[]}' | ConvertFrom-Json
            $noAssets = '{"tag_name":"v1"}' | ConvertFrom-Json
            $noSide = '{"tag_name":"v1","assets":[{"name":"snipdeck-v1-windows-x64.zip","size":5,"browser_download_url":"https://example.invalid/a.zip"}]}' | ConvertFrom-Json
            $other = '{"tag_name":"v1","assets":[{"name":"other.zip","browser_download_url":"https://example.invalid/o.zip"},{"name":"other.zip.sha256","browser_download_url":"https://example.invalid/o.sha"}]}' | ConvertFrom-Json
            return (($null -eq (Get-ReleaseAsset $null)) -and ($null -eq (Get-ReleaseAsset $none)) -and ($null -eq (Get-ReleaseAsset $noAssets)) -and ($null -eq (Get-ReleaseAsset $noSide)) -and ($null -eq (Get-ReleaseAsset $other)))
        }
        $h1 = ('ab' * 32)
        $h2 = ('CD' * 32)
        $pinText = "# comment`n`n$h1  snipdeck-v0.1.0-windows-x64.zip`n$h2 *snipdeck-v0.2.0-windows-x64.zip`nnot a pin line`n"
        Test-Case 'release: pin file parser' {
            $pins = Read-PinFile $pinText
            return (((Get-PinnedHash $pins 'snipdeck-v0.1.0-windows-x64.zip') -eq $h1) -and ((Get-PinnedHash $pins 'snipdeck-v0.2.0-windows-x64.zip') -eq $h2.ToLowerInvariant()) -and ((Get-PinnedHash $pins 'snipdeck-v9-windows-x64.zip') -eq '') -and ((Get-PinnedHash (Read-PinFile '') 'x.zip') -eq '') -and ((Get-PinnedHash (Read-PinFile $null) 'x.zip') -eq ''))
        }
        Test-Case 'release: pin file problems are reported, comments and blanks are not' {
            return ((@(Get-PinFileProblems $pinText).Count -eq 1) -and (@(Get-PinFileProblems "# only a comment`n`n").Count -eq 0) -and (@(Get-PinFileProblems "$h1  a.zip`n$h1  a.zip`n").Count -eq 1))
        }
        Test-Case 'release: the pin file shipped in the repo is well formed' {
            $shipped = [System.IO.File]::ReadAllText([System.IO.Path]::Combine($dir, 'release-sha256.txt'))
            $probs = @(Get-PinFileProblems $shipped)
            if ($probs.Count -gt 0) { return ($probs -join '; ') }
            return $true
        }
        Test-Case 'release: sidecar parser' {
            $name = 'snipdeck-v0.1.0-windows-x64.zip'
            return (((Read-ShaSidecar "$h1  $name`r`n" $name) -eq $h1) -and ((Read-ShaSidecar "$($h2.ToUpperInvariant()) *$name" $name) -eq $h2.ToLowerInvariant()) -and ((Read-ShaSidecar "$h1  other.zip" $name) -eq '') -and ((Read-ShaSidecar 'garbage' $name) -eq '') -and ((Read-ShaSidecar '' $name) -eq '') -and ((Read-ShaSidecar "$h1" $name) -eq $h1))
        }
        $shaFile = [System.IO.Path]::Combine($tmp, 'abc.bin')
        [System.IO.File]::WriteAllBytes($shaFile, [System.Text.Encoding]::ASCII.GetBytes('abc'))
        $abc = 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
        Test-Case 'release: SHA-256 compare on a temp file' {
            return ((Test-FileSha256 $shaFile $abc) -and (Test-FileSha256 $shaFile $abc.ToUpperInvariant()) -and -not (Test-FileSha256 $shaFile ('0' * 64)) -and -not (Test-FileSha256 $shaFile 'abc') -and -not (Test-FileSha256 $shaFile '') -and -not (Test-FileSha256 ([System.IO.Path]::Combine($tmp, 'missing.bin')) $abc))
        }
        Test-Case 'uninstall: target list has the app and leftover downloads, data only with --data (no real folders)' {
            $rr = [System.IO.Path]::Combine($tmp, 'fakeroot')
            $dd = [System.IO.Path]::Combine($tmp, 'fakedata', 'Snipdeck')
            $left = @('snipdeck-dl-' + ('a' * 32), 'snipdeck-dl-nothex', 'other-folder')
            $keep = @(Get-UninstallTargets $rr $dd $tmp $left $false)
            $all = @(Get-UninstallTargets $rr $dd $tmp $left $true)
            $kinds = ($keep | ForEach-Object { $_.Kind }) -join ','
            $paths = @($keep | ForEach-Object { $_.Path })
            $okApp = ($paths -contains [System.IO.Path]::Combine($rr, 'app')) -and ($paths -contains ([System.IO.Path]::Combine($rr, 'app') + '.new')) -and ($paths -contains [System.IO.Path]::Combine($tmp, 'snipdeck-dl-' + ('a' * 32)))
            $okSkip = ($paths -notcontains [System.IO.Path]::Combine($tmp, 'snipdeck-dl-nothex')) -and ($paths -notcontains [System.IO.Path]::Combine($tmp, 'other-folder'))
            $okData = ($paths -notcontains $dd) -and (@($all | Where-Object { $_.Path -eq $dd -and $_.Kind -eq 'data' }).Count -eq 1) -and ($all.Count -eq ($keep.Count + 1))
            if (-not ($okApp -and $okSkip -and $okData)) { return "kinds=$kinds app=$okApp skip=$okSkip data=$okData" }
            return (-not (Test-Path -LiteralPath $rr))
        }
        Test-Case 'uninstall: options accept --data and -Yes' {
            $o = Read-Options @('uninstall', '--data', '-Yes')
            return ($o.RemoveData -and $o.Yes -and -not $o.Error -and ($o.Command -eq 'uninstall'))
        }
        Test-Case 'release: HTTP status is read from an error record' {
            $fake = [pscustomobject]@{ Exception = [pscustomobject]@{ Response = [pscustomobject]@{ StatusCode = 404 } } }
            $none = [pscustomobject]@{ Exception = [pscustomobject]@{ Response = $null } }
            return (((Get-HttpStatus $fake) -eq 404) -and ((Get-HttpStatus $none) -eq 0) -and ((Get-HttpStatus $null) -eq 0))
        }

        # 7. every launcher script parses, and is ASCII
        foreach ($f in @('snipdeck.ps1', 'lib.ps1', 'cli.ps1', 'core.ps1', 'build.ps1', 'release.ps1', 'data.ps1', 'selftest.ps1')) {
            Test-Case "parses and is ASCII: $f" {
                $p = [System.IO.Path]::Combine($dir, $f)
                $tokens = $null
                $errs = $null
                [void][System.Management.Automation.Language.Parser]::ParseFile($p, [ref]$tokens, [ref]$errs)
                $bytes = [System.IO.File]::ReadAllBytes($p)
                $nonAscii = @($bytes | Where-Object { $_ -gt 127 }).Count
                if (@($errs).Count -gt 0) { return ('parse error: ' + $errs[0].Message) }
                return ($nonAscii -eq 0)
            }
        }
    } finally {
        try { if (Test-PathUnder $tmp $tmpRoot) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue } } catch { }
    }
    Write-Host ''
    Write-Host ("{0} passed, {1} failed" -f $script:StPass, $script:StFail)
    if ($script:StFail -gt 0) { return 1 }
    return 0
}
