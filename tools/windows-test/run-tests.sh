#!/usr/bin/env bash
# Launcher for the Windows test wizard from Git Bash, WSL or a machine with PowerShell 7.
# The wizard itself is run-tests.ps1; the apps under test are Windows-only.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
script="$here/run-tests.ps1"

if command -v powershell.exe >/dev/null 2>&1; then
  if command -v wslpath >/dev/null 2>&1; then script="$(wslpath -w "$script")"
  elif command -v cygpath >/dev/null 2>&1; then script="$(cygpath -w "$script")"; fi
  exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$script" "$@"
elif command -v pwsh >/dev/null 2>&1; then
  exec pwsh -NoProfile -File "$script" "$@"
else
  echo "PowerShell not found. Run tools/windows-test/run-tests.bat from Windows instead." >&2
  exit 1
fi
