# cli.ps1 - command-line parsing and help text (pure). Dot-sourced by snipdeck.ps1. ASCII only.

$script:Commands = @('run', 'demo', 'build', 'doctor', 'update', 'reinstall', 'backup', 'restore', 'clean', 'uninstall', 'logs', 'help', 'selftest')

function Read-Options([string[]]$ArgList) {
    # The launcher has no param() block on purpose: Windows PowerShell would treat "--whatif" and the app's own
    # options ("--dark", "--screenshot x.png") as errors. Returns an object with Command, flags, Pass and Error.
    $o = [ordered]@{
        Command = ''; Yes = $false; WhatIf = $false; Rebuild = $false; Stale = $false; Open = $false
        RemoveTarget = $false; RemoveData = $false; AllowUnpinned = $false; Pass = @(); Error = ''
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
            elseif ($name -eq 'stale' -or $name -eq 'no-rebuild') { $o.Stale = $true }
            elseif ($name -eq 'open') { $o.Open = $true }
            elseif ($name -eq 'target') { $o.RemoveTarget = $true }
            elseif ($name -eq 'data') { $o.RemoveData = $true }
            elseif ($name -eq 'allow-unpinned') { $o.AllowUnpinned = $true }
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
        $appCommand = ($cmd -eq 'run' -or $cmd -eq 'demo' -or $cmd -eq 'build' -or $cmd -eq 'reinstall')
        if ($o.AllowUnpinned -and @('run', 'demo', 'update', 'reinstall') -notcontains $cmd) { $o.Error = "--allow-unpinned only applies to run, demo, update and reinstall. Try: Snipdeck.bat help" }
        if ($o.Rebuild -and $cmd -ne 'build') { $o.Error = "--rebuild only applies to 'build'. Try: Snipdeck.bat help" }
        if ($o.Stale -and @('run', 'demo', 'build') -notcontains $cmd) { $o.Error = "--stale only applies to run, demo and build. Try: Snipdeck.bat help" }
        if ($o.Stale -and $o.Rebuild) { $o.Error = '--stale and --rebuild contradict each other. Try: Snipdeck.bat help' }
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

Snipdeck.bat needs nothing installed. The first time it downloads the app once
from the project's GitHub Release, checks it, and keeps it in the app folder next to it.

Commands
  run         (default) start Snipdeck on your real library (downloads the app first if needed)
  demo        start the demo: a seeded library kept in memory, nothing is saved
  update      download the latest release again (your library and settings are kept) and start it
  reinstall   remove the app folder and set Snipdeck up again from scratch (library and settings kept)
  build       developers only: build from source (needs Rust, Visual C++ tools, git) and start
  selftest    run the launcher's own checks (no network, changes nothing)
  doctor      check this PC and Snipdeck's data; writes a report you can send
  backup      zip your library and settings to Documents\Snipdeck-backups
  restore     put a backup back (saves your current data first)
  clean       delete build files (target\ and dist\); your data is not touched
  uninstall   remove the app folder (app\), build files, shortcuts and the start-with-Windows
              entry; your library is KEPT. Add --data to also erase it (you must type DELETE)
  logs        show the newest launcher log (add --open to open the folder)
  help        this text

Options
  -Yes        answer yes to questions (unattended); never deletes your data
              unless you also pass --data to uninstall
  --whatif    show what a command would do and change nothing
  --rebuild   build: ignore the fast-start copy and build again
  --stale     run, demo, build: start the last build even if the source changed
              since (skips the rebuild after a git pull; build later when you have time)
  --target    uninstall: also delete the build folder without asking
  --allow-unpinned
              run, demo, update, reinstall: accept a release whose hash is not pinned in
              tools\launcher\release-sha256.txt (without it you are asked, or refused when unattended)
  --data      uninstall: also erase your library and settings (asks you to type DELETE;
              with -Yes it erases without asking)
  --          everything after goes to the app

To uninstall and install again:  Snipdeck.bat uninstall   (keeps your library)
                                 Snipdeck.bat uninstall --data   (erases it too)
                                 Snipdeck.bat   (installs again, finds a kept library)

Anything else after run, demo or build goes to the app, for example:
  Snipdeck.bat demo --dark
  Snipdeck.bat demo --screenshot panel.png

Files
  Logs      %LOCALAPPDATA%\Snipdeck\logs   (newest 10 kept)
  Backups   Documents\Snipdeck-backups     (newest 10 of each kind kept)
  App       app\snipdeck.exe next to Snipdeck.bat (build: dist\ instead)
  Library   %LOCALAPPDATA%\Snipdeck
'@
}
