@echo off
rem Double-click this file to test Snipdeck on this Windows PC. It asks Quick check (about 10 minutes) or
rem Full test (about an hour), then starts the test wizard (tools\windows-test\run-tests.ps1) in Windows
rem PowerShell 5.1 and keeps this window open at the end. Any argument skips the question and goes to the wizard
rem unchanged (for example: -Mode qc, or -SelfTest).
setlocal
title Test Snipdeck on Windows
cd /d "%~dp0"
set "CODE=1"
set "PSEXE=%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"
set "WIZARD=%~dp0tools\windows-test\run-tests.ps1"

set "QUICK="
if not exist "%WIZARD%" goto :nofiles
if not exist "%PSEXE%" goto :nopowershell

if not "%~1"=="" goto :run
echo.
echo  What do you want to do?
echo.
echo    1  Quick check - about 10 minutes. Only what changed lately
echo       (the new Panel, Recent, Alt+Tab, docking, the render check).
echo    2  Full test - about an hour. Everything.
echo.
choice /c 12 /n /m " Press 1 or 2: "
if errorlevel 2 goto :run
set "QUICK=-Mode qc"

:run
"%PSEXE%" -NoProfile -ExecutionPolicy Bypass -File "%WIZARD%" %QUICK% %*
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
