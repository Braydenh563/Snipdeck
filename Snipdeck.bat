@echo off
rem Snipdeck one-click launcher. Double-click to run; or: Snipdeck.bat help
rem Runs tools\launcher\snipdeck.ps1 with the execution policy bypassed for this one process only.
setlocal
pushd "%~dp0"
set "PS=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
if not exist "%PS%" set "PS=powershell"
where "%PS%" >nul 2>nul
if errorlevel 1 (
  echo Windows PowerShell was not found on this PC.
  pause
  popd
  exit /b 1
)
rem Started by double-click? Then the PowerShell side keeps the window open after information commands.
echo %cmdcmdline% | find /i "%~nx0" >nul && set "SNIPDECK_DBLCLICK=1"
"%PS%" -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\launcher\snipdeck.ps1" %*
set "CODE=%ERRORLEVEL%"
if not "%CODE%"=="0" (
  echo.
  echo Snipdeck stopped with exit code %CODE%. The text above says what happened; the log file path is in it.
  pause
)
popd
endlocal & exit /b %CODE%
