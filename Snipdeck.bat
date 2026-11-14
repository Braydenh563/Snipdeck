@echo off
rem Snipdeck one-click launcher. Double-click to run; or: Snipdeck.bat help
rem Runs tools\launcher\snipdeck.ps1 with the execution policy bypassed for this one process only.
setlocal
pushd "%~dp0"
set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if exist "%PS%" goto havePS
set "PS=powershell"
rem "where" takes a bare command name only; a full path always fails it, so it is used only here.
where powershell >nul 2>nul
if errorlevel 1 (
  echo Windows PowerShell was not found on this PC ^(looked in %SystemRoot%\System32\WindowsPowerShell and on PATH^).
  pause
  popd
  exit /b 1
)
:havePS
rem Started by double-click? Then the PowerShell side keeps the window open after information commands.
rem The command line can hold special characters from the folder name: keep it inside quotes (quotes dropped) and test by substitution.
set "CL=%cmdcmdline:"=%"
call set "R=%%CL:%~nx0=%%"
if not "%R%"=="%CL%" set "SNIPDECK_DBLCLICK=1"
"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\launcher\snipdeck.ps1" %*
set "CODE=%ERRORLEVEL%"
if not "%CODE%"=="0" (
  echo.
  echo Snipdeck stopped with exit code %CODE%. The text above says what happened; the log file path is in it.
  pause
)
popd
endlocal & exit /b %CODE%
