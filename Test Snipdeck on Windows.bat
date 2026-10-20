@echo off
rem Double-click this file to test Snipdeck on this Windows PC. It starts the test wizard
rem (tools\windows-test\run-tests.ps1) in Windows PowerShell 5.1 and keeps this window open at the end.
setlocal
title Test Snipdeck on Windows
cd /d "%~dp0"
set "CODE=1"
set "PSEXE=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
set "WIZARD=%~dp0tools\windows-test\run-tests.ps1"

if not exist "%WIZARD%" goto :nofiles
if not exist "%PSEXE%" goto :nopowershell

"%PSEXE%" -NoProfile -ExecutionPolicy Bypass -File "%WIZARD%" %*
set "CODE=%ERRORLEVEL%"
if "%CODE%"=="0" goto :end
echo.
echo  The test wizard stopped because of a problem (code %CODE%).
echo  Read the message above. More detail is saved in:
echo    %~dp0spikes-results\wizard-log.txt
echo  If you ask Claude for help, send that file.
goto :end

:nofiles
echo.
echo  Some Snipdeck files are missing next to this file.
echo  If you downloaded a ZIP: right-click the ZIP, choose "Extract All", open the new folder,
echo  and double-click this file again from there (not from inside the ZIP).
goto :end

:nopowershell
echo.
echo  Windows PowerShell was not found on this PC, so the test wizard cannot start.
echo  It is part of Windows 10 and 11; ask Claude for help if it is missing.
goto :end

:end
echo.
echo  Press any key to close this window.
pause >nul
endlocal & exit /b %CODE%
