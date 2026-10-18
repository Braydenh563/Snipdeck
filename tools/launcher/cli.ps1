# cli.ps1 - command-line parsing and help text (pure). Dot-sourced by snipdeck.ps1. ASCII only.

$script:Commands = @('run', 'demo', 'doctor', 'update', 'backup', 'restore', 'clean', 'uninstall', 'logs', 'help', 'selftest')

function Read-Options([string[]]$ArgList) {
    # The launcher has no param() block on purpose: Windows PowerShell would treat "--whatif" and the app's own
    # options ("--dark", "--screenshot x.png") as errors. Returns an object with Command, flags, Pass and Error.
    $o = [ordered]@{
        Command = ''; Yes = $false; WhatIf = $false; Rebuild = $false; Open = $false
        RemoveTarget = $false; RemoveData = $false; Pass = @(); Error = ''
    }
    $rest = $false
    foreach ($a in @($ArgList)) {
        if ($null -eq $a) { continue }
        if ($rest) { $o.Pass += $a; continue }
        if ($a -eq '--') { $rest = $true; continue }
        $low = $a.ToLowerInvariant()
        if ($low -eq '/?') {
            if (-not $o.Command) { $o.Command = 'help' }
            continue
        }
        if ($low.StartsWith('-')) {
            $name = $low.TrimStart([char]'-')
            if ($name -eq 'yes' -or $name -eq 'y') { $o.Yes = $true }
            elseif ($name -eq 'whatif' -or $name -eq 'dry-run' -or $name -eq 'dryrun') { $o.WhatIf = $true }
            elseif ($name -eq 'rebuild' -or $name -eq 'force') { $o.Rebuild = $true }
            elseif ($name -eq 'open') { $o.Open = $true }
            elseif ($name -eq 'target') { $o.RemoveTarget = $true }
            elseif ($name -eq 'data') { $o.RemoveData = $true }
            elseif (($name -eq 'help' -or $name -eq 'h') -and -not $o.Command) { $o.Command = 'help' }
            else { $o.Pass += $a }
            continue
        }
        if (-not $o.Command -and ($script:Commands -contains $low)) { $o.Command = $low; continue }
        if (-not $o.Command -and $o.Pass.Count -eq 0) {
            $o.Error = "Unknown command '$a'. Try: Snipdeck.bat help"
            break
        }
        $o.Pass += $a
    }
    if (-not $o.Error) {
        $cmd = $o.Command
        if (-not $cmd) { $cmd = 'run' }
        $appCommand = ($cmd -eq 'run' -or $cmd -eq 'demo')
        if (-not $appCommand) {
            foreach ($p in @($o.Pass)) {
                if ($p.StartsWith('-')) { $o.Error = "Unknown option '$p' for '$cmd'. Try: Snipdeck.bat help"; break }
            }
            if (-not $o.Error -and $cmd -ne 'restore' -and @($o.Pass).Count -gt 0) {
                $o.Error = "'$cmd' does not take '$($o.Pass[0])'. Try: Snipdeck.bat help"
            }
        }
    }
    return $o
}

function Get-HelpText {
    return @'
Snipdeck launcher

Usage:  Snipdeck.bat [command] [options]

Commands
  run         (default) build if needed and start Snipdeck on your real library
  demo        start the demo: a seeded library kept in memory, nothing is saved
  doctor      check this PC and Snipdeck's data; writes a report you can send
  update      get the latest version (git pull), rebuild and start
  backup      zip your library and settings to Documents\Snipdeck-backups
  restore     put a backup back (saves your current data first)
  clean       delete build files (target\ and dist\); your data is not touched
  uninstall   remove build files, shortcuts and the start-with-Windows entry;
              your data only if you type DELETE
  logs        show the newest launcher log (add --open to open the folder)
  help        this text

Options
  -Yes        answer yes to questions (unattended); never deletes your data
              unless you also pass --data to uninstall
  --whatif    show what a command would do and change nothing
  --rebuild   run/demo: ignore the fast-start copy and build again
  --target    uninstall: also delete the build folder without asking
  --data      uninstall with -Yes: also delete your library and settings
  --          everything after goes to the app

Anything else after run or demo goes to the app, for example:
  Snipdeck.bat demo --dark
  Snipdeck.bat demo --screenshot panel.png

Files
  Logs      %LOCALAPPDATA%\Snipdeck\logs   (newest 10 kept)
  Backups   Documents\Snipdeck-backups     (newest 10 of each kind kept)
  Library   %LOCALAPPDATA%\Snipdeck
'@
}
