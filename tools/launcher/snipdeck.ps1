<#
.SYNOPSIS
  Snipdeck one-click launcher: run, demo, update, build, doctor, backup, restore, clean, uninstall, logs.

.DESCRIPTION
  Start it with Snipdeck.bat in the repository root (double-click, or from a terminal with a command).
  run (the default) starts app\snipdeck.exe; when it is missing it downloads the portable zip from the GitHub
  Release, verifies its SHA-256 and unpacks it. It never checks for Rust, Visual Studio or git.
  "build" is the developer path: it checks the PC, installs what is missing (with your OK), builds a debug
  build, keeps a fast-start copy in dist\, starts the app, and repairs known build failures on its own.
  Every step is logged to %LOCALAPPDATA%\Snipdeck\logs. Run "Snipdeck.bat help" for the commands.

  Works on Windows PowerShell 5.1 (built in) and PowerShell 7. The launcher has no param() block on purpose,
  so that "--whatif" and the app's own options pass through; see cli.ps1.

.NOTES
  Files: lib.ps1 (pure helpers), cli.ps1 (arguments, help), core.ps1 (console, logging, process, checks),
  release.ps1 (download, verify, unpack, run, update), build.ps1 (stamp, build, cache, launch),
  data.ps1 (backup, restore, uninstall, doctor),
  selftest.ps1 ("snipdeck.ps1 selftest"), tests\ (sample cargo logs).
  This launcher has not yet been run on Windows by its owner; see docs/troubleshooting.md.
#>

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$script:LauncherVersion = '1'
$script:LauncherDir = $PSScriptRoot
$script:RepoRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($PSScriptRoot, '..', '..'))
$script:TargetOverride = $null
$script:Opt = @{ Yes = $false; WhatIf = $false; Rebuild = $false; Open = $false; RemoveTarget = $false; RemoveData = $false }

. ([System.IO.Path]::Combine($PSScriptRoot, 'lib.ps1'))
. ([System.IO.Path]::Combine($PSScriptRoot, 'cli.ps1'))
. ([System.IO.Path]::Combine($PSScriptRoot, 'core.ps1'))
. ([System.IO.Path]::Combine($PSScriptRoot, 'build.ps1'))
. ([System.IO.Path]::Combine($PSScriptRoot, 'release.ps1'))
. ([System.IO.Path]::Combine($PSScriptRoot, 'data.ps1'))
. ([System.IO.Path]::Combine($PSScriptRoot, 'selftest.ps1'))

function Invoke-Main([string[]]$ArgList) {
    $o = Read-Options $ArgList
    if ($o.Error) {
        Write-Host $o.Error -ForegroundColor Red
        return 2
    }
    $script:Opt = $o
    $cmd = $o.Command
    if (-not $cmd) { $cmd = 'run' }
    if ($cmd -eq 'help') { Write-Host (Get-HelpText); return 0 }
    if ($cmd -eq 'selftest') { return (Invoke-SelfTest) }

    Start-Log $cmd
    Say ''
    if ($cmd -eq 'build') { Say 'Developer build from source. Users do not need this: run Snipdeck.bat without arguments.' 'Yellow' }
    Say ("Snipdeck launcher v{0}   command: {1}" -f $script:LauncherVersion, $cmd) 'Cyan'
    if ($o.WhatIf) { Say 'Dry run (--whatif): nothing will be changed.' 'Yellow' }
    if (-not $script:OnWindows) { Say 'Note: this is not Windows. Snipdeck is Windows-first; some checks do not apply.' 'DarkGray' }

    switch ($cmd) {
        'run' { return (Invoke-RunPrebuilt $false @($o.Pass)) }
        'demo' { return (Invoke-RunPrebuilt $true @($o.Pass)) }
        'build' { return (Invoke-RunFlow $false @($o.Pass) $o.Rebuild) }
        'doctor' { return (Invoke-Doctor) }
        'update' { return (Invoke-UpdatePrebuilt) }
        'reinstall' { return (Invoke-ReinstallPrebuilt @($o.Pass)) }
        'backup' { return (Invoke-Backup) }
        'restore' { return (Invoke-Restore @($o.Pass)) }
        'clean' { return (Invoke-Clean) }
        'uninstall' { return (Invoke-Uninstall) }
        'logs' { return (Invoke-Logs $o.Open) }
    }
    return 2
}

$exitCode = 1
try {
    # Capture everything: a stray value from a helper must never become the exit code.
    $results = @(Invoke-Main $args)
    $last = $null
    if ($results.Count -gt 0) { $last = $results[$results.Count - 1] }
    if ($last -is [int]) { $exitCode = $last } else { $exitCode = 1 }
} catch {
    Write-Log 'FATAL' ($_.Exception.Message + ' | ' + $_.ScriptStackTrace)
    Write-FailureReport 'finish' ('the launcher hit an unexpected problem: ' + $_.Exception.Message) @() @('Run it again; every step is safe to repeat.', 'Run "Snipdeck.bat doctor" and send the report with the log file named below.')
    $exitCode = 1
}
if ($exitCode -eq 0 -and $env:SNIPDECK_DBLCLICK -eq '1') {
    $quiet = @('run', 'demo', 'build', 'update', 'reinstall', 'selftest')
    $first = ''
    if (@($args).Count -gt 0) { $first = ([string]$args[0]).ToLowerInvariant() }
    if ($first -and ($quiet -notcontains $first)) { Wait-ForEnter }
}
exit $exitCode
